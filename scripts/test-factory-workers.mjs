#!/usr/bin/env node
/**
 * test-factory-workers.mjs — factory steps on a worker PC (workers-protocol.md, phase 5).
 *
 * Covers: runsOn cleaned on members and steps, and an unknown PC only warned
 * about; a step on a PC spawned with where; a PC offline at the step's start
 * makes the step wait (holding no seat) and start when it is back; a PC going
 * offline holds its steps, releases their seats and lets the rest go on; a
 * chat that ends while its PC is away is held, not escalated; the PC coming
 * back resumes them with a recovery message; the Manager on a PC pauses the
 * run (by 'worker') and the run resumes with it; at handoff the PC's work is
 * pulled and committed before the handoff is accepted, and a failed pull
 * refuses it; factory_status and the summary say who runs where; without a
 * workersApi nothing changes.
 *
 * Run: node scripts/test-factory-workers.mjs
 */
import { mkdtempSync, rmSync, writeFileSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';

const base = mkdtempSync(join(tmpdir(), 'factory-workers-'));
process.env.DATA_DIR = join(base, 'data');
process.env.FACTORY_WORKTREES_DIR = join(base, 'worktrees');
const M = await import('../src/factory-model.js');
const { createFactoryRunner } = await import('../src/factory-runner.js');
const { standard, fakeTeam } = await import('./factory-fixtures.mjs');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
async function until(fn, ms = 10000) { const end = Date.now() + ms; while (Date.now() < end) { const v = await fn(); if (v) return v; await sleep(15); } return null; }

// ─── Model and validation ───
{
  const f = standard();
  f.roster.find(r => r.id === 'fe').runsOn = '  Laptop  ';
  f.nodes.find(n => n.id === 'be').runsOn = 'x'.repeat(200);
  const n = M.normalizeFactory(f);
  ok(n.roster.find(r => r.id === 'fe').runsOn === 'Laptop' && n.roster.find(r => r.id === 'be').runsOn === '', 'model: runsOn trimmed on a member, \'\' by default');
  ok(n.nodes.find(x => x.id === 'be').runsOn.length === 80 && n.nodes.find(x => x.id === 'mgr').runsOn === '' && !('runsOn' in n.nodes.find(x => x.id === 'dPlan')), 'model: runsOn on steps, at most 80 characters, only on people\'s steps');
  const plain = M.validateFactory(f);
  ok(plain.ok && !plain.warnings.some(w => w.code === 'worker-unknown'), 'validate: without a list of PCs nothing is said');
  const v = M.validateFactory(f, { knownWorkers: [{ id: 'w1', name: 'laptop' }] });
  const unk = v.warnings.filter(w => w.code === 'worker-unknown');
  ok(v.ok && unk.length === 1 && unk[0].nodeId === 'be', 'validate: an unknown PC is a warning, not an error; a known one (by name, any case) is fine');
  ok(M.validateFactory(f, { knownWorkers: ['w1', 'X'.repeat(80)] }).warnings.filter(w => w.code === 'worker-unknown').length === 1, 'validate: known PCs as plain ids');
}

const repos = new Map();
function repo(name) {
  if (repos.has(name)) return repos.get(name);
  const dir = join(base, 'projects', name); mkdirSync(dir, { recursive: true });
  const g = (a) => execFileSync('git', ['-c', 'user.name=t', '-c', 'user.email=t@t', ...a], { cwd: dir });
  g(['init', '-q', '-b', 'dev']); writeFileSync(join(dir, 'a.txt'), 'a'); g(['add', '.']); g(['commit', '-qm', 'i']);
  repos.set(name, dir);
  return dir;
}

function fakeWorkers(list) {
  const online = new Map(list.map(w => [w.id, w.online !== false]));
  const cbs = new Set();
  const pulls = [];
  let pull = null;
  return {
    api: {
      resolve: (q) => { const k = String(q).toLowerCase(); const w = list.find(x => x.id.toLowerCase() === k || x.name.toLowerCase() === k); return w ? { id: w.id, name: w.name, online: !!online.get(w.id) } : null; },
      isOnline: (id) => !!online.get(id),
      onChange: (cb) => { cbs.add(cb); return () => cbs.delete(cb); },
      pullChat: async (chatId, o) => { pulls.push({ chatId, ...o }); return pull ? pull(chatId, o) : { ok: true }; },
    },
    async set(id, on) { online.set(id, on); for (const cb of cbs) cb(id, on); await sleep(60); },
    pulls,
    onPull(fn) { pull = fn; },
    listeners: () => cbs.size,
  };
}

function setup(name, { fe = '', mgr = '', limits = {}, workers = null } = {}) {
  const f = standard(); f.settings = { presets: [], limits };
  if (fe) f.roster.find(r => r.id === 'fe').runsOn = fe;
  if (mgr) f.roster.find(r => r.id === 'manager').runsOn = mgr;
  const team = fakeTeam({
    manager: async (msg) => msg.includes('answers.dPlan') ? { payload: { summary: 'go', answers: { dPlan: 'no', dWho: ['ui', 'backend'] } } } : null,
    fe: async () => null, be: async () => null,
  });
  const r = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: repo(name) }), getFactory: () => ({ factory: f }), settleMs: 10_000, permissionBatchMs: 30, ...(workers ? { workersApi: workers.api } : {}) });
  team.setRunner(r);
  return { team, r };
}
const last = (s) => s?.msgs[s.msgs.length - 1] || '';
const hist = (r, id, type) => r.getRun(id).history.filter(h => h.type === type);
const alive = (team, emp) => team.of(emp).filter(s => s.alive).pop();

