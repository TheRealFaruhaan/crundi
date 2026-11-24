/**
 * workers-chats.js — whole chats on a worker PC (phase 4, specs/workers-protocol.md).
 *
 * claude-ui.js starts a chat through launch() here when it is created with
 * where: 'worker:<id>'. The PC runs its own claude (the owner's own sign-in
 * there) in its own copy of the project; this file turns that process into a
 * ChildProcess-like object (stdin, stdout, stderr, pid, kill, 'exit') carried
 * over the link, so the stream-json handling above it is the same as for a
 * chat here. It also:
 *   - keeps stdin while the PC is away and hands it over when it is back, and
 *     picks stdout up from the byte it stopped at (chat.attach);
 *   - runs the chat's Crundi tools: the worker program is the chat's MCP
 *     server and relays tools/list and tools/call here (mcp.list, mcp.call);
 *     they run exactly as /api/mcp/call runs them for that chat, as that chat;
 *   - brings the PC's work back (chat.pull) into a worktree of the server's
 *     project on a branch of its own, at each turn end and on demand; a
 *     factory run's chat is pulled by the run instead (pullChat).
 *
 * Created by workers.js, which owns the link and hands every chat.* and mcp.*
 * message here.
 */
import { mkdirSync, writeFileSync, readFileSync, renameSync, existsSync, unlinkSync, rmSync } from 'node:fs';
import { join, relative, resolve as resolvePath, sep, dirname } from 'node:path';
import { tmpdir } from 'node:os';
import { randomBytes } from 'node:crypto';
import { execFile } from 'node:child_process';
import { EventEmitter } from 'node:events';
import { PassThrough, Writable } from 'node:stream';
import { StringDecoder } from 'node:string_decoder';
import http from 'node:http';
import https from 'node:https';
import { git, httpsRemote } from './workers-jobs.js';
import { listedTools, prepareToolArgs } from './mcp-tools.js';
import { chatToken } from './chat-token.js';

const LOST_AFTER_MS = 30 * 60 * 1000;
const PULL_TIMEOUT_MS = 10 * 60 * 1000;
/** A pull whose PC dropped waits this long for it to come back, then asks again. */
const PULL_GRACE_MS = 2 * 60 * 1000;
/** At most this many tries of one pull (each try is cut short only by a drop). */
const PULL_TRIES = 4;
const KEEP_CHATS = 200;
const QUEUE_MAX = 16 * 1024 * 1024;

export const CLAUDE_MISSING = (pc) => `Install Claude Code on ${pc}: curl -fsSL https://claude.ai/install.sh | bash (Windows: irm https://claude.ai/install.ps1 | iex), then run claude once to sign in`;

const str = (v, n) => (typeof v === 'string' ? v.slice(0, n) : '');

/** A PC's name as a branch segment: "Big PC" -> "big-pc". */
export function slug(s) {
  return String(s || '').toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 30) || 'pc';
}

/** The branch a chat's work comes back on: pc/<worker-name-slug>/<chat-short>. */
export function pullBranch(workerName, chatId) {
  return `pc/${slug(workerName)}/${String(chatId).slice(0, 8)}`;
}

function run(cmd, args, cwd) {
  return new Promise((resolve) => {
    execFile(cmd, args, { cwd, maxBuffer: 16 * 1024 * 1024, timeout: 300_000, windowsHide: true }, (err, stdout, stderr) => {
      resolve({ ok: !err, out: String(stdout || ''), err: String(stderr || (err && err.message) || '') });
    });
  });
}

/**
 * A PC's tar is checked before anything is unpacked on this server: every
 * member must be a plain relative path with no ".." part, and a link may
 * only point inside (relative, no ".."), so a PC cannot write outside the
 * folder it is unpacked into. Returns '' when it is safe, else why not.
 */
export async function unsafeTar(file) {
  const l = await run('tar', ['-tvf', file], tmpdir());
  if (!l.ok) return `the PC's files are not a readable tar (${(l.err || '').slice(0, 200)})`;
  const names = await run('tar', ['-tf', file], tmpdir());
  if (!names.ok) return 'the PC\'s files are not a readable tar';
  const bad = (p) => !p || p.startsWith('/') || /^[A-Za-z]:/.test(p) || p.split(/[\\/]/).includes('..');
  for (const n of String(names.out || '').split(/\r?\n/).filter(Boolean)) if (bad(n)) return `a path that leaves the folder (${n.slice(0, 120)})`;
  for (const line of String(l.out || '').split(/\r?\n/)) {
    const t = line.charAt(0);
    if (t !== 'l' && t !== 'h') continue;
    const target = (line.split(t === 'l' ? ' -> ' : ' link to ')[1] || '').trim();
    if (bad(target)) return `a link that points outside the folder (${target.slice(0, 120)})`;
  }
  return '';
}

/** POST JSON and read the JSON answer (the loopback /api/mcp/call). */
function postJson(url, body, headers) {
  return new Promise((resolve) => {
    let u;
    try { u = new URL(url); } catch { resolve({ ok: false, error: 'No API address for tool calls.' }); return; }
    const data = JSON.stringify(body);
    const lib = u.protocol === 'https:' ? https : http;
    const req = lib.request(u, { method: 'POST', headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(data), ...headers } }, (res) => {
      const parts = [];
      res.on('data', (c) => parts.push(c));
      res.on('end', () => {
        try { resolve(JSON.parse(Buffer.concat(parts).toString('utf8'))); }
        catch { resolve({ ok: false, error: 'Invalid response' }); }
      });
    });
    req.on('error', (err) => resolve({ ok: false, error: `Could not reach Crundi: ${err.message}` }));
    req.end(data);
  });
}

