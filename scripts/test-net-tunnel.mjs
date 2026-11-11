#!/usr/bin/env node
// The "server network" for a drawer browser tab: the desktop app's loopback
// proxy (app/net-tunnel-client.js) carries each connection over a real
// websocket to the server's end (src/net-tunnel.js), which connects on its
// side. The target here answers only to a name the server side resolves
// ("internal.test"), so a reply proves the traffic went through the server.

import http from 'node:http';
import net from 'node:net';
import { createRequire } from 'node:module';
import { createTunnelEndpoint, parseTarget, frame as sFrame, parseFrame as sParse } from '../src/net-tunnel.js';
import { createServerNetProxy, frame as cFrame, parseFrame as cParse } from '../app/net-tunnel-client.js';

const require = createRequire(import.meta.url);
const { WebSocketServer, WebSocket } = require('ws');

let failed = 0;
const check = (name, ok, extra = '') => { console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${ok ? '' : ' ' + extra}`); if (!ok) failed++; };

// ─── Codec: both copies agree ───
{
  const a = sFrame(2, 77, Buffer.from('hi'));
  const b = cFrame(2, 77, Buffer.from('hi'));
  check('codec copies are identical', a.equals(b));
  const p = cParse(a);
  check('codec round-trips', p.type === 2 && p.id === 77 && p.payload.toString() === 'hi');
  check('short frame rejected', sParse(Buffer.alloc(3)) === null);
  check('target host:port', JSON.stringify(parseTarget('example.com:443')) === '{"host":"example.com","port":443}');
  check('target [v6]:port', JSON.stringify(parseTarget('[::1]:8080')) === '{"host":"::1","port":8080}');
  check('target bad port refused', parseTarget('x:0') === null && parseTarget('x:70000') === null && parseTarget('a b:80') === null);
}

// ─── A target only the server can reach ───
const BIG = Buffer.alloc(3 * 1024 * 1024, 7);
const target = http.createServer((req, res) => {
  if (req.url === '/big') { res.setHeader('Content-Length', BIG.length); res.end(BIG); return; }
  if (req.method === 'POST') { let n = 0; req.on('data', (d) => { n += d.length; }); req.on('end', () => res.end('got ' + n)); return; }
  res.end('hello from ' + req.headers.host + req.url);
});
await new Promise((r) => target.listen(0, '127.0.0.1', r));
const targetPort = target.address().port;
const serverConnect = (o) => net.connect({ host: o.host === 'internal.test' ? '127.0.0.1' : o.host, port: o.port });

// ─── The server's websocket (stands in for webapp.js's /ws) ───
const wss = new WebSocketServer({ host: '127.0.0.1', port: 0 });
await new Promise((r) => wss.once('listening', r));
let endpoint = null;
wss.on('connection', (ws) => {
  endpoint = createTunnelEndpoint({ send: (b) => ws.send(b, { binary: true }), buffered: () => ws.bufferedAmount, connect: serverConnect });
  ws.on('message', (raw, isBinary) => { if (isBinary) endpoint.handle(raw); });
  ws.on('close', () => endpoint.closeAll());
});

// ─── The desktop app ───
const ws = new WebSocket(`ws://127.0.0.1:${wss.address().port}`);
await new Promise((r) => ws.once('open', r));
let connected = true;
const proxy = createServerNetProxy({ send: (b) => ws.send(b, { binary: true }), isConnected: () => connected && ws.readyState === 1 });
ws.on('message', (raw, isBinary) => { if (isBinary) proxy.handleFrame(raw); });
ws.on('close', () => proxy.onDisconnect());
const port = await proxy.start();
const AUTH = 'Basic ' + Buffer.from(`${proxy.credentials.user}:${proxy.credentials.pass}`).toString('base64');

function viaProxy(path, { auth = AUTH, method = 'GET', body } = {}) {
  return new Promise((resolve, reject) => {
    const headers = { Host: new URL(path).host };
    if (auth) headers['Proxy-Authorization'] = auth;
    const req = http.request({ host: '127.0.0.1', port, path, method, headers }, (res) => {
      const parts = []; res.on('data', (d) => parts.push(d)); res.on('end', () => resolve({ status: res.statusCode, body: Buffer.concat(parts), headers: res.headers }));
    });
    req.on('error', reject);
    if (body) req.write(body);
    req.end();
  });
}

function viaConnect(hostPort, raw) {
  return new Promise((resolve, reject) => {
    const s = net.connect(port, '127.0.0.1');
    const parts = [];
    let established = false;
    s.on('connect', () => s.write(`CONNECT ${hostPort} HTTP/1.1\r\nHost: ${hostPort}\r\nProxy-Authorization: ${AUTH}\r\n\r\n`));
    s.on('data', (d) => {
      parts.push(d);
      if (!established && Buffer.concat(parts).includes('\r\n\r\n')) {
        const head = Buffer.concat(parts).toString('latin1');
        if (!/^HTTP\/1\.1 200/.test(head)) { s.end(); return; }
        established = true; parts.length = 0;
        s.write(raw);
      }
    });
    s.on('end', () => resolve({ established, body: Buffer.concat(parts) }));
    s.on('error', reject);
  });
}

// ─── Behaviour ───
check('proxy listens on loopback only', proxy.port > 0);
{
  const srv = net.connect(port, '127.0.0.1'); await new Promise((r) => srv.once('connect', r)); srv.destroy();
}
{
  const r = await viaProxy(`http://internal.test:${targetPort}/x`, { auth: '' });
  check('no password: 407 with a Basic challenge', r.status === 407 && /Basic/.test(r.headers['proxy-authenticate'] || ''), String(r.status));
  const w = await viaProxy(`http://internal.test:${targetPort}/x`, { auth: 'Basic ' + Buffer.from('crundi:wrong').toString('base64') });
  check('wrong password: 407', w.status === 407);
}
{
  const r = await viaProxy(`http://internal.test:${targetPort}/hello?q=1`);
  check('plain http reaches a host only the server resolves', r.status === 200 && r.body.toString() === `hello from internal.test:${targetPort}/hello?q=1`, r.status + ' ' + r.body.toString().slice(0, 80));
}
{
  const r = await viaProxy(`http://internal.test:${targetPort}/big`);
  check('3 MB response arrives intact', r.status === 200 && r.body.equals(BIG), String(r.body.length));
}
{
  const up = Buffer.alloc(1024 * 1024, 1);
  const r = await viaProxy(`http://internal.test:${targetPort}/up`, { method: 'POST', body: up });
  check('1 MB upload arrives intact', r.body.toString() === 'got ' + up.length, r.body.toString());
}
{
  const r = await viaConnect(`internal.test:${targetPort}`, `GET /tls-stand-in HTTP/1.1\r\nHost: internal.test\r\nConnection: close\r\n\r\n`);
  check('CONNECT tunnel (https / websockets) carries bytes both ways', r.established && /hello from internal\.test\/tls-stand-in/.test(r.body.toString()), r.body.toString().slice(0, 120));
}
{
  const r = await viaProxy(`http://127.0.0.1:1/nothing`);
  check('refused on the server: 502 that says why', r.status === 502 && /ECONNREFUSED/.test(r.body.toString()), r.status + ' ' + r.body.toString().slice(-120));
}
{
  // Let the closes from the requests above land.
  await new Promise((r) => setTimeout(r, 150));
  check('streams are released after use', proxy.streamCount() === 0 && endpoint.size() === 0, proxy.streamCount() + '/' + endpoint.size());
}
{
  connected = false;
  const r = await viaProxy(`http://internal.test:${targetPort}/x`);
  check('app not connected to the server: 502 that says so', r.status === 502 && /not connected/.test(r.body.toString()));
  connected = true;
}
{
  // A stream that is open when the socket drops must end, not hang.
  const s = net.connect(port, '127.0.0.1');
  await new Promise((r) => s.once('connect', r));
  s.write(`CONNECT internal.test:${targetPort} HTTP/1.1\r\nProxy-Authorization: ${AUTH}\r\n\r\n`);
  await new Promise((r) => s.once('data', r));
  const ended = new Promise((r) => s.once('close', () => r(true)));
  ws.close();
  const done = await Promise.race([ended, new Promise((r) => setTimeout(() => r(false), 3000))]);
  check('open tunnel ends when the websocket drops', done === true);
  await new Promise((r) => setTimeout(r, 100));
  check('server side released its streams', endpoint.size() === 0);
}
check('isOurs matches only our proxy challenge', proxy.isOurs({ isProxy: true, host: '127.0.0.1', port }) && !proxy.isOurs({ isProxy: true, host: 'example.com', port }) && !proxy.isOurs({ isProxy: false, host: '127.0.0.1', port }));

proxy.stop(); wss.close(); target.close();
console.log(failed ? `\n${failed} failed` : '\nall passed');
process.exit(failed ? 1 : 0);