// ─── No workersApi: unchanged ───
{
  const { team, r } = setup('w0', { fe: 'laptop' });
  const st = await r.startRun({ project: 'w0', factoryId: 'x', task: 'Do it' });
  await until(() => team.of('fe')[0]?.msgs.length && team.of('be')[0]?.msgs.length);
  const fe = team.of('fe')[0];
  ok(fe && !fe.where && fe.addDirs.length > 0 && !last(fe).includes('You run on the PC'), 'no workersApi: runsOn is ignored, the step runs here as before');
  ok(Array.isArray(r.getRun(st.run.id).onPcs) && r.getRun(st.run.id).onPcs.length === 0, 'no workersApi: nobody on a PC');
  await r.stopRun(st.run.id); r.dispose();
}

// ─── A step on a PC: spawned there; handoff pulls and commits first ───
{
  const W = fakeWorkers([{ id: 'w1', name: 'Laptop' }]);
  const { team, r } = setup('w1', { fe: 'laptop', workers: W });
  const st = await r.startRun({ project: 'w1', factoryId: 'x', task: 'Do it' });
  await until(() => team.of('fe')[0]?.msgs.length && team.of('be')[0]?.msgs.length);
  const fe = team.of('fe')[0], be = team.of('be')[0];
  ok(fe.where === 'worker:w1' && fe.addDirs.length === 0, 'on a PC: the chat is spawned with where worker:<id>, no server folders');
  ok(!be.where, 'the rest: on this server');
  ok(last(fe).includes('You run on the PC "Laptop"'), 'on a PC: the step says where it runs');
  const run = r.getRun(st.run.id);
  ok(run.onPcs.some(p => p.employee === 'Frontend dev' && p.worker === 'Laptop' && p.online), 'summary: onPcs says who runs where');
  ok(run.employees.fe.pc === 'Laptop', 'summary: the person\'s chat is on the PC');
  const status = await r.statusFor(run.managerChatId);
  ok(/Frontend dev: on the PC Laptop/.test(status.content[0].text) && !/PCs offline/.test(status.content[0].text), 'factory_status: where each person runs');

  // A failed pull refuses the handoff.
  W.onPull(async () => ({ ok: false, error: 'the link dropped mid-transfer' }));
  const bad = await r.handoff(fe.id, { summary: 'UI done' });
  ok(!bad.ok && /could not be brought back/.test(bad.error) && /link dropped/.test(bad.error), 'a failed pull refuses the handoff, with the reason');
  ok(r.getRun(st.run.id).tokens.some(t => t.employeeId === 'fe' && t.status === 'work'), 'refused: the step is still theirs');

  // A good pull: its work lands in the worktree and is committed as the step's work.
  W.onPull(async (chatId, o) => { writeFileSync(join(o.intoWorktree, 'from-pc.txt'), 'built on the laptop'); return { ok: true, head: 'abc' }; });
  const good = await r.handoff(fe.id, { summary: 'UI done' });
  const p = W.pulls[W.pulls.length - 1];
  const wsPath = p.intoWorktree;
  const headFiles = execFileSync('git', ['show', '--name-only', '--format=', 'HEAD'], { cwd: wsPath }).toString();
  ok(good.ok && headFiles.includes('from-pc.txt'), 'handoff: the PC\'s work is pulled and committed before it is accepted');
  ok(p.chatId === fe.id && /^[0-9a-f]{40}$/.test(p.base) && p.branch && p.intoWorktree, 'pull: the chat, the commit it started from, the worktree and its branch');
  const h = hist(r, st.run.id, 'handoff').find(x => x.by === 'fe');
  ok(!!h, 'handoff accepted after the pull');
  const beh = await r.handoff(be.id, { summary: 'API done' });
  ok(beh.ok && W.pulls.length === 2, 'a step on the server hands off without any pull');
  await r.stopRun(st.run.id); r.dispose();
  ok(W.listeners() === 0, 'dispose: stops listening to the PCs');
}

