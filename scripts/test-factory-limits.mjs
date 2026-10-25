#!/usr/bin/env node
/**
 * test-factory-limits.mjs — a run's usage limits, all the person's choice.
 *
 * Covers: the limits cleaned, with defaults that keep today's behaviour; at
 * most N people at once, the rest waiting for a seat and starting as seats
 * free; raising the limit on a running run starts whoever waits; the pause
 * threshold below 100% pausing early and resuming after the reset; one at a
 * time above a share of the 5-hour window; the Manager pausing and resuming
 * with a reason, and nobody else; a pause that stops the turns in progress
 * (mode 'now') and carries them on at resume; someone woken by a background
 * build while paused or held by the Manager told to hold, never nudged; a
 * pause that resumes by itself once the usage window resets (resumeOnReset).
 *
 * Run: node scripts/test-factory-limits.mjs
 */
import { mkdtempSync, rmSync, writeFileSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';

const base = mkdtempSync(join(tmpdir(), 'factory-limits-'));
process.env.DATA_DIR = join(base, 'data');
process.env.FACTORY_WORKTREES_DIR = join(base, 'worktrees');
const M = await import('../src/factory-model.js');
const { createFactoryRunner } = await import('../src/factory-runner.js');
const { standard, fakeTeam } = await import('./factory-fixtures.mjs');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
async function until(fn, ms = 20000) { const end = Date.now() + ms; while (Date.now() < end) { const v = await fn(); if (v) return v; await sleep(15); } return null; }

// ─── Cleaning ───
const d = M.normalizeLimits(undefined);
ok(d.pauseAt5h === 100 && d.pauseAtWeek === 100 && d.maxParallel === 0 && d.oneAtATimeAbove === 0, 'defaults: today\'s behaviour (pause only when full, no limit)');
const c = M.normalizeLimits({ pauseAt5h: 20, pauseAtWeek: 140, maxParallel: -3, oneAtATimeAbove: 5 });
ok(c.pauseAt5h === 50 && c.pauseAtWeek === 100 && c.maxParallel === 0 && c.oneAtATimeAbove === 10, 'cleaned: thresholds kept between 50 and 100, nonsense dropped');
ok(M.normalizeFactory(standard()).settings.limits.pauseAt5h === 100, 'model: every factory has limits');

function repo(name) {
  const dir = join(base, 'projects', name); mkdirSync(dir, { recursive: true });
  const g = (a) => execFileSync('git', ['-c', 'user.name=t', '-c', 'user.email=t@t', ...a], { cwd: dir });
  g(['init', '-q', '-b', 'dev']); writeFileSync(join(dir, 'a.txt'), 'a'); g(['add', '.']); g(['commit', '-qm', 'i']);
  return dir;
}
function setup(name, limits, usage = { value: null }, extra = {}) {
  const f = standard(); f.settings = { presets: [], limits };
  const go = { fe: false, be: false };
  const team = fakeTeam({
    manager: async (msg) => msg.includes('answers.dPlan') ? { payload: { summary: 'go', answers: { dPlan: 'no', dWho: ['ui', 'backend'] } } } : null,
    fe: async () => null, be: async () => null,
  });
  const r = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: repo(name) }), getFactory: () => ({ factory: f }), settleMs: 10_000, permissionBatchMs: 30, ...extra });
  team.setRunner(r);
  return { team, r, go };
}

// ─── At most one at once ───
{
  const { team, r } = setup('l1', { maxParallel: 1 });
  const st = await r.startRun({ project: 'l1', factoryId: 'x', task: 'Do it' });
  await until(() => team.of('fe')[0] || team.of('be')[0]);
  await sleep(150);
  const started = ['fe', 'be'].filter(k => team.of(k)[0]?.msgs.length);
  const run = r.getRun(st.run.id);
  ok(started.length === 1 && run.waitingForSeat.length === 1, 'one at once: one developer starts, the other waits for a seat');
  ok(run.limits.maxParallel === 1, 'summary: the limits in force');
  const first = started[0], second = first === 'fe' ? 'be' : 'fe';
  const h = await r.handoff(team.of(first)[0].id, { summary: 'done' });
  ok(h.ok, 'handoff: the first hands off');
  await until(() => team.of(second)[0]?.msgs.length);
  ok(team.of(second)[0]?.msgs.length > 0 && r.getRun(st.run.id).waitingForSeat.length === 0, 'seat freed: the second starts');
  await r.stopRun(st.run.id); r.dispose();
}

