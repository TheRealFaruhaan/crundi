// The shared harness for the areas' end-to-end tests (AREAS.md):
// test/workspace.mjs, test/planning.mjs, test/operations.mjs, test/desktop.mjs.
//
//   import * as t from './lib.mjs';
//   const T = await t.begin('planning');          // server set up, app started, signed in
//   await T.ui('tab', 'kanban');                   // a window action
//   await T.act('open-card', id);                  // the area's own: planning:open-card
//   const p = await T.until('the board loads', (p) => p.areas.planning.cards?.length > 0);
//   await T.snap('kanban-board');                  // native-client/screens/planning/kanban-board.png
//   await T.end();                                 // stops the app, prints the count, exits
//
// Each area has its own ports, CEF profile and config, so the four tests run
// side by side (each under its own `xvfb-run -a`):
//
//   area        test server   driver
//   workspace   4390          4391
//   planning    4392          4393
//   ops         4394          4395
//   desktop     4396          4397
//   (lead / tester: 4398, 4399)
//
// It needs a FRESH test server with no sign-in (it sets a password), started
// through tools/test-server.sh <port> --bare, never by hand:
//
//   native-client/tools/test-server.sh 4392 --bare
//   BIN=$(native-client/tools/area-build.sh planning)
//   NATIVE_BIN=$BIN xvfb-run -a -s "-screen 0 1440x900x24" node native-client/test/planning.mjs
//
// Environment: NATIVE_BIN (required), CRUNDI_TEST_SERVER and CRUNDI_TEST_PORT
// (default: the area's ports above), SCREENS=0 to skip writing screens.

