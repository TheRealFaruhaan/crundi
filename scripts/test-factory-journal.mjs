#!/usr/bin/env node
/**
 * test-factory-journal.mjs — runs as a fold over their journal.
 *
 * Covers: replaying the journal gives exactly the cached run; a missing,
 * damaged, or out-of-date cache (the process died between the journal write
 * and the cache write) is rebuilt by replay; a torn last line is ignored;
 * branch worktrees made twice for the same id are one worktree, including a
 * crash right after git made one and before it was recorded; the time limit
 * counts active time only (not the time the process was dead or the run
 * paused) and fires after a restart; and auto-recover, with retries, and
 * giving up to the person when it keeps failing.
 *
 * Run: node scripts/test-factory-journal.mjs
 */

import { mkdtempSync, rmSync, writeFileSync, mkdirSync, readFileSync, existsSync, readdirSync, appendFileSync, unlinkSync, renameSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';

const base = mkdtempSync(join(tmpdir(), 'factory-journal-'));
process.env.DATA_DIR = join(base, 'data');
process.env.FACTORY_WORKTREES_DIR = join(base, 'worktrees');
const J = await import('../src/factory-journal.js');
const Git = await import('../src/factory-git.js');
const M = await import('../src/factory-model.js');
const { createFactoryRunner } = await import('../src/factory-runner.js');
const { standard, fakeTeam } = await import('./factory-fixtures.mjs');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
async function until(fn, ms = 20000) { const end = Date.now() + ms; while (Date.now() < end) { const v = await fn(); if (v) return v; await sleep(15); } return null; }
const RUNS = join(process.env.DATA_DIR, 'factory-runs');
const events = (id) => readFileSync(join(RUNS, id, 'events.jsonl'), 'utf-8').trim().split('\n').map(l => JSON.parse(l));
const cache = (id) => JSON.parse(readFileSync(join(RUNS, id, 'run.json'), 'utf-8'));
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
function repo(name) {
  const dir = join(base, 'projects', name);
  mkdirSync(dir, { recursive: true });
  const g = (a) => execFileSync('git', a, { cwd: dir });
  g(['init', '-q', '-b', 'dev']); writeFileSync(join(dir, 'a.txt'), 'a\n');
  g(['-c', 'user.name=t', '-c', 'user.email=t@t', 'add', '.']); g(['-c', 'user.name=t', '-c', 'user.email=t@t', 'commit', '-q', '-m', 'i']);
  return dir;
}
const workIn = (msg) => (msg.match(/^Work in: (.+)$/m) || [])[1];
function brains() {
  let n = 0;
  return {
    manager: async (msg) => {
      if (msg.startsWith('[Factory] This run is being recovered')) return null;
      if (msg.includes('answers.dSec')) return { payload: { summary: 'ok', answers: { dSec: 'no', dRev: ['ui'] } } };
      if (msg.includes('answers.dOk')) return { payload: { summary: 'approved', answers: { dOk: 'yes' } } };
      if (msg.includes('answers.dPlan')) return ++n === 1 ? { payload: { summary: 'build', answers: { dPlan: 'no', dWho: ['ui', 'backend'] } } } : { payload: { summary: 'done', finish: true } };
      return null;
    },
    fe: async (msg) => { writeFileSync(join(workIn(msg), 'ui.txt'), 'ui\n'); return { payload: { summary: 'UI done' } }; },
    be: async (msg) => { writeFileSync(join(workIn(msg), 'api.txt'), 'api\n'); return { payload: { summary: 'API done' } }; },
    uir: async () => ({ payload: { summary: 'fine' } }),
  };
}

// ─── A whole run: replay == cache ───
let runId = '';
{
  const team = fakeTeam(brains());
  const r = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: repo('full') }), getFactory: () => ({ factory: standard() }), settleMs: 10_000 });
  team.setRunner(r);
  const st = await r.startRun({ project: 'full', factoryId: 'x', task: 'Journal me' });
  runId = st.run.id;
  await until(() => r.getRun(runId).status === 'finishing');
  r.dispose();
  const evs = events(runId);
  ok(evs[0].type === 'start' && evs.some(e => e.type === 'handoff') && evs.some(e => e.type === 'effect-done' && e.result?.path), 'journal: start, handoffs and effect results are all in it');
  ok(same(J.replay(evs), cache(runId).rec), 'replay: folding the journal gives exactly the cached run');
  ok(cache(runId).rec.events === evs.length, 'cache: records how many events it reflects');
}

