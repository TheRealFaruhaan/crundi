#!/usr/bin/env node
/**
 * test-factory-engine.mjs — src/factory-engine.js, walked through the Standard
 * factory (the design sketch) step by step.
 *
 * Covers: the Manager getting the task, decisions answered in one handoff
 * along a chain, parallel branches in their own workspaces, Joins merging,
 * decisions after a Join asked of their named answerer, note edges arriving in
 * an inbox, required notes on a route, pick-many reusing the last picks, loop
 * limits escalating to the Manager, the no-progress detector, escalation out
 * of a fan-out, the handoff cap, Ask me, and rejected handoffs leaving the
 * state untouched.
 *
 * Run: node scripts/test-factory-engine.mjs
 */

import * as E from '../src/factory-engine.js';
import { standard } from './factory-fixtures.mjs';

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const acts = (res) => res.effects.filter(e => e.type === 'activate');
const tokenOf = (run, who) => E.tokensFor(run, who)[0];

function begin(f = standard(), task = 'Add dark mode') {
  const run = E.createRun({ id: 'r1', project: 'p', factory: f, task });
  const s = E.start(run);
  return s;
}
function step(run, who, payload, meta) {
  const t = tokenOf(run, who);
  if (!t) throw new Error(`nobody waiting for ${who}: ${JSON.stringify(run.tokens)}`);
  const res = E.handoff(run, t.id, payload, meta);
  if (!res.ok) throw new Error(`${who}: ${res.error}`);
  return res;
}

// ─── Start ───
let res = begin();
ok(res.ok && acts(res).length === 1 && acts(res)[0].employeeId === 'manager' && acts(res)[0].mode === 'work', 'start: the Manager gets the task');
let run = res.run;
ok(run.status === 'running' && run.tokens.length === 1, 'start: one token, running');

// ─── Requirements for the Manager ───
{
  const req = E.requirements(run, tokenOf(run, 'manager').id);
  ok(req.canFinish && req.route.decision.id === 'dPlan', 'requirements: the Manager can finish, or answer "Needs plan?"');
  const no = req.route.decision.branches.find(b => b.answer === 'no');
  ok(no.next.decision && no.next.decision.id === 'dWho', 'requirements: answering "no" leads to "Which workers?", also the Manager\'s');
  ok(req.noteTargets.length === 2 && req.noteTargets.every(n => n.label === 'Instructions for reviewers'), 'requirements: the Manager may leave notes for both reviewers');
}

// ─── Rejected handoffs change nothing ───
{
  const t = tokenOf(run, 'manager');
  const bad = E.handoff(run, t.id, { summary: 'go', answers: {} });
  ok(!bad.ok && /Needs plan|plan/i.test(bad.error) && /answers\.dPlan/.test(bad.error), 'reject: a missing answer names the decision and where to put it');
  const bad2 = E.handoff(run, t.id, { summary: 'go', answers: { dPlan: 'no', dWho: ['db'] } });
  ok(!bad2.ok && /not an option/.test(bad2.error), 'reject: an unknown option is refused, listing the options');
  const bad3 = E.handoff(run, t.id, { answers: { dPlan: 'yes' } });
  ok(!bad3.ok && /summary/.test(bad3.error), 'reject: a handoff without a summary is refused');
  const bad4 = E.handoff(run, t.id, { summary: 's', answers: { dPlan: 'yes' }, notes: { bogus: 'x' } });
  ok(!bad4.ok && /notes\.bogus/.test(bad4.error), 'reject: an unknown note key is refused');
  ok(run.handoffs === 0 && run.tokens[0].nodeId === 'mgr', 'reject: the state is untouched');
}

// ─── Planner loop ───
res = step(run, 'manager', { summary: 'Needs a plan', answers: { dPlan: 'yes' } });
ok(acts(res)[0].employeeId === 'planner' && acts(res)[0].context.summary === 'Needs a plan', 'plan: the Planner is activated with the Manager\'s summary');
run = res.run;
res = step(run, 'planner', { summary: 'Plan written' }, { treeHash: 'A' });
ok(acts(res)[0].employeeId === 'manager' && run.edgeCounts.e4 === undefined && res.run.edgeCounts.e4 === 1, 'plan: back to the Manager, one round of the planner loop used');
run = res.run;

