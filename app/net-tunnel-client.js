/**
 * net-tunnel-client.js — the desktop app's end of a browser tab's "server network".
 *
 * A drawer tab set to the server's network uses its own session, whose proxy
 * is this: an HTTP proxy on 127.0.0.1 (a random port, never another
 * interface) that requires a per-launch password. Every connection the tab
 * makes — CONNECT for https and websockets, absolute-form requests for plain
 * http — becomes one stream over the app's native-host websocket to the
 * server, which opens the TCP connection on its side (src/net-tunnel.js).
 * So the tab reaches the server's localhost and internal hosts, the server
 * opens no port, and nothing new is exposed on this machine either: the
 * listener is loopback-only and refuses anyone without the password.
 *
 * The frame codec mirrors src/net-tunnel.js; scripts/test-net-tunnel.mjs runs
 * the two together.
 */

import net from 'node:net';
import { randomBytes } from 'node:crypto';

export const T = { OPEN: 1, DATA: 2, CLOSE: 3, OPENED: 4, ERROR: 5 };

export function frame(type, id, payload) {
  const p = payload == null ? Buffer.alloc(0) : (Buffer.isBuffer(payload) ? payload : Buffer.from(String(payload), 'utf8'));
  const b = Buffer.allocUnsafe(5 + p.length);
  b[0] = type;
  b.writeUInt32BE(id >>> 0, 1);
  p.copy(b, 5);
  return b;
}

/** Which proxy (idBase) a frame belongs to. */
export function idRange(buf) { return buf && buf.length >= 5 ? (buf[1] & 0xff) : -1; }

export function parseFrame(buf) {
  if (!buf) return null;
  if (!Buffer.isBuffer(buf)) buf = Buffer.from(buf);
  if (buf.length < 5) return null;
  return { type: buf[0], id: buf.readUInt32BE(1), payload: buf.subarray(5) };
}

const MAX_HEAD = 64 * 1024;
const OPEN_TIMEOUT_MS = 20000;

function page(status, reason, text, extraHeaders = '', heading = 'Server network', title = '') {
  const body = `<!doctype html><meta charset="utf-8"><title>${title || reason}</title>`
    + `<body style="font:14px system-ui;padding:32px;color:#444"><h3 style="margin:0 0 8px">${heading}</h3>${text}</body>`;
  return `HTTP/1.1 ${status} ${reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: ${Buffer.byteLength(body)}\r\nConnection: close\r\n${extraHeaders}\r\n${body}`;
}

/**
 * send(buf)       write a binary frame to the server
 * isConnected()   is the native-host socket up, and does the server tunnel?
 */
/**
 * targetPrefix  put before each OPEN target: "hp|<network>|" makes the server
 *               open the tab's connections through that homepool network
 * heading       the title of the error pages this proxy answers with
 */
