// A WebSocket upgrade through a forward, in both directions.
//
// The regression this exists for: the upstream's own first bytes — the ones
// that arrive in the same read as its 101 — were unshifted onto the CLIENT
// socket, so the proxy piped the server's first packet straight back at the
// server and never gave it to the client. A WebSocket server rejects its own
// unmasked frame with "bad MASK". Invisible with Vite/webpack, which say
// nothing until spoken to; fatal for anything that speaks first (chisel, SSH
// over WS, WS-RPC, terminals).
import { createServer } from 'http';
import { connect } from 'net';
import { proxyUpgrade } from '../src/forwards.js';

let failed = 0;
const check = (name, ok, extra = '') => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${ok ? '' : ' ' + extra}`);
  if (!ok) failed++;
};

const GREETING = 'UPSTREAM-SPEAKS-FIRST';
const FROM_CLIENT = 'PING-FROM-CLIENT';
const listen = (srv) => new Promise((r) => srv.listen(0, '127.0.0.1', () => r(srv.address().port)));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ─── Upstream: answers the upgrade and greets in the SAME write, so the
// greeting lands in the same TCP segment as the 101 and shows up as `head`.
let upstreamHeard = Buffer.alloc(0);
const upstream = createServer();
upstream.on('upgrade', (req, sock) => {
  sock.write(
    'HTTP/1.1 101 Switching Protocols\r\n'
    + 'Upgrade: websocket\r\nConnection: Upgrade\r\n\r\n'
    + GREETING,
  );
  sock.on('data', (d) => { upstreamHeard = Buffer.concat([upstreamHeard, d]); });
});
const upstreamPort = await listen(upstream);

// ─── The proxy, calling the real thing.
const proxy = createServer();
proxy.on('upgrade', (req, sock, head) => proxyUpgrade({ port: upstreamPort }, req, sock, head));
const proxyPort = await listen(proxy);

// ─── Client.
let clientHeard = Buffer.alloc(0);
const client = connect(proxyPort, '127.0.0.1');
client.on('data', (d) => { clientHeard = Buffer.concat([clientHeard, d]); });
await new Promise((r) => client.on('connect', r));
client.write(
  'GET /socket HTTP/1.1\r\nHost: app.example.com\r\n'
  + 'Upgrade: websocket\r\nConnection: Upgrade\r\n\r\n',
);

for (let i = 0; i < 40 && !clientHeard.includes('\r\n\r\n'); i++) await sleep(25);
const text = clientHeard.toString('binary');
const body = text.slice(text.indexOf('\r\n\r\n') + 4);

check('client gets the 101', text.startsWith('HTTP/1.1 101 '), JSON.stringify(text.slice(0, 40)));
check("upstream's first packet reaches the client", body === GREETING, `got ${JSON.stringify(body)}`);
check("upstream is not sent its own packet back", !upstreamHeard.includes(GREETING),
  `upstream heard ${JSON.stringify(upstreamHeard.toString('binary'))}`);

// ─── The other direction still works, and nothing extra rode along with it.
client.write(FROM_CLIENT);
for (let i = 0; i < 40 && !upstreamHeard.includes(FROM_CLIENT); i++) await sleep(25);
check('client data reaches the upstream', upstreamHeard.toString('binary') === FROM_CLIENT,
  `upstream heard ${JSON.stringify(upstreamHeard.toString('binary'))}`);

client.destroy(); upstream.close(); proxy.close();
console.log(failed ? `\n${failed} check(s) failed` : '\nAll forward-upgrade checks passed');
process.exit(failed ? 1 : 0);
