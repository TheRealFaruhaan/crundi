/**
 * net-tunnel.js — the server end of a browser tab's "server network".
 *
 * A desktop app on another machine can give a browser tab this server's
 * network: localhost dev servers, internal hosts, whatever this machine can
 * reach. The app runs a proxy on its own loopback; every connection the tab
 * makes becomes a stream carried over the app's existing, already
 * authenticated native-host websocket, and this module opens the matching TCP
 * connection here. No port is opened on this server.
 *
 * Frames are binary websocket messages: [type u8][stream id u32 BE][payload].
 *   OPEN   1  client → server  payload "host:port" (utf8, IPv6 in brackets)
 *   DATA   2  both ways        payload bytes
 *   CLOSE  3  both ways        no payload (or a reason, ignored)
 *   OPENED 4  server → client  the TCP connection is up
 *   ERROR  5  server → client  payload: what went wrong (utf8); a CLOSE follows
 *
 * The same codec is duplicated in app/net-tunnel-client.js (a client-only
 * install ships no src/); scripts/test-net-tunnel.mjs runs the two together.
 */

import net from 'node:net';

export const T = { OPEN: 1, DATA: 2, CLOSE: 3, OPENED: 4, ERROR: 5 };

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

/** "host:port" / "[::1]:port" → { host, port } or null. */
export function parseTarget(s) {
  const m = /^\[([0-9a-fA-F:.]+)\]:(\d{1,5})$/.exec(s) || /^([^\s:[\]/]+):(\d{1,5})$/.exec(s);
  if (!m) return null;
  const port = Number(m[2]);
  if (!port || port > 65535) return null;
  return { host: m[1], port };
}

const MAX_STREAMS = 512;
const HIGH_WATER = 4 * 1024 * 1024;   // pause reading once the websocket holds this much

/**
 * One endpoint per attached desktop app.
 *   send(buf)    write a binary frame to that app's websocket
 *   buffered()   bytes still queued on that websocket (for back-pressure)
 *   connect      injectable for tests; defaults to net.connect
 */
export function createTunnelEndpoint({ send, buffered = () => 0, connect = (o) => net.connect(o), log = () => {} } = {}) {
  const streams = new Map();   // id → socket
  let drainTimer = null;
  const paused = new Set();

  function out(type, id, payload) { try { send(frame(type, id, payload)); } catch { /* socket gone */ } }

  function watchDrain() {
    if (drainTimer) return;
    drainTimer = setInterval(() => {
      if (buffered() > HIGH_WATER / 2) return;
      for (const s of paused) { try { s.resume(); } catch { /* gone */ } }
      paused.clear();
      clearInterval(drainTimer); drainTimer = null;
    }, 30);
    drainTimer.unref?.();
  }

  function drop(id) {
    const s = streams.get(id);
    if (!s) return;
    streams.delete(id);
    paused.delete(s);
    try { s.destroy(); } catch { /* gone */ }
  }

  function open(id, payload) {
    if (streams.has(id)) { out(T.ERROR, id, 'stream id in use'); out(T.CLOSE, id); return; }
    if (streams.size >= MAX_STREAMS) { out(T.ERROR, id, 'too many connections'); out(T.CLOSE, id); return; }
    const target = parseTarget(payload.toString('utf8'));
    if (!target) { out(T.ERROR, id, 'bad target'); out(T.CLOSE, id); return; }
    let sock;
    try { sock = connect({ host: target.host, port: target.port }); } catch (err) {
      out(T.ERROR, id, err.message); out(T.CLOSE, id); return;
    }
    streams.set(id, sock);
    let closed = false;
    sock.setNoDelay?.(true);
    sock.on('connect', () => out(T.OPENED, id));
    sock.on('data', (d) => {
      out(T.DATA, id, d);
      if (buffered() > HIGH_WATER) { sock.pause(); paused.add(sock); watchDrain(); }
    });
    sock.on('error', (err) => { if (!closed) out(T.ERROR, id, err.code || err.message); });
    sock.on('close', () => {
      if (closed) return;
      closed = true;
      if (streams.get(id) === sock) { streams.delete(id); paused.delete(sock); out(T.CLOSE, id); }
    });
  }

  function handle(buf) {
    const f = parseFrame(buf);
    if (!f) return;
    if (f.type === T.OPEN) return open(f.id, f.payload);
    const s = streams.get(f.id);
    if (f.type === T.DATA) { if (s) s.write(f.payload); return; }
    if (f.type === T.CLOSE) { if (s) { streams.delete(f.id); paused.delete(s); try { s.end(); } catch { /* gone */ } setTimeout(() => { try { s.destroy(); } catch { /* gone */ } }, 2000).unref?.(); } }
  }

  function closeAll() {
    for (const id of [...streams.keys()]) drop(id);
    if (drainTimer) { clearInterval(drainTimer); drainTimer = null; }
    log(`[net-tunnel] closed all streams`);
  }

  return { handle, closeAll, size: () => streams.size };
}