// ─── Rebuilding from the journal ───
function freshRunner() {
  const team = fakeTeam({});
  const r = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => null, getFactory: () => null, settleMs: 10_000 });
  r.load();
  return r;
}
{
  const good = cache(runId).rec;
  unlinkSync(join(RUNS, runId, 'run.json'));
  let r = freshRunner();
  ok(r.getRun(runId)?.outcome?.summary === 'done' && existsSync(join(RUNS, runId, 'run.json')), 'missing cache: rebuilt from the journal, and written back');
  r.dispose();

  writeFileSync(join(RUNS, runId, 'run.json'), '{ not json');
  r = freshRunner();
  ok(r.getRun(runId)?.outcome?.summary === 'done', 'damaged cache: rebuilt from the journal');
  r.dispose();

  // The process died after writing an event but before the cache caught up.
  const evs = events(runId);
  const behind = J.replay(evs.slice(0, -3));
  writeFileSync(join(RUNS, runId, 'run.json'), JSON.stringify({ rec: behind }));
  r = freshRunner();
  const rebuilt = cache(runId).rec;
  ok(rebuilt.events === evs.length && rebuilt.state.handoffs === good.state.handoffs && same(rebuilt, J.replay(evs)), 'stale cache: the missing events are replayed');
  ok(r.getRun(runId).status === 'crashed' && r.getRun(runId).crash.from === 'finishing', 'stale cache: a finishing run comes back crashed, knowing it was finishing');
  r.dispose();

  // A torn last line from a crash mid-append is ignored.
  appendFileSync(join(RUNS, runId, 'events.jsonl'), '{"type":"handoff","tokenId":"t9","pay');
  unlinkSync(join(RUNS, runId, 'run.json'));
  r = freshRunner();
  ok(r.getRun(runId)?.status === 'crashed', 'torn line: the journal up to it is used, the rest ignored');
  r.dispose();
}

// ─── Branch worktrees are safe to make twice ───
{
  const dir = repo('idem');
  const wt = await Git.createRunWorktree({ repo: dir, project: 'idem', runId: 'run_i1', name: 'x' });
  const a = await Git.createBranchWorktree({ repo: dir, parentPath: wt.path, root: wt.root, name: 'UI', branchPrefix: wt.branch, key: 'w5' });
  const b = await Git.createBranchWorktree({ repo: dir, parentPath: wt.path, root: wt.root, name: 'UI', branchPrefix: wt.branch, key: 'w5' });
  ok(a.ok && b.ok && b.reused && a.path === b.path && a.branch === b.branch, 'workspace: made twice for one id, it is the same worktree');
  ok(readdirSync(wt.root).length === 2, 'workspace: no second folder');
  execFileSync('git', ['worktree', 'remove', '--force', a.path], { cwd: dir });
  const c = await Git.createBranchWorktree({ repo: dir, parentPath: wt.path, root: wt.root, name: 'UI', branchPrefix: wt.branch, key: 'w5' });
  ok(c.ok && c.reused && existsSync(c.path), 'workspace: folder gone but branch kept: the branch is checked out again, its work intact');
}
{
  // Crash right after git made the worktree, before effect-done was written.
  let calls = 0;
  const flakyGit = { ...Git, createBranchWorktree: async (o) => { const res = await Git.createBranchWorktree(o); if (++calls === 1) return new Promise(() => {}); return res; } };
  const team = fakeTeam({ ...brains(), fe: async () => null, be: async () => null });
  const dir = repo('crashws');
  const r = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: dir }), getFactory: () => ({ factory: standard() }), git: flakyGit, settleMs: 10_000 });
  team.setRunner(r);
  const st = await r.startRun({ project: 'crashws', factoryId: 'x', task: 'Crash after git' });
  await until(() => calls === 1);
  await sleep(50);
  r.dispose();
  const root = join(cache(st.run.id).rec.run.root);
  ok(readdirSync(root).length === 2, 'crash after git: the worktree exists on disk but was never recorded');
  const team2 = fakeTeam({ ...brains(), fe: async () => null, be: async () => null });
  const r2 = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team2.agents, getProject: () => ({ path: dir }), getFactory: () => ({ factory: standard() }), settleMs: 10_000 });
  team2.setRunner(r2);
  r2.load();
  const rec = await r2.recover(st.run.id);
  await until(() => team2.of('fe')[0] && team2.of('be')[0]);
  ok(rec.ok && readdirSync(root).length === 3, 'crash after git: recovery reuses that worktree and makes only the missing one');
  ok(workIn(team2.of('fe')[0].msgs[0]) !== workIn(team2.of('be')[0].msgs[0]), 'crash after git: the two branches still work apart');
  await r2.stopRun(st.run.id);
  r2.dispose();
}

