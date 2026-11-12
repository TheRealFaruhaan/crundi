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

// Shipped in part: only these files, and at runtime it loads only [deps] (its
// other dependencies are for its install script). Each file pattern must be in
// the allowlist, and its runtime code must require nothing else.
const PARTIAL = {
  'onnxruntime-node': { files: ['package.json', 'dist/**/*', 'bin/napi-v3/win32/x64/**/*'], deps: ['onnxruntime-common'] },
};

// ─── What the app loads ───
const builtins = new Set(builtinModules.flatMap((m) => [m, `node:${m}`]));
const roots = new Set();
// The app's own windows, and the "Hey Cru" spotter process (app/voice-wake, shipped whole).
import { readdirSync } from 'fs';
const entries = ['app/main.js', 'app/preload.js', ...readdirSync(join(root, 'app/voice-wake')).filter((f) => /\.(c?js|mjs)$/.test(f) && !f.startsWith('test-')).map((f) => 'app/voice-wake/' + f)];
for (const f of entries) {
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
check('the spotter process\'s voice check is scanned (onnxruntime-node)', roots.has('onnxruntime-node'), [...roots].join(','));

// ─── The allowlist ───
const yml = readFileSync(join(root, 'build/electron-builder-client.yml'), 'utf8');
check('node_modules is excluded wholesale first', /^\s*-\s*["']?!node_modules\/\*\*\/\*["']?\s*$/m.test(yml));
const allowed = [...yml.matchAll(/^\s*-\s*["']?node_modules\/((?:@[^/\s]+\/)?[^/\s"']+)\/\*\*\/\*["']?\s*$/gm)].map((m) => m[1]);
check('allowlist is not empty', allowed.length > 0);

// ─── The app's own files ───
// The client build lists app files one by one, so a module app/main.js
// imports that is not listed is left out of the installer and the client dies
// at startup with "Cannot find module". (app/browser-drawer.js nearly was.)
{
  const listed = yml.split('\n').map((l) => l.trim().replace(/^-\s*["']?|["']?$/g, ''));
  const covers = (rel) => listed.some((l) => l === rel || (l.endsWith('/**/*') && rel.startsWith(l.slice(0, -4))));
  const absent = [];
  for (const f of ['app/main.js']) {
    const src = readFileSync(join(root, f), 'utf8');
    for (const m of src.matchAll(/(?:\bfrom\s+|\bimport\(\s*)['"](\.\/[^'"]+)['"]/g)) {
      const rel = join(dirname(f), m[1]).split(sep).join('/');
      if (!covers(rel)) absent.push(rel);
    }
  }
  check('every local module app/main.js imports is in the client build', absent.length === 0, absent.join(', '));
}

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
  if (PARTIAL[name]) { for (const dep of PARTIAL[name].deps) walk(dep, dir, name, false); return; }
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
for (const [name, { files, deps }] of Object.entries(PARTIAL)) {
  const lines = files.map((f) => 'node_modules/' + name + '/' + f);
  const absent = lines.filter((l) => !yml.split('\n').some((y) => y.trim().replace(/^-\s*["']?|["']?$/g, '') === l));
  check(name + ': its files are in the allowlist', absent.length === 0, absent.join(', '));
  if (!absent.length) allowedSet.add(name);
  // What its runtime code requires: only [deps] (and itself, and its binaries).
  const dist = join(root, 'node_modules', name, 'dist');
  const req = new Set();
  if (existsSync(dist)) for (const f of readdirSync(dist).filter((f) => f.endsWith('.js'))) for (const m of readFileSync(join(dist, f), 'utf8').matchAll(/require\(\s*['"`]([^'"`]+)['"`]/g)) if (!m[1].startsWith('.') && !builtins.has(m[1])) req.add(m[1]);
  const extra = [...req].filter((r) => !deps.includes(r));
  check(name + ': at runtime it loads only ' + deps.join(', '), extra.length === 0, extra.join(', '));
}
const uncovered = [...reached].filter(([rel]) => !allowedSet.has(nameOf(rel))).map(([rel, via]) => `${nameOf(rel)} (at ${rel}, via ${via})`);
check('allowlist names every package the client loads', uncovered.length === 0,
  `\n  add to build/electron-builder-client.yml files, as node_modules/<name>/**/*:\n    ${uncovered.join('\n    ')}`);

// Not a failure, but a stale entry is worth knowing about.
const usedNames = new Set([...reached.keys()].map(nameOf));
const unused = allowed.filter((a) => !usedNames.has(a));
if (unused.length) console.log(`note allowlisted but not reached here: ${unused.join(', ')}`);

console.log(`\n${failed ? failed + ' failed' : 'all passed'} (${reached.size} packages reached from ${[...roots].join(', ')})`);
process.exit(failed ? 1 : 0);
