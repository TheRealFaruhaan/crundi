#!/usr/bin/env node
/**
 * test-factory-recovery.mjs — the runner surviving interruptions.
 *
 * Covers: effects written ahead, so a crash in the middle of a fan-out leaves
 * on disk exactly what was still to do; a restart bringing the run back as
 * crashed; recover() finishing the half-done work, relaunching the Manager on
 * its chat with an overview, and giving each interrupted employee a continue
 * prompt (reason, last handoff, worktree, the person's note); pause holding new
 * steps while handoffs still route; the usage limit pausing runs, the stall
 * watcher staying quiet meanwhile, and the run carrying on by itself after the
 * reset; and the reachability collector stopping employees the current loop
 * cannot reach, then resuming them with their memory when the Manager sends
 * work their way again.
 *
 * Run: node scripts/test-factory-recovery.mjs
 */

import { mkdtempSync, rmSync, writeFileSync, mkdirSync, readFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';

const base = mkdtempSync(join(tmpdir(), 'factory-recover-'));
process.env.DATA_DIR = join(base, 'data');
process.env.FACTORY_WORKTREES_DIR = join(base, 'worktrees');
const { createFactoryRunner } = await import('../src/factory-runner.js');
const Git = await import('../src/factory-git.js');
const { standard, fakeTeam } = await import('./factory-fixtures.mjs');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
async function until(fn, ms = 20000) {
  const end = Date.now() + ms;
  while (Date.now() < end) { const v = await fn(); if (v) return v; await sleep(15); }
  return null;
}
function repo(name) {
  const dir = join(base, 'projects', name);
  mkdirSync(dir, { recursive: true });
  const g = (a) => execFileSync('git', a, { cwd: dir });
  g(['init', '-q', '-b', 'dev']); writeFileSync(join(dir, 'a.txt'), 'a\n');
  g(['-c', 'user.name=t', '-c', 'user.email=t@t', 'add', '.']); g(['-c', 'user.name=t', '-c', 'user.email=t@t', 'commit', '-q', '-m', 'i']);
  return dir;
}
const workIn = (msg) => (msg.match(/^Work in: (.+)$/m) || [])[1];
const runFile = (id) => JSON.parse(readFileSync(join(process.env.DATA_DIR, 'factory-runs', id, 'run.json'), 'utf-8'));

function brains(extra = {}) {
  let mgrSteps = 0;
  return {
    manager: async (msg) => {
      if (msg.startsWith('[Factory] This run is being recovered')) return null;
      if (msg.includes('answers.dSec')) return { payload: { summary: 'ok', answers: { dSec: 'no', dRev: [] } } };
      if (msg.includes('answers.dOk')) return { payload: { summary: 'approved', answers: { dOk: 'yes' } } };
      if (msg.includes('answers.dPlan')) {
        mgrSteps++;
        return mgrSteps === 1 ? { payload: { summary: 'build', answers: { dPlan: 'no', dWho: ['ui', 'backend'] } } } : { payload: { summary: 'All done', finish: true } };
      }
      return null;
    },
    fe: async (msg) => { writeFileSync(join(workIn(msg), 'ui.txt'), 'ui\n'); return { payload: { summary: 'UI done' } }; },
    be: async (msg) => { writeFileSync(join(workIn(msg), 'api.txt'), 'api\n'); return { payload: { summary: 'API done' } }; },
    ...extra,
  };
}

// ─── Crash in the middle of a fan-out, restart, recover ───
let crashedId = '';
{
  // The second branch worktree never gets made: the process "dies" there.
  let calls = 0;
  const hangingGit = { ...Git, createBranchWorktree: (o) => (++calls === 2 ? new Promise(() => {}) : Git.createBranchWorktree(o)) };
  const team = fakeTeam(brains({ fe: async () => null, be: async () => null }));
  const r = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: repo('crash') }), getFactory: () => ({ factory: standard() }), git: hangingGit, settleMs: 10_000 });
  team.setRunner(r);
  const st = await r.startRun({ project: 'crash', factoryId: 'x', task: 'Survive a crash' });
  crashedId = st.run.id;
  await until(() => calls === 2);
  await sleep(50);
  const disk = runFile(crashedId);
  const pendingTypes = disk.rec.run.pending.map(e => e.type);
  ok(pendingTypes[0] === 'workspace' && pendingTypes.filter(t => t === 'activate').length === 2, 'write-ahead: the unfinished workspace and both activations are on disk');
  ok(team.of('fe').length === 0 && team.of('be').length === 0, 'write-ahead: nobody was started past the point of the crash');
  r.dispose();
}
{
  const team = fakeTeam(brains());
  const r = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: join(base, 'projects', 'crash') }), getFactory: () => ({ factory: standard() }), settleMs: 10_000 });
  team.setRunner(r);
  r.load();
  const run = r.getRun(crashedId);
  ok(run.status === 'crashed' && /Crundi restarted/.test(run.crash.reason) && run.crash.from === 'running', 'restart: the run is crashed, saying why and what it was doing');
  ok(!(await r.recover('nope')).ok, 'recover: an unknown run is refused');

  const rec = await r.recover(crashedId, { note: 'Keep the UI minimal' });
  ok(rec.ok, 'recover: succeeds' + (rec.ok ? '' : ': ' + rec.error));
  const mgr = team.of('manager')[0];
  ok(mgr && mgr.id === 'c1', 'recover: the Manager is started again on its old chat id');
  const overview = mgr.msgs.find(m => m.startsWith('[Factory] This run is being recovered'));
  ok(overview && /Crundi restarted/.test(overview) && /Keep the UI minimal/.test(overview) && /Frontend dev: was working/.test(overview), 'recover: the Manager gets an overview with the reason, where everyone was, and the note');
  await until(() => team.of('fe')[0] && team.of('be')[0]);
  const fe = team.of('fe')[0];
  ok(fe && /You were interrupted: Crundi restarted/.test(fe.msgs[0]) && /Keep the UI minimal/.test(fe.msgs[0]) && /— The step, as it was given —/.test(fe.msgs[0]), 'recover: an interrupted employee gets the continue prompt, with the note, then its step');
  ok(workIn(team.of('be')[0].msgs[0]) !== workIn(fe.msgs[0]), 'recover: the unfinished branch workspace was created, so the branches stay apart');
  const fin = await until(() => r.getRun(crashedId).status === 'finishing' && r.getRun(crashedId), 30_000);
  ok(!!fin, 'recover: the run goes on to finish');
  if (fin) ok(existsSync(join(fin.worktree, 'ui.txt')) && existsSync(join(fin.worktree, 'api.txt')), 'recover: both branches\' work ends up merged');
  ok(!(await r.recover(crashedId)).ok, 'recover: a run that is not crashed is refused');
  r.dispose();
}