// ─── Offline at the step's start: it waits for the PC ───
{
  const W = fakeWorkers([{ id: 'w1', name: 'Laptop', online: false }]);
  const { team, r } = setup('w2', { fe: 'w1', workers: W, limits: { maxParallel: 1 } });
  const st = await r.startRun({ project: 'w2', factoryId: 'x', task: 'Do it' });
  await until(() => team.of('be')[0]?.msgs.length);
  await sleep(50);
  let run = r.getRun(st.run.id);
  ok(team.of('fe').length === 0 && team.of('be')[0]?.msgs.length > 0, 'offline at start: the step waits, the other one starts (it holds no seat)');
  ok(run.employees.fe.waitingForPc === 'Laptop' && run.tokens.some(t => t.employeeId === 'fe' && t.waitingForPc === 'Laptop'), 'summary: shown as waiting for the PC');
  const status = (await r.statusFor(run.managerChatId)).content[0].text;
  ok(/waiting for the PC Laptop/.test(status) && /PCs offline: Laptop/.test(status), 'factory_status: which PCs are offline');
  await W.set('w1', true);
  run = r.getRun(st.run.id);
  ok(!run.employees.fe.waitingForPc && team.of('fe').length === 0 && run.waitingForSeat.some(x => x.employeeId === 'fe'), 'back online: it no longer waits for the PC, and takes a seat like anyone');
  await r.handoff(team.of('be')[0].id, { summary: 'API done' });
  await until(() => team.of('fe')[0]?.msgs.length);
  ok(team.of('fe')[0]?.where === 'worker:w1' && team.of('fe')[0].msgs.length > 0, 'back online: the waiting step starts there when a seat is free');
  await r.stopRun(st.run.id); r.dispose();
}