import net from 'node:net';
import { spawn } from 'node:child_process';
import { mkdtempSync, mkdirSync, openSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { totpNow } from '../../src/auth-password.js';

export const PORTS = {
  workspace: [4390, 4391],
  planning: [4392, 4393],
  ops: [4394, 4395],
  desktop: [4396, 4397],
  lead: [4398, 4399],
};

const here = dirname(fileURLToPath(import.meta.url));
export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let failed = 0;
let passed = 0;
/** One check: prints ok / FAIL and counts. Returns ok. */
export function check(name, ok, extra = '') {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${ok ? '' : '  ' + JSON.stringify(extra).slice(0, 600)}`);
  if (ok) passed++; else failed++;
  return ok;
}

/** The server's HTTP API (for setting up data and checking what the app did). */
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
  return { http, setToken: (t) => { token = t; }, get token() { return token; } };
}

/** Connect to the app's test driver (src/testctl.rs). */
export function driver(port) {
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
        resolve({
          send: (cmd) => new Promise((r) => { waiting.push(r); sock.write(JSON.stringify(cmd) + '\n'); }),
          close: () => sock.end(),
        });
      });
      sock.once('error', () => (left > 0 ? setTimeout(() => tryConnect(left - 1), 500) : reject(new Error('no test driver'))));
    };
    tryConnect(120);
  });
}

/**
 * Set up the server (a password, a project "demo"), start the app with its
 * own config and CEF profile, sign in, open the project. Returns the test's
 * handle (see the top of this file).
 */
export async function begin(area, { project = true } = {}) {
  const [sPort, dPort] = PORTS[area] || PORTS.lead;
  const SERVER = process.env.CRUNDI_TEST_SERVER || `http://localhost:${sPort}`;
  const PORT = Number(process.env.CRUNDI_TEST_PORT || dPort);
  const BIN = process.env.NATIVE_BIN;
  if (!BIN) { console.error('Set NATIVE_BIN (tools/area-build.sh <area> prints it)'); process.exit(2); }
  // SCREENS_DIR: elsewhere (the GPUI client's, native-gpui/screens/<area>).
  const SCREENS = process.env.SCREENS_DIR || join(here, '..', 'screens', area);
  const OUT = process.env.DRIVE_OUT || join(tmpdir(), `crundi-native-${area}-out`);
  mkdirSync(OUT, { recursive: true });

  const api = client(SERVER);
  const methods = await api.http('GET', '/api/auth/methods');
  if (methods.body?.telegram || methods.body?.telegramBotId) {
    console.error(`Refusing ${SERVER}: it has a Telegram bot configured. Use tools/test-server.sh ${sPort} --bare.`);
    process.exit(2);
  }
  if (!methods.body?.setupRequired) { console.error(`${SERVER} needs to be fresh: tools/test-server.sh ${sPort} --bare`); process.exit(2); }
  const password = `native-${area}-` + Math.random().toString(36).slice(2, 10);
  const setup = await api.http('POST', '/api/auth/setup', { method: 'password', password });
  if (!setup.body?.token) { console.error('setup failed', setup); process.exit(2); }
  api.setToken(setup.body.token);
  const secret = setup.body.totpSecret;
  const status = await api.http('GET', '/api/status');
  if (status.body?.botConfigured !== false) { console.error(`Refusing ${SERVER}: a Telegram bot is configured.`); process.exit(2); }
  // The project folder of the server actually used (its own port's test
  // dir, which tools/test-server.sh makes fresh each start): never a folder
  // shared with another run or reviewer (F17).
  const usedPort = new URL(SERVER).port || sPort;
  const projDir = `/tmp/crundi-devtest-${usedPort}/proj`;
  if (project) {
    const p = await api.http('POST', '/api/projects', { alias: 'demo', name: 'Demo', path: projDir, create: true });
    check('a project to work in', p.status < 300, p.body);
  }

  const configDir = mkdtempSync(join(tmpdir(), `crundi-native-config-${area}-`));
  const cefDir = join(tmpdir(), `crundi-native-cef-${area}`);
  const log = openSync(join(OUT, `native-${area}.log`), 'w');
  const app = spawn(BIN, [], {
    env: {
      ...process.env,
      CRUNDI_TEST_PORT: String(PORT), CRUNDI_CONFIG_DIR: configDir, CRUNDI_CEF_CACHE: cefDir,
      CRUNDI_OPEN_URL_FILE: join(configDir, 'open-url.txt'), CRUNDI_URL_A: 'about:blank', CRUNDI_URL_B: 'about:blank',
      SLINT_BACKEND: process.env.SLINT_BACKEND || 'winit-software',
    },
    stdio: ['ignore', log, log],
  });
  let exited = false;
  app.on('exit', () => { exited = true; });
  const d = await driver(PORT);

  const T = {
    area, SERVER, api, d, app, password, secret, projDir, OUT, configDir,
    send: (cmd) => d.send(cmd),
    /** A window action ({"cmd":"ui"}). */
    ui: (name, ...args) => d.send({ cmd: 'ui', name, args }),
    /** The area's own action: "<area>:<name>". */
    act: (name, ...args) => d.send({ cmd: 'ui', name: `${area}:${name}`, args }),
    // (The GPUI client's props have no "areas": the area's own filled in from {"cmd":"area"}, as `until` does.)
    props: async () => {
      const p = await d.send({ cmd: 'props' });
      if (!p.areas?.[area]) { const a = (await d.send({ cmd: 'area', area })).props; p.areas = { ...(p.areas || {}), [area]: a?.areas?.[area] ?? a }; }
      return p;
    },
    /** The area's props alone. */
    // (The GPUI client's part answers with { areas: { <area>: … } }.)
    mine: async () => { const p = (await d.send({ cmd: 'area', area })).props; return p?.areas?.[area] ?? p; },
    /** Wait until test(props) holds (props has .areas.<area>); a failed check otherwise. */
    async until(what, test, ms = 20000) {
      const end = Date.now() + ms;
      let last;
      while (Date.now() < end) {
        last = await d.send({ cmd: 'props' });
        // The GPUI client's props have no "areas" (each part's are merged at
        // the top): the area's own from {"cmd":"area"}, under the Slint name.
        if (!last.areas?.[area]) { const p = (await d.send({ cmd: 'area', area })).props; last.areas = { ...(last.areas || {}), [area]: p?.areas?.[area] ?? p }; }
        try { if (test(last)) { check(what, true); return last; } } catch { /* not yet */ }
        await sleep(250);
      }
      check(what, false, last && { area: last.areas?.[area], toast: last.toast, dialog: last.dialog, tab: last.currentTab });
      return null;
    },
    /** The pointer, as a person's (window px). */
    mouse: (kind, x, y, extra = {}) => d.send({ cmd: 'mouse', kind, x, y, ...extra }),
    key: (text, mods) => d.send({ cmd: 'key', text, ...(mods ? { mods } : {}) }),
    /** A screen for the review: native-client/screens/<area>/<name>.png. */
    async snap(name) {
      if (process.env.SCREENS === '0') return d.send({ cmd: 'snapshot', path: join(OUT, `${name}.png`) });
      mkdirSync(SCREENS, { recursive: true });
      return d.send({ cmd: 'snapshot', path: join(SCREENS, `${name}.png`) });
    },
    /** Answer the shared confirm / prompt, or pick a context-menu item. */
    dialog: (ok, value = '') => d.send({ cmd: 'ui', name: 'dialog-answer', args: [ok ? 'true' : 'false', value] }),
    menuPick: (id) => d.send({ cmd: 'ui', name: 'menu-pick', args: [id] }),
    async end() {
      try { d.close(); } catch { /* gone */ }
      if (!exited) app.kill();
      await sleep(500);
      try { rmSync(configDir, { recursive: true, force: true }); } catch { /* fine */ }
      console.log(`\n${area}: ${passed} passed, ${failed} failed`);
      process.exit(failed ? 1 : 0);
    },
  };

  // Sign in with the password and its code, then the project.
  await T.ui('sign-in-password', SERVER, password, totpNow(secret));
  await T.until('signed in', (p) => p.signedIn, 30000);
  if (project) {
    await T.ui('select-project', 'demo');
    await T.until('the project is open', (p) => p.currentProject === 'demo');
  }
  return T;
}

export const failures = () => failed;