// ─── Pause ───
{
  let release;
  const gate = new Promise(res => { release = res; });
  const team = fakeTeam(brains({
    manager: async (msg) => msg.includes('answers.dPlan') ? { payload: { summary: 'ui', answers: { dPlan: 'no', dWho: ['ui'] } } } : null,
    fe: async () => { await gate; return { payload: { summary: 'UI done' } }; },
  }));
  const r = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: repo('pause') }), getFactory: () => ({ factory: standard() }), settleMs: 30 });
  team.setRunner(r);
  const st = await r.startRun({ project: 'pause', factoryId: 'x', task: 'Pause me' });
  await until(() => team.of('fe')[0]);
  const p = await r.pause(st.run.id);
  ok(p.ok && p.run.status === 'paused' && p.run.waitingOn === 'paused', 'pause: the run is paused');
  const mgr = team.of('manager')[0];
  const before = mgr.msgs.length;
  release();
  await until(() => team.of('fe')[0].results.length);
  await sleep(80);
  const run = r.getRun(st.run.id);
  ok(team.of('fe')[0].results[0].ok && run.status === 'paused', 'pause: the employee\'s handoff is still accepted and routed');
  ok(mgr.msgs.length === before && run.tokens[0].employeeId === 'manager', 'pause: the next step (the Manager) is held, not started');
  const res = await r.resume(st.run.id);
  ok(res.ok && await until(() => mgr.msgs.length > before), 'resume: the held step starts');
  await r.stopRun(st.run.id);
  r.dispose();
}

