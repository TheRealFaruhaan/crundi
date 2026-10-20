#!/usr/bin/env node
/**
 * test-factory-validate.mjs — src/factory-model.js and src/factory-store.js.
 *
 * The validator is the promise that a factory always ends: every node can
 * reach End, every loop has a round limit and a way out, every decision is
 * fully wired, and fan-outs are closed by their Join. The reference case is
 * the hand-drawn "Standard" flow from the design (planner loop, parallel
 * devs, optional security, parallel reviewers, approval loop): it must pass,
 * and breaking any one of those properties must produce the matching error.
 *
 * Run: node scripts/test-factory-validate.mjs
 */

import { mkdtempSync, rmSync, readdirSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const dir = mkdtempSync(join(tmpdir(), 'factory-test-'));
process.env.DATA_DIR = dir;
const m = await import('../src/factory-model.js');
const store = await import('../src/factory-store.js');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const clone = o => JSON.parse(JSON.stringify(o));
const codes = r => r.errors.map(e => e.code);
const has = (r, code) => codes(r).includes(code);

// ─── The reference factory: the design sketch ───

function standard() {
  return {
    name: 'Standard',
    roster: [
      { id: 'manager', name: 'Manager', persona: 'Runs it.' },
      { id: 'planner', name: 'Planner', persona: 'Plans.' },
      { id: 'fe', name: 'Frontend dev', persona: 'UI.' },
      { id: 'be', name: 'Backend dev', persona: 'Server.' },
      { id: 'sec', name: 'Security dev', persona: 'Security.' },
      { id: 'uir', name: 'UI reviewer', persona: 'Reviews UI.' },
      { id: 'ber', name: 'Backend reviewer', persona: 'Reviews server.' },
    ],
    nodes: [
      { id: 'start', type: 'start' },
      { id: 'mgr', type: 'manager', label: 'Manager' },
      { id: 'end', type: 'end' },
      { id: 'dPlan', type: 'decision', label: 'Needs plan?', decision: { kind: 'yesno', question: 'Does it need a plan or a plan update?' } },
      { id: 'planner', type: 'employee', employeeId: 'planner', label: 'Planner' },
      { id: 'dWho', type: 'decision', label: 'Which workers?', decision: { kind: 'many', question: 'Which workers are needed?', options: [{ id: 'ui', label: 'UI' }, { id: 'backend', label: 'Backend' }] } },
      { id: 'fe', type: 'employee', employeeId: 'fe', label: 'Frontend dev' },
      { id: 'be', type: 'employee', employeeId: 'be', label: 'Backend dev' },
      { id: 'jDev', type: 'join', label: 'Devs done', join: { forkId: 'dWho' } },
      { id: 'dSec', type: 'decision', label: 'Security?', decision: { kind: 'yesno', question: 'Is a security analysis needed?', answeredBy: 'manager' } },
      { id: 'sec', type: 'employee', employeeId: 'sec', label: 'Security dev' },
      { id: 'dRev', type: 'decision', label: 'Which reviewers?', decision: { kind: 'many', question: 'Which reviews are needed?', answeredBy: 'manager', options: [{ id: 'ui', label: 'UI' }, { id: 'backend', label: 'Backend' }] } },
      { id: 'uir', type: 'employee', employeeId: 'uir', label: 'UI reviewer' },
      { id: 'ber', type: 'employee', employeeId: 'ber', label: 'Backend reviewer' },
      { id: 'jRev', type: 'join', label: 'Reviews done', join: { forkId: 'dRev' } },
      { id: 'dOk', type: 'decision', label: 'Approved?', decision: { kind: 'yesno', question: 'Is the work approved?', answeredBy: 'manager' } },
    ],
    edges: [
      { id: 'e1', source: 'start', target: 'mgr' },
      { id: 'eFin', source: 'mgr', sourceHandle: 'finish', target: 'end' },
      { id: 'e2', source: 'mgr', target: 'dPlan' },
      { id: 'e3', source: 'dPlan', sourceHandle: 'yes', target: 'planner' },
      { id: 'e4', source: 'planner', target: 'mgr', maxRounds: 3 },
      { id: 'e5', source: 'dPlan', sourceHandle: 'no', target: 'dWho' },
      { id: 'e6', source: 'dWho', sourceHandle: 'ui', target: 'fe' },
      { id: 'e7', source: 'dWho', sourceHandle: 'backend', target: 'be' },
      { id: 'e8', source: 'dWho', sourceHandle: 'none', target: 'jDev' },
      { id: 'e9', source: 'fe', target: 'jDev' },
      { id: 'e10', source: 'be', target: 'jDev' },
      { id: 'e11', source: 'jDev', target: 'dSec' },
      { id: 'e12', source: 'dSec', sourceHandle: 'yes', target: 'sec' },
      { id: 'e13', source: 'dSec', sourceHandle: 'no', target: 'dRev' },
      { id: 'e14', source: 'sec', target: 'dRev' },
      { id: 'e15', source: 'dRev', sourceHandle: 'ui', target: 'uir' },
      { id: 'e16', source: 'dRev', sourceHandle: 'backend', target: 'ber' },
      { id: 'e17', source: 'dRev', sourceHandle: 'none', target: 'jRev' },
      { id: 'e18', source: 'uir', target: 'jRev' },
      { id: 'e19', source: 'ber', target: 'jRev' },
      { id: 'e20', source: 'jRev', target: 'dOk' },
      { id: 'e21', source: 'dOk', sourceHandle: 'yes', target: 'mgr', maxRounds: 5 },
      { id: 'e22', source: 'dOk', sourceHandle: 'no', target: 'dWho', maxRounds: 3, requireNote: true, noteLabel: 'What to change' },
      { id: 'n1', kind: 'note', source: 'mgr', target: 'uir', noteLabel: 'Instructions for reviewers' },
      { id: 'n2', kind: 'note', source: 'mgr', target: 'ber', noteLabel: 'Instructions for reviewers' },
    ],
  };
}
const edit = (fn) => { const f = standard(); fn(f); return m.validateFactory(f); };
const dropEdge = (f, id) => { f.edges = f.edges.filter(e => e.id !== id); };
const edge = (f, id) => f.edges.find(e => e.id === id);

// ─── Valid factories ───

const sk = m.validateFactory(m.skeletonFactory());
ok(sk.ok && sk.errors.length === 0, 'the skeleton (Start -> Manager -> Finish -> End) is valid');

const std = m.validateFactory(standard());
ok(std.ok, 'the Standard sketch is valid' + (std.ok ? '' : ': ' + codes(std).join(', ')));
ok(std.warnings.length === 0, 'the Standard sketch has no warnings' + (std.warnings.length ? ': ' + std.warnings.map(w => w.code).join(', ') : ''));

// ─── Always a way out ───

ok(has(edit(f => dropEdge(f, 'eFin')), 'output-unwired'), 'the Manager without Finish is refused');
ok(has(edit(f => { f.nodes = f.nodes.filter(n => n.id !== 'end'); dropEdge(f, 'eFin'); }), 'end-missing'), 'no End is refused');
ok(has(edit(f => { f.nodes.push({ id: 'end2', type: 'end' }); }), 'end-many'), 'two Ends are refused');
ok(has(edit(f => { f.nodes.push({ id: 'mgr2', type: 'manager' }); }), 'manager-many'), 'two Manager nodes are refused');
ok(has(edit(f => { f.nodes = f.nodes.filter(n => n.id !== 'start'); dropEdge(f, 'e1'); }), 'start-missing'), 'no Start is refused');
ok(has(edit(f => { edge(f, 'e1').target = 'dPlan'; }), 'start-not-manager'), 'Start must lead to the Manager');

{
  const r = edit(f => { f.nodes.push({ id: 'orphan', type: 'employee', employeeId: 'fe' }); f.edges.push({ source: 'orphan', target: 'mgr' }); });
  ok(r.errors.some(e => e.code === 'node-unreachable' && e.nodeId === 'orphan'), 'a node nothing leads to is flagged as unreachable');
}
{
  const r = edit(f => {
    f.roster.push({ id: 'x', name: 'Extra', persona: 'p' });
    f.nodes.push({ id: 'x', type: 'employee', employeeId: 'x' });
    f.edges.push({ source: 'x', target: 'mgr' }, { kind: 'note', source: 'mgr', target: 'x' });
  });
  ok(r.errors.some(e => e.code === 'reached-only-by-notes' && e.nodeId === 'x'), 'a node reached only by a note edge is flagged: notes do not move control');
}
{
  const r = edit(f => { edge(f, 'e14').target = 'dead'; f.roster.push({ id: 'd', name: 'Dead', persona: 'p' }); f.nodes.push({ id: 'dead', type: 'employee', employeeId: 'd' }); });
  ok(r.errors.some(e => e.code === 'output-unwired' && e.nodeId === 'dead'), 'a node with an unwired output is flagged');
  ok(r.errors.some(e => e.code === 'cannot-finish' && e.nodeId === 'dead'), 'a dead end is flagged as unable to finish');
}
{
  // A -> B -> A with a limit, and nothing leads out.
  const r = edit(f => {
    f.roster.push({ id: 'a', name: 'A', persona: 'p' }, { id: 'b', name: 'B', persona: 'p' });
    f.nodes.push({ id: 'a', type: 'employee', employeeId: 'a' }, { id: 'b', type: 'employee', employeeId: 'b' });
    edge(f, 'e14').target = 'a';
    f.edges.push({ source: 'a', target: 'b' }, { source: 'b', target: 'a', maxRounds: 3 });
  });
  ok(r.errors.some(e => e.code === 'loop-no-exit' && e.nodeIds.includes('a') && e.nodeIds.includes('b')), 'a loop with no way out is flagged, naming its nodes');
  ok(!r.errors.some(e => e.code === 'cannot-finish' && (e.nodeId === 'a' || e.nodeId === 'b')), '...once, as a loop, not again node by node');
}
ok(has(edit(f => { edge(f, 'e19').target = 'end'; }), 'end-not-via-manager'), 'an employee cannot go straight to End; the Manager decides the end');
ok(has(edit(f => { edge(f, 'eFin').target = 'dPlan'; }), 'finish-not-end'), 'Finish must lead to End');

// ─── Decisions wired ───

ok(has(edit(f => dropEdge(f, 'e5')), 'output-unwired'), 'a yes/no decision missing "no" is refused');
ok(has(edit(f => dropEdge(f, 'e8')), 'output-unwired'), 'a pick-many without a "none of these" path is refused');
ok(has(edit(f => { f.nodes.find(n => n.id === 'dWho').decision.options.push({ id: 'db', label: 'Database' }); }), 'output-unwired'), 'adding an option without wiring it is refused');
ok(has(edit(f => { f.nodes.find(n => n.id === 'dWho').decision.options = [{ id: 'ui', label: 'UI' }]; }), 'decision-few-options'), 'a pick decision with one option is refused');
ok(has(edit(f => { f.nodes.find(n => n.id === 'dPlan').decision.question = ' '; }), 'decision-no-question'), 'a decision without a question is refused');
ok(has(edit(f => { f.edges.push({ source: 'fe', target: 'mgr' }); }), 'handle-many'), 'two connections on one output are refused (branching needs a Decision)');
ok(has(edit(f => { f.nodes.find(n => n.id === 'dSec').decision.answeredBy = null; }), 'decision-needs-answerer'), 'a decision right after a Join must say who answers it');
ok(!has(edit(f => { f.nodes.find(n => n.id === 'dPlan').decision.answeredBy = null; }), 'decision-needs-answerer'), 'a decision right after the Manager is answered by the Manager');
ok(has(edit(f => { f.nodes.find(n => n.id === 'dSec').decision.answeredBy = 'ghost'; }), 'decision-answerer-missing'), 'an answerer no longer on the roster is refused');

{
  const r = edit(f => {
    f.nodes.push({ id: 'ask', type: 'ask', ask: { prompt: 'Ship it?', replies: [{ id: 'go', label: 'Go' }, { id: 'stop', label: 'Stop' }] } });
    edge(f, 'e21').target = 'ask';
    f.edges.push({ source: 'ask', sourceHandle: 'go', target: 'mgr', maxRounds: 5 });
  });
  ok(r.errors.some(e => e.code === 'output-unwired' && e.nodeId === 'ask' && e.handle === 'stop'), 'an Ask me reply left unwired is refused');
}

// ─── Loops have limits ───

ok(has(edit(f => { edge(f, 'e22').maxRounds = 0; }), 'loop-no-limit'), 'the review loop without a round limit is refused');
ok(has(edit(f => { edge(f, 'e4').maxRounds = 0; }), 'loop-no-limit'), 'the planner loop without a round limit is refused');
{
  const r = edit(f => { edge(f, 'e21').maxRounds = 0; });
  ok(has(r, 'loop-no-limit'), 'the whole-flow loop back to the Manager without a limit is refused');
}
{
  const r = edit(f => { edge(f, 'e1').maxRounds = 2; });
  ok(r.ok && r.warnings.some(w => w.code === 'limit-not-in-loop'), 'a round limit outside any loop is only a warning');
}
{
  const g = m.flowGraph(m.normalizeFactory(standard()));
  const cs = m.cycles(g);
  ok(cs.length === 1 && cs[0].length >= 10, 'cycles(): the Standard graph is one big strongly connected loop');
  const g2 = m.flowGraph(m.normalizeFactory(standard()), { exclude: e => e.maxRounds > 0 });
  ok(m.cycles(g2).length === 0, 'cycles(): with limited edges removed, the Standard graph is acyclic');
}

// ─── Fan-out and Join ───

ok(has(edit(f => { f.nodes.find(n => n.id === 'jDev').join.forkId = ''; }), 'join-no-fork'), 'a Join that does not name its decision is refused');
ok(has(edit(f => { f.nodes.find(n => n.id === 'jDev').join.forkId = 'dPlan'; }), 'join-no-fork'), 'a Join naming a yes/no decision is refused');
{
  const r = edit(f => { f.nodes.find(n => n.id === 'jRev').join.forkId = 'dWho'; });
  ok(has(r, 'fork-no-join') && has(r, 'fork-many-joins'), 'two Joins on one fan-out (and none on the other) are refused');
}
ok(has(edit(f => { edge(f, 'e10').target = 'mgr'; }), 'fork-branch-escapes'), 'a branch that leaves for the Manager before its Join is refused');
{
  const r = edit(f => { edge(f, 'e13').target = 'be'; });
  ok(has(r, 'fork-branch-entered'), 'jumping into a branch from outside its decision is refused');
}
ok(has(edit(f => { edge(f, 'e14').target = 'jDev'; }), 'join-entered-outside'), 'a Join reached from outside its fan-out is refused');
ok(has(edit(f => { edge(f, 'e8').target = 'fe'; }), 'fork-none-into-branch'), '"none of these" leading into a branch is refused');
{
  const r = edit(f => { edge(f, 'e7').target = 'jDev'; });
  ok(r.warnings.some(w => w.code === 'fork-empty-branch'), 'a branch that goes straight to its Join is a warning');
}

// ─── Roster and edges ───

ok(has(edit(f => { f.roster = f.roster.filter(r => r.id !== 'manager'); }), 'roster-no-manager'), 'a roster without the Manager is refused');
ok(has(edit(f => { f.roster.find(r => r.id === 'be').name = 'frontend DEV'; }), 'roster-duplicate-name'), 'two employees with the same name are refused');
ok(has(edit(f => { f.nodes.find(n => n.id === 'fe').employeeId = 'nobody'; }), 'employee-unassigned'), 'an employee node with nobody assigned is refused');
ok(has(edit(f => { f.nodes.find(n => n.id === 'fe').employeeId = 'manager'; }), 'employee-is-manager'), 'the Manager cannot be placed as an employee');
ok(edit(f => { f.roster.push({ id: 'idle', name: 'Idle', persona: 'p' }); }).warnings.some(w => w.code === 'roster-unused'), 'an employee not in the flow is a warning');
ok(has(edit(f => { f.edges.push({ kind: 'note', source: 'dPlan', target: 'fe' }); }), 'note-not-agents'), 'notes only go between people');
ok(has(edit(f => { edge(f, 'e3').target = 'gone'; }), 'edge-dangling'), 'a connection to a deleted node is refused');
ok(has(edit(f => { edge(f, 'e3').sourceHandle = 'maybe'; }), 'edge-bad-handle'), 'a connection from an output the node lacks is refused');
ok(has(edit(f => { f.edges.push({ source: 'end', target: 'mgr' }); }), 'edge-out-of-end'), 'End cannot lead anywhere');

// ─── Every error says how to fix it ───

{
  const broken = standard();
  broken.edges = broken.edges.filter(e => !['e5', 'e8', 'eFin'].includes(e.id));
  edge(broken, 'e22').maxRounds = 0;
  const r = m.validateFactory(broken);
  ok(r.errors.length > 0 && r.errors.every(e => e.message && e.fix), 'every error carries a message and a fix');
}

// ─── Normalize is defensive ───

{
  let threw = false, n;
  try { n = m.normalizeFactory({ nodes: 'x', edges: [null, 5, { kind: 'weird' }], roster: [{ id: '../evil', name: 'x'.repeat(500) }], settings: { caps: { maxHandoffs: -5 } } }); } catch { threw = true; }
  ok(!threw, 'normalize never throws on garbage');
  ok(n.nodes.length === 0 && n.edges.length === 1 && n.edges[0].kind === 'flow', 'normalize drops non-objects and defaults unknown edge kinds');
  ok(n.roster[0].id !== '../evil' && n.roster[0].name.length === m.LIMITS.name, 'normalize replaces unsafe ids and truncates names');
  ok(n.settings.caps.maxHandoffs === 1 && n.settings.managerMode === 'normal' && n.settings.mergeMode === 'ask', 'normalize clamps caps and defaults modes');
  ok(m.validateFactory(null).ok === false, 'validate on nothing is not runnable, and does not throw');
}

// ─── Store ───

{
  const c = store.createFactory({ name: 'Solo' });
  ok(c.ok && c.factory.id.startsWith('f_') && c.factory.version === 1 && c.validation.ok, 'create: a new factory is the valid skeleton at version 1');
  const id = c.factory.id;

  const s1 = store.saveFactory(id, { ...standard(), name: 'Standard' }, { baseVersion: 1 });
  ok(s1.ok && s1.factory.version === 2 && s1.validation.ok && s1.factory.createdAt === c.factory.createdAt, 'save: a whole definition is written, version bumps, createdAt kept');
  const s2 = store.saveFactory(id, standard(), { baseVersion: 1 });
  ok(!s2.ok && s2.conflict && s2.version === 2, 'save: a stale baseVersion is refused as a conflict');

  const broken = standard(); broken.edges = broken.edges.filter(e => e.id !== 'eFin');
  const s3 = store.saveFactory(id, broken, { baseVersion: 2 });
  ok(s3.ok && !s3.validation.ok, 'save: an invalid factory is still saved, as a draft');

  const list = store.listFactories();
  ok(list.length === 1 && list[0].id === id && list[0].runnable === false && list[0].errorCount > 0, 'list: summaries say whether each factory can run');

  const g = store.getFactory(id);
  ok(g && g.factory.nodes.length === standard().nodes.length && g.validation.errors.length > 0, 'get: returns the factory with its validation');
  ok(store.getFactory('../../etc/passwd') === null, 'get: an id with path characters is refused');

  const d = store.duplicateFactory(id);
  ok(d.ok && d.factory.id !== id && d.factory.name === 'Standard (copy)' && d.factory.version === 1, 'duplicate: new id, "(copy)" name, version 1');

  const ex = store.exportFactory(d.factory.id);
  ok(ex.ok && ex.data.format === 'crundi-factory' && !('id' in ex.data.factory) && !('version' in ex.data.factory), 'export: portable, without id or version');
  const im = store.importFactory(JSON.stringify(ex.data));
  ok(im.ok && im.factory.id !== d.factory.id && im.factory.nodes.length === d.factory.nodes.length, 'import: round-trips as a new factory');
  ok(!store.importFactory('{nope').ok, 'import: bad JSON is refused');
  ok(!store.importFactory({ format: 'something-else', factory: {} }).ok, 'import: a different format is refused');
  ok(!store.importFactory({ format: 'crundi-factory', formatVersion: 99, factory: standard() }).ok, 'import: a newer format version is refused');
  ok(store.importFactory(standard()).validation.ok, 'import: a bare definition is accepted');

  const del = store.deleteFactory(d.factory.id);
  ok(del.ok && store.getFactory(d.factory.id) === null, 'delete: the factory is gone from the library');
  ok(existsSync(join(dir, 'factories', '.trash')) && readdirSync(join(dir, 'factories', '.trash')).some(n => n.startsWith(d.factory.id)), 'delete: ...but kept in the trash');
  ok(!store.deleteFactory('nope').ok, 'delete: an unknown id is refused');
}

rmSync(dir, { recursive: true, force: true });
console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
