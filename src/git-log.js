// Git history for the phone's Git tab: the commits of the current branch and
// one commit's changes. Read-only; nothing here writes to the repository.
import { execFileSync } from 'node:child_process';

const FS = '\x1f';   // between fields
const RS = '\x1e';   // between commits

function git(cwd, args, max = 5 * 1024 * 1024) {
  return execFileSync('git', args, { cwd, encoding: 'utf8', timeout: 15000, windowsHide: true, maxBuffer: max, stdio: ['ignore', 'pipe', 'pipe'] });
}

/** `git log` output in our format to commits. */
export function parseLog(raw) {
  const out = [];
  for (const rec of String(raw || '').split(RS)) {
    const t = rec.replace(/^\n+/, '');
    if (!t.trim()) continue;
    const [hash, parents, author, email, date, refs, subject, body] = t.split(FS);
    if (!hash || !/^[0-9a-f]{7,64}$/.test(hash)) continue;
    out.push({
      hash,
      short: hash.slice(0, 7),
      parents: (parents || '').split(' ').filter(Boolean),
      author: author || '',
      email: email || '',
      date: date || '',
      refs: (refs || '').split(',').map(s => s.trim()).filter(Boolean),
      subject: subject || '',
      body: (body || '').trim(),
    });
  }
  return out;
}

const FORMAT = ['%H', '%P', '%an', '%ae', '%aI', '%D', '%s', '%b'].join(FS) + RS;

/** The last commits reachable from HEAD, newest first. */
export function gitLog(cwd, { limit = 50, skip = 0 } = {}) {
  const n = Math.max(1, Math.min(200, parseInt(limit, 10) || 50));
  const s = Math.max(0, parseInt(skip, 10) || 0);
  let raw = '';
  try {
    raw = git(cwd, ['log', `--max-count=${n + 1}`, `--skip=${s}`, `--format=${FORMAT}`, 'HEAD']);
  } catch (err) {
    // A repository with no commits yet has no HEAD to log: that is an empty history.
    if (/does not have any commits|unknown revision|bad default revision/i.test(String(err.stderr || err.message))) return { commits: [], more: false };
    throw err;
  }
  const commits = parseLog(raw);
  return { commits: commits.slice(0, n), more: commits.length > n };
}

/** One commit: who, when, the message, which files, and the patch (cut at 400 KB). */
export function gitShow(cwd, hash) {
  if (!/^[0-9a-f]{4,64}$/i.test(String(hash || ''))) throw new Error('Not a commit id');
  const head = parseLog(git(cwd, ['show', '-s', `--format=${FORMAT}`, hash]))[0];
  if (!head) throw new Error('Commit not found');
  const files = [];
  const numstat = git(cwd, ['show', '--format=', '--numstat', '-M', hash]);
  for (const l of numstat.split('\n')) {
    const m = l.match(/^(\d+|-)\t(\d+|-)\t(.+)$/);
    if (m) files.push({ file: m[3], add: m[1] === '-' ? 0 : +m[1], del: m[2] === '-' ? 0 : +m[2], binary: m[1] === '-' });
  }
  const LIMIT = 400 * 1024;
  let diff = git(cwd, ['show', '--format=', '--patch', '-M', hash], 20 * 1024 * 1024);
  const truncated = diff.length > LIMIT;
  if (truncated) diff = diff.slice(0, LIMIT);
  return { ...head, files, diff, truncated };
}
