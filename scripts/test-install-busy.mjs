// An update must not fail because a program in node_modules is running.
//
// install.sh copied the prebuilt node_modules over the installed one. That
// folder holds cloudflared, which is executing from it whenever a tunnel is
// open, and Linux refuses to write to a running program's file: "Text file
// busy". cp failed on that file, the installer (set -e) stopped, and the server
// was left with new code on disk and no restart - which is how 1.19.16 stalled
// on a live server.
//
// This runs the installer's own copy step, lifted from install.sh, against a
// tree that has a program running out of it.
import { readFileSync, mkdtempSync, mkdirSync, writeFileSync, copyFileSync, chmodSync, existsSync, readdirSync, rmSync } from 'node:fs';
import { spawn, spawnSync } from 'node:child_process';
import { join, dirname } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
let failed = 0;
const ok = (cond, name, extra = '') => { console.log(`${cond ? 'PASS' : 'FAIL'}  ${name}${cond || !extra ? '' : ' -> ' + extra}`); if (!cond) failed++; };

const sh = readFileSync(join(root, 'scripts', 'install.sh'), 'utf8').replace(/\r\n/g, '\n');
const from = sh.indexOf('if [ "$PREBUILT" -eq 1 ]; then\n  say "Copying the prebuilt modules"');
const to = sh.indexOf('\nelse\n', from);
ok(from > 0 && to > from, 'found the prebuilt copy step in install.sh');
const step = sh.slice(from, to) + '\nfi\n';
ok(!/cp -r "\$SRC\/node_modules" "\$PREFIX\/"/.test(step), 'node_modules is not copied over the installed tree');

// The rest needs a real "busy" file, which is a Linux behaviour.
if (process.platform !== 'linux' || !existsSync('/bin/sleep')) {
  console.log('SKIP  the live check needs Linux');
} else {
  const tmp = mkdtempSync(join(tmpdir(), 'crundi-busy-'));
  const SRC = join(tmp, 'src'), PREFIX = join(tmp, 'prefix');
  const bin = (base) => join(base, 'node_modules', 'cloudflared', 'bin');
  for (const base of [SRC, PREFIX]) {
    mkdirSync(bin(base), { recursive: true });
    copyFileSync('/bin/sleep', join(bin(base), 'cloudflared'));
    chmodSync(join(bin(base), 'cloudflared'), 0o755);
    mkdirSync(join(base, 'node_modules', 'ws'), { recursive: true });
  }
  writeFileSync(join(SRC, 'node_modules', 'ws', 'index.js'), 'new');
  writeFileSync(join(PREFIX, 'node_modules', 'ws', 'index.js'), 'old');
  writeFileSync(join(PREFIX, 'node_modules', 'ws', 'removed-in-this-release.js'), 'stale');
  writeFileSync(join(SRC, '.prebuilt'), '127\n');
  mkdirSync(join(PREFIX, 'node_modules.old.999'));          // left by an install that was killed

  // A tunnel, as far as the kernel is concerned: a program running from the installed tree.
  const tunnel = spawn(join(bin(PREFIX), 'cloudflared'), ['30'], { stdio: 'ignore' });
  await new Promise(r => setTimeout(r, 300));

  // The old way, to prove the test would catch it.
  const naive = spawnSync('cp', ['-r', join(SRC, 'node_modules'), PREFIX + '/'], { encoding: 'utf8' });
  ok(naive.status !== 0 && /Text file busy/.test(naive.stderr), 'copying over a running program fails, as it did on the live server', naive.stderr.trim());

  const script = `set -euo pipefail\nsay() { :; }\nPREBUILT=1\nSRC=${JSON.stringify(SRC)}\nPREFIX=${JSON.stringify(PREFIX)}\n${step}\necho carried-on\n`;
  const run = spawnSync('bash', ['-c', script], { encoding: 'utf8' });
  ok(run.status === 0 && /carried-on/.test(run.stdout), 'the installer\'s copy step succeeds with the program running', (run.stderr || '').trim());
  ok(readFileSync(join(PREFIX, 'node_modules', 'ws', 'index.js'), 'utf8') === 'new', 'the new modules are in place');
  ok(!existsSync(join(PREFIX, 'node_modules', 'ws', 'removed-in-this-release.js')), 'and files dropped from the release are gone, not left behind');
  ok(existsSync(join(bin(PREFIX), 'cloudflared')), 'the program\'s new copy is there for the next tunnel');
  ok(tunnel.exitCode === null && tunnel.signalCode === null, 'the running tunnel was not disturbed');
  ok(readdirSync(PREFIX).filter(n => /^node_modules\./.test(n)).length === 0, 'no staging or old copies are left, including one from an earlier interrupted install', readdirSync(PREFIX).join(' '));
  ok(readFileSync(join(PREFIX, '.prebuilt'), 'utf8') === '127\n', 'the steps after it ran');

  // Installing in place (source and prefix are the same folder) must not delete the modules.
  const inplace = spawnSync('bash', ['-c', script.replace(`SRC=${JSON.stringify(SRC)}`, `SRC=${JSON.stringify(PREFIX)}`)], { encoding: 'utf8' });
  ok(inplace.status === 0 && existsSync(join(PREFIX, 'node_modules', 'ws', 'index.js')), 'installing a folder onto itself leaves its modules alone', (inplace.stderr || '').trim());

  tunnel.kill();
  rmSync(tmp, { recursive: true, force: true });
}

console.log(failed ? `\n${failed} install check(s) failed` : '\nAll install checks passed');
process.exit(failed ? 1 : 0);
