#!/usr/bin/env node
// Download CEF for this machine and lay it out the way cef-rs's build expects
// (CEF_PATH), the same as download-cef does. download-cef 3.0.0 cannot do it
// itself: CEF's index.json is over its 10 MB response limit.
//
//   node scripts/fetch-cef.mjs [outDir]     prints the CEF_PATH to use
//
// outDir defaults to ~/.local/share/cef. The version is the one cef-rs was built
// for (CEF_VERSION below, from the cef crate's version).

import { createHash } from 'node:crypto';
import { createWriteStream, existsSync, mkdirSync, readdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { join } from 'node:path';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { execFileSync } from 'node:child_process';

const CEF_VERSION = '156.0.26+g8ea0c73+chromium-156.0.8078.88';
const CDN = process.env.CEF_DOWNLOAD_URL || 'https://cef-builds.spotifycdn.com';

const arch = process.arch === 'arm64' ? 'arm64' : 'x64';
const plat = { linux: `linux${arch === 'x64' ? '64' : 'arm64'}`, win32: `windows${arch === 'x64' ? '64' : 'arm64'}`, darwin: `macos${arch === 'x64' ? 'x64' : 'arm64'}` }[process.platform];
const dirName = `cef_${{ linux: 'linux', win32: 'windows', darwin: 'macos' }[process.platform]}_${arch === 'x64' ? 'x86_64' : 'aarch64'}`;
const out = process.argv[2] || join(homedir(), '.local', 'share', 'cef');
const cefDir = join(out, dirName);

if (existsSync(join(cefDir, 'archive.json')) && JSON.parse(readFileSync(join(cefDir, 'archive.json'), 'utf8')).name.includes(CEF_VERSION)) {
  console.log(cefDir);
  process.exit(0);
}

const index = await (await fetch(`${CDN}/index.json`)).json();
const version = (index[plat]?.versions || []).find((v) => v.cef_version === CEF_VERSION);
if (!version) throw new Error(`CEF ${CEF_VERSION} is not in the index for ${plat}`);
const file = version.files.find((f) => f.type === 'minimal');
console.error(`downloading ${file.name} (${Math.round(file.size / 1e6)} MB)`);

const work = join(tmpdir(), `cef-fetch-${process.pid}`);
mkdirSync(work, { recursive: true });
const archive = join(work, file.name);
const res = await fetch(`${CDN}/${encodeURIComponent(file.name)}`);
if (!res.ok) throw new Error(`download failed: ${res.status}`);
await pipeline(Readable.fromWeb(res.body), createWriteStream(archive));
const sha1 = createHash('sha1').update(readFileSync(archive)).digest('hex');
if (sha1 !== file.sha1) throw new Error(`sha1 mismatch: ${sha1} != ${file.sha1}`);

console.error('extracting');
// Windows' own tar (bsdtar): Git's GNU tar, first on PATH in Git Bash, reads
// "C:\\…" as a remote host.
const tar = process.platform === 'win32' ? join(process.env.SystemRoot || 'C:\\Windows', 'System32', 'tar.exe') : 'tar';
execFileSync(tar, ['-xjf', archive, '-C', work], { stdio: 'inherit' });
const extracted = join(work, file.name.replace(/\.tar\.bz2$/, ''));

// Release/ becomes the CEF dir; Resources/ is merged in (not on macOS, where
// the resources live inside the framework); the build files go alongside.
mkdirSync(out, { recursive: true });
rmSync(cefDir, { recursive: true, force: true });
renameSync(join(extracted, 'Release'), cefDir);
if (process.platform !== 'darwin') {
  for (const f of readdirSync(join(extracted, 'Resources'))) renameSync(join(extracted, 'Resources', f), join(cefDir, f));
}
for (const f of ['CMakeLists.txt', 'cmake', 'include', 'libcef_dll', 'CREDITS.html']) renameSync(join(extracted, f), join(cefDir, f));
writeFileSync(join(cefDir, 'archive.json'), JSON.stringify({ type: 'minimal', name: file.name, sha1: file.sha1 }, null, 2));
rmSync(work, { recursive: true, force: true });
console.log(cefDir);
