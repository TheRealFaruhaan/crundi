#!/usr/bin/env node
/**
 * test-factory-run-limits.mjs — raising a running run's loop limits and caps.
 *
 * Covers: the engine giving a loop a new limit in the run's own snapshot (not
 * below what it used, 0 for none, flow connections only) and clearing its
 * exhausted count; a run whose loop ran out and escalated to the Manager,
 * raised by the person: the Manager is told, routes through the loop again
 * and the handoff goes through; replaying the journal gives the cached run,
 * also from a cache written before the change; a run held at its handoff cap
 * carrying on once the cap is raised; a run held at its time cap carrying on
 * once the time is raised, with the clock re-armed so the new limit holds it
 * again; validation; the Manager's factory_control run-limits.
 *
 * Run: node scripts/test-factory-run-limits.mjs
 */
import { mkdtempSync, rmSync, writeFileSync, mkdirSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';

const base = mkdtempSync(join(tmpdir(), 'factory-run-limits-'));
process.env.DATA_DIR = join(base, 'data');
process.env.FACTORY_WORKTREES_DIR = join(base, 'worktrees');
const E = await import('../src/factory-engine.js');
const J = await import('../src/factory-journal.js');
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
const workIn = (msg) => (msg.match(/^Work in: (.+)$/m) || [])[1];
function repo(name) {
  const dir = join(base, 'projects', name); mkdirSync(dir, { recursive: true });
  const g = (a) => execFileSync('git', ['-c', 'user.name=t', '-c', 'user.email=t@t', ...a], { cwd: dir });
  g(['init', '-q', '-b', 'dev']); writeFileSync(join(dir, 'a.txt'), 'a'); g(['add', '.']); g(['commit', '-qm', 'i']);
  return dir;
}

// ─── Engine: a loop's limit, changed on the run ───
{
  const step = (run, who, payload, meta) => { const t = E.tokensFor(run, who)[0]; const res = E.handoff(run, t.id, payload, meta); if (!res.ok) throw new Error(res.error); return res; };
  let r = E.start(E.createRun({ id: 'r1', project: 'p', factory: standard(), task: 't' })).run;
  for (let i = 0; i < 3; i++) {
    r = step(r, 'manager', { summary: 'plan', answers: { dPlan: 'yes' } }).run;
    r = step(r, 'planner', { summary: 'p' + i }, { treeHash: 'h' + i }).run;
  }
  r = step(r, 'manager', { summary: 'plan', answers: { dPlan: 'yes' } }).run;
  const esc = step(r, 'planner', { summary: 'p4' }, { treeHash: 'h4' });
  r = esc.run;
  ok(r.exhausted.e4 === 1 && /limit of 3 rounds/.test(esc.effects.find(e => e.type === 'activate')?.context?.escalation || ''), 'engine: the planner loop ran out and escalated to the Manager');
  ok(!E.setLoopLimit(r, 'e4', 2).ok, 'engine: a limit below the rounds used is refused');
  ok(!E.setLoopLimit(r, 'n1', 5).ok && !E.setLoopLimit(r, 'nope', 5).ok, 'engine: only the flow\'s own connections (not notes, not unknown ids)');
  ok(!E.setLoopLimit(r, 'e4', 2.5).ok && !E.setLoopLimit(r, 'e4', -1).ok && !E.setLoopLimit(r, 'e4', E.RUN_ROUNDS_MAX + 1).ok, 'engine: whole numbers from 0 up to the most only');
  const same3 = E.setLoopLimit(r, 'e4', 3);
  ok(same3.ok && same3.run.exhausted.e4 === 1, 'engine: a limit equal to what is used keeps the loop closed (exhausted stays)');
  const up = E.setLoopLimit(r, 'e4', 5);
  ok(up.ok && up.run.snapshot.edges.find(e => e.id === 'e4').maxRounds === 5 && !up.run.exhausted.e4 && up.before === 3 && up.used === 3, 'engine: raised in the run\'s snapshot, exhausted cleared');
  ok(r.snapshot.edges.find(e => e.id === 'e4').maxRounds === 3, 'engine: the run it was given is untouched');
  let r2 = step(up.run, 'manager', { summary: 'plan more', answers: { dPlan: 'yes' } }).run;
  const through = step(r2, 'planner', { summary: 'p5' }, { treeHash: 'h5' });
  ok(through.run.edgeCounts.e4 === 4 && !through.effects.find(e => e.type === 'activate')?.context?.escalation, 'engine: after raising, the planner\'s handoff goes through to the Manager');
  const free = E.setLoopLimit(r, 'e4', 0);
  ok(free.ok && free.run.snapshot.edges.find(e => e.id === 'e4').maxRounds === 0, 'engine: 0 lifts the limit');
  const loops = E.loopsOf(free.run, ['e4']);
  ok(loops.some(l => l.edgeId === 'e4' && l.maxRounds === 0 && l.used === 3 && l.from === 'Planner' && l.to === 'Manager'), 'engine: loopsOf lists a lifted loop it is asked about, with labels and use');
  ok(!E.loopsOf(free.run).some(l => l.edgeId === 'e4') && E.loopsOf(r).some(l => l.edgeId === 'e22' && l.maxRounds === 3), 'engine: loopsOf lists every limited loop');
}

// ─── Journal: the event replays; caps release ───
{
  const T = (min) => new Date(Date.UTC(2026, 0, 1, 0, min)).toISOString();
  const wt = { repo: '/r', root: '/w', path: '/w/main', branch: 'factory/x', parentBranch: 'dev', baseCommit: 'abc' };
  const f = M.normalizeFactory({ ...standard(), settings: { caps: { maxMinutes: 30 } } });
  const evs = [
    { type: 'start', id: 'r1', project: 'p', factory: f, task: 't', wt, at: T(0) },
    { type: 'begin', at: T(0) },
    { type: 'cap', which: 'maxMinutes', limit: 30, at: T(30) },
  ];
  let rec = J.replay(evs);
  ok(rec.run.capHit?.which === 'maxMinutes' && rec.state.status === 'waiting' && J.isHeld(rec), 'journal: held at the time cap');
  const more = { type: 'run-limits', by: 'person', edges: { e4: 6, n1: 9, nope: 2 }, maxMinutes: 45, at: T(40) };
  rec = J.reduce(rec, more);
  ok(!rec.run.capHit && rec.state.status === 'running' && rec.run.time.since === T(40), 'journal: raising the time cap lets it carry on, the clock running again');
  ok(rec.run.time.maxMinutes === 45 && rec.state.snapshot.settings.caps.maxMinutes === 45 && J.timeLeft(rec, Date.parse(T(50))) === 5 * 60_000, 'journal: the new limit counts the time already used');
  ok(rec.state.snapshot.edges.find(e => e.id === 'e4').maxRounds === 6 && rec.run.runLimits.edges.e4 === 6 && !('n1' in rec.run.runLimits.edges) && !('nope' in rec.run.runLimits.edges), 'journal: loop limits set in the run\'s snapshot; notes and unknown ids ignored');
  ok(rec.state.history.some(h => h.type === 'run-limits' && h.released === 'maxMinutes' && h.edges.e4.from === 3 && h.edges.e4.to === 6), 'journal: the change is on the timeline');
  ok(same(J.replay([...evs, more]), rec), 'journal: replay gives the same run');
  // Raising too little: still no time left, still held.
  const tight = J.reduce(J.replay(evs), { type: 'run-limits', maxMinutes: 30, at: T(40) });
  ok(tight.run.capHit?.which === 'maxMinutes', 'journal: a cap with no room left keeps it held');
  // Held at the handoff cap, and paused too: the cap goes, the pause stays.
  const h = J.replay([...evs.slice(0, 2), { type: 'cap', which: 'maxHandoffs', limit: 50, at: T(5) }, { type: 'pause', by: 'person', at: T(6) }]);
  const hr = J.reduce(h, { type: 'run-limits', maxHandoffs: 80, at: T(7) });
  ok(!hr.run.capHit && hr.run.paused && hr.state.status === 'paused' && hr.state.snapshot.settings.caps.maxHandoffs === 80 && hr.run.capExtra === 0, 'journal: the handoff cap raised while paused: no longer capped, still paused');
  // A cache written before this change has no runLimits: the event still applies.
  const old = J.replay(evs); delete old.run.runLimits;
  ok(same(J.reduce(old, more), rec), 'journal: a cache from before the change takes the new event the same way (JOURNAL_VERSION unchanged: ' + J.JOURNAL_VERSION + ')');
}

// ─── A run whose loop ran out, raised by the person ───
function plannerBrains(o = {}) {
  const st = { plans: 0, escalated: 0, told: [], planned: 0 };
  return {
    st,
    brains: {
      manager: async (msg) => {
        if (msg.startsWith('[Factory] The person changed this run')) { st.told.push(msg); return { payload: { summary: 'once more', answers: { dPlan: 'yes' } } }; }
        if (/reached its limit/.test(msg)) { st.escalated++; return null; }
        if (msg.includes('answers.dPlan')) {
          if (st.plans < (o.plans ?? 4)) { st.plans++; return { payload: { summary: 'plan ' + st.plans, answers: { dPlan: 'yes' } } }; }
          return { payload: { summary: 'done', finish: true } };
        }
        return null;
      },
      planner: async (msg) => {
        const w = workIn(msg);
        if (w) writeFileSync(join(w, 'plan.txt'), 'plan ' + (++st.planned) + '\n');
        return { payload: { summary: 'planned ' + st.planned } };
      },
    },
  };
}

{
  const dir = repo('loop');
  const { st, brains } = plannerBrains({ plans: 4 });
  const team = fakeTeam(brains);
  const r = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: dir }), getFactory: () => ({ factory: standard() }), settleMs: 10_000 });
  team.setRunner(r);
  const s0 = await r.startRun({ project: 'loop', factoryId: 'x', task: 'Plan it' });
  const id = s0.run.id;
  ok(await until(() => st.escalated === 1), 'run: the planner loop ran out after 3 rounds and escalated to the Manager');
  const before = r.getRun(id);
  const l4 = before.loops.find(l => l.edgeId === 'e4');
  ok(l4 && l4.used === 3 && l4.maxRounds === 3 && l4.reopened === 1, 'summary: loops show what each used and its limit');
  ok(before.caps && before.caps.maxHandoffs >= 1 && before.caps.maxMinutes >= 1 && before.caps.handoffs === before.handoffs, 'summary: caps show the limits in force and what is used');
  const cachedBefore = cache(id).rec;

  ok(!(await r.setRunLimits(id, { edges: { e4: 2 } })).ok, 'validate: below the rounds used is refused');
  ok(!(await r.setRunLimits(id, { edges: { nope: 5 } })).ok, 'validate: an unknown connection is refused');
  ok(!(await r.setRunLimits(id, { edges: [5] })).ok, 'validate: edges must be an object');
  ok(!(await r.setRunLimits(id, { maxHandoffs: before.handoffs })).ok, 'validate: a handoff cap not above the handoffs made is refused');
  ok(!(await r.setRunLimits(id, { maxMinutes: 0 })).ok && !(await r.setRunLimits(id, { maxMinutes: 1.5 })).ok, 'validate: the time cap is whole minutes, at least 1');
  ok((await r.setRunLimits('missing', { edges: { e4: 5 } })).error === 'Run not found.', 'validate: an unknown run');
  const same0 = await r.setRunLimits(id, { edges: { e4: 3 } });
  ok(same0.ok && /Nothing changed/.test(same0.message) && events(id).every(e => e.type !== 'run-limits'), 'validate: no change writes nothing');

  const res = await r.setRunLimits(id, { edges: { e4: 5 } });
  ok(res.ok && res.run.loops.find(l => l.edgeId === 'e4').maxRounds === 5 && res.run.loops.find(l => l.edgeId === 'e4').reopened === 0, 'raise: the loop may go round 5 times, its exhausted count cleared');
  ok(await until(() => st.told.length === 1), 'raise: the Manager is told in its chat');
  ok(/"Planner" -> "Manager" \(e4\) may go round 5 times now/.test(st.told[0] || ''), 'raise: the note names the loop and the new limit');
  ok(await until(() => r.getRun(id).status === 'finishing'), 'raise: the Manager routed through the loop, the handoff went through, and it finished');
  const after = r.getRun(id);
  ok(after.loops.find(l => l.edgeId === 'e4').used === 4 && st.escalated === 1, 'raise: the loop counted one more round with no second escalation');
  ok(standard().edges.find(e => e.id === 'e4').maxRounds === 3, 'raise: the factory itself is not changed');
  const evs = events(id);
  ok(evs.some(e => e.type === 'run-limits' && e.edges.e4 === 5 && e.by === 'person'), 'journal: the change is an event');
  ok(same(J.replay(evs), cache(id).rec), 'journal: replaying the journal gives exactly the cached run');
  // From the cache written before the change, folding the rest gives the same.
  let rec = cachedBefore;
  for (const ev of evs.slice(cachedBefore.events)) rec = J.reduce(rec, ev);
  ok(same(rec, cache(id).rec), 'journal: the cache from before the change plus the new events gives the same run');
  // After a restart the run comes back with its raised limit.
  r.dispose();
  const r2 = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: fakeTeam({}).agents, getProject: () => ({ path: dir }), getFactory: () => ({ factory: standard() }), settleMs: 10_000 });
  r2.load();
  ok(r2.getRun(id).loops.find(l => l.edgeId === 'e4').maxRounds === 5, 'restart: the raised limit is still there');
  const later = await r2.setRunLimits(id, { edges: { e4: 9 } });
  ok(later.ok && later.run.loops.find(l => l.edgeId === 'e4').maxRounds === 9, 'restart: still changeable while the run is unfinished (here: ' + r2.getRun(id).status + ')');
  await r2.stopRun(id);
  ok(!(await r2.setRunLimits(id, { edges: { e4: 9 } })).ok, 'validate: a stopped run cannot be changed');
  r2.dispose();
}

