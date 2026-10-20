#!/usr/bin/env node
/**
 * test-factory-runner.mjs — src/factory-runner.js against a real git repo and
 * a scripted fake team (no Claude is spawned).
 *
 * Each fake employee reads the step message it is sent, writes files in the
 * folder the message names, and calls the runner's handoff exactly as the
 * factory_handoff tool would. Covers: refusals at start, a full Standard run
 * with parallel branches merged into the run worktree, one run per project,
 * a merge conflict escalating to the Manager, parallel handoffs arriving at
 * once, a stalled employee nudged then escalated, the handoff cap holding new
 * steps until resume, stop, and a restart bringing an unfinished run back as
 * crashed.
 *
 * Run: node scripts/test-factory-runner.mjs
 */

import { mkdtempSync, rmSync, writeFileSync, readFileSync, existsSync, readdirSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';

const base = mkdtempSync(join(tmpdir(), 'factory-runner-'));
process.env.DATA_DIR = join(base, 'data');
process.env.FACTORY_WORKTREES_DIR = join(base, 'worktrees');
const { createFactoryRunner } = await import('../src/factory-runner.js');
const { standard, fakeTeam } = await import('./factory-fixtures.mjs');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
async function until(fn, ms = 8000) {
  const end = Date.now() + ms;
  while (Date.now() < end) { const v = await fn(); if (v) return v; await sleep(20); }
  return null;
}
const sh = (args, cwd) => execFileSync('git', args, { cwd, encoding: 'utf-8' }).trim();

function makeRepo(name) {
  const dir = join(base, 'projects', name);
  mkdirSync(dir, { recursive: true });
  sh(['init', '-q', '-b', 'dev'], dir);
  writeFileSync(join(dir, 'README.md'), '# app\n');
  sh(['-c', 'user.name=t', '-c', 'user.email=t@t', 'add', '.'], dir);
  sh(['-c', 'user.name=t', '-c', 'user.email=t@t', 'commit', '-q', '-m', 'init'], dir);
  return dir;
}

const workIn = (msg) => (msg.match(/^Work in: (.+)$/m) || [])[1];
const write = (msg, file, body) => writeFileSync(join(workIn(msg), file), body);

function makeRunner(team, projects, factories, extra = {}) {
  const r = createFactoryRunner({
    dataDir: process.env.DATA_DIR,
    agents: team.agents,
    getProject: (a) => projects[a] ? { path: projects[a] } : null,
    getFactory: (id) => factories[id] ? { factory: factories[id], validation: { ok: true } } : null,
    settleMs: 40,
    ...extra,
  });
  team.setRunner(r);
  return r;
}

// ─── Brains for the Standard factory ───

function standardBrains({ conflict = false, simultaneous = false } = {}) {
  let mgrSteps = 0;
  let gate = null;
  const arrive = () => {
    if (!simultaneous) return Promise.resolve();
    if (!gate) { let open; const p = new Promise(r => { open = r; }); gate = { p, n: 0, open }; }
    gate.n++;
    if (gate.n === 2) gate.open();
    return gate.p;
  };
  return {
    manager: async (msg) => {
      if (msg.includes('answers.dSec')) return { payload: { summary: 'No security work', answers: { dSec: 'no', dRev: ['ui'] } } };
      if (msg.includes('answers.dOk')) return { payload: { summary: 'Approved', answers: { dOk: 'yes' } } };
      if (msg.includes('answers.dPlan')) {
        mgrSteps++;
        if (mgrSteps === 1) return { payload: { summary: 'UI and API work', answers: { dPlan: 'no', dWho: ['ui', 'backend'] }, notes: { uir: 'Check the toggle' } } };
        return { payload: { summary: 'Dark mode is in', finish: true } };
      }
      return null;
    },
    fe: async (msg) => { write(msg, conflict ? 'shared.txt' : 'ui.txt', 'ui side\n'); await arrive(); return { payload: { summary: 'UI done' } }; },
    be: async (msg) => { write(msg, conflict ? 'shared.txt' : 'api.txt', 'api side\n'); await arrive(); return { payload: { summary: 'API done' } }; },
    uir: async () => ({ payload: { summary: 'Looks right' } }),
  };
}

// ─── Refusals ───
{
  const team = fakeTeam({});
  const plain = join(base, 'projects', 'nogit'); mkdirSync(plain, { recursive: true });
  const r = makeRunner(team, { nogit: plain, app0: makeRepo('app0') }, { std: standard() });
  const a = await r.startRun({ project: 'nogit', factoryId: 'std', task: 'x' });
  ok(!a.ok && /not a git repository/.test(a.error), 'start: a project without git is refused');
  const b = await r.startRun({ project: 'app0', factoryId: 'std', task: '  ' });
  ok(!b.ok && /task/.test(b.error), 'start: an empty task is refused');
  const bad = standard(); bad.edges = bad.edges.filter(e => e.id !== 'eFin');
  const r2 = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: join(base, 'projects', 'app0') }), getFactory: () => ({ factory: bad }) });
  const c = await r2.startRun({ project: 'app0', factoryId: 'x', task: 'x' });
  ok(!c.ok && c.invalid && c.errors.length > 0, 'start: an invalid factory is refused with its problems');
  ok(r.listRuns().length === 0, 'start: refused runs leave nothing behind');
  r.dispose(); r2.dispose();
}

