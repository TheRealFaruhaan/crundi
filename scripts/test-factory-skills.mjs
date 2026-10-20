#!/usr/bin/env node
/**
 * test-factory-skills.mjs — skills in factories (src/factory-skills.js and
 * how the runner uses it).
 *
 * Covers: telling /skill-name mentions from paths and URLs; finding every
 * mention in a factory (persona, pinned, question, Ask me, note label);
 * unknown skills as editor warnings; the project's own skills linked into the
 * run and branch worktrees and never committed, whether or not the project
 * commits a skills folder; the start check refusing a factory that names
 * skills the project lacks unless started anyway; skill lines in the system
 * prompt and the step message; and the Skill tool pre-allowed.
 *
 * Run: node scripts/test-factory-skills.mjs
 */

import { mkdtempSync, rmSync, writeFileSync, mkdirSync, existsSync, lstatSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';

const base = mkdtempSync(join(tmpdir(), 'factory-skills-'));
process.env.DATA_DIR = join(base, 'data');
process.env.FACTORY_WORKTREES_DIR = join(base, 'worktrees');
const S = await import('../src/factory-skills.js');
const M = await import('../src/factory-model.js');
const Git = await import('../src/factory-git.js');
const { createFactoryRunner } = await import('../src/factory-runner.js');
const { standard, fakeTeam } = await import('./factory-fixtures.mjs');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
async function until(fn, ms = 6000) { const end = Date.now() + ms; while (Date.now() < end) { const v = await fn(); if (v) return v; await sleep(15); } return null; }
const sh = (a, cwd) => execFileSync('git', a, { cwd, encoding: 'utf-8' }).trim();

// ─── Mentions ───
const eq = (a, b) => JSON.stringify(a) === JSON.stringify(b);
ok(eq(S.findMentions('Use /frontend-design, then /run.'), ['frontend-design', 'run']), 'mentions: names in a sentence, punctuation after');
ok(eq(S.findMentions('try /engineering:code-review.'), ['engineering:code-review']), 'mentions: plugin skills');
ok(eq(S.findMentions('see /home/crundi/x, src/a.js, https://x.io/a, /api/x and run /tests.js'), []), 'mentions: paths, files and URLs are not skills');
ok(eq(S.findMentions('/run first, and /run again'), ['run']), 'mentions: at the start, without repeats');

const f = standard();
f.roster.find(r => r.id === 'fe').persona = 'Build pages with /frontend-design.';
f.roster.find(r => r.id === 'uir').skills = ['code-review'];
f.nodes.find(n => n.id === 'dSec').decision.question = 'Is a /security-review needed?';
f.edges.find(e => e.id === 'e22').noteLabel = 'What to change (see /style-guide)';
const found = S.mentionsInFactory(M.normalizeFactory(f));
ok(['frontend-design', 'code-review', 'security-review', 'style-guide'].every(n => found.has(n)), 'factory: mentions in a persona, pinned, a question and a note label are all found');
ok(found.get('frontend-design')[0].employeeId === 'fe' && found.get('security-review')[0].nodeId === 'dSec', 'factory: each says where it is');
const v = M.validateFactory(f, { skills: new Set(['frontend-design', 'code-review']) });
ok(v.ok && v.warnings.filter(w => w.code === 'skill-unknown').map(w => w.skill).sort().join() === 'security-review,style-guide', 'validate: unknown skills are warnings, not errors');
ok(!M.validateFactory(f).warnings.some(w => w.code === 'skill-unknown'), 'validate: without a skill list, no skill warnings');
ok(eq(S.missingSkills(M.normalizeFactory(f), 'and use /seo', new Set(['frontend-design', 'code-review', 'security-review'])).sort(), ['seo', 'style-guide']), 'missing: from the factory and the task');

// ─── Linking project skills into worktrees ───
function repo(name, { commitSkills = false } = {}) {
  const dir = join(base, 'projects', name);
  mkdirSync(join(dir, '.claude', 'skills', 'house-style'), { recursive: true });
  writeFileSync(join(dir, '.claude', 'skills', 'house-style', 'SKILL.md'), '---\nname: house-style\ndescription: Our house style.\n---\nBe terse.\n');
  writeFileSync(join(dir, 'a.txt'), 'a\n');
  const g = (a) => execFileSync('git', a, { cwd: dir });
  g(['init', '-q', '-b', 'dev']);
  if (commitSkills) {
    mkdirSync(join(dir, '.claude', 'skills', 'shared'), { recursive: true });
    writeFileSync(join(dir, '.claude', 'skills', 'shared', 'SKILL.md'), '---\nname: shared\ndescription: Committed.\n---\n');
    g(['add', 'a.txt', '.claude/skills/shared']);
  } else g(['add', 'a.txt']);
  g(['-c', 'user.name=t', '-c', 'user.email=t@t', 'commit', '-q', '-m', 'i']);
  return dir;
}
{
  const r1 = repo('link1');
  const wt = await Git.createRunWorktree({ repo: r1, project: 'link1', runId: 'run_a1', name: 'x' });
  const ex = Git.linkProjectSkills(r1, wt.path);
  ok(eq(ex, ['.claude/skills']) && lstatSync(join(wt.path, '.claude', 'skills')).isSymbolicLink() && existsSync(join(wt.path, '.claude', 'skills', 'house-style', 'SKILL.md')), 'link: an uncommitted skills folder is linked into the worktree');
  writeFileSync(join(wt.path, 'b.txt'), 'b\n');
  const c = await Git.commitAll(wt.path, 'step', { exclude: ex });
  const files = sh(['ls-files'], wt.path).split('\n');
  ok(c.committed && files.includes('b.txt') && !files.some(x => x.startsWith('.claude')), 'link: the work is committed, the link never is');

  const r2 = repo('link2', { commitSkills: true });
  const wt2 = await Git.createRunWorktree({ repo: r2, project: 'link2', runId: 'run_a2', name: 'x' });
  const ex2 = Git.linkProjectSkills(r2, wt2.path);
  ok(eq(ex2, ['.claude/skills/house-style']) && existsSync(join(wt2.path, '.claude', 'skills', 'shared', 'SKILL.md')), 'link: with a committed skills folder, only the missing skills are linked in');
  writeFileSync(join(wt2.path, '.claude', 'skills', 'shared', 'SKILL.md'), '---\nname: shared\ndescription: Edited.\n---\n');
  await Git.commitAll(wt2.path, 'step', { exclude: ex2 });
  const files2 = sh(['ls-files'], wt2.path).split('\n');
  ok(files2.includes('.claude/skills/shared/SKILL.md') && !files2.some(x => x.includes('house-style')), 'link: edits to committed skills are kept; the linked ones are left out');
  ok(eq(Git.linkProjectSkills(join(base, 'nowhere'), wt.path), []), 'link: a project without skills links nothing');
}

// ─── The runner ───
{
  const projectSkills = [{ name: 'house-style', description: 'Our house style.' }, { name: 'frontend-design', description: 'Distinctive interfaces.' }];
  const team = fakeTeam({
    manager: async (msg) => msg.includes('answers.dPlan') ? { payload: { summary: 'ui', answers: { dPlan: 'no', dWho: ['ui', 'backend'] } } } : null,
    fe: async () => null, be: async () => null,
  });
  const fac = standard();
  fac.roster.find(r => r.id === 'fe').persona = 'Build pages with /frontend-design.';
  fac.roster.find(r => r.id === 'be').skills = ['house-style'];
  fac.roster.find(r => r.id === 'uir').persona = 'Review with /code-review.';
  const dir = repo('run1');
  const r = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: dir }), getFactory: () => ({ factory: fac }), listSkills: () => projectSkills, settleMs: 10_000 });
  team.setRunner(r);
  const refused = await r.startRun({ project: 'run1', factoryId: 'x', task: 'Follow /house-style' });
  ok(!refused.ok && eq(refused.missingSkills, ['code-review']) && /\/code-review/.test(refused.error), 'start: a skill the project lacks is named, and the run is not started');
  ok(r.listRuns().length === 0, 'start: nothing is left behind');
  const st = await r.startRun({ project: 'run1', factoryId: 'x', task: 'Follow /house-style', ignoreMissingSkills: true });
  ok(st.ok, 'start: "start anyway" starts it');
  await until(() => team.of('fe')[0] && team.of('be')[0]);
  const fe = team.of('fe')[0], be = team.of('be')[0], mgr = team.of('manager')[0];
  ok(/\/frontend-design: Distinctive interfaces\./.test(fe.systemPromptExtra), 'prompt: a skill named in the persona is in the system prompt, with its description');
  ok(/\/house-style: Our house style\./.test(be.systemPromptExtra), 'prompt: a pinned skill is in the system prompt');
  ok(/Skills to use for this/.test(fe.msgs[0]) && /\/house-style/.test(fe.msgs[0]), 'step: a skill named in the task is in every step message');
  ok(fe.allowedTools.includes('Skill') && mgr.allowedTools.includes('Skill'), 'the Skill tool never asks');
  const ws = (fe.msgs[0].match(/^Work in: (.+)$/m) || [])[1];
  ok(ws && existsSync(join(ws, '.claude', 'skills', 'house-style', 'SKILL.md')), 'branch worktrees get the project\'s skills too');
  await r.stopRun(st.run.id);
  r.dispose();
}

rmSync(base, { recursive: true, force: true });
console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
