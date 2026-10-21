#!/usr/bin/env node
/**
 * test-factory-cleanup.mjs — leftovers of factory runs, and the phone's live
 * card for a run.
 *
 * Covers: a stopped run's worktree found with its size, age and uncommitted
 * changes; a folder no run knows about; a branch kept with "Keep branch";
 * runs still going left alone; removal confined to the worktrees root and to
 * factory/* branches, unregistering the worktrees from their repository;
 * auto-delete sparing anything with uncommitted work, kept branches and
 * anything too young; and what the live card says in each state of a run.
 *
 * Run: node scripts/test-factory-cleanup.mjs
 */
import { mkdtempSync, rmSync, writeFileSync, mkdirSync, existsSync, utimesSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';

const base = mkdtempSync(join(tmpdir(), 'factory-cleanup-'));
process.env.DATA_DIR = join(base, 'data');
process.env.FACTORY_WORKTREES_DIR = join(base, 'wt');
mkdirSync(process.env.DATA_DIR, { recursive: true });
const C = await import('../src/factory-cleanup.js');
const Git = await import('../src/factory-git.js');
const { factoryLiveCard } = await import('../src/factory-runner.js');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const sh = (a, cwd) => execFileSync('git', a, { cwd, encoding: 'utf-8' }).trim();
const root = process.env.FACTORY_WORKTREES_DIR;

const repo = join(base, 'proj'); mkdirSync(repo);
sh(['init', '-q', '-b', 'dev'], repo); writeFileSync(join(repo, 'a'), 'a'); sh(['add', '.'], repo); sh(['-c', 'user.name=t', '-c', 'user.email=t@t', 'commit', '-qm', 'i'], repo);

// A stopped run with uncommitted work in its worktree.
const stopped = await Git.createRunWorktree({ repo, project: 'proj', runId: 'run_stop', name: 'x' });
writeFileSync(join(stopped.path, 'wip.txt'), 'half done');
// A running one (must be left alone).
const live = await Git.createRunWorktree({ repo, project: 'proj', runId: 'run_live', name: 'y' });
// A folder nobody knows.
mkdirSync(join(root, 'proj', 'run_ghost', 'main'), { recursive: true });
writeFileSync(join(root, 'proj', 'run_ghost', 'main', 'x'), 'x'.repeat(5000));
// A kept branch.
sh(['branch', 'factory/kept-1'], repo);

const runs = [
  { id: 'run_stop', project: 'proj', status: 'stopped', root: stopped.root, repo: stopped.repo, branch: stopped.branch, factoryName: 'Solo', updatedAt: new Date().toISOString() },
  { id: 'run_live', project: 'proj', status: 'running', root: live.root, repo: live.repo, branch: live.branch, factoryName: 'Solo' },
  { id: 'run_kept', project: 'proj', status: 'finished', outcome: { how: 'kept' }, repo, branch: 'factory/kept-1', factoryName: 'Solo', updatedAt: new Date(Date.now() - 3 * 86400e3).toISOString() },
];

let items = await C.scan({ runs, root });
const st = items.find(i => i.runId === 'run_stop');
const ghost = items.find(i => i.kind === 'orphan');
const kept = items.find(i => i.kind === 'kept-branch');
ok(st && st.kind === 'run-worktree' && st.uncommitted === 1 && st.bytes > 0, 'scan: a stopped run\'s worktree, with its uncommitted change counted');
ok(ghost && ghost.bytes >= 5000 && /No run knows/.test(ghost.why), 'scan: a folder no run knows about, with its size');
ok(kept && kept.branch === 'factory/kept-1', 'scan: a branch kept with "Keep branch"');
ok(!items.some(i => i.runId === 'run_live'), 'scan: a run still going is left alone');

ok(!(await C.remove({ kind: 'orphan', path: '/etc' }, { root })).ok, 'remove: refuses anything outside the worktrees root');
ok(!(await C.remove({ kind: 'kept-branch', branch: 'dev', repo }, { root })).ok, 'remove: refuses a branch that is not factory/*');
const r1 = await C.remove(st, { root });
ok(r1.ok && !existsSync(st.path) && !sh(['worktree', 'list'], repo).includes('run_stop') && !sh(['branch', '--list', stopped.branch], repo), 'remove: the worktree is unregistered, its folder and its factory branch gone');
ok((await C.remove(ghost, { root })).ok && !existsSync(ghost.path), 'remove: an unknown folder is deleted');
ok((await C.remove(kept, { root })).ok && !sh(['branch', '--list', 'factory/kept-1'], repo), 'remove: a kept branch is deleted when asked');
ok(existsSync(live.path), 'remove: the running run\'s worktree is untouched');

// Auto-delete.
ok(C.readSettings(process.env.DATA_DIR).autoDeleteDays === 0, 'settings: auto-delete is off by default');
C.writeSettings(process.env.DATA_DIR, { autoDeleteDays: 2 });
ok(C.readSettings(process.env.DATA_DIR).autoDeleteDays === 2, 'settings: saved');
const oldDirty = await Git.createRunWorktree({ repo, project: 'proj', runId: 'run_old_dirty', name: 'a' });
writeFileSync(join(oldDirty.path, 'keep-me.txt'), 'work');
const oldClean = await Git.createRunWorktree({ repo, project: 'proj', runId: 'run_old_clean', name: 'b' });
const young = await Git.createRunWorktree({ repo, project: 'proj', runId: 'run_young', name: 'c' });
const past = (Date.now() - 5 * 86400e3) / 1000;
for (const w of [oldDirty, oldClean]) utimesSync(w.root, past, past);
sh(['branch', 'factory/kept-2'], repo);
const runs2 = [
  { id: 'run_old_dirty', project: 'proj', status: 'stopped', root: oldDirty.root, repo, branch: oldDirty.branch },
  { id: 'run_old_clean', project: 'proj', status: 'stopped', root: oldClean.root, repo, branch: oldClean.branch },
  { id: 'run_young', project: 'proj', status: 'stopped', root: young.root, repo, branch: young.branch },
  { id: 'run_kept2', project: 'proj', status: 'finished', outcome: { how: 'kept' }, repo, branch: 'factory/kept-2', updatedAt: new Date(Date.now() - 30 * 86400e3).toISOString() },
];
const auto = await C.autoDelete({ runs: runs2, dataDir: process.env.DATA_DIR, root });
ok(auto.removed === 1 && !existsSync(oldClean.root), 'auto-delete: an old, clean leftover goes');
ok(existsSync(oldDirty.root), 'auto-delete: never one with uncommitted work');
ok(existsSync(young.root), 'auto-delete: not one younger than the limit');
ok(sh(['branch', '--list', 'factory/kept-2'], repo).includes('kept-2'), 'auto-delete: never a kept branch');
await C.prune(runs2);
ok(true, 'prune: runs without error');

// ─── The live card ───
const run = (o) => ({ factoryName: 'Std', status: 'running', waitingOn: '', tokens: [], ...o });
const card = (o, m) => factoryLiveCard(run(o), m);
ok(card({ tokens: [{ status: 'work', employee: 'Frontend dev' }] }).state === 'working' && card({ tokens: [{ status: 'work', employee: 'Frontend dev' }] }).title === 'Std: Frontend dev working', 'live card: working, saying who');
ok(card({ tokens: [{ status: 'answer', employee: 'Manager' }] }).title === 'Std: Manager deciding', 'live card: deciding');
ok(card({ tokens: [{ status: 'join' }] }, 'idle').state === 'waiting', 'live card: between steps while the Manager is idle and nobody works is waiting, not gone');
ok(card({ waitingOn: 'ask' }).state === 'needs-input' && card({ waitingOn: 'manager' }).state === 'needs-input' && card({ waitingOn: 'cap' }).state === 'needs-input', 'live card: a question, the Manager talking to you, or a limit need you');
ok(card({ status: 'finishing' }).title === 'Std: done, your decision' && card({ status: 'finishing' }).state === 'needs-input', 'live card: the result waiting is a decision for you');
ok(card({ status: 'crashed' }).title === 'Std: crashed', 'live card: crashed');
ok(card({ status: 'paused' }).state === 'waiting' && card({ status: 'paused' }).title === 'Std: paused', 'live card: paused');
ok(card({ status: 'finished' }).state === 'idle' && card({ status: 'stopped' }).state === 'idle', 'live card: over, the card goes');

try { rmSync(base, { recursive: true, force: true, maxRetries: 10, retryDelay: 300 }); } catch { /* Windows may still hold a file; it is a temp folder */ };
console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