// ─── A full Standard run ───
let fullRunId = '';
{
  const repo = makeRepo('app1');
  const team = fakeTeam(standardBrains());
  const r = makeRunner(team, { app1: repo }, { std: standard() });
  const st = await r.startRun({ project: 'app1', factoryId: 'std', task: 'Add dark mode' });
  ok(st.ok && st.run.branch.startsWith('factory/') && st.run.parentBranch === 'dev', 'run: starts on a factory/ branch cut from dev');
  fullRunId = st.run?.id;
  ok(existsSync(st.run.worktree) && !st.run.worktree.startsWith(repo), 'run: the worktree is outside the project');

  const second = await r.startRun({ project: 'app1', factoryId: 'std', task: 'Another' });
  ok(!second.ok && second.busy, 'run: a second run in the same project is refused');

  const fin = await until(() => r.getRun(fullRunId)?.status === 'finishing' && r.getRun(fullRunId));
  ok(!!fin, 'run: reaches the end (Manager finished)' + (fin ? '' : ` — stuck at ${JSON.stringify(r.getRun(fullRunId)?.tokens)} ${r.getRun(fullRunId)?.error}`));
  if (fin) {
    ok(fin.outcome.summary === 'Dark mode is in', 'run: the Manager\'s summary is the outcome');
    const main = fin.worktree;
    ok(existsSync(join(main, 'ui.txt')) && existsSync(join(main, 'api.txt')), 'join: both branches\' work is merged into the run worktree');
    ok(sh(['status', '--porcelain'], main) === '', 'commits: every step is committed');
    const log = sh(['log', '--oneline', `${fin.parentBranch}..HEAD`], main);
    ok(/factory: Frontend dev/.test(log) && /factory: Backend dev/.test(log), 'commits: each step is a commit named after it');
    const left = readdirSync(join(main, '..'));
    ok(left.length === 1 && left[0] === 'main', 'join: the branch worktrees are removed after merging');
    ok(!sh(['branch', '--list', 'factory/*--*'], repo), 'join: the branch branches are deleted');
    ok(sh(['status', '--porcelain'], repo) === '' && !existsSync(join(repo, 'ui.txt')), 'the project\'s own checkout is untouched');
    const uir = team.of('uir')[0];
    ok(uir && /Check the toggle/.test(uir.msgs[0]), 'notes: the UI reviewer got the Manager\'s note');
    ok(team.of('ber').length === 0, 'single pick: the backend reviewer was never started');
    ok(['fe', 'be', 'uir'].every(e => team.of(e).every(s => !s.alive)), 'finish: the employees\' sessions are closed');
    ok(team.of('manager')[0].alive, 'finish: the Manager stays, to present the result');
    const events = readFileSync(join(process.env.DATA_DIR, 'factory-runs', fullRunId, 'events.jsonl'), 'utf-8').trim().split('\n').map(l => JSON.parse(l));
    ok(events.filter(e => e.type === 'handoff').length >= 6 && events[0].type === 'start', 'persist: every handoff is in the journal');
    const res = team.of('fe')[0].results[0];
    ok(res.ok && /waiting for the other branches|takes over|asked/.test(res.message), 'handoff: the employee is told where the work went');
  }
  ok(!r.activeRunFor('app1') || r.activeRunFor('app1').status === 'finishing', 'one run: a finishing run still holds the project until it is merged or stopped');
  r.dispose();
}