// ─── Fan-out to both devs, with notes for the reviewers ───
res = step(run, 'manager', { summary: 'Build it', answers: { dPlan: 'no', dWho: ['ui', 'backend'] }, notes: { uir: 'Check contrast' } });
{
  const ws = res.effects.filter(e => e.type === 'workspace');
  const a = acts(res);
  ok(ws.length === 2 && a.length === 2, 'fan-out: two workspaces, two employees activated');
  ok(new Set(a.map(x => x.employeeId)).size === 2 && a.every(x => ['fe', 'be'].includes(x.employeeId)), 'fan-out: the frontend and backend devs');
  ok(a[0].ws !== a[1].ws && a.every(x => x.ws !== 'main'), 'fan-out: each branch works in its own workspace');
  ok(res.effects.indexOf(ws[1]) < res.effects.indexOf(a[0]), 'fan-out: workspaces are created before anyone is activated');
}
run = res.run;
ok(run.inbox.uir && run.inbox.uir[0].text === 'Check contrast' && !run.inbox.ber, 'notes: the note waits in the UI reviewer\'s inbox; none for the backend reviewer');

res = step(run, 'fe', { summary: 'UI done' }, { treeHash: 'B' });
ok(res.effects.length === 0 && res.run.tokens.some(t => t.status === 'join'), 'join: the first branch waits at the Join');
run = res.run;
res = step(run, 'be', { summary: 'API done' }, { treeHash: 'C' });
{
  const m = res.effects.find(e => e.type === 'merge');
  ok(m && m.into === 'main' && m.from.length === 2, 'join: the last branch arriving merges both workspaces into main');
  const a = acts(res)[0];
  ok(a && a.employeeId === 'manager' && a.mode === 'answer' && a.nodeId === 'dSec', 'join: "Security?" after the Join is asked of its named answerer, the Manager');
  ok(/UI done/.test(a.context.summary) && /API done/.test(a.context.summary), 'join: the answerer sees both branches\' summaries');
  ok(res.effects.indexOf(m) < res.effects.indexOf(a), 'join: merging happens before the next step');
}
run = res.run;
ok(run.tokens.length === 1 && run.tokens[0].ws === 'main' && Object.keys(run.groups).length === 0, 'join: back to one token in main');

// ─── Answer mode: a chain of the Manager's decisions in one go ───
{
  const req = E.requirements(run, tokenOf(run, 'manager').id);
  ok(req.mode === 'answer' && req.route.decision.id === 'dSec', 'answer: requirements start at the decision itself');
}
res = step(run, 'manager', { summary: 'No security risk', answers: { dSec: 'no', dRev: ['ui'] } });
{
  const a = acts(res);
  ok(a.length === 1 && a[0].employeeId === 'uir' && a[0].ws === 'main', 'single pick: one reviewer, no extra workspace');
  ok(res.effects.every(e => e.type !== 'workspace'), 'single pick: no workspace is created');
  ok(a[0].context.notes.length === 1 && a[0].context.notes[0].text === 'Check contrast' && a[0].context.notes[0].label === 'Instructions for reviewers', 'notes: delivered when the flow reaches the reviewer');
}
run = res.run;
ok(!run.inbox.uir, 'notes: the inbox is emptied once delivered');

res = step(run, 'uir', { summary: 'Contrast too low' }, { treeHash: 'C' });
ok(acts(res)[0].employeeId === 'manager' && acts(res)[0].nodeId === 'dOk' && !res.effects.some(e => e.type === 'merge'), 'review: "Approved?" goes to the Manager; nothing to merge for a single branch');
run = res.run;

