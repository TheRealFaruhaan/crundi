// The shared harness for native-v2's end-to-end suites (AREAS.md): a fresh
// test server set up (a password, a project "demo"), the app started with
// its own config and the frame meter on its stderr, signed in through its
// test driver, optionally through a proxy that can make every connection
// go silent without closing (the dead-connection test).
//
//   import * as t from './lib.mjs';
//   const T = await t.begin('slice', { proxy: true });
//   await T.ui('new-chat');
//   const s = await T.until('a pane', (s) => s.panes?.length > 0);
//   const m = T.frames.mark(); …; T.frames.since(m)    // tools/frames.mjs
//   await T.end();
//
// Run on the Crundi server only (never on the owner's PC), under Xvfb, with
// a FRESH test server from native-gpui/tools/test-server.sh <port> --bare:
//
//   native-gpui/tools/test-server.sh 4880 --bare
//   native-v2/tools/build.sh
//   VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json CRUNDI_TEST_SERVER=http://localhost:4880 CRUNDI_TEST_PORT=4881 \
//     NATIVE_BIN=$HOME/.cache/crundi-native-target/debug/crundi-v2 \
//     xvfb-run -a -s "-screen 0 1440x900x24" node native-v2/test/slice.mjs
//   kill $(cat /tmp/crundi-devtest-4880/server.pid)

import net from 'node:net';
import { spawn, execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, createWriteStream } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { totpNow } from '../../src/auth-password.js';
import { FrameMeter } from '../tools/frames.mjs';
import { fileURLToPath } from 'node:url';

const SCREENS = process.env.SCREENS_DIR || fileURLToPath(new URL('../screens/', import.meta.url));

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let failed = 0;
let passed = 0;
/** One check: prints ok / FAIL and counts. Returns ok. */
export function check(name, ok, extra = '') {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${ok ? '' : '  ' + JSON.stringify(extra).slice(0, 600)}`);
  if (ok) passed++; else failed++;
  return ok;
}

/** The server's HTTP API (setting up data, checking what the app did). */
export function client(server) {
  let token = '';
  const http = async (method, path, body, tok = token) => {
    const res = await fetch(server + path, {
      method,
      headers: { 'Content-Type': 'application/json', ...(tok ? { Authorization: 'Bearer ' + tok } : {}) },
      body: body ? JSON.stringify(body) : undefined,
    });
    const text = await res.text();
    let json = null;
    try { json = JSON.parse(text); } catch { /* not JSON */ }
    return { status: res.status, body: json, text };
  };
  return { http, setToken: (t) => { token = t; } };
}

/** The app's test driver (crundi_core::driver: one JSON line each way). */
export function driver(port, alive) {
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
        sock.setNoDelay(true);
        resolve({ send: (cmd) => new Promise((r) => { waiting.push(r); sock.write(JSON.stringify(cmd) + '\n'); }), close: () => sock.end() });
      });
      sock.once('error', () => (left > 0 && alive() ? setTimeout(() => tryConnect(left - 1), 500) : reject(new Error('no test driver'))));
    };
    tryConnect(120);
  });
}

/**
 * A TCP proxy to the server. `freeze()` makes every connection open now go
 * silent both ways without closing (a network that died under it); new
 * connections pass as usual.
 */
export function proxy(target) {
  const u = new URL(target);
  const pairs = new Set();
  const srv = net.createServer((c) => {
    const up = net.connect(Number(u.port || 80), u.hostname);
    const p = { c, up, frozen: false };
    pairs.add(p);
    c.on('data', (d) => { if (!p.frozen) up.write(d); });
    up.on('data', (d) => { if (!p.frozen) c.write(d); });
    const end = () => { pairs.delete(p); c.destroy(); up.destroy(); };
    c.on('error', end); up.on('error', end); c.on('close', end); up.on('close', end);
  });
  return new Promise((resolve) => srv.listen(0, '127.0.0.1', () => resolve({
    url: `http://127.0.0.1:${srv.address().port}`,
    freeze: () => { for (const p of pairs) p.frozen = true; return pairs.size; },
    close: () => { for (const p of pairs) { p.c.destroy(); p.up.destroy(); } srv.close(); },
  })));
}

