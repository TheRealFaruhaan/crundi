#!/usr/bin/env node
/**
 * test-workers-jobs.mjs — commands on a worker PC (src/workers-jobs.js),
 * against fake workers over real websockets.
 *
 * Covers: worker_run picks a free PC (by name, by labels; refuses a paused,
 * busy or commands-off one with a reason); the git source sends the https
 * remote, branch and the server's commit, and says uncommitted changes are
 * not in it; a bundle for commits GitHub lacks (a real git bundle that
 * verifies); the copy source sends only new and changed files as a tar and
 * lists the removed ones; the log streams, and after the link drops and comes
 * back it resumes from where the server stopped with nothing doubled; the
 * artifact comes back as a file; cancel reaches the PC; worker_job; the
 * record survives a restart.
 *
 * Run: node scripts/test-workers-jobs.mjs
 */
import { createServer } from 'node:http';
import { mkdtempSync, rmSync, writeFileSync, readFileSync, mkdirSync, statSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import WebSocket from 'ws';
import { createWorkersService } from '../src/workers.js';
import { httpsRemote } from '../src/workers-jobs.js';

let failed = 0;
const ok = (c, l, extra = '') => { console.log((c ? 'PASS  ' : 'FAIL  ') + l + (c ? '' : '  ' + String(extra).slice(0, 400))); if (!c) failed++; };
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
async function until(fn, ms = 5000) { const end = Date.now() + ms; while (Date.now() < end) { const v = await fn(); if (v) return v; await sleep(10); } return null; }

const dir = mkdtempSync(join(tmpdir(), 'workers-jobs-'));
let svc = createWorkersService({ dataDir: join(dir, 'data'), serverVersion: 't' });
const server = createServer(async (req, res) => { if (!(await svc.handlePublic(req, res, new URL(req.url, 'http://x').pathname))) { res.writeHead(404); res.end(); } });
server.on('upgrade', (req, socket, head) => { if (!svc.handleUpgrade(req, socket, head)) socket.destroy(); });
await new Promise(r => server.listen(0, '127.0.0.1', r));
const base = `http://127.0.0.1:${server.address().port}`;

// A project: a git repo whose origin is GitHub (never contacted: the PC is fake).
const proj = join(dir, 'proj');
mkdirSync(join(proj, 'src'), { recursive: true });
const g = (...a) => execFileSync('git', ['-c', 'user.name=t', '-c', 'user.email=t@t', ...a], { cwd: proj, encoding: 'utf8' }).trim();
g('init', '-q', '-b', 'dev');
writeFileSync(join(proj, 'a.txt'), 'one\n');
writeFileSync(join(proj, 'src', 'b.txt'), 'two\n');
writeFileSync(join(proj, '.gitignore'), 'ignored.txt\n');
g('add', '.'); g('commit', '-qm', 'first');
const firstSha = g('rev-parse', 'HEAD');
writeFileSync(join(proj, 'a.txt'), 'one more\n');
g('commit', '-qam', 'second');
const headSha = g('rev-parse', 'HEAD');
g('remote', 'add', 'origin', 'git@github.com:someone/proj.git');

ok(httpsRemote('git@github.com:a/b.git') === 'https://github.com/a/b.git' && httpsRemote('ssh://git@github.com/a/b') === 'https://github.com/a/b.git' && httpsRemote('https://x/y.git') === 'https://x/y.git', 'GitHub SSH remotes are sent as https');

function fakeWorker(key, { caps = ['info', 'terminal', 'pause', 'jobs'], jobs = [], info = {} } = {}) {
  const got = [];
  const ws = new WebSocket(base.replace('http', 'ws') + '/api/workers/link', { headers: { Authorization: 'Bearer ' + key } });
  const w = { ws, got, closed: false };
  ws.on('message', (raw) => got.push(JSON.parse(raw.toString())));
  ws.on('close', () => { w.closed = true; });
  w.send = (m) => ws.send(JSON.stringify(m));
  w.of = (type) => got.filter(m => m.type === type);
  w.ready = new Promise((resolve) => ws.on('open', () => {
    w.send({ type: 'hello', proto: 1, version: '0.2.0', kind: 'electron', name: 'x', caps, jobs, info: { os: 'linux', arch: 'x86_64', ramMb: 65536, cpu: { threads: 32 }, gpus: [{ vendor: 'NVIDIA', name: 'RTX' }], tools: { cargo: '1.83', git: '2.4' }, ...info } });
    resolve();
  }));
  return w;
}
const join1 = (name, deviceId) => svc.act({ action: 'join-self', name, deviceId, kind: 'electron', version: '1.21.8', os: 'linux' });
const run = (a) => svc.mcpRun({ alias: 'proj', ...a }, { projectPath: proj, chatId: 'chat1' });
const job = (id) => svc.jobs.get(id);

const k1 = join1('Big PC', 'dev-big');
const k2 = join1('Old PC', 'dev-old');
let big = fakeWorker(k1.key);
const old = fakeWorker(k2.key, { caps: ['info', 'terminal', 'pause'], info: { gpus: [], tools: {} } });
await big.ready; await old.ready;
await until(() => big.of('welcome')[0] && old.of('welcome')[0]);
ok(big.of('welcome')[0].caps.includes('jobs'), 'welcome: the server offers jobs');

// ── Picking ──
{
  const r = await run({ command: 'x', worker: 'Old PC', wait: false });
  ok(!r.ok && /older worker/.test(r.error), 'a named PC with an older worker: says to update it', r.error);
  const r2 = await run({ command: 'x', labels: ['gpu:amd'], wait: false });
  ok(!r2.ok && /label/.test(r2.error), 'no PC has the labels: says so', r2.error);
  svc.act({ action: 'set', id: k1.id, allowCommands: false });
  const r3 = await run({ command: 'x', worker: 'Big PC', wait: false });
  ok(!r3.ok && /does not allow commands/.test(r3.error), 'commands switched off on that PC: refused with the reason', r3.error);
  svc.act({ action: 'set', id: k1.id, allowCommands: true });
}

// ── A git job, end to end ──
writeFileSync(join(proj, 'a.txt'), 'dirty\n');   // an uncommitted change
const started = run({ command: 'cargo build --release', labels: ['gpu:nvidia', 'tool:cargo'], artifacts: ['target/release/*.exe'], waitMinutes: 1 });
const start = await until(() => big.of('job.start')[0]);
ok(start && start.source === 'git' && start.git.url === 'https://github.com/someone/proj.git' && start.git.branch === 'dev' && start.git.commit === headSha && start.lowPriority === true && start.artifacts[0] === 'target/release/*.exe', 'git job: https remote, the branch and the server\'s commit, low priority, artifact globs', JSON.stringify(start));
const J = start.job;
big.send({ type: 'job.accepted', job: J });
big.send({ type: 'job.status', job: J, phase: 'preparing', detail: 'fetching' });
// The PC lacks the commit: it asks for a bundle on top of what it has.
big.send({ type: 'job.need', job: J, kind: 'bundle', have: firstSha });
const bundleLast = await until(() => big.of('job.bundle').find(m => m.job === J && m.last), 10000);
ok(!!bundleLast, 'job.need: the server sends a git bundle');
{
  const bfile = join(dir, 'got.bundle');
  writeFileSync(bfile, Buffer.concat(big.of('job.bundle').filter(m => m.job === J).sort((a, b) => a.seq - b.seq).map(m => Buffer.from(m.b64, 'base64'))));
  const clone = join(dir, 'clone');
  execFileSync('git', ['init', '-q', clone]);
  // The PC side: it has the first commit (from GitHub); the bundle brings the rest.
  execFileSync('git', ['fetch', '-q', proj, firstSha], { cwd: clone });
  let verified = true;
  try { execFileSync('git', ['bundle', 'verify', bfile], { cwd: clone, stdio: 'pipe' }); } catch { verified = false; }
  execFileSync('git', ['fetch', '-q', bfile, '+refs/heads/*:refs/crundi-bundle/*'], { cwd: clone });
  const has = execFileSync('git', ['cat-file', '-t', headSha], { cwd: clone, encoding: 'utf8' }).trim();
  ok(verified && has === 'commit', 'the bundle verifies on a clone that has the first commit, and brings the server\'s commit');
}
big.send({ type: 'job.status', job: J, phase: 'running' });
big.send({ type: 'job.out', job: J, off: 0, b64: Buffer.from('Compiling crundi v1\n').toString('base64') });
await until(() => job(J).logBytes === 20);
// The link drops mid-job; the PC carries on and comes back.
big.ws.close();
await until(() => job(J).offline);
ok(job(J).offline && job(J).state === 'running', 'the link drops: the job is still running there');
big = fakeWorker(k1.key, { jobs: [{ job: J, state: 'running', logBytes: 45 }] });
await big.ready;
const att = await until(() => big.of('job.attach').find(m => m.job === J));
ok(att && att.from === 20, 'back again: the server asks for the log from where it stopped (byte 20)', JSON.stringify(att));
// The PC resends from 10 (overlap) and on: nothing is doubled.
big.send({ type: 'job.out', job: J, off: 10, b64: Buffer.from('crundi v1\nFinished release\n').toString('base64') });
big.send({ type: 'job.exit', job: J, code: 0, durationMs: 61234 });
const art = Buffer.from('fake tar.gz bytes');
big.send({ type: 'job.artifact', job: J, seq: 0, b64: art.subarray(0, 5).toString('base64'), last: false, count: 1, bytes: art.length });
big.send({ type: 'job.artifact', job: J, seq: 1, b64: art.subarray(5).toString('base64'), last: true, count: 1, bytes: art.length });
big.send({ type: 'job.done', job: J });
const res = await started;
ok(res.ok && res.state === 'done' && res.exitCode === 0 && res.durationMs === 61234, 'worker_run: done, exit code and duration', JSON.stringify(res));
ok(res.tail === 'Compiling crundi v1\nFinished release\n', 'the log has every line once, across the reconnect', JSON.stringify(res.tail));
ok(res.artifacts && existsSync(res.artifacts.path) && readFileSync(res.artifacts.path).equals(art) && res.artifacts.count === 1, 'the artifact is a file on the server', JSON.stringify(res.artifacts));
ok(res.notes.some(n => /uncommitted/.test(n)) && res.notes.some(n => /sent from the server/.test(n)), 'notes: uncommitted changes are not in it; missing commits came from the server', JSON.stringify(res.notes));
g('checkout', '-q', '--', 'a.txt');

// ── Busy, paused ──
{
  const r1 = run({ command: 'sleep 100', worker: 'Big PC', wait: false });
  const st = await until(() => big.of('job.start').find(m => m.command === 'sleep 100'));
  big.send({ type: 'job.accepted', job: st.job });
  const a = await r1;
  ok(a.ok && a.running, 'wait: false returns at once, still running');
  const r2 = await run({ command: 'echo', worker: 'Big PC', wait: false });
  ok(!r2.ok && /another job/.test(r2.error), 'one job per PC: a second is refused while it runs');
  const c = await svc.mcpJob({ job: st.job, action: 'cancel' });
  ok(c.ok && !!(await until(() => big.of('job.cancel').find(m => m.job === st.job))), 'cancel reaches the PC');
  big.send({ type: 'job.exit', job: st.job, code: null, cancelled: true, durationMs: 50 });
  big.send({ type: 'job.done', job: st.job });
  ok((await until(() => job(st.job).state === 'cancelled')) === true, 'and it ends as cancelled');
  svc.act({ action: 'set', id: k1.id, paused: true });
  const r3 = await run({ command: 'echo', worker: 'Big PC', wait: false });
  ok(!r3.ok && /paused/.test(r3.error), 'a paused PC is not given jobs');
  svc.act({ action: 'set', id: k1.id, paused: false });
  // The PC itself refuses (its own switch, say).
  const r4 = run({ command: 'echo refused', worker: 'Big PC', wait: false });
  const st4 = await until(() => big.of('job.start').find(m => m.command === 'echo refused'));
  big.send({ type: 'job.refused', job: st4.job, error: 'Commands are off on this PC.' });
  const a4 = await r4;
  ok(!a4.ok && /Commands are off/.test(a4.error), 'a refusal from the PC is the answer', JSON.stringify(a4));
}

// ── A copy job ──
{
  writeFileSync(join(proj, 'new.txt'), 'new and untracked\n');
  writeFileSync(join(proj, 'ignored.txt'), 'never sent\n');
  mkdirSync(join(proj, 'node_modules', 'x'), { recursive: true });
  writeFileSync(join(proj, 'node_modules', 'x', 'i.js'), 'never sent\n');
  const sha = (s) => execFileSync('sha256sum', { input: s, encoding: 'utf8' }).split(' ')[0];
  // The PC has src/b.txt as it is, an old a.txt, gone.txt (which an earlier copy
  // sent and the server no longer has) and out/built.bin (which a build made there).
  writeFileSync(join(dir, 'data', 'workers', 'copies.json'), JSON.stringify({ [`${k1.id}:proj`]: ['a.txt', 'src/b.txt', 'gone.txt'] }));
  svc.dispose(); await sleep(50);
  svc = createWorkersService({ dataDir: join(dir, 'data'), serverVersion: 't' });
  big.ws.close(); big = fakeWorker(k1.key); await big.ready; await until(() => big.of('welcome')[0]);
  const p = run({ command: 'make', source: 'copy', worker: 'Big PC', waitMinutes: 1 });
  const st = await until(() => big.of('job.start').find(m => m.command === 'make'));
  ok(st && st.source === 'copy' && !st.git, 'copy job: no git details');
  big.send({ type: 'job.accepted', job: st.job });
  big.send({ type: 'job.manifest', job: st.job, files: { 'src/b.txt': [4, sha('two\n')], 'a.txt': [4, sha('one\n')], 'gone.txt': [3, sha('old')], 'out/built.bin': [3, sha('bin')] } });
  const last = await until(() => big.of('job.files').find(m => m.job === st.job && m.last), 10000);
  ok(last && last.removed.join() === 'gone.txt', 'copy: a file an earlier copy sent and the server no longer has is removed; what a build made on the PC is not', JSON.stringify(last && last.removed));
  const tarFile = join(dir, 'files.tar');
  writeFileSync(tarFile, Buffer.concat(big.of('job.files').filter(m => m.job === st.job).sort((a, b) => a.seq - b.seq).map(m => Buffer.from(m.b64, 'base64'))));
  const names = execFileSync('tar', ['-tf', tarFile], { encoding: 'utf8' }).split('\n').filter(Boolean).sort();
  ok(names.join() === '.gitignore,a.txt,new.txt', 'copy: only new and changed files are sent; unchanged, ignored and node_modules are not', names.join());
  const x = join(dir, 'x'); mkdirSync(x);
  execFileSync('tar', ['-xf', tarFile, '-C', x]);
  ok(readFileSync(join(x, 'a.txt'), 'utf8') === 'one more\n' && readFileSync(join(x, 'new.txt'), 'utf8') === 'new and untracked\n', 'copy: the tar holds the files as they are on the server');
  big.send({ type: 'job.exit', job: st.job, code: 2, durationMs: 10 });
  big.send({ type: 'job.artifact', job: st.job, seq: 0, b64: '', last: true, count: 0, bytes: 0 });
  big.send({ type: 'job.done', job: st.job });
  const r = await p;
  ok(r.ok && r.state === 'failed' && r.exitCode === 2 && r.copy.sent === 3 && r.copy.removed === 1, 'copy job: failed with its exit code, what was sent counted', JSON.stringify(r.copy));
}

// ── worker_job and the Workers tab's view ──
{
  const list = svc.act({ action: 'jobs', id: k1.id });
  ok(list.ok && list.jobs.length >= 4 && list.jobs[0].command === 'make', 'the PC\'s jobs, newest first');
  const lg = svc.act({ action: 'job-log', job: J });
  ok(lg.ok && /Finished release/.test(lg.job.tail), 'its log, for the tab');
  const st = await svc.mcpJob({ job: J });
  ok(st.ok && st.state === 'done', 'worker_job status');
  const bad = await svc.mcpJob({ job: 'jnope' });
  ok(!bad.ok, 'worker_job: an unknown job');
  const l = svc.mcpList();
  ok(l.workers.find(w => w.name === 'Big PC') && /worker_run/.test(l.note), 'worker_list points at worker_run');
}

// ── A restart keeps the jobs ──
svc.dispose();
await sleep(50);
svc = createWorkersService({ dataDir: join(dir, 'data'), serverVersion: 't' });
{
  const v = svc.jobs.view(J, 100);
  ok(v && v.state === 'done' && /Finished/.test(v.tail) && v.artifacts && v.artifacts.count === 1, 'after a restart: the job, its log and its artifact are still there');
}

svc.dispose();
server.close();
try { rmSync(dir, { recursive: true, force: true }); } catch { /* temp */ }
console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
