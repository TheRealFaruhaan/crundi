/**
 * widget-sources.js — where a widget's data comes from.
 *
 * The widget itself runs with no network and no filesystem. Its manifest names
 * sources; this module does the reading on the server and hands the frame
 * plain values. Kinds:
 *
 *   push     a value Claude set with widget_set_data (kept in widget-store)
 *   session  what the chat that owns the widget is doing, read from the stream
 *            Crundi already parses, so a live dashboard costs no tokens
 *   file     json / jsonl / csv / lines / text, re-read when the file changes
 *   sqlite   a read-only query, re-run when the database file changes
 *   command  a shell command's output, on an interval          (needs a grant)
 *   http     a URL's body, on an interval                      (needs a grant)
 *   crundi   this project's kanban board, services or schedules
 *
 * Nothing runs for a widget nobody is looking at: the page says "I am watching"
 * every so often and a source that has not heard that for a minute is stopped.
 * `"background": true` in the manifest keeps it going regardless.
 */

import { readFileSync, statSync, openSync, readSync, closeSync, watch as fsWatch } from 'node:fs';
import { dirname, basename, extname } from 'node:path';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import * as store from './widget-store.js';

const VIEW_TTL_MS = 60_000;
const REAP_EVERY_MS = 15_000;
const MAX_VALUE = 1024 * 1024;
const MAX_FILE = 4 * 1024 * 1024;
const TAIL_BYTES = 256 * 1024;
const MAX_ROWS = 5000;

const clamp = (n, lo, hi, dflt) => { const v = Number(n); return Number.isFinite(v) ? Math.min(hi, Math.max(lo, v)) : dflt; };

// ─── Parsing ───

/** RFC 4180-ish: quoted fields, doubled quotes, CRLF. First row is the header. */
export function parseCsv(text, { limit = MAX_ROWS } = {}) {
  const rows = [];
  let row = [], field = '', quoted = false, i = 0;
  const s = String(text || '');
  const sep = s.slice(0, 2000).split('\t').length > s.slice(0, 2000).split(',').length ? '\t' : ',';
  while (i < s.length) {
    const c = s[i];
    if (quoted) {
      if (c === '"') { if (s[i + 1] === '"') { field += '"'; i += 2; continue; } quoted = false; i++; continue; }
      field += c; i++; continue;
    }
    if (c === '"' && field === '') { quoted = true; i++; continue; }
    if (c === sep) { row.push(field); field = ''; i++; continue; }
    if (c === '\n' || c === '\r') {
      if (c === '\r' && s[i + 1] === '\n') i++;
      row.push(field); field = '';
      if (row.length > 1 || row[0] !== '') rows.push(row);
      row = []; i++;
      if (rows.length > limit + 1) break;
      continue;
    }
    field += c; i++;
  }
  if (field !== '' || row.length) { row.push(field); rows.push(row); }
  if (!rows.length) return [];
  const head = rows[0].map((h, n) => String(h || '').trim() || 'col' + (n + 1));
  return rows.slice(1, limit + 1).map(r => {
    const o = {};
    head.forEach((h, n) => {
      const v = r[n] === undefined ? '' : r[n];
      // Numbers as numbers, so a chart does not have to guess.
      o[h] = v !== '' && /^-?\d+(\.\d+)?$/.test(v) && v.length < 16 ? Number(v) : v;
    });
    return o;
  });
}

function formatFor(spec, path) {
  const f = String(spec.format || 'auto').toLowerCase();
  if (f !== 'auto') return f;
  const ext = extname(path || '').toLowerCase();
  if (ext === '.json') return 'json';
  if (ext === '.jsonl' || ext === '.ndjson') return 'jsonl';
  if (ext === '.csv' || ext === '.tsv') return 'csv';
  if (ext === '.log') return 'lines';
  return 'text';
}

export function parseAs(text, format, spec = {}) {
  const tail = clamp(spec.tail, 1, MAX_ROWS, 200);
  switch (format) {
    case 'json': return JSON.parse(text);
    case 'jsonl': {
      const out = [];
      for (const line of String(text).split('\n')) {
        const l = line.trim(); if (!l) continue;
        try { out.push(JSON.parse(l)); } catch { /* a half-written last line */ }
      }
      return out.slice(-tail);
    }
    case 'csv': return parseCsv(text, { limit: clamp(spec.limit, 1, MAX_ROWS, 1000) });
    case 'lines': {
      const lines = String(text).split('\n');
      if (lines.length && lines[lines.length - 1] === '') lines.pop();
      return lines.slice(-tail);
    }
    default: return String(text);
  }
}