// ─── The handoff cap, raised ───
{
  const dir = repo('handoffs');
  const { st, brains } = plannerBrains({ plans: 1 });
  const team = fakeTeam(brains);
  const f = standard(); f.settings = { caps: { maxHandoffs: 2 } };
  const r = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: dir }), getFactory: () => ({ factory: f }), settleMs: 10_000 });
  team.setRunner(r);
  const s0 = await r.startRun({ project: 'handoffs', factoryId: 'x', task: 'Cap' });
  const id = s0.run.id;
  ok(await until(() => r.getRun(id).capHit?.which === 'maxHandoffs'), 'handoffs: held at 2 handoffs');
  const msgs = () => team.of('manager')[0].msgs.length;
  const n = msgs();
  await sleep(100);
  ok(msgs() === n && r.getRun(id).status === 'waiting', 'handoffs: nobody starts while held');
  const res = await r.setRunLimits(id, { maxHandoffs: 40 });
  ok(res.ok && !res.run.capHit && res.run.caps.maxHandoffs === 40, 'handoffs: raised to 40, no longer held');
  ok(await until(() => r.getRun(id).status === 'finishing'), 'handoffs: the held step started and the run finished');
  ok(/carries on now/.test(team.of('manager')[0].msgs.slice(n).join('\n')), 'handoffs: the Manager was told why it carries on');
  ok(same(J.replay(events(id)), cache(id).rec), 'handoffs: replay gives the cached run');
  await r.stopRun(id); r.dispose();
}

