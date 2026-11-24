#!/usr/bin/env node
/**
 * test-workers-reconnect.mjs — bringing a chat's work back from a PC survives
 * the link dropping (src/workers-chats.js pull / pullChat, src/workers.js).
 *
 * The failure seen in a real run: a step on a PC finished, and its handoff
 * failed three times with "The PC disconnected." while the PC went offline
 * and online within seconds of each try. The PC's uncommitted work (retaken
 * screenshots) went as ONE chat.patch message, over the link's 4 MB message
 * limit: the server closed the link (1009) on every try, and the pull failed
 * at once on the drop.
 *
 * Covers: an older worker's single big chat.patch (6 MB) is taken; a newer
 * worker's patch in numbered pieces (chunks: true is asked); chat.pull says
 * which commits the server has (have); the PC dropping mid-bundle and coming
 * back: the pull is asked again (a new req), pieces of the cut-off try are
 * ignored, and pullChat resolves with the work; a pull asked while the PC is
 * away waits for it; the PC not coming back within the grace: an error that
 * says where the work is on the PC and what to do; dropping on every try:
 * bounded tries, the same kind of error; "Bring changes back" on an offline
 * PC answers at once; any message from the PC keeps the link alive.
 *
 * Run: node scripts/test-workers-reconnect.mjs
 */