/** Set up the server, start the app, sign in, open the project. */
export async function begin(name, { proxy: withProxy = false } = {}) {
  const SERVER = process.env.CRUNDI_TEST_SERVER;
  const PORT = Number(process.env.CRUNDI_TEST_PORT || 0);
  const BIN = process.env.NATIVE_BIN;
  if (!SERVER || !PORT || !BIN) { console.error('Set CRUNDI_TEST_SERVER, CRUNDI_TEST_PORT and NATIVE_BIN (see test/lib.mjs)'); process.exit(2); }
  const OUT = process.env.DRIVE_OUT || join(tmpdir(), `crundi-v2-${name}-out`);
  mkdirSync(OUT, { recursive: true });

  const api = client(SERVER);
  const methods = await api.http('GET', '/api/auth/methods');
  if (methods.body?.telegram || methods.body?.telegramBotId) { console.error(`Refusing ${SERVER}: it has a Telegram bot configured.`); process.exit(2); }
  if (!methods.body?.setupRequired) { console.error(`${SERVER} needs to be fresh: native-gpui/tools/test-server.sh <port> --bare`); process.exit(2); }
  const password = `native-v2-${name}-` + Math.random().toString(36).slice(2, 10);
  const setup = await api.http('POST', '/api/auth/setup', { method: 'password', password });
  if (!setup.body?.token) { console.error('setup failed', setup); process.exit(2); }
  api.setToken(setup.body.token);
  const secret = setup.body.totpSecret;
  const usedPort = new URL(SERVER).port;
  const projDir = `/tmp/crundi-devtest-${usedPort}/proj`;
  const p = await api.http('POST', '/api/projects', { alias: 'demo', name: 'Demo', path: projDir, create: true });
  check('a project to work in', p.status < 300, p.body);

  const px = withProxy ? await proxy(SERVER) : null;
  const via = px ? px.url : SERVER;
  const configDir = mkdtempSync(join(tmpdir(), `crundi-v2-config-${name}-`));
  const log = createWriteStream(join(OUT, `native-${name}.log`), { flags: 'a' });
  let exited = false;
  // Only ever on the virtual display xvfb-run gives: never a Wayland
  // session the shell happens to carry (it would open on a person's screen).
  const env = { ...process.env, CRUNDI_TEST_PORT: String(PORT), CRUNDI_CONFIG_DIR: configDir, CRUNDI_OPEN_URL_FILE: join(configDir, 'open-url.txt'), ZED_MEASUREMENTS: '1' };
  delete env.WAYLAND_DISPLAY;
  if (!env.DISPLAY || env.DISPLAY === ':0') { console.error('Run under xvfb-run (DISPLAY is not a virtual display).'); process.exit(2); }
  const app = spawn(BIN, [], {
    env,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  app.stdout.pipe(log);
  const frames = new FrameMeter(app.stderr, (l) => log.write(l));
  app.on('exit', () => { exited = true; });
  const d = await driver(PORT, () => !exited);

  const state = async () => (await d.send({ cmd: 'state' })).state || {};
  const T = {
    name, SERVER, api, OUT, configDir, frames, proxy: px, app, d,
    get exited() { return exited; },
    state,
    ui: (n, ...args) => d.send({ cmd: 'ui', name: n, args }),
    /** The pointer, as a person's (move, click, wheel with dy…), at window x,y. */
    mouse: (kind, x, y, extra = {}) => d.send({ cmd: 'mouse', kind, x, y, ...extra }),
    /** How many pixels differ between two PNGs, optionally in a crop WxH+X+Y. */
    diff(a, b, crop) {
      const c = crop ? ['-crop', crop, '+repage'] : [];
      const cut = (f) => { const o = `${f}.cut.png`; execFileSync('convert', [f, ...c, o]); return o; };
      try { execFileSync('compare', ['-metric', 'AE', cut(a), cut(b), 'null:'], { stdio: ['ignore', 'ignore', 'pipe'] }); return 0; }
      catch (e) { return Number(String(e.stderr).trim().split(/\s/)[0]) || -1; }
    },
    /** A picture of the window into native-v2/screens/<area>/<file>.png (the latest only), for the UI reviewer. */
    async snap(area, file) {
      const dir = join(SCREENS, area);
      mkdirSync(dir, { recursive: true });
      const r = await d.send({ cmd: 'snapshot', path: join(dir, `${file}.png`) });
      if (!r?.ok) console.log(`(no snapshot ${area}/${file}: ${r?.error})`);
      return r;
    },
    /** Wait until test(state) holds; a failed check otherwise. */
    async until(what, test, ms = 20000) {
      const end = Date.now() + ms;
      let last;
      while (Date.now() < end) {
        last = await state();
        try { if (test(last)) { check(what, true); return last; } } catch { /* not yet */ }
        await sleep(200);
      }
      check(what, false, last);
      return null;
    },
    async end() {
      try { app.kill(); } catch { /* gone */ }
      px?.close();
      d.close();
      console.log(`\n${passed} passed, ${failed} failed`);
      process.exit(failed ? 1 : 0);
    },
  };

  // Sign in as a person would: the server step, then password and code.
  await T.ui('check-server', via);
  await T.until('the login step', (s) => s.signinStep === 'Login');
  await T.snap('sign-in', 'login-step');
  await T.ui('sign-in-password', via, password, totpNow(secret));
  await T.until('signed in, the project open, online', (s) => s.signedIn && s.project === 'demo' && s.online, 30000);
  await sleep(500);
  await T.snap('shell', 'signed-in-project');
  return T;
}
