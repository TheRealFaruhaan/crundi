#!/usr/bin/env node
// Lay out one install folder (package/CONTRACT.md, "Install layout"), the
// same on Linux and Windows, for the .deb and the NSIS installer to wrap:
//
//   crundi[.exe] | crundi-client[.exe]   the native client (crundi-v2, renamed)
//   crundi-browser-host[.exe] + CEF's files
//   crundi-worker[.exe]                  the worker from worker/
//   node/                                Node 22 (Linux node/bin/node, Windows node/node.exe); all-in-one only
//   server/                              src/, app/vendor/ and app/homepool-node.js, scripts/, skills/, assets/, package.json,
//                                        .env.example and the production node_modules (built for that Node)
//
//   node native-v2/package/stage.mjs --out DIR --edition full|client
//        --bin DIR      the native release folder (crundi-v2, crundi-browser-host)
//        --cef DIR      CEF's distribution (fetch-cef.mjs)
//        --worker FILE  the built crundi-worker
//        --node DIR     an unpacked Node 22 for the target OS (all-in-one)
//        [--modules DIR] a server/node_modules already installed (skips npm ci)
//
// The edition is only whether server/ exists (CONTRACT.md, "Editions").

import { cpSync, existsSync, mkdirSync, readdirSync, rmSync, copyFileSync, chmodSync, statSync } from 'node:fs';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const args = {};
for (let i = 2; i < process.argv.length; i += 2) args[process.argv[i].replace(/^--/, '')] = process.argv[i + 1];
const need = (k) => { if (!args[k]) { console.error(`stage.mjs: --${k} is required`); process.exit(2); } return resolve(args[k]); };

const win = process.platform === 'win32';
const exe = (n) => (win ? `${n}.exe` : n);
const out = need('out');
const edition = args.edition || 'full';
const full = edition === 'full';
const bin = need('bin');
const cef = need('cef');
const worker = need('worker');

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });

// The binaries. Linux release builds carry their debug info (588 MB for the
// client): strip the copies, never the build folder's own.
const put = (from, to) => {
  if (!win && process.env.STAGE_NO_STRIP !== '1') {
    try { execFileSync('strip', ['--strip-unneeded', '-o', to, from]); chmodSync(to, 0o755); return; } catch { /* no strip: copy */ }
  }
  copyFileSync(from, to);
  if (!win) chmodSync(to, 0o755);
};
put(join(bin, exe('crundi-v2')), join(out, exe(full ? 'crundi' : 'crundi-client')));
put(join(bin, exe('crundi-browser-host')), join(out, exe('crundi-browser-host')));
put(worker, join(out, exe('crundi-worker')));

// CEF next to the host, as native-gpui/scripts/stage-cef.mjs stages it (real copies).
const skip = new Set(['include', 'cmake', 'libcef_dll', 'CMakeLists.txt', 'CREDITS.html', 'archive.json', 'README.txt', 'LICENSE.txt',
  // The browser host runs CEF with no_sandbox (browser-host/src/host.rs): no setuid helper.
  'chrome-sandbox']);
for (const f of readdirSync(cef)) {
  if (skip.has(f) || /\.lib$/i.test(f) || /^cef_sandbox/i.test(f)) continue;
  cpSync(join(cef, f), join(out, f), { recursive: true, dereference: true });
  // CEF's Linux libraries come with their symbols (libcef.so is 1.4 GB): strip the copies.
  if (!win && /\.so(\.\d+)*$/.test(f) && process.env.STAGE_NO_STRIP !== '1') {
    try { execFileSync('strip', ['--strip-unneeded', join(out, f)]); } catch { /* no strip */ }
  }
}
// Only the English UI locale, as Electron's packages ship (electronLanguages).
const locales = join(out, 'locales');
if (existsSync(locales)) for (const f of readdirSync(locales)) if (!/^en-US\./.test(f)) rmSync(join(locales, f), { recursive: true, force: true });

if (full) {
  // Node 22 for the server.
  const nodeSrc = need('node');
  if (win) { mkdirSync(join(out, 'node')); copyFileSync(join(nodeSrc, 'node.exe'), join(out, 'node', 'node.exe')); }
  else { mkdirSync(join(out, 'node', 'bin'), { recursive: true }); put(join(nodeSrc, 'bin', 'node'), join(out, 'node', 'bin', 'node')); }
  for (const f of ['LICENSE']) if (existsSync(join(nodeSrc, f))) copyFileSync(join(nodeSrc, f), join(out, 'node', f));

  // The server, as Electron's package carries it (package.json "files" and extraResources).
  const server = join(out, 'server');
  mkdirSync(server);
  const notTests = (src) => !/[\\/]app[\\/]voice-wake[\\/]test-[^\\/]*\.mjs$/.test(src) && !/[\\/]\.env$/.test(src);
  for (const d of ['src', 'scripts', 'skills', 'assets']) cpSync(join(repo, d), join(server, d), { recursive: true, filter: notTests });
  cpSync(join(repo, 'app', 'vendor'), join(server, 'app', 'vendor'), { recursive: true });
  // The server imports the homepool frame format from app/ (src/homepool.js, homepool-service.js).
  copyFileSync(join(repo, 'app', 'homepool-node.js'), join(server, 'app', 'homepool-node.js'));
  for (const f of ['package.json', 'package-lock.json', '.env.example']) copyFileSync(join(repo, f), join(server, f));

  if (args.modules) {
    cpSync(resolve(args.modules), join(server, 'node_modules'), { recursive: true, verbatimSymlinks: true });
  } else {
    // The production node_modules, native modules built by this Node (22):
    // node-pty and node-screenshots run in the server, never in Electron.
    const npm = win ? 'npm.cmd' : 'npm';
    execFileSync(npm, ['ci', '--omit=dev', '--no-audit', '--no-fund'], { cwd: server, stdio: 'inherit', shell: win });
  }
  // The payloads npm can quietly fail to fetch (release.yml has the story).
  const payloads = win
    ? ['node_modules/cloudflared/bin/cloudflared.exe', 'node_modules/node-pty/prebuilds/win32-x64/winpty-agent.exe']
    : ['node_modules/cloudflared/bin/cloudflared'];
  for (const p of payloads) {
    const f = join(server, p);
    if (!existsSync(f) || statSync(f).size === 0) { console.error(`stage.mjs: missing ${p}`); process.exit(1); }
  }
}
console.log(out);
