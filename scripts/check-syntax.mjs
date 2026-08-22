#!/usr/bin/env node
/**
 * check-syntax.mjs — parse every JS this app actually runs.
 *
 * `node --check` is not enough on its own here. webapp-html.js builds the whole
 * page as ONE template literal, so the browser JS inside it is, to Node, just a
 * string: a real SyntaxError in it (an `await` in a non-async click handler, a
 * stray backtick, an unclosed brace) passes `node --check` cleanly and then
 * takes the entire UI down at boot with a blank login screen. That has now
 * happened for real, so it gets a check of its own.
 *
 * Two passes:
 *   1. node --check over the server + vendor sources.
 *   2. render the page and parse each inline <script> body with new Function,
 *      which raises exactly the errors the browser would.
 *
 * Run: npm run check
 */

import { execFileSync } from 'node:child_process';
import { readdirSync, statSync } from 'node:fs';
import { join, dirname, extname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
let failures = 0;

function fail(where, err) {
  failures++;
  console.error(`  FAIL  ${where}\n        ${err}`);
}

// ─── Pass 1: every server-side and vendor source ───

function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    if (name === 'node_modules' || name.startsWith('.')) continue;
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (['.js', '.mjs', '.cjs'].includes(extname(name))) out.push(p);
  }
  return out;
}

const files = [
  ...walk(join(root, 'src')),
  ...walk(join(root, 'scripts')),
  join(root, 'app', 'vendor', 'claude-chat.js'),
];

for (const f of files) {
  try {
    execFileSync(process.execPath, ['--check', f], { stdio: 'pipe' });
  } catch (err) {
    fail(f.slice(root.length + 1), String(err.stderr || err.message).trim().split('\n').slice(0, 3).join(' '));
  }
}
console.log(`node --check: ${files.length} file(s)`);

// ─── Pass 2: the browser JS embedded in webapp-html.js ───

try {
  const mod = await import(pathToFileURL(join(root, 'src', 'webapp-html.js')).href);
  const html = mod.getWebappHtml({});
  const blocks = [...html.matchAll(/<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/gi)].map(m => m[1]);
  if (!blocks.length) fail('webapp-html.js', 'no inline <script> found — did the markup change?');
  blocks.forEach((code, i) => {
    // new Function parses the body under the same rules the browser applies to
    // a classic script, so `await` outside async, bad syntax etc. all surface.
    try { new Function(code); }
    catch (err) { fail(`webapp-html.js inline script #${i + 1}`, `${err.name}: ${err.message}`); }
  });
  console.log(`inline browser scripts: ${blocks.length} block(s)`);
} catch (err) {
  fail('webapp-html.js', `could not render the page: ${err.message}`);
}

if (failures) {
  console.error(`\n${failures} syntax problem(s) found.`);
  process.exit(1);
}
console.log('All JS parses.');
