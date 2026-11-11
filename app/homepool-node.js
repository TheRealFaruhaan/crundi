/**
 * homepool-node.js — a homepool node: lends this machine's connection to a
 * Crundi server, and the wire format both ends speak.
 *
 * A node holds ONE websocket to the server (/api/homepool/node, signed in with
 * a node key) and the server asks it, stream by stream, to open TCP
 * connections from here. The node only ever dials out. It listens on nothing,
 * needs no port forwarding, and reaches only what the server asks for, one
 * host:port at a time. Host names are resolved here, so DNS leaves from this
 * connection too.
 *
 * Used by the desktop app (app/main.js), the standalone Docker node
 * (homepool-node/docker) and the server (src/homepool.js, for the codec). The
 * Android node (android/.../homepool/NodeLink.kt) speaks the same format.
 *
 * ── Frames ──
 * Binary: [type u8][stream id u32 BE][payload]
 *   OPEN    1  server → node   payload: JSON {"host","port"}
 *   DATA    2  both            payload: bytes
 *   CLOSE   3  both            payload: empty
 *   OPENED  4  node → server
 *   ERROR   5  node → server   payload: a short reason (utf8)
 *   PAUSE   6  both            receiver's socket is full: stop sending on this stream
 *   RESUME  7  both            it drained: carry on
 * Text (JSON):
 *   server → node  {type:"hello", node:{id,name,network}, bindTtlMs}
 *                  {type:"stats", conns, slots, bytesToday, grants:[…]}   every ~10 s
 *                  {type:"bye", reason}                                   revoked, or replaced
 *   node → server  {type:"info", platform, app, version, network, publicIp,
 *                   paused, pauseReason, bytesToday, mobileBytesToday}
 */

import net from 'node:net';
import { createRequire } from 'node:module';

export const T = Object.freeze({ OPEN: 1, DATA: 2, CLOSE: 3, OPENED: 4, ERROR: 5, PAUSE: 6, RESUME: 7 });
export const NODE_PATH = '/api/homepool/node';
/** Above this many bytes queued on the websocket, stop reading the TCP side. */
export const WS_HIGH_WATER = 1024 * 1024;

export function frame(type, id, payload) {
  const p = payload == null ? Buffer.alloc(0) : (Buffer.isBuffer(payload) ? payload : Buffer.from(String(payload), 'utf8'));
  const b = Buffer.allocUnsafe(5 + p.length);
  b[0] = type;
  b.writeUInt32BE(id >>> 0, 1);
  p.copy(b, 5);
  return b;
}

export function parseFrame(buf) {
  if (!buf) return null;
  if (!Buffer.isBuffer(buf)) buf = Buffer.from(buf);
  if (buf.length < 5) return null;
  return { type: buf[0], id: buf.readUInt32BE(1), payload: buf.subarray(5) };
}

/**
 * The connection code a node is set up from — scanned or pasted:
 *   crundi-homepool://node?s=<server url>&k=<node key>
 * The key alone is enough; it is not a Crundi login and opens nothing else.
 */
export function nodeCode(server, key) {
  return `crundi-homepool://node?s=${encodeURIComponent(server)}&k=${encodeURIComponent(key)}`;
}