/** Read a file, or only its end when the format is line-shaped. */
function readForFormat(path, format) {
  const st = statSync(path);
  if (!st.isFile()) throw new Error('Not a file');
  const tailOnly = format === 'lines' || format === 'jsonl';
  if (!tailOnly) {
    if (st.size > MAX_FILE) throw new Error(`File is ${(st.size / 1048576).toFixed(1)} MB; the limit is 4 MB. Use format "lines" or "jsonl" to read its end, or a sqlite source.`);
    return readFileSync(path, 'utf8');
  }
  if (st.size <= TAIL_BYTES) return readFileSync(path, 'utf8');
  const fd = openSync(path, 'r');
  try {
    const buf = Buffer.alloc(TAIL_BYTES);
    readSync(fd, buf, 0, buf.length, st.size - buf.length);
    const s = buf.toString('utf8');
    return s.slice(s.indexOf('\n') + 1); // drop the partial first line
  } finally { closeSync(fd); }
}

// ─── Session summary ───

function shortPath(p) {
  const parts = String(p || '').split('/').filter(Boolean);
  return parts.length > 2 ? '…/' + parts.slice(-2).join('/') : String(p || '');
}

function toolLabel(name, input) {
  const i = input || {};
  const n = String(name || '');
  if (n === 'Bash') return String(i.description || i.command || '').slice(0, 120);
  if (n === 'Read' || n === 'Edit' || n === 'Write' || n === 'NotebookEdit') return shortPath(i.file_path || i.notebook_path);
  if (n === 'Grep' || n === 'Glob') return String(i.pattern || '').slice(0, 80);
  if (n === 'Agent' || n === 'Task') return String(i.description || '').slice(0, 80);
  if (n === 'WebFetch') return String(i.url || '').slice(0, 80);
  if (n === 'WebSearch') return String(i.query || '').slice(0, 80);
  if (n === 'Skill') return String(i.skill || '');
  const first = Object.values(i).find(v => typeof v === 'string' && v);
  return first ? String(first).slice(0, 80) : '';
}

function prettyTool(name) {
  const m = /^mcp__(.+?)__(.+)$/.exec(String(name || ''));
  return m ? m[2] : String(name || '');
}

