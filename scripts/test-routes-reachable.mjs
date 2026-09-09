#!/usr/bin/env node
/**
 * test-routes-reachable.mjs — every handler has a route, every call has a handler.
 *
 * The ui-sessions endpoints are dispatched through ONE regex allowlist and then
 * a chain of `action === '...'` checks. Adding the handler without adding it to
 * the allowlist produces a feature that is fully implemented, fully unit-tested
 * and completely unreachable: the path never matches, so the handler is dead
 * code and the button silently does nothing.
 *
 * That is exactly how message recall shipped in 1.15.0. `cancelMessage` was
 * correct, the CLI protocol was verified against the real binary, the unit tests
 * passed — and clicking the drawer did nothing at all, because `cancel-queued`
 * was missing from the allowlist. Nothing in the suite touched the HTTP route.
 *
 * Run: node scripts/test-routes-reachable.mjs
 */

import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const webapp = readFileSync(join(root, 'src', 'webapp.js'), 'utf8');
const chat = readFileSync(join(root, 'app', 'vendor', 'claude-chat.js'), 'utf8');
const html = readFileSync(join(root, 'src', 'webapp-html.js'), 'utf8');

let failures = 0;
const check = (label, ok, detail) => {
  if (!ok) failures++;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${label}${detail === undefined ? '' : ` -> ${detail}`}`);
};

// The allowlist itself.
const m = webapp.match(/\/\^\\\/api\\\/ui-sessions\\\/\(\[\^\/\]\+\)\\\/\(([^)]+)\)\$\//);
check('found the ui-sessions route allowlist', !!m);
const allowed = new Set((m ? m[1] : '').split('|'));

// Every action the server actually handles, taken from the dispatch chain that
// sits under that route.
// Bounded to the ui-sessions dispatch block. Unbounded, this walks on into the
// service/auth routers, whose actions live behind their own paths entirely.
const uiStart = webapp.indexOf('const uiMatch = path.match(');
const after = webapp.indexOf("\n    if (path === '", uiStart);
const body = webapp.slice(uiStart, after > uiStart ? after : undefined);
const handled = new Set();
for (const mm of body.matchAll(/action === '([a-z-]+)'/g)) handled.add(mm[1]);
check('found the action handlers', handled.size > 3, handled.size);

for (const a of handled) {
  check(`handler "${a}" is reachable`, allowed.has(a), allowed.has(a) ? '' : 'MISSING from the route allowlist');
}

// And every action the clients actually call.
const called = new Set();
for (const src of [chat, html]) {
  for (const mm of src.matchAll(/ui-sessions\/'\s*\+\s*encodeURIComponent\([^)]*\)\s*\+\s*'\/([a-z-]+)'/g)) called.add(mm[1]);
  for (const mm of src.matchAll(/ui-sessions\/\$\{[^}]+\}\/([a-z-]+)/g)) called.add(mm[1]);
}
check('found client calls', called.size > 0, [...called].join(','));
for (const a of called) {
  check(`client call "${a}" has a route`, allowed.has(a), allowed.has(a) ? '' : 'MISSING from the route allowlist');
}

// The specific one this test was written for.
check('cancel-queued is reachable', allowed.has('cancel-queued'));

if (failures) {
  console.error(`\n${failures} failure(s).`);
  process.exit(1);
}
console.log('\nNo handler is stranded behind a route that cannot match it.');