// ─── Parallel handoffs at the same instant ───
{
  const repo = makeRepo('app2');
  const team = fakeTeam(standardBrains({ simultaneous: true }));
  const r = makeRunner(team, { app2: repo }, { std: standard() });
  const st = await r.startRun({ project: 'app2', factoryId: 'std', task: 'Both at once' });
  const fin = await until(() => r.getRun(st.run.id)?.status === 'finishing');
  ok(!!fin, 'parallel: two branches handing off at the same instant both arrive and the run finishes');
  r.dispose();
}

// ─── Merge conflict ───
{
  const repo = makeRepo('app3');
  const brains = standardBrains({ conflict: true });
  let escalation = '';
  const mgr = brains.manager;
  brains.manager = async (msg) => {
    if (/Escalated to you:/.test(msg)) { escalation = msg; return null; }
    return mgr(msg);
  };
  const team = fakeTeam(brains);
  const r = makeRunner(team, { app3: repo }, { std: standard() });
  const st = await r.startRun({ project: 'app3', factoryId: 'std', task: 'Conflict' });
  await until(() => escalation);
  ok(/Merge conflict in shared\.txt/.test(escalation) && /branches are kept/.test(escalation), 'conflict: the Manager is told which file conflicts, and the branches are kept');
  const run = r.getRun(st.run.id);
  ok(run.tokens.length === 1 && run.tokens[0].employeeId === 'manager', 'conflict: control is with the Manager');
  ok(readdirSync(join(run.worktree, '..')).length >= 2, 'conflict: the conflicting branch worktree is kept to resolve');
  const stop = await r.stopRun(st.run.id);
  ok(stop.ok && r.getRun(st.run.id).status === 'stopped' && !r.activeRunFor('app3'), 'stop: the run stops and frees the project');
  ok(team.of('manager')[0].alive && ['fe', 'be'].every(e => team.of(e).every(s => !s.alive)), 'stop: employees are closed, the Manager\'s chat is left');
  r.dispose();
}

// ─── Stall: nudge, then escalate ───
{
  const repo = makeRepo('app4');
  let escalation = '';
  let feCalls = 0;
  const team = fakeTeam({
    manager: async (msg) => {
      if (/Escalated to you:/.test(msg)) { escalation = msg; return null; }
      if (msg.includes('answers.dPlan')) return { payload: { summary: 'UI only', answers: { dPlan: 'no', dWho: ['ui'] } } };
      return null;
    },
    fe: async () => { feCalls++; return { idle: true }; },   // never hands off
  });
  const r = makeRunner(team, { app4: repo }, { std: standard() });
  await r.startRun({ project: 'app4', factoryId: 'std', task: 'Stall' });
  await until(() => escalation, 5000);
  const fe = team.of('fe')[0];
  ok(fe && fe.msgs.some(m => /without calling factory_handoff/.test(m)), 'stall: the employee is nudged once');
  ok(/stopped twice without handing off/.test(escalation), 'stall: the second time, the Manager is told');
  r.dispose();
}

// ─── Cap holds new steps until resume ───
{
  const repo = makeRepo('app5');
  const f = standard(); f.settings = { caps: { maxHandoffs: 1 } };
  const team = fakeTeam(standardBrains());
  const r = makeRunner(team, { app5: repo }, { std: f });
  const st = await r.startRun({ project: 'app5', factoryId: 'std', task: 'Capped' });
  const held = await until(() => r.getRun(st.run.id)?.capHit && r.getRun(st.run.id));
  ok(held && held.capHit.which === 'maxHandoffs' && held.waitingOn === 'cap', 'cap: the run holds after maxHandoffs');
  ok(team.of('fe').length === 0, 'cap: nobody new is started while held');
  const res = await r.resume(st.run.id);
  ok(res.ok, 'cap: resume lets it carry on');
  const moved = await until(() => team.of('fe').length > 0);
  ok(!!moved, 'cap: the held steps start after resume');
  r.dispose();
}

// ─── Restart ───
{
  const team = fakeTeam({});
  const r = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => null, getFactory: () => null });
  r.load();
  const runs = r.listRuns();
  const capped = runs.find(x => x.project === 'app5');
  ok(capped && capped.status === 'crashed', 'restart: a run that was going comes back as crashed');
  ok(runs.find(x => x.id === fullRunId)?.status === 'crashed', 'restart: a finishing run whose Manager is gone comes back crashed too');
  ok(runs.find(x => x.project === 'app3')?.status === 'stopped', 'restart: a stopped run stays stopped');
  ok(r.activeRunFor('app5'), 'restart: a crashed run still holds its project until recovered or stopped');
  r.dispose();
}

rmSync(base, { recursive: true, force: true });
console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