import { createServer } from 'node:http';
import { mkdtempSync, rmSync, writeFileSync, readFileSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import WebSocket from 'ws';

let failed = 0;
const ok = (c, l, extra = '') => { console.log((c ? 'PASS  ' : 'FAIL  ') + l + (c ? '' : '  ' + String(extra).slice(0, 600))); if (!c) failed++; };
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
async function until(fn, ms = 8000) { const end = Date.now() + ms; while (Date.now() < end) { const v = await fn(); if (v) return v; await sleep(10); } return null; }

const dir = mkdtempSync(join(tmpdir(), 'workers-reconnect-'));
const projects = join(dir, 'projects');
mkdirSync(projects, { recursive: true });
process.env.DATA_DIR = join(dir, 'data');
process.env.PROJECTS_DIR = projects;
const { createWorkersService } = await import('../src/workers.js');

// The server's project: a git repo with a GitHub origin (never contacted).
const proj = join(projects, 'proj');
mkdirSync(proj, { recursive: true });
const gitIn = (cwd, ...a) => execFileSync('git', ['-c', 'user.name=t', '-c', 'user.email=t@t', ...a], { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
const g = (...a) => gitIn(proj, ...a);
g('init', '-q', '-b', 'main');
g('config', 'core.autocrlf', 'false');
writeFileSync(join(proj, 'a.txt'), 'one\n');
g('add', '.'); g('commit', '-qm', 'first');
const headSha = g('rev-parse', 'HEAD');
g('remote', 'add', 'origin', 'git@github.com:someone/proj.git');

const server = createServer((req, res) => { res.writeHead(404); res.end(); });
let svc;
server.on('upgrade', (req, socket, head) => { if (!svc.handleUpgrade(req, socket, head)) socket.destroy(); });
await new Promise(r => server.listen(0, '127.0.0.1', r));
const base = `http://127.0.0.1:${server.address().port}`;
const GRACE = 1500;
svc = createWorkersService({ dataDir: join(dir, 'data'), serverVersion: 't', chatsOptions: { pullGraceMs: GRACE, pullTries: 3 } });

const WORKDIR = '/home/someone/crundi-work';
function fakeWorker(key, { chats = [] } = {}) {
  const got = [];
  const ws = new WebSocket(base.replace('http', 'ws') + '/api/workers/link', { headers: { Authorization: 'Bearer ' + key }, maxPayload: 256 * 1024 * 1024 });
  const w = { ws, got, closed: false, closeCode: null };
  ws.on('message', (raw) => got.push(JSON.parse(raw.toString())));
  ws.on('close', (code) => { w.closed = true; w.closeCode = code; });
  w.send = (m) => ws.send(JSON.stringify(m));
  w.of = (type, chat) => got.filter(m => m.type === type && (!chat || m.chat === chat));
  w.ready = new Promise((resolve) => ws.on('open', () => {
    w.send({ type: 'hello', proto: 1, version: '0.5.0', kind: 'native', name: 'x', caps: ['info', 'terminal', 'pause', 'jobs', 'chats', 'chat-sync'], chats, info: { os: 'linux', arch: 'x86_64', workDir: WORKDIR, tools: { git: '2.4', claude: '2.1' }, allow: { chats: true, commands: true } } });
    resolve();
  }));
  return w;
}
const k1 = svc.act({ action: 'join-self', name: 'RP 15', deviceId: 'dev-rp', kind: 'native', version: '1', os: 'linux' });
let pc = fakeWorker(k1.key);
await pc.ready;
await until(() => pc.of('welcome')[0]);

// A chat on the PC (a factory step's), started there.
const C = 'a282fb950a363e62';
await svc.chats.launch(`worker:${k1.id}`, { cwd: proj, chatId: C, project: 'proj', factory: { runId: 'r1' } });
await until(() => pc.of('chat.spawn', C)[0]);
pc.send({ type: 'chat.started', chat: C, pid: 7 });
await until(() => svc.chats.view(C)?.state === 'running');

// The PC's folder: commits on pc/<chat>, uncommitted work (a big "screenshot").
const pcDir = join(dir, 'pc');
execFileSync('git', ['clone', '-q', proj, pcDir]);
gitIn(pcDir, 'config', 'core.autocrlf', 'false');
gitIn(pcDir, 'checkout', '-q', '-b', `pc/${C}`, headSha);
writeFileSync(join(pcDir, 'b.txt'), 'made on the PC\n');
gitIn(pcDir, 'add', '.'); gitIn(pcDir, 'commit', '-qm', 'pc work');
const pcHead = gitIn(pcDir, 'rev-parse', 'HEAD');
const shot = randomBytes(4_500_000);   // incompressible: a ~6 MB base85 patch, ~8 MB as base64
writeFileSync(join(pcDir, 'shot.png'), shot);
gitIn(pcDir, 'add', '-N', 'shot.png');
const bigPatch = execFileSync('git', ['diff', '--binary', 'HEAD'], { cwd: pcDir, maxBuffer: 256 * 1024 * 1024 });
gitIn(pcDir, 'reset', '-q', 'shot.png');

// The run's worktree the work comes into (as factory-runner's bringBack).
const runWt = join(dir, 'run-wt');
g('worktree', 'add', '-q', '-b', 'factory/run-1', runWt, headSha);

const CH = 512 * 1024;
function bundleBytes(p) {
  const bf = join(dir, `b-${p.req}.bundle`);
  // As the worker does: only the commits it knows are left out.
  const knows = (sha) => { try { execFileSync('git', ['cat-file', '-e', `${sha}^{commit}`], { cwd: pcDir, stdio: 'pipe' }); return true; } catch { return false; } };
  const neg = [p.base, ...(p.have || [])].filter(Boolean).filter(knows).map(s => `^${s}`);
  // Nothing the server lacks: git refuses an empty bundle, the PC sends none.
  try { execFileSync('git', ['bundle', 'create', '-q', bf, `refs/heads/pc/${C}`, ...neg], { cwd: pcDir, stdio: 'pipe' }); } catch { return Buffer.alloc(0); }
  return readFileSync(bf);
}
/** Answer a pull: the bundle in pieces, then the patch (one message, or in pieces when asked). */
function answer(w, p, { chunked = !!p.chunks, cut = false, patch = bigPatch } = {}) {
  const bytes = bundleBytes(p);
  const n = Math.max(1, Math.ceil(bytes.length / 64));
  for (let i = 0; i < n; i++) {
    w.send({ type: 'chat.bundle', chat: C, req: p.req, seq: i, b64: bytes.subarray(i * 64, (i + 1) * 64).toString('base64'), last: i === n - 1, ...(i === n - 1 ? { head: pcHead } : {}) });
  }
  if (!chunked) { w.send({ type: 'chat.patch', chat: C, req: p.req, b64: patch.toString('base64') }); return true; }
  const m = Math.max(1, Math.ceil(patch.length / CH));
  // cut: the link goes mid-patch (after its first piece).
  for (let i = 0; i < (cut ? 1 : m); i++) w.send({ type: 'chat.patch', chat: C, req: p.req, seq: i, b64: patch.subarray(i * CH, (i + 1) * CH).toString('base64'), last: i === m - 1 });
  return true;
}
const resetRun = () => { gitIn(runWt, 'reset', '-q', '--hard', headSha); gitIn(runWt, 'clean', '-fdq'); };
const landed = () => {
  try { return readFileSync(join(runWt, 'shot.png')).equals(shot) && readFileSync(join(runWt, 'b.txt'), 'utf8') === 'made on the PC\n' && gitIn(runWt, 'rev-parse', 'HEAD') === pcHead; } catch { return false; }
};
const nextPull = (w, i) => until(() => w.of('chat.pull', C)[i], 8000);

// ── An older worker: its patch as ONE big message (the real failure) ──
{
  const pulling = svc.workersApi.pullChat(C, { base: headSha, intoWorktree: runWt, branch: 'factory/run-1' });
  const p = await nextPull(pc, 0);
  ok(p && p.chunks === true && Array.isArray(p.have) && p.have.includes(headSha), 'chat.pull asks for the patch in pieces and says which commits the server has', JSON.stringify(p));
  answer(pc, p, { chunked: false });
  const r = await pulling;
  ok(r.ok && r.head === pcHead && landed(), 'a 6 MB patch in one message (an older worker) is taken: the link is not closed, the work lands', JSON.stringify(r));
  ok(!pc.closed, 'the link stayed up', String(pc.closeCode));
}

// ── A newer worker: the patch in numbered pieces ──
{
  resetRun();
  const pulling = svc.workersApi.pullChat(C, { base: headSha, intoWorktree: runWt, branch: 'factory/run-1' });
  const p = await nextPull(pc, 1);
  answer(pc, p, { chunked: true });
  const r = await pulling;
  ok(r.ok && r.head === pcHead && landed(), 'the patch in 512 KB pieces is put together and applied', JSON.stringify(r));
}

// ── The run's worktree already has the PC's commit: the server says so (have) ──
{
  gitIn(runWt, 'add', '-A'); gitIn(runWt, 'commit', '-qm', 'factory: step');   // as the runner commits a handoff
  const runHead = gitIn(runWt, 'rev-parse', 'HEAD');
  const pulling = svc.workersApi.pullChat(C, { base: headSha, intoWorktree: runWt, branch: 'factory/run-1' });
  const p = await nextPull(pc, 2);
  ok(p && p.have.includes(runHead) && p.have.includes(pcHead), 'have names the run worktree\'s head and the PC head last brought back', JSON.stringify(p && p.have));
  answer(pc, p, { chunked: true, patch: Buffer.alloc(0) });
  const r = await pulling;
  ok(r.ok, 'and the pull still completes', JSON.stringify(r));
}

// ── The PC drops mid-transfer and comes back: asked again, the handoff waits ──
{
  resetRun();
  const pulling = svc.workersApi.pullChat(C, { base: headSha, intoWorktree: runWt, branch: 'factory/run-1' });
  const p1 = await nextPull(pc, 3);
  answer(pc, p1, { cut: true });   // the bundle and part of the patch, then the link goes
  await sleep(50);
  const old = pc;
  pc.ws.terminate();
  await until(() => !svc.isOnline(k1.id));
  let settled = false; pulling.then((x) => { settled = x; });
  await sleep(300);
  ok(!settled, 'the drop does not fail the pull: it waits for the PC', JSON.stringify(settled));
  pc = fakeWorker(k1.key, { chats: [{ chat: C, running: true, outBytes: 0 }] });
  await pc.ready;
  const p2 = await nextPull(pc, 0);
  ok(p2 && p2.req !== p1.req, 'back within the grace: chat.pull is sent again, with a new req', JSON.stringify(p2));
  // A piece of the cut-off try arriving late is ignored.
  pc.send({ type: 'chat.patch', chat: C, req: p1.req, b64: Buffer.from('garbage').toString('base64') });
  answer(pc, p2, { chunked: true });
  const r = await pulling;
  ok(r.ok && r.head === pcHead && landed(), 'and the work comes back whole', JSON.stringify(r));
  ok(old.closed, 'the old link is gone');
}

// ── A pull asked while the PC is away waits for it to come back ──
{
  resetRun();
  pc.ws.terminate();
  await until(() => !svc.isOnline(k1.id));
  const pulling = svc.workersApi.pullChat(C, { base: headSha, intoWorktree: runWt, branch: 'factory/run-1' });
  await sleep(200);
  pc = fakeWorker(k1.key, { chats: [{ chat: C, running: true, outBytes: 0 }] });
  await pc.ready;
  const p = await nextPull(pc, 0);
  answer(pc, p, { chunked: true });
  const r = await pulling;
  ok(r.ok && landed(), 'a pull asked while the PC was briefly away completes when it is back', JSON.stringify(r));
}

// ── "Bring changes back" on an offline PC answers at once ──
{
  pc.ws.terminate();
  await until(() => !svc.isOnline(k1.id));
  const t0 = Date.now();
  const r = await svc.act({ action: 'chat-pull', chat: C });
  ok(!r.ok && /offline/.test(r.error) && Date.now() - t0 < GRACE, 'Bring changes back with the PC offline: told at once', JSON.stringify(r));
}

// ── The PC drops and does not come back: says where the work is ──
{
  pc = fakeWorker(k1.key, { chats: [{ chat: C, running: true, outBytes: 0 }] });
  await pc.ready;
  await until(() => svc.isOnline(k1.id));
  const pulling = svc.workersApi.pullChat(C, { base: headSha, intoWorktree: runWt, branch: 'factory/run-1' });
  const p = await nextPull(pc, 0);
  answer(pc, p, { cut: true });
  pc.ws.terminate();
  const t0 = Date.now();
  const r = await pulling;
  const where = `${WORKDIR}/proj@${C}`;
  ok(!r.ok && /did not come back within/.test(r.error) && Date.now() - t0 >= GRACE - 100, 'not back within the grace: the pull fails then, saying so', JSON.stringify(r));
  ok(r.error.includes('Nothing is lost') && r.error.includes(where) && r.error.includes(`pc/${C}`) && /push origin/.test(r.error), 'the error says where the work is on the PC (folder, branch) and how to get it', r.error);
}

// ── Dropping on every try: bounded, then the same kind of error ──
{
  let tries = 0;
  const pulling = svc.workersApi.pullChat(C, { base: headSha, intoWorktree: runWt, branch: 'factory/run-1' });
  for (let i = 0; i < 5; i++) {
    pc = fakeWorker(k1.key, { chats: [{ chat: C, running: true, outBytes: 0 }] });
    await pc.ready;
    const p = await nextPull(pc, 0);
    if (!p) break;
    tries++;
    answer(pc, p, { cut: true });
    pc.ws.terminate();
    await until(() => !svc.isOnline(k1.id));
    const settled = await Promise.race([pulling.then(() => true), sleep(150).then(() => false)]);
    if (settled) break;
  }
  const r = await pulling;
  ok(!r.ok && tries === 3 && /3 times in a row/.test(r.error) && r.error.includes('Nothing is lost'), 'a PC that drops on every try: 3 tries, then an error that says where the work is', `${tries} ${JSON.stringify(r)}`);
}

// ── A long transfer keeps the link alive (any message counts, not only pongs) ──
{
  const src = readFileSync(new URL('../src/workers.js', import.meta.url), 'utf8');
  ok(/ws\.on\('message'[\s\S]{0,300}link\.lastPong = now\(\)/.test(src), 'every message from the PC counts as a sign of life');
  ok(/maxPayload: MAX_PAYLOAD/.test(src) && /const MAX_PAYLOAD = 64 \* 1024 \* 1024/.test(src), 'the link takes messages up to 64 MB');
}

svc.dispose();
try { pc.ws.close(); } catch { /* gone */ }
server.close();
await sleep(50);
try { rmSync(dir, { recursive: true, force: true }); } catch { /* temp */ }
console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
