#!/usr/bin/env node
/**
 * test-workers-real.mjs — the real crundi-worker binary (worker/, Rust)
 * against the real server side (src/workers.js), end to end.
 *
 * Skipped (passes) when no worker binary has been built: the Rust build is
 * not part of npm run check. Looks in CRUNDI_WORKER_BIN, worker/target, and
 * the shared target folder ~/.cache/crundi-native-target.
 *
 * Covers: join with a code; headless: online with real info and labels; a
 * terminal running a command; the owner's pause reaching it; revoke ending it
 * with exit 3; embedded mode (the PC client's): JSON status lines, online,
 * and exit when stdin closes.
 *
 * Run: node scripts/test-workers-real.mjs
 */
import { createServer } from 'node:http';
import { mkdtempSync, rmSync, statSync, readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir, homedir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn, execFileSync } from 'node:child_process';
import { createWorkersService } from '../src/workers.js';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const exe = process.platform === 'win32' ? 'crundi-worker.exe' : 'crundi-worker';
const bin = [process.env.CRUNDI_WORKER_BIN, join(root, 'worker/target/release', exe), join(root, 'worker/target/debug', exe),
  join(homedir(), '.cache/crundi-native-target/release', exe), join(homedir(), '.cache/crundi-native-target/debug', exe)]
  .filter(Boolean).find(p => { try { return statSync(p).isFile(); } catch { return false; } });
if (!bin) { console.log('SKIP  no crundi-worker binary built (worker/); nothing to run'); process.exit(0); }

let failed = 0;
const ok = (c, l, extra = '') => { console.log((c ? 'PASS  ' : 'FAIL  ') + l + (c ? '' : '  ' + String(extra).slice(0, 400))); if (!c) failed++; };
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
async function until(fn, ms = 20000) { const end = Date.now() + ms; while (Date.now() < end) { const v = await fn(); if (v) return v; await sleep(50); } return null; }

const dir = mkdtempSync(join(tmpdir(), 'workers-real-'));
const svc = createWorkersService({ dataDir: dir, serverVersion: 'test' });
const server = createServer(async (req, res) => {
  const path = new URL(req.url, 'http://x').pathname;
  if (await svc.handlePublic(req, res, path)) return;
  res.writeHead(404); res.end();
});
server.on('upgrade', (req, socket, head) => { if (!svc.handleUpgrade(req, socket, head)) socket.destroy(); });
await new Promise(r => server.listen(0, '127.0.0.1', r));
const url = `http://127.0.0.1:${server.address().port}`;
const cfg = join(dir, 'config.json');
const work = join(dir, 'work');
console.log(`      binary: ${bin}`);

const procs = [];
function run(args, { stdin = 'ignore' } = {}) {
  const p = spawn(bin, args, { stdio: [stdin, 'pipe', 'pipe'], env: { ...process.env, RUST_LOG: 'warn' } });
  p.out = ''; p.err = ''; p.code = undefined;
  p.stdout.on('data', d => { p.out += d; });
  p.stderr.on('data', d => { p.err += d; });
  p.on('exit', (c) => { p.code = c; });
  procs.push(p);
  return p;
}
const live = () => svc.list().workers[0];