// ─── The time cap, raised: carries on, and the clock is re-armed ───
{
  // Every event is stamped 2 min 59 s in the past: the run has been at work
  // that long when it starts, so a 1-minute cap holds it at once.
  const shift = 179_000;
  const dir = repo('clock');
  const team = fakeTeam({ manager: async () => null });
  const f = standard(); f.settings = { caps: { maxMinutes: 1 } };
  const r = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: dir }), getFactory: () => ({ factory: f }), settleMs: 10_000, now: () => new Date(Date.now() - shift).toISOString() });
  team.setRunner(r);
  const s0 = await r.startRun({ project: 'clock', factoryId: 'x', task: 'Over time' });
  const id = s0.run.id;
  ok(await until(() => r.getRun(id).capHit?.which === 'maxMinutes'), 'clock: held at the 1-minute cap');
  const res = await r.setRunLimits(id, { maxMinutes: 3 });
  ok(res.ok && !res.run.capHit && res.run.caps.maxMinutes === 3 && res.run.status === 'running', 'clock: raised to 3 minutes, it carries on');
  const again = await until(() => r.getRun(id).capHit, 5000);
  ok(again && again.which === 'maxMinutes' && again.limit === 3, 'clock: re-armed: the new limit holds it again when its time is up');
  const res2 = await r.setRunLimits(id, { maxMinutes: 60 });
  await sleep(1500);
  ok(res2.ok && !r.getRun(id).capHit && r.getRun(id).timeLeftMs > 50 * 60_000, 'clock: raised to an hour, it keeps going');
  ok(same(J.replay(events(id)), cache(id).rec), 'clock: replay gives the cached run');
  await r.stopRun(id); r.dispose();
}

