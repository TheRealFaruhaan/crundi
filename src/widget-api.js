/**
 * widget-api.js — the two doors into widgets: the page's HTTP routes and
 * Claude's MCP tools. Kept out of webapp.js, which only forwards to here.
 *
 * Owner only. Collaborators get neither the routes nor the tools: a widget can
 * be bound to commands and to files, and nothing about the collaborator
 * sandbox has been thought through for that yet.
 */

import { watch as fsWatch, readFileSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';
import * as store from './widget-store.js';
import { createWidgetSources } from './widget-sources.js';
import { buildDoc, FRAMES, tokenList } from './widget-doc.js';
import { renderWidget, describeRender, takeHarnessPage, renderSupported } from './widget-render.js';

const __dirname = dirname(fileURLToPath(import.meta.url));

/** Crundi tools a widget's button may be wired to. Deliberately short. */
export const ACTION_TOOLS = new Set([
  'kanban_add_task', 'kanban_update_task', 'kanban_move_task', 'kanban_add_todo', 'kanban_update_todo',
  'start_service', 'stop_service', 'restart_service', 'schedule_set_enabled', 'send_message_to_user',
]);

const shq = (s) => "'" + String(s == null ? '' : s).replace(/'/g, "'\\''") + "'";

/** Fill {{name}} in a template. `quote` wraps each value for a shell. */
export function fillTemplate(tpl, params, quote = false) {
  return String(tpl || '').replace(/\{\{\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*\}\}/g, (_m, k) => {
    const v = params && Object.prototype.hasOwnProperty.call(params, k) ? params[k] : '';
    const s = typeof v === 'object' ? JSON.stringify(v) : String(v == null ? '' : v);
    return quote ? shq(s) : s;
  });
}

function fillArgs(value, params) {
  if (typeof value === 'string') {
    // "{{x}}" alone keeps x's type (a number stays a number).
    const whole = /^\{\{\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*\}\}$/.exec(value);
    if (whole) return params ? params[whole[1]] : undefined;
    return fillTemplate(value, params);
  }
  if (Array.isArray(value)) return value.map((v) => fillArgs(v, params));
  if (value && typeof value === 'object') { const o = {}; for (const [k, v] of Object.entries(value)) o[k] = fillArgs(v, params); return o; }
  return value;
}

/**
 * @param {object} deps
 * @param {(event:string, data:object)=>void} deps.broadcast   owner pages only
 * @param {object} deps.claudeUi
 * @param {()=>string} deps.baseUrl       loopback URL of this server
 * @param {(tool:string, args:object)=>Promise<object>} deps.callTool
 * @param {Record<string,Function>} deps.crundi   readers for the crundi source
 */
export function createWidgetApi({ broadcast, claudeUi, baseUrl, callTool, crundi = {} }) {
  const tell = (alias, id, kind, extra = {}) => broadcast('widget', { project: store.projectKey(alias), id, kind, ...extra });

  // Data changes arrive in bursts (a log tail, a busy chat). One ping per
  // source every 250 ms is plenty: the page fetches the latest value anyway.
  const pending = new Map();
  const sources = createWidgetSources({
    claudeUi, crundi,
    onChange: (alias, id, source) => {
      const k = store.projectKey(alias) + '/' + id + '/' + source;
      if (pending.has(k)) return;
      pending.set(k, setTimeout(() => { pending.delete(k); tell(alias, id, 'data', { source }); }, 250));
    },
  });

  // ─── Hot reload: watch the source folder of anything on screen ───

  const docWatch = new Map(); // "<project>/<id>" -> { watcher, timer, seenAt }
  const wkey = (alias, id) => store.projectKey(alias) + '/' + id;

  /** Source changed on disk? Version it, restart its data, reload the frames. */
  function syncSource(alias, id) {
    const meta = store.get(alias, id);
    if (!meta) return false;
    const snap = store.snapshot(alias, id);
    if (!snap.ok || !snap.changed) return false;
    store.clearFaults(alias, id);
    store.touchDoc(alias, id);
    sources.refresh(alias, id);
    tell(alias, id, 'doc');
    return true;
  }

  function watchDoc(alias, id) {
    const k = wkey(alias, id);
    const have = docWatch.get(k);
    if (have) { have.seenAt = Date.now(); return; }
    const dir = store.sourceDir(alias, id);
    const entry = { watcher: null, timer: null, seenAt: Date.now() };
    const kick = () => { clearTimeout(entry.timer); entry.timer = setTimeout(() => { try { syncSource(alias, id); } catch { /* next change retries */ } }, 250); };
    try {
      entry.watcher = fsWatch(dir, { persistent: false, recursive: true }, kick);
      entry.watcher.on('error', () => {});
    } catch { /* no folder yet, or no recursive watch here: widget tools sync explicitly */ }
    docWatch.set(k, entry);
  }

  const reaper = setInterval(() => {
    const now = Date.now();
    for (const [k, e] of docWatch) {
      if (now - e.seenAt < 90_000) continue;
      clearTimeout(e.timer);
      try { e.watcher?.close(); } catch { /* gone */ }
      docWatch.delete(k);
    }
  }, 30_000);
  reaper.unref?.();

  // ─── Views ───

  function publicMeta(alias, meta) {
    const m = store.readManifest(alias, meta.id);
    const g = store.grantState(alias, meta.id, m.ok ? m.manifest : {});
    return {
      id: meta.id, project: meta.project, title: meta.title, icon: meta.icon || '',
      slot: store.effectiveSlot(meta), requestedSlot: meta.slot, chat: meta.chat || '', beside: meta.beside || '',
      state: meta.state, lifecycle: meta.lifecycle, rev: meta.rev || 0, version: meta.version || 0,
      global: meta.project === store.GLOBAL, home: meta.home || '',
      size: meta.size || null, chip: meta.chip || (m.ok && m.manifest.chip) || null,
      collapsed: !!(meta.user && meta.user.collapsed),
      chatLive: !!(meta.chat && claudeUi && claudeUi.has && claudeUi.has(meta.chat)),
      needsGrant: g.needed && !g.granted, faults: (meta.faults || []).length,
      createdAt: meta.createdAt, updatedAt: meta.updatedAt,
    };
  }

  /**
   * What a project's pages show: its own widgets, then the global ones. A
   * project widget hides a global one with the same id, so an id means one
   * thing on any given page.
   */
  function listFor(alias) {
    const own = store.list(alias).map((m) => publicMeta(alias, m));
    if (store.projectKey(alias) === store.GLOBAL) return own;
    const have = new Set(own.map((w) => w.id));
    return own.concat(store.list(store.GLOBAL).filter((m) => !have.has(m.id)).map((m) => publicMeta(store.GLOBAL, m)));
  }

  /** Move a widget between its project and global, and tell every page. */
  function changeScope(alias, id, toGlobal) {
    const from = store.resolveScope(alias, id);
    if (!from) return { ok: false, error: 'No such widget' };
    const to = toGlobal ? store.GLOBAL : (store.homeAlias(from, id) || store.projectKey(alias));
    sources.drop(from, id);
    const r = store.moveScope(from, id, to);
    if (!r.ok) return r;
    if (!r.unchanged) {
      const e = docWatch.get(wkey(from, id));
      if (e) { clearTimeout(e.timer); try { e.watcher?.close(); } catch { /* gone */ } docWatch.delete(wkey(from, id)); }
      // Both sides: pages on the old scope drop it, pages everywhere pick it up.
      tell(from, id, 'list');
      tell(to, id, 'list');
    }
    return { ok: true, widget: publicMeta(to, r.widget), global: store.projectKey(to) === store.GLOBAL };
  }

  // ─── Calls from inside a frame ───

  function runCommand(cmd, cwd) {
    return new Promise((resolve) => {
      let out = '', err = '', done = false;
      const finish = (r) => { if (!done) { done = true; resolve(r); } };
      let child;
      try { child = spawn(cmd, { cwd, shell: true, stdio: ['ignore', 'pipe', 'pipe'] }); }
      catch (e) { return finish({ ok: false, error: e.message }); }
      const killer = setTimeout(() => { try { child.kill('SIGKILL'); } catch { /* gone */ } finish({ ok: false, error: 'Timed out after 30s' }); }, 30_000);
      child.stdout.on('data', (d) => { if (out.length < 16_000) out += d; });
      child.stderr.on('data', (d) => { if (err.length < 4_000) err += d; });
      child.on('error', (e) => { clearTimeout(killer); finish({ ok: false, error: e.message }); });
      child.on('close', (code) => { clearTimeout(killer); finish({ ok: code === 0, code, stdout: out.slice(0, 16_000), stderr: err.slice(0, 4_000), ...(code === 0 ? {} : { error: (err || out).trim().slice(0, 300) || `Exit ${code}` }) }); });
    });
  }

  async function frameCall(alias, id, op, args) {
    const meta = store.get(alias, id);
    if (!meta) return { ok: false, error: 'No such widget' };
    const a = args && typeof args === 'object' ? args : {};
    if (op === 'store.set') return store.storeSet(alias, id, a.key, a.value);
    if (op === 'emit') return store.addEvent(alias, id, a.name, a.payload);
    if (op === 'toast' || op === 'openLink') return { ok: true }; // the page does these itself
    const m = store.readManifest(alias, id);
    if (!m.ok) return m;
    const manifest = m.manifest;
    const g = store.grantState(alias, id, manifest);
    const needGrant = () => (g.granted ? null : { ok: false, error: 'This widget is waiting for approval. Approve it from the widget\'s header first.', needsGrant: true });
    if (op === 'prompt') {
      if (!(Array.isArray(manifest.allow) && manifest.allow.includes('prompt'))) return { ok: false, error: 'This widget does not declare "allow": ["prompt"] in widget.json' };
      const no = needGrant(); if (no) return no;
      if (!meta.chat || !claudeUi) return { ok: false, error: 'The chat that owns this widget is not known' };
      return claudeUi.sendMessage(meta.chat, String(a.text || '').slice(0, 4000));
    }
    if (op === 'action') {
      const spec = manifest.actions && manifest.actions[a.name];
      if (!spec) return { ok: false, error: `No action "${a.name}" in widget.json` };
      const params = a.params && typeof a.params === 'object' ? a.params : {};
      if (spec.kind === 'event') { store.addEvent(alias, id, a.name, params); return { ok: true }; }
      const no = needGrant(); if (no) return no;
      if (spec.kind === 'command') return runCommand(fillTemplate(spec.run, params, true), store.dataRoot(alias, id) || process.env.HOME || '/');
      if (spec.kind === 'prompt') {
        if (!meta.chat || !claudeUi) return { ok: false, error: 'The chat that owns this widget is not known' };
        return claudeUi.sendMessage(meta.chat, fillTemplate(spec.text, params).slice(0, 4000));
      }
      if (spec.kind === 'tool') {
        if (!ACTION_TOOLS.has(spec.tool)) return { ok: false, error: `"${spec.tool}" cannot be wired to a widget. Allowed: ${[...ACTION_TOOLS].join(', ')}` };
        const out = await callTool(spec.tool, { alias: store.homeAlias(alias, id), ...fillArgs(spec.args || {}, params) });
        if (out && out.ok) sources.refresh(alias, id);
        return out;
      }
      return { ok: false, error: `Unknown action kind "${spec.kind}". Use command, tool, prompt or event.` };
    }
    return { ok: false, error: `Unknown call "${op}"` };
  }

  // ─── HTTP ───

  /** The harness page, fetched by the headless browser with a one-off ticket. */
  function handleHarness(req, res, url) {
    const html = takeHarnessPage(url.searchParams.get('ticket'));
    if (!html) { res.writeHead(404, { 'Content-Type': 'text/plain' }); res.end('Expired'); return; }
    res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8', 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff' });
    res.end(html);
  }

  /**
   * @returns {Promise<boolean>} true when the request was a widget route
   */
  async function handleHttp(req, res, url, path, { json, readBody, confined }) {
    if (!path.startsWith('/api/widgets')) return false;
    if (confined) { json(res, { ok: false, error: 'Not available' }, 403); return true; }
    const q = url.searchParams;
    let body = {};
    if (req.method === 'POST') { try { body = JSON.parse(await readBody(req)) || {}; } catch { body = {}; } }
    const asked = String((req.method === 'POST' ? body.project : q.get('project')) || '');
    const id = String((req.method === 'POST' ? body.id : q.get('id')) || '');
    // A page names the project it is showing. The widget may be a global one
    // shown there, so the id is looked up in the project first, then globally.
    const alias = (id && store.resolveScope(asked, id)) || asked;

    if (path === '/api/widgets' && req.method === 'GET') { json(res, { ok: true, widgets: listFor(asked), slots: store.SLOTS }); return true; }

    if (path === '/api/widgets/scope' && req.method === 'POST') { json(res, changeScope(asked, id, !!body.global)); return true; }

    if (path === '/api/widgets/doc' && req.method === 'GET') {
      const meta = store.get(alias, id);
      if (!meta) { json(res, { ok: false, error: 'No such widget' }, 404); return true; }
      watchDoc(alias, id);
      const doc = await buildDoc(alias, id);
      const g = store.grantState(alias, id, doc.manifest);
      json(res, {
        ok: true, html: doc.html, rev: meta.rev || 0, widget: publicMeta(alias, meta), problems: doc.problems,
        grant: { needed: g.needed, granted: g.granted, hash: g.hash, items: g.items },
        actions: Object.keys((doc.manifest && doc.manifest.actions) || {}),
      });
      return true;
    }

    if (path === '/api/widgets/data' && req.method === 'GET') {
      if (!store.get(alias, id)) { json(res, { ok: false, error: 'No such widget' }, 404); return true; }
      watchDoc(alias, id);
      let all = sources.values(alias, id);
      // The first look starts the sources, and a query or a command has not
      // answered yet. Hand back real values rather than a frame of nulls that
      // depends on a follow-up event to fill in.
      for (let i = 0; i < 15 && Object.values(all).some((v) => v.kind !== 'push' && !v.rev); i++) {
        await new Promise((r) => setTimeout(r, 100));
        all = sources.values(alias, id);
      }
      const only = q.get('source');
      json(res, { ok: true, data: only ? (all[only] ? { [only]: all[only] } : {}) : all, store: only ? undefined : store.storeAll(alias, id) });
      return true;
    }

    if (path === '/api/widgets/watch' && req.method === 'POST') {
      for (const wid of (Array.isArray(body.ids) ? body.ids : []).slice(0, 40)) {
        const scope = store.resolveScope(asked, String(wid));
        if (!scope) continue;
        sources.watch(scope, String(wid));
        watchDoc(scope, String(wid));
      }
      json(res, { ok: true }); return true;
    }

    if (path === '/api/widgets/call' && req.method === 'POST') { json(res, await frameCall(alias, id, String(body.op || ''), body.args)); return true; }

    if (path === '/api/widgets/user' && req.method === 'POST') {
      const r = store.setUser(alias, id, body.patch || {});
      if (r.ok) tell(alias, id, 'list');
      json(res, r.ok ? { ok: true, widget: publicMeta(alias, r.widget) } : r); return true;
    }

    if (path === '/api/widgets/grant' && req.method === 'POST') {
      const r = body.revoke ? store.revoke(alias, id) : store.grant(alias, id, String(body.hash || ''));
      if (r.ok) { sources.refresh(alias, id); tell(alias, id, 'doc'); }
      json(res, r); return true;
    }

    if (path === '/api/widgets/fault' && req.method === 'POST') {
      store.addFault(alias, id, { message: body.message, where: body.where });
      json(res, { ok: true }); return true;
    }

    if (path === '/api/widgets/delete' && req.method === 'POST') {
      sources.drop(alias, id);
      const r = store.remove(alias, id, { keepSource: !!body.keepSource });
      if (r.ok) tell(alias, id, 'list');
      json(res, r); return true;
    }

    if (path === '/api/widgets/versions' && req.method === 'GET') { json(res, { ok: true, versions: store.listVersions(alias, id) }); return true; }

    if (path === '/api/widgets/rollback' && req.method === 'POST') {
      const r = store.rollback(alias, id, body.version);
      if (r.ok) { sources.refresh(alias, id); tell(alias, id, 'doc'); }
      json(res, r); return true;
    }

    if (path === '/api/widgets/renders' && req.method === 'GET') {
      const file = join(rendersDir(alias, id), 'index.json');
      let idx = null; try { idx = JSON.parse(readFileSync(file, 'utf8')); } catch { /* none yet */ }
      json(res, { ok: true, renders: idx }); return true;
    }

    if (path === '/api/widgets/render-image' && req.method === 'GET') {
      const name = String(q.get('name') || '');
      if (!/^[a-z-]{1,24}--[a-zA-Z0-9_-]{1,40}$/.test(name) || !store.validId(id)) { json(res, { ok: false, error: 'Bad name' }, 400); return true; }
      const file = join(rendersDir(alias, id), name + '.png');
      if (!existsSync(file)) { json(res, { ok: false, error: 'Not found' }, 404); return true; }
      res.writeHead(200, { 'Content-Type': 'image/png', 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff' });
      res.end(readFileSync(file)); return true;
    }

    json(res, { ok: false, error: 'Not found' }, 404);
    return true;
  }

  function rendersDir(alias, id) { return join(store.stateDirOf(alias, id), 'renders'); }

  // ─── MCP tools ───

  let guideCache = null;
  async function guide() {
    if (!guideCache) {
      try { guideCache = readFileSync(join(__dirname, 'widget-runtime', 'GUIDE.md'), 'utf8'); }
      catch { guideCache = 'The widget guide is missing from this install.'; }
    }
    let tokens = '';
    try { tokens = (await tokenList()).filter((t) => !/^--(sidebar|topbar)/.test(t.name)).map((t) => `${t.name}: ${t.value}`).join('\n'); } catch { /* guide still useful */ }
    return guideCache.replace('{{TOKENS}}', tokens).replace('{{FRAMES}}', Object.entries(FRAMES).map(([k, f]) => `- \`${k}\` ${f.width}×${f.height} — ${f.label}`).join('\n'));
  }

  function statusFor(alias, id) {
    const meta = store.get(alias, id);
    if (!meta) return { ok: false, error: `No widget "${id}"` };
    const m = store.readManifest(alias, id);
    const g = store.grantState(alias, id, m.ok ? m.manifest : {});
    const vals = sources.values(alias, id);
    const src = {};
    for (const [name, v] of Object.entries(vals)) {
      let size = 0; try { size = JSON.stringify(v.value).length; } catch { /* unsized */ }
      src[name] = { kind: v.kind, rev: v.rev, ...(v.error ? { error: v.error } : { bytes: size }), ...(v.at ? { updated: new Date(v.at).toISOString() } : {}) };
    }
    const user = meta.user || {};
    const out = {
      ok: true, id, title: meta.title, state: meta.state, lifecycle: meta.lifecycle,
      scope: meta.project === store.GLOBAL ? 'global' : 'project', ...(meta.home ? { homeProject: meta.home } : {}),
      slot: store.effectiveSlot(meta), requestedSlot: meta.slot, version: meta.version || 0,
      sourceDir: store.sourceDir(alias, id), sources: src, fixtures: store.listFixtures(alias, id),
      faults: (meta.faults || []).map((f) => ({ message: f.message, where: f.where || undefined, at: new Date(f.at).toISOString() })),
    };
    if (!m.ok) out.manifestError = m.error;
    if (g.needed) out.approval = g.granted ? 'granted' : { status: 'waiting for the owner', items: g.items };
    const notes = [];
    if (user.closed) notes.push('The person closed this widget. Do not reopen it unless they ask.');
    if (user.slot && user.slot !== meta.slot) notes.push(`The person moved it from "${meta.slot}" to "${user.slot}". Leave it there.`);
    if (user.collapsed) notes.push('The person collapsed it.');
    if (notes.length) out.person = notes;
    return out;
  }

  /**
   * @returns {Promise<object|null>} a tool result, or null when not a widget tool
   */
  async function handleMcp(tool, a = {}) {
    if (!tool.startsWith('widget_')) return null;
    const project = String(a.alias || '');
    const id = String(a.id || '').toLowerCase();
    // An id is this project's widget if there is one, else a global one.
    // A new id is created in the project.
    const alias = (id && store.resolveScope(project, id)) || project;

    if (tool === 'widget_guide') return { ok: true, content: [{ type: 'text', text: await guide() }] };

    if (tool === 'widget_list') {
      return { ok: true, widgets: listFor(project).map((w) => ({ id: w.id, title: w.title, slot: w.slot, state: w.state, lifecycle: w.lifecycle, global: w.global || undefined, needsApproval: w.needsGrant || undefined, faults: w.faults || undefined })) };
    }

    if (tool === 'widget_open') {
      if (!store.validId(id)) return { ok: false, error: 'Give the widget an id: lowercase letters, digits and dashes, e.g. "task-progress"' };
      const existing = store.get(alias, id);
      const prevClosed = existing && existing.user && existing.user.closed;
      let manifest = a.manifest;
      if (typeof manifest === 'string') { try { manifest = JSON.parse(manifest); } catch (e) { return { ok: false, error: 'manifest is not valid JSON: ' + e.message }; } }
      if (a.html !== undefined || manifest || a.fixtures) {
        const w = store.writeSource(alias, id, { html: a.html, manifest, fixtures: a.fixtures });
        if (!w.ok) return w;
      }
      const mf = store.readManifest(alias, id);
      const fromManifest = mf.ok ? mf.manifest : {};
      const r = store.upsert(alias, id, {
        title: a.title !== undefined ? a.title : (existing ? undefined : fromManifest.title),
        icon: a.icon !== undefined ? a.icon : fromManifest.icon,
        slot: a.slot !== undefined ? a.slot : (existing ? undefined : (fromManifest.slot || 'cell')),
        chat: a.sessionId || (existing ? undefined : ''),
        beside: a.beside, lifecycle: a.lifecycle !== undefined ? a.lifecycle : (existing ? undefined : fromManifest.lifecycle),
        size: a.size || fromManifest.size, chip: a.chip !== undefined ? a.chip : undefined,
      });
      if (!r.ok) return r;
      if (a.data && typeof a.data === 'object') {
        for (const [name, value] of Object.entries(a.data)) {
          const p = store.push(alias, id, name, { value });
          if (!p.ok) return p;
        }
      }
      const changed = syncSource(alias, id);
      if (!changed) sources.refresh(alias, id);
      tell(alias, id, 'list');
      if (a.data) tell(alias, id, 'data', {});
      const st = statusFor(alias, id);
      const doc = await buildDoc(alias, id);
      const next = [];
      if (!store.readHtml(alias, id).ok) next.push(`Write ${join(store.sourceDir(alias, id), 'index.html')} (a body fragment: markup, <style>, <script>). It hot-reloads on save.`);
      else next.push('Edit the files in sourceDir; the widget reloads on save. Call widget_render to see it before you say it is done.');
      if (st.approval && st.approval !== 'granted') next.push('It asks for things the owner must approve; they see the request on the widget. Sources that need it stay empty until then.');
      if (prevClosed) next.push('The person had closed this widget; it is open again because you asked.');
      return { ...st, created: r.created, problems: doc.problems.length ? doc.problems : undefined, next };
    }

    if (!store.validId(id)) return { ok: false, error: 'Missing widget id' };
    if (!store.get(alias, id)) return { ok: false, error: `No widget "${id}" in this project. widget_list shows what exists.` };

    if (tool === 'widget_set_data') {
      const source = String(a.source || 'data');
      const change = {};
      if (Object.prototype.hasOwnProperty.call(a, 'value')) change.value = a.value;
      if (a.merge !== undefined) change.merge = a.merge;
      if (a.append !== undefined) { change.append = a.append; change.max = a.max; }
      if (!Object.keys(change).length) return { ok: false, error: 'Give one of: value (replace), merge (deep-merge an object; null deletes a key), append (add to an array)' };
      const r = store.push(alias, id, source, change);
      if (r.ok) tell(alias, id, 'data', { source });
      return r;
    }

    if (tool === 'widget_get') {
      syncSource(alias, id);
      const st = statusFor(alias, id);
      const events = store.drainEvents(alias, id);
      if (events.length) st.events = events.map((e) => ({ name: e.name, payload: e.payload, at: new Date(e.at).toISOString() }));
      st.versions = store.listVersions(alias, id).slice(0, 6).map((v) => ({ version: v.version, at: new Date(v.at).toISOString() }));
      return st;
    }

    if (tool === 'widget_close') {
      if (a.remove) {
        sources.drop(alias, id);
        const r = store.remove(alias, id, { keepSource: !!a.keepSource });
        if (r.ok) tell(alias, id, 'list');
        return r;
      }
      const r = store.setState(alias, id, 'closed');
      if (r.ok) { sources.drop(alias, id); tell(alias, id, 'list'); }
      return r.ok ? { ok: true, closed: id, note: 'Closed, not deleted: widget_open brings it back. Pass remove:true to delete it and its files.' } : r;
    }

    if (tool === 'widget_scope') {
      const r = changeScope(project, id, a.global !== false);
      if (!r.ok) return r;
      return { ok: true, id, scope: r.global ? 'global' : 'project', sourceDir: store.sourceDir(r.global ? store.GLOBAL : r.widget.project, id),
        note: r.global ? 'Now global: it shows in every project and is pinned. Its files moved to sourceDir; its data paths still resolve against its home project. Slot "chip" keeps it in the top bar everywhere.' : 'Back in its project only.' };
    }

    if (tool === 'widget_rollback') {
      const r = store.rollback(alias, id, a.version);
      if (r.ok) { sources.refresh(alias, id); tell(alias, id, 'doc'); }
      return r;
    }

    if (tool === 'widget_render') {
      syncSource(alias, id);
      if (!renderSupported()) return { ok: false, error: 'No headless browser on this server, so a widget cannot be rendered for checking.' };
      const live = sources.values(alias, id);
      // Give slow sources (a command, a query) a moment on their first run.
      if (Object.values(live).some((v) => v.kind !== 'push' && !v.rev)) await new Promise((r) => setTimeout(r, 900));
      let states = a.states;
      if (states === 'all' || a.allStates) states = ['live', ...store.listFixtures(alias, id)];
      const r = await renderWidget({ baseUrl: baseUrl(), alias, id, frames: a.frames, states, liveData: sources.values(alias, id) });
      if (!r.ok) return r;
      const d = describeRender(r);
      tell(alias, id, 'render');
      const content = [{ type: 'text', text: d.text }];
      for (const s of r.shots) {
        content.push({ type: 'text', text: `${s.frame}${s.state !== 'live' ? ' · ' + s.state : ''}:` });
        content.push({ type: 'image', data: s.png, mimeType: 'image/png' });
      }
      return { ok: true, content };
    }

    return { ok: false, error: `Unknown widget tool: ${tool}` };
  }

  function stop() { clearInterval(reaper); sources.stopAll(); for (const e of docWatch.values()) { try { e.watcher?.close(); } catch { /* gone */ } } docWatch.clear(); }

  return { handleHttp, handleHarness, handleMcp, stop, sources };
}
