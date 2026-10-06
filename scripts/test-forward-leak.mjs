// A forward must not keep an upstream connection its client has left.
//
// The regression this exists for: proxy() was `up.pipe(res)`. When the client
// went away pipe() unpiped and left the upstream response paused and open, and
// when the client merely stopped reading it waited for a 'drain' that never
// came. Either way the upstream kept sending into a socket nobody read. On a
// server with streaming forwards (SSE, live view, media) that was 66 sockets
// holding ~150 MB of kernel receive buffer, enough to push the whole machine
// into TCP memory pressure and throttle everything else on it.
//
// Counted from the upstream's side — a response or socket it still has open —
// so this needs no `ss` and runs anywhere.
import { createServer } from 'http';
import { connect } from 'net';
import { createHash } from 'crypto';

process.env.FORWARD_STALL_TIMEOUT_MS = '1000';
const { proxy, proxyUpgrade } = await import('../src/forwards.js');

let failed = 0;
const check = (name, ok, extra = '') => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${ok ? '' : ' ' + extra}`);
  if (!ok) failed++;
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const listen = (srv) => new Promise((r) => srv.listen(0, '127.0.0.1', () => r(srv.address().port)));
const until = async (fn, ms = 3000) => {
  for (const end = Date.now() + ms; Date.now() < end;) { if (fn()) return true; await sleep(25); }
  return fn();
};
const sha = (b) => createHash('sha1').update(b).digest('hex');
const CHUNK = Buffer.alloc(64 * 1024, 'x');
const BIG = Buffer.alloc(5 * 1024 * 1024);
for (let i = 0; i < BIG.length; i++) BIG[i] = (i * 31) & 255;

// ─── Upstream. `streams` is what it still has open: the thing that must drain
// back to zero after every client exit.
let streams = 0;
const flood = (w) => {
  streams++;
  w.on('close', () => { streams--; });
  w.on('error', () => {});
  const go = () => { while (!w.destroyed && w.write(CHUNK)); };
  w.on('drain', go); go();
};
const upstream = createServer((req, res) => {
  if (req.url === '/big') { res.writeHead(200, { 'Content-Length': BIG.length }); res.end(BIG); return; }
  if (req.url === '/echo') {
    const parts = [];
    req.on('data', (d) => parts.push(d));
    req.on('end', () => res.end(sha(Buffer.concat(parts))));
    return;
  }
  if (req.url === '/hang') { streams++; res.on('close', () => { streams--; }); return; }
  if (req.url === '/die') {
    res.writeHead(200, { 'Content-Length': 1000 }); res.write('partial');
    setTimeout(() => res.socket.destroy(), 50);
    return;
  }
  res.writeHead(200, { 'Content-Type': 'text/event-stream' });
  flood(res);
});
upstream.on('upgrade', (req, sock) => {
  sock.write('HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n');
  if (req.url === '/quiet') {
    // Says nothing, and never answers a FIN with its own. Counted as released
    // once the FIN arrives: that is all the proxy can do to a peer like this.
    streams++;
    let open = true;
    const done = () => { if (open) { open = false; streams--; } };
    sock.on('end', done); sock.on('close', done); sock.on('error', () => {});
    sock.resume();
    return;
  }
  flood(sock);
});
const upPort = await listen(upstream);

// An upstream with no 'upgrade' handler: it answers an upgrade request with an
// ordinary response.
const plain = createServer((req, res) => { res.writeHead(200); flood(res); });
const plainPort = await listen(plain);

const px = createServer((req, res) => proxy({ port: upPort }, req, res));
px.on('upgrade', (req, sock, head) => proxyUpgrade({ port: req.url === '/refused' ? plainPort : upPort }, req, sock, head));
const pxPort = await listen(px);

const raw = (text) => new Promise((resolve) => {
  const c = connect(pxPort, '127.0.0.1', () => { c.write(text); resolve(c); });
  c.on('error', () => {});
});
const get = (path) => raw(`GET ${path} HTTP/1.1\r\nHost: app.example.com\r\n\r\n`);
const upgrade = (path) => raw(`GET ${path} HTTP/1.1\r\nHost: app.example.com\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n`);
const closed = (c) => { let done = false; c.on('close', () => { done = true; }); return () => done; };

// ─── Ordinary traffic still works. The pump replaced pipe(); it has to carry
// bodies whole in both directions and leave keep-alive alone.
{
  const want = sha(BIG);
  let ok = true;
  for (let i = 0; i < 3 && ok; i++) {
    const r = await fetch(`http://127.0.0.1:${pxPort}/big`);
    ok = r.status === 200 && sha(Buffer.from(await r.arrayBuffer())) === want;
  }
  check('a 5 MB response arrives intact, repeatedly', ok);
  const r = await fetch(`http://127.0.0.1:${pxPort}/echo`, { method: 'POST', body: BIG });
  check('a 5 MB request body arrives intact', (await r.text()) === want);
}