export function parseNodeCode(code) {
  try {
    const u = new URL(String(code || '').trim());
    if (u.protocol !== 'crundi-homepool:') return null;
    const s = u.searchParams.get('s') || '';
    const k = u.searchParams.get('k') || '';
    if (!/^https?:\/\//i.test(s) || !/^hpn_/.test(k)) return null;
    return { server: s.replace(/\/+$/, ''), key: k };
  } catch { return null; }
}

/** May a node open this? Never the node's own machine or its private network. */
export function isPrivateTarget(host) {
  const h = String(host || '').toLowerCase().replace(/^\[|\]$/g, '');
  if (h === 'localhost' || h.endsWith('.localhost') || h.endsWith('.local') || h.endsWith('.internal')) return true;
  const v = net.isIP(h);
  if (v === 4) {
    const [a, b] = h.split('.').map(Number);
    return a === 10 || a === 127 || a === 0 || (a === 169 && b === 254) || (a === 172 && b >= 16 && b <= 31)
      || (a === 192 && b === 168) || (a === 100 && b >= 64 && b <= 127) || a >= 224;
  }
  if (v === 6) return h === '::1' || h === '::' || h.startsWith('fc') || h.startsWith('fd') || h.startsWith('fe80');
  return false;
}

/**
 * Run a node. Reconnects for as long as it is started.
 *
 *   server   https://dev.example.com
 *   key      hpn_…                    (from Settings › Homepool › Nodes)
 *   info()   extra fields for the "info" message (network label, app, …)
 *   onState  (state, detail) — "connecting" | "connected" | "stopped" | "refused"
 *   onLog    (line)
 *   onStats  (msg) — the server's view of this node
 *   allowPrivate  tests only: let the node open loopback/private addresses
 *
 * Returns { stop(), status() }.
 */
export function startNode({ server, key, info = () => ({}), onState = () => {}, onLog = () => {}, onStats = () => {}, WebSocketImpl, allowPrivate = false } = {}) {
  const require = createRequire(import.meta.url);
  const WS = WebSocketImpl || (() => { try { return require('ws'); } catch { return globalThis.WebSocket; } })();
  if (!WS) throw new Error('no WebSocket available');
  const base = String(server || '').replace(/\/+$/, '');
  const url = base.replace(/^http/i, 'ws') + NODE_PATH;

  let ws = null;
  let stopped = false;
  let retry = 1000;
  let timer = null;
  let infoTimer = null;
  let state = 'stopped';
  let hello = null;
  let lastStats = null;
  let connectedAt = 0;
  const streams = new Map();       // id → net.Socket
  const counters = { bytesUp: 0, bytesDown: 0, opened: 0 };

  const setState = (s, detail = '') => { state = s; try { onState(s, detail); } catch { /* the caller's problem */ } };
  const log = (l) => { try { onLog(l); } catch { /* ignore */ } };

  function send(b) { if (ws && ws.readyState === 1) ws.send(b); }
  function buffered() { return ws ? (ws.bufferedAmount || 0) : 0; }

  function closeStream(id, tell) {
    const s = streams.get(id);
    if (!s) return;
    streams.delete(id);
    s.destroy();
    if (tell) send(frame(T.CLOSE, id));
  }

  function open(id, payload) {
    let host, port;
    try { ({ host, port } = JSON.parse(payload.toString('utf8'))); } catch { /* bad frame */ }
    port = Number(port);
    if (!host || !(port > 0 && port < 65536)) { send(frame(T.ERROR, id, 'bad target')); return; }
    // The server is the one deciding what to open, but a node should never be
    // turned into a way into its own home network.
    if (!allowPrivate && isPrivateTarget(host)) { send(frame(T.ERROR, id, 'private address refused by the node')); return; }
    const sock = net.connect({ host, port });
    streams.set(id, sock);
    counters.opened++;
    sock.setNoDelay(true);
    sock.once('connect', () => send(frame(T.OPENED, id)));
    sock.on('data', (d) => {
      counters.bytesUp += d.length;
      send(frame(T.DATA, id, d));
      if (buffered() > WS_HIGH_WATER) {
        sock.pause();
        const wait = setInterval(() => { if (!streams.has(id) || buffered() < WS_HIGH_WATER / 2) { clearInterval(wait); sock.resume(); } }, 20);
      }
    });
    sock.on('drain', () => send(frame(T.RESUME, id)));
    sock.once('error', (err) => {
      if (!streams.has(id)) return;
      streams.delete(id);
      send(frame(T.ERROR, id, err.code || err.message || 'error'));
    });
    sock.once('close', () => { if (streams.has(id)) { streams.delete(id); send(frame(T.CLOSE, id)); } });
  }

  function onBinary(raw) {
    const f = parseFrame(raw);
    if (!f) return;
    if (f.type === T.OPEN) return open(f.id, f.payload);
    const s = streams.get(f.id);
    if (!s) return;
    if (f.type === T.DATA) { counters.bytesDown += f.payload.length; if (!s.write(f.payload)) send(frame(T.PAUSE, f.id)); }
    else if (f.type === T.CLOSE) { streams.delete(f.id); s.end(); setTimeout(() => s.destroy(), 5000).unref?.(); }
    else if (f.type === T.PAUSE) s.pause();
    else if (f.type === T.RESUME) s.resume();
  }

  function sendInfo() {
    let extra = {};
    try { extra = info() || {}; } catch { /* ignore */ }
    send(JSON.stringify({ type: 'info', ...extra }));
  }

  function connect() {
    if (stopped) return;
    setState('connecting');
    let sock;
    try {
      // The key goes in the query too: a plain WebSocket cannot set headers.
      sock = new WS(`${url}?key=${encodeURIComponent(key)}`, WS === globalThis.WebSocket ? undefined : { headers: { Authorization: `Bearer ${key}` } });
    } catch (err) { log(`connect failed: ${err.message}`); return later(); }
    ws = sock;
    // ws gives Buffers; a plain WebSocket (Node 22's own) gives ArrayBuffers.
    sock.binaryType = sock.on ? 'nodebuffer' : 'arraybuffer';
    const on = (ev, fn) => (sock.on ? sock.on(ev, fn) : sock.addEventListener(ev, (e) => fn(e.data ?? e, e)));
    on('open', () => {
      retry = 1000;
      connectedAt = Date.now();
      log(`connected to ${base}`);
      sendInfo();
      clearInterval(infoTimer);
      infoTimer = setInterval(sendInfo, 30000);
      setState('connected');
    });
    on('message', (data, isBinary) => {
      const bin = typeof isBinary === 'boolean' ? isBinary : typeof data !== 'string';
      if (bin) return onBinary(Buffer.isBuffer(data) ? data : Buffer.from(data));
      let m; try { m = JSON.parse(String(data)); } catch { return; }
      if (m.type === 'hello') { hello = m; log(`node "${m.node?.name || '?'}" on network "${m.node?.networkName || m.node?.network || '-'}"`); }
      else if (m.type === 'stats') { lastStats = m; try { onStats(m); } catch { /* ignore */ } }
      else if (m.type === 'bye') { log(`server ended the node: ${m.reason || ''}`); if (m.final) { stopped = true; setState('refused', m.reason || ''); } }
    });
    // ws gives the HTTP status of a refused upgrade; a revoked key is final.
    if (sock.on) sock.on('unexpected-response', (_req, res) => {
      if (res.statusCode === 401 || res.statusCode === 403) { stopped = true; log('the server refused this node key'); setState('refused', 'the server refused this node key'); }
      else log(`server answered ${res.statusCode}`);
    });
    // Once per socket, whichever comes first: Node's own WebSocket can report a
    // failed connect with 'error' and never send 'close'.
    let gone = false;
    const lost = () => {
      if (gone) return;
      gone = true;
      clearInterval(infoTimer);
      for (const id of [...streams.keys()]) closeStream(id, false);
      if (ws === sock) ws = null;
      connectedAt = 0;
      if (!stopped) { log('disconnected'); later(); }
      else if (state !== 'refused') setState('stopped');
    };
    // Never close() from here: on Node's own WebSocket that fires 'error'
    // again at once, and a refused connect looped.
    on('error', (e) => {
      if (gone) return;
      log(`connection error: ${e?.message || 'failed'}`);
      if (sock.readyState !== 1) setTimeout(lost, 50);
    });
    on('close', lost);
  }

  function later() {
    if (stopped) return;
    setState('connecting', `retrying in ${Math.round(retry / 1000)}s`);
    clearTimeout(timer);
    timer = setTimeout(connect, retry);
    retry = Math.min(retry * 2, 60000);
  }

  connect();

  return {
    stop() {
      stopped = true;
      clearTimeout(timer);
      clearInterval(infoTimer);
      try { ws?.close(); } catch { /* gone */ }
      for (const id of [...streams.keys()]) closeStream(id, false);
      setState('stopped');
    },
    /** Reconnect at once (the network changed under us). */
    kick() { if (stopped) return; try { ws?.terminate ? ws.terminate() : ws?.close(); } catch { /* gone */ } retry = 1000; },
    sendInfo,
    status() {
      return { state, connectedAt, node: hello?.node || null, streams: streams.size, ...counters, server: lastStats };
    },
  };
}

// ─── standalone: `node homepool-node.js` with HOMEPOOL_SERVER and HOMEPOOL_KEY ───
// This is what the Docker node runs (see Settings › Homepool › Nodes, or
// homepool-node/docker). Plain Node 22, no packages: the server hands out this
// very file at /api/homepool/node.mjs.

async function publicIp() {
  try {
    const r = await fetch('https://api.ipify.org', { signal: AbortSignal.timeout(10000) });
    return r.ok ? (await r.text()).trim() : '';
  } catch { return ''; }
}

async function main() {
  let server = process.env.HOMEPOOL_SERVER || '';
  let key = process.env.HOMEPOOL_KEY || '';
  if (process.env.HOMEPOOL_CODE) {
    const c = parseNodeCode(process.env.HOMEPOOL_CODE);
    if (c) { server = c.server; key = c.key; }
  }
  if (!server || !key) {
    console.error('Set HOMEPOOL_CODE (crundi-homepool://…), or HOMEPOOL_SERVER and HOMEPOOL_KEY.');
    process.exit(2);
  }
  let ip = '';
  const stamp = () => new Date().toISOString().replace('T', ' ').slice(0, 19);
  const node = startNode({
    server, key,
    info: () => ({ platform: process.env.HOMEPOOL_PLATFORM || 'docker', app: 'homepool-node', version: '1', network: 'Ethernet', publicIp: ip }),
    onState: (s, d) => console.log(`${stamp()}  ${s}${d ? ` (${d})` : ''}`),
    onLog: (l) => console.log(`${stamp()}  ${l}`),
  });
  const refreshIp = async () => { const n = await publicIp(); if (n && n !== ip) { ip = n; console.log(`${stamp()}  public IP ${ip}`); node.sendInfo(); } };
  refreshIp();
  setInterval(refreshIp, 10 * 60 * 1000);
  setInterval(() => {
    const s = node.status();
    if (s.state === 'refused') { console.error('The server refused this node key. Ask the owner for a new one.'); process.exit(3); }
  }, 5000);
  for (const sig of ['SIGINT', 'SIGTERM']) process.on(sig, () => { node.stop(); process.exit(0); });
}

const self = (() => { try { return new URL(import.meta.url).pathname; } catch { return ''; } })();
if (process.argv[1] && self && (process.argv[1] === self || process.argv[1].endsWith('/n.mjs') || process.argv[1].endsWith('homepool-node.mjs'))) main();