// ─── The Manager, on the person's word ───
{
  const dir = repo('mgr');
  const team = fakeTeam({ manager: async () => null });
  const r = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: dir }), getFactory: () => ({ factory: standard() }), settleMs: 10_000 });
  team.setRunner(r);
  const s0 = await r.startRun({ project: 'mgr', factoryId: 'x', task: 'Ask' });
  const id = s0.run.id;
  const chat = await until(() => team.of('manager')[0]?.id);
  const n = team.of('manager')[0].msgs.length;
  const res = await r.controlFromManager(chat, { action: 'run-limits', edges: { e22: 0 }, maxHandoffs: 999 });
  ok(res.ok && /e22/.test(res.message) && /999 handoffs/.test(res.message), 'manager: run-limits changes the run and says what it is now');
  const run = r.getRun(id);
  ok(run.loops.find(l => l.edgeId === 'e22').maxRounds === 0 && run.caps.maxHandoffs === 999, 'manager: the loop has no limit and the cap is 999');
  ok(events(id).some(e => e.type === 'run-limits' && e.by === 'manager'), 'manager: the event says the Manager did it');
  await sleep(50);
  ok(team.of('manager')[0].msgs.length === n, 'manager: it is not sent a note about its own change');
  const emp = team.of('manager')[0];
  ok(!(await r.controlFromManager('not-a-chat', { action: 'run-limits', edges: { e22: 4 } })).ok, 'manager: only the Manager of a run');
  ok(!(await r.controlFromManager(emp.id, { action: 'run-limits', edges: { e22: -4 } })).ok, 'manager: validated like the person\'s');
  const st = await r.statusFor(emp.id);
  ok(st.ok !== false, 'manager: factory_status still answers');
  await r.stopRun(id); r.dispose();
}

try { rmSync(base, { recursive: true, force: true, maxRetries: 10, retryDelay: 300 }); } catch { /* a temp folder */ }
console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
