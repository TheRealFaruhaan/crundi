#!/usr/bin/env node
// Sign in to a seeded test server (tools/test-server.sh <port>) and print the
// web UI's session, to put in a browser's localStorage (crundi_token,
// crundi_refresh) so the web UI opens signed in, for screenshots and
// measurements.
//
//   node native-client/tools/web-session.mjs [port]   ->  {"t": "...", "r": "..."}
//
// port: the first argument, or DEV_WEB_PORT, default 8889. The credentials
// come from /tmp/crundi-devtest-<port>/session.json.
import { totpNow } from '../../src/auth-password.js';
import { readFileSync } from 'node:fs';
const port = process.argv[2] || process.env.DEV_WEB_PORT || '8889';
const file = `/tmp/crundi-devtest-${port}/session.json`;
let s;
try { s = JSON.parse(readFileSync(file, 'utf8')); }
catch { console.error(`No ${file}: start the server with native-client/tools/test-server.sh ${port}`); process.exit(2); }
const r = await (await fetch(`http://localhost:${port}/api/auth/password`, {
  method: 'POST', headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ password: s.password, code: totpNow(s.secret) }),
})).json();
console.log(JSON.stringify({ t: r.token, r: r.refreshToken }));
