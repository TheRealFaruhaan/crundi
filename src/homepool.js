/**
 * homepool.js — the shared exit. Traffic from chats, software and devices
 * leaves through a network the owner chose: the server's own connection, one
 * of his home networks (Crundi apps and standalone nodes he assigned to it),
 * or, while it still runs, the standalone homepool relay. See docs/homepool.md.
 *
 * NOTHING passes without an access key the owner made or approved. A key
 * names the networks it may use; within them it gets sticky slots, exactly as
 * the standalone relay does:
 *
 *   • a slot is bound to one exit and stays there (a person does not hop
 *     networks mid-session)
 *   • it is re-drawn after bindTtlMs, and only when there is somewhere else
 *   • an exit that disappears releases its slots at once
 *   • with no exit up, connections are REFUSED; leaving from the server
 *     instead happens only if the key says fallbackToServer
 *
 * Ways in (all on the loopback and Docker bridge addresses, never public):
 *   SOCKS5   :base      user = <key id>[@<network>][#<slot>], password = key secret
 *   HTTP     :base+1    the same login as Proxy-Authorization; GET /status
 *   blocks   :P, P+1…   per key with portBlock: no login, only from allowFrom
 *                       sources; P = any slot, P+N = slot N, P+slots+1 = status
 *   fetch    in-process, for the MCP tool homepool_fetch
 *
 * Nodes come in over /api/homepool/node (app/homepool-node.js has the format).
 */

import net from 'node:net';
import dns from 'node:dns/promises';
import os from 'node:os';
import http from 'node:http';
import { EventEmitter } from 'node:events';
import { Duplex } from 'node:stream';
import { execFile } from 'node:child_process';
import { T, frame, parseFrame, WS_HIGH_WATER, isPrivateTarget } from '../app/homepool-node.js';
import { SERVER_NETWORK, POOL_NETWORK, publicKey, publicNode } from './homepool-store.js';

const PING_MS = 25000;
const OPEN_TIMEOUT_MS = 20000;
const POOL_POLL_MS = 15000;

// ─── a stream carried over a node's websocket ───

class NodeStream extends Duplex {
  constructor(link, id) {
    super({ allowHalfOpen: false });
    this.link = link; this.id = id; this.remotePaused = false; this.waiters = [];
  }
  _write(chunk, _enc, cb) {
    this.link.send(frame(T.DATA, this.id, chunk));
    const go = () => (this.remotePaused || this.link.buffered() > WS_HIGH_WATER) ? this.waiters.push(go) : cb();
    go();
  }
  _final(cb) { this.link.send(frame(T.CLOSE, this.id)); cb(); }
  _read() { if (this.pausedRemote) { this.pausedRemote = false; this.link.send(frame(T.RESUME, this.id)); } }
  _destroy(err, cb) {
    if (this.link.streams.get(this.id) === this) { this.link.streams.delete(this.id); this.link.send(frame(T.CLOSE, this.id)); }
    cb(err);
  }
  wake() { const w = this.waiters; this.waiters = []; for (const f of w) f(); }
}

class NodeLink {
  constructor(ws, node, onGone) {
    this.ws = ws; this.node = node; this.streams = new Map(); this.nextId = 1;
    this.info = {}; this.connectedAt = Date.now(); this.lastPong = Date.now();
    this.onGone = onGone;
    this.drainTimer = setInterval(() => { if (this.buffered() < WS_HIGH_WATER / 2) for (const s of this.streams.values()) s.wake(); }, 25);
    this.drainTimer.unref?.();
  }
  get paused() { return !!this.info.paused; }
  buffered() { return this.ws.bufferedAmount || 0; }
  send(b) { if (this.ws.readyState === 1) this.ws.send(b, { binary: typeof b !== 'string' }); }
  open(host, port) {
    return new Promise((resolve, reject) => {
      const id = this.nextId++ >>> 0;
      const s = new NodeStream(this, id);
      this.streams.set(id, s);
      const t = setTimeout(() => { s.destroy(); reject(new Error('the node did not answer in time')); }, OPEN_TIMEOUT_MS);
      s.onOpened = () => { clearTimeout(t); resolve(s); };
      s.onError = (msg) => { clearTimeout(t); this.streams.delete(id); s.destroy(); reject(new Error(msg || 'the node could not connect')); };
      this.send(frame(T.OPEN, id, JSON.stringify({ host, port })));
    });
  }
  handle(raw) {
    const f = parseFrame(raw);
    if (!f) return;
    const s = this.streams.get(f.id);
    if (!s) return;
    switch (f.type) {
      case T.OPENED: s.onOpened?.(); s.onOpened = null; break;
      case T.ERROR: if (s.onError && s.onOpened) s.onError(f.payload.toString('utf8')); else { this.streams.delete(f.id); s.destroy(); } break;
      case T.DATA: if (!s.push(f.payload) && !s.pausedRemote) { s.pausedRemote = true; this.send(frame(T.PAUSE, f.id)); } break;
      case T.CLOSE: this.streams.delete(f.id); s.push(null); setTimeout(() => s.destroy(), 5000).unref?.(); break;
      case T.PAUSE: s.remotePaused = true; break;
      case T.RESUME: s.remotePaused = false; s.wake(); break;
    }
  }
  close(reason) {
    clearInterval(this.drainTimer);
    for (const s of this.streams.values()) { s.onError?.(reason || 'node gone'); s.destroy(); }
    this.streams.clear();
  }
}

