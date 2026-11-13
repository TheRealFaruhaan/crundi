#!/usr/bin/env node
// Sign in to the test server (tools/test-server.sh) and print the web UI's
// session, to put in a browser's localStorage (crundi_token, crundi_refresh)
// so the web UI opens signed in, for screenshots and measurements.
//
//   node native-client/tools/web-session.mjs   ->  {"t": "...", "r": "..."}
import { totpNow } from '../../src/auth-password.js';
import { readFileSync } from 'node:fs';
const s = JSON.parse(readFileSync('/tmp/crundi-devtest/session.json', 'utf8'));
const r = await (await fetch('http://localhost:8889/api/auth/password', {
  method: 'POST', headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ password: s.password, code: totpNow(s.secret) }),
})).json();
console.log(JSON.stringify({ t: r.token, r: r.refreshToken }));
