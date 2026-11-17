#!/usr/bin/env node
// Start the GPUI client and check that it runs: its test driver answers
// (crundi_core::driver, the Slint client's protocol), it shows the sign-in
// page (no server is set), and, under X11, a picture of its window.
//
//   CRUNDI_TEST_PORT=7612 node native-gpui/tools/smoke.mjs <binary> <out dir>
//
// Used by CI (.github/workflows/native-gpui.yml) on Linux, Windows and macOS.

import net from 'node:net';
import { spawn } from 'node:child_process';
import { mkdirSync, openSync } from 'node:fs';
import { join } from 'node:path';

const [bin, out] = process.argv.slice(2);
if (!bin || !out) { console.error('usage: smoke.mjs <binary> <out dir>'); process.exit(2); }
mkdirSync(out, { recursive: true });
const port = Number(process.env.CRUNDI_TEST_PORT || 7612);
const log = openSync(join(out, 'client.log'), 'w');
const app = spawn(bin, [], { env: process.env, stdio: ['ignore', log, log] });
let exited = null;
app.on('exit', (code, sig) => { exited = sig || code; });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
function driver() {
  return new Promise((resolve, reject) => {
    const tryConnect = (left) => {
      const sock = net.connect(port, '127.0.0.1');
      sock.once('connect', () => {
        let buf = '';
        const waiting = [];
        sock.on('data', (d) => {
          buf += d;
          let i;
          while ((i = buf.indexOf('\n')) >= 0) { const line = buf.slice(0, i); buf = buf.slice(i + 1); waiting.shift()?.(JSON.parse(line)); }
        });
        resolve({ send: (c) => new Promise((r) => { waiting.push(r); sock.write(JSON.stringify(c) + '\n'); }), close: () => sock.end() });
      });
      sock.once('error', () => (left > 0 && exited === null ? setTimeout(() => tryConnect(left - 1), 500) : reject(new Error(`no test driver (client exited: ${exited})`))));
    };
    tryConnect(240);
  });
}

let failed = 0;
const check = (name, ok, extra = '') => { console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${ok ? '' : '  ' + JSON.stringify(extra).slice(0, 400)}`); if (!ok) failed++; };

try {
  const d = await driver();
  let p;
  for (let i = 0; i < 40; i++) { p = await d.send({ cmd: 'props' }); if (p.ok) break; await sleep(250); }
  check('the test driver answers', p?.ok, p);
  check('signed out: the sign-in page', p?.signedIn === false, p);
  const s = await d.send({ cmd: 'state' });
  check('state answers', s?.ok, s);
  const tab = await d.send({ cmd: 'ui', name: 'tab', args: ['git'] });
  check('a window action runs', tab?.ok, tab);
  if (process.platform === 'linux') {
    await sleep(1500);
    const shot = await d.send({ cmd: 'snapshot', path: join(out, 'window.png') });
    check('a picture of the window', shot?.ok, shot);
  }
  d.close();
} catch (e) {
  check('the client starts', false, String(e));
}
check('the client is still running', exited === null, exited);
app.kill();
await sleep(500);
process.exit(failed ? 1 : 0);