// ─── SOCKS5 (server side and the client used for the standalone relay) ───

/** Read SOCKS5 greeting/auth/request from a socket. Resolves { user, pass, host, port, reply(ok) }. */
function socksAccept(sock, { needAuth }) {
  return new Promise((resolve, reject) => {
    let buf = Buffer.alloc(0);
    let stage = 'greet';
    let user = '', pass = '';
    const fail = (why, code) => { if (code != null) try { sock.write(code); } catch { /* gone */ } sock.removeListener('data', onData); reject(new Error(why)); };
    const timer = setTimeout(() => fail('socks handshake timed out'), 15000);
    const onData = (d) => {
      buf = Buffer.concat([buf, d]);
      for (;;) {
        if (stage === 'greet') {
          if (buf.length < 2) return;
          if (buf[0] !== 5) { clearTimeout(timer); return fail('not socks5'); }
          const n = buf[1];
          if (buf.length < 2 + n) return;
          const methods = [...buf.subarray(2, 2 + n)];
          buf = buf.subarray(2 + n);
          if (needAuth && methods.includes(2)) { sock.write(Buffer.from([5, 2])); stage = 'auth'; }
          else if (!needAuth && methods.includes(0)) { sock.write(Buffer.from([5, 0])); stage = 'req'; }
          else if (!needAuth && methods.includes(2)) { sock.write(Buffer.from([5, 2])); stage = 'auth'; }
          else { clearTimeout(timer); return fail('no acceptable auth method', Buffer.from([5, 0xff])); }
        } else if (stage === 'auth') {
          if (buf.length < 2) return;
          const ul = buf[1];
          if (buf.length < 3 + ul) return;
          const pl = buf[2 + ul];
          if (buf.length < 3 + ul + pl) return;
          user = buf.subarray(2, 2 + ul).toString('utf8');
          pass = buf.subarray(3 + ul, 3 + ul + pl).toString('utf8');
          buf = buf.subarray(3 + ul + pl);
          stage = 'authreply';
          clearTimeout(timer);
          sock.removeListener('data', onData);
          // The caller checks the login, then calls cont() to read the request.
          return resolve({
            user, pass, authed: true,
            accept: () => new Promise((res2, rej2) => {
              sock.write(Buffer.from([1, 0]));
              stage = 'req';
              const rest = buf; buf = Buffer.alloc(0);
              socksAccept._readRequest(sock, rest).then(res2, rej2);
            }),
            deny: () => { try { sock.end(Buffer.from([1, 1])); } catch { /* gone */ } },
          });
        } else if (stage === 'req') {
          clearTimeout(timer);
          sock.removeListener('data', onData);
          const rest = buf; buf = Buffer.alloc(0);
          return resolve({ user: '', pass: '', authed: false, accept: () => socksAccept._readRequest(sock, rest), deny: () => sock.destroy() });
        }
        if (!buf.length) return;
      }
    };
    sock.on('data', onData);
    sock.once('error', (e) => { clearTimeout(timer); reject(e); });
  });
}
socksAccept._readRequest = (sock, initial) => new Promise((resolve, reject) => {
  let buf = initial;
  const timer = setTimeout(() => { sock.removeListener('data', onData); reject(new Error('socks request timed out')); }, 15000);
  const tryParse = () => {
    if (buf.length < 5) return false;
    if (buf[0] !== 5) { finish(); reject(new Error('bad socks request')); return true; }
    const cmd = buf[1], atyp = buf[3];
    let host, off;
    if (atyp === 1) { if (buf.length < 10) return false; host = [...buf.subarray(4, 8)].join('.'); off = 8; }
    else if (atyp === 3) { const l = buf[4]; if (buf.length < 7 + l) return false; host = buf.subarray(5, 5 + l).toString('utf8'); off = 5 + l; }
    else if (atyp === 4) { if (buf.length < 22) return false; const p = []; for (let i = 0; i < 16; i += 2) p.push(buf.readUInt16BE(4 + i).toString(16)); host = p.join(':'); off = 20; }
    else { finish(); sock.end(Buffer.from([5, 8, 0, 1, 0, 0, 0, 0, 0, 0])); reject(new Error('bad address type')); return true; }
    const port = buf.readUInt16BE(off);
    const extra = buf.subarray(off + 2);
    finish();
    if (cmd !== 1) { sock.end(Buffer.from([5, 7, 0, 1, 0, 0, 0, 0, 0, 0])); reject(new Error('only CONNECT is supported')); return true; }
    resolve({
      host, port, extra,
      reply: (ok, code = 5) => { try { sock.write(Buffer.from([5, ok ? 0 : code, 0, 1, 0, 0, 0, 0, 0, 0])); } catch { /* gone */ } },
    });
    return true;
  };
  const finish = () => { clearTimeout(timer); sock.removeListener('data', onData); };
  const onData = (d) => { buf = Buffer.concat([buf, d]); tryParse(); };
  if (!tryParse()) sock.on('data', onData);
});