export function createChats({
  dataDir, now = () => Date.now(), sendTo, bufferedOf = () => 0,
  worker,                       // (id) -> { id, name, online, caps:[], allowChats, info } | null
  api = () => ({ url: '', key: '' }),   // this server's own loopback API and its internal key
  serveSource = null,           // workers-jobs.js's serveSource: a chat's folder is prepared like a job's
  changed = () => {}, log = () => {},
  pullGraceMs = PULL_GRACE_MS, pullTries = PULL_TRIES,   // tests shorten these
}) {
  const DIR = join(dataDir, 'workers');
  const FILE = join(DIR, 'chats.json');
  const WORKTREES = join(DIR, 'worktrees');
  try { mkdirSync(DIR, { recursive: true }); } catch { /* reported on use */ }

  /** chat id -> record. Live parts (proc, queue, timers) are not saved. */
  const chats = new Map();
  try {
    for (const c of JSON.parse(readFileSync(FILE, 'utf8')).chats || []) {
      if (c && c.chat) chats.set(c.chat, { ...c, state: 'ended', offline: false });
    }
  } catch { /* first run */ }
  const live = new Map();       // chat id -> { proc, queue, queued, lostTimer, pulls }
  const pulls = new Map();      // req -> pending pull

  let saveTimer = null;
  function save() {
    if (saveTimer) return;
    saveTimer = setTimeout(() => {
      saveTimer = null;
      try {
        const all = [...chats.values()];
        if (all.length > KEEP_CHATS) for (const c of all.slice(0, all.length - KEEP_CHATS)) if (!live.has(c.chat)) chats.delete(c.chat);
        const out = [...chats.values()].map(c => ({ ...c, offline: undefined }));
        writeFileSync(`${FILE}.tmp`, JSON.stringify({ chats: out }, null, 1), { mode: 0o600 });
        renameSync(`${FILE}.tmp`, FILE);
      } catch (err) { log(`[workers] chats save failed: ${err.message}`); }
    }, 500);
    saveTimer.unref?.();
  }
  const iso = () => new Date(now()).toISOString();

  // ── Where the code comes from ──

  /** The chat's source, as for a job: git (https remote) when it can, else a copy. */
  async function sourceOf(cwd) {
    const top = (await git(['rev-parse', '--show-toplevel'], cwd)).out.trim();
    if (!top) return { source: 'copy', git: null, base: '', repoRoot: '', relCwd: '' };
    const relCwd = relative(resolvePath(top), resolvePath(cwd)).split(sep).join('/');
    const commit = (await git(['rev-parse', 'HEAD^{commit}'], cwd)).out.trim();
    const branch = (await git(['rev-parse', '--abbrev-ref', 'HEAD'], cwd)).out.trim();
    // The URL as configured (get-url would apply this server's own insteadOf rewrites).
    const url = httpsRemote((await git(['config', '--get', 'remote.origin.url'], cwd)).out.trim() || (await git(['remote', 'get-url', 'origin'], cwd)).out.trim());
    const base = /^[0-9a-f]{40}$/.test(commit) ? commit : '';
    if (base && url && /^https?:\/\//.test(url)) {
      return { source: 'git', git: { url, branch: branch && branch !== 'HEAD' ? branch : '', commit: base }, base, repoRoot: top, relCwd };
    }
    // A repository without a remote the PC can clone: a copy, with the
    // server's commit kept as the base its work comes back on.
    return { source: 'copy', git: null, base, repoRoot: top, relCwd };
  }

  // ── The launcher (claude-ui.js's launchRemote) ──

  /** Why a PC cannot take a chat now, or ''. */
  function refusal(w) {
    if (!w) return 'That PC is not joined to this server (Workers tab).';
    if (!w.online) return `${w.name} is offline.`;
    if (!(w.caps || []).includes('chats')) return `${w.name} runs an older worker without chats. Update it on that PC.`;
    if (w.allowChats === false) return `${w.name} does not allow chats (its switch in the Workers tab).`;
    if (!(w.info && w.info.tools && w.info.tools.claude)) return CLAUDE_MISSING(w.name);
    return '';
  }

  /** PCs a new chat may run on now (online, chats allowed, claude there). */
  function offer(list) {
    return list.filter(w => !refusal(w)).map(w => ({ id: w.id, name: w.name, where: `worker:${w.id}` }));
  }

  /**
   * Start claude on a PC. Throws (with the reason) when that PC cannot take
   * it; else returns the process-like object at once, and the PC's
   * chat.started / chat.refused arrive on it later.
   */
  async function launch(where, { args = [], env = {}, cwd = '', chatId, project = '', resumeSessionId = '', factory = null, pty = null, command = '' } = {}) {
    const workerId = String(where || '').replace(/^worker:/, '');
    const w = worker(workerId);
    const why = refusal(w);
    if (why) throw new Error(why);
    if (!chatId) throw new Error('No chat id.');
    const src = await sourceOf(cwd);
    if (!existsSync(cwd)) throw new Error(`Folder does not exist: ${cwd}`);
    const prev = chats.get(chatId);
    const c = {
      chat: chatId, workerId: w.id, workerName: w.name, project: str(project, 120), cwd,
      source: src.source, git: src.git, base: prev && prev.base && prev.workerId === w.id ? prev.base : src.base,
      repoRoot: src.repoRoot, relCwd: src.relCwd, factory: !!factory, pty: !!pty, notice: '',
      state: 'starting', createdAt: (prev && prev.createdAt) || iso(), startedAt: null, endedAt: null,
      pid: null, outBytes: 0, exitCode: null, error: '', offline: false,
      branch: (prev && prev.branch) || '', worktree: (prev && prev.worktree) || '', lastPull: (prev && prev.lastPull) || null,
    };
    chats.set(chatId, c);
    const proc = makeProc(c);
    const size = pty ? { cols: clampCols(pty.cols), rows: clampRows(pty.rows) } : null;
    live.set(chatId, { proc, queue: [], queued: 0, lostTimer: null, killAfter: '', size, sentSize: size ? `${size.cols}x${size.rows}` : '' });
    const cleanEnv = {};
    for (const [k, v] of Object.entries(env || {})) if (/^[A-Za-z_][A-Za-z0-9_]*$/.test(k) && typeof v === 'string') cleanEnv[k] = v;
    const sent = sendTo(w.id, {
      type: 'chat.spawn', chat: chatId, project: c.project || 'project', args: args.map(String),
      source: c.source, ...(c.git ? { git: c.git } : {}), ...(c.relCwd ? { cwd: c.relCwd } : {}),
      env: cleanEnv, ...(resumeSessionId ? { resumeSessionId: String(resumeSessionId) } : {}),
      // Terminal mode: claude in a pseudo-terminal there, its bytes as stdout.
      ...(size ? { pty: size } : {}),
      // A plain shell there instead of claude (an Empty Shell pane on a PC).
      ...(size && command === 'shell' ? { command: 'shell' } : {}),
    });
    if (!sent) { live.delete(chatId); c.state = 'ended'; c.error = 'Could not reach the PC.'; save(); throw new Error(`Could not reach ${w.name}.`); }
    save(); changed();
    return proc;
  }

  /** The ChildProcess-like object claude-ui drives. */
  function makeProc(c) {
    const proc = new EventEmitter();
    proc.pid = undefined;
    proc.stdout = new PassThrough();
    proc.stderr = new PassThrough();
    proc.exitCode = null;
    proc.stdin = new Writable({
      write(chunk, _enc, cb) { input(c, Buffer.isBuffer(chunk) ? chunk : Buffer.from(String(chunk))); cb(); },
      // stdin closed: claude's stdin closes on the PC too (chat.in eof).
      final(cb) { input(c, null); cb(); },
    });
    proc.stdin.on('error', () => { /* never fatal */ });
    proc.kill = (sig = 'SIGTERM') => kill(c, sig);
    return proc;
  }

  const inMsg = (c, b) => (b === null ? { type: 'chat.in', chat: c.chat, eof: true } : { type: 'chat.in', chat: c.chat, b64: b.toString('base64') });
  /** Bytes for claude's stdin, or null for its end. */
  function input(c, b) {
    const l = live.get(c.chat);
    if (!l || c.state === 'ended') return;
    if (c.state === 'running' && !c.offline && !l.queue.length) {
      if (sendTo(c.workerId, inMsg(c, b))) return;
    }
    if (b === null) { l.queue.push(null); return; }
    // The PC is away (or still preparing): kept, in order, for when it is there.
    if (l.queued + b.length > QUEUE_MAX) { l.proc.stderr.write('[crundi] The PC is away and too much is waiting for it; this message was dropped.\n'); return; }
    l.queue.push(b); l.queued += b.length;
  }

  function flush(c) {
    const l = live.get(c.chat);
    if (!l || c.state !== 'running' || c.offline) return;
    while (l.queue.length) {
      const b = l.queue[0];
      if (!sendTo(c.workerId, inMsg(c, b))) return;
      l.queue.shift(); l.queued -= b ? b.length : 0;
    }
  }

  function kill(c, sig) {
    const l = live.get(c.chat);
    if (!l || c.state === 'ended') return false;
    const signal = sig === 'SIGINT' ? 'SIGINT' : sig === 'SIGKILL' ? 'SIGKILL' : 'SIGTERM';
    if (!c.offline && sendTo(c.workerId, { type: 'chat.kill', chat: c.chat, signal })) return true;
    // Away: it ends here now, and the PC is told when it is back.
    if (signal !== 'SIGINT') ended(c, null, null, 'Closed while the PC was away.');
    return true;
  }

  const clampCols = (n) => Math.max(10, Math.min(500, Number(n) | 0 || 120));
  const clampRows = (n) => Math.max(4, Math.min(300, Number(n) | 0 || 30));
  /** A terminal on a PC changed size: told now, or when the PC is there again. */
  function resize(c, cols, rows) {
    const l = live.get(c.chat);
    if (!l || !l.size || c.state === 'ended') return false;
    l.size = { cols: clampCols(cols), rows: clampRows(rows) };
    sendSize(c);
    return true;
  }
  function sendSize(c) {
    const l = live.get(c.chat);
    if (!l || !l.size || c.state !== 'running' || c.offline) return;
    const k = `${l.size.cols}x${l.size.rows}`;
    if (k === l.sentSize) return;
    if (sendTo(c.workerId, { type: 'chat.resize', chat: c.chat, cols: l.size.cols, rows: l.size.rows })) l.sentSize = k;
  }

  /** The chat ended (its exit, a refusal, or the PC staying away). */
  function ended(c, code, signal, why = '') {
    const l = live.get(c.chat);
    c.state = 'ended'; c.endedAt = iso(); c.exitCode = code;
    if (why) c.error = why;
    live.delete(c.chat);
    if (l) {
      if (l.lostTimer) clearTimeout(l.lostTimer);
      const p = l.proc;
      p.exitCode = code;
      if (why) {
        try { p.stderr.write(`[crundi] ${why}\n`); } catch { /* closed */ }
        // Shown in the chat (claude-ui puts a process error there).
        if (p.listenerCount('error')) setImmediate(() => p.emit('error', new Error(why)));
      }
      try { p.stdout.end(); p.stderr.end(); } catch { /* closed */ }
      // After the stream's last data is through.
      setImmediate(() => { p.emit('exit', code, signal || null); p.emit('close', code, signal || null); });
    }
    save(); changed();
  }

  function onOut(c, m) {
    const l = live.get(c.chat);
    if (!l || typeof m.b64 !== 'string') return;
    const data = Buffer.from(m.b64, 'base64');
    if (m.stream === 'stderr') { try { l.proc.stderr.write(data); } catch { /* closed */ } return; }
    const off = typeof m.off === 'number' ? m.off : c.outBytes;
    if (off > c.outBytes) {
      // A gap (something was missed): from what we have.
      sendTo(c.workerId, { type: 'chat.attach', chat: c.chat, from: c.outBytes });
      return;
    }
    const skip = c.outBytes - off;
    if (skip >= data.length) return;
    const piece = skip ? data.subarray(skip) : data;
    c.outBytes += piece.length;
    try { l.proc.stdout.write(piece); } catch { /* closed */ }
  }

  // ── Sending the code (when the PC asks, exactly as for a job) ──
  // chat.need -> chat.bundle, chat.manifest -> chat.files, all WITHOUT req
  // (the pull answers come the other way, with req). A worker that asks in
  // job words (job = the chat id) is answered in job words. The serving is
  // workers-jobs.js's own (its bundle exclusions, its copies.json rule).
  function serve(c, kind, arg, style) {
    if (!serveSource) return;
    const t = style === 'job' ? { type: kind === 'bundle' ? 'job.bundle' : 'job.files', idKey: 'job' } : { type: kind === 'bundle' ? 'chat.bundle' : 'chat.files', idKey: 'chat' };
    Promise.resolve(serveSource(kind, { id: c.chat, workerId: c.workerId, project: c.project, git: c.git, root: c.cwd }, arg, t))
      .catch((err) => log(`[workers] chat ${c.chat} source: ${err.message}`));
  }

  // ── The chat's tools (the worker is its MCP server and relays here) ──

  function toolsFor(c) {
    return listedTools({ inChat: true, inFactory: !!c.factory })
      // Checked where the files are: on the PC, they are not here.
      .filter(t => t.name !== 'syntax_check');
  }

  async function callTool(c, tool, args) {
    const name = String(tool || '');
    if (!name) return { ok: false, error: 'No tool named.' };
    if (name === 'syntax_check') return { ok: false, error: 'syntax_check is not available in a chat on a PC; run the checker in the shell there.' };
    const a = prepareToolArgs(name, args && typeof args === 'object' && !Array.isArray(args) ? { ...args } : {}, {
      CRUNDI_PROJECT: c.project, CRUNDI_CHAT_ID: c.chat, CRUNDI_CHAT_TOKEN: chatToken(c.chat, c.project),
    });
    const { url, key } = api() || {};
    if (!url || !key) return { ok: false, error: 'The server is not ready for tool calls yet.' };
    return postJson(new URL('/api/mcp/call', url).href, { tool: name, args: a }, { 'X-Api-Key': key });
  }

  // ── Changes coming back ──

  function worktreeOf(c) {
    return join(WORKTREES, slug(c.project) || 'project', c.chat);
  }

  // A PC that drops and comes back within pullGraceMs does not fail a pull:
  // it is asked again (a new req, so pieces of the cut-off try are ignored).
  const backWaiters = new Map();   // worker id -> Set<(back: boolean) => void>
  /** Resolves true when the PC is (back) online, false after `ms`. */
  function waitBack(workerId, ms) {
    return new Promise((resolve) => {
      if (worker(workerId)?.online) { resolve(true); return; }
      let set = backWaiters.get(workerId);
      if (!set) backWaiters.set(workerId, (set = new Set()));
      const fn = (back) => {
        clearTimeout(t); set.delete(fn);
        if (!set.size && backWaiters.get(workerId) === set) backWaiters.delete(workerId);
        resolve(back);
      };
      const t = setTimeout(() => fn(false), Math.max(0, ms));
      t.unref?.();
      set.add(fn);
    });
  }
  function wakeBack(workerId, back) {
    for (const fn of [...(backWaiters.get(workerId) || [])]) fn(back);
  }

  const mins = (ms) => (ms >= 60_000 ? `${Math.round(ms / 60_000)} minute${Math.round(ms / 60_000) === 1 ? '' : 's'}` : `${Math.round(ms / 1000)} seconds`);

  /**
   * Ask the PC for the chat's work since `base` and wait for it, through
   * short drops: a PC that goes away mid-transfer and is back within
   * pullGraceMs is asked again (up to pullTries). `have`: commits this
   * server has, so the PC's bundle holds only what is missing here.
   * Resolves { ok, head, bundle: path|'', patch: Buffer, files: path|'', removed } (temp files: the caller removes).
   */
  async function requestPull(c, base, have, { waitOffline = true } = {}) {
    let tries = 0, last = '';
    for (;;) {
      if (!worker(c.workerId)?.online) {
        if (!waitOffline && !tries) return { ok: false, offline: true, error: `${c.workerName} is offline.` };
        if (!(await waitBack(c.workerId, pullGraceMs))) {
          return { ok: false, offline: true, transfer: true, tries, error: last
            ? `${c.workerName} disconnected while sending the work (${tries} ${tries === 1 ? 'try' : 'tries'}) and did not come back within ${mins(pullGraceMs)}.`
            : `${c.workerName} is offline and did not come back within ${mins(pullGraceMs)}.` };
        }
      }
      tries++;
      const r = await pullOnce(c, base, have);
      if (r.ok || !r.dropped) return { ...r, tries, transfer: !r.ok && !!r.transfer };
      last = r.error;
      // Still shown online (the link is closing): a moment before asking again.
      if (worker(c.workerId)?.online) await new Promise((res) => { const t = setTimeout(res, 500); t.unref?.(); });
      log(`[workers] pull ${c.chat} from ${c.workerName}: ${r.error} (try ${tries} of ${pullTries})`);
      if (tries >= pullTries) {
        return { ok: false, transfer: true, tries, error: `${c.workerName} disconnected while sending the work, ${tries} times in a row.` };
      }
    }
  }

  /** One try: chat.pull and its answer. A drop resolves { ok: false, dropped: true }. */
  function pullOnce(c, base, have) {
    return new Promise((resolve) => {
      const req = 'p' + randomBytes(6).toString('hex');
      const p = {
        req, chat: c.chat, workerId: c.workerId, resolve, head: '', removed: [],
        bundleFile: join(tmpdir(), `crundi-pull-${req}.bundle`), bundleSeq: 0, bundleDone: false, bundleBytes: 0,
        filesFile: join(tmpdir(), `crundi-pull-${req}.tar`), filesSeq: 0,
        patchFile: join(tmpdir(), `crundi-pull-${req}.patch`), patchSeq: 0,
        timer: null,
      };
      // Quiet for this long (no piece arriving): the PC is not sending. A big
      // transfer that keeps arriving is never cut off by it.
      p.touch = () => {
        clearTimeout(p.timer);
        p.timer = setTimeout(() => done(p, { ok: false, transfer: true, error: `${c.workerName} stopped sending the chat's work (nothing for ${mins(PULL_TIMEOUT_MS)}).` }), PULL_TIMEOUT_MS);
        p.timer.unref?.();
      };
      p.touch();
      pulls.set(req, p);
      // chunks: the PC may send its uncommitted work (chat.patch) in numbered
      // pieces like the bundle; an older worker ignores it and sends one.
      if (!sendTo(c.workerId, { type: 'chat.pull', chat: c.chat, req, base: base || c.base || '', have: have || [], chunks: true })) {
        done(p, { ok: false, dropped: true, error: `Could not reach ${c.workerName}.` });
      }
    });
  }
  function done(p, r) {
    if (!pulls.has(p.req)) return;
    pulls.delete(p.req);
    clearTimeout(p.timer);
    if (!r.ok) { for (const f of [p.bundleFile, p.filesFile]) { try { unlinkSync(f); } catch { /* none */ } } }
    try { unlinkSync(p.patchFile); } catch { /* none */ }
    p.resolve(r);
  }
  function onPullPiece(w, m) {
    const p = pulls.get(String(m.req || ''));
    if (!p || p.workerId !== w.id || p.chat !== m.chat) return;
    p.touch();
    const b = typeof m.b64 === 'string' ? Buffer.from(m.b64, 'base64') : Buffer.alloc(0);
    if (m.type === 'chat.bundle') {
      if (m.seq !== p.bundleSeq) return;
      p.bundleSeq++;
      try { writeFileSync(p.bundleFile, b, { flag: p.bundleSeq === 1 ? 'w' : 'a' }); } catch (err) { done(p, { ok: false, error: `Could not keep the bundle: ${err.message}` }); return; }
      p.bundleBytes += b.length;
      if (m.last) { p.bundleDone = true; p.head = str(m.head, 64); }
      return;
    }
    // A pull that failed on the PC says so on its patch (git) or its last files chunk (copy).
    if ((m.type === 'chat.patch' || (m.type === 'chat.files' && m.last)) && m.error) { done(p, { ok: false, error: `${worker(p.workerId)?.name || 'The PC'}: ${str(m.error, 400)}` }); return; }
    if (m.type === 'chat.patch') {
      let patch = b;
      if (typeof m.seq === 'number') {
        // In pieces (a newer worker, asked with chunks): kept until the last.
        if (m.seq !== p.patchSeq) return;
        p.patchSeq++;
        try { writeFileSync(p.patchFile, b, { flag: p.patchSeq === 1 ? 'w' : 'a' }); } catch (err) { done(p, { ok: false, error: `Could not keep the patch: ${err.message}` }); return; }
        if (!m.last) return;
        try { patch = readFileSync(p.patchFile); } catch (err) { done(p, { ok: false, error: `Could not read the patch: ${err.message}` }); return; }
      }
      done(p, { ok: true, kind: 'git', head: p.head, bundle: p.bundleBytes ? p.bundleFile : '', patch });
      return;
    }
    if (m.type === 'chat.files') {
      if (m.seq !== p.filesSeq) return;
      p.filesSeq++;
      try { writeFileSync(p.filesFile, b, { flag: p.filesSeq === 1 ? 'w' : 'a' }); } catch (err) { done(p, { ok: false, error: `Could not keep the files: ${err.message}` }); return; }
      if (m.last) done(p, { ok: true, kind: 'files', files: p.filesFile, removed: Array.isArray(m.removed) ? m.removed.map(String).slice(0, 100_000) : [] });
      return;
    }
    if (m.type === 'chat.pullError' || m.error) done(p, { ok: false, error: str(m.error, 400) || 'The PC could not send the chat\'s work.' });
  }

  /** Commits this server has that the PC's bundle need not carry (the PC uses those it knows). */
  async function havesFor(c, opts) {
    const out = [];
    const add = (sha) => { const x = String(sha || '').trim(); if (/^[0-9a-f]{40}$/.test(x) && !out.includes(x)) out.push(x); };
    add(opts.base); add(c.base);
    if (!c.repoRoot || !existsSync(c.repoRoot)) return out;
    const wt = opts.intoWorktree || worktreeOf(c);
    if (existsSync(wt)) add((await git(['rev-parse', '--verify', '--quiet', 'HEAD^{commit}'], wt)).out);
    add((await git(['rev-parse', '--verify', '--quiet', `refs/crundi-pc/${c.chat}^{commit}`], c.repoRoot)).out);
    const known = [];
    for (const sha of out.slice(0, 16)) if ((await git(['cat-file', '-e', `${sha}^{commit}`], c.repoRoot)).ok) known.push(sha);
    return known;
  }

  /** Where the chat's work is on the PC (worker/src/chats.rs prepare_chat), for an error that says so. */
  function whereOnPc(c) {
    const w = worker(c.workerId);
    const wd = String(w?.info?.workDir || '').replace(/[\\/]+$/, '');
    const s = w?.info?.os === 'windows' ? '\\' : '/';
    const folder = c.source === 'copy' ? `${c.project || 'project'}@copy-${c.chat}` : `${c.project || 'project'}@${c.chat}`;
    const dir = wd ? `${wd}${s}${folder}` : `the folder ${folder} in its work folder`;
    const pc = c.workerName || 'the PC';
    if (c.source !== 'git') return `Nothing is lost: the work is still on ${pc} in ${dir}. Try again once ${pc} stays connected.`;
    return `Nothing is lost: the work is still on ${pc} in ${dir} (commits on branch pc/${c.chat}, plus its uncommitted changes). `
      + `Try again once ${pc} stays connected; if it keeps failing, commit and push it from that PC (git -C "${dir}" push origin HEAD:refs/heads/pc/${c.chat}) and fetch that branch here.`;
  }

  /** The ref in a bundle that is at `head` (or its only one). */
  async function bundleRef(file, head, cwd) {
    const r = await git(['bundle', 'list-heads', file], cwd);
    const lines = r.out.split(/\r?\n/).map(l => l.trim().split(/\s+/)).filter(x => x.length >= 2);
    const hit = lines.find(([sha]) => head && sha === head) || lines[0];
    return hit ? { sha: hit[0], ref: hit[1] } : null;
  }

  /** Remove `removed` (paths relative to root) inside root only. */
  function removeInside(root, removed) {
    const top = resolvePath(root) + sep;
    for (const rel of removed || []) {
      const abs = resolvePath(root, String(rel));
      if (!abs.startsWith(top)) continue;
      try { rmSync(abs, { force: true }); } catch { /* gone */ }
    }
  }

  async function applyPatch(wt, patch) {
    if (!patch || !patch.length) return { ok: true };
    const pf = join(tmpdir(), `crundi-pull-${randomBytes(4).toString('hex')}.patch`);
    writeFileSync(pf, patch);
    try {
      let r = await git(['apply', '--binary', '--whitespace=nowarn', pf], wt);
      if (!r.ok) r = await git(['apply', '--binary', '--3way', '--whitespace=nowarn', pf], wt);
      return r.ok ? { ok: true } : { ok: false, error: `The PC's uncommitted work did not apply: ${r.err.slice(0, 300)}` };
    } finally { try { unlinkSync(pf); } catch { /* gone */ } }
  }

  /** Our own worktree for a chat, on its branch from base (made the first time). */
  async function ensureWorktree(c, wt, branch, base) {
    if (existsSync(join(wt, '.git'))) return { ok: true };
    mkdirSync(dirname(wt), { recursive: true });
    const has = (await git(['rev-parse', '--verify', '--quiet', `refs/heads/${branch}`], c.repoRoot)).ok;
    const r = has ? await git(['worktree', 'add', '-f', wt, branch], c.repoRoot) : await git(['worktree', 'add', '-b', branch, wt, base], c.repoRoot);
    return r.ok ? { ok: true } : { ok: false, error: `Could not make the worktree: ${r.err.slice(0, 300)}` };
  }

  /**
   * Apply what the PC sent. Own worktree (default): it becomes exactly the
   * PC's state (branch reset to the PC's head, its uncommitted work applied
   * on top, uncommitted). Into a given worktree (a factory run's): the PC's
   * commits are fetched onto `branch` (fast-forward, else merged) and its
   * uncommitted work applied, uncommitted.
   */

  async function apply(c, got, { base, intoWorktree = '', branch = '' }) {
    const own = !intoWorktree;
    const wt = intoWorktree || worktreeOf(c);
    const br = branch || pullBranch(c.workerName, c.chat);
    const b = base || c.base;
    if (!c.repoRoot) {
      // A project without git: the files go beside it, never into it.
      if (got.kind !== 'files') return { ok: false, error: 'This project has no git repository to bring commits into.' };
      const dir = intoWorktree || join(c.cwd, '.crundi', 'pc-changes', c.chat);
      mkdirSync(dir, { recursive: true });
      const why = await unsafeTar(got.files);
      if (why) { try { unlinkSync(got.files); } catch { /* gone */ } return { ok: false, error: `Refused the PC's files: ${why}.` }; }
      const x = await run('tar', ['-xf', got.files, '--no-same-owner', '-C', dir], dir);
      try { unlinkSync(got.files); } catch { /* gone */ }
      if (!x.ok) return { ok: false, error: `Could not unpack the PC's files: ${x.err.slice(0, 300)}` };
      writeFileSync(join(dir, 'removed.txt'), (got.removed || []).join('\n') + ((got.removed || []).length ? '\n' : ''));
      return { ok: true, head: '', worktree: dir, branch: '' };
    }
    if (own) {
      const e = await ensureWorktree(c, wt, br, b);
      if (!e.ok) return e;
    } else if (!existsSync(wt)) return { ok: false, error: `No worktree at ${wt}` };
    try {
      if (got.kind === 'files') {
        if (own) { await git(['reset', '-q', '--hard', b], wt); await git(['clean', '-fdq'], wt); }
        const why = await unsafeTar(got.files);
        if (why) return { ok: false, error: `Refused the PC's files: ${why}.` };
        const x = await run('tar', ['-xf', got.files, '--no-same-owner', '-C', wt], wt);
        if (!x.ok) return { ok: false, error: `Could not unpack the PC's files: ${x.err.slice(0, 300)}` };
        removeInside(wt, got.removed);
        return { ok: true, head: (await git(['rev-parse', 'HEAD'], wt)).out.trim(), worktree: wt, branch: br };
      }
      let head = got.head || b;
      if (got.bundle) {
        const ref = await bundleRef(got.bundle, got.head, wt);
        if (!ref) return { ok: false, error: 'The PC\'s bundle has no branch in it.' };
        const f = await git(['fetch', '-q', got.bundle, `${ref.ref}:refs/crundi-pc/${c.chat}`, '--force'], wt);
        if (!f.ok) return { ok: false, error: `Could not fetch the PC's commits: ${f.err.slice(0, 300)}` };
        head = got.head || ref.sha;
      }
      if (!(await git(['cat-file', '-e', `${head}^{commit}`], wt)).ok) return { ok: false, error: `The PC's head ${head.slice(0, 12)} is not here.` };
      if (own) {
        // Ours alone: exactly the PC's state.
        const cur = (await git(['rev-parse', '--abbrev-ref', 'HEAD'], wt)).out.trim();
        if (cur !== br) { const co = await git(['checkout', '-q', '-B', br], wt); if (!co.ok) return { ok: false, error: co.err.slice(0, 300) }; }
        await git(['reset', '-q', '--hard', head], wt);
        await git(['clean', '-fdq'], wt);
      } else {
        const cur = (await git(['rev-parse', '--abbrev-ref', 'HEAD'], wt)).out.trim();
        if (cur !== br) {
          const exists = (await git(['rev-parse', '--verify', '--quiet', `refs/heads/${br}`], wt)).ok;
          const co = exists ? await git(['checkout', '-q', br], wt) : await git(['checkout', '-q', '-b', br, b], wt);
          if (!co.ok) return { ok: false, error: `Could not switch to ${br}: ${co.err.slice(0, 300)}` };
        }
        if (head !== b || !(await git(['merge-base', '--is-ancestor', head, 'HEAD'], wt)).ok) {
          let m = await git(['merge', '-q', '--ff-only', head], wt);
          if (!m.ok) {
            m = await git(['-c', 'user.name=Crundi', '-c', 'user.email=crundi@localhost', 'merge', '-q', '--no-edit', head], wt);
            if (!m.ok) { await git(['merge', '--abort'], wt); return { ok: false, error: `The PC's commits do not merge onto ${br}: ${m.err.slice(0, 300)}` }; }
          }
        }
      }
      const pa = await applyPatch(wt, got.patch);
      if (!pa.ok) return pa;
      return { ok: true, head, worktree: wt, branch: br };
    } finally {
      for (const f of [got.bundle, got.files]) { if (f) { try { unlinkSync(f); } catch { /* gone */ } } }
    }
  }

  const pulling = new Map();   // chat -> Promise (one pull at a time per chat)
  /** Bring a chat's work back. opts: { base, intoWorktree, branch } (phase 5) or none (its own worktree). */
  function pull(chatId, opts = {}) {
    const c = chats.get(String(chatId || ''));
    if (!c) return Promise.resolve({ ok: false, error: 'That chat does not run on a PC.' });
    const prev = pulling.get(c.chat) || Promise.resolve();
    const p = prev.catch(() => {}).then(async () => {
      const have = await havesFor(c, opts).catch(() => []);
      const got = await requestPull(c, opts.base, have, { waitOffline: opts.waitOffline !== false });
      if (!got.ok) {
        // A transfer that never made it: say where the work is and what to do.
        const r = got.transfer ? { ...got, error: `${got.error} ${whereOnPc(c)}` } : got;
        if (!opts.intoWorktree) { c.lastPull = { at: iso(), ok: false, head: '', error: r.error || '' }; save(); changed(); }
        return r;
      }
      let r;
      try { r = await apply(c, got, opts); } catch (err) { r = { ok: false, error: err.message }; }
      if (!opts.intoWorktree) {
        c.lastPull = { at: iso(), ok: !!r.ok, head: r.head || '', error: r.error || '' };
        if (r.ok) { c.branch = r.branch || ''; c.worktree = r.worktree || ''; }
        save(); changed();
      }
      return r;
    });
    pulling.set(c.chat, p);
    p.finally(() => { if (pulling.get(c.chat) === p) pulling.delete(c.chat); });
    return p;
  }

  // ── Bringing a live chat's folder on the PC up to a new commit, in place ──
  // A chat keeps its folder (and its build caches) for its whole life; a
  // factory step that reuses it must first see what others committed since.
  // The caller brings the chat's own work back first (pullChat): the PC may
  // then reset its folder to the new commit (keeping a stash as a backup).
  const syncs = new Map();     // req -> { chat, workerId, resolve, timer }
  async function syncChat(chatId, { cwd = '' } = {}) {
    const c = chats.get(String(chatId || ''));
    if (!c) return { ok: false, error: 'That chat does not run on a PC.' };
    if (c.source !== 'git') return { ok: false, unsupported: true, error: 'Only a git chat can be brought up to a new commit.' };
    const w = worker(c.workerId);
    if (!w || !w.online || c.offline) return { ok: false, offline: true, error: `${c.workerName} is offline.` };
    if (!(w.caps || []).includes('chat-sync')) return { ok: false, unsupported: true, error: `${c.workerName} runs an older worker that cannot bring a chat's folder up to date. Update the PC app.` };
    const src = await sourceOf(cwd || c.cwd);
    if (src.source !== 'git' || !src.git) return { ok: false, unsupported: true, error: 'The server folder is not a git checkout with a remote.' };
    c.git = src.git;   // what chat.need serves while the PC catches up
    return new Promise((resolve) => {
      const req = 's' + randomBytes(6).toString('hex');
      const timer = setTimeout(() => { syncs.delete(req); resolve({ ok: false, error: `${c.workerName} did not bring the chat's folder up to date in time.` }); }, PULL_TIMEOUT_MS);
      timer.unref?.();
      syncs.set(req, { chat: c.chat, workerId: c.workerId, timer, resolve: (r) => {
        if (r.ok) { c.base = src.base; save(); changed(); }
        resolve(r);
      } });
      if (!sendTo(c.workerId, { type: 'chat.sync', chat: c.chat, req, git: src.git })) { clearTimeout(timer); syncs.delete(req); resolve({ ok: false, error: `Could not reach ${c.workerName}.` }); }
    });
  }
  function onSynced(w, m) {
    const x = syncs.get(String(m.req || ''));
    if (!x || x.workerId !== w.id || x.chat !== m.chat) return;
    syncs.delete(m.req); clearTimeout(x.timer);
    x.resolve(m.ok ? { ok: true, head: str(m.head, 64), ffwd: !!m.ffwd, stashed: !!m.stashed } : { ok: false, error: str(m.error, 600) || 'The PC could not bring the folder up to date.' });
  }

  /** claude-ui's onTurnEnd: a chat on a PC brings its work back (never a factory chat: the run pulls). */
  function onTurnEnd(chatId) {
    const c = chats.get(String(chatId || ''));
    if (!c || c.factory || !live.has(c.chat) || c.offline) return;
    pull(c.chat).then((r) => { if (!r.ok) log(`[workers] bring back ${c.chat}: ${r.error}`); });
  }

  // ── The link ──

  // A worker resends an unanswered mcp.list / mcp.call after a reconnect
  // with the same req: answered again from here, a tool never run twice.
  const mcpSeen = new Map();   // `${worker}:${req}` -> { at, answer: Promise<msg> }
  function mcpOnce(w, req, make) {
    const k = `${w.id}:${req}`;
    let e = mcpSeen.get(k);
    if (!e) {
      e = { at: now(), answer: Promise.resolve().then(make) };
      mcpSeen.set(k, e);
      if (mcpSeen.size > 1000) { const cut = now() - 3600e3; for (const [x, v] of mcpSeen) if (v.at < cut || mcpSeen.size > 1000) mcpSeen.delete(x); else break; }
    }
    e.answer.then((msg) => sendTo(w.id, msg));
  }
  function onMessage(w, m) {
    if (m.type === 'mcp.list' || m.type === 'mcp.call') {
      const c = chats.get(String(m.chat || ''));
      const req = str(m.req, 80);
      const mine = c && c.workerId === w.id && live.has(c.chat);
      if (m.type === 'mcp.list') { mcpOnce(w, req, () => ({ type: 'mcp.tools', req, tools: mine ? toolsFor(c) : [] })); return; }
      if (!mine) { sendTo(w.id, { type: 'mcp.result', req, result: { ok: false, error: 'That chat is not running on this PC.' } }); return; }
      mcpOnce(w, req, () => callTool(c, m.tool, m.args).then((result) => ({ type: 'mcp.result', req, result }), (err) => ({ type: 'mcp.result', req, result: { ok: false, error: err.message } })));
      return;
    }
    if (m.type === 'chat.bundle' || m.type === 'chat.patch' || m.type === 'chat.files' || m.type === 'chat.pullError') { if (m.req) onPullPiece(w, m); return; }
    if (m.type === 'chat.synced') { onSynced(w, m); return; }
    const c = chats.get(String(m.chat || m.job || ''));
    if (!c || c.workerId !== w.id) return;
    switch (m.type) {
      case 'chat.started': {
        if (c.state === 'ended') { sendTo(w.id, { type: 'chat.kill', chat: c.chat, signal: 'SIGTERM' }); return; }
        c.state = 'running'; c.startedAt = c.startedAt || iso();
        c.pid = typeof m.pid === 'number' ? m.pid : null;
        const l = live.get(c.chat); if (l) l.proc.pid = c.pid ?? undefined;
        flush(c); sendSize(c); save(); changed();
        return;
      }
      case 'chat.refused': ended(c, -1, null, str(m.error, 600) || 'The PC refused the chat.'); return;
      // Something the PC says about the chat that is not an error ("the PC did
      // not have that conversation; starting fresh"): shown, nothing else.
      case 'chat.notice': {
        const text = str(m.text, 600).trim();
        if (!text) return;
        c.notice = text;
        const l = live.get(c.chat);
        if (l) { try { l.proc.emit('notice', text); } catch { /* a listener */ } }
        save(); changed();
        return;
      }
      case 'chat.out': onOut(c, m); return;
      case 'chat.exit': ended(c, typeof m.code === 'number' ? m.code : null, str(m.signal, 20) || null, m.unknown ? `${c.workerName} no longer had this chat.` : m.error ? str(m.error, 600) : ''); return;
      case 'chat.need': case 'job.need': if (m.kind === 'bundle' && c.git) serve(c, 'bundle', m.have, m.type.startsWith('job') ? 'job' : 'chat'); return;
      case 'chat.manifest': case 'job.manifest': serve(c, 'copy', m.files && typeof m.files === 'object' ? m.files : {}, m.type.startsWith('job') ? 'job' : 'chat'); return;
      default: return;
    }
  }

  /** Is this a chat's message in job words (its folder being prepared)? */
  const isChatId = (id) => chats.has(String(id || ''));

  /** A PC (re)connected: carry its chats on from where the server stopped. */
  function onHello(w, list) {
    const there = new Map();
    for (const x of Array.isArray(list) ? list : []) if (x && typeof x.chat === 'string') there.set(x.chat, x);
    for (const [id, x] of there) {
      const c = chats.get(id);
      if (!c || c.workerId !== w.id || !live.has(id)) {
        // Not a chat this server is running (it was closed meanwhile, or the
        // server restarted): stop it rather than leave it running there.
        if (x.running !== false) sendTo(w.id, { type: 'chat.kill', chat: id, signal: 'SIGTERM' });
        continue;
      }
      const l = live.get(id);
      if (l.lostTimer) { clearTimeout(l.lostTimer); l.lostTimer = null; }
      c.offline = false;
      if (c.state === 'starting') { c.state = 'running'; c.startedAt = c.startedAt || iso(); }
      sendTo(w.id, { type: 'chat.attach', chat: id, from: c.outBytes });
      flush(c);
      l.sentSize = '';   // the PC may have missed a resize while it was away
      sendSize(c);
    }
    for (const c of chats.values()) {
      if (c.workerId !== w.id || !live.has(c.chat) || there.has(c.chat)) continue;
      if (c.offline) ended(c, null, null, `${c.workerName} no longer had this chat when it came back.`);
    }
    // Pulls cut off by its drop ask again now.
    wakeBack(w.id, true);
    save(); changed();
  }

  /** A PC's link dropped: its chats carry on there; ended if it stays away. */
  function onDrop(workerId) {
    for (const c of chats.values()) {
      if (c.workerId !== workerId || !live.has(c.chat)) continue;
      c.offline = true;
      const l = live.get(c.chat);
      if (l.lostTimer) clearTimeout(l.lostTimer);
      l.lostTimer = setTimeout(() => ended(c, null, null, `${c.workerName} was away for more than 30 minutes; the chat is shown as ended.`), LOST_AFTER_MS);
      l.lostTimer.unref?.();
    }
    // A pull in flight is asked again when the PC is back (requestPull waits for it).
    for (const p of [...pulls.values()]) if (p.workerId === workerId) done(p, { ok: false, dropped: true, error: 'The PC disconnected.' });
    for (const [req, x] of [...syncs]) if (x.workerId === workerId) { syncs.delete(req); clearTimeout(x.timer); x.resolve({ ok: false, offline: true, error: 'The PC disconnected.' }); }
    changed();
  }

  /** What the UI shows for a chat on a PC. */
  function status(where, chatId) {
    const workerId = String(where || '').replace(/^worker:/, '');
    const w = worker(workerId);
    const c = chatId ? chats.get(chatId) : null;
    return {
      name: (w && w.name) || (c && c.workerName) || workerId,
      online: !!(w && w.online) && !(c && c.offline),
      branch: (c && c.branch) || '', worktree: (c && c.worktree) || '',
      lastPull: (c && c.lastPull) || null, source: (c && c.source) || '',
      notice: (c && c.notice) || '',
    };
  }

  function view(chatId) {
    const c = chats.get(String(chatId || ''));
    if (!c) return null;
    return {
      chat: c.chat, worker: c.workerName, workerId: c.workerId, project: c.project, source: c.source, base: c.base,
      state: c.state, running: live.has(c.chat), offline: !!c.offline, factory: !!c.factory, terminal: !!c.pty, notice: c.notice || '',
      branch: c.branch || '', worktree: c.worktree || '', lastPull: c.lastPull || null, error: c.error || '',
    };
  }
  const list = ({ workerId } = {}) => [...chats.values()].filter(c => !workerId || c.workerId === workerId).slice(-50).reverse().map(c => view(c.chat));

  /**
   * Claude in Terminal mode on a PC (claude-terminals.js): the same chat
   * underneath (folder, MCP through the worker, attach after a drop, pull
   * back), in a pseudo-terminal there. Returns what node-pty returns, as far
   * as claude-terminals uses it: onData, onExit, write, resize, kill, pid.
   */
  async function launchPty(where, { args = [], env = {}, cwd = '', chatId, project = '', resumeSessionId = '', cols = 120, rows = 30, command = '' } = {}) {
    const proc = await launch(where, { args, env, cwd, chatId, project, resumeSessionId, pty: { cols, rows }, command: command === 'shell' ? 'shell' : '' });
    const c = chats.get(chatId);
    const dataCbs = [], exitCbs = [], noticeCbs = [];
    const dec = new StringDecoder('utf8');
    const out = (text) => { if (text) for (const cb of dataCbs) { try { cb(text); } catch { /* a listener */ } } };
    proc.stdout.on('data', (b) => out(dec.write(Buffer.isBuffer(b) ? b : Buffer.from(String(b)))));
    // The server's own lines (why it ended, a dropped message) on the screen too.
    proc.stderr.on('data', (b) => out(String(b).replace(/\r?\n/g, '\r\n')));
    proc.on('notice', (text) => { for (const cb of noticeCbs) { try { cb(text); } catch { /* a listener */ } } });
    proc.on('exit', (code, signal) => {
      out(dec.end());
      for (const cb of exitCbs) { try { cb({ exitCode: code ?? (signal ? 1 : 0), signal: signal || undefined }); } catch { /* a listener */ } }
    });
    return {
      get pid() { return proc.pid; },
      onData: (cb) => { if (typeof cb === 'function') dataCbs.push(cb); },
      onExit: (cb) => { if (typeof cb === 'function') exitCbs.push(cb); },
      onNotice: (cb) => { if (typeof cb === 'function') noticeCbs.push(cb); },
      write: (data) => { if (data != null && data !== '') input(c, Buffer.from(String(data), 'utf8')); },
      resize: (cols2, rows2) => { resize(c, cols2, rows2); },
      kill: (sig) => { kill(c, sig || 'SIGTERM'); },
      remote: true,
    };
  }

  function dispose() {
    for (const l of live.values()) if (l.lostTimer) clearTimeout(l.lostTimer);
    for (const p of [...pulls.values()]) done(p, { ok: false, error: 'The server is stopping.' });
    for (const id of [...backWaiters.keys()]) wakeBack(id, false);
    if (saveTimer) {
      clearTimeout(saveTimer); saveTimer = null;
      try { writeFileSync(FILE, JSON.stringify({ chats: [...chats.values()] }, null, 1)); } catch { /* fine */ }
    }
  }

  return {
    launch, launchPty, offer, refusal, onMessage, onHello, onDrop, onTurnEnd, isChatId,
    pull, pullChat: (chatId, opts = {}) => pull(chatId, opts || {}), syncChat,
    status, view, list, dispose, live: (id) => live.has(id),
  };
}
