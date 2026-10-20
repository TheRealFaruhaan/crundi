/**
 * factory-git.js — the worktrees a factory run works in.
 *
 * A run never touches the project's own checkout. It gets:
 *
 *   <root>/<project>/<runId>/main        the run worktree, on factory/<slug>-<id>
 *   <root>/<project>/<runId>/<name>      one per parallel branch, cut from main
 *
 * <root> is FACTORY_WORKTREES_DIR or ~/crundi-factory-worktrees: outside
 * PROJECTS_DIR (or each worktree would be discovered as a project of its own)
 * and beside, not inside, the collaborators' worktrees.
 *
 * The runner commits at every handoff, so each step is a commit: that is how
 * progress is detected (the tree hash changes or it does not), and branches
 * can be merged at a Join. Files Crundi places in a worktree for its own use
 * are never committed: git only honours info/exclude from the shared git dir
 * (verified: a linked worktree's own info/exclude is ignored), and writing to
 * the shared one would change the project's checkout too, so the exclusions
 * are pathspecs on every `git add` instead.
 *
 * Commits are made as "Crundi Factory" so they work on a machine with no git
 * identity; the final squash onto the parent branch (card 5) is the user's.
 */

import { execFile } from 'child_process';
import { existsSync, mkdirSync, readdirSync, symlinkSync, lstatSync } from 'fs';
import { join, resolve, sep } from 'path';
import { homedir } from 'os';

export const worktreesRoot = () => process.env.FACTORY_WORKTREES_DIR || join(homedir(), 'crundi-factory-worktrees');

/** Paths Crundi may place in a worktree that must never be committed. */
export const NEVER_COMMIT = ['.mcp.json', '.factory'];

/**
 * Make the project's skills visible in a worktree. Claude finds project skills
 * in <cwd>/.claude/skills, and a worktree only has the ones committed. If the
 * worktree has no skills folder, the project's whole folder is linked; if it
 * has one (committed skills), each project skill it lacks is linked in. The
 * links are returned so every commit can leave them out.
 * @returns {string[]} paths, relative to the worktree, never to commit
 */
export function linkProjectSkills(repo, worktree) {
  const src = join(repo, '.claude', 'skills');
  if (!existsSync(src)) return [];
  const dst = join(worktree, '.claude', 'skills');
  try {
    if (!existsSync(dst)) {
      mkdirSync(join(worktree, '.claude'), { recursive: true });
      symlinkSync(src, dst, 'dir');
      return ['.claude/skills'];
    }
    if (lstatSync(dst).isSymbolicLink()) return ['.claude/skills'];
    const out = [];
    for (const name of readdirSync(src)) {
      if (name.startsWith('.') || existsSync(join(dst, name))) continue;
      symlinkSync(join(src, name), join(dst, name), 'dir');
      out.push(`.claude/skills/${name}`);
    }
    return out;
  } catch { return []; }
}

const IDENTITY = ['-c', 'user.name=Crundi Factory', '-c', 'user.email=factory@crundi.invalid', '-c', 'commit.gpgsign=false'];

export function git(args, cwd, { timeout = 120000 } = {}) {
  return new Promise((done) => {
    execFile('git', args, { cwd, timeout, maxBuffer: 16 * 1024 * 1024 }, (err, stdout, stderr) => done({
      ok: !err,
      code: err ? (typeof err.code === 'number' ? err.code : 1) : 0,
      stdout: String(stdout || ''),
      stderr: String(stderr || ''),
      error: err ? String(stderr || err.message || err).trim() : '',
    }));
  });
}

export function slug(s, max = 32) {
  return String(s || '').toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, max) || 'run';
}

function isWithin(parent, child) {
  const p = resolve(parent), c = resolve(child);
  return c === p || c.startsWith(p + sep);
}

/** Folder a run's worktrees live in. */
export function runRoot(project, runId) {
  return join(worktreesRoot(), slug(project, 64), String(runId).replace(/[^A-Za-z0-9_-]/g, ''));
}

/**
 * What the run would start from: the project's current branch and commit, and
 * whether there is uncommitted work that the run will NOT see.
 */
export async function inspectRepo(repo) {
  if (!repo || !existsSync(repo)) return { ok: false, error: 'The project folder does not exist.' };
  const top = await git(['rev-parse', '--show-toplevel'], repo);
  if (!top.ok) return { ok: false, error: 'This project is not a git repository. A factory works in a git worktree, so the project needs git.' };
  const head = await git(['rev-parse', 'HEAD'], repo);
  if (!head.ok) return { ok: false, error: 'This repository has no commits yet. Make a first commit, then start the factory.' };
  const br = await git(['symbolic-ref', '--quiet', '--short', 'HEAD'], repo);
  if (!br.ok) return { ok: false, error: 'The project is on a detached HEAD. Check out a branch first, so the result has somewhere to merge back to.' };
  const st = await git(['status', '--porcelain'], repo);
  return {
    ok: true,
    top: top.stdout.trim(),
    branch: br.stdout.trim(),
    commit: head.stdout.trim(),
    dirty: st.ok && st.stdout.trim().length > 0,
  };
}

/**
 * Create the run worktree on a new branch cut from the project's HEAD.
 * @returns {{ok, path, branch, parentBranch, baseCommit, dirty, error?}}
 */