// ─── Raising the limit on a running run ───
{
  const { team, r } = setup('l2', { maxParallel: 1 });
  const st = await r.startRun({ project: 'l2', factoryId: 'x', task: 'Do it' });
  await until(() => r.getRun(st.run.id).waitingForSeat.length === 1);
  const res = await r.setLimits(st.run.id, { maxParallel: 0 });
  await until(() => team.of('fe')[0]?.msgs.length && team.of('be')[0]?.msgs.length);
  ok(res.ok && res.run.limits.maxParallel === 0 && team.of('fe')[0]?.msgs.length && team.of('be')[0]?.msgs.length, 'run limits: lifting the limit starts whoever waited');
  await r.stopRun(st.run.id); r.dispose();
}

// ─── Pause threshold and one at a time ───
{
  const usage = { fiveHour: { utilization: 60, resetsAt: new Date(Date.now() + 3600e3).toISOString() }, week: { utilization: 10 } };
  const { team, r } = setup('l3', { pauseAt5h: 80, oneAtATimeAbove: 50 });
  const w = r.watchUsage(async () => usage, { tickMs: 3600e3 });
  const st = await r.startRun({ project: 'l3', factoryId: 'x', task: 'Do it' });
  await until(() => r.getRun(st.run.id).waitingForSeat.length === 1);
  ok(r.getRun(st.run.id).waitingForSeat.length === 1, 'one at a time above 50%: at 60% the second developer waits');
  usage.fiveHour = { utilization: 85, resetsAt: new Date(Date.now() + 400).toISOString() };
  await w.tick();
  ok(r.getRun(st.run.id).paused?.by === 'usage', 'threshold: at 85% with the line at 80% the run pauses early');
  await sleep(500);   // the window resets at the time recorded when it paused
  usage.fiveHour = { utilization: 5, resetsAt: new Date(Date.now() + 5 * 3600e3).toISOString() };
  await w.tick();
  await until(() => !r.getRun(st.run.id).paused && r.getRun(st.run.id).waitingForSeat.length === 0);
  ok(!r.getRun(st.run.id).paused && r.getRun(st.run.id).waitingForSeat.length === 0, 'after the reset: it resumes, and the waiting developer starts');
  w.stop(); await r.stopRun(st.run.id); r.dispose();
}

// ─── The Manager pauses and resumes ───
{
  const { team, r } = setup('l4', {});
  const st = await r.startRun({ project: 'l4', factoryId: 'x', task: 'Do it' });
  await until(() => team.of('fe')[0]);
  const mgr = team.of('manager')[0];
  ok(!(await r.controlFromManager(team.of('fe')[0].id, { action: 'pause' })).ok, 'control: only the Manager');
  const p = await r.controlFromManager(mgr.id, { action: 'pause', reason: 'The person wants headroom' });
  ok(p.ok && r.getRun(st.run.id).paused?.by === 'manager' && r.getRun(st.run.id).paused.reason === 'The person wants headroom', 'control: paused by the Manager, with its reason');
  const q = await r.controlFromManager(mgr.id, { action: 'resume' });
  ok(q.ok && !r.getRun(st.run.id).paused, 'control: resumed');
  const l = await r.controlFromManager(mgr.id, { action: 'limits', limits: { maxParallel: 2 } });
  ok(l.ok && r.getRun(st.run.id).limits.maxParallel === 2, 'control: the Manager changes the limits when asked');
  await r.stopRun(st.run.id); r.dispose();
}