// ─── HTTP / SSE.
{
  const c = await get('/sse'); c.resume();
  await until(() => streams === 1);
  c.destroy();
  check('client closes mid-stream: upstream is destroyed', await until(() => streams === 0), `streams=${streams}`);
}
{
  const c = await get('/sse'); c.pause();                  // connected, reading nothing
  const gone = closed(c);
  await until(() => streams === 1);
  await sleep(300);
  check('a stalled client is left alone inside the timeout', streams === 1, `streams=${streams}`);
  check('client stalls: upstream is destroyed after the timeout', await until(() => streams === 0, 4000), `streams=${streams}`);
  c.resume();
  check('client stalls: its own connection is closed too', await until(gone), 'still open');
}
{
  // Slow, not stalled: takes a little every 200 ms, for three timeouts' worth.
  const c = await get('/sse'); c.pause();
  await until(() => streams === 1);
  const t = setInterval(() => { c.resume(); setTimeout(() => c.pause(), 20); }, 200);
  await sleep(3000);
  clearInterval(t);
  check('a slow client that keeps reading is not cut off', streams === 1 && !c.destroyed, `streams=${streams}`);
  c.destroy();
  await until(() => streams === 0);
}
{
  const c = await get('/hang');
  await until(() => streams === 1);
  c.destroy();
  check('client leaves before the upstream answers: request is destroyed', await until(() => streams === 0), `streams=${streams}`);
}
{
  const c = await get('/die'); c.resume();
  const gone = closed(c);
  check('upstream dies mid-response: the client is not left hanging', await until(gone), 'still open');
}

// ─── WebSocket upgrades.
{
  const c = await upgrade('/ws'); c.resume();
  await until(() => streams === 1);
  c.destroy();
  check('upgrade, client closes: upstream socket is destroyed', await until(() => streams === 0), `streams=${streams}`);
}
{
  const c = await upgrade('/ws'); c.pause();
  const gone = closed(c);
  await until(() => streams === 1);
  check('upgrade, client stalls: upstream socket is destroyed after the timeout', await until(() => streams === 0, 4000), `streams=${streams}`);
  c.resume();
  check('upgrade, client stalls: its own connection is closed too', await until(gone), 'still open');
}
{
  // Nothing queued in either direction: idle is not a stall.
  const c = await upgrade('/quiet'); c.resume();
  await until(() => streams === 1);
  await sleep(2500);
  check('an idle upgraded connection is not cut off', streams === 1 && !c.destroyed, `streams=${streams}`);
  // Client half-closes; this upstream never closes in reply. The FIN is passed
  // on at once, and after 5 s of grace the proxy stops waiting and drops both.
  const gone = closed(c);
  c.end();
  check('upgrade, client sends FIN: it is passed to the upstream', await until(() => streams === 0), `streams=${streams}`);
  await sleep(1000);
  check('upgrade, half-closed: the upstream gets a grace period', !gone());
  check('upgrade, half-closed and upstream never closes: dropped after the grace', await until(gone, 7000), 'still open');
}
{
  const c = await upgrade('/refused'); c.resume();
  const gone = closed(c);
  check('upstream refuses the upgrade: client is closed', await until(gone), 'still open');
  check('upstream refuses the upgrade: its unread response is destroyed', await until(() => streams === 0), `streams=${streams}`);
}

for (const s of [upstream, plain, px]) { s.closeAllConnections(); s.close(); }
console.log(failed ? `\n${failed} check(s) failed` : '\nAll forward-leak checks passed');
process.exit(failed ? 1 : 0);