export async function createRunWorktree({ repo, project, runId, name }) {
  const info = await inspectRepo(repo);
  if (!info.ok) return info;
  const root = runRoot(project, runId);
  const path = join(root, 'main');
  if (existsSync(path)) return { ok: false, error: `A worktree already exists at ${path}` };
  mkdirSync(root, { recursive: true });
  const branch = `factory/${slug(name, 40)}-${String(runId).slice(-6)}`;
  const add = await git(['worktree', 'add', '-b', branch, path, info.commit], info.top);
  if (!add.ok) return { ok: false, error: `Could not create the worktree: ${add.error}` };
  return { ok: true, path, root, branch, parentBranch: info.branch, baseCommit: info.commit, dirty: info.dirty, repo: info.top };
}

/** A worktree for one parallel branch, cut from the current commit of `parentPath`. */
export async function createBranchWorktree({ repo, parentPath, root, name, branchPrefix, exclude = [] }) {
  const base = await commitAll(parentPath, 'factory: before fan-out', { exclude });
  if (!base.ok) return base;
  const dirName = slug(name, 40);
  let path = join(root, dirName), n = 1;
  while (existsSync(path)) path = join(root, `${dirName}-${++n}`);
  const branch = `${branchPrefix}--${dirName}${n > 1 ? '-' + n : ''}`;
  const add = await git(['worktree', 'add', '-b', branch, path, base.head], repo);
  if (!add.ok) return { ok: false, error: `Could not create the branch worktree: ${add.error}` };
  return { ok: true, path, branch, baseCommit: base.head };
}

/**
 * Stage everything except Crundi's own files and commit it, if anything
 * changed. Returns the resulting HEAD and its tree hash either way.
 */
export async function commitAll(path, message, { exclude = [] } = {}) {
  const add = await git(['add', '-A', '--', '.', ...[...NEVER_COMMIT, ...exclude].map(p => `:(exclude)${p}`)], path);
  if (!add.ok) return { ok: false, error: `git add failed: ${add.error}` };
  const staged = await git(['diff', '--cached', '--quiet'], path);
  let committed = false;
  if (staged.code === 1) {
    const c = await git([...IDENTITY, 'commit', '--no-verify', '-q', '-m', String(message || 'factory: step').slice(0, 2000)], path);
    if (!c.ok) return { ok: false, error: `git commit failed: ${c.error}` };
    committed = true;
  } else if (!staged.ok && staged.code !== 0) {
    return { ok: false, error: `git diff failed: ${staged.error}` };
  }
  const head = await git(['rev-parse', 'HEAD'], path);
  const tree = await git(['rev-parse', 'HEAD^{tree}'], path);
  return { ok: true, committed, head: head.stdout.trim(), tree: tree.stdout.trim() };
}

/**
 * Merge a branch into the worktree at `parentPath` (both committed first).
 * On conflict the merge is aborted and the conflicted files are returned, so
 * the parent is left exactly as it was.
 */
export async function mergeBranch({ parentPath, childPath, childBranch, message, exclude = [] }) {
  if (childPath) {
    const c = await commitAll(childPath, 'factory: branch done', { exclude });
    if (!c.ok) return c;
  }
  const p = await commitAll(parentPath, 'factory: before join', { exclude });
  if (!p.ok) return p;
  const m = await git([...IDENTITY, 'merge', '--no-ff', '--no-edit', '-m', message || `factory: join ${childBranch}`, childBranch], parentPath);
  if (m.ok) {
    const head = await git(['rev-parse', 'HEAD'], parentPath);
    return { ok: true, head: head.stdout.trim() };
  }
  const conflicted = await git(['diff', '--name-only', '--diff-filter=U'], parentPath);
  await git(['merge', '--abort'], parentPath);
  const files = conflicted.stdout.split('\n').map(s => s.trim()).filter(Boolean);
  return { ok: false, conflict: files.length > 0, files, error: files.length ? `Merge conflict in ${files.join(', ')}` : `Merge failed: ${m.error}` };
}

/** Remove a worktree folder and, optionally, its branch. Only inside the factory root. */
export async function removeWorktree({ repo, path, branch, deleteBranch = true }) {
  if (path && !isWithin(worktreesRoot(), path)) return { ok: false, error: 'Refusing to remove a folder outside the factory worktrees.' };
  if (path && existsSync(path)) {
    const r = await git(['worktree', 'remove', '--force', path], repo);
    if (!r.ok) return { ok: false, error: `Could not remove the worktree: ${r.error}` };
  }
  if (deleteBranch && branch && branch.startsWith('factory/')) await git(['branch', '-D', branch], repo);
  await git(['worktree', 'prune'], repo);
  return { ok: true };
}

/** Short status and diff stat of a worktree against a base commit, for prompts. */
export async function workSummary(path, baseCommit) {
  const st = await git(['status', '--short'], path);
  const stat = baseCommit ? await git(['diff', '--stat', baseCommit], path) : { stdout: '' };
  const keep = s => s.split('\n').filter(l => l.trim() && ![...NEVER_COMMIT, '.claude/skills'].some(p => l.includes(p))).slice(0, 60).join('\n');
  return { status: keep(st.stdout), diffStat: keep(stat.stdout) };
}