// ─── The time limit counts active time ───
{
  const f = M.normalizeFactory({ ...standard(), settings: { caps: { maxMinutes: 60 } } });
  const T = (min) => new Date(Date.UTC(2026, 0, 1, 0, min)).toISOString();
  const wt = { repo: '/r', root: '/w', path: '/w/main', branch: 'factory/x', parentBranch: 'dev', baseCommit: 'abc' };
  let rec = J.replay([
    { type: 'start', id: 'r1', project: 'p', factory: f, task: 't', wt, at: T(0) },
    { type: 'begin', at: T(0) },
    { type: 'pause', by: 'person', at: T(10) },
    { type: 'resume', at: T(30) },
    { type: 'crash', reason: 'restart', diedAt: T(35), at: T(90) },
    { type: 'recover', at: T(100) },
  ]);
  ok(rec.run.time.activeMs === 15 * 60_000, 'clock: active time leaves out the pause and the time the process was dead');
  ok(J.timeLeft(rec, Date.parse(T(110))) === 35 * 60_000, 'clock: what is left counts on from the recovery');
}
{
  // A run started two minutes ago with a one-minute limit: after a restart and
  // recovery the limit is still spent, so it holds at once.
  const ago = Date.now() - 2 * 60_000;
  const team = fakeTeam({ manager: async () => null });
  const f = standard(); f.settings = { caps: { maxMinutes: 1 } };
  const dir = repo('clock');
  const r = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: dir }), getFactory: () => ({ factory: f }), settleMs: 10_000, now: () => new Date(ago).toISOString() });
  team.setRunner(r);
  const st = await r.startRun({ project: 'clock', factoryId: 'x', task: 'Over time' });
  const held = await until(() => r.getRun(st.run.id).capHit);
  ok(held && held.which === 'maxMinutes' && r.getRun(st.run.id).waitingOn === 'cap', 'clock: an overdue limit holds the run');
  r.dispose();
}

// ─── Auto-recover ───
{
  const f = standard(); f.settings = { autoRecover: true };
  const dir = repo('auto');
  const team = fakeTeam({ ...brains(), fe: async () => null, be: async () => null });
  const r = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: dir }), getFactory: () => ({ factory: f }), settleMs: 10_000 });
  team.setRunner(r);
  const st = await r.startRun({ project: 'auto', factoryId: 'x', task: 'Auto' });
  await until(() => team.of('fe')[0]);
  r.dispose();
  const notes = [];
  const team2 = fakeTeam({ ...brains(), fe: async () => null, be: async () => null });
  const r2 = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team2.agents, getProject: () => ({ path: dir }), getFactory: () => ({ factory: f }), settleMs: 10_000, autoRecoverDelays: [20, 20, 20], notify: (k) => notes.push(k) });
  team2.setRunner(r2);
  r2.load();
  ok(await until(() => r2.getRun(st.run.id).status === 'running' && notes.includes('recovered')), 'auto-recover: a crashed run set to recover by itself does, without anyone pressing Recover');
  ok(await until(() => team2.of('fe')[0]?.msgs[0]?.includes('You were interrupted')), 'auto-recover: employees get the continue prompt');
  await r2.stopRun(st.run.id);
  r2.dispose();
}
{
  // When it keeps failing it gives up and asks the person.
  const f = standard(); f.settings = { autoRecover: true };
  const dir = repo('autofail');
  const team = fakeTeam({ manager: async () => null });
  const r = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: dir }), getFactory: () => ({ factory: f }), settleMs: 10_000 });
  team.setRunner(r);
  const st = await r.startRun({ project: 'autofail', factoryId: 'x', task: 'Fail' });
  r.dispose();
  rmSync(cache(st.run.id).rec.run.worktree, { recursive: true, force: true });
  const notes = [];
  const r2 = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: fakeTeam({}).agents, getProject: () => ({ path: dir }), getFactory: () => ({ factory: f }), settleMs: 10_000, autoRecoverDelays: [10, 10, 10], notify: (k, run, text) => notes.push(text) });
  r2.load();
  ok(await until(() => notes.some(t => /could not recover by itself/.test(t))), 'auto-recover: after its retries fail, the person is told to press Recover');
  ok(r2.getRun(st.run.id).status === 'crashed' && /worktree is gone/.test(r2.getRun(st.run.id).error), 'auto-recover: the run stays crashed, saying why');
  r2.dispose();
}

try { rmSync(base, { recursive: true, force: true, maxRetries: 10, retryDelay: 300 }); } catch { /* Windows may still hold a file; it is a temp folder */ };
console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
