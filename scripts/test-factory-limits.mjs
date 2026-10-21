#!/usr/bin/env node
/**
 * test-factory-limits.mjs — a run's usage limits, all the person's choice.
 *
 * Covers: the limits cleaned, with defaults that keep today's behaviour; at
 * most N people at once, the rest waiting for a seat and starting as seats
 * free; raising the limit on a running run starts whoever waits; the pause
 * threshold below 100% pausing early and resuming after the reset; one at a
 * time above a share of the 5-hour window; the Manager pausing and resuming
 * with a reason, and nobody else.
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
function setup(name, limits, usage = { value: null }) {
  const f = standard(); f.settings = { presets: [], limits };
  const go = { fe: false, be: false };
  const team = fakeTeam({
    manager: async (msg) => msg.includes('answers.dPlan') ? { payload: { summary: 'go', answers: { dPlan: 'no', dWho: ['ui', 'backend'] } } } : null,
    fe: async () => null, be: async () => null,
  });
  const r = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: repo(name) }), getFactory: () => ({ factory: f }), settleMs: 10_000, permissionBatchMs: 30 });
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
