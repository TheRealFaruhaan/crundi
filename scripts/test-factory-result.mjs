#!/usr/bin/env node
/**
 * test-factory-result.mjs — how a factory run ends.
 *
 * Covers, on real git: the squash merge (one commit on the parent with the
 * given message, the person's checkout updated without losing anything,
 * refusing when uncommitted changes are in the way, reporting conflicts,
 * moving a branch that is not checked out) and undo (the parent goes back,
 * the work is kept on the run's branch, refusing when the parent moved on).
 * Through the runner: the result card in the Manager's chat; Merge, Undo,
 * Request changes (back to the Manager with the note, card updated in place
 * when it finishes again), Keep branch, Discard; auto-merge and its fallback
 * to asking after a conflict; preview services removed with the run; and the
 * close guard on a running factory's chats.
 *
 * Run: node scripts/test-factory-result.mjs
 */

import { mkdtempSync, rmSync, writeFileSync, mkdirSync, readFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';

const base = mkdtempSync(join(tmpdir(), 'factory-result-'));
process.env.DATA_DIR = join(base, 'data');
process.env.FACTORY_WORKTREES_DIR = join(base, 'worktrees');
const Git = await import('../src/factory-git.js');
const { createFactoryRunner } = await import('../src/factory-runner.js');
const { fakeTeam } = await import('./factory-fixtures.mjs');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
async function until(fn, ms = 6000) { const end = Date.now() + ms; while (Date.now() < end) { const v = await fn(); if (v) return v; await sleep(15); } return null; }
const sh = (a, cwd) => execFileSync('git', a, { cwd, encoding: 'utf-8' }).trim();
const ID = ['-c', 'user.name=Person', '-c', 'user.email=p@p'];
function repo(name) {
  const dir = join(base, 'projects', name);
  mkdirSync(dir, { recursive: true });
  sh(['init', '-q', '-b', 'dev'], dir);
  sh(['config', 'user.name', 'Person'], dir); sh(['config', 'user.email', 'p@p'], dir);
  writeFileSync(join(dir, 'a.txt'), 'a\n');
  sh(['add', '.'], dir); sh(['commit', '-q', '-m', 'init'], dir);
  return dir;
}

// ─── Squash merge and undo on real git ───
{
  const dir = repo('sq');
  const wt = await Git.createRunWorktree({ repo: dir, project: 'sq', runId: 'run_s1', name: 'x' });
  writeFileSync(join(wt.path, 'b.txt'), 'b\n'); await Git.commitAll(wt.path, 'one');
  writeFileSync(join(wt.path, 'c.txt'), 'c\n'); await Git.commitAll(wt.path, 'two');
  const d = await Git.diffSummary(wt.path, wt.baseCommit);
  ok(d.files.length === 2 && d.added === 2, 'diff: files and line counts against the start');

  // Uncommitted change to a file the merge brings: refused, nothing moves.
  writeFileSync(join(dir, 'b.txt'), 'mine\n');
  const blocked = await Git.squashMerge({ repo: dir, parentBranch: 'dev', branch: wt.branch, message: 'Add b and c' });
  ok(!blocked.ok && blocked.blocked && readFileSync(join(dir, 'b.txt'), 'utf-8') === 'mine\n' && sh(['rev-parse', 'dev'], dir) === wt.baseCommit, 'squash: refused when the checkout\'s uncommitted changes are in the way, and nothing is lost');
  rmSync(join(dir, 'b.txt'));

  writeFileSync(join(dir, 'notes.md'), 'unrelated local work\n');
  const m = await Git.squashMerge({ repo: dir, parentBranch: 'dev', branch: wt.branch, message: 'Add b and c\n\nBody' });
  ok(m.ok && sh(['rev-list', '--count', `${wt.baseCommit}..dev`], dir) === '1', 'squash: exactly one new commit on the parent');
  ok(sh(['log', '-1', '--format=%s|%an', 'dev'], dir) === 'Add b and c|Person', 'squash: with the given message, as the repository\'s own identity');
  ok(existsSync(join(dir, 'b.txt')) && existsSync(join(dir, 'c.txt')) && readFileSync(join(dir, 'notes.md'), 'utf-8') === 'unrelated local work\n', 'squash: the checkout gets the files; unrelated local work is untouched');

  // As the runner does: after a merge the run's worktree and branch are cleaned up first.
  await Git.removeWorktree({ repo: dir, path: wt.path, branch: wt.branch });
  const u = await Git.undoMerge({ repo: dir, parentBranch: 'dev', before: m.before, after: m.after, restoreBranch: wt.branch });
  ok(u.ok && sh(['rev-parse', 'dev'], dir) === m.before && !existsSync(join(dir, 'b.txt')), 'undo: the parent and the checkout are back where they were');
  ok(sh(['rev-parse', wt.branch], dir) === m.after && existsSync(join(dir, 'notes.md')), 'undo: the work is kept on the run\'s branch; local work survives');

  const m2 = await Git.squashMerge({ repo: dir, parentBranch: 'dev', branch: wt.branch, message: 'again' });
  writeFileSync(join(dir, 'z.txt'), 'z\n'); sh(['add', 'z.txt'], dir); sh(['commit', '-q', '-m', 'later'], dir);
  const u2 = await Git.undoMerge({ repo: dir, parentBranch: 'dev', before: m2.before, after: m2.after });
  ok(!u2.ok && u2.moved && /git revert/.test(u2.error), 'undo: refused once something else is committed on top, saying how to revert');
}
{
  const dir = repo('conf');
  const wt = await Git.createRunWorktree({ repo: dir, project: 'conf', runId: 'run_c1', name: 'x' });
  writeFileSync(join(wt.path, 'a.txt'), 'factory\n'); await Git.commitAll(wt.path, 'f');
  writeFileSync(join(dir, 'a.txt'), 'person\n'); sh(['commit', '-qam', 'p'], dir);
  const c = await Git.squashMerge({ repo: dir, parentBranch: 'dev', branch: wt.branch, message: 'x' });
  ok(!c.ok && c.conflict && c.files.includes('a.txt') && readFileSync(join(dir, 'a.txt'), 'utf-8') === 'person\n', 'squash: a conflict is reported with its files, and nothing is touched');
}
{
  const dir = repo('away');
  const wt = await Git.createRunWorktree({ repo: dir, project: 'away', runId: 'run_a1', name: 'x' });
  writeFileSync(join(wt.path, 'b.txt'), 'b\n'); await Git.commitAll(wt.path, 'b');
  sh(['checkout', '-q', '-b', 'other'], dir);
  const m = await Git.squashMerge({ repo: dir, parentBranch: 'dev', branch: wt.branch, message: 'b' });
  ok(m.ok && sh(['rev-parse', 'dev'], dir) === m.after && !existsSync(join(dir, 'b.txt')), 'squash: a parent that is not checked out is moved without touching the checkout');
}

// ─── Through the runner ───

function solo(settings = {}) {
  return {
    name: 'Solo', settings,
    roster: [{ id: 'manager', name: 'Manager', persona: 'm' }, { id: 'dev', name: 'Developer', persona: 'd' }],
    nodes: [{ id: 'start', type: 'start' }, { id: 'mgr', type: 'manager' }, { id: 'end', type: 'end' }, { id: 'dev', type: 'employee', employeeId: 'dev', label: 'Developer' }],
    edges: [{ source: 'start', target: 'mgr' }, { source: 'mgr', sourceHandle: 'finish', target: 'end' }, { source: 'mgr', target: 'dev' }, { source: 'dev', target: 'mgr', maxRounds: 5 }],
  };
}
const workIn = (msg) => (msg.match(/^Work in: (.+)$/m) || [])[1];
function teamFor({ onChanges } = {}) {
  let rounds = 0;
  return fakeTeam({
    manager: async (msg) => {
      if (/asked for changes/.test(msg)) { onChanges && onChanges(msg); return { payload: { summary: 'Making the change' } }; }
      if (/^\[Factory\] Your step: Developer/m.test(msg)) return null;
      if (/conflicts in/.test(msg)) return { payload: { summary: 'Tried again. Add the greeting.', finish: true } };
      if (msg.includes('[Factory] Your step')) {
        rounds++;
        return rounds === 1 ? { payload: { summary: 'Over to the developer' } } : { payload: { summary: 'Add the greeting. It says hello.', finish: true } };
      }
      return null;
    },
    dev: async (msg) => { writeFileSync(join(workIn(msg), 'hello.txt'), 'hello ' + Date.now() + '\n'); return { payload: { summary: 'Wrote hello.txt' } }; },
  });
}
function runnerFor(team, dir, f, extra = {}) {
  const r = createFactoryRunner({ dataDir: process.env.DATA_DIR, agents: team.agents, getProject: () => ({ path: dir }), getFactory: () => ({ factory: f }), settleMs: 10_000, ...extra });
  team.setRunner(r);
  return r;
}
const cardOf = (team) => team.of('manager')[0]?.cards?.[0];

{
  const dir = repo('merge');
  const removed = [];
  const team = teamFor();
  const r = runnerFor(team, dir, solo(), { services: { under: (root) => (root ? ['svc:preview'] : []), remove: (k) => removed.push(k) } });
  const st = await r.startRun({ project: 'merge', factoryId: 'x', task: 'Say hello' });
  const mgrChat = team.of('manager')[0].id;
  ok(r.guardsChat(mgrChat) && r.guardsChat(mgrChat).id === st.run.id, 'guard: the Manager\'s chat is guarded while the run goes');
  const card = await until(() => cardOf(team));
  ok(card && card.state === 'ask' && card.diff.count === 1 && card.diff.files[0].file === 'hello.txt' && /greeting/.test(card.summary), 'card: the result, with the Manager\'s summary and the diff, waits for a choice');
  ok(r.getRun(st.run.id).status === 'finishing' && r.activeRunFor('merge'), 'card: while it waits, the run still holds the project');
  const diff = await r.runDiff(st.run.id);
  ok(diff.ok && /hello\.txt/.test(diff.diff), 'diff: the full diff is available');

  const res = await r.finishAction(st.run.id, 'merge');
  ok(res.ok && r.getRun(st.run.id).status === 'finished' && !r.activeRunFor('merge'), 'merge: the run is finished and the project is free');
  ok(sh(['log', '-1', '--format=%s', 'dev'], dir) === 'Add the greeting' && /Made by the Crundi factory "Solo"/.test(sh(['log', '-1', '--format=%b', 'dev'], dir)), 'merge: one commit on dev, its subject from the Manager\'s summary');
  ok(existsSync(join(dir, 'hello.txt')), 'merge: the person\'s checkout has the result');
  ok(!existsSync(st.run.worktree) && !sh(['branch', '--list', st.run.branch], dir), 'merge: the worktree and the branch are cleaned up');
  ok(!existsSync(join(st.run.worktree, '..')), 'merge: the run\'s own folder goes too');
  ok(removed.includes('svc:preview'), 'merge: preview services started from the run are removed');
  ok(cardOf(team).state === 'merged' && cardOf(team).commit, 'merge: the same card now says it was merged, with the commit');
  ok(!r.guardsChat(mgrChat), 'guard: once finished, the Manager\'s chat can be closed');
  ok(!(await r.finishAction(st.run.id, 'merge')).ok, 'merge: a finished run cannot be merged twice');

  const un = await r.finishAction(st.run.id, 'undo');
  ok(un.ok && !existsSync(join(dir, 'hello.txt')) && sh(['branch', '--list', st.run.branch], dir).includes(st.run.branch.slice(8)), 'undo: dev goes back, and the work is on the run\'s branch again');
  ok(cardOf(team).state === 'undone', 'undo: the card says so');
  r.dispose();
}
{
  const dir = repo('changes');
  let told = '';
  const team = teamFor({ onChanges: (m) => { told = m; } });
  const r = runnerFor(team, dir, solo());
  const st = await r.startRun({ project: 'changes', factoryId: 'x', task: 'Say hello' });
  await until(() => cardOf(team));
  ok(!(await r.finishAction(st.run.id, 'changes', { note: ' ' })).ok, 'changes: a note is required');
  const res = await r.finishAction(st.run.id, 'changes', { note: 'Make it say hi instead' });
  ok(res.ok && await until(() => /Make it say hi instead/.test(told)), 'changes: the Manager gets the person\'s note');
  ok(cardOf(team).state === 'changes' || cardOf(team).state === 'ask', 'changes: the card shows the team is on it');
  const again = await until(() => r.getRun(st.run.id).status === 'finishing' && cardOf(team).state === 'ask');
  ok(again && team.of('manager')[0].cards.length === 1, 'changes: when the Manager finishes again, the same card asks again (updated in place)');
  const keep = await r.finishAction(st.run.id, 'keep');
  ok(keep.ok && r.getRun(st.run.id).status === 'finished' && !existsSync(st.run.worktree) && sh(['branch', '--list', st.run.branch], dir), 'keep: the worktree goes, the branch stays, nothing is merged');
  ok(!existsSync(join(dir, 'hello.txt')) && cardOf(team).state === 'kept', 'keep: dev is untouched and the card says kept');
  r.dispose();
}
{
  const dir = repo('discard');
  const team = teamFor();
  const r = runnerFor(team, dir, solo());
  const st = await r.startRun({ project: 'discard', factoryId: 'x', task: 'Say hello' });
  await until(() => cardOf(team));
  const res = await r.finishAction(st.run.id, 'discard');
  ok(res.ok && !existsSync(st.run.worktree) && !sh(['branch', '--list', st.run.branch], dir) && cardOf(team).state === 'discarded', 'discard: worktree and branch are gone');
  r.dispose();
}
{
  const dir = repo('auto');
  const team = teamFor();
  const r = runnerFor(team, dir, solo({ mergeMode: 'auto' }));
  const st = await r.startRun({ project: 'auto', factoryId: 'x', task: 'Say hello' });
  const card = await until(() => cardOf(team));
  ok(card && card.state === 'merged' && card.auto && existsSync(join(dir, 'hello.txt')) && r.getRun(st.run.id).status === 'finished', 'auto: merged without asking, the card says so, with Undo');
  r.dispose();
}
{
  // Auto mode, but dev moved on with a conflicting change: the Manager is sent
  // back once; when it still conflicts the card asks the person.
  const dir = repo('autoconf');
  writeFileSync(join(dir, 'hello.txt'), 'base\n'); sh(['add', '.'], dir); sh(['commit', '-q', '-m', 'base'], dir);
  let conflictMsg = '';
  const team = teamFor();
  const orig = team.agents.send;
  team.agents.send = (id, text) => { if (/conflicts in/.test(text)) conflictMsg = text; return orig(id, text); };
  const r = runnerFor(team, dir, solo({ mergeMode: 'auto' }));
  const st = await r.startRun({ project: 'autoconf', factoryId: 'x', task: 'Say hello' });
  await until(() => team.of('dev')[0]?.results?.length);
  writeFileSync(join(dir, 'hello.txt'), 'the person changed this\n'); sh(['commit', '-qam', 'person'], dir);
  const card = await until(() => cardOf(team)?.state === 'ask' && cardOf(team));
  ok(/merge dev into your branch/.test(conflictMsg), 'auto conflict: the Manager is told to bring dev in and resolve it');
  ok(card && /gave up/.test(card.error) && r.getRun(st.run.id).status === 'finishing', 'auto conflict: still conflicting, the person is asked');
  ok(readFileSync(join(dir, 'hello.txt'), 'utf-8') === 'the person changed this\n', 'auto conflict: dev is untouched');
  await r.stopRun(st.run.id);
  r.dispose();
}

rmSync(base, { recursive: true, force: true });
console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
