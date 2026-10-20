/**
 * factory-cleanup.js — what factory runs leave behind, and clearing it.
 *
 * Merge and Discard clean up after themselves. What can be left:
 *   - the worktree of a run that was stopped (kept on purpose: the work is
 *     there, the person may want it)
 *   - a branch kept with "Keep branch"
 *   - branch worktrees kept after a merge conflict
 *   - folders under the worktrees root that no run knows about (a crash
 *     between creating a worktree and recording it, a run deleted by hand)
 *
 * Nothing here deletes on its own unless the person turned on auto-delete,
 * and even then never a worktree with uncommitted changes. Every removal is
 * confined to the factory worktrees root, and branches to factory/*.
 *
 * Spec: specs/factory.md §5.3.
 */

import { existsSync, readdirSync, statSync, lstatSync, rmSync, readFileSync, writeFileSync, renameSync } from 'fs';
import { join, resolve, sep } from 'path';
import * as Git from './factory-git.js';

const SIZE_FILE_LIMIT = 200_000;
const SETTINGS = (dataDir) => join(dataDir, 'factory-settings.json');

function within(root, p) {
  const r = resolve(root), c = resolve(p);
  return c === r || c.startsWith(r + sep);
}

/** Bytes under a folder, stopping after a lot of files (then it says so). */
function folderSize(dir) {
  let bytes = 0, files = 0, capped = false;
  const stack = [dir];
  while (stack.length) {
    const d = stack.pop();
    let names;
    try { names = readdirSync(d); } catch { continue; }
    for (const n of names) {
      if (n === '.git') continue;   // a worktree's .git is a pointer file; the objects are shared
      const p = join(d, n);
      let st;
      try { st = lstatSync(p); } catch { continue; }
      if (st.isSymbolicLink()) continue;
      if (st.isDirectory()) stack.push(p);
      else { bytes += st.size; if (++files > SIZE_FILE_LIMIT) { capped = true; stack.length = 0; break; } }
    }
  }
  return { bytes, capped };
}

async function uncommitted(path) {
  if (!existsSync(path)) return 0;
  const r = await Git.git(['status', '--porcelain'], path);
  if (!r.ok) return 0;
  return r.stdout.split('\n').filter(l => l.trim() && !Git.NEVER_COMMIT.some(x => l.includes(x)) && !l.includes('.claude/skills')).length;
}

/**
 * Everything left over, newest first.
 * @param {object} o
 * @param {Array} o.runs     runner.listRuns() (with worktree, root, repo, branch, status, outcome)
 * @param {string} [o.root]  the worktrees root
 */
export async function scan({ runs, root = Git.worktreesRoot() }) {
  const items = [];
  const byRoot = new Map();
  for (const r of runs) if (r.root) byRoot.set(resolve(r.root), r);
  const busy = new Set(['starting', 'running', 'waiting', 'paused', 'finishing', 'crashed']);

  if (existsSync(root)) {
    for (const proj of readdirSync(root)) {
      const pdir = join(root, proj);
      let st; try { st = statSync(pdir); } catch { continue; }
      if (!st.isDirectory()) continue;
      for (const runDir of readdirSync(pdir)) {
        const rdir = join(pdir, runDir);
        try { if (!statSync(rdir).isDirectory()) continue; } catch { continue; }
        const run = byRoot.get(resolve(rdir));
        if (run && busy.has(run.status)) continue;   // in use
        const age = (() => { try { return Date.now() - statSync(rdir).mtimeMs; } catch { return 0; } })();
        const size = folderSize(rdir);
        const main = join(rdir, 'main');
        items.push({
          id: 'wt:' + Buffer.from(rdir).toString('base64url'),
          kind: run ? 'run-worktree' : 'orphan',
          path: rdir,
          project: run ? run.project : proj,
          runId: run ? run.id : runDir,
          factoryName: run ? run.factoryName : '',
          status: run ? run.status : 'unknown',
          branch: run ? run.branch : '',
          repo: run ? run.repo : '',
          bytes: size.bytes, sizeCapped: size.capped,
          ageMs: age,
          uncommitted: await uncommitted(main),
          why: run ? (run.status === 'stopped' ? 'The run was stopped; its work was kept here.' : `The run is ${run.status}; its folders were not removed.`) : 'No run knows about this folder.',
        });
      }
    }
  }
  // Branches kept on purpose with "Keep branch".
  for (const r of runs) {
    if (r.status !== 'finished' || r.outcome?.how !== 'kept' || !r.repo || !r.branch) continue;
    const exists = await Git.git(['rev-parse', '--verify', '--quiet', `refs/heads/${r.branch}`], r.repo);
    if (!exists.ok) continue;
    items.push({
      id: 'br:' + r.id, kind: 'kept-branch', path: '', project: r.project, runId: r.id, factoryName: r.factoryName,
      status: r.status, branch: r.branch, repo: r.repo, bytes: 0, ageMs: Date.now() - Date.parse(r.updatedAt || 0), uncommitted: 0,
      why: 'Kept with "Keep branch". Deleting it deletes that work.',
    });
  }
  return items.sort((a, b) => a.ageMs - b.ageMs);
}