/** A chat's activity, boiled down to what a dashboard wants to draw. */
export function summariseSession(snap) {
  if (!snap) return { status: 'ended', state: 'ended', todos: [], tools: [], files: [], agents: [], counts: { tools: 0, edits: 0, commands: 0, errors: 0 } };
  const msgs = snap.messages || [];
  // The current turn starts at the last thing the person said.
  let turnStart = 0;
  for (let i = msgs.length - 1; i >= 0; i--) if (msgs[i].kind === 'user') { turnStart = i; break; }
  const turn = msgs.slice(turnStart);
  const counts = { tools: 0, edits: 0, commands: 0, errors: 0 };
  const files = new Map();
  const tools = [];
  let todos = [];
  const tasks = new Map();
  let lastText = '', prompt = '';
  for (const e of turn) {
    if (e.kind === 'user' && !prompt) prompt = String(e.text || '').slice(0, 300);
    if (e.kind === 'assistant-text' && e.text) lastText = String(e.text);
    if (e.kind !== 'tool') continue;
    counts.tools++;
    if (e.isError) counts.errors++;
    const n = String(e.name || '');
    if (n === 'Bash') counts.commands++;
    if (n === 'Edit' || n === 'Write' || n === 'NotebookEdit') {
      counts.edits++;
      const p = String((e.input && (e.input.file_path || e.input.notebook_path)) || '');
      if (p) files.set(p, (files.get(p) || 0) + 1);
    }
    if (n === 'TodoWrite' && e.input && Array.isArray(e.input.todos)) {
      todos = e.input.todos.map(t => ({ text: String(t.content || t.subject || ''), active: String(t.activeForm || ''), status: String(t.status || 'pending') }));
    }
    if (n === 'TaskCreate' && e.input) {
      const id = (/#(\d+)/.exec(String(e.result || '')) || [])[1] || String(tasks.size + 1);
      tasks.set(id, { text: String(e.input.subject || e.input.description || ''), active: String(e.input.activeForm || ''), status: 'pending' });
    }
    if (n === 'TaskUpdate' && e.input && tasks.has(String(e.input.taskId))) {
      const t = tasks.get(String(e.input.taskId));
      if (e.input.status === 'deleted') tasks.delete(String(e.input.taskId));
      else { if (e.input.status) t.status = String(e.input.status); if (e.input.subject) t.text = String(e.input.subject); }
    }
    tools.push({ name: prettyTool(n), label: toolLabel(n, e.input), status: e.status === 'done' ? (e.isError ? 'error' : 'done') : 'running' });
  }
  if (!todos.length && tasks.size) todos = [...tasks.values()];
  const done = todos.filter(t => t.status === 'completed').length;
  const running = tools.filter(t => t.status === 'running');
  return {
    status: snap.status || 'running',
    state: snap.state || 'idle',                 // idle | working | waiting | needs-input
    working: snap.state === 'working',
    title: snap.title || 'Chat',
    model: snap.model || '',
    costUsd: Number(snap.totalCostUsd) || 0,
    prompt,
    now: running.length ? (running[running.length - 1].name + (running[running.length - 1].label ? ': ' + running[running.length - 1].label : '')) : '',
    lastText: lastText.length > 600 ? lastText.slice(0, 600) + '…' : lastText,
    todos,
    progress: todos.length ? { done, total: todos.length, pct: Math.round((done / todos.length) * 100) } : null,
    tools: tools.slice(-30),
    files: [...files.entries()].map(([path, edits]) => ({ path, short: shortPath(path), edits })).slice(-40),
    agents: (snap.agents || []).filter(a => !a.dismissed).slice(-12).map(a => ({
      description: String(a.description || a.subagentType || 'agent').slice(0, 100),
      status: String(a.status || ''), step: String(a.step || a.lastTool || '').slice(0, 100),
    })),
    counts,
    plan: snap.plan ? { status: snap.plan.status } : null,
  };
}

// ─── Engine ───

/**
 * @param {object} deps
 * @param {(alias:string, id:string, source:string, rev:number)=>void} deps.onChange
 * @param {object} [deps.claudeUi]     the chat manager (history/on/off)
 * @param {Record<string,(alias:string)=>any>} [deps.crundi]  kanban/services/schedules readers
 */
export function createWidgetSources({ onChange = () => {}, claudeUi = null, crundi = {} } = {}) {
  const live = new Map();   // "<project>/<id>" -> runtime
  const keyOf = (alias, id) => store.projectKey(alias) + '/' + id;

  function emit(rt, name) { try { onChange(rt.alias, rt.id, name, rt.sources.get(name)?.rev || 0); } catch { /* a dead listener is not our problem */ } }

  /** Record a new value (or error) for a source; tell listeners only on change. */
  function settle(rt, name, value, error) {
    const s = rt.sources.get(name);
    if (!s) return;
    let body = '';
    if (!error) {
      try { body = JSON.stringify(value === undefined ? null : value); } catch (err) { error = 'Value is not JSON-serialisable: ' + err.message; }
      if (!error && body.length > MAX_VALUE) error = `This source produced ${(body.length / 1048576).toFixed(1)} MB; the limit is 1 MB. Narrow it (tail, limit, a tighter query).`;
    }
    const sig = error ? 'E:' + error : createHash('sha1').update(body).digest('hex');
    if (sig === s.sig) return;
    s.sig = sig;
    s.value = error ? (s.value === undefined ? null : s.value) : (value === undefined ? null : value);
    s.error = error || '';
    s.rev = (s.rev || 0) + 1;
    s.at = Date.now();
    emit(rt, name);
  }

  // ── starters: each returns a dispose() ──

  function startFile(rt, name, spec) {
    const r = store.resolveDataPath(rt.alias, spec.path, rt.id);
    if (!r.ok) { settle(rt, name, null, r.error); return () => {}; }
    const format = formatFor(spec, r.path);
    let timer = null, lastStat = '';
    const read = () => {
      try { settle(rt, name, parseAs(readForFormat(r.path, format), format, spec)); }
      catch (err) { settle(rt, name, null, err.code === 'ENOENT' ? `No such file: ${spec.path}` : err.message); }
    };
    const kick = () => { clearTimeout(timer); timer = setTimeout(read, 120); };
    let watcher = null;
    try {
      // Watch the folder, not the file: editors and atomic writers replace the
      // file, and a watch on the old inode goes quiet for good.
      watcher = fsWatch(dirname(r.path), { persistent: false }, (_ev, fname) => { if (!fname || fname === basename(r.path)) kick(); });
      watcher.on('error', () => {});
    } catch { /* no watch here: the poll below still catches it */ }
    const poll = setInterval(() => {
      let sig = 'gone';
      try { const st = statSync(r.path); sig = st.mtimeMs + ':' + st.size; } catch { /* gone */ }
      if (sig !== lastStat) { lastStat = sig; kick(); }
    }, clamp(spec.every, 1, 3600, 4) * 1000);
    poll.unref?.();
    read();
    return () => { clearTimeout(timer); clearInterval(poll); try { watcher?.close(); } catch { /* gone */ } };
  }

  function startSqlite(rt, name, spec) {
    const r = store.resolveDataPath(rt.alias, spec.path, rt.id);
    if (!r.ok) { settle(rt, name, null, r.error); return () => {}; }
    const queries = spec.queries && typeof spec.queries === 'object' ? spec.queries : null;
    if (!queries && !spec.query) { settle(rt, name, null, 'A sqlite source needs "query" (or "queries": { name: sql })'); return () => {}; }
    const limit = clamp(spec.limit, 1, MAX_ROWS, 500);
    let lastSig = '', busy = false, stopped = false;
    const run = async (force) => {
      if (busy || stopped) return;
      let sig = '';
      try {
        const st = statSync(r.path);
        sig = st.mtimeMs + ':' + st.size;
        try { const w = statSync(r.path + '-wal'); sig += ':' + w.mtimeMs + ':' + w.size; } catch { /* no WAL */ }
      } catch { settle(rt, name, null, `No such database: ${spec.path}`); return; }
      if (!force && !spec.always && sig === lastSig) return;
      lastSig = sig;
      busy = true;
      let db = null;
      try {
        const { DatabaseSync } = await import('node:sqlite');
        db = new DatabaseSync(r.path, { readOnly: true });
        const one = (sql) => {
          const params = Array.isArray(spec.params) ? spec.params : [];
          const rows = db.prepare(String(sql)).all(...params);
          return rows.length > limit ? rows.slice(0, limit) : rows;
        };
        if (queries) {
          const out = {};
          for (const [k, sql] of Object.entries(queries).slice(0, 12)) out[k] = one(sql);
          settle(rt, name, out);
        } else settle(rt, name, one(spec.query));
      } catch (err) {
        settle(rt, name, null, err.code === 'ERR_UNKNOWN_BUILTIN_MODULE'
          ? 'sqlite sources need Node 22.5 or newer on the Crundi server' : 'sqlite: ' + err.message);
      } finally { try { db?.close(); } catch { /* closed */ } busy = false; }
    };
    const poll = setInterval(() => run(false), clamp(spec.every, 1, 3600, 3) * 1000);
    poll.unref?.();
    run(true);
    return () => { stopped = true; clearInterval(poll); };
  }

  function startCommand(rt, name, spec) {
    const cmd = Array.isArray(spec.run) ? null : String(spec.run || '');
    const argv = Array.isArray(spec.run) ? spec.run.map(String) : null;
    if (!cmd && !(argv && argv.length)) { settle(rt, name, null, 'A command source needs "run"'); return () => {}; }
    const timeoutMs = clamp(spec.timeout, 1, 60, 10) * 1000;
    const format = String(spec.format || 'text').toLowerCase();
    const cwd = store.commandRoot(rt.alias, rt.id) || process.env.HOME || '/';
    let child = null, stopped = false;
    const run = () => {
      if (child || stopped) return;
      let out = '', err = '', over = false;
      try {
        child = argv ? spawn(argv[0], argv.slice(1), { cwd, stdio: ['ignore', 'pipe', 'pipe'] })
          : spawn(cmd, { cwd, shell: true, stdio: ['ignore', 'pipe', 'pipe'] });
      } catch (e) { settle(rt, name, null, e.message); return; }
      const c = child;
      const killer = setTimeout(() => { over = true; try { c.kill('SIGKILL'); } catch { /* gone */ } }, timeoutMs);
      c.stdout.on('data', d => { if (out.length < MAX_VALUE) out += d; });
      c.stderr.on('data', d => { if (err.length < 4000) err += d; });
      c.on('error', (e) => { clearTimeout(killer); child = null; settle(rt, name, null, e.message); });
      c.on('close', (code) => {
        clearTimeout(killer); child = null;
        if (stopped) return;
        if (over) return settle(rt, name, null, `Timed out after ${timeoutMs / 1000}s`);
        if (code !== 0 && !spec.allowFail) return settle(rt, name, null, `Exit ${code}: ${(err || out).trim().slice(0, 400)}`);
        try { settle(rt, name, parseAs(out, format === 'auto' ? 'text' : format, spec)); }
        catch (e) { settle(rt, name, null, `Could not parse output as ${format}: ${e.message}`); }
      });
    };
    const poll = setInterval(run, clamp(spec.every, 2, 3600, 10) * 1000);
    poll.unref?.();
    run();
    return () => { stopped = true; clearInterval(poll); try { child?.kill('SIGKILL'); } catch { /* gone */ } };
  }

  function startHttp(rt, name, spec) {
    let url;
    try { url = new URL(String(spec.url || '')); } catch { settle(rt, name, null, 'An http source needs a valid "url"'); return () => {}; }
    if (url.protocol !== 'http:' && url.protocol !== 'https:') { settle(rt, name, null, 'Only http and https URLs'); return () => {}; }
    let busy = false, stopped = false;
    const run = async () => {
      if (busy || stopped) return;
      busy = true;
      try {
        const headers = spec.headers && typeof spec.headers === 'object' ? spec.headers : {};
        const res = await fetch(url, { headers, signal: AbortSignal.timeout(clamp(spec.timeout, 1, 60, 10) * 1000), redirect: 'follow' });
        const text = (await res.text()).slice(0, MAX_VALUE + 1);
        if (!res.ok && !spec.allowFail) throw new Error(`HTTP ${res.status}`);
        let format = String(spec.format || 'auto').toLowerCase();
        if (format === 'auto') format = /json/i.test(res.headers.get('content-type') || '') ? 'json' : 'text';
        if (!stopped) settle(rt, name, parseAs(text, format, spec));
      } catch (err) { if (!stopped) settle(rt, name, null, err.name === 'TimeoutError' ? 'Timed out' : err.message); }
      finally { busy = false; }
    };
    const poll = setInterval(run, clamp(spec.every, 2, 3600, 15) * 1000);
    poll.unref?.();
    run();
    return () => { stopped = true; clearInterval(poll); };
  }

  function startSession(rt, name, spec) {
    const meta = store.get(rt.alias, rt.id) || {};
    const chat = String(spec.chat || meta.chat || '');
    if (!claudeUi || !chat) { settle(rt, name, summariseSession(null)); return () => {}; }
    let timer = null, stopped = false;
    const compute = () => { if (!stopped) settle(rt, name, summariseSession(claudeUi.history(chat))); };
    const handler = () => { if (!timer) timer = setTimeout(() => { timer = null; compute(); }, 350); };
    try { claudeUi.on(chat, handler); } catch { /* chat gone */ }
    // A chat that closes stops emitting, so nothing would ever say "ended".
    const poll = setInterval(compute, 5000);
    poll.unref?.();
    compute();
    return () => { stopped = true; clearTimeout(timer); clearInterval(poll); try { claudeUi.off(chat, handler); } catch { /* gone */ } };
  }

  function startCrundi(rt, name, spec) {
    const what = String(spec.what || '');
    const reader = crundi[what];
    if (!reader) { settle(rt, name, null, `A crundi source needs "what": one of ${Object.keys(crundi).join(', ')}`); return () => {}; }
    // A global widget still reads ITS project's board, wherever it is shown.
    let busy = false, stopped = false;
    const run = async () => {
      if (busy || stopped) return;
      busy = true;
      try { const v = await reader(store.homeAlias(rt.alias, rt.id), spec); if (!stopped) settle(rt, name, v); }
      catch (err) { if (!stopped) settle(rt, name, null, err.message); }
      finally { busy = false; }
    };
    // Machine stats move every couple of seconds; the rest change rarely.
    const poll = setInterval(run, clamp(spec.every, 1, 600, what === 'stats' ? 2 : 3) * 1000);
    poll.unref?.();
    run();
    return () => { stopped = true; clearInterval(poll); };
  }

  const STARTERS = { file: startFile, sqlite: startSqlite, command: startCommand, http: startHttp, session: startSession, crundi: startCrundi };
  const NEEDS_GRANT = new Set(['command', 'http']);

  function stop(rt) {
    for (const s of rt.sources.values()) { try { s.dispose?.(); } catch { /* best effort */ } }
    live.delete(keyOf(rt.alias, rt.id));
  }

  /** Bring a widget's sources up to date with its manifest, starting them. */
  function ensure(alias, id) {
    const k = keyOf(alias, id);
    const m = store.readManifest(alias, id);
    const manifest = m.ok ? m.manifest : {};
    const specs = manifest.sources && typeof manifest.sources === 'object' ? manifest.sources : {};
    const g = store.grantState(alias, id, manifest);
    const sig = JSON.stringify(specs) + '|' + (g.granted ? g.hash : 'ungranted') + '|' + ((store.get(alias, id) || {}).chat || '');
    let rt = live.get(k);
    if (rt && rt.sig === sig) return rt;
    // Keep the last values across a restart so the frame does not blink empty.
    const prev = rt ? rt.sources : new Map();
    if (rt) stop(rt);
    rt = { alias, id, sig, sources: new Map(), viewedAt: Date.now(), background: !!manifest.background, manifestError: m.ok ? '' : m.error };
    live.set(k, rt);
    const privilegedOutside = new Set(g.items.filter(i => /outside the project/.test(i.what)).map(i => i.name));
    for (const [name, spec] of Object.entries(specs).slice(0, 24)) {
      if (!store.validName(name) || !spec || typeof spec !== 'object') continue;
      const kind = String(spec.kind || 'push');
      const old = prev.get(name);
      const entry = { kind, rev: old ? old.rev : 0, value: old ? old.value : null, error: '', sig: '', at: old ? old.at : 0, dispose: null };
      rt.sources.set(name, entry);
      if (kind === 'push') continue;
      if (!STARTERS[kind]) { settle(rt, name, null, `Unknown source kind "${kind}". Use one of: ${store.SOURCE_KINDS.join(', ')}`); continue; }
      if (!g.granted && (NEEDS_GRANT.has(kind) || privilegedOutside.has(name))) { settle(rt, name, null, 'needs-approval'); continue; }
      try { entry.dispose = STARTERS[kind](rt, name, spec); }
      catch (err) { settle(rt, name, null, err.message); }
    }
    return rt;
  }

  /** The page is looking at this widget: keep (or start) its sources. */
  function watch(alias, id) {
    const rt = ensure(alias, id);
    rt.viewedAt = Date.now();
    return rt;
  }

  /** Current values for every source, pushed ones included. */
  function values(alias, id, { start = true } = {}) {
    const rt = start ? watch(alias, id) : live.get(keyOf(alias, id));
    const pushed = store.getPushed(alias, id);
    const out = {};
    if (rt) {
      for (const [name, s] of rt.sources) {
        if (s.kind === 'push') continue;
        out[name] = { value: s.value, rev: s.rev, at: s.at, kind: s.kind, ...(s.error ? { error: s.error } : {}) };
      }
    }
    // Pushed values are served whether or not the manifest declares them:
    // Claude can push first and write the manifest second.
    for (const [name, p] of Object.entries(pushed)) {
      if (out[name]) continue;
      out[name] = { value: p.value, rev: p.rev, at: p.at, kind: 'push' };
    }
    return out;
  }

  function one(alias, id, name) { return values(alias, id)[name] || null; }

  /** Source or grant changed: restart on the next look (or now, if watched). */
  function refresh(alias, id) {
    const rt = live.get(keyOf(alias, id));
    if (rt) { rt.sig = ''; ensure(alias, id); }
  }

  function drop(alias, id) { const rt = live.get(keyOf(alias, id)); if (rt) stop(rt); }

  const reaper = setInterval(() => {
    const now = Date.now();
    for (const rt of [...live.values()]) {
      if (rt.background) continue;
      if (now - rt.viewedAt > VIEW_TTL_MS) stop(rt);
    }
  }, REAP_EVERY_MS);
  reaper.unref?.();

  function stopAll() { clearInterval(reaper); for (const rt of [...live.values()]) stop(rt); }

  return { watch, values, one, refresh, drop, stopAll, running: () => [...live.keys()] };
}
