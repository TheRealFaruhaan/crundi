#!/usr/bin/env node
/**
 * test-factory-ledger.mjs — the review ledger, the review gate and delegation,
 * in src/factory-engine.js.
 *
 * Covers: findings filed with ids that carry over between rounds; "no" at a
 * review gate refused without an open blocking finding; "yes" refused while
 * one is open; fixes claimed, verified and reopened; a new blocking finding
 * after the first review needing a reason; only the Manager dismissing;
 * approving closing what was claimed fixed; nits left as follow-ups; the
 * loop-limit escalation listing the findings; and the Manager handing an
 * ad-hoc job to someone, which always comes back to it and counts as a
 * handoff.
 *
 * Run: node scripts/test-factory-ledger.mjs
 */
import * as E from '../src/factory-engine.js';
import { validateFactory as validate, normalizeFactory as normalize } from '../src/factory-model.js';

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const acts = (res) => res.effects.filter(e => e.type === 'activate');
const tok = (run, who) => E.tokensFor(run, who)[0];
const step = (run, who, p) => { const r = E.handoff(run, tok(run, who).id, p); if (!r.ok) throw new Error(who + ': ' + r.error); return r.run; };
const tryStep = (run, who, p) => E.handoff(run, tok(run, who).id, p);

function factory() {
  return {
    name: 'Review loop',
    roster: [
      { id: 'manager', name: 'Manager', persona: 'Runs it.' },
      { id: 'dev', name: 'Developer', persona: 'Builds.' },
      { id: 'rev', name: 'Reviewer', persona: 'Reviews.' },
    ],
    nodes: [
      { id: 'start', type: 'start' }, { id: 'mgr', type: 'manager', label: 'Manager' }, { id: 'end', type: 'end' },
      { id: 'dev', type: 'employee', employeeId: 'dev', label: 'Developer' },
      { id: 'rev', type: 'employee', employeeId: 'rev', label: 'Reviewer' },
      { id: 'dOk', type: 'decision', label: 'Approved?', decision: { kind: 'yesno', question: 'Approved?', review: true } },
    ],
    edges: [
      { id: 'e1', source: 'start', target: 'mgr' },
      { id: 'e2', source: 'mgr', sourceHandle: 'finish', target: 'end' },
      { id: 'e3', source: 'mgr', target: 'dev' },
      { id: 'e4', source: 'dev', target: 'rev' },
      { id: 'e5', source: 'rev', target: 'dOk' },
      { id: 'e6', source: 'dOk', sourceHandle: 'yes', target: 'mgr', maxRounds: 3 },
      { id: 'e7', source: 'dOk', sourceHandle: 'no', target: 'dev', maxRounds: 2 },
    ],
  };
}

const f = factory();
const v = validate(normalize(f));
ok(v.ok, 'factory: a review loop validates' + (v.ok ? '' : ': ' + JSON.stringify(v.errors)));
ok(normalize(f).nodes.find(n => n.id === 'dOk').decision.review === true, 'model: the review flag survives normalize');

let run = E.start(E.createRun({ id: 'r', project: 'p', factory: f, task: 'Add dark mode' })).run;
run = step(run, 'manager', { summary: 'go' });
run = step(run, 'dev', { summary: 'built it' });

// Round 1: the reviewer.
let bad = tryStep(run, 'rev', { summary: 'meh', answers: { dOk: 'no' } });
ok(!bad.ok && /no open blocking finding/.test(bad.error), 'gate: "no" without a blocking finding is refused');
bad = tryStep(run, 'rev', { summary: 'meh', findings: [{ text: '' }] });
ok(!bad.ok && /needs a text/.test(bad.error), 'ledger: an empty finding is refused');
run = step(run, 'rev', { summary: 'two problems', answers: { dOk: 'no' }, findings: [{ text: 'Contrast too low', where: 'theme.css', blocking: true }, { text: 'Rename var', blocking: false }] });
let L = E.ledgerOf(run);
ok(L.items.length === 2 && L.items[0].id === 'F1' && L.items[0].blocking && L.items[0].status === 'open' && L.items[1].id === 'F2' && !L.items[1].blocking, 'ledger: findings get ids F1, F2, blocking and nit');
ok(tok(run, 'dev'), 'gate: "no" with a blocking finding goes back to the developer');

// The developer claims the fix; an unknown id is caught.
bad = tryStep(run, 'dev', { summary: 'fixed', done: ['F9'] });
ok(!bad.ok && /no finding F9/.test(bad.error) && /F1/.test(bad.error), 'ledger: an unknown id is refused, listing the open ones');
bad = tryStep(run, 'dev', { summary: 'fixed', dismiss: ['F2'] });
ok(!bad.ok && /Only the Manager/.test(bad.error), 'ledger: only the Manager dismisses');
run = step(run, 'dev', { summary: 'fixed', done: ['f1'] });
ok(E.ledgerOf(run).items[0].status === 'claimed' && E.ledgerOf(run).items[0].claimedBy === 'dev', 'ledger: "done" marks it claimed (ids are case-insensitive)');