/** Connect to host:port through a SOCKS5 proxy (no auth). Resolves the socket. */
export function socksConnect(proxyHost, proxyPort, host, port, timeoutMs = OPEN_TIMEOUT_MS) {
  return new Promise((resolve, reject) => {
    const s = net.connect(proxyPort, proxyHost);
    let stage = 0; let buf = Buffer.alloc(0);
    const t = setTimeout(() => { s.destroy(); reject(new Error('the relay did not answer in time')); }, timeoutMs);
    const fail = (e) => { clearTimeout(t); s.destroy(); reject(e); };
    s.once('error', fail);
    s.once('connect', () => s.write(Buffer.from([5, 1, 0])));
    s.on('data', function onData(d) {
      buf = Buffer.concat([buf, d]);
      if (stage === 0) {
        if (buf.length < 2) return;
        if (buf[1] !== 0) return fail(new Error('the relay refused'));
        buf = buf.subarray(2); stage = 1;
        const h = Buffer.from(host, 'utf8');
        const req = Buffer.concat([Buffer.from([5, 1, 0, 3, h.length]), h, Buffer.from([port >> 8, port & 255])]);
        s.write(req);
      }
      if (stage === 1) {
        if (buf.length < 10) return;
        if (buf[1] !== 0) return fail(new Error(`the relay could not connect (${buf[1]})`));
        clearTimeout(t);
        s.removeListener('data', onData);
        s.removeListener('error', fail);
        const rest = buf.subarray(buf[3] === 4 ? 22 : buf[3] === 3 ? 7 + buf[4] : 10);
        if (rest.length) s.unshift(rest);
        resolve(s);
      }
    });
  });
}

// ─── the engine ───