// ─── The Manager messages someone mid-step ───
{
  const { team, r } = setup('l6', {});
  const st = await r.startRun({ project: 'l6', factoryId: 'x', task: 'Do it' });
  await until(() => team.of('fe')[0]?.msgs.length);
  const mgr = team.of('manager')[0], fe = team.of('fe')[0];
  const m = await r.messageFromManager(mgr.id, 'Frontend dev', 'Delete your Terminals.kt; the workspace owns terminals.');
  ok(m.ok && /A message from the Manager while you work on "Frontend dev"/.test(fe.msgs.at(-1)) && /Terminals\.kt/.test(fe.msgs.at(-1)) && /does not end it/.test(fe.msgs.at(-1)), 'message: reaches the running step, saying it does not end it');
  ok(!(await r.messageFromManager(fe.id, 'Backend dev', 'x')).ok, 'message: only the Manager sends them');
  let interrupted = 0; team.agents.interrupt = () => { interrupted++; };
  const h = await r.messageFromManager(mgr.id, 'Frontend dev', 'Stop: the build lock is jammed.', { interrupt: true, hold: true });
  ok(h.ok && interrupted === 1 && /your turn was stopped for it/.test(fe.msgs.at(-1)) && /Stop where you are and wait/.test(fe.msgs.at(-1)), 'message: interrupt stops their turn first; hold tells them to wait');
  ok(r.getRun(st.run.id).employees.fe.held === true, 'summary: shows them held');
  const go = await r.messageFromManager(mgr.id, 'Frontend dev', 'Lock is free.');
  ok(go.ok && /You may carry on with your step now/.test(fe.msgs.at(-1)) && !r.getRun(st.run.id).employees.fe.held, 'message: the next message releases them');
  const idle = await r.messageFromManager(mgr.id, 'Security dev', 'x');
  ok(!idle.ok && /not working on a step/.test(idle.error) && /Working now: .*Frontend dev/.test(idle.error), 'message: refused for someone not mid-step, naming who is');
  await r.stopRun(st.run.id); r.dispose();
}

// ─── Pause: let current steps finish, or stop them now ───
{
  const { team, r } = setup('p1', {});
  const st = await r.startRun({ project: 'p1', factoryId: 'x', task: 'Do it' });
  await until(() => team.of('fe')[0]?.msgs.length && team.of('be')[0]?.msgs.length);
  const fe = team.of('fe')[0], be = team.of('be')[0], mgr = team.of('manager')[0];
  ok(!(await r.pause(st.run.id, { mode: 'sideways' })).ok, 'pause now: an unknown mode is refused');
  const stoppedChats = [];
  team.agents.interrupt = (id) => { stoppedChats.push(id); team.sessions.get(id).state = 'idle'; };
  fe.state = 'working'; mgr.state = 'working';   // be is resting between turns
  const p = await r.pause(st.run.id, { mode: 'now' });
  const paused = r.getRun(st.run.id).paused;
  ok(p.ok && paused.mode === 'now' && paused.stopped.includes('fe') && paused.stopped.includes('manager') && !paused.stopped.includes('be'), 'pause now: whoever is mid-turn (the Manager too) is recorded as stopped; someone resting is not');
  ok(stoppedChats.includes(fe.id) && stoppedChats.includes(mgr.id) && !stoppedChats.includes(be.id), 'pause now: their turns are interrupted');
  ok(!!r.getRun(st.run.id).employees.fe.busy, 'pause now: the step stays theirs');
  const feN = fe.msgs.length, mgrN = mgr.msgs.length;
  const res = await r.resume(st.run.id);
  ok(res.ok && fe.msgs.length > feN && /You were interrupted: the person paused the run; it is resumed now\. Your turn was stopped by the pause; pick up where you were/.test(fe.msgs.at(-1)), 'resume: a stopped step is told its turn was stopped by the pause and carries on');
  ok(mgr.msgs.length > mgrN && /The pause stopped your turn; carry on where you were/.test(mgr.msgs.at(-1)), 'resume: the stopped Manager is told to carry on');
  ok(!/stopped by the pause/.test(be.msgs.at(-1)), 'resume: someone who was not stopped is not told they were');
  // The Manager pausing "now" does not stop its own turn: that turn is the one asking.
  stoppedChats.length = 0; fe.state = 'working'; mgr.state = 'working';
  const c = await r.controlFromManager(mgr.id, { action: 'pause', mode: 'now', reason: 'Build lock' });
  ok(c.ok && /turns in progress were stopped/.test(c.message) && stoppedChats.includes(fe.id) && !stoppedChats.includes(mgr.id) && r.getRun(st.run.id).paused.by === 'manager', 'factory_control pause now: stops the team, not the Manager itself');
  await r.resume(st.run.id);
  const plain = await r.pause(st.run.id);
  ok(plain.ok && !r.getRun(st.run.id).paused.mode && !r.getRun(st.run.id).paused.resumeOnReset, 'plain pause: as before, nobody stopped');
  await r.stopRun(st.run.id); r.dispose();
}

