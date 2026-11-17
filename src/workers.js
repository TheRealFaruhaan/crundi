/**
 * workers.js — PCs that join this server as workers (phase 1).
 *
 * A worker is the crundi-worker program (worker/, Rust) on a PC or a Linux
 * server: a tray app, headless, or run by the Crundi PC client. It joins with
 * a one-time code (or, from the PC client, with the owner's own sign-in), gets
 * a key whose hash is all that is kept here, and holds one websocket link to
 * /api/workers/link. Over it: what the PC is (info, from which labels are
 * made), pause, and a terminal on the PC for the owner. Jobs and chats on a PC
 * come in later phases (specs/cluster-plan.md); the contract is
 * specs/workers-protocol.md.
 *
 * Owner only, everywhere: collaborators never see or reach a worker.
 */

import { readFileSync, writeFileSync, renameSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { randomBytes, createHash, timingSafeEqual, randomInt } from 'node:crypto';
import { WebSocketServer } from 'ws';

export const LINK_PATH = '/api/workers/link';
export const JOIN_PATH = '/api/workers/join';
export const PROTO = 1;
export const SERVER_CAPS = Object.freeze(['terminal', 'pause', 'rename']);

const CODE_ALPHABET = 'ABCDEFGHJKMNPQRSTUVWXYZ23456789';
const CODE_TTL_MS = 10 * 60 * 1000;
const HISTORY_KEEP = 300;
const LOGS_KEEP = 200;
const MAX_TERMS = 8;
const PING_MS = 25_000;
const DEAD_MS = 60_000;
const JOIN_FAILS = 10;
const JOIN_WINDOW_MS = 10 * 60 * 1000;
const LABEL_RE = /^[a-z0-9][a-z0-9:._-]{0,39}$/;
const KINDS = ['tray', 'headless', 'electron', 'native'];

const hash = (s) => createHash('sha256').update(String(s)).digest('hex');
const sameHash = (a, b) => {
  const x = Buffer.from(String(a || ''), 'hex');
  const y = Buffer.from(String(b || ''), 'hex');
  return x.length === 32 && x.length === y.length && timingSafeEqual(x, y);
};
const str = (v, n) => (typeof v === 'string' ? v.slice(0, n) : '');
const num = (v) => (typeof v === 'number' && Number.isFinite(v) ? v : null);

/** A join code: 8 characters nobody misreads. */
export function makeCode() {
  let s = '';
  for (let i = 0; i < 8; i++) s += CODE_ALPHABET[randomInt(CODE_ALPHABET.length)];
  return s;
}

/** The info a worker reports, cleaned to the spec's shape (unknown fields dropped). */
export function cleanInfo(raw) {
  const i = raw && typeof raw === 'object' ? raw : {};
  const os = ['windows', 'macos', 'linux'].includes(i.os) ? i.os : str(i.os, 20);
  const cpu = i.cpu && typeof i.cpu === 'object' ? { model: str(i.cpu.model, 120), cores: num(i.cpu.cores), threads: num(i.cpu.threads) } : null;
  const gpus = Array.isArray(i.gpus) ? i.gpus.slice(0, 8).map(g => ({ vendor: str(g?.vendor, 40), name: str(g?.name, 120), vramMb: num(g?.vramMb) })) : [];
  const disk = i.disk && typeof i.disk === 'object' ? { path: str(i.disk.path, 400), freeMb: num(i.disk.freeMb), totalMb: num(i.disk.totalMb) } : null;
  const tools = {};
  if (i.tools && typeof i.tools === 'object') {
    for (const [k, v] of Object.entries(i.tools).slice(0, 40)) {
      if (/^[a-z0-9_-]{1,20}$/.test(k) && typeof v === 'string') tools[k] = v.slice(0, 60);
    }
  }
  return {
    os, osVersion: str(i.osVersion, 80), arch: str(i.arch, 20), hostname: str(i.hostname, 120),
    cpu, ramMb: num(i.ramMb), ramFreeMb: num(i.ramFreeMb),
    load: Array.isArray(i.load) ? i.load.slice(0, 3).map(num) : null,
    gpus, disk, workDir: str(i.workDir, 400),
    onBattery: typeof i.onBattery === 'boolean' ? i.onBattery : null,
    idleSec: num(i.idleSec),
    tools,
    claudeLoggedIn: typeof i.claudeLoggedIn === 'boolean' ? i.claudeLoggedIn : null,
    allow: i.allow && typeof i.allow === 'object' ? { chats: !!i.allow.chats, commands: !!i.allow.commands } : null,
    paused: !!i.paused,
  };
}

/** Labels made from info (os:, arch:, ram:, cores:, gpu:, tool:), then the owner's own. */
export function labelsOf(info, custom = []) {
  const out = [];
  const add = (l) => { const v = String(l).toLowerCase().replace(/[^a-z0-9:._-]/g, '-'); if (v && !out.includes(v)) out.push(v); };
  if (info) {
    if (info.os) add(`os:${info.os}`);
    if (info.arch) add(`arch:${info.arch}`);
    if (info.ramMb) add(`ram:${Math.floor(info.ramMb / 1024)}g`);
    if (info.cpu?.threads) add(`cores:${info.cpu.threads}`);
    for (const g of info.gpus || []) {
      const v = (g.vendor || g.name || '').toLowerCase();
      const vendor = /nvidia|geforce|rtx|quadro/.test(v) ? 'nvidia' : /amd|radeon/.test(v) ? 'amd' : /intel/.test(v) ? 'intel' : /apple/.test(v) ? 'apple' : (g.vendor || '').toLowerCase();
      if (vendor) add(`gpu:${vendor}`);
    }
    for (const t of Object.keys(info.tools || {})) add(`tool:${t}`);
  }
  for (const l of custom || []) add(l);
  return out;
}

/** Custom labels as the owner typed them: a list or "a, b c" → clean, unique, up to 20. */
export function cleanLabels(v) {
  const list = Array.isArray(v) ? v : String(v || '').split(/[\s,]+/);
  const out = [];
  for (const x of list) {
    const l = String(x || '').trim().toLowerCase();
    if (LABEL_RE.test(l) && !out.includes(l)) out.push(l);
    if (out.length >= 20) break;
  }
  return out;
}

export function createWorkersService({ dataDir, file, now = () => Date.now(), serverVersion = '', onChange = () => {}, log = () => {} } = {}) {
  const FILE = file || join(dataDir, 'workers.json');
  let d = { version: 1, workers: [] };
  try { const raw = JSON.parse(readFileSync(FILE, 'utf8')); if (Array.isArray(raw.workers)) d = { version: 1, workers: raw.workers }; } catch { /* first run */ }

  let saveTimer = null;
  function save(immediate = false) {
    const write = () => {
      saveTimer = null;
      try {
        mkdirSync(join(FILE, '..'), { recursive: true });
        writeFileSync(`${FILE}.tmp`, JSON.stringify(d, null, 2), { mode: 0o600 });
        renameSync(`${FILE}.tmp`, FILE);
      } catch (err) { log(`[workers] save failed: ${err.message}`); }
    };
    if (immediate) { clearTimeout(saveTimer); write(); return; }
    if (!saveTimer) { saveTimer = setTimeout(write, 2000); saveTimer.unref?.(); }
  }

  const codes = new Map();          // code -> { expiresAt, reissueFor? }
  const joinFails = new Map();      // ip -> [times]
  const links = new Map();          // worker id -> live link
  const terms = new Map();          // server ref -> { workerId, ui, uiRef }
  let changeTimer = null;
  function changed() {
    if (changeTimer) return;
    changeTimer = setTimeout(() => { changeTimer = null; try { onChange(); } catch { /* a listener */ } }, 50);
    changeTimer.unref?.();
  }

  const byId = (id) => d.workers.find(w => w.id === id) || null;
  function event(w, type, extra = {}) {
    w.history = Array.isArray(w.history) ? w.history : [];
    w.history.push({ at: new Date(now()).toISOString(), type, ...extra });
    if (w.history.length > HISTORY_KEEP) w.history.splice(0, w.history.length - HISTORY_KEEP);
  }

  function newKey(w) {
    const secret = randomBytes(32).toString('base64url');
    w.keyHash = hash(secret);
    w.revoked = false;
    return `cw_${w.id}.${secret}`;
  }
  function workerByKey(key) {
    const m = /^cw_(w[0-9a-f]{10})\.([A-Za-z0-9_-]{20,})$/.exec(String(key || ''));
    if (!m) return null;
    const w = byId(m[1]);
    return w && !w.revoked && sameHash(w.keyHash, hash(m[2])) ? w : null;
  }

  function sweepCodes() { const t = now(); for (const [c, v] of codes) if (v.expiresAt <= t) codes.delete(c); }
  function newCode(reissueFor = '') {
    sweepCodes();
    if (codes.size > 50) throw new Error('Too many join codes waiting. Wait for some to expire.');
    let c; do c = makeCode(); while (codes.has(c));
    codes.set(c, { expiresAt: now() + CODE_TTL_MS, reissueFor });
    return { code: c, expiresAt: new Date(now() + CODE_TTL_MS).toISOString() };
  }

  /** A new worker, or a known one again (same device, or a reissue): returns { id, key, name }. */
  function enroll({ name, deviceId, kind, version, os, reissueFor = '' }) {
    const dev = str(deviceId, 80);
    let w = reissueFor ? byId(reissueFor) : (dev ? d.workers.find(x => x.deviceId === dev) : null);
    const nm = str(name, 60).trim();
    if (!w) {
      let id; do id = 'w' + randomBytes(5).toString('hex'); while (byId(id));
      w = {
        id, name: nm || str(os, 20) || 'PC', deviceId: dev, kind: KINDS.includes(kind) ? kind : 'headless',
        version: str(version, 40), createdAt: new Date(now()).toISOString(), lastSeenAt: null,
        allowChats: true, allowCommands: true, paused: false, pausedBy: '', customLabels: [],
        info: null, revoked: false, keyHash: '', history: [],
      };
      d.workers.push(w);
      event(w, 'joined', { kind: w.kind });
    } else {
      if (nm && !reissueFor) w.name = nm;
      if (KINDS.includes(kind)) w.kind = kind;
      if (version) w.version = str(version, 40);
      if (dev) w.deviceId = dev;
      event(w, 'rejoined', { kind: w.kind });
      // The old key ends here: drop a link that used it.
      const l = links.get(w.id);
      if (l) { try { l.ws.send(JSON.stringify({ type: 'bye', reason: 'replaced' })); l.ws.close(4001, 'replaced'); } catch { /* gone */ } }
    }
    const key = newKey(w);
    save(true); changed();
    return { id: w.id, key, name: w.name };
  }

  // ── Public: POST /api/workers/join { code, ... } ──
  async function readJson(req, limit = 16 * 1024) {
    return new Promise((resolve) => {
      let size = 0; const parts = [];
      req.on('data', (c) => { size += c.length; if (size > limit) { req.destroy(); resolve(null); } else parts.push(c); });
      req.on('end', () => { try { resolve(JSON.parse(Buffer.concat(parts).toString('utf8') || '{}')); } catch { resolve(null); } });
      req.on('error', () => resolve(null));
    });
  }
  function reply(res, code, body) {
    res.writeHead(code, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' });
    res.end(JSON.stringify(body));
  }
  async function handlePublic(req, res, path) {
    if (path !== JOIN_PATH) return false;
    if (req.method !== 'POST') { reply(res, 405, { ok: false, error: 'POST only' }); return true; }
    const ip = String(req.socket?.remoteAddress || '');
    const t = now();
    const fails = (joinFails.get(ip) || []).filter(x => t - x < JOIN_WINDOW_MS);
    if (fails.length >= JOIN_FAILS) { reply(res, 429, { ok: false, error: 'Too many wrong codes. Try again in a few minutes.' }); return true; }
    const b = await readJson(req);
    if (!b) { reply(res, 400, { ok: false, error: 'Bad request' }); return true; }
    sweepCodes();
    const code = String(b.code || '').trim().toUpperCase().replace(/[\s-]/g, '');
    const c = codes.get(code);
    if (!c) {
      fails.push(t); joinFails.set(ip, fails);
      reply(res, 403, { ok: false, error: 'That code is wrong or has expired. Make a new one in Crundi: Workers, Add a PC.' });
      return true;
    }
    codes.delete(code);
    const r = enroll({ name: b.name, deviceId: b.deviceId, kind: b.kind, version: b.version, os: b.os, reissueFor: c.reissueFor });
    log(`[workers] ${r.name} (${r.id}) joined`);
    reply(res, 200, { ok: true, ...r });
    return true;
  }

  // ── The link ──
  const wss = new WebSocketServer({ noServer: true, maxPayload: 4 * 1024 * 1024 });
  function handleUpgrade(req, socket, head) {
    let url;
    try { url = new URL(req.url, 'http://localhost'); } catch { return false; }
    if (url.pathname !== LINK_PATH) return false;
    const auth = String(req.headers.authorization || '');
    const key = auth.startsWith('Bearer cw_') ? auth.slice(7) : url.searchParams.get('key');
    const w = workerByKey(key);
    if (!w) { socket.write('HTTP/1.1 401 Unauthorized\r\n\r\n'); socket.destroy(); return true; }
    wss.handleUpgrade(req, socket, head, (ws) => attach(ws, w));
    return true;
  }

  function sendTo(id, msg) {
    const l = links.get(id);
    if (!l || l.ws.readyState !== 1) return false;
    try { l.ws.send(JSON.stringify(msg)); return true; } catch { return false; }
  }

  function attach(ws, w) {
    const prev = links.get(w.id);
    if (prev) { try { prev.ws.send(JSON.stringify({ type: 'bye', reason: 'replaced' })); prev.ws.close(4001, 'replaced'); } catch { /* gone */ } drop(w.id, prev, 'replaced'); }
    const link = { ws, connectedAt: now(), lastPong: now(), hello: false, caps: [], logs: [] };
    links.set(w.id, link);
    ws.on('pong', () => { link.lastPong = now(); });
    const ping = setInterval(() => {
      if (now() - link.lastPong > DEAD_MS) { try { ws.terminate(); } catch { /* gone */ } return; }
      try { ws.ping(); } catch { /* closing */ }
    }, PING_MS);
    ping.unref?.();
    ws.on('message', (raw, isBinary) => {
      if (isBinary) return;   // phase 2 streams
      let m; try { m = JSON.parse(raw.toString()); } catch { return; }
      if (!m || typeof m.type !== 'string') return;
      onWorkerMessage(w, link, m);
    });
    ws.on('close', () => { clearInterval(ping); if (links.get(w.id) === link) drop(w.id, link, 'disconnected'); });
    ws.on('error', () => { /* close follows */ });
  }

  function drop(id, link, why) {
    if (links.get(id) === link) links.delete(id);
    const w = byId(id);
    if (w) { w.lastSeenAt = new Date(now()).toISOString(); if (link.hello && why !== 'replaced') event(w, 'disconnected'); save(); }
    for (const [ref, t] of terms) {
      if (t.workerId !== id) continue;
      terms.delete(ref);
      t.ui({ type: 'worker-term-exit', ref: t.uiRef, code: null, error: 'The PC disconnected.' });
    }
    changed();
  }

  function onWorkerMessage(w, link, m) {
    w.lastSeenAt = new Date(now()).toISOString();
    switch (m.type) {
      case 'hello': {
        link.hello = true;
        link.caps = Array.isArray(m.caps) ? m.caps.filter(c => typeof c === 'string').slice(0, 40) : [];
        if (KINDS.includes(m.kind)) w.kind = m.kind;
        if (m.version) w.version = str(m.version, 40);
        if (m.info) applyInfo(w, m.info);
        event(w, 'connected', { version: w.version });
        save();
        sendTo(w.id, {
          type: 'welcome', proto: PROTO, id: w.id, name: w.name, server: { version: serverVersion },
          caps: [...SERVER_CAPS], settings: { allowChats: w.allowChats, allowCommands: w.allowCommands, paused: w.paused },
        });
        changed();
        return;
      }
      case 'info': applyInfo(w, m.info); save(); changed(); return;
      case 'paused': {
        w.paused = !!m.paused; w.pausedBy = w.paused ? 'pc' : '';
        event(w, w.paused ? 'paused' : 'resumed', { by: 'pc' });
        save(); changed(); return;
      }
      case 'log': {
        link.logs.push({ at: new Date(now()).toISOString(), level: ['info', 'warn', 'error'].includes(m.level) ? m.level : 'info', text: str(m.text, 1000) });
        if (link.logs.length > LOGS_KEEP) link.logs.splice(0, link.logs.length - LOGS_KEEP);
        changed(); return;
      }
      case 'term.data': case 'term.exit': case 'term.error': {
        const t = terms.get(String(m.ref || ''));
        if (!t || t.workerId !== w.id) return;
        if (m.type === 'term.data') {
          if (typeof m.b64 !== 'string') return;
          t.ui({ type: 'worker-term-data', ref: t.uiRef, data: Buffer.from(m.b64, 'base64').toString('utf8') });
        } else {
          terms.delete(m.ref);
          if (m.type === 'term.exit') { t.ui({ type: 'worker-term-exit', ref: t.uiRef, code: num(m.code) }); event(w, 'terminal-closed'); }
          else t.ui({ type: 'worker-term-error', ref: t.uiRef, error: str(m.error, 300) || 'The terminal could not open.' });
        }
        return;
      }
      default: return;   // newer worker, older server
    }
  }

  function applyInfo(w, raw) {
    const info = cleanInfo(raw);
    w.info = info;
    // The PC's own pause wins over a stale record (it is the PC's switch too).
    if (info.paused !== w.paused && w.pausedBy !== 'owner') { w.paused = info.paused; w.pausedBy = info.paused ? 'pc' : ''; }
  }

  // ── The owner's view and actions ──
  function view(w) {
    const l = links.get(w.id);
    return {
      id: w.id, name: w.name, kind: w.kind, version: w.version, deviceId: w.deviceId || '',
      online: !!(l && l.hello), connectedSince: l ? new Date(l.connectedAt).toISOString() : null,
      lastSeenAt: w.lastSeenAt, createdAt: w.createdAt, revoked: !!w.revoked,
      info: w.info, labels: labelsOf(w.info, w.customLabels), customLabels: w.customLabels || [],
      allowChats: w.allowChats !== false, allowCommands: w.allowCommands !== false,
      paused: !!w.paused, pausedBy: w.pausedBy || '', terminals: [...terms.values()].filter(t => t.workerId === w.id).length,
      logs: l ? l.logs.slice(-50) : [],
    };
  }
  function list() {
    sweepCodes();
    return {
      workers: d.workers.map(view),
      codes: [...codes.entries()].filter(([, v]) => !v.reissueFor).map(([code, v]) => ({ code, expiresAt: new Date(v.expiresAt).toISOString() })),
    };
  }

  function act(body = {}) {
    const a = String(body.action || '');
    if (a === 'code') return { ok: true, ...newCode() };
    if (a === 'join-self') {
      const r = enroll({ name: body.name, deviceId: body.deviceId, kind: KINDS.includes(body.kind) ? body.kind : 'electron', version: body.version, os: body.os });
      return { ok: true, ...r };
    }
    const w = byId(String(body.id || ''));
    if (!w) return { ok: false, status: 404, error: 'No such worker.' };
    switch (a) {
      case 'rename': {
        const n = str(body.name, 60).trim();
        if (!n) return { ok: false, error: 'A name is needed.' };
        event(w, 'renamed', { from: w.name, to: n });
        w.name = n;
        sendTo(w.id, { type: 'settings', name: n });
        break;
      }
      case 'set': {
        const s = {};
        if (typeof body.allowChats === 'boolean') { w.allowChats = body.allowChats; s.allowChats = body.allowChats; }
        if (typeof body.allowCommands === 'boolean') { w.allowCommands = body.allowCommands; s.allowCommands = body.allowCommands; }
        if (typeof body.paused === 'boolean' && body.paused !== !!w.paused) {
          w.paused = body.paused; w.pausedBy = body.paused ? 'owner' : ''; s.paused = body.paused;
          event(w, body.paused ? 'paused' : 'resumed', { by: 'owner' });
        }
        if (body.labels !== undefined) w.customLabels = cleanLabels(body.labels);
        if (Object.keys(s).length) { event(w, 'settings', s); sendTo(w.id, { type: 'settings', ...s }); }
        break;
      }
      case 'revoke': {
        w.revoked = true; w.keyHash = '';
        event(w, 'revoked');
        const l = links.get(w.id);
        if (l) { try { l.ws.send(JSON.stringify({ type: 'bye', reason: 'revoked' })); l.ws.close(4003, 'revoked'); } catch { /* gone */ } }
        break;
      }
      case 'reissue': {
        save(true); changed();
        return { ok: true, ...newCode(w.id) };
      }
      case 'remove': {
        const l = links.get(w.id);
        if (l) { try { l.ws.send(JSON.stringify({ type: 'bye', reason: 'removed' })); l.ws.close(4003, 'removed'); } catch { /* gone */ } }
        d.workers = d.workers.filter(x => x !== w);
        save(true); changed();
        return { ok: true };
      }
      case 'history': return { ok: true, history: (w.history || []).slice().reverse() };
      default: return { ok: false, error: 'Unknown action.' };
    }
    save(true); changed();
    return { ok: true, worker: view(w) };
  }

  /** /api/workers (owner only). Returns true when it answered. */
  async function handleApi(req, res, path, { principal, readBody, json, isOwner }) {
    if (path !== '/api/workers') return false;
    if (!isOwner(principal)) { json(res, { ok: false, error: 'Workers are the owner\'s.' }, 403); return true; }
    if (req.method === 'GET') { json(res, { ok: true, ...list() }); return true; }
    if (req.method !== 'POST') { json(res, { ok: false, error: 'GET or POST' }, 405); return true; }
    let body = {};
    try { body = JSON.parse(await readBody(req) || '{}'); } catch { json(res, { ok: false, error: 'Bad JSON' }, 400); return true; }
    let r;
    try { r = act(body); } catch (err) { r = { ok: false, error: err.message }; }
    json(res, r, r.ok ? 200 : (r.status || 400));
    return true;
  }

  // ── The owner's terminal on a PC, relayed for one UI socket ──
  /** Terminals for one UI socket: `ui(msg)` sends to that page. Call close() when it goes. */
  function uiTerminals(ui) {
    const mine = new Map();   // uiRef -> server ref
    return {
      open({ ref, worker, cols, rows }) {
        const uiRef = str(ref, 40);
        if (!uiRef) return;
        const w = byId(String(worker || ''));
        const l = w && links.get(w.id);
        if (!w || !l || !l.hello) { ui({ type: 'worker-term-error', ref: uiRef, error: 'That PC is not connected.' }); return; }
        if (!l.caps.includes('terminal')) { ui({ type: 'worker-term-error', ref: uiRef, error: 'This worker cannot open a terminal. Update it.' }); return; }
        if ([...terms.values()].filter(t => t.workerId === w.id).length >= MAX_TERMS) { ui({ type: 'worker-term-error', ref: uiRef, error: `At most ${MAX_TERMS} terminals on one PC.` }); return; }
        if (mine.has(uiRef)) this.close({ ref: uiRef });
        const sref = 't' + randomBytes(6).toString('hex');
        terms.set(sref, { workerId: w.id, ui, uiRef });
        mine.set(uiRef, sref);
        event(w, 'terminal-opened');
        save();
        sendTo(w.id, { type: 'term.open', ref: sref, cols: Math.max(10, Math.min(500, cols | 0 || 80)), rows: Math.max(4, Math.min(300, rows | 0 || 24)) });
        changed();
      },
      input({ ref, data }) {
        const sref = mine.get(str(ref, 40)); const t = sref && terms.get(sref);
        if (t && typeof data === 'string') sendTo(t.workerId, { type: 'term.input', ref: sref, b64: Buffer.from(data, 'utf8').toString('base64') });
      },
      resize({ ref, cols, rows }) {
        const sref = mine.get(str(ref, 40)); const t = sref && terms.get(sref);
        if (t && cols > 0 && rows > 0) sendTo(t.workerId, { type: 'term.resize', ref: sref, cols: Math.min(500, cols | 0), rows: Math.min(300, rows | 0) });
      },
      close({ ref }) {
        const sref = mine.get(str(ref, 40)); mine.delete(str(ref, 40));
        const t = sref && terms.get(sref);
        if (t) { terms.delete(sref); sendTo(t.workerId, { type: 'term.close', ref: sref }); changed(); }
      },
      closeAll() { for (const r of [...mine.keys()]) this.close({ ref: r }); },
    };
  }

  /** worker_list for a chat (owner chats only; the caller checks). */
  function mcpList() {
    return {
      ok: true,
      workers: d.workers.filter(w => !w.revoked).map(w => {
        const v = view(w); const i = v.info || {};
        return {
          id: v.id, name: v.name, online: v.online, paused: v.paused, kind: v.kind, labels: v.labels,
          allowChats: v.allowChats, allowCommands: v.allowCommands,
          os: i.os || null, cpuThreads: i.cpu?.threads ?? null, ramGb: i.ramMb ? Math.round(i.ramMb / 102.4) / 10 : null,
          gpus: (i.gpus || []).map(g => g.name).filter(Boolean), diskFreeGb: i.disk?.freeMb ? Math.round(i.disk.freeMb / 102.4) / 10 : null,
          tools: i.tools || {}, lastSeenAt: v.online ? null : v.lastSeenAt,
        };
      }),
      note: 'Running commands or chats on a PC comes in a later Crundi version; this lists them.',
    };
  }

  function dispose() {
    for (const [, l] of links) { try { l.ws.close(1001, 'server stopping'); } catch { /* gone */ } }
    links.clear();
    if (saveTimer) save(true);
    try { wss.close(); } catch { /* fine */ }
  }

  return { handlePublic, handleUpgrade, handleApi, act, list, uiTerminals, mcpList, dispose, isOnline: (id) => !!links.get(id)?.hello };
}