/** user = key[@network][#slot] */
export function parseLogin(user) {
  const m = /^([a-z0-9]+)(?:@([a-z0-9_-]+))?(?:#(\d{1,2}))?$/i.exec(String(user || ''));
  return m ? { keyId: m[1], network: m[2] || '', slot: m[3] ? Number(m[3]) : 0 } : null;
}

export function inCidr(ip, cidr) {
  const [base, bits] = String(cidr).split('/');
  if (net.isIPv4(ip) && net.isIPv4(base)) {
    const n = (a) => a.split('.').reduce((x, y) => (x << 8) + Number(y), 0) >>> 0;
    const b = Number(bits ?? 32);
    const mask = b === 0 ? 0 : (~0 << (32 - b)) >>> 0;
    return ((n(ip) & mask) >>> 0) === ((n(base) & mask) >>> 0);
  }
  return ip === base;
}

export function createHomepool({ store, checkLogin, allowPrivateTargets = false, basePort = 7480, blockPortMin, blockPortMax, bindHosts, dockerSubnets, now = () => Date.now(), log = (...a) => console.log('[homepool]', ...a), notify = () => {} } = {}) {
  const ev = new EventEmitter();
  const loginOk = checkLogin || ((id, pass) => store.keyBySecret(id, pass));
  const links = new Map();           // nodeId → NodeLink
  const active = new Map();          // keyId → Set<socket>
  const bindings = new Map();        // `${keyId}:${slot}` → { exit, boundAt }
  const listeners = new Map();       // `${host}:${port}` → net.Server
  const blockFor = new Map();        // port → { keyId, slot, status }
  let pool = { up: false, liveCount: 0, checkedAt: 0, error: '', host: '' };
  const minBlock = blockPortMin ?? basePort + 10;
  const maxBlock = blockPortMax ?? basePort + 99;
  let timers = [];
  let subnetsCache = { at: 0, map: {} };

  for (const [k, b] of Object.entries(store.data.bindings || {})) bindings.set(k, b);
  const saveBindings = () => { store.data.bindings = Object.fromEntries(bindings); store.save(); };
  const changed = () => ev.emit('change');

  // ── nodes ──
  function attachNode(ws, node) {
    const prev = links.get(node.id);
    if (prev) { try { prev.ws.send(JSON.stringify({ type: 'bye', reason: 'replaced by a newer connection' })); prev.ws.close(); } catch { /* gone */ } prev.close('replaced'); }
    const link = new NodeLink(ws, node);
    links.set(node.id, link);
    store.seenNode(node.id);
    log(`node "${node.name}" (${node.id}) joined${node.network ? ` network ${node.network}` : ' (no network yet)'}`);
    store.log('node-up', { node: node.id });
    ws.send(JSON.stringify({ type: 'hello', node: { id: node.id, name: node.name, network: node.network, networkName: networkName(node.network) }, bindTtlMs: store.settings().bindTtlMs }));
    ws.on('message', (raw, isBinary) => {
      if (isBinary) return link.handle(raw);
      let m; try { m = JSON.parse(String(raw)); } catch { return; }
      if (m.type === 'info') {
        const wasPaused = link.paused;
        link.info = sanitizeInfo(m);
        store.seenNode(node.id, { platform: link.info.platform });
        if (link.paused && !wasPaused) releaseExit(`node:${node.id}`);
        changed();
      }
    });
    ws.on('pong', () => { link.lastPong = now(); });
    const ping = setInterval(() => {
      if (now() - link.lastPong > PING_MS * 2.5) { try { ws.terminate(); } catch { /* gone */ } return; }
      try { ws.ping(); } catch { /* gone */ }
      sendStats(link);
    }, PING_MS);
    const stats = setInterval(() => sendStats(link), 10000);
    ws.on('close', () => {
      clearInterval(ping); clearInterval(stats);
      link.close('node gone');
      if (links.get(node.id) === link) {
        links.delete(node.id);
        releaseExit(`node:${node.id}`);
        log(`node "${node.name}" (${node.id}) left`);
        store.log('node-down', { node: node.id });
        changed();
      }
    });
    changed();
  }

  function sanitizeInfo(m) {
    const s = (v, n = 80) => (v == null ? '' : String(v).slice(0, n));
    return {
      platform: s(m.platform, 40), app: s(m.app, 40), version: s(m.version, 30), network: s(m.network, 40),
      publicIp: s(m.publicIp, 60), paused: !!m.paused, pauseReason: s(m.pauseReason, 200),
      bytesToday: Number(m.bytesToday) || 0, mobileBytesToday: Number(m.mobileBytesToday) || 0, mobileCapMb: Number(m.mobileCapMb) || 0,
    };
  }

  function sendStats(link) {
    const id = `node:${link.node.id}`;
    const slots = [...bindings.entries()].filter(([, b]) => b.exit === id).map(([k]) => k);
    const keyIds = [...new Set(slots.map(k => k.split(':')[0]))];
    const usage = store.usage().nodes[link.node.id] || { bytes: 0, conns: 0 };
    link.send(JSON.stringify({
      type: 'stats', conns: link.streams.size, slots: slots.length, bytesToday: usage.bytes, connsToday: usage.conns,
      network: link.node.network, networkName: networkName(link.node.network),
      keys: keyIds.map(k => store.keys().find(x => x.id === k)?.name || k),
    }));
  }

  /** A node's record changed (moved, switched off, removed, reissued): apply it. */
  function nodeChanged(id, { kick = false } = {}) {
    const link = links.get(id);
    const rec = store.nodes().find(n => n.id === id);
    if (link && (kick || !rec || rec.enabled === false)) {
      try { link.ws.send(JSON.stringify({ type: 'bye', reason: rec ? 'switched off by the owner' : 'removed by the owner', final: !rec || kick })); link.ws.close(4001); } catch { /* gone */ }
    } else if (link && rec) {
      link.node = rec;
      releaseExit(`node:${id}`);
      try { link.ws.send(JSON.stringify({ type: 'hello', node: { id: rec.id, name: rec.name, network: rec.network, networkName: networkName(rec.network) }, bindTtlMs: store.settings().bindTtlMs })); } catch { /* gone */ }
    }
    changed();
  }

  function networkName(id) {
    if (id === SERVER_NETWORK) return 'Server';
    if (id === POOL_NETWORK) return 'Standalone homepool';
    return store.networks().find(n => n.id === id)?.name || id || '';
  }

  // ── exits ──
  /** The exits a network has right now. */
  function exitsOf(network) {
    if (network === SERVER_NETWORK) return [{ id: 'server', kind: 'server', network }];
    if (network === POOL_NETWORK) return pool.up && store.settings().poolBridge.enabled ? [{ id: 'pool', kind: 'pool', network }] : [];
    const out = [];
    for (const link of links.values()) {
      if (link.node.network === network && link.node.enabled !== false && !link.paused) out.push({ id: `node:${link.node.id}`, kind: 'node', network, link });
    }
    return out;
  }
  function exitById(id) {
    if (id === 'server') return { id, kind: 'server', network: SERVER_NETWORK };
    if (id === 'pool') return exitsOf(POOL_NETWORK)[0] || null;
    const link = links.get(id.replace(/^node:/, ''));
    return link && !link.paused && link.node.enabled !== false ? { id, kind: 'node', network: link.node.network, link } : null;
  }
  function loadOf(exitId) {
    let n = 0;
    for (const b of bindings.values()) if (b.exit === exitId) n++;
    return n;
  }
  function releaseExit(exitId) {
    let any = false;
    for (const [k, b] of bindings) if (b.exit === exitId) { bindings.delete(k); any = true; }
    if (any) saveBindings();
  }

  /**
   * Which exit should key/slot use? Returns { exit } or { refusal }.
   * pin: one of the key's networks, to use that one only.
   */
  function choose(key, slot = 0, pin = '') {
    const refusal = store.keyRefusal(key);
    if (refusal) return { refusal };
    let nets = key.networks;
    if (pin) {
      if (!nets.includes(pin)) return { refusal: `this key has no access to network "${pin}"` };
      nets = [pin];
    }
    const live = nets.flatMap(exitsOf);
    const bk = `${key.id}:${slot}${pin ? '@' + pin : ''}`;
    const ttl = store.settings().bindTtlMs;
    const b = bindings.get(bk);
    if (live.length) {
      if (b) {
        const cur = live.find(e => e.id === b.exit);
        const expired = now() - b.boundAt > ttl;
        if (cur && !(expired && live.length > 1)) return { exit: cur, slotKey: bk };
        if (cur && expired && live.length > 1) {
          const next = live.filter(e => e.id !== b.exit).sort((x, y) => loadOf(x.id) - loadOf(y.id))[0];
          bindings.set(bk, { exit: next.id, boundAt: now() });
          saveBindings();
          log(`${key.name}: slot ${slot} rotated ${b.exit} → ${next.id}`);
          return { exit: next, slotKey: bk };
        }
      }
      const pick = live.slice().sort((x, y) => loadOf(x.id) - loadOf(y.id))[0];
      bindings.set(bk, { exit: pick.id, boundAt: now() });
      saveBindings();
      return { exit: pick, slotKey: bk };
    }
    if (key.fallbackToServer) return { exit: { id: 'server', kind: 'server', network: SERVER_NETWORK, fallback: true }, slotKey: bk };
    return { refusal: nets.length === 1 && nets[0] !== SERVER_NETWORK ? `no node of network "${networkName(nets[0])}" is up` : 'no exit of this key\'s networks is up' };
  }

  async function dial(exit, host, port, slot) {
    if (exit.kind === 'node') return exit.link.open(host, port);
    if (exit.kind === 'pool') {
      const pb = store.settings().poolBridge;
      if (!pool.host) throw new Error('the standalone relay\'s address is unknown');
      return socksConnect(pool.host, Number(pb.relayPort || 1080) + (slot || 0), host, port);
    }
    // The server's own connection — never onto this machine or its private
    // networks: a key for "the internet from here" is not a key to the
    // server's own services.
    if (!allowPrivateTargets && isPrivateTarget(host)) throw new Error('private address refused');
    let ip = host;
    if (!net.isIP(host)) {
      const r = await dns.lookup(host);
      ip = r.address;
      if (!allowPrivateTargets && isPrivateTarget(ip)) throw new Error('private address refused');
    }
    return new Promise((resolve, reject) => {
      const s = net.connect({ host: ip, port });
      const t = setTimeout(() => { s.destroy(); reject(new Error('connect timed out')); }, OPEN_TIMEOUT_MS);
      s.once('connect', () => { clearTimeout(t); s.removeAllListeners('error'); resolve(s); });
      s.once('error', (e) => { clearTimeout(t); reject(e); });
    });
  }

  /** Open host:port for a key. Resolves { upstream, exit } or throws with .refusal. */
  async function open(key, { slot = 0, network = '', host, port, via = '' }) {
    const c = choose(key, slot, network);
    if (c.refusal) {
      store.addUsage({ keyId: key.id, refused: true });
      const e = new Error(c.refusal); e.refusal = true; throw e;
    }
    let upstream;
    try { upstream = await dial(c.exit, host, port, slot); }
    catch (err) {
      // A node that died between choosing and dialling: drop its slot so the
      // next try picks again, and say so.
      if (c.exit.kind !== 'server') bindings.delete(c.slotKey);
      throw err;
    }
    store.addUsage({ keyId: key.id, nodeId: c.exit.kind === 'node' ? c.exit.link.node.id : '', conn: true });
    return { upstream, exit: c.exit, via };
  }

  /** Pipe client <-> upstream, count bytes against key and node, cut on revoke. */
  function splice(key, client, upstream, exit) {
    const set = active.get(key.id) || new Set();
    active.set(key.id, set);
    set.add(client);
    const nodeId = exit.kind === 'node' ? exit.link.node.id : '';
    let pending = 0;
    const flush = () => { if (pending) { store.addUsage({ keyId: key.id, nodeId, bytes: pending }); pending = 0; } };
    const flushT = setInterval(() => {
      flush();
      // A cap reached or a key switched off mid-connection ends it.
      const cur = store.keys().find(k => k.id === key.id);
      if (store.keyRefusal(cur)) { client.destroy(); upstream.destroy(); }
    }, 5000);
    const count = (d) => { pending += d.length; };
    client.on('data', count); upstream.on('data', count);
    client.pipe(upstream); upstream.pipe(client);
    const end = () => { clearInterval(flushT); flush(); set.delete(client); client.destroy(); upstream.destroy(); };
    client.once('close', end); upstream.once('close', end);
    client.on('error', () => {}); upstream.on('error', () => {});
  }

  /** The key was changed, switched off or removed: open connections it no longer allows end now. */
  function keyChanged(id) {
    const rec = store.keys().find(k => k.id === id);
    if (!rec || store.keyRefusal(rec)) {
      for (const s of active.get(id) || []) s.destroy();
      active.delete(id);
    }
    for (const k of [...bindings.keys()]) if (k.startsWith(`${id}:`)) bindings.delete(k);
    saveBindings();
    syncBlocks();
    changed();
  }

  // ── what a key's software reads before it starts (same fields as the standalone relay) ──
  function statusFor(key, pin = '') {
    const nets = pin ? key.networks.filter(n => n === pin) : key.networks;
    const live = nets.flatMap(exitsOf);
    const refusal = store.keyRefusal(key);
    const homeLive = live.filter(e => e.kind !== 'server');
    return {
      key: key.name,
      networks: nets.map(n => ({ id: n, name: networkName(n), live: exitsOf(n).length })),
      nodes: live.map(e => ({ id: e.id, name: e.kind === 'node' ? e.link.node.name : e.kind, network: e.network, up: true,
        conns: e.kind === 'node' ? e.link.streams.size : undefined })),
      // Counted as the standalone relay counts: home lines. The pool is as
      // many as it says it has; the server counts only when it is all there is.
      liveCount: homeLive.reduce((n, e) => n + (e.kind === 'pool' ? (pool.liveCount || 1) : 1), 0) || live.length,
      slots: [...bindings.entries()].filter(([k]) => k.startsWith(`${key.id}:`)).map(([k, b]) => ({ slot: k.split(':')[1], node: b.exit,
        boundAt: new Date(b.boundAt).toISOString(), rotatesInMs: Math.max(0, store.settings().bindTtlMs - (now() - b.boundAt)) })),
      egressAvailable: !refusal && (live.length > 0 || !!key.fallbackToServer),
      allowFallback: !!key.fallbackToServer,
      fallbackInUse: !refusal && live.length === 0 && !!key.fallbackToServer,
      refusal: refusal || (live.length ? '' : (key.fallbackToServer ? '' : 'no exit is up')),
      bindTtlMs: store.settings().bindTtlMs,
    };
  }

  // ── listeners ──
  function localAddresses() {
    if (bindHosts) return bindHosts;
    const out = ['127.0.0.1'];
    for (const [name, addrs] of Object.entries(os.networkInterfaces())) {
      if (!/^(docker\d*|br-[0-9a-f]+)$/.test(name)) continue;
      for (const a of addrs || []) if (a.family === 'IPv4' && !a.internal) out.push(a.address);
    }
    return out;
  }

  async function subnets() {
    if (dockerSubnets) return dockerSubnets;
    if (now() - subnetsCache.at < 60000) return subnetsCache.map;
    const map = {};
    try {
      const ids = (await run('docker', ['network', 'ls', '-q'])).split('\n').filter(Boolean);
      if (ids.length) {
        const out = await run('docker', ['network', 'inspect', '-f', '{{.Name}} {{range .IPAM.Config}}{{.Subnet}} {{end}}', ...ids]);
        for (const line of out.split('\n')) { const [name, ...s] = line.trim().split(/\s+/); if (name) map[name] = s.filter(Boolean); }
      }
    } catch { /* no docker: only loopback and CIDRs */ }
    subnetsCache = { at: now(), map };
    return map;
  }

  async function sourceAllowed(key, remote) {
    const ip = String(remote || '').replace(/^::ffff:/, '');
    const nets = await subnets();
    for (const a of key.allowFrom || []) {
      if (a === 'loopback' && (ip === '127.0.0.1' || ip === '::1')) return true;
      if (a.includes('/') && inCidr(ip, a)) return true;
      if (net.isIP(a) && ip === a) return true;
      for (const c of nets[a] || []) if (inCidr(ip, c)) return true;
    }
    return false;
  }

  function listen(port, onConn) {
    for (const host of localAddresses()) {
      const k = `${host}:${port}`;
      if (listeners.has(k)) continue;
      const srv = net.createServer(onConn);
      srv.on('error', (err) => { log(`could not listen on ${k}: ${err.code || err.message}`); listeners.delete(k); });
      srv.listen(port, host);
      listeners.set(k, srv);
    }
  }
  function unlisten(port) {
    for (const [k, srv] of listeners) if (k.endsWith(`:${port}`)) { srv.close(); listeners.delete(k); }
  }

  async function handleSocks(client, { block } = {}) {
    client.on('error', () => {});
    let key, slot = 0, network = '';
    try {
      const hs = await socksAccept(client, { needAuth: !block });
      if (block) {
        key = store.keys().find(k => k.id === block.keyId);
        if (!key || !key.portBlock || !(await sourceAllowed(key, client.remoteAddress))) { client.destroy(); return; }
        slot = block.slot;
      } else {
        const login = parseLogin(hs.user);
        key = login && loginOk(login.keyId, hs.pass);
        if (!key) { store.log('refused-login', { user: String(hs.user).slice(0, 40), from: client.remoteAddress }); hs.deny(); return; }
        slot = login.slot; network = login.network;
      }
      const req = await hs.accept();
      try {
        const { upstream, exit } = await open(key, { slot, network, host: req.host, port: req.port });
        req.reply(true);
        if (req.extra?.length) upstream.write(req.extra);
        splice(key, client, upstream, exit);
      } catch (err) {
        req.reply(false, err.refusal ? 2 : 5);
        client.end();
      }
    } catch { client.destroy(); }
  }

  function handleHttp(client) {
    client.on('error', () => {});
    // One http.Server parses this socket's requests (CONNECT and absolute-form).
    const srv = http.createServer();
    srv.on('connect', async (req, sock, head) => {
      const key = httpKey(req);
      if (!key) { sock.end('HTTP/1.1 407 Proxy Authentication Required\r\nProxy-Authenticate: Basic realm="homepool"\r\n\r\n'); return; }
      const [host, p] = splitHostPort(req.url);
      try {
        const { upstream, exit } = await open(key.rec, { slot: key.slot, network: key.network, host, port: p || 443 });
        sock.write('HTTP/1.1 200 Connection Established\r\n\r\n');
        if (head?.length) upstream.write(head);
        splice(key.rec, sock, upstream, exit);
      } catch (err) { sock.end(`HTTP/1.1 ${err.refusal ? 403 : 502} ${err.refusal ? 'Forbidden' : 'Bad Gateway'}\r\n\r\n${err.message}`); }
    });
    srv.on('request', async (req, res) => {
      const key = httpKey(req);
      if (!key) { res.writeHead(407, { 'Proxy-Authenticate': 'Basic realm="homepool"' }); return res.end(); }
      if (req.url.startsWith('/')) {
        if (req.url.startsWith('/status')) { res.writeHead(200, { 'Content-Type': 'application/json' }); return res.end(JSON.stringify(statusFor(key.rec, key.network), null, 2)); }
        res.writeHead(404); return res.end();
      }
      let u; try { u = new URL(req.url); } catch { res.writeHead(400); return res.end(); }
      if (u.protocol !== 'http:') { res.writeHead(400); return res.end('use CONNECT for https'); }
      try {
        const { upstream, exit } = await open(key.rec, { slot: key.slot, network: key.network, host: u.hostname, port: Number(u.port) || 80 });
        const headers = { ...req.headers }; delete headers['proxy-authorization']; delete headers['proxy-connection'];
        headers.connection = 'close';
        const preq = http.request({ method: req.method, path: u.pathname + u.search, headers, createConnection: () => upstream });
        let n = 0;
        const nodeId = exit.kind === 'node' ? exit.link.node.id : '';
        preq.on('response', (pres) => {
          res.writeHead(pres.statusCode, pres.headers);
          pres.on('data', (d) => { n += d.length; });
          pres.pipe(res);
          pres.on('end', () => store.addUsage({ keyId: key.rec.id, nodeId, bytes: n }));
        });
        preq.on('error', (e) => { if (!res.headersSent) { res.writeHead(502); res.end(e.message); } else res.destroy(); });
        req.pipe(preq);
      } catch (err) { res.writeHead(err.refusal ? 403 : 502); res.end(err.message); }
    });
    srv.emit('connection', client);
  }

  function httpKey(req) {
    const h = req.headers['proxy-authorization'] || '';
    const m = /^Basic\s+(.+)$/i.exec(h);
    if (!m) return null;
    const [user, ...rest] = Buffer.from(m[1], 'base64').toString('utf8').split(':');
    const login = parseLogin(user);
    const rec = login && loginOk(login.keyId, rest.join(':'));
    return rec ? { rec, slot: login.slot, network: login.network } : null;
  }

  function handleBlockStatus(client, keyId) {
    client.on('error', () => {});
    const srv = http.createServer(async (req, res) => {
      const key = store.keys().find(k => k.id === keyId);
      if (!key || !key.portBlock || !(await sourceAllowed(key, client.remoteAddress))) { res.writeHead(403); return res.end(); }
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify(statusFor(key), null, 2));
    });
    srv.emit('connection', client);
  }

  /** Make the port blocks match the keys that ask for one. */
  function syncBlocks() {
    const want = store.keys().filter(k => k.portBlock);
    const taken = new Set();
    for (const k of want) if (k.ports && k.ports.base) for (let p = k.ports.base; p <= k.ports.status; p++) taken.add(p);
    for (const k of want) {
      const size = (k.slots || 0) + 2;
      if (!k.ports || k.ports.status - k.ports.base + 1 !== size) {
        let base = null;
        for (let p = minBlock; p + size - 1 <= maxBlock; p++) {
          let free = true;
          for (let q = p; q < p + size; q++) if (taken.has(q)) { free = false; p = q; break; }
          if (free) { base = p; break; }
        }
        if (base == null) { log(`no room for a port block for key ${k.name}`); continue; }
        if (k.ports) for (let p = k.ports.base; p <= k.ports.status; p++) taken.delete(p);
        store.setKeyPorts(k.id, { base, status: base + size - 1 });
        for (let p = base; p < base + size; p++) taken.add(p);
      }
    }
    const wanted = new Map();
    for (const k of store.keys().filter(x => x.portBlock && x.ports)) {
      for (let s = 0; s <= (k.slots || 0); s++) wanted.set(k.ports.base + s, { keyId: k.id, slot: s });
      wanted.set(k.ports.status, { keyId: k.id, status: true });
    }
    for (const port of [...blockFor.keys()]) {
      const w = wanted.get(port), cur = blockFor.get(port);
      if (!w || w.keyId !== cur.keyId || w.slot !== cur.slot || !!w.status !== !!cur.status) { unlisten(port); blockFor.delete(port); }
    }
    for (const [port, w] of wanted) {
      blockFor.set(port, w);
      listen(port, w.status ? (c) => handleBlockStatus(c, w.keyId) : (c) => handleSocks(c, { block: blockFor.get(port) || w }));
    }
  }

  // ── the standalone relay, as a network ──
  async function pollPool() {
    const pb = store.settings().poolBridge;
    if (!pb.enabled) { if (pool.up) { pool = { ...pool, up: false }; releaseExit('pool'); changed(); } return; }
    try {
      const st = await getJson(pb.statusUrl, 4000);
      let host = pb.relayHost;
      if (!host) {
        try { host = (await run('docker', ['inspect', '-f', '{{range .NetworkSettings.Networks}}{{.IPAddress}} {{end}}', 'homepool-relay'])).trim().split(/\s+/)[0] || ''; }
        catch { host = ''; }
      }
      const up = !!host && (st.liveCount > 0);
      if (up !== pool.up) { log(`standalone homepool ${up ? 'up' : 'down'} (${st.liveCount} node(s))`); if (!up) releaseExit('pool'); }
      pool = { up, liveCount: st.liveCount || 0, nodes: (st.nodes || []).filter(n => n.up).map(n => ({ id: n.id, name: n.name })), checkedAt: now(), error: '', host };
    } catch (err) {
      if (pool.up) releaseExit('pool');
      pool = { up: false, liveCount: 0, nodes: [], checkedAt: now(), error: err.message, host: '' };
    }
    changed();
  }

  // ── for the owner's screens ──
  function overview() {
    const usage = store.usage();
    const nodeView = (n) => {
      const link = links.get(n.id);
      return {
        ...publicNode(n), up: !!link, paused: link ? link.paused : false, info: link ? link.info : null,
        connectedAt: link ? link.connectedAt : 0, streams: link ? link.streams.size : 0,
        slots: loadOf(`node:${n.id}`), usage: usage.nodes[n.id] || { bytes: 0, conns: 0 },
      };
    };
    return {
      ports: { socks: basePort, http: basePort + 1, blocks: [minBlock, maxBlock], hosts: localAddresses() },
      networks: [
        { id: SERVER_NETWORK, name: 'Server', builtin: true, live: 1 },
        { id: POOL_NETWORK, name: 'Standalone homepool', builtin: true, live: pool.up ? pool.liveCount : 0, pool, enabled: store.settings().poolBridge.enabled },
        ...store.networks().map(n => ({ ...n, live: exitsOf(n.id).length })),
      ],
      nodes: store.nodes().map(nodeView),
      keys: store.keys().map(k => ({ ...publicKey(k), usage: usage.keys[k.id] || { bytes: 0, conns: 0, refused: 0, lastUsedAt: 0 },
        refusal: store.keyRefusal(k), open: active.get(k.id)?.size || 0, status: statusFor(k) })),
      requests: store.requests().slice().reverse(),
      settings: store.settings(),
      log: store.recentLog(80).reverse(),
      usageDay: usage.day,
    };
  }

  function start() {
    listen(basePort, (c) => handleSocks(c));
    listen(basePort + 1, (c) => handleHttp(c));
    syncBlocks();
    pollPool();
    // Docker networks come and go; a listener for a new bridge is added late.
    timers.push(setInterval(() => { listen(basePort, (c) => handleSocks(c)); listen(basePort + 1, (c) => handleHttp(c)); syncBlocks(); }, 60000));
    timers.push(setInterval(pollPool, POOL_POLL_MS));
    for (const t of timers) t.unref?.();
    log(`SOCKS5 on :${basePort}, HTTP on :${basePort + 1} (${localAddresses().join(', ')})`);
  }
  function stop() {
    for (const t of timers) clearInterval(t);
    timers = [];
    for (const srv of listeners.values()) srv.close();
    listeners.clear();
    for (const l of links.values()) { try { l.ws.close(); } catch { /* gone */ } }
  }

  return {
    on: (e, f) => ev.on(e, f), off: (e, f) => ev.off(e, f),
    start, stop, attachNode, nodeChanged, keyChanged, open, splice, choose, statusFor, overview, networkName,
    pollPool, syncBlocks, notify,
    links, get pool() { return pool; },
  };
}

function splitHostPort(s) {
  const m = /^\[([^\]]+)\]:(\d+)$/.exec(s) || /^([^:]+):(\d+)$/.exec(s);
  return m ? [m[1], Number(m[2])] : [s, 0];
}

function run(cmd, args) {
  return new Promise((resolve, reject) => execFile(cmd, args, { timeout: 8000 }, (err, out) => err ? reject(err) : resolve(String(out))));
}

function getJson(url, timeoutMs) {
  return new Promise((resolve, reject) => {
    const req = http.get(url, { timeout: timeoutMs }, (res) => {
      let b = ''; res.setEncoding('utf8');
      res.on('data', d => { b += d; });
      res.on('end', () => { try { resolve(JSON.parse(b)); } catch { reject(new Error('unparseable status')); } });
    });
    req.on('timeout', () => req.destroy(new Error('timed out')));
    req.on('error', reject);
  });
}