// ─── Woken by a background build while paused, or held by the Manager ───
{
  const { team, r } = setup('p2', {}, undefined, { settleMs: 60, wokeGraceMs: 300 });   // grace wide enough for a slow CI runner
  const st = await r.startRun({ project: 'p2', factoryId: 'x', task: 'Do it' });
  await until(() => team.of('fe')[0]?.msgs.length && team.of('be')[0]?.msgs.length);
  const fe = team.of('fe')[0], mgr = team.of('manager')[0];
  let interrupts = 0; team.agents.interrupt = () => { interrupts++; };
  await sleep(400);   // past the grace since the last message, so the next turn is a wake-up
  await r.pause(st.run.id);
  const n0 = fe.msgs.length;
  team.emit(fe.id, { type: 'state', state: 'working' });   // a background command finished: nothing was sent
  await until(() => fe.msgs.length > n0, 2000);
  ok(fe.msgs.length === n0 + 1 && interrupts === 1 && /The run is paused, and your chat started a new turn on its own/.test(fe.msgs.at(-1)) && /Stop where you are and wait\. Do not hand off and do nothing else until the run is resumed/.test(fe.msgs.at(-1)), 'paused: a turn that starts by itself is stopped and told to hold, in the hold\'s words');
  team.emit(fe.id, { type: 'state', state: 'idle' });
  await sleep(250);
  ok(fe.msgs.length === n0 + 1 && !r.getRun(st.run.id).tokens.some(t => t.status === 'answer' && /stopped twice/.test(JSON.stringify(t))), 'paused: no stall nudge and no escalation after it');
  ok(r.runEvents(st.run.id).events.some(e => e.type === 'hold' && e.employeeId === 'fe' && e.why === 'paused'), 'paused: the hold is in the journal');
  await r.resume(st.run.id);
  ok(/You were interrupted: .*You were told to hold while it was paused; you may carry on now/.test(fe.msgs.at(-1)), 'resume: someone told to hold is told to carry on');
  // A turn started by a message (the runner, the Manager or the person) is not a wake-up.
  await sleep(80);
  const m = await r.messageFromManager(mgr.id, 'Frontend dev', 'The build lock is jammed.', { hold: true });
  const n1 = fe.msgs.length;
  team.emit(fe.id, { type: 'state', state: 'working' });
  await sleep(100);
  ok(m.ok && fe.msgs.length === n1 && interrupts === 1, 'a turn right after a message is not taken for a wake-up');
  team.emit(fe.id, { type: 'state', state: 'idle' });
  await sleep(400);   // past the grace since the Manager's message
  team.emit(fe.id, { type: 'state', state: 'working' });   // the build again, while the Manager holds them
  await until(() => fe.msgs.length > n1, 2000);
  ok(fe.msgs.length === n1 + 1 && /The Manager told you to wait, and your chat started a new turn on its own/.test(fe.msgs.at(-1)) && /until the Manager tells you to carry on/.test(fe.msgs.at(-1)), 'Manager hold: a wake-up is told to keep holding');
  team.emit(fe.id, { type: 'state', state: 'idle' });
  await sleep(250);
  ok(fe.msgs.length === n1 + 1 && r.getRun(st.run.id).employees.fe.held, 'Manager hold: not taken for a stall, still held');
  // Not paused, not held: a wake-up is the employee's own business, as before.
  await r.messageFromManager(mgr.id, 'Frontend dev', 'Go on.');
  await sleep(100);
  team.emit(fe.id, { type: 'state', state: 'idle' });
  const n2 = fe.msgs.length;
  team.emit(fe.id, { type: 'state', state: 'working' });
  await sleep(100);
  ok(fe.msgs.length === n2, 'running: a wake-up is left alone');
  await r.stopRun(st.run.id); r.dispose();
}

