#!/usr/bin/env node
/**
 * test-factory-wait.mjs — Wait nodes.
 *
 * Covers: the model (kinds, outputs, a command required, reserved handles);
 * the engine holding a token at a Wait and routing it down Done or Timed out
 * with what happened, keeping what came in for the next step; the journal
 * replaying it; the runner arming a command wait that succeeds, a signal wait
 * ended by its secret URL key (and refused with a wrong one), a time wait the
 * person ends from the panel, deadlines journaled and re-armed after a
 * recovery, and the live card while waiting.
 *
 * Run: node scripts/test-factory-wait.mjs
 */
import { mkdtempSync, rmSync, writeFileSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';

const base = mkdtempSync(join(tmpdir(), 'factory-wait-'));
process.env.DATA_DIR = join(base, 'data');
process.env.FACTORY_WORKTREES_DIR = join(base, 'worktrees');
const M = await import('../src/factory-model.js');
const E = await import('../src/factory-engine.js');
const J = await import('../src/factory-journal.js');
const { createFactoryRunner, factoryLiveCard } = await import('../src/factory-runner.js');
const { fakeTeam } = await import('./factory-fixtures.mjs');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
async function until(fn, ms = 8000) { const end = Date.now() + ms; while (Date.now() < end) { const v = await fn(); if (v) return v; await sleep(20); } return null; }

function factory(wait) {
  return {
    name: 'With a wait',
    settings: { presets: [] },
    roster: [{ id: 'manager', name: 'Manager', persona: 'x' }, { id: 'dev', name: 'Developer', persona: 'x' }],
    nodes: [
      { id: 'start', type: 'start' }, { id: 'mgr', type: 'manager', label: 'Manager' }, { id: 'end', type: 'end' },
      { id: 'dev', type: 'employee', employeeId: 'dev', label: 'Developer' },
      { id: 'w', type: 'wait', label: 'CI green', wait },
    ],
    edges: [
      { id: 'e1', source: 'start', target: 'mgr' }, { id: 'e2', source: 'mgr', sourceHandle: 'finish', target: 'end' },
      { id: 'e3', source: 'mgr', target: 'dev' }, { id: 'e4', source: 'dev', target: 'w' },
      ...(wait.kind === 'command' || wait.kind === 'signal'
        ? [{ id: 'e5', source: 'w', sourceHandle: 'done', target: 'mgr', maxRounds: 3 }, { id: 'e6', source: 'w', sourceHandle: 'timeout', target: 'mgr', maxRounds: 3 }]
        : [{ id: 'e5', source: 'w', sourceHandle: 'out', target: 'mgr', maxRounds: 3 }]),
    ],
  };
}

// ─── Model ───
{
  const n = M.normalizeFactory(factory({ kind: 'command', command: 'npm test', every: 1, timeout: 99999 })).nodes.find(x => x.id === 'w');
  ok(n.wait.kind === 'command' && n.wait.every === 15 && n.wait.timeout === 10080, 'model: a wait is normalised and clamped');
  ok(M.outputsOf(n).join() === 'done,timeout' && M.outputsOf({ type: 'wait', wait: { kind: 'time' } }).join() === 'out', 'model: command and signal waits have Done and Timed out; time waits one route');
  ok(M.validateFactory(factory({ kind: 'command', command: 'npm test' })).ok, 'validate: a command wait with both routes is fine');
  const v = M.validateFactory(factory({ kind: 'command', command: '' }));
  ok(!v.ok && v.errors.some(e => e.code === 'wait-no-command'), 'validate: a command wait needs a command');
  ok(M.validateFactory(factory({ kind: 'time', minutes: 5 })).ok && M.validateFactory(factory({ kind: 'until', at: '07:30' })).ok, 'validate: time and until waits');
  const bad = factory({ kind: 'signal' }); bad.edges = bad.edges.filter(e => e.id !== 'e6');
  ok(!M.validateFactory(bad).ok, 'validate: a missing Timed out route is an error (every output must lead somewhere)');
}

// ─── Engine ───
{
  let r = E.start(E.createRun({ id: 'r', project: 'p', factory: factory({ kind: 'command', command: 'true' }), task: 't' })).run;
  r = E.handoff(r, E.tokensFor(r, 'manager')[0].id, { summary: 'go' }).run;
  const res = E.handoff(r, E.tokensFor(r, 'dev')[0].id, { summary: 'pushed the fix' });
  const w = res.effects.find(e => e.type === 'wait');
  ok(res.ok && w && w.wait.kind === 'command' && res.run.tokens[0].status === 'wait' && res.run.status === 'running', 'engine: the token waits at the Wait; the run is still running');
  ok(!E.waitDone(res.run, 'nope', 'done').ok, 'engine: an unknown wait is refused');
  const done = E.waitDone(res.run, res.run.tokens[0].id, 'done', 'checks passed');
  const a = done.effects.find(e => e.type === 'activate');
  ok(done.ok && a.employeeId === 'manager' && /CI green: the wait is over \(checks passed\)/.test(a.context.message) && a.context.summary === 'pushed the fix' && a.context.from.name === 'Developer', 'engine: Done routes on, with what happened and the developer\'s summary');
  const to = E.waitDone(res.run, res.run.tokens[0].id, 'timeout', 'still red');
  ok(to.ok && /gave up waiting \(still red\)/.test(to.effects.find(e => e.type === 'activate').context.message) && to.run.history.at(-1).outcome === 'timeout', 'engine: Timed out takes the other route');
  // Journal replay.
  const evs = [
    { type: 'start', id: 'r2', project: 'p', factory: factory({ kind: 'signal' }), task: 't', wt: { repo: '/r', root: '/x', path: '/x/main', branch: 'b', parentBranch: 'dev', baseCommit: 'c' } },
    { type: 'begin' },
  ];
  let rec = J.replay(evs);
  rec = J.reduce(rec, { type: 'handoff', tokenId: rec.state.tokens[0].id, payload: { summary: 'go' }, by: 'manager' });
  rec = J.reduce(rec, { type: 'handoff', tokenId: rec.state.tokens[0].id, payload: { summary: 'deployed' }, by: 'dev' });
  const wef = rec.run.pending.find(e => e.type === 'wait');
  rec = J.reduce(rec, { type: 'effect-done', effectId: wef.id, result: { kind: 'signal', nodeId: 'w', ws: 'main', startedAt: 'x', deadline: '', timeoutAt: 'y', key: 'k' } });
  ok(rec.run.waits[wef.tokenId]?.key === 'k', 'journal: the wait\'s plan is kept');
  rec = J.reduce(rec, { type: 'wait-done', tokenId: wef.tokenId, outcome: 'done', detail: 'signal' });
  ok(!rec.run.waits[wef.tokenId] && rec.state.tokens[0].status === 'work', 'journal: wait-done replays the routing and forgets the plan');
}

// ─── Runner ───
function repo(name) {
  const dir = join(base, 'projects', name); mkdirSync(dir, { recursive: true });
  const g = (a) => execFileSync('git', ['-c', 'user.name=t', '-c', 'user.email=t@t', ...a], { cwd: dir });
  g(['init', '-q', '-b', 'dev']); writeFileSync(join(dir, 'a.txt'), 'a'); g(['add', '.']); g(['commit', '-qm', 'i']);
  return dir;
}
function setup(name, f) {
  const seen = [];
  const team = fakeTeam({
    manager: async (msg) => {
      seen.push(msg);
      if (/the wait is over|gave up waiting/.test(msg)) return { payload: { summary: 'done', finish: true } };
      if (/Your step: Manager/.test(msg)) return { payload: { summary: 'go' } };
      return null;
    },
    dev: async () => ({ payload: { summary: 'pushed' } }),
  });
  const opts = { dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: repo(name) }), getFactory: () => ({ factory: f }), settleMs: 10_000, permissionBatchMs: 30 };
  const r = createFactoryRunner(opts);
  team.setRunner(r);
  return { team, r, seen, opts };
}