try {
  // ── Join ──
  const { code } = svc.act({ action: 'code' });
  const j = run(['--config', cfg, 'join', url, code, '--name', 'Real PC', '--work-dir', work]);
  await until(() => j.code !== undefined, 30000);
  ok(j.code === 0 && svc.list().workers.length === 1 && live().name === 'Real PC', 'join: the code works, the PC is listed', j.err + j.out);
  const saved = JSON.parse(readFileSync(cfg, 'utf8'));
  ok(/^cw_w[0-9a-f]{10}\./.test(saved.key || '') && saved.id === live().id, 'join: the key and id are saved in its config');

  // ── Headless ──
  const h = run(['--config', cfg, '--headless']);
  const w = await until(() => { const x = live(); return x.online && x.info ? x : null; }, 30000);
  ok(!!w, 'headless: online', h.err);
  if (w) {
    ok(w.info.cpu && w.info.cpu.threads > 0 && w.info.ramMb > 0 && w.info.os && w.info.workDir, 'real info: CPU, memory, OS, work folder', JSON.stringify(w.info));
    ok(w.labels.some(l => l.startsWith('os:')) && w.labels.some(l => l.startsWith('ram:')) && w.labels.includes('tool:git'), 'labels from what it has (git is on this machine)', w.labels.join(','));
    ok(w.kind === 'headless', 'kind: headless');

    // ── A terminal ──
    if (process.platform !== 'win32') {
      const toUi = [];
      const t = svc.uiTerminals((m) => toUi.push(m));
      t.open({ ref: 'r1', worker: w.id, cols: 100, rows: 30 });
      await sleep(500);
      t.input({ ref: 'r1', data: 'echo hello-$((6*7))\r' });
      const got = await until(() => toUi.filter(m => m.type === 'worker-term-data').map(m => m.data).join('').includes('hello-42'), 15000);
      ok(!!got, 'terminal: a command runs on the PC and its output comes back', JSON.stringify(toUi).slice(0, 300));
      t.input({ ref: 'r1', data: 'exit 5\r' });
      const ex = await until(() => toUi.find(m => m.type === 'worker-term-exit'), 15000);
      ok(ex && ex.code === 5, 'terminal: exit code back', JSON.stringify(ex));
    }

    // ── Pause from the owner ──
    svc.act({ action: 'set', id: w.id, paused: true });
    const pz = await until(() => live().info && live().info.paused, 15000);
    ok(!!pz, 'pause: the owner\'s pause reaches the PC (its info says paused)');
    svc.act({ action: 'set', id: w.id, paused: false });

    // ── Revoke ends it with exit 3 ──
    svc.act({ action: 'revoke', id: w.id });
    await until(() => h.code !== undefined, 20000);
    ok(h.code === 3, 'revoke: the headless worker stops with exit 3', `code ${h.code} ${h.err.slice(-300)}`);
  }

  // ── Embedded (the PC client's way) ──
  const s = svc.act({ action: 'join-self', name: 'Client PC', deviceId: 'dev-client', kind: 'electron', version: '1.21.8', os: process.platform });
  const e = run(['--embedded', '--server', url, '--key', s.key, '--work-dir', work, '--name', 'Client PC'], { stdin: 'pipe' });
  const online = await until(() => e.out.split('\n').some(l => { try { const j2 = JSON.parse(l); return j2.event === 'status' && j2.online === true; } catch { return false; } }), 30000);
  ok(!!online, 'embedded: JSON status lines on stdout, online', e.out.slice(-300) + e.err.slice(-300));
  ok(!!(await until(() => svc.list().workers.find(x => x.id === s.id && x.online))), 'embedded: listed online on the server');

  // ── Real jobs through the PC client's way (embedded) ──
  if (process.platform !== 'win32') {
    const proj = join(dir, 'proj');
    mkdirSync(join(proj, 'src'), { recursive: true });
    writeFileSync(join(proj, 'hello.txt'), 'hello from the server\n');
    writeFileSync(join(proj, 'src', 'keep.txt'), 'unchanged\n');
    const r = await svc.mcpRun({ alias: 'proj', command: 'cat hello.txt && mkdir -p out && printf abc > out/r.bin && echo "niceness=$(nice)" && echo "job=$CRUNDI_JOB"', artifacts: ['out/*.bin'], waitMinutes: 2 }, { projectPath: proj });
    ok(r.ok && r.state === 'done' && r.exitCode === 0 && r.source === 'copy', 'a real job: a copy of the project, run on the PC, exit 0', JSON.stringify(r).slice(0, 400));
    ok(/hello from the server/.test(r.tail || '') && /niceness=10/.test(r.tail || '') && new RegExp('job=' + r.job).test(r.tail || ''), 'its output: the copied file, low priority (nice 10), CRUNDI_JOB set', r.tail);
    const names = r.artifacts ? execFileSync('tar', ['-tzf', r.artifacts.path], { encoding: 'utf8' }) : '';
    ok(r.artifacts && r.artifacts.count === 1 && /out\/r\.bin/.test(names), 'the file it made came back as a .tar.gz', names);
    writeFileSync(join(proj, 'hello.txt'), 'changed\n');
    const r2 = await svc.mcpRun({ alias: 'proj', command: 'cat hello.txt; ls out', waitMinutes: 2 }, { projectPath: proj });
    ok(r2.ok && r2.exitCode === 0 && /changed/.test(r2.tail) && r2.copy && r2.copy.sent === 1 && /r\.bin/.test(r2.tail), 'the second copy sends only the changed file, and what the last job built is still there (the cache)', JSON.stringify(r2.copy) + ' ' + r2.tail);
    const r3 = await svc.mcpRun({ alias: 'proj', command: 'sleep 30', wait: false }, { projectPath: proj });
    await until(() => svc.jobs.get(r3.job)?.state === 'running', 15000);
    await svc.mcpJob({ job: r3.job, action: 'cancel' });
    await until(() => svc.jobs.FINISHED.has(svc.jobs.get(r3.job)?.state), 15000);
    ok(svc.jobs.get(r3.job).state === 'cancelled', 'cancel stops a running job on the PC', svc.jobs.get(r3.job).state);
  }

  e.stdin.end();
  await until(() => e.code !== undefined, 15000);
  ok(e.code === 0, 'embedded: stdin closing ends it (exit 0)', `code ${e.code}`);
} finally {
  for (const p of procs) if (p.code === undefined) { try { p.kill(); } catch { /* gone */ } }
  svc.dispose();
  server.close();
  await sleep(200);
  try { rmSync(dir, { recursive: true, force: true }); } catch { /* temp */ }
}
console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
