#!/usr/bin/env node
/**
 * test-factory-live.mjs — what the live graph reads.
 *
 * Covers: the run's events, whole and from an index (how the live view keeps
 * up without refetching everything); the skills a person loaded during a step
 * recorded on that step's handoff, counted per step rather than again and
 * again; replaying a prefix of the events giving the run as it was at that
 * point (the replay slider); and the journal staying the source of truth for
 * all of it.
 *
 * Run: node scripts/test-factory-live.mjs
 */
import { mkdtempSync, rmSync, writeFileSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';

const base = mkdtempSync(join(tmpdir(), 'factory-live-'));
process.env.DATA_DIR = join(base, 'data');
process.env.FACTORY_WORKTREES_DIR = join(base, 'wt');
const J = await import('../src/factory-journal.js');
const { createFactoryRunner } = await import('../src/factory-runner.js');
const { standard, fakeTeam } = await import('./factory-fixtures.mjs');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
async function until(fn, ms = 20000) { const end = Date.now() + ms; while (Date.now() < end) { const v = await fn(); if (v) return v; await sleep(15); } return null; }

const repo = join(base, 'p'); mkdirSync(repo);
const g = (a) => execFileSync('git', a, { cwd: repo }); g(['init', '-q', '-b', 'dev']); writeFileSync(join(repo, 'a'), 'a'); g(['add', '.']); g(['-c', 'user.name=t', '-c', 'user.email=t@t', 'commit', '-qm', 'i']);
const workIn = (m) => (m.match(/^Work in: (.+)$/m) || [])[1];
let feTurn = 0;
const used = new Map();   // chatId -> skills loaded so far
const team = fakeTeam({
  manager: async (msg) => {
    if (msg.includes('answers.dSec')) return { payload: { summary: 's', answers: { dSec: 'no', dRev: [] } } };
    if (msg.includes('answers.dOk')) return { payload: { summary: 'again', answers: { dOk: 'no' }, notes: { e22: 'more' } } };
    if (msg.includes('answers.dPlan')) return { payload: { summary: 'go', answers: { dPlan: 'no', dWho: ['ui'] } } };
    return null;
  },
  fe: async (m, s) => {
    feTurn++;
    const list = used.get(s.id) || [];
    list.push(feTurn === 1 ? 'frontend-design' : 'run');
    if (feTurn === 1) list.push('frontend-design');
    used.set(s.id, list);
    writeFileSync(join(workIn(m), 'ui.txt'), 'v' + feTurn);
    return feTurn <= 2 ? { payload: { summary: 'UI ' + feTurn } } : null;
  },
});
team.agents.skillsUsed = (id) => used.get(id) || [];
const r = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: repo }), getFactory: () => ({ factory: standard() }), settleMs: 10_000 });
team.setRunner(r);
const st = await r.startRun({ project: 'p', factoryId: 'x', task: 'Live' });
await until(() => feTurn >= 3);
await sleep(100);

const all = r.runEvents(st.run.id);
ok(all.ok && all.from === 0 && all.total === all.events.length && all.events[0].type === 'start', 'events: the whole journal, from the start');
const tail = r.runEvents(st.run.id, all.total - 3);
ok(tail.events.length === 3 && tail.from === all.total - 3 && JSON.stringify(tail.events) === JSON.stringify(all.events.slice(-3)), 'events: from an index, only what came after');
ok(!r.runEvents('nope').ok, 'events: an unknown run is refused');

const handoffs = all.events.filter(e => e.type === 'handoff' && e.by === 'fe');
ok(handoffs.length === 2 && JSON.stringify(handoffs[0].skills) === '["frontend-design"]', 'skills: the first step records the skill it loaded, once');
ok(JSON.stringify(handoffs[1].skills) === '["run"]', 'skills: the next step records only what was loaded since');
const rec = J.replay(all.events);
const hist = rec.state.history.filter(h => h.type === 'handoff' && h.by === 'fe');
ok(hist[0].skills?.[0] === 'frontend-design' && hist[1].skills?.[0] === 'run', 'skills: they ride on the step\'s history entry in the fold');

// Replay of a prefix: the run as it was then.
const iFirst = all.events.findIndex(e => e.type === 'handoff' && e.by === 'fe');
const before = J.replay(all.events.slice(0, iFirst));
const after = J.replay(all.events.slice(0, iFirst + 1));
ok(before.state.tokens.some(t => t.nodeId === 'fe' && t.status === 'work'), 'replay: just before, the frontend dev has the work');
ok(!after.state.tokens.some(t => t.nodeId === 'fe' && t.status === 'work') && after.state.handoffs === before.state.handoffs + 1, 'replay: one event later, the handoff has happened');

await r.stopRun(st.run.id);
r.dispose();
try { rmSync(base, { recursive: true, force: true, maxRetries: 10, retryDelay: 300 }); } catch { /* Windows may still hold a file; it is a temp folder */ };
console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
