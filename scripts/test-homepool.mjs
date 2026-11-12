// Homepool end to end, on real sockets: a node joins over a websocket, keys
// decide who may leave through which network, and revoking means now.
// Uses high loopback ports only (17480-17530) and a temporary data folder.
import net from 'node:net';
import http from 'node:http';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { WebSocketServer } from 'ws';
import { createHomepoolStore } from '../src/homepool-store.js';
import { createHomepool, parseLogin, inCidr } from '../src/homepool.js';
import { startNode, nodeCode, parseNodeCode, isPrivateTarget, frame, parseFrame, T } from '../app/homepool-node.js';

let failed = 0;
const check = (name, ok, extra = '') => { console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${ok ? '' : ' ' + extra}`); if (!ok) failed++; };
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
const until = async (f, ms = 4000) => { const t = Date.now(); while (Date.now() - t < ms) { if (await f()) return true; await sleep(30); } return false; };

const BASE = 17480;
const dir = mkdtempSync(join(tmpdir(), 'homepool-test-'));

// A target: echoes with a prefix, and an HTTP server.
const echo = net.createServer((s) => s.on('data', (d) => s.write(Buffer.concat([Buffer.from('echo:'), d])))).listen(17520, '127.0.0.1');
const web = http.createServer((req, res) => res.end(`hello ${req.url}`)).listen(17521, '127.0.0.1');

const store = createHomepoolStore({ dataDir: dir });
const pool = createHomepool({ store, basePort: BASE, bindHosts: ['127.0.0.1'], dockerSubnets: {}, allowPrivateTargets: true, log: () => {} });
store.setSettings({ poolBridge: { enabled: false } });
pool.start();

// The node endpoint, as webapp.js wires it.
const wss = new WebSocketServer({ port: 17519, host: '127.0.0.1', verifyClient: (info, cb) => {
  const key = new URL(info.req.url, 'http://x').searchParams.get('key');
  const node = store.nodeByKey(key);
  if (!node) return cb(false, 401);
  info.req.node = node; cb(true);
} });
wss.on('connection', (ws, req) => pool.attachNode(ws, req.node));

// SOCKS5 client with optional login.
function socks(port, { user, pass, host = '127.0.0.1', dport = 17520 } = {}) {
  return new Promise((resolve) => {
    const s = net.connect(port, '127.0.0.1');
    let buf = Buffer.alloc(0), stage = 0;
    const done = (r) => { resolve(r); };
    s.on('error', () => done({ ok: false, code: 'error' }));
    s.on('close', () => done({ ok: false, code: 'closed', stage }));
    s.on('connect', () => s.write(Buffer.from(user != null ? [5, 1, 2] : [5, 1, 0])));
    s.on('data', (d) => {
      buf = Buffer.concat([buf, d]);
      if (stage === 0 && buf.length >= 2) {
        const m = buf[1]; buf = buf.subarray(2);
        if (m === 0xff) return done({ ok: false, code: 'method' });
        if (m === 2) { const u = Buffer.from(user || ''), p = Buffer.from(pass || ''); s.write(Buffer.concat([Buffer.from([1, u.length]), u, Buffer.from([p.length]), p])); stage = 1; }
        else stage = 2;
      }
      if (stage === 1 && buf.length >= 2) { if (buf[1] !== 0) return done({ ok: false, code: 'auth' }); buf = buf.subarray(2); stage = 2; }
      if (stage === 2) {
        const h = Buffer.from(host); s.write(Buffer.concat([Buffer.from([5, 1, 0, 3, h.length]), h, Buffer.from([dport >> 8, dport & 255])])); stage = 3; return;
      }
      if (stage === 3 && buf.length >= 10) {
        if (buf[1] !== 0) return done({ ok: false, code: `rep${buf[1]}` });
        buf = buf.subarray(10); stage = 4;
        s.write('ping');
      }
      if (stage === 4 && buf.length >= 9) { done({ ok: true, reply: buf.toString(), sock: s }); }
    });
  });
}

// ── pure bits ──
check('parseLogin key', JSON.stringify(parseLogin('kabc')) === JSON.stringify({ keyId: 'kabc', network: '', slot: 0 }));
check('parseLogin key@net#slot', JSON.stringify(parseLogin('kabc@home#3')) === JSON.stringify({ keyId: 'kabc', network: 'home', slot: 3 }));
check('inCidr', inCidr('172.21.0.5', '172.21.0.0/16') && !inCidr('172.22.0.5', '172.21.0.0/16'));
check('private targets', isPrivateTarget('127.0.0.1') && isPrivateTarget('localhost') && isPrivateTarget('192.168.1.1') && isPrivateTarget('10.0.0.1') && !isPrivateTarget('8.8.8.8') && !isPrivateTarget('example.com'));
const code = nodeCode('https://dev.example.com', 'hpn_nabc.secret');
check('node code round trip', parseNodeCode(code)?.key === 'hpn_nabc.secret' && parseNodeCode(code)?.server === 'https://dev.example.com');
check('frame round trip', (() => { const f = parseFrame(frame(T.DATA, 7, 'hi')); return f.type === T.DATA && f.id === 7 && f.payload.toString() === 'hi'; })());

// ── keys ──
const serverKey = store.addKey({ kind: 'software', name: 'srv', networks: ['server'] });
const homeKey = store.addKey({ kind: 'software', name: 'home-only', networks: ['home'] });
pool.keyChanged(homeKey.key.id);

let r = await socks(BASE, { user: 'knope', pass: 'x' });
check('unknown key is refused at login', !r.ok && r.code === 'auth', r.code);
r = await socks(BASE, { user: serverKey.key.id, pass: 'wrong' });
check('wrong secret is refused', !r.ok && r.code === 'auth', r.code);
r = await socks(BASE);
check('no login at all is refused', !r.ok, r.code);
r = await socks(BASE, { user: serverKey.key.id, pass: serverKey.secret });
check('server key leaves via the server', r.ok && r.reply === 'echo:ping', JSON.stringify(r)); r.sock?.destroy();

r = await socks(BASE, { user: homeKey.key.id, pass: homeKey.secret });
check('home key with no node up is refused (not sent out from the server)', !r.ok && r.code === 'rep2', r.code);
check('status says no egress', pool.statusFor(store.keys().find(k => k.id === homeKey.key.id)).egressAvailable === false);

// ── a node joins "home" ──
const n1 = store.addNode({ name: 'phone', network: 'home' });
const states = [];
const node1 = startNode({ server: 'http://127.0.0.1:17519', key: n1.key, allowPrivate: true, info: () => ({ platform: 'test', network: 'Wi-Fi' }), onState: (s) => states.push(s) });
check('node connects', await until(() => pool.links.has(n1.id)));
r = await socks(BASE, { user: homeKey.key.id, pass: homeKey.secret });
check('home key now leaves through the node', r.ok && r.reply === 'echo:ping', JSON.stringify(r));
check('the node carried it', store.usage().nodes[n1.id]?.conns === 1, JSON.stringify(store.usage().nodes));
const st = pool.statusFor(store.keys().find(k => k.id === homeKey.key.id));
check('status is relay-compatible', st.liveCount === 1 && st.egressAvailable === true && st.allowFallback === false && st.fallbackInUse === false, JSON.stringify(st));

// Revoking means now: the open connection ends, and the next is refused.
const open1 = r.sock;
let closed = false; open1.on('close', () => { closed = true; });
store.updateKey(homeKey.key.id, { enabled: false }); pool.keyChanged(homeKey.key.id);
check('switching a key off cuts its open connection', await until(() => closed));
r = await socks(BASE, { user: homeKey.key.id, pass: homeKey.secret });
check('…and refuses the next', !r.ok, r.code);
store.updateKey(homeKey.key.id, { enabled: true }); pool.keyChanged(homeKey.key.id);

// ── networks: the node moved elsewhere is not "home" any more ──
const friend = store.addNetwork("Ali's place");
store.updateNode(n1.id, { network: friend }); pool.nodeChanged(n1.id);
r = await socks(BASE, { user: homeKey.key.id, pass: homeKey.secret });
check('moving the node out of home leaves the home key without an exit', !r.ok && r.code === 'rep2', r.code);

// Two networks, a node in each: slots spread, and stick.
const n2 = store.addNode({ name: 'pc', network: 'home' });
const node2 = startNode({ server: 'http://127.0.0.1:17519', key: n2.key, allowPrivate: true });
check('second node connects', await until(() => pool.links.has(n2.id)));
const both = store.addKey({ kind: 'software', name: 'both', networks: ['home', friend] });
const exits = new Set();
for (let slot = 1; slot <= 4; slot++) exits.add(pool.choose(store.keys().find(k => k.id === both.key.id), slot).exit.id);
check('a key with two networks spreads its slots over both', exits.size === 2, [...exits].join(','));
const first = pool.choose(store.keys().find(k => k.id === both.key.id), 1).exit.id;
check('a slot sticks to its exit', pool.choose(store.keys().find(k => k.id === both.key.id), 1).exit.id === first);
const before = store.usage().nodes[n1.id]?.conns || 0;
r = await socks(BASE, { user: `${both.key.id}@${friend}#2`, pass: both.secret });
check('user@network pins the network', r.ok && (store.usage().nodes[n1.id]?.conns || 0) === before + 1, `${before} -> ${store.usage().nodes[n1.id]?.conns}`); r.sock?.destroy();

// Bulk both ways through a node: 8 MB up, the sink answers with what it got.
{
  const sink = net.createServer((s) => { let n = 0; s.on('data', d => { n += d.length; if (n >= 8 * 1024 * 1024) { const big = Buffer.alloc(8 * 1024 * 1024, 7); s.end(big); } }); }).listen(17522, '127.0.0.1');
  const got = await new Promise((resolve) => {
    const s = net.connect(BASE, '127.0.0.1'); let stage = 0, buf = Buffer.alloc(0), back = 0;
    s.on('connect', () => s.write(Buffer.from([5, 1, 2])));
    s.on('data', (d) => {
      if (stage === 4) { back += d.length; return; }
      buf = Buffer.concat([buf, d]);
      if (stage === 0 && buf.length >= 2) { buf = buf.subarray(2); const u = Buffer.from(`${both.key.id}@${friend}`), p = Buffer.from(both.secret); s.write(Buffer.concat([Buffer.from([1, u.length]), u, Buffer.from([p.length]), p])); stage = 1; }
      if (stage === 1 && buf.length >= 2) { buf = buf.subarray(2); s.write(Buffer.from([5, 1, 0, 1, 127, 0, 0, 1, 17522 >> 8, 17522 & 255])); stage = 2; }
      if (stage === 2 && buf.length >= 10) { stage = 4; buf = Buffer.alloc(0); for (let i = 0; i < 128; i++) s.write(Buffer.alloc(65536, 1)); }
    });
    s.on('close', () => resolve(back)); s.on('error', () => resolve(back));
    setTimeout(() => { s.destroy(); }, 20000);
  });
  sink.close();
  check('8 MB each way through a node arrives whole', got === 8 * 1024 * 1024, String(got));
}
r = await socks(BASE, { user: `${homeKey.key.id}@${friend}`, pass: homeKey.secret });
check('a key cannot pin a network it was not given', !r.ok, r.code);

// A node that leaves releases its slots.
node2.stop();
await until(() => !pool.links.has(n2.id));
check('a node that leaves releases its slots', [1, 2, 3, 4].every(s => pool.choose(store.keys().find(k => k.id === both.key.id), s).exit.id === `node:${n1.id}`));

// Removing a node's record ends its connection for good.
store.removeNode(n1.id); pool.nodeChanged(n1.id);
check('a removed node is disconnected and stays off', await until(() => !pool.links.has(n1.id) && states.includes('refused'), 3000), states.join(','));
const n1b = startNode({ server: 'http://127.0.0.1:17519', key: n1.key });
const refusedAgain = await until(() => n1b.status().state === 'refused', 3000);
check('its old key is refused at the door', refusedAgain, n1b.status().state);
n1b.stop();

// ── fallback, caps, expiry ──
const fb = store.addKey({ kind: 'software', name: 'fb', networks: ['home'], fallbackToServer: true });
r = await socks(BASE, { user: fb.key.id, pass: fb.secret });
check('fallbackToServer leaves from the server when no node is up', r.ok && pool.statusFor(store.keys().find(k => k.id === fb.key.id)).fallbackInUse === true); r.sock?.destroy();
const capped = store.addKey({ kind: 'software', name: 'cap', networks: ['server'], dailyCapMb: 1 });
store.addUsage({ keyId: capped.key.id, bytes: 2 * 1024 * 1024 });
r = await socks(BASE, { user: capped.key.id, pass: capped.secret });
check('a key over its daily cap is refused', !r.ok);
const old = store.addKey({ kind: 'software', name: 'old', networks: ['server'], expiresAt: Date.now() - 1 });
r = await socks(BASE, { user: old.key.id, pass: old.secret });
check('an expired key is refused', !r.ok);

// ── port block: no login, only from allowed sources ──
const blk = store.addKey({ kind: 'software', name: 'scraper', networks: ['server'], portBlock: true, slots: 2, allowFrom: ['loopback'] });
pool.syncBlocks();
const ports = store.keys().find(k => k.id === blk.key.id).ports;
check('port block allocated', ports && ports.base >= BASE + 10 && ports.status === ports.base + 3, JSON.stringify(ports));
await sleep(100);
r = await socks(ports.base + 1);
check('port block slot works without a login from loopback', r.ok && r.reply === 'echo:ping', JSON.stringify(r)); r.sock?.destroy();
const stJson = await new Promise((res) => http.get(`http://127.0.0.1:${ports.status}/`, (x) => { let b = ''; x.on('data', d => b += d); x.on('end', () => res(JSON.parse(b))); }));
check('port block status port answers like the relay', stJson.egressAvailable === true && 'liveCount' in stJson, JSON.stringify(stJson));
store.updateKey(blk.key.id, { allowFrom: ['10.9.9.0/24'] });
r = await socks(ports.base);
check('…and not from a source outside allowFrom', !r.ok);
store.removeKey(blk.key.id); pool.keyChanged(blk.key.id);
await sleep(100);
const gone = await new Promise((res) => { const s = net.connect(ports.base, '127.0.0.1'); s.on('connect', () => { s.destroy(); res(false); }); s.on('error', () => res(true)); });
check('revoking the key closes its ports', gone);

// ── HTTP proxy ──
const auth = 'Basic ' + Buffer.from(`${serverKey.key.id}:${serverKey.secret}`).toString('base64');
const body = await new Promise((res) => {
  http.get({ host: '127.0.0.1', port: BASE + 1, path: 'http://127.0.0.1:17521/x', headers: { 'Proxy-Authorization': auth } }, (x) => { let b = ''; x.on('data', d => b += d); x.on('end', () => res(`${x.statusCode} ${b}`)); }).on('error', (e) => res(e.message));
});
check('HTTP proxy forwards plain http', body === '200 hello /x', body);
const noauth = await new Promise((res) => http.get({ host: '127.0.0.1', port: BASE + 1, path: 'http://127.0.0.1:17521/x' }, (x) => { x.resume(); res(x.statusCode); }));
check('HTTP proxy without a login gets 407', noauth === 407, String(noauth));
const tunnel = await new Promise((res) => {
  const req = http.request({ host: '127.0.0.1', port: BASE + 1, method: 'CONNECT', path: '127.0.0.1:17520', headers: { 'Proxy-Authorization': auth } });
  req.on('connect', (resp, sock) => { sock.write('x'); sock.once('data', (d) => { sock.destroy(); res(`${resp.statusCode} ${d}`); }); });
  req.on('error', (e) => res(e.message)); req.end();
});
check('HTTP CONNECT tunnels', tunnel === '200 echo:x', tunnel);

// ── a chat asks, the owner decides ──
const rq = store.addRequest({ chatId: 'c1', project: 'insta', kind: 'chat', networks: ['home'], reason: 'scrape' });
check('a request is pending, and no key exists for the chat yet', rq.status === 'pending' && !store.keyForChat('c1', 'insta'));
check('asking twice does not make two requests', store.addRequest({ chatId: 'c1', project: 'insta', kind: 'chat', networks: ['home'] }).id === rq.id);
const dec = store.decideRequest(rq.id, { approve: true, changes: { networks: ['server'] } });
check('approving makes the key, with the owner\'s changes', dec.key.networks.join() === 'server' && store.keyForChat('c1', 'insta')?.id === dec.key.id);
check('the secret is handed over once', store.takeApprovedSecret(rq.id) === dec.secret && store.takeApprovedSecret(rq.id) === '');
const rq2 = store.addRequest({ chatId: 'c2', project: 'x', kind: 'software', name: 'bot' });
store.decideRequest(rq2.id, { approve: false });
check('a refused request makes nothing', store.requests().find(x => x.id === rq2.id).status === 'refused' && !store.keys().some(k => k.requestId === rq2.id));
const pk = store.addKey({ kind: 'project', project: 'insta', networks: ['server'] });
check('a project key covers the project\'s other chats', store.keyForChat('c9', 'insta')?.id === pk.key.id);

// ── persistence: secrets are not stored ──
store.save(true);
const raw = (await import('node:fs')).readFileSync(join(dir, 'homepool.json'), 'utf8');
check('no secret is written to disk', !raw.includes(serverKey.secret) && !raw.includes(n2.key.split('.')[1]));

// ── a node on Node's own WebSocket (the Docker node) comes back after the server does ──
{
  const n3 = store.addNode({ name: 'docker', network: 'home' });
  const mk = () => { const w = new WebSocketServer({ port: 17518, host: '127.0.0.1', verifyClient: (info, cb) => { const node = store.nodeByKey(new URL(info.req.url, 'http://x').searchParams.get('key')); if (!node) return cb(false, 401); info.req.node = node; cb(true); } }); w.on('connection', (ws, req) => pool.attachNode(ws, req.node)); return w; };
  let w3 = mk();
  const node3 = startNode({ server: 'http://127.0.0.1:17518', key: n3.key, WebSocketImpl: globalThis.WebSocket });
  check('plain-WebSocket node connects', await until(() => pool.links.has(n3.id)));
  for (const c of w3.clients) c.terminate();
  await new Promise(r => w3.close(r));
  await sleep(1500);   // a reconnect attempt fails against nothing
  w3 = mk();
  check('…and reconnects after the server was away', await until(() => pool.links.has(n3.id), 8000), node3.status().state);
  node3.stop(); w3.close();
}

// A refused connect on Node's own WebSocket backs off; it once looped
// (close() inside the error handler fired another error at once).
{
  const lines = [];
  const n = startNode({ server: 'http://127.0.0.1:17517', key: 'hpn_nx.y', WebSocketImpl: globalThis.WebSocket, onLog: l => lines.push(l) });
  await sleep(3500);
  n.stop();
  check('a refused connect retries calmly (1 s, 2 s, …), not in a loop', lines.length > 2 && lines.length < 20, `${lines.length} lines`);
}

pool.stop(); wss.close(); echo.close(); web.close();
rmSync(dir, { recursive: true, force: true });
console.log(failed ? `\n${failed} failed` : '\nall passed');
process.exit(failed ? 1 : 0);
