#!/usr/bin/env node
// Put CEF where the built binary finds it, on each OS:
//
//   Linux     libcef.so and its resources next to the binary (symlinks)
//   Windows   libcef.dll and its resources next to the .exe (copies)
//   macOS     an app bundle: crundi-native.app, with the framework in
//             Contents/Frameworks and the five helper apps CEF starts its
//             processes from (the same binary: it loads the framework from
//             three levels deeper when it sees --type=), signed ad hoc
//
//   node scripts/stage-cef.mjs [targetDir]     (default target/debug)
//   CEF_PATH   the CEF from scripts/fetch-cef.mjs (default: ~/.local/share/cef/cef_<os>_<arch>)
//
// Prints the path to start: the binary, or on macOS the binary inside the bundle.

import { cpSync, existsSync, mkdirSync, readdirSync, rmSync, symlinkSync, writeFileSync, copyFileSync, chmodSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

const here = dirname(fileURLToPath(import.meta.url));
const target = process.argv[2] || join(here, '..', 'target', 'debug');
const os = { linux: 'linux', win32: 'windows', darwin: 'macos' }[process.platform];
const arch = process.arch === 'arm64' ? 'aarch64' : 'x86_64';
const cef = process.env.CEF_PATH || join(homedir(), '.local', 'share', 'cef', `cef_${os}_${arch}`);
const NAME = 'crundi-native';
if (!existsSync(cef)) throw new Error(`No CEF at ${cef}: run scripts/fetch-cef.mjs first`);

if (process.platform === 'darwin') {
  const app = join(target, `${NAME}.app`);
  rmSync(app, { recursive: true, force: true });
  const plist = (exe, id, helper) => `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>${exe}</string>
<key>CFBundleIdentifier</key><string>${id}</string>
<key>CFBundleName</key><string>${exe}</string>
<key>CFBundleDisplayName</key><string>${helper ? exe : 'Crundi (native prototype)'}</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>0.1.0</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
<key>LSMinimumSystemVersion</key><string>11.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>LSEnvironment</key><dict><key>MallocNanoZone</key><string>0</string></dict>
${helper ? '<key>LSUIElement</key><string>1</string>' : ''}
</dict></plist>
`;
  const makeApp = (dir, exe, id, helper) => {
    mkdirSync(join(dir, 'Contents', 'MacOS'), { recursive: true });
    mkdirSync(join(dir, 'Contents', 'Resources'), { recursive: true });
    writeFileSync(join(dir, 'Contents', 'Info.plist'), plist(exe, id, helper));
    copyFileSync(join(target, NAME), join(dir, 'Contents', 'MacOS', exe));
    chmodSync(join(dir, 'Contents', 'MacOS', exe), 0o755);
  };
  makeApp(app, NAME, 'com.faruhaan.crundi.native', false);
  const frameworks = join(app, 'Contents', 'Frameworks');
  mkdirSync(frameworks, { recursive: true });
  cpSync(join(cef, 'Chromium Embedded Framework.framework'), join(frameworks, 'Chromium Embedded Framework.framework'), { recursive: true, verbatimSymlinks: true });
  for (const [suffix, id] of [['', 'helper'], [' (GPU)', 'helper.gpu'], [' (Renderer)', 'helper.renderer'], [' (Plugin)', 'helper.plugin'], [' (Alerts)', 'helper.alerts']]) {
    const exe = `${NAME} Helper${suffix}`;
    makeApp(join(frameworks, `${exe}.app`), exe, `com.faruhaan.crundi.native.${id}`, true);
  }
  // Apple silicon runs nothing unsigned: an ad-hoc signature, inside out.
  try { execFileSync('codesign', ['--force', '--deep', '--sign', '-', app], { stdio: 'inherit' }); } catch (e) { console.error('codesign failed:', e.message); }
  console.log(join(app, 'Contents', 'MacOS', NAME));
} else {
  const skip = new Set(['include', 'cmake', 'libcef_dll', 'CMakeLists.txt', 'CREDITS.html', 'archive.json']);
  const wanted = (f) => !skip.has(f) && !/\.lib$/i.test(f) && !/^cef_sandbox/i.test(f);
  for (const f of readdirSync(cef).filter(wanted)) {
    const to = join(target, f);
    rmSync(to, { recursive: true, force: true });
    if (process.platform === 'win32') cpSync(join(cef, f), to, { recursive: true });
    else symlinkSync(join(cef, f), to);
  }
  console.log(join(target, process.platform === 'win32' ? `${NAME}.exe` : NAME));
}