// ─── Resume by itself when the usage window resets ───
{
  const notes = [];
  const usage = { fiveHour: { utilization: 100, resetsAt: new Date(Date.now() + 400).toISOString() }, week: { utilization: 20, resetsAt: new Date(Date.now() + 5 * 86400e3).toISOString() } };
  const { team, r } = setup('p3', {}, undefined, { notify: (kind, run, text) => notes.push({ kind, text }) });
  const w = r.watchUsage(async () => usage, { tickMs: 3600e3 });
  const st = await r.startRun({ project: 'p3', factoryId: 'x', task: 'Do it' });
  await until(() => team.of('fe')[0]?.msgs.length && team.of('be')[0]?.msgs.length);
  const fe = team.of('fe')[0], mgr = team.of('manager')[0];
  // Held by the Manager before the pause: let go when it resumes.
  await r.messageFromManager(mgr.id, 'Frontend dev', 'Wait for the lock.', { hold: true });
  usage.fiveHour.resetsAt = new Date(Date.now() + 1500).toISOString();   // from here, not from before the run started
  const p = await r.pause(st.run.id, { resumeOnReset: true });
  const paused = r.getRun(st.run.id).paused;
  ok(p.ok && paused.by === 'person' && paused.resumeOnReset && paused.windows.join() === 'fiveHour' && paused.resetsAt === usage.fiveHour.resetsAt, 'resumeOnReset: the full window and its reset are recorded with the pause');
  await w.tick();
  ok(!!r.getRun(st.run.id).paused, 'resumeOnReset: still paused while the window is full');
  usage.fiveHour = { utilization: 3, resetsAt: usage.fiveHour.resetsAt };
  await w.tick();
  ok(!!r.getRun(st.run.id).paused, 'resumeOnReset: still paused before the recorded reset, even under the line');
  await until(() => Date.now() > Date.parse(paused.resetsAt) + 50, 3000);
  usage.fiveHour = { utilization: 3, resetsAt: new Date(Date.now() + 5 * 3600e3).toISOString() };
  await w.tick();
  ok(!r.getRun(st.run.id).paused && notes.some(x => x.kind === 'resumed' && /carries on: the usage window has reset/.test(x.text)), 'resumeOnReset: after the reset it resumes by itself, and says so');
  ok(!r.getRun(st.run.id).employees.fe.held && /the person paused the run until the usage window reset; it has reset now\. The Manager had told you to wait; you may carry on now/.test(fe.msgs.at(-1)), 'resumeOnReset: a held employee is let go and told to carry on');
  // A plain pause stays paused whatever the usage does.
  await r.pause(st.run.id);
  await w.tick();
  ok(!!r.getRun(st.run.id).paused, 'plain pause: usage resetting does not resume it');
  await r.resume(st.run.id);
  // Nothing full: it waits for the 5-hour window's next reset.
  usage.fiveHour = { utilization: 40, resetsAt: new Date(Date.now() + 1500).toISOString() };
  const c = await r.controlFromManager(mgr.id, { action: 'pause', resumeOnReset: true, reason: 'Headroom' });
  ok(c.ok && /resumes by itself when the usage window resets/.test(c.message) && r.getRun(st.run.id).paused.by === 'manager' && r.getRun(st.run.id).paused.resumeOnReset && !r.getRun(st.run.id).paused.windows.length && r.getRun(st.run.id).paused.resetsAt === usage.fiveHour.resetsAt, 'factory_control resumeOnReset: nothing full, so it waits for the 5-hour reset');
  ok(notes.some(x => x.kind === 'paused' && /carries on by itself when the usage window resets/.test(x.text)), 'resumeOnReset: the Manager\'s pause notice says it carries on by itself');
  await w.tick();
  ok(!!r.getRun(st.run.id).paused, 'resumeOnReset: not before that reset');
  await until(() => Date.now() > Date.parse(usage.fiveHour.resetsAt) + 50, 3000);
  await w.tick();
  ok(!r.getRun(st.run.id).paused, 'resumeOnReset: resumed after it');
  w.stop(); await r.stopRun(st.run.id); r.dispose();
}