// ─── Usage limit ───
{
  const team = fakeTeam(brains({
    manager: async (msg) => msg.includes('answers.dPlan') ? { payload: { summary: 'ui', answers: { dPlan: 'no', dWho: ['ui'] } } } : null,
    fe: async (msg) => /usage limit/.test(msg) ? { payload: { summary: 'done after reset' } } : { idle: true },
  }));
  let usage = { fiveHour: { utilization: 40, resetsAt: null } };
  const r = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: repo('usage') }), getFactory: () => ({ factory: standard() }), settleMs: 30 });
  team.setRunner(r);
  const w = r.watchUsage(async () => usage, { tickMs: 1e9 });
  const st = await r.startRun({ project: 'usage', factoryId: 'x', task: 'Hit the limit' });
  await until(() => team.of('fe')[0]);
  usage = { fiveHour: { utilization: 100, resetsAt: new Date(Date.now() + 60_000).toISOString() } };
  await w.tick();
  let run = r.getRun(st.run.id);
  ok(run.status === 'paused' && run.waitingOn === 'usage' && run.paused.resetsAt, 'usage: a full window pauses the run, with the reset time');
  await sleep(150);   // the employee's turn "stopped" (idle) while paused
  ok(!team.of('fe')[0].msgs.some(m => /without calling factory_handoff/.test(m)), 'usage: no nudge while paused for the limit');
  usage = { fiveHour: { utilization: 3, resetsAt: null } };
  await w.tick();
  ok(r.getRun(st.run.id).status === 'paused', 'usage: it waits until the reset time recorded at the pause has passed');
  w.stop(); await r.stopRun(st.run.id); r.dispose();
}
{
  // Live check of the auto-resume: a fresh runner, paused-by-usage set in memory.
  const team = fakeTeam(brains({
    manager: async (msg) => msg.includes('answers.dPlan') ? { payload: { summary: 'ui', answers: { dPlan: 'no', dWho: ['ui'] } } } : null,
    fe: async (msg) => /usage limit/.test(msg) ? { payload: { summary: 'done after reset' } } : { idle: true },
  }));
  let usage = { fiveHour: { utilization: 100, resetsAt: new Date(Date.now() + 200).toISOString() } };
  const r = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: repo('usage2') }), getFactory: () => ({ factory: standard() }), settleMs: 30 });
  team.setRunner(r);
  const w = r.watchUsage(async () => usage, { tickMs: 1e9 });
  const st = await r.startRun({ project: 'usage2', factoryId: 'x', task: 'Hit the limit' });
  await until(() => team.of('fe')[0]);
  await w.tick();
  ok(r.getRun(st.run.id).status === 'paused', 'usage: paused');
  await sleep(300);
  usage = { fiveHour: { utilization: 1, resetsAt: new Date(Date.now() + 5 * 3600e3).toISOString() } };
  await w.tick();
  const fe = team.of('fe')[0];
  ok(r.getRun(st.run.id).status !== 'paused', 'usage: after the reset the run carries on by itself');
  ok(await until(() => fe.msgs.some(m => /You were interrupted: the usage limit was reached; it has reset now/.test(m))), 'usage: the employee whose turn was cut short gets the continue prompt');
  ok(await until(() => fe.results.some(x => x.ok)), 'usage: and finishes its step');
  w.stop(); r.dispose();
}

// ─── Collector ───
{
  let planned = 0;
  const team = fakeTeam({
    manager: async (msg) => {
      if (msg.includes('answers.dSec')) return { payload: { summary: 'ok', answers: { dSec: 'no', dRev: [] } } };
      if (msg.includes('answers.dOk')) return { payload: { summary: 'again', answers: { dOk: 'yes' } } };
      if (msg.includes('answers.dPlan')) {
        planned++;
        if (planned === 1) return { payload: { summary: 'plan first', answers: { dPlan: 'yes' } } };
        if (planned === 2) return { payload: { summary: 'build', answers: { dPlan: 'no', dWho: ['ui'] } } };
        if (planned === 3) return { payload: { summary: 'replan', answers: { dPlan: 'yes' } } };
        return { payload: { summary: 'done', finish: true } };
      }
      return null;
    },
    planner: async () => ({ payload: { summary: 'plan ' + planned } }),
    fe: async (msg) => { writeFileSync(join(workIn(msg), 'ui.txt'), 'ui ' + Date.now()); return { payload: { summary: 'UI done' } }; },
  });
  let feSeen = null;
  const r = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: repo('gc') }), getFactory: () => ({ factory: standard() }), settleMs: 10_000 });
  team.setRunner(r);
  const st = await r.startRun({ project: 'gc', factoryId: 'x', task: 'Collect' });
  // While the devs work, the Planner (reachable only through the Manager) is stopped.
  feSeen = await until(() => team.of('fe')[0]);
  const plannerChat = team.of('planner')[0];
  ok(plannerChat && await until(() => !plannerChat.alive), 'collector: the Planner is stopped once the work has moved past it');
  ok(r.getRun(st.run.id).employees.planner.retired, 'collector: and marked retired');
  ok(!!feSeen, 'collector: the frontend dev was started');
  const fin = await until(() => r.getRun(st.run.id).status === 'finishing');
  const again = team.of('planner')[0];
  ok(fin && again.spawns === 1 && team.of('planner').length === 2 && team.of('planner')[1].resumeId === 'uuid-' + plannerChat.id, 'collector: sent work again, the Planner is resumed on its old conversation');
  r.dispose();
}

try { rmSync(base, { recursive: true, force: true, maxRetries: 10, retryDelay: 300 }); } catch { /* Windows may still hold a file; it is a temp folder */ };
console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