/** Remove one leftover. Only inside the worktrees root; only factory/* branches. */
export async function remove(item, { root = Git.worktreesRoot() } = {}) {
  if (item.kind === 'kept-branch') {
    if (!item.branch.startsWith('factory/')) return { ok: false, error: 'Not a factory branch.' };
    const r = await Git.git(['branch', '-D', item.branch], item.repo);
    return r.ok ? { ok: true } : { ok: false, error: r.error };
  }
  if (!item.path || !within(root, item.path) || resolve(item.path) === resolve(root)) return { ok: false, error: 'Refusing to remove a folder outside the factory worktrees.' };
  // Every worktree inside the run folder: unregister it from its repository
  // (and drop its factory branch), then remove what is left.
  let names = [];
  try { names = readdirSync(item.path); } catch { names = []; }
  for (const n of names) {
    const wt = join(item.path, n);
    const gitFile = join(wt, '.git');
    if (!existsSync(gitFile)) continue;
    let repo = item.repo;
    if (!repo) {
      const common = await Git.git(['rev-parse', '--git-common-dir'], wt);
      if (common.ok) repo = resolve(wt, common.stdout.trim(), '..');
    }
    const br = await Git.git(['rev-parse', '--abbrev-ref', 'HEAD'], wt);
    const branch = br.ok ? br.stdout.trim() : '';
    if (repo) await Git.removeWorktree({ repo, path: wt, branch, deleteBranch: branch.startsWith('factory/') });
  }
  try { rmSync(item.path, { recursive: true, force: true }); } catch (err) { return { ok: false, error: err.message }; }
  if (item.repo) await Git.git(['worktree', 'prune'], item.repo);
  return { ok: true };
}

/** At boot: forget worktrees whose folders are gone, in every repo a run used. */
export async function prune(runs) {
  const repos = new Set(runs.map(r => r.repo).filter(Boolean));
  for (const repo of repos) if (existsSync(repo)) await Git.git(['worktree', 'prune'], repo);
}

export function readSettings(dataDir) {
  try { const s = JSON.parse(readFileSync(SETTINGS(dataDir), 'utf-8')); return { autoDeleteDays: Math.max(0, Math.min(365, Number(s.autoDeleteDays) || 0)) }; }
  catch { return { autoDeleteDays: 0 }; }
}

export function writeSettings(dataDir, s) {
  const next = { autoDeleteDays: Math.max(0, Math.min(365, Math.round(Number(s.autoDeleteDays) || 0))) };
  const tmp = SETTINGS(dataDir) + '.tmp';
  writeFileSync(tmp, JSON.stringify(next));
  renameSync(tmp, SETTINGS(dataDir));
  return next;
}

/**
 * Auto-delete, when turned on: leftovers older than N days, never one with
 * uncommitted work, never a kept branch (keeping it was a decision).
 */
export async function autoDelete({ runs, dataDir, root = Git.worktreesRoot() }) {
  const { autoDeleteDays } = readSettings(dataDir);
  if (!autoDeleteDays) return { removed: 0 };
  const limit = autoDeleteDays * 86_400_000;
  let removed = 0;
  for (const it of await scan({ runs, root })) {
    if (it.kind === 'kept-branch' || it.uncommitted || it.ageMs < limit) continue;
    const r = await remove(it, { root });
    if (r.ok) removed++;
  }
  return { removed };
}