// ─── Required note on the way back; last picks reused ───
{
  const t = tokenOf(run, 'manager');
  const bad = E.handoff(run, t.id, { summary: 'Fix it', answers: { dOk: 'no' } });
  ok(!bad.ok && /What to change/.test(bad.error) && /notes\.e22/.test(bad.error), 'required note: going back without "What to change" is refused');
  const req = E.requirements(run, t.id);
  const noBranch = req.route.decision.branches.find(b => b.answer === 'no');
  ok(noBranch.next.requireNote && noBranch.next.decision.id === 'dWho' && noBranch.next.decision.lastPicks.join() === 'ui,backend', 'requirements: show the required note and the last picks');
}
res = step(run, 'manager', { summary: 'Fix contrast', answers: { dOk: 'no' }, notes: { e22: 'Raise contrast to 4.5:1' } });
{
  const a = acts(res);
  ok(a.length === 2 && a.every(x => x.context.message === 'Raise contrast to 4.5:1'), 'back-edge: both devs re-run (last picks) and get "What to change"');
}
run = res.run;

// ─── Loop limit: the review loop runs out ───
function reviewRound(run, n) {
  let r = step(run, 'fe', { summary: `UI fix ${n}` }, { treeHash: 'fe' + n }).run;
  r = step(r, 'be', { summary: `API fix ${n}` }, { treeHash: 'be' + n }).run;
  r = step(r, 'manager', { summary: 's', answers: { dSec: 'no', dRev: [] } }).run;   // no reviews: straight to Approved?
  return r;
}
run = reviewRound(run, 1);
res = step(run, 'manager', { summary: 'again', answers: { dOk: 'no' }, notes: { e22: 'more' } });
run = res.run;
run = reviewRound(run, 2);
res = step(run, 'manager', { summary: 'again', answers: { dOk: 'no' }, notes: { e22: 'more' } });
run = res.run;
ok(run.edgeCounts.e22 === 3, 'limit: three rounds of the review loop used');
run = reviewRound(run, 3);
// The Manager answering "no" a 4th time crosses a used-up edge: allowed once more, counted.
res = step(run, 'manager', { summary: 'one more', answers: { dOk: 'no' }, notes: { e22: 'last try' } });
ok(res.ok && res.run.exhausted.e22 === 1, 'limit: the Manager may reopen a used-up loop, and it is counted');
run = reviewRound(res.run, 4);
res = step(run, 'manager', { summary: 'again', answers: { dOk: 'no' }, notes: { e22: 'x' } });
run = reviewRound(res.run, 5);
{
  const t = tokenOf(run, 'manager');
  const r2 = E.handoff(run, t.id, { summary: 'again', answers: { dOk: 'no' }, notes: { e22: 'x' } });
  ok(!r2.ok && /closed/.test(r2.error), 'limit: after reopening twice the path is closed; the Manager must choose otherwise');
}

// An employee hitting a used-up edge escalates instead.
{
  let r = begin().run;
  for (let i = 0; i < 3; i++) {
    r = step(r, 'manager', { summary: 'plan', answers: { dPlan: 'yes' } }).run;
    r = step(r, 'planner', { summary: 'p' + i }, { treeHash: 'h' + i }).run;
  }
  r = step(r, 'manager', { summary: 'plan', answers: { dPlan: 'yes' } }).run;
  const res4 = step(r, 'planner', { summary: 'p4' }, { treeHash: 'h4' });
  const a = acts(res4)[0];
  ok(a.employeeId === 'manager' && /limit of 3 rounds/.test(a.context.escalation), 'limit: the Planner crossing a used-up loop escalates to the Manager with the reason');
}

// ─── No progress ───
{
  let r = begin().run;
  let last;
  for (let i = 0; i < 3; i++) {
    r = step(r, 'manager', { summary: 'plan', answers: { dPlan: 'yes' } }).run;
    last = step(r, 'planner', { summary: 'same' }, { treeHash: 'SAME' });
    r = last.run;
  }
  const a = acts(last)[0];
  ok(a.employeeId === 'manager' && /No progress/.test(a.context.escalation), 'no progress: the same answers on an unchanged tree three times escalate');
}

