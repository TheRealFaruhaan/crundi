#!/usr/bin/env node
// Phase 1, end to end: the native client against a real Crundi server, driven
// through its test driver (src/testctl.rs), the same actions as its buttons.
//
//   1. password sign-in (the authenticator code from the server's secret)
//   2. projects and chats from the server; a new chat; a message and Claude's reply
//   3. an approval: a shell command that asks first, allowed from the client
//   4. sign out, then pairing with a link from a signed-in session
//   5. a restart picks the saved login up again
//
// Needs a FRESH test server with no sign-in set up (this sets a password on
// it), started through tools/test-server.sh <port> --bare, never by hand; it
// refuses a server with a Telegram bot configured:
//
//   native-client/tools/test-server.sh 4811 --bare
//   CRUNDI_TEST_SERVER=http://localhost:4811 NATIVE_BIN=… DISPLAY=… node native-client/test/phase1.mjs
//
// Pictures go to DRIVE_OUT (default: the temp dir).

import net from 'node:net';
import { spawn } from 'node:child_process';
import { mkdtempSync, mkdirSync, openSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { totpNow } from '../../src/auth-password.js';

const SERVER = process.env.CRUNDI_TEST_SERVER || 'http://localhost:8889';
const BIN = process.env.NATIVE_BIN;
const PORT = Number(process.env.CRUNDI_TEST_PORT || 7612);
const OUT = process.env.DRIVE_OUT || tmpdir();
const CLAUDE = process.env.SKIP_CLAUDE ? false : true;   // the parts that run a real Claude
mkdirSync(OUT, { recursive: true });
if (!BIN) { console.error('Set NATIVE_BIN'); process.exit(2); }

let failed = 0;
const check = (name, ok, extra = '') => { console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${ok ? '' : '  ' + JSON.stringify(extra).slice(0, 400)}`); if (!ok) failed++; return ok; };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function http(method, path, body, token) {
  const res = await fetch(SERVER + path, {
    method,
    headers: { 'Content-Type': 'application/json', ...(token ? { Authorization: 'Bearer ' + token } : {}) },
    body: body ? JSON.stringify(body) : undefined,
  });
  return { status: res.status, body: await res.json().catch(() => null) };
}

// ─── The server: a password, a project ───
const PASSWORD = 'native-client-test-' + Math.random().toString(36).slice(2, 10);
const methods = await http('GET', '/api/auth/methods');
// Never against a server running the LIVE Telegram bot (checked again below,
// once signed in): a test server comes from tools/test-server.sh <port> --bare.
if (methods.body?.telegram || methods.body?.telegramBotId) {
  console.error(`Refusing ${SERVER}: it has a Telegram bot configured. Start a test server with native-client/tools/test-server.sh <port> --bare.`);
  process.exit(2);
}
if (!methods.body?.setupRequired) { console.error('This needs a fresh dev server (no sign-in set up yet)'); process.exit(2); }
const setup = await http('POST', '/api/auth/setup', { method: 'password', password: PASSWORD });
check('the server takes a password', setup.body?.ok && setup.body.totpSecret, setup.body);
const SECRET = setup.body.totpSecret;
let token = setup.body.token;

// No bot behind the sign-in either (an empty .env, no TELEGRAM_BOT_TOKEN).
const status = await http('GET', '/api/status', null, token);
if (status.body?.botConfigured !== false) {
  console.error(`Refusing ${SERVER}: it has a Telegram bot configured (botConfigured=${status.body?.botConfigured}). Start a test server with native-client/tools/test-server.sh <port> --bare.`);
  process.exit(2);
}
const proj = await http('POST', '/api/projects', { alias: 'demo', name: 'Demo', path: process.env.CRUNDI_TEST_PROJECT || join(tmpdir(), 'crundi-native-demo'), create: true }, token);
check('a project to work in', proj.status < 300, proj.body);

// ─── The app and its driver ───
const configDir = mkdtempSync(join(tmpdir(), 'crundi-native-config-'));
// The app writes the browser address here instead of opening a browser.
const URL_FILE = join(configDir, 'open-url.txt');
function launch(tag) {
  const log = openSync(join(OUT, `native-${tag}.log`), 'w');
  const app = spawn(BIN, [], { env: { ...process.env, CRUNDI_TEST_PORT: String(PORT), CRUNDI_CONFIG_DIR: configDir, CRUNDI_OPEN_URL_FILE: URL_FILE, CRUNDI_URL_A: 'about:blank', CRUNDI_URL_B: 'about:blank' }, stdio: ['ignore', log, log] });
  return app;
}

function driver() {
  return new Promise((resolve, reject) => {
    const tryConnect = (left) => {
      const sock = net.connect(PORT, '127.0.0.1');
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

async function until(d, what, test, ms = 30000) {
  const end = Date.now() + ms;
  let last;
  while (Date.now() < end) {
    const [props, state] = await Promise.all([d.send({ cmd: 'props' }), d.send({ cmd: 'state' })]);
    last = { props, state: state.state };
    if (test(last)) return last;
    await sleep(400);
  }
  check(what, false, last && { props: last.props, chat: last.state?.chat, entries: last.state?.entries?.slice(-4) });
  return null;
}

const ui = (d, name, ...args) => d.send({ cmd: 'ui', name, args });
const snap = (d, name) => d.send({ cmd: 'snapshot', path: join(OUT, `phase1-${name}.png`) });

let app = launch('run1');
let d = await driver();

// 1. Password sign-in.
await ui(d, 'check-server', SERVER);
await until(d, 'the server offers a password', (x) => x.props.offerPassword && x.props.signinChecked);
await snap(d, '1-signin');
await ui(d, 'sign-in-password', SERVER, PASSWORD, totpNow(SECRET));
const signed = await until(d, 'signed in with password and code', (x) => x.props.signedIn);
if (signed) check('signed in with password and code', true);
await until(d, 'live link up', (x) => x.props.online, 15000) && check('live link up', true);

// 2. Projects, a new chat, a message.
const withProject = await until(d, 'the project list comes from the server', (x) => x.state.projects.includes('demo'));
if (withProject) check('the project list comes from the server', true);
await ui(d, 'select-project', 'demo');
await ui(d, 'new-chat');
const chatOpen = await until(d, 'a new chat opens', (x) => x.state.chat && x.state.chats.some((c) => c.id === x.state.chat));
if (chatOpen) check('a new chat opens', true);

if (CLAUDE && chatOpen) {
  await ui(d, 'send', 'Reply with exactly the words: hello from native. Do not use any tools.');
  const replied = await until(d, 'Claude answers in the chat', (x) => x.state.entries.some((e) => e.kind === 'assistant' && /hello from native/i.test(e.text)) && x.state.chatState !== 'working', 180000);
  if (replied) check('Claude answers in the chat', true);
  if (replied) check('the user message shows', replied.state.entries.some((e) => e.kind === 'user' && /hello from native/.test(e.text)));
  await snap(d, '2-chat');

  // 3. An approval: this chat asks before shell commands.
  const mode = await http('POST', `/api/ui-sessions/${chatOpen.state.chat}/permission-mode`, { mode: 'default' }, token);
  check('the chat asks before tools', mode.status < 300, mode.body);
  await ui(d, 'send', 'Run this exact shell command with the Bash tool and nothing else: printf native-ok-42 > approval-test.txt && cat approval-test.txt');
  const asked = await until(d, 'the approval shows', (x) => x.state.entries.some((e) => e.kind === 'permission' && e.status === 'pending') || x.state.entries.some((e) => e.kind === 'tool' && /native-ok-42/.test(e.text)), 180000);
  const pending = asked?.state.entries.find((e) => e.kind === 'permission' && e.status === 'pending');
  if (pending) {
    check('the approval shows', true);
    await snap(d, '3-approval');
    await ui(d, 'respond', pending.requestId, 'allow', 'false');
  } else if (asked) {
    console.log('  (the command ran without asking: this server allows it)');
  }
  const ran = await until(d, 'the command runs and its output shows', (x) => x.state.entries.some((e) => e.kind === 'tool' && /native-ok-42/.test(e.text)) && x.state.chatState !== 'working', 180000);
  if (ran) check('the command runs and its output shows', true);
  await snap(d, '4-tool');
}

// 4. Sign out; pair with a link instead.
await ui(d, 'sign-out');
await until(d, 'signed out', (x) => !x.props.signedIn) && check('signed out', true);
// The server will not make a link for "localhost" (a phone could not reach
// it), so ask for one at a network address, then point it back here.
const pair = await http('POST', '/api/pair/start', { origin: 'http://10.0.0.2:8889' }, token);
check('a pairing link from a signed-in session', pair.body?.ok && pair.body.link, pair.body);
const link = SERVER + '/pair#' + String(pair.body?.link || '').split('#')[1];
await ui(d, 'sign-in-pair', link);
await until(d, 'signed in by pairing', (x) => x.props.signedIn) && check('signed in by pairing', true);
await snap(d, '5-paired');

// 4b. Through the browser: the app opens the server's page, the person signs in
// there and allows it (here: the page's "Allow", through the API), and the
// app collects a login of its own.
await ui(d, 'sign-out');
await until(d, 'signed out again', (x) => !x.props.signedIn) && check('signed out again', true);
await ui(d, 'check-server', SERVER);
await until(d, 'the login step again', (x) => x.props.signinChecked);
await ui(d, 'sign-in-browser', SERVER, 'telegram');
const waiting = await until(d, 'the app shows the code to compare', (x) => x.props.oauthCode !== '');
let pageUrl = '';
for (let i = 0; i < 20 && !pageUrl; i++) { try { pageUrl = readFileSync(URL_FILE, 'utf8'); } catch { await sleep(250); } }
check('the app opens the server page for it', /\/\?app=[a-f0-9]{24}$/.test(pageUrl), pageUrl);
const appId = (pageUrl.match(/app=([a-f0-9]{24})/) || [])[1];
const info = await http('POST', '/api/auth/app/info', { id: appId }, token);
check('the page sees this computer and the same code', waiting && info.body?.ok && info.body.confirmCode === waiting.props.oauthCode, [info.body, waiting?.props.oauthCode]);
const allowed = await http('POST', '/api/auth/app/approve', { id: appId }, token);
check('allowing it on the page', allowed.body?.ok, allowed.body);
await until(d, 'signed in through the browser', (x) => x.props.signedIn) && check('signed in through the browser', true);
await snap(d, '5b-browser');

// 5. A restart picks the login up again.
d.close();
app.kill();
await sleep(1500);
app = launch('run2');
d = await driver();
const back = await until(d, 'a restart signs in from the saved login', (x) => x.props.signedIn && x.state.projects.includes('demo'), 30000);
if (back) check('a restart signs in from the saved login', true);
await snap(d, '6-restart');

d.close();
app.kill();
console.log(failed ? `${failed} failed` : 'all passed');
setTimeout(() => process.exit(failed ? 1 : 0), 500);
