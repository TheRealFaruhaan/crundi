#!/usr/bin/env node
/**
 * test-factory-cr.mjs — the change request a run starts from.
 *
 * Covers: the default template and a factory's own, cleaned; a CR checked
 * against its template (title and required sections, lists needing an item);
 * the CR as markdown (lists as checklists, attachments by path) and as the
 * brief every step carries; the runner writing CHANGE-REQUEST.md into the
 * shared folder with copies of the attachments; attachments only from the
 * project's upload folder; the Manager's first step getting the whole CR and
 * an employee the brief; the CR surviving a replay of the journal.
 *
 * Run: node scripts/test-factory-cr.mjs
 */
import { mkdtempSync, rmSync, writeFileSync, mkdirSync, existsSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';

const base = mkdtempSync(join(tmpdir(), 'factory-cr-'));
process.env.DATA_DIR = join(base, 'data');
process.env.FACTORY_WORKTREES_DIR = join(base, 'worktrees');
const CR = await import('../src/factory-cr.js');
const { normalizeFactory } = await import('../src/factory-model.js');
const { createFactoryRunner } = await import('../src/factory-runner.js');
const { standard, fakeTeam } = await import('./factory-fixtures.mjs');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
async function until(fn, ms = 6000) { const end = Date.now() + ms; while (Date.now() < end) { const v = await fn(); if (v) return v; await sleep(15); } return null; }

// ─── Template ───
const T = CR.normalizeTemplate(undefined);
ok(T.length === CR.DEFAULT_CR_TEMPLATE.length && T.find(s => s.id === 'done').list && T.find(s => s.id === 'goal').required, 'template: the default when none is set');
ok(normalizeFactory(standard()).settings.crTemplate.length === T.length, 'model: every factory has a CR template');
const own = CR.normalizeTemplate([{ label: 'Bug' }, { label: 'Bug', required: 1 }, { label: '' }, { id: 'title', label: 'Title again' }, 'junk']);
ok(own.length === 3 && own[0].id === 'bug' && own[1].id === 'bug-2' && own[1].required === true && own[2].id !== 'title', 'template: cleaned (ids unique, never "title", empty labels dropped)');

// ─── Checking ───
ok(!CR.checkCr({ title: ' ' }, T).ok && CR.checkCr({ title: ' ' }, T).field === 'title', 'check: a title is required');
const miss = CR.checkCr({ title: 'Dark mode', sections: { goal: 'Night use', expected: 'A toggle' } }, T);
ok(!miss.ok && miss.field === 'done' && /Done when/.test(miss.error), 'check: a required section is named when missing');
ok(!CR.checkCr({ title: 'x', sections: { goal: 'g', expected: 'e', done: '  -  \n * ' } }, T).ok, 'check: a list needs at least one real item');
const good = CR.checkCr({ title: '  Dark   mode ', sections: { goal: 'Night use', expected: 'A toggle in settings', done: '- Toggle persists\n2. Follows the system\n[ ] Contrast 4.5:1', notes: '', junk: 'x' }, attachments: [{ path: '/a/b/shot.png' }] }, T);
ok(good.ok && good.cr.title === 'Dark mode' && !('notes' in good.cr.sections) && !('junk' in good.cr.sections) && good.cr.attachments[0].name === 'shot.png', 'check: cleaned, empty and unknown sections dropped, attachments named');

// ─── Markdown and brief ───
const md = CR.crMarkdown(good.cr, T);
ok(/^# Change request: Dark mode/.test(md) && /## Done when\n- \[ \] Toggle persists\n- \[ \] Follows the system\n- \[ \] Contrast 4\.5:1/.test(md), 'markdown: lists become checklists, whatever bullets were typed');
ok(/## Attachments[\s\S]*shot\.png: \/a\/b\/shot\.png/.test(md) && !/## Current situation/.test(md), 'markdown: attachments by path; empty sections left out');
const brief = CR.crBrief(good.cr, T, '/run/shared/CHANGE-REQUEST.md');
ok(/^Dark mode\nGoal: Night use\nDone when:\n- Toggle persists/.test(brief) && /with 1 attachment, is in \/run\/shared\/CHANGE-REQUEST\.md/.test(brief) && !/A toggle in settings/.test(brief), 'brief: title, goal, done checks and the file, not the rest');

// ─── The runner ───
const proj = join(base, 'proj');
mkdirSync(join(proj, 'crundi_attachments'), { recursive: true });
const g = (a) => execFileSync('git', ['-c', 'user.name=t', '-c', 'user.email=t@t', ...a], { cwd: proj });
g(['init', '-q', '-b', 'dev']); writeFileSync(join(proj, '.gitignore'), 'crundi_attachments/\n'); writeFileSync(join(proj, 'a.txt'), 'a'); g(['add', '.']); g(['commit', '-qm', 'i']);
const shot = join(proj, 'crundi_attachments', '123-shot.png');
writeFileSync(shot, 'PNGDATA');
writeFileSync(join(base, 'secret.txt'), 'nope');

const f = standard(); f.settings = { presets: [] };
const team = fakeTeam({
  manager: async (msg) => msg.includes('answers.dPlan') ? { payload: { summary: 'go', answers: { dPlan: 'no', dWho: ['ui'] } } } : null,
  fe: async () => null,
});
const opts = { dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: proj }), getFactory: () => ({ factory: f }), settleMs: 10_000, permissionBatchMs: 30 };
let r = createFactoryRunner(opts);
team.setRunner(r);
const cr = { title: 'Dark mode', sections: { goal: 'Night use', expected: 'A toggle in settings', done: 'Toggle persists\nFollows the system' } };

const bad1 = await r.startRun({ project: 'p', factoryId: 'x', cr: { ...cr, sections: { goal: 'g' } } });
ok(!bad1.ok && bad1.field === 'expected', 'start: refused with the field to fill in');
const bad2 = await r.startRun({ project: 'p', factoryId: 'x', cr: { ...cr, attachments: [{ path: join(base, 'secret.txt'), name: 'secret.txt' }] } });
ok(!bad2.ok && bad2.field === 'attachments' && /not an uploaded attachment/.test(bad2.error), 'start: an attachment from outside the upload folder is refused');
const bad3 = await r.startRun({ project: 'p', factoryId: 'x', cr: { ...cr, attachments: [{ path: join(proj, 'crundi_attachments', '..', '..', 'secret.txt'), name: 'x' }] } });
ok(!bad3.ok && bad3.field === 'attachments', 'start: no climbing out of the upload folder');

const st = await r.startRun({ project: 'p', factoryId: 'x', cr: { ...cr, attachments: [{ path: shot, name: 'shot.png' }] } });
ok(st.ok && st.run.task === 'Dark mode', 'start: the title is the run\'s task line');
const run = r.getRun(st.run.id);
ok(run.cr && run.cr.sections[0].label === 'Goal' && run.cr.sections[0].text === 'Night use' && run.cr.attachments[0].name === 'shot.png', 'summary: the CR with section labels, for the panel');
ok(existsSync(run.cr.file) && /# Change request: Dark mode/.test(readFileSync(run.cr.file, 'utf-8')), 'start: CHANGE-REQUEST.md is in the shared folder');
const copied = readFileSync(run.cr.file, 'utf-8').match(/shot\.png: (\S+)/)[1];
ok(copied !== shot && existsSync(copied) && readFileSync(copied, 'utf-8') === 'PNGDATA' && copied.includes('shared'), 'start: the attachment is copied next to it');

await until(() => team.of('manager')[0]?.msgs.length);
const m0 = team.of('manager')[0].msgs[0];
ok(/The change request this run is for:/.test(m0) && /## Expected result\nA toggle in settings/.test(m0) && /- \[ \] Toggle persists/.test(m0) && /shot\.png/.test(m0), 'Manager: its first step has the whole CR, attachments included');
ok(/change request \(CR\)/.test(team.of('manager')[0].systemPromptExtra) && /ask the person here before handing anything out/.test(team.of('manager')[0].systemPromptExtra), 'Manager: told to ask when the CR is unclear');
await until(() => team.of('fe')[0]?.msgs.length);
const e0 = team.of('fe')[0].msgs[0];
ok(/The change request:\n  Dark mode\n  Goal: Night use/.test(e0) && /CHANGE-REQUEST\.md/.test(e0) && !/A toggle in settings/.test(e0), 'employee: the brief and where the full CR is');
ok(/Done when/.test(team.of('fe')[0].systemPromptExtra), 'employee: told to hold reviews to the done checks');

// The journal keeps it.
r.dispose();
r = createFactoryRunner(opts);
team.setRunner(r);
r.load();
ok(r.getRun(st.run.id)?.cr?.title === 'Dark mode', 'replay: the CR survives a restart');
await r.stopRun(st.run.id);
r.dispose();

// A plain task still works (older callers).
{
  const r2 = createFactoryRunner(opts); team.setRunner(r2);
  const p = await r2.startRun({ project: 'p', factoryId: 'x', task: 'Just a task' });
  ok(p.ok && p.run.cr === null && p.run.task === 'Just a task', 'start: a plain task still works');
  await r2.stopRun(p.run.id); r2.dispose();
}

rmSync(base, { recursive: true, force: true });
console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