// ─── Escalation from inside a fan-out ───
{
  let r = begin().run;
  r = step(r, 'manager', { summary: 'build', answers: { dPlan: 'no', dWho: ['ui', 'backend'] } }).run;
  const res2 = step(r, 'fe', { escalate: 'The API contract is unclear' });
  const cancel = res2.effects.find(e => e.type === 'cancel');
  const merge = res2.effects.find(e => e.type === 'merge');
  const a = acts(res2)[0];
  ok(cancel && cancel.tokenIds.length === 1, 'escalate: the other branch is stopped');
  ok(merge && merge.abort && merge.into === 'main' && merge.from.length === 2, 'escalate: both branches\' work is merged back, not thrown away');
  ok(a.employeeId === 'manager' && /API contract/.test(a.context.escalation) && a.ws === 'main', 'escalate: the Manager gets the reason, working in main');
  ok(res2.run.tokens.length === 1 && Object.keys(res2.run.groups).length === 0, 'escalate: back to one token, no open fan-out');
  const t = tokenOf(res2.run, 'manager');
  ok(!E.handoff(res2.run, t.id, { escalate: 'x' }).ok, 'escalate: the Manager cannot escalate to itself');
}

// ─── Finish ───
{
  const r = begin().run;
  const fin = step(r, 'manager', { summary: 'Done: dark mode added', finish: true });
  ok(fin.effects.some(e => e.type === 'finish' && /dark mode/.test(e.summary)) && fin.run.status === 'finishing', 'finish: the Manager finishes; the run moves to presenting');
}

// ─── Cap ───
{
  const f = standard(); f.settings = { caps: { maxHandoffs: 2 } };
  let r = begin(f).run;
  r = step(r, 'manager', { summary: 'plan', answers: { dPlan: 'yes' } }).run;
  const res2 = step(r, 'planner', { summary: 'p' }, { treeHash: 'x' });
  ok(res2.effects.some(e => e.type === 'cap' && e.which === 'maxHandoffs') && res2.run.status === 'waiting', 'cap: reaching maxHandoffs stops the run for the human');
}

// ─── Ask me ───
{
  const f = standard();
  f.nodes.push({ id: 'ask', type: 'ask', ask: { prompt: 'Ship it?', replies: [{ id: 'go', label: 'Go' }, { id: 'more', label: 'More work' }] } });
  f.edges.find(e => e.id === 'e21').target = 'ask';
  f.edges.push({ source: 'ask', sourceHandle: 'go', target: 'mgr', maxRounds: 5 }, { source: 'ask', sourceHandle: 'more', target: 'dWho', maxRounds: 3, requireNote: true, noteLabel: 'What else' });
  let r = begin(f).run;
  r = step(r, 'manager', { summary: 's', answers: { dPlan: 'no', dWho: [] } }).run;
  const r2 = step(r, 'manager', { summary: 's', answers: { dSec: 'no', dRev: [] } });
  const res3 = step(r2.run, 'manager', { summary: 'looks good', answers: { dOk: 'yes' } });
  const ask = res3.effects.find(e => e.type === 'ask');
  ok(ask && ask.prompt === 'Ship it?' && ask.replies.length === 2 && res3.run.status === 'waiting', 'ask: the run waits on the human with the question and replies');
  const reply = E.askReply(res3.run, ask.tokenId, 'more', 'Add a toggle in settings');
  const a2 = reply.ok && acts(reply)[0];
  ok(a2 && a2.employeeId === 'manager' && a2.mode === 'answer' && a2.nodeId === 'dWho' && a2.context.message === 'Add a toggle in settings',
    'ask: after "More work", nobody is in front of "Which workers?", so the Manager is asked it, with your note');
  ok(!E.askReply(res3.run, ask.tokenId, 'nope').ok, 'ask: an unknown reply is refused');
}

console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