// ─── Resume on reset, paused with no usage reading at the time ───
{
  const { team, r } = setup('p4', {});
  const st = await r.startRun({ project: 'p4', factoryId: 'x', task: 'Do it' });
  await until(() => team.of('fe')[0]?.msgs.length);
  const p = await r.pause(st.run.id, { resumeOnReset: true });   // nothing reads usage yet
  ok(p.ok && r.getRun(st.run.id).paused.resumeOnReset && !r.getRun(st.run.id).paused.resetsAt, 'no reading: the pause waits for the reset without knowing when it is');
  const usage = { fiveHour: { utilization: 30, resetsAt: new Date(Date.now() + 1500).toISOString() }, week: { utilization: 10, resetsAt: new Date(Date.now() + 5 * 86400e3).toISOString() } };
  const w = r.watchUsage(async () => usage, { tickMs: 3600e3 });
  await w.tick();
  ok(!!r.getRun(st.run.id).paused && r.getRun(st.run.id).paused.resetsAt === usage.fiveHour.resetsAt, 'no reading: the first one, under the line, records the 5-hour reset and does not resume');
  ok(r.runEvents(st.run.id).events.some(e => e.type === 'pause-until' && e.resetsAt === usage.fiveHour.resetsAt), 'no reading: the reset it waits for is in the journal');
  await w.tick();
  ok(!!r.getRun(st.run.id).paused, 'no reading: still paused before that reset');
  await until(() => Date.now() > Date.parse(r.getRun(st.run.id).paused.resetsAt) + 50, 3000);
  usage.fiveHour = { utilization: 2, resetsAt: new Date(Date.now() + 5 * 3600e3).toISOString() };
  await w.tick();
  ok(!r.getRun(st.run.id).paused, 'no reading: resumed after the 5-hour reset');
  w.stop(); await r.stopRun(st.run.id); r.dispose();
}

