// The client-only installer ships node_modules as an ALLOWLIST
// (build/electron-builder-client.yml). That keeps server packages and their
// dependencies out, but it also means a dependency electron-updater grows in a
// future release would silently be left behind, and the installed client would
// die at startup with "Cannot find module". This walks what the desktop app
// actually loads, through the installed tree, and fails if the allowlist does
// not cover every package it reaches.
import { readFileSync, existsSync } from 'fs';
import { join, dirname, relative, sep } from 'path';
import { builtinModules } from 'module';
import { fileURLToPath } from 'url';

const root = join(fileURLToPath(new URL('.', import.meta.url)), '..');
let failed = 0;
const check = (name, ok, extra = '') => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${ok ? '' : ' ' + extra}`);
  if (!ok) failed++;
};

// Loaded by app/main.js only inside try/catch, and only meaningful when the
// server is bundled - which the client build never is.
const SERVER_ONLY = new Set(['node-pty']);

// ─── What the app loads ───
const builtins = new Set(builtinModules.flatMap((m) => [m, `node:${m}`]));
const roots = new Set();
for (const f of ['app/main.js', 'app/preload.js']) {
  const src = readFileSync(join(root, f), 'utf8');
  const re = /(?:\bfrom\s+|\brequire\(\s*|\bimport\(\s*)['"]([^'"]+)['"]/g;
  for (const m of src.matchAll(re)) {
    const spec = m[1];
    if (spec.startsWith('.') || spec.startsWith('/') || builtins.has(spec) || spec === 'electron') continue;
    const name = spec.startsWith('@') ? spec.split('/').slice(0, 2).join('/') : spec.split('/')[0];
    if (!SERVER_ONLY.has(name)) roots.add(name);
  }
}
check('app loads electron-updater (sanity: the scan finds imports)', roots.has('electron-updater'), [...roots].join(','));

// ─── The allowlist ───
const yml = readFileSync(join(root, 'build/electron-builder-client.yml'), 'utf8');
check('node_modules is excluded wholesale first', /^\s*-\s*["']?!node_modules\/\*\*\/\*["']?\s*$/m.test(yml));
const allowed = [...yml.matchAll(/^\s*-\s*["']?node_modules\/((?:@[^/\s]+\/)?[^/\s"']+)\/\*\*\/\*["']?\s*$/gm)].map((m) => m[1]);
check('allowlist is not empty', allowed.length > 0);

// ─── Everything reachable, resolved the way Node resolves it ───
function resolvePkg(name, fromDir) {
  let d = fromDir;
  for (;;) {
    const p = join(d, 'node_modules', name, 'package.json');
    if (existsSync(p)) return p;
    const up = dirname(d);
    if (up === d) return null;
    d = up;
  }
}
const reached = new Map(); // relative dir -> via
const missing = [];
function walk(name, fromDir, via, optional) {
  const pj = resolvePkg(name, fromDir);
  if (!pj) { if (!optional) missing.push(`${name} (needed by ${via})`); return; }
  const dir = dirname(pj);
  const rel = relative(root, dir).split(sep).join('/');
  if (reached.has(rel)) return;
  reached.set(rel, via);
  const j = JSON.parse(readFileSync(pj, 'utf8'));
  for (const dep of Object.keys(j.dependencies || {})) walk(dep, dir, name, false);
  for (const dep of Object.keys(j.optionalDependencies || {})) walk(dep, dir, name, true);
}
for (const r of roots) walk(r, root, 'app', false);
check('every dependency is installed', missing.length === 0, missing.join(', '));

// Compare by PACKAGE NAME, not by where npm happened to put it. electron-builder
// re-hoists the tree when packaging, so electron-updater/node_modules/fs-extra
// in the source tree ships as node_modules/fs-extra - and is filtered out unless
// fs-extra itself is on the list. A path-based check passed while the packaged
// client failed on "Cannot find module 'fs-extra'".
const nameOf = (rel) => {
  const parts = rel.split('node_modules/').pop().split('/');
  return parts[0].startsWith('@') ? `${parts[0]}/${parts[1]}` : parts[0];
};
const allowedSet = new Set(allowed);
const uncovered = [...reached].filter(([rel]) => !allowedSet.has(nameOf(rel))).map(([rel, via]) => `${nameOf(rel)} (at ${rel}, via ${via})`);
check('allowlist names every package the client loads', uncovered.length === 0,
  `\n  add to build/electron-builder-client.yml files, as node_modules/<name>/**/*:\n    ${uncovered.join('\n    ')}`);

// Not a failure, but a stale entry is worth knowing about.
const usedNames = new Set([...reached.keys()].map(nameOf));
const unused = allowed.filter((a) => !usedNames.has(a));
if (unused.length) console.log(`note allowlisted but not reached here: ${unused.join(', ')}`);

console.log(`\n${failed ? failed + ' failed' : 'all passed'} (${reached.size} packages reached from ${[...roots].join(', ')})`);
process.exit(failed ? 1 : 0);