{
  const { r, seen } = setup('w1', factory({ kind: 'command', command: 'test -f a.txt && echo all green', every: 15, timeout: 5 }));
  const st = await r.startRun({ project: 'w1', factoryId: 'x', task: 'go' });
  await until(() => r.getRun(st.run.id).status === 'finishing');
  ok(r.getRun(st.run.id).status === 'finishing' && seen.some(m => /CI green: the wait is over \(the command succeeded after 1 try; it said: all green\)/.test(m)), 'command wait: run in the worktree, it succeeded, and the Manager was told what it said');
  r.dispose();
}
{
  const { r, team, seen } = setup('w2', factory({ kind: 'signal', timeout: 30, what: 'the deploy finishes' }));
  const st = await r.startRun({ project: 'w2', factoryId: 'x', task: 'go' });
  const tok = await until(() => r.getRun(st.run.id).tokens.find(t => t.status === 'wait'));
  ok(tok && tok.wait.kind === 'signal' && /^\/api\/factory-hook\/wait\/run_\w+\/t\d+\?key=[\w-]{20,}$/.test(tok.wait.signalPath) && tok.wait.what === 'the deploy finishes', 'signal wait: a secret URL is shown, with what it waits for');
  ok(Date.parse(tok.wait.timeoutAt) - Date.now() > 29 * 60_000, 'signal wait: its time limit is journaled');
  ok(factoryLiveCard(r.getRun(st.run.id)).state === 'waiting' && /waiting: CI green/.test(factoryLiveCard(r.getRun(st.run.id)).title), 'live card: waiting, saying for what');
  const wrong = await r.endWait(st.run.id, tok.id, { key: 'nope' });
  ok(!wrong.ok && wrong.forbidden, 'signal wait: a wrong key is refused');
  const key = tok.wait.signalPath.split('key=')[1];
  const right = await r.endWait(st.run.id, tok.id, { key, note: 'deploy 42 is live' });
  ok(right.ok, 'signal wait: the right key ends it');
  await until(() => r.getRun(st.run.id).status === 'finishing');
  ok(seen.some(m => /a signal came: deploy 42 is live/.test(m)), 'signal wait: the note reaches the next step');
  ok(!(await r.endWait(st.run.id, tok.id, { key })).ok, 'signal wait: a second signal finds nothing waiting');
  r.dispose();
}
{
  const { r, team, opts } = setup('w3', factory({ kind: 'time', minutes: 120 }));
  const st = await r.startRun({ project: 'w3', factoryId: 'x', task: 'go' });
  const tok = await until(() => r.getRun(st.run.id).tokens.find(t => t.status === 'wait'));
  const left = Date.parse(tok.wait.deadline) - Date.now();
  ok(left > 119 * 60_000 && left <= 120 * 60_000, 'time wait: the deadline is two hours out');
  // A restart: the run comes back crashed; recovering re-arms the wait from the same deadline.
  r.dispose();
  const r2 = createFactoryRunner(opts); team.setRunner(r2); r2.load();
  ok(r2.getRun(st.run.id).status === 'crashed', 'restart: the run is crashed, as every run after a restart');
  const rec = await r2.recover(st.run.id);
  const tok2 = r2.getRun(st.run.id).tokens.find(t => t.status === 'wait');
  ok(rec.ok && tok2 && tok2.wait.deadline === tok.wait.deadline, 'recover: the wait is back, with the same deadline');
  const byMe = await r2.endWait(st.run.id, tok2.id, {});
  await until(() => r2.getRun(st.run.id).status === 'finishing');
  ok(byMe.ok && r2.getRun(st.run.id).status === 'finishing', 'time wait: the person can end it early from the panel');
  r2.dispose();
}

rmSync(base, { recursive: true, force: true });
console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
