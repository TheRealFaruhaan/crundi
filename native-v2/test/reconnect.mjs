// FINDINGS T14 end to end: the server restarts under an open chat (a
// Cloudflare-style 521 while it is down) and the chat carries on live with no
// user action. A stub server (no real Claude): an open chat with a streamed
// reply; the server killed by its PID; for a while every request and /ws
// upgrade on its port answers 521 (the origin down behind Cloudflare); then
// the same server (its data, environment and folder) started again. The
// client shows "reconnecting" while down and comes back to "connected" by
// itself; a message sent to the chat (through the API, not the client) before
// the client is back appears in the open chat, its streamed reply too; then a
// message from the client's own box streams live. Every attempt is logged.
//
//   native-v2/tools/stub-server.sh 4550
//   CRUNDI_TEST_SERVER=http://localhost:4550 CRUNDI_TEST_PORT=4551 \
//     dbus-run-session -- xvfb-run -a -s "-screen 0 1440x900x24" node native-v2/test/reconnect.mjs
//   kill $(cat /tmp/crundi-devtest-4550/server.pid)
import * as t from './lib.mjs';
import { totpNow } from '../../src/auth-password.js';
import http from 'node:http';
import { spawn } from 'node:child_process';
import { readFileSync, writeFileSync, readlinkSync, openSync, existsSync } from 'node:fs';
import { join } from 'node:path';

const T = await t.begin('reconnect');
const port = Number(new URL(process.env.CRUNDI_TEST_SERVER).port);
const dir = `/tmp/crundi-devtest-${port}`;
const badge = (st) => st.conn?.badge;
const texts = (st) => (st.entries || []).map((e) => e.text || '').join('\n');

await T.ui('new-chat');
await T.until('a chat pane', (st) => st.chat && st.panes?.length > 0, 30000);
await T.until('the chat is idle', (st) => st.chatState === 'idle', 30000);
const chat = (await T.state()).chat;
await T.ui('composer-text', 'first before the restart');
await T.ui('composer-send');
await T.until('the reply streams live before the restart', (st) => /Reply \d/.test(texts(st)), 30000);
await T.until('…and ends', (st) => st.composer?.state === 'idle', 60000);
t.check('connected before the restart', badge(await T.state()) === 'connected', (await T.state()).conn);

// The server's process: its command, folder and environment, then killed by PID.
const pid = Number(readFileSync(join(dir, 'server.pid'), 'utf8'));
const argv = readFileSync(`/proc/${pid}/cmdline`, 'utf8').split('\0').filter(Boolean);
const cwd = readlinkSync(`/proc/${pid}/cwd`);
const env = Object.fromEntries(readFileSync(`/proc/${pid}/environ`, 'utf8').split('\0').filter(Boolean).map((l) => { const i = l.indexOf('='); return [l.slice(0, i), l.slice(i + 1)]; }));
process.kill(pid);
for (let i = 0; i < 40 && existsSync(`/proc/${pid}`); i++) await t.sleep(250);
t.check('the server stopped (by its PID)', !existsSync(`/proc/${pid}`));

// The origin down behind Cloudflare: 521 for everything, the upgrade too.
const down = http.createServer((q, r) => { r.writeHead(521, { 'Content-Type': 'text/plain' }); r.end('error code: 521'); });
down.on('upgrade', (q, sock) => { sock.end('HTTP/1.1 521 Origin Down\r\nContent-Length: 15\r\nConnection: close\r\n\r\nerror code: 521'); });
await new Promise((res) => down.listen(port, '0.0.0.0', res));
await T.until('the badge says reconnecting while the server is down', (st) => badge(st) === 'reconnecting', 20000);
await t.sleep(6000);
const log = () => { try { return readFileSync(join(T.configDir, 'crundi.log'), 'utf8'); } catch { return ''; } };
const tries = (log().match(/live: cannot connect: .*521/g) || []).length;
t.check('each attempt logged, 521 among them', tries >= 2, tries);
t.check('…still reconnecting', badge(await T.state()) === 'reconnecting');
await new Promise((res) => down.close(res));
down.closeAllConnections?.();

// The same server again: its data, folder and environment.
const out = openSync(join(dir, 'server.log'), 'a');
const again = spawn(argv[0], argv.slice(1), { cwd, env, detached: true, stdio: ['ignore', out, out] });
again.unref();
writeFileSync(join(dir, 'server.pid'), String(again.pid));
let up = false;
for (let i = 0; i < 120 && !up; i++) { try { up = (await fetch(`${process.env.CRUNDI_TEST_SERVER}/api/auth/methods`)).status < 500; } catch { /* not yet */ } if (!up) await t.sleep(250); }
t.check('the server is back (started again with its data)', up);

// The test's own API signs in again (its setup token did not outlive the restart).
for (let i = 0; i < 2; i++) {
  const r = await T.api.http('POST', '/api/auth/password', { password: T.password, code: totpNow(T.secret) });
  if (r.body?.token) { T.api.setToken(r.body.token); break; }
  await t.sleep(31000 - (Date.now() % 30000));
}
await T.until('connected again by itself (no user action)', (st) => badge(st) === 'connected', 60000);
t.check('…the success logged', /live: connected again after \d+ failed attempt/.test(log()));
// The server parked the chat across its restart (src/panes.js, as the web
// shows it): the open chat says so by itself, from the live link alone.
await T.until('the open chat shows the server\'s parked card by itself', (st) => st.log?.parked?.why, 30000);
await T.ui('park-resume', 'resume');
await T.until('Resume: the chat is live again', (st) => !st.log?.parked && st.chatState === 'idle', 60000);

// Sent from elsewhere (the API, as another device): it shows live.
const sent = await T.api.http('POST', `/api/ui-sessions/${chat}/send`, { text: 'second from another device' });
t.check('a message sent to the chat from elsewhere', sent.status < 300 && sent.body?.ok !== false, sent.body);
await T.until('it shows live in the open chat', (st) => texts(st).includes('second from another device'), 30000);
await T.until('…and its reply streams in live', (st) => (texts(st).match(/Reply \d/g) || []).length >= 2, 60000);
await T.until('…which ends', (st) => st.composer?.state === 'idle' && st.chatState === 'idle', 60000);

// The box: a message from the client streams live after the restart.
const n0 = (texts(await T.state()).match(/Reply \d/g) || []).length;
await T.ui('composer-text', 'third after the restart');
await T.ui('composer-send');
await T.until('the client\'s own message shows', (st) => texts(st).includes('third after the restart'), 30000);
await T.until('…and its reply streams in live', (st) => (texts(st).match(/Reply \d/g) || []).length > n0, 60000);
t.check('the same chat throughout', (await T.state()).chat === chat);
await T.end();