// ─── The Manager's ticker ───
{
  const f = standard(); f.settings = { presets: [] };
  const team = fakeTeam({ manager: async () => null, fe: async () => null, be: async () => null });
  const r = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: repo('l7') }), getFactory: () => ({ factory: f }), settleMs: 10_000, permissionBatchMs: 30, tickScale: 2 / 60000 });   // a minute is 2 ms
  team.setRunner(r);
  const st = await r.startRun({ project: 'l7', factoryId: 'x', task: 'Do it' });
  const mgr = team.of('manager')[0];
  const ticks = () => mgr.msgs.filter(m => m.startsWith('[Factory] Tick')).length;
  await until(() => ticks() > 0, 3000);
  ok(ticks() > 0 && /not a task/.test(mgr.msgs.find(m => m.startsWith('[Factory] Tick'))) && /have not set your ticker/.test(mgr.msgs.find(m => m.startsWith('[Factory] Tick'))), 'tick: an idle Manager is woken, told it is not a task, and asked to set its ticker');
  ok(/Set how often at the start with factory_tick/.test(mgr.systemPromptExtra), 'prompt: the Manager is told to set its ticker');
  mgr.state = 'working';
  const before = ticks(); await sleep(150);
  ok(ticks() === before, 'tick: skipped entirely while the Manager is busy');
  mgr.state = 'idle';
  ok(!(await r.tickFromManager(mgr.id, 2)).ok && !(await r.tickFromManager(mgr.id, 2000)).ok, 'factory_tick: 5 minutes to 24 hours');
  const set = await r.tickFromManager(mgr.id, 30);
  ok(set.ok && /not a guaranteed timer/.test(set.message), 'factory_tick: set, and told it is a wake-up, not a timer');
  await r.pause(st.run.id);
  const p0 = ticks(); await sleep(200);
  ok(ticks() === p0, 'tick: none while the run is paused');
  await r.stopRun(st.run.id); r.dispose();
}
{
  const f = standard(); f.settings = { presets: [], tickMinutes: 60 };
  const team = fakeTeam({ manager: async () => null, fe: async () => null, be: async () => null });
  const r = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: repo('l8') }), getFactory: () => ({ factory: f }), settleMs: 10_000, permissionBatchMs: 30 });
  team.setRunner(r);
  const st = await r.startRun({ project: 'l8', factoryId: 'x', task: 'Do it' });
  const mgr = team.of('manager')[0];
  const no = await r.tickFromManager(mgr.id, 10);
  ok(!no.ok && /set your ticker to every 60 minutes/.test(no.error) && /every 60 minutes \(the person set it\)/.test(mgr.systemPromptExtra), 'ticker fixed in the factory: the Manager is told, and cannot change it');
  await r.stopRun(st.run.id); r.dispose();
}

// ─── Paused, restarted, recovered while paused, then resumed ───
{
  const f = standard(); f.settings = { presets: [] };
  const team = fakeTeam({
    manager: async (msg) => msg.includes('answers.dPlan') ? { payload: { summary: 'go', answers: { dPlan: 'no', dWho: ['ui', 'backend'] } } } : null,
    fe: async () => null, be: async () => null,
  });
  const opts = { dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: repo('l5') }), getFactory: () => ({ factory: f }), settleMs: 10_000, permissionBatchMs: 30 };
  let r = createFactoryRunner(opts); team.setRunner(r);
  const st = await r.startRun({ project: 'l5', factoryId: 'x', task: 'Do it' });
  await until(() => team.of('fe')[0]?.msgs.length && team.of('be')[0]?.msgs.length);
  await r.pause(st.run.id);
  r.dispose();
  for (const sid of [...team.sessions.keys()]) team.sessions.get(sid).alive = false;   // the restart ends every chat
  r = createFactoryRunner(opts); team.setRunner(r); r.load();
  const rec = await r.recover(st.run.id);
  const feBefore = team.of('fe').reduce((n, x) => n + x.msgs.length, 0);
  ok(rec.ok && r.getRun(st.run.id).paused, 'recovered while paused: the steps wait for Resume');
  const res = await r.resume(st.run.id);
  await until(() => team.of('fe').reduce((n, x) => n + x.msgs.length, 0) > feBefore && team.of('be').some(x => x.alive && x.msgs.some(m => /interrupted/.test(m))));
  ok(res.ok && team.of('fe').some(x => x.alive && x.msgs.some(m => /You were interrupted/.test(m))) && team.of('be').some(x => x.alive && x.msgs.some(m => /You were interrupted/.test(m))), 'resume: steps whose chats ended with the restart are started again, told to carry on');
  await r.stopRun(st.run.id); r.dispose();
}

try { rmSync(base, { recursive: true, force: true, maxRetries: 10, retryDelay: 300 }); } catch { /* a temp folder */ }
console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