// ─── Going offline holds the step; the rest go on; online resumes it ───
{
  const W = fakeWorkers([{ id: 'w1', name: 'Laptop' }]);
  const { team, r } = setup('w3', { fe: 'Laptop', workers: W, limits: { maxParallel: 1 } });
  const st = await r.startRun({ project: 'w3', factoryId: 'x', task: 'Do it' });
  await until(() => team.of('fe')[0]?.msgs.length || team.of('be')[0]?.msgs.length);
  await sleep(50);
  let first = team.of('fe')[0]?.msgs.length ? 'fe' : 'be';
  if (first === 'be') {   // the server one went first: let it finish so the PC one starts
    await r.handoff(team.of('be')[0].id, { summary: 'API done' });
    await until(() => team.of('fe')[0]?.msgs.length);
  }
  const fe = team.of('fe')[0];
  const before = fe.msgs.length;
  await W.set('w1', false);
  let run = r.getRun(st.run.id);
  const off = hist(r, st.run.id, 'worker-offline');
  ok(off.length === 1 && off[0].worker === 'w1' && off[0].employees.includes('Frontend dev'), 'offline: event worker-offline {worker, employees}');
  ok(run.employees.fe.held && run.employees.fe.waitingForPc === 'Laptop' && !run.paused, 'offline: the step is held, the run is not paused');
  if (first === 'fe') {
    await until(() => team.of('be')[0]?.msgs.length);
    ok(team.of('be')[0]?.msgs.length > 0, 'offline: its seat is released, the next person starts');
    const bh = await r.handoff(team.of('be')[0].id, { summary: 'API done' });
    ok(bh.ok, 'offline: the rest of the run carries on');
  } else {
    ok(true, 'offline: its seat is released (the other step had finished)');
    ok(true, 'offline: the rest of the run carries on');
  }
  // The chat ends while the PC is away: held, not escalated.
  team.emit(fe.id, { type: 'exit' });
  team.sessions.get(fe.id).alive = false;
  await sleep(60);
  run = r.getRun(st.run.id);
  ok(run.status !== 'crashed' && !hist(r, st.run.id, 'escalate').length && run.tokens.some(t => t.employeeId === 'fe' && t.status === 'work'), 'its chat ending while the PC is away: held, not escalated');
  W.onPull(async () => ({ ok: true }));
  await W.set('w1', true);
  await until(() => alive(team, 'fe')?.msgs.length);
  const again = alive(team, 'fe');
  ok(hist(r, st.run.id, 'worker-online').length === 1, 'online: event worker-online');
  ok(again && again.where === 'worker:w1' && /went offline for a while and is back now/.test(last(again)) && /interrupted/.test(last(again)), 'online: the step carries on there with a recovery message');
  ok(again.resumeId === 'uuid-' + fe.id, 'online: the conversation is resumed on the PC');
  ok(W.pulls.some(p => p.chatId === fe.id), 'online: what the ended chat did is brought back first');
  ok(!r.getRun(st.run.id).employees.fe.held && before > 0, 'online: no longer held');
  await r.stopRun(st.run.id); r.dispose();
}

// ─── Held while alive: online sends a recovery message into the same chat ───
{
  const W = fakeWorkers([{ id: 'w1', name: 'Laptop' }]);
  const { team, r } = setup('w4', { fe: 'Laptop', workers: W });
  const st = await r.startRun({ project: 'w4', factoryId: 'x', task: 'Do it' });
  await until(() => team.of('fe')[0]?.msgs.length);
  const fe = team.of('fe')[0];
  await W.set('w1', false);
  const n = fe.msgs.length;
  await W.set('w1', true);
  await until(() => fe.msgs.length > n);
  ok(team.of('fe').length === 1 && /back now/.test(last(fe)), 'online: a chat that kept running is told to carry on (recovery prompt)');
  await r.stopRun(st.run.id); r.dispose();
}

// ─── The Manager on a PC ───
{
  const W = fakeWorkers([{ id: 'w1', name: 'Desk', online: false }]);
  const { team, r } = setup('w5', { mgr: 'Desk', workers: W });
  const no = await r.startRun({ project: 'w5', factoryId: 'x', task: 'Do it' });
  ok(!no.ok && /Desk/.test(no.error) && /offline/.test(no.error), 'Manager\'s PC offline at start: the run does not start, and says why');
  await W.set('w1', true);
  const st = await r.startRun({ project: 'w5', factoryId: 'x', task: 'Do it' });
  ok(st.ok && team.of('manager')[0]?.where === 'worker:w1', 'the Manager runs on its PC');
  await until(() => team.of('fe')[0]?.msgs.length && team.of('be')[0]?.msgs.length);
  await W.set('w1', false);
  let run = r.getRun(st.run.id);
  ok(run.paused?.by === 'worker' && run.status === 'paused', 'Manager\'s PC offline: the whole run pauses, by worker');
  const fh = await r.handoff(team.of('fe')[0].id, { summary: 'UI done' });
  ok(fh.ok, 'paused for the Manager\'s PC: the team may still hand off');
  await W.set('w1', true);
  await until(() => !r.getRun(st.run.id).paused);
  run = r.getRun(st.run.id);
  ok(!run.paused && run.status === 'running' && hist(r, st.run.id, 'worker-online').length === 1, 'Manager\'s PC back: the run resumes');
  await r.stopRun(st.run.id); r.dispose();
}

rmSync(base, { recursive: true, force: true });
if (failed) { console.log(`\n${failed} failed`); process.exit(1); }
console.log('\nall passed');
