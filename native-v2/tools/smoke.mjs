#!/usr/bin/env node
// Start the v2 client and check that it runs: its test driver answers, it
// shows the sign-in page (no server is set), it draws frames when it opens
// and then none while nothing changes (ARCHITECTURE.md: idle draws no
// frames), and, under X11, a picture of its window.
//
//   CRUNDI_TEST_PORT=7622 node native-v2/tools/smoke.mjs <binary> <out dir>
//
// Used by CI (.github/workflows/native-v2.yml) on Linux, Windows and macOS.

import net from 'node:net';
import { spawn } from 'node:child_process';
import { mkdirSync, createWriteStream } from 'node:fs';
import { join } from 'node:path';
import { FrameMeter } from './frames.mjs';

const [bin, out] = process.argv.slice(2);
if (!bin || !out) { console.error('usage: smoke.mjs <binary> <out dir>'); process.exit(2); }
mkdirSync(out, { recursive: true });
const port = Number(process.env.CRUNDI_TEST_PORT || 7622);
const log = createWriteStream(join(out, 'client.log'));
// (Never on a Wayland session the shell carries: Linux runs it under xvfb-run.)
const env = { ...process.env, ZED_MEASUREMENTS: '1' };
if (process.platform === 'linux') delete env.WAYLAND_DISPLAY;
const app = spawn(bin, [], { env, stdio: ['ignore', 'pipe', 'pipe'] });
app.stdout.pipe(log);
const frames = new FrameMeter(app.stderr, (l) => log.write(l));
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
  let s;
  for (let i = 0; i < 40; i++) { s = await d.send({ cmd: 'state' }); if (s.ok) break; await sleep(250); }
  check('the test driver answers', s?.ok, s);
  check('signed out: the sign-in page', s?.state?.signedIn === false, s);
  check('the window drew its first frames', frames.frames.length > 0, frames.frames.length);
  await sleep(2000);
  const from = frames.mark();
  await sleep(4000);
  const idle = frames.since(from);
  check('idle: no frames while nothing changes', idle.frames === 0, idle);
  if (process.platform === 'linux') {
    const shot = await d.send({ cmd: 'snapshot', path: join(out, 'window.png') });
    check('a picture of the window', shot?.ok, shot);
  }
  d.close();
} catch (e) {
  check('the client runs', false, String(e));
}
app.kill();
console.log(failed ? `${failed} failed` : 'smoke ok');
process.exit(failed ? 1 : 0);