// Round 2: a new blocking finding without a reason is refused.
bad = tryStep(run, 'rev', { summary: 'new thing', answers: { dOk: 'no' }, findings: [{ text: 'Use CSS vars', blocking: true }] });
ok(!bad.ok && /needs a reason/.test(bad.error), 'ledger: after the first review, a new blocking finding needs a reason');
bad = tryStep(run, 'rev', { summary: 'ok', answers: { dOk: 'no' }, findings: [{ text: 'Use CSS vars', blocking: false }] });
ok(!bad.ok && /no open blocking finding/.test(bad.error), 'gate: a nit alone cannot block');
// Reopen F1: not really fixed.
run = step(run, 'rev', { summary: 'still low', answers: { dOk: 'no' }, reopen: ['F1'] });
ok(E.ledgerOf(run).items[0].status === 'open' && E.ledgerOf(run).items[0].reopened === 1, 'ledger: reopened, counted');
run = step(run, 'dev', { summary: 'fixed for real', done: ['F1'] });
// Round 3 would be past the loop limit, so try approving with an open blocker first.
bad = tryStep(run, 'rev', { summary: 'ok', answers: { dOk: 'yes' }, findings: [{ text: 'Breaks IE', blocking: true, reason: 'Customers use it' }] });
ok(!bad.ok && /still open: F3/.test(bad.error), '"yes" is refused while a blocking finding is open');
run = step(run, 'rev', { summary: 'approved', answers: { dOk: 'yes' } });
L = E.ledgerOf(run);
ok(L.items[0].status === 'closed' && L.items[0].closedBy === 'rev', 'gate: approving closes what was claimed fixed');
ok(L.items[1].status === 'open' && !L.items[1].blocking, 'ledger: the nit is still open (a follow-up)');
ok(tok(run, 'manager'), 'gate: approved goes back to the Manager');

// The Manager dismisses the nit.
run = step(run, 'manager', { summary: 'done', dismiss: ['F2'], finish: true });
ok(E.ledgerOf(run).items[1].status === 'dismissed' && run.status === 'finishing', 'ledger: the Manager may dismiss a finding');

// ─── Loop-limit escalation lists the findings ───
{
  let r = E.start(E.createRun({ id: 'r2', project: 'p', factory: factory(), task: 'x' })).run;
  r = step(r, 'manager', { summary: 'go' });
  for (let i = 0; i < 2; i++) {
    r = step(r, 'dev', { summary: 'try ' + i, ...(i ? { done: ['F1'] } : {}) });
    r = step(r, 'rev', { summary: 'no', answers: { dOk: 'no' }, ...(i ? { reopen: ['F1'] } : { findings: [{ text: 'Contrast too low', blocking: true }] }) });
  }
  r = step(r, 'dev', { summary: 'try 2', done: ['F1'] });
  const res = E.handoff(r, tok(r, 'rev').id, { summary: 'no', answers: { dOk: 'no' }, reopen: ['F1'] });
  const a = acts(res)[0];
  ok(res.ok && a.employeeId === 'manager' && /limit of 2/.test(a.context.escalation) && /Still open: F1 \(blocking\): Contrast too low \[reopened 2x\]/.test(a.context.escalation), 'limit: the escalation says why, with the open findings and how often they were reopened');
}

// ─── Delegation ───
{
  let r = E.start(E.createRun({ id: 'r3', project: 'p', factory: factory(), task: 'x' })).run;
  const m = tok(r, 'manager');
  ok(!E.delegate(r, m.id, 'Nobody', 'x').ok, 'delegate: an unknown person is refused');
  ok(!E.delegate(r, m.id, 'Developer', '  ').ok, 'delegate: a job needs words');
  const before = r.handoffs;
  const d = E.delegate(r, m.id, 'Developer', 'Resolve the conflict in theme.css');
  ok(d.ok && acts(d)[0].employeeId === 'dev' && acts(d)[0].mode === 'delegate' && acts(d)[0].context.message === 'Resolve the conflict in theme.css', 'delegate: the job goes to the developer, by name');
  r = d.run;
  ok(r.handoffs === before + 1 && r.tokens.find(t => t.id === m.id).status === 'delegated', 'delegate: counted as a handoff; the Manager waits');
  ok(!E.delegate(r, m.id, 'Reviewer', 'y').ok, 'delegate: one job at a time');
  const dt = tok(r, 'dev');
  ok(dt && dt.delegate && E.personOf(r, dt) === 'dev', 'delegate: the job is the developer\'s token');
  ok(!E.delegate(r, dt.id, 'Reviewer', 'y').ok, 'delegate: only the Manager hands out jobs');
  const back = E.handoff(r, dt.id, { summary: 'Resolved, kept both rules' });
  ok(back.ok && acts(back)[0].employeeId === 'manager' && /finished the job you handed out: Resolved/.test(acts(back)[0].context.message), 'delegate: it always comes back to the Manager, with the summary');
  r = back.run;
  ok(r.tokens.length === 1 && r.tokens[0].status === 'work' && !r.tokens.some(t => t.delegate), 'delegate: the Manager carries on where it was');
  const esc = E.delegate(r, tok(r, 'manager').id, 'Reviewer', 'Check it');
  const e2 = E.handoff(esc.run, tok(esc.run, 'rev').id, { escalate: 'No access' });
  ok(e2.ok && /could not do the job you handed out: No access/.test(acts(e2)[0].context.message), 'delegate: an escalated job comes back saying why');
}

console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