export function createServerNetProxy({ send, isConnected = () => true, log = () => {}, targetPrefix = '', heading = 'Server network', idBase = 0 } = {}) {
  const user = 'crundi';
  const pass = randomBytes(18).toString('base64url');
  const expectAuth = 'Basic ' + Buffer.from(`${user}:${pass}`).toString('base64');
  const streams = new Map();   // id → { sock, onOpened, onError }
  // Several proxies can share one websocket: each takes stream ids in its own
  // range (idBase << 24), and the socket's owner routes frames by it (idRange).
  const base = (idBase & 0xff) * 0x1000000;
  let nextId = 1;
  let server = null;
  let port = 0;

  function out(type, id, payload) { try { send(frame(type, id, payload)); return true; } catch { return false; } }

  function openStream(target, sock, onOpened, onError) {
    const id = base + nextId++;
    if (nextId > 0xfffff0) nextId = 1;
    const st = { sock, onOpened, onError, open: false };
    streams.set(id, st);
    const timer = setTimeout(() => { if (!st.open) { streams.delete(id); onError('The server did not answer'); out(T.CLOSE, id); } }, OPEN_TIMEOUT_MS);
    timer.unref?.();
    st.timer = timer;
    if (!out(T.OPEN, id, targetPrefix + target)) { streams.delete(id); clearTimeout(timer); onError('Not connected to the server'); return null; }
    sock.on('data', (d) => { if (st.open) out(T.DATA, id, d); });
    sock.on('close', () => { if (streams.get(id) === st) { streams.delete(id); clearTimeout(timer); out(T.CLOSE, id); } });
    sock.on('error', () => { /* close follows */ });
    return id;
  }

  function handleFrame(buf) {
    const f = parseFrame(buf);
    if (!f) return;
    const st = streams.get(f.id);
    if (!st) return;
    if (f.type === T.OPENED) { st.open = true; clearTimeout(st.timer); st.onOpened(); return; }
    if (f.type === T.DATA) { try { st.sock.write(f.payload); } catch { /* gone */ } return; }
    if (f.type === T.ERROR) { if (!st.open) { st.failed = true; st.onError(f.payload.toString('utf8')); } return; }
    if (f.type === T.CLOSE) {
      streams.delete(f.id); clearTimeout(st.timer);
      if (!st.open && !st.failed) st.onError('The server closed the connection');
      try { st.sock.end(); } catch { /* gone */ }
    }
  }

  /** The websocket went away: every stream on it is dead. */
  function onDisconnect() {
    for (const [id, st] of streams) {
      streams.delete(id); clearTimeout(st.timer);
      if (!st.open) st.onError('Not connected to the server');
      try { st.sock.destroy(); } catch { /* gone */ }
    }
  }

  function onConnection(sock) {
    let head = Buffer.alloc(0);
    const onData = (d) => {
      head = Buffer.concat([head, d]);
      const end = head.indexOf('\r\n\r\n');
      if (end < 0) { if (head.length > MAX_HEAD) sock.destroy(); return; }
      sock.removeListener('data', onData);
      sock.pause();
      const rest = head.subarray(end + 4);
      const lines = head.subarray(0, end).toString('latin1').split('\r\n');
      const [method, target, version] = (lines[0] || '').split(' ');
      const headers = lines.slice(1);
      const auth = headers.find((h) => /^proxy-authorization:/i.test(h));
      if (!auth || auth.slice(auth.indexOf(':') + 1).trim() !== expectAuth) {
        sock.end(page(407, 'Proxy Authentication Required', 'Sign-in needed.', 'Proxy-Authenticate: Basic realm="Crundi server network"\r\n', heading));
        return;
      }
      if (!isConnected()) {
        sock.end(page(502, 'Bad Gateway', 'This tab goes out through the server, and the desktop app is not connected to the server right now.', '', heading));
        return;
      }
      const fail = (why) => {
        const w = String(why).replace(/[<&]/g, '');
        // Homepool with no exit up: say that plainly, it is the usual case.
        const text = /no (node|exit)/i.test(w) ? 'No exit is up: ' + w.replace(/^homepool:\s*/, '') + '. The page loads once a node of this network is online.' : 'The server could not connect: ' + w;
        try { sock.end(page(502, 'Bad Gateway', text, '', heading, /^No exit/.test(text) ? 'No exit is up' : 'Could not connect')); } catch { /* gone */ }
      };
      if (method === 'CONNECT') {
        openStream(target, sock, () => {
          sock.write('HTTP/1.1 200 Connection Established\r\n\r\n');
          if (rest.length) sock.emit('data', rest);
          sock.resume();
        }, fail);
        return;
      }
      let u;
      try { u = new URL(target); } catch { sock.end(page(400, 'Bad Request', 'Bad request.', '', heading)); return; }
      if (u.protocol !== 'http:') { sock.end(page(400, 'Bad Request', 'Only http and https are carried.', '', heading)); return; }
      const hostPort = (u.hostname.includes(':') ? `[${u.hostname}]` : u.hostname) + ':' + (u.port || 80);
      // One request per connection: the proxy pipes bytes, so a kept-alive
      // socket would carry a later request for another host to this one.
      const kept = headers.filter((h) => !/^(proxy-authorization|proxy-connection|connection|keep-alive):/i.test(h));
      const newHead = Buffer.from(`${method} ${u.pathname}${u.search} ${version || 'HTTP/1.1'}\r\n${kept.join('\r\n')}${kept.length ? '\r\n' : ''}Connection: close\r\n\r\n`, 'latin1');
      const id = openStream(hostPort, sock, () => {
        out(T.DATA, id, newHead);
        if (rest.length) out(T.DATA, id, rest);
        sock.resume();
      }, fail);
    };
    sock.on('data', onData);
    sock.on('error', () => { /* ignore */ });
  }

  function start() {
    if (server) return Promise.resolve(port);
    return new Promise((resolve, reject) => {
      const s = net.createServer(onConnection);
      s.once('error', reject);
      s.listen(0, '127.0.0.1', () => { server = s; port = s.address().port; log(`[server-net] proxy on 127.0.0.1:${port}`); resolve(port); });
    });
  }

  function stop() {
    onDisconnect();
    try { server?.close(); } catch { /* ignore */ }
    server = null; port = 0;
  }

  return {
    start, stop, handleFrame, onDisconnect,
    get port() { return port; },
    credentials: { user, pass },
    /** Is this proxy challenge ours? (Electron's 'login' event.) */
    isOurs: (authInfo) => !!(authInfo && authInfo.isProxy && Number(authInfo.port) === port && /^(127\.0\.0\.1|localhost)$/.test(String(authInfo.host))),
    streamCount: () => streams.size,
  };
}
