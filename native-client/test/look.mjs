#!/usr/bin/env node
// Sign the running native client in to a test server and take window
// snapshots, for comparing with the web UI (the driver: src/testctl.rs).
//
//   node native-client/test/look.mjs <session.json> <out-prefix> [actions…]
//
// session.json: {password, secret} of the test server (CRUNDI_TEST_SERVER,
// default http://localhost:8889). Actions after sign-in, each a snapshot:
//   wait:<ms>  ui:<name>[:arg…]  snap:<name>
//   mouse:<move|click|dblclick|leave>:<x>:<y>   key:<Escape|Return|text>
// A ui: argument "@pane" is the first chat pane's leaf id.
import net from 'node:net';
import { readFileSync } from 'node:fs';
import { totpNow } from '../../src/auth-password.js';

const [sessionFile, prefix, ...actions] = process.argv.slice(2);
const SERVER = process.env.CRUNDI_TEST_SERVER || 'http://localhost:8889';
const PORT = Number(process.env.CRUNDI_TEST_PORT || 7612);
const sess = JSON.parse(readFileSync(sessionFile, 'utf8'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let sock;
for (let i = 0; i < 120 && !sock; i++) {
  sock = await new Promise((res) => { const c = net.connect(PORT, '127.0.0.1'); c.once('connect', () => res(c)); c.once('error', () => res(null)); });
  if (!sock) await sleep(500);
}
let buf = ''; const q = [];
sock.on('data', (d) => { buf += d; let i; while ((i = buf.indexOf('\n')) >= 0) { const l = buf.slice(0, i); buf = buf.slice(i + 1); q.shift()(JSON.parse(l)); } });
const send = (c) => new Promise((r) => { q.push(r); sock.write(JSON.stringify(c) + '\n'); });
const props = () => send({ cmd: 'props' });

let p = await props();
if (!p.signedIn) {
  await send({ cmd: 'ui', name: 'check-server', args: [SERVER] });
  for (let i = 0; i < 40 && !(await props()).signinChecked; i++) await sleep(250);
  await send({ cmd: 'ui', name: 'sign-in-password', args: [SERVER, sess.password, totpNow(sess.secret)] });
}
for (let i = 0; i < 60; i++) { p = await props(); if (p.signedIn && p.panes.some((x) => x.kind === 'chat')) break; await sleep(500); }
await sleep(2500);
console.log('panes', JSON.stringify(p.panes.map((x) => [x.kind, x.title, Math.round(x.x), Math.round(x.w)])));
await send({ cmd: 'snapshot', path: `${prefix}-0.png` });
let n = 1;
for (const a of actions) {
  const [kind, name, ...args] = a.split(':');
  if (kind === 'wait') await sleep(Number(name));
  else if (kind === 'ui') {
    const leaf = String((await props()).panes.find((x) => x.kind === 'chat')?.id ?? 0);
    console.log(a, JSON.stringify(await send({ cmd: 'ui', name, args: args.map((x) => (x === '@pane' ? leaf : x)) })));
  }
  else if (kind === 'mouse') await send({ cmd: 'mouse', kind: name, x: Number(args[0] || 0), y: Number(args[1] || 0) });
  else if (kind === 'key') await send({ cmd: 'key', text: [name, ...args].join(':') });
  else if (kind === 'snap') { await sleep(600); await send({ cmd: 'snapshot', path: `${prefix}-${name}.png` }); console.log('snap', name); }
  n++;
}
sock.end();
