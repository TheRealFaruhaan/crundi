#!/usr/bin/env node
/**
 * test-factory-templates.mjs — the factories and personas Crundi provides.
 *
 * Covers: every template validates with no errors or warnings; every one
 * can be walked by the engine from the change request to Finish (taking the
 * approving answers); each has its own layout; skills are kept only when the
 * machine has them; the database engineer persona exists and every persona
 * that builds has skills that fit.
 *
 * Run: node scripts/test-factory-templates.mjs
 */
import { factoryTemplates } from '../src/factory-templates.js';
import { PERSONAS, rosterFromPersona } from '../src/factory-personas.js';
import * as M from '../src/factory-model.js';
import * as E from '../src/factory-engine.js';

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };

const all = factoryTemplates();
ok(all.length >= 5 && new Set(all.map(t => t.id)).size === all.length, 'templates: five or more, unique ids');

// Walk a run to the end: approve every gate, pick every part, reply with the first reply.
function walk(f) {
  let run = E.start(E.createRun({ id: 'r', project: 'p', factory: f, task: 'Build it' })).run;
  for (let i = 0; i < 80; i++) {
    if (run.status === 'finishing') return true;
    const t = run.tokens.find(x => x.status === 'work' || x.status === 'answer' || x.status === 'ask');
    if (!t) return false;
    if (t.status === 'ask') { const n = f.nodes.find(x => x.id === t.nodeId); const res = E.askReply(run, t.id, n.ask.replies[0].id); if (!res.ok) throw new Error(res.error); run = res.run; continue; }
    const req = E.requirements(run, t.id);
    const answers = {};
    let item = req.route;
    while (item && item.decision) {
      const d = item.decision;
      const v = d.kind === 'yesno' ? 'yes' : d.kind === 'many' ? d.branches.filter(b => b.answer !== 'none').map(b => b.answer) : d.branches[0].answer;
      answers[d.id] = v;
      const b = d.branches.find(x => x.answer === (Array.isArray(v) ? v[0] : v));
      item = b && b.next;
    }
    const isMgr = E.personOf(run, t) === 'manager' && t.status === 'work';
    const payload = { summary: 'done', answers };
    if (isMgr && run.handoffs > 0) payload.finish = true;
    const res = E.handoff(run, t.id, payload);
    if (!res.ok) throw new Error(f.name + ': ' + res.error);
    run = res.run;
  }
  return false;
}

for (const t of all) {
  const v = M.validateFactory(t);
  ok(v.ok && !v.warnings.length, `${t.name}: valid, no warnings` + (v.ok ? '' : ' — ' + v.errors.map(e => e.message).join('; ')) + (v.warnings.length ? ' — ' + v.warnings.map(w => w.message).join('; ') : ''));
  let done = false;
  try { done = walk(t); } catch (err) { console.log('   ', err.message); }
  ok(done, `${t.name}: runs from the change request to Finish`);
  ok(!M.needsLayout(t), `${t.name}: laid out`);
  ok(t.goodFor && t.description, `${t.name}: says what it is good for`);
}

const none = factoryTemplates({ available: new Set() });
ok(none.every(t => t.roster.every(r => !r.skills.length)), 'skills: none kept when the machine has none');
const some = factoryTemplates({ available: new Set(['crundi']) });
ok(some.find(t => t.id === 'tpl_squad').roster.find(r => r.id === 'fe').skills.join() === 'crundi', 'skills: only the installed ones are kept');

const db = PERSONAS.find(p => p.key === 'database');
ok(db && /migration/i.test(db.persona) && db.skills.length, 'personas: a database engineer, with migrations in the brief and skills');
for (const key of ['frontend', 'backend', 'database', 'fullstack', 'designer', 'devops', 'qa', 'ui-reviewer', 'researcher']) {
  ok(PERSONAS.find(p => p.key === key)?.skills.length > 0, `personas: ${key} has skills that fit`);
}
const r = rosterFromPersona('reviewer', { id: 'x' });
ok(r.tools.deny.includes('Edit') && r.memory === 'fresh', 'personas: reviewers are read-only with fresh eyes');

console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
