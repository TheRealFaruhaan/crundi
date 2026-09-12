/**
 * collaborators.js — time-boxed access for people outside the household.
 *
 * A collaborator is someone you hand a project to, for a while, and then stop.
 * Everything here follows from that sentence:
 *
 *   a project     — one record is one invitation. A person may hold several,
 *                   and sees exactly those; every other project on the machine
 *                   stays invisible to them (see filterStateFor)
 *   for a while   — expiry is checked on every request, not just at login,
 *                   so revoking is immediate rather than "after they log out"
 *   and then stop — removing one takes its worktree with it
 *
 * ─── Why a worktree, not the project folder ───
 *
 * `git worktree add` gives them a real checkout with its own HEAD on its own
 * branch. They can commit without moving the branch you are standing on, you
 * can read their work with ordinary git commands, and when they are done the
 * branch is either merged or deleted. It also gives the access layer a single
 * directory to confine them to, which the project folder could not: that one
 * contains .git, .env, and whatever else you keep next to your code.
 *
 * ─── Credentials ───
 *
 * Two ways in, both optional, at least one required:
 *
 *   telegram — their @username, checked the same way the owner's is
 *   passcode — a generated phrase, scrypt-hashed here, shown to you ONCE
 *
 * A collaborator also gets their own Crundi API key, distinct from the
 * internal one, so the MCP bridge can tell whose request it is serving and
 * narrow the tool list accordingly. The internal key reaches secret_get; this
 * one does not.
 */

import { readFileSync, writeFileSync, existsSync, mkdirSync, rmSync, readdirSync, statSync } from 'fs';
import { join } from 'path';
import { randomBytes, scryptSync, timingSafeEqual } from 'crypto';
import { execFile } from 'child_process';
import { config } from './config.js';
import { getProject } from './project-store.js';

const FILE = () => join(config.dataDir, 'collaborators.json');

/** Where worktrees live. Outside every project, so none of them can see it. */
const ROOT = () => join(config.dataDir, 'worktrees');

const genId = () => randomBytes(8).toString('hex');

function loadAll() {
  try {
    const d = JSON.parse(readFileSync(FILE(), 'utf8'));
    return Array.isArray(d?.collaborators) ? d.collaborators : [];
  } catch { return []; }
}

function saveAll(list) {
  try {
    mkdirSync(config.dataDir, { recursive: true });
    const tmp = FILE() + '.tmp';
    writeFileSync(tmp, JSON.stringify({ collaborators: list }, null, 2), { mode: 0o600 });
    // Rename rather than write in place: a truncated collaborators.json would
    // read back as "no collaborators", which fails open into everyone losing
    // access rather than closed. Cheap insurance.
    writeFileSync(FILE(), readFileSync(tmp), { mode: 0o600 });
    rmSync(tmp, { force: true });
  } catch { /* a failed write must not take the request with it */ }
}

// ─── How long access lasts ───
//
// Expressed in HOURS internally, because the useful range spans both ends:
// "four hours to look at this" and "a fortnight to build it" are both normal
// asks, and a days-only field cannot say the first at all.
//
// Clamped to 1 hour .. 365 days. The floor stops a zero or a stray minus sign
// producing access that has already expired — which would look like the
// feature is broken rather than like the input was wrong.

const MIN_HOURS = 1;
const MAX_HOURS = 365 * 24;

/**
 * Resolve a duration to hours, accepting either unit.
 *
 * `days` is still honoured so older callers and stored schedules keep working;
 * `hours` wins when both are given.
 */
export function toHours({ hours, days } = {}) {
  let h = null;
  if (hours !== undefined && hours !== null && hours !== '') h = parseFloat(hours);
  else if (days !== undefined && days !== null && days !== '') h = parseFloat(days) * 24;
  if (!Number.isFinite(h) || h <= 0) h = 7 * 24;
  return Math.max(MIN_HOURS, Math.min(MAX_HOURS, Math.round(h)));
}

// ─── Identity ───
//
// One record is one INVITATION: a person, a project, a worktree, an expiry.
// The same person can hold several, and signing in once must reach all of
// them — so the thing a session is bound to is the person, not the invitation.
//
// That key is the Telegram username where there is one, and the display name
// otherwise. Both are compared lowercased.

export function identityOf(rec) {
  return rec?.telegram ? `tg:${rec.telegram}` : `pc:${String(rec?.name || '').toLowerCase()}`;
}

/** Every live invitation belonging to one person. */
export function liveByIdentity(key) {
  if (!key) return [];
  return loadAll().filter(c => !isExpired(c) && identityOf(c) === key);
}

// ─── Passcodes ───

/** Readable rather than random-looking: these get typed, often on a phone. */
const WORDS = ['amber', 'basalt', 'cobalt', 'dune', 'ember', 'fjord', 'grove', 'harbor',
  'indigo', 'jetty', 'kelp', 'lantern', 'meadow', 'nimbus', 'onyx', 'pier',
  'quarry', 'ridge', 'saffron', 'thicket', 'umber', 'vellum', 'willow', 'zephyr'];

export function generatePasscode() {
  const pick = () => WORDS[randomBytes(1)[0] % WORDS.length];
  return `${pick()}-${pick()}-${pick()}-${randomBytes(2).toString('hex')}`;
}

function hashPasscode(pass, salt = randomBytes(16).toString('hex')) {
  return { salt, hash: scryptSync(String(pass), salt, 64, { N: 16384, r: 8, p: 1 }).toString('hex') };
}

function verifyPasscode(pass, rec) {
  if (!rec?.passSalt || !rec?.passHash) return false;
  try {
    const got = scryptSync(String(pass), rec.passSalt, 64, { N: 16384, r: 8, p: 1 });
    const want = Buffer.from(rec.passHash, 'hex');
    return got.length === want.length && timingSafeEqual(got, want);
  } catch { return false; }
}

// ─── Worktrees ───

function git(args, cwd) {
  return new Promise((resolve) => {
    execFile('git', args, { cwd, timeout: 60000, maxBuffer: 4 * 1024 * 1024 },
      (err, stdout, stderr) => resolve({
        ok: !err,
        stdout: String(stdout || ''),
        stderr: String(stderr || ''),
        error: err ? String(err.message || err) : '',
      }));
  });
}

/** Branch and directory names, from a display name, without surprises. */
function slug(s) {
  return String(s || '').toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 32)
    || 'collab';
}

// ─── Projects that are not repositories yet ───
//
// A worktree needs a commit to branch from, so a folder with no git in it
// cannot be shared as-is. Initialising one is not a neutral act: it creates a
// commit containing whatever is currently in the folder, which on a working
// project may include an .env, a private key, or a 300 MB node_modules.
//
// So this reports first and acts second. The preview is the whole point —
// "set up git" is a fine button to press once you can see what it would
// sweep up, and a bad one before.

/**
 * Files that must not reach a shared repository.
 *
 * `.env.example` and friends are deliberately NOT here: committing the example
 * is the whole point of having one, and flagging it would teach you to ignore
 * the warnings that matter.
 */
const RISKY = [
  { re: /(^|\/)\.env(\.(?!example|sample|template|dist)[^/]*)?$/i, why: 'environment file' },
  { re: /(^|\/)id_(rsa|ed25519|ecdsa)$/i, why: 'private SSH key' },
  { re: /\.(pem|key|p12|pfx|keystore)$/i, why: 'key or certificate' },
  { re: /(^|\/)(credentials|secrets?|service-account)[^/]*\.(json|ya?ml|ini)$/i, why: 'credentials file' },
  { re: /(^|\/)\.npmrc$/i, why: 'may hold an auth token' },
  { re: /(^|\/)\.git-credentials$/i, why: 'stored git passwords' },
];

/** Directories kept out of the first commit, when we are writing the .gitignore. */
const IGNORE_DIRS = ['node_modules', '.venv', 'venv', '__pycache__', 'dist', 'build',
  '.next', '.cache', 'target', 'vendor'];

/**
 * The .gitignore written for a project that had none.
 *
 * Secrets are EXCLUDED rather than merely warned about. A warning you have to
 * act on is one you will eventually click past, and the cost of clicking past
 * this one is handing an outsider your credentials. The collaborator gets a
 * checkout without them, which is the right default — their Claude can ask you
 * for whatever it actually needs.
 */
const DEFAULT_IGNORE = [
  '# Written by Crundi when this project was first shared.',
  '# Edit freely \u2014 this is your file now.',
  '',
  '# Dependencies and build output',
  ...IGNORE_DIRS,
  '',
  '# Noise',
  '.DS_Store',
  '*.log',
  '',
  '# Secrets. Kept out on purpose: this repository gets shared.',
  '.env',
  '.env.*',
  '!.env.example',
  '!.env.sample',
  '!.env.template',
  '*.pem',
  '*.key',
  '*.p12',
  '*.pfx',
  '.npmrc',
  '.git-credentials',
  'id_rsa',
  'id_ed25519',
  '',
];

/**
 * Does the generated .gitignore cover this path?
 *
 * Shared by the preview and the commit so the two cannot disagree. A preview
 * saying "12 files" followed by a commit of 4000 would be worse than showing
 * no preview at all.
 */
function wouldIgnore(rel) {
  const name = rel.split('/').pop();
  if (rel.split('/').some(seg => IGNORE_DIRS.includes(seg))) return true;
  if (name === '.DS_Store' || name.endsWith('.log')) return true;
  if (/^\.env($|\.)/i.test(name) && !/^\.env\.(example|sample|template)$/i.test(name)) return true;
  if (/\.(pem|key|p12|pfx)$/i.test(name)) return true;
  if (['.npmrc', '.git-credentials', 'id_rsa', 'id_ed25519'].includes(name)) return true;
  return false;
}

/**
 * What initialising git here would sweep into the first commit.
 *
 * Walks the folder rather than trusting a guess, skipping the directories the
 * generated .gitignore would exclude — so the count is what would ACTUALLY be
 * committed, not what is on disk.
 */
export function gitPreview(projectAlias) {
  const project = getProject(projectAlias);
  if (!project) return { ok: false, error: `Project "${projectAlias}" not found` };
  if (existsSync(join(project.path, '.git'))) {
    return { ok: true, alreadyGit: true };
  }
  const hasIgnore = existsSync(join(project.path, '.gitignore'));
  let files = 0;
  let bytes = 0;
  const risky = [];      // would be committed AND looks sensitive
  const excluded = [];   // looks sensitive, but the .gitignore keeps it out
  let truncated = false;
  const walk = (dir, rel) => {
    if (files > 20000) { truncated = true; return; }
    let ents = [];
    try { ents = readdirSync(dir, { withFileTypes: true }); } catch { return; }
    for (const e of ents) {
      if (e.name === '.git') continue;
      const full = join(dir, e.name);
      const r = rel ? `${rel}/${e.name}` : e.name;
      if (e.isSymbolicLink()) continue;
      // Only OUR .gitignore can be predicted. If the project already has one,
      // git decides, so the walk counts everything and the caller is told.
      const ignored = !hasIgnore && wouldIgnore(r);
      if (e.isDirectory()) { if (!ignored) walk(full, r); continue; }
      const hit = RISKY.find(x => x.re.test(r));
      if (ignored) {
        if (hit && excluded.length < 25) excluded.push({ path: r, why: hit.why });
        continue;
      }
      files++;
      try { bytes += statSync(full).size; } catch { /* vanished */ }
      if (hit && risky.length < 25) risky.push({ path: r, why: hit.why });
    }
  };
  walk(project.path, '');
  // The .gitignore we are about to write is itself committed, and did not
  // exist to be counted. Without this the preview says 2 and the commit says
  // 3 — a small gap, but the whole value of a preview is that it matches.
  if (!hasIgnore) files += 1;
  return {
    ok: true, alreadyGit: false, files, bytes, risky, excluded, truncated,
    hasIgnore, wouldWriteIgnore: !hasIgnore,
    // Only the ones actually present, so the list describes this project
    // rather than reciting a template.
    ignoring: hasIgnore ? [] : IGNORE_DIRS.filter(d => existsSync(join(project.path, d))),
  };
}

/**
 * Make a project a repository so it can be shared.
 *
 * Writes a .gitignore first when there is none — committing node_modules is
 * not a mistake worth making on someone's behalf — then commits everything
 * else as a starting point.
 */
export async function initGit(projectAlias) {
  const project = getProject(projectAlias);
  if (!project) return { ok: false, error: `Project "${projectAlias}" not found` };
  if (existsSync(join(project.path, '.git'))) return { ok: true, alreadyGit: true };

  const wroteIgnore = !existsSync(join(project.path, '.gitignore'));
  if (wroteIgnore) {
    try {
      writeFileSync(join(project.path, '.gitignore'), DEFAULT_IGNORE.join('\n'));
    } catch (err) {
      return { ok: false, error: `Could not write .gitignore: ${err.message}` };
    }
  }
  const init = await git(['init'], project.path);
  if (!init.ok) return { ok: false, error: `git init failed: ${init.stderr || init.error}` };
  await git(['add', '-A'], project.path);
  const commit = await git(['commit', '-m', 'Initial commit'], project.path);
  if (!commit.ok) {
    // An empty folder, or a machine with no user.name/user.email configured.
    return { ok: false, error: (commit.stderr || commit.error || 'git commit failed').split('\n')[0].slice(0, 300) };
  }
  return { ok: true, wroteIgnore, output: commit.stdout.split('\n')[0] };
}

/**
 * Give a collaborator their own checkout.
 *
 * Returns { ok, path, branch } or { ok:false, error }. Deliberately does not
 * create the branch from whatever HEAD happens to be: it branches from the
 * project's current HEAD and says which commit that was, so "what did they
 * start from" has an answer later.
 */
export async function provisionWorktree(projectAlias, name) {
  const project = getProject(projectAlias);
  if (!project) return { ok: false, error: `Project "${projectAlias}" not found` };

  const inside = await git(['rev-parse', '--is-inside-work-tree'], project.path);
  if (!inside.ok || inside.stdout.trim() !== 'true') {
    // needsGit lets the caller offer to set it up, instead of dead-ending on
    // advice the owner may not be anywhere near a terminal to act on.
    return {
      ok: false,
      needsGit: true,
      project: projectAlias,
      error: `"${projectAlias}" is not a git repository yet. Sharing gives each person their own branch, so it needs one.`,
    };
  }

  const base = await git(['rev-parse', 'HEAD'], project.path);
  if (!base.ok) return { ok: false, error: `Could not read HEAD: ${base.stderr || base.error}` };

  const branch = `collab/${slug(name)}-${randomBytes(3).toString('hex')}`;
  const dir = join(ROOT(), `${slug(projectAlias)}--${slug(name)}-${randomBytes(3).toString('hex')}`);
  mkdirSync(ROOT(), { recursive: true });

  const add = await git(['worktree', 'add', '-b', branch, dir, 'HEAD'], project.path);
  if (!add.ok) return { ok: false, error: `git worktree add failed: ${add.stderr || add.error}` };

  return { ok: true, path: dir, branch, baseCommit: base.stdout.trim() };
}

/** Remove a worktree and its branch. Best effort: a stuck worktree must not block revocation. */
export async function removeWorktree(projectAlias, rec) {
  const project = getProject(projectAlias);
  if (!project || !rec?.worktreePath) return { ok: true, note: 'nothing to remove' };
  const notes = [];
  const rm = await git(['worktree', 'remove', '--force', rec.worktreePath], project.path);
  if (!rm.ok) {
    notes.push(`worktree remove: ${rm.stderr || rm.error}`);
    try { rmSync(rec.worktreePath, { recursive: true, force: true }); } catch { /* ignore */ }
    await git(['worktree', 'prune'], project.path);
  }
  // The branch is left alone when it has commits on it: deleting someone's
  // work because their access lapsed is not a cleanup, it is a data loss.
  const merged = await git(['branch', '--merged', 'HEAD'], project.path);
  const isMerged = merged.ok && merged.stdout.split('\n').map(s => s.trim().replace(/^\*\s*/, '')).includes(rec.branch);
  if (isMerged && rec.branch) {
    const del = await git(['branch', '-d', rec.branch], project.path);
    if (!del.ok) notes.push(`branch delete: ${del.stderr || del.error}`);
  } else if (rec.branch) {
    notes.push(`branch ${rec.branch} kept (has unmerged commits)`);
  }
  return { ok: true, notes };
}

// ─── Handing work back ───

/**
 * Push a collaborator's branch, and only theirs.
 *
 * The refspec is built here from the record rather than taken from the caller.
 * `git push` reaches main when given the wrong argument, and "the wrong
 * argument" is not something to leave to a request body.
 */
export async function pushBranch(id) {
  const c = loadAll().find(x => x.id === id);
  if (!c) return { ok: false, error: 'No such collaborator' };
  if (isExpired(c)) return { ok: false, error: 'That access has ended' };
  if (!c.worktreePath || !existsSync(c.worktreePath)) return { ok: false, error: 'Their worktree is gone' };

  const remote = await git(['remote'], c.worktreePath);
  if (!remote.ok || !remote.stdout.trim()) {
    return { ok: false, error: 'This project has no git remote to push to.' };
  }
  const origin = remote.stdout.split('\n')[0].trim();
  // Explicit src:dst, so the branch cannot land under another name, and
  // --force-with-lease is deliberately ABSENT: a collaborator does not get to
  // overwrite a remote branch, even their own.
  const r = await git(['push', '--set-upstream', origin, `${c.branch}:${c.branch}`], c.worktreePath);
  if (!r.ok) return { ok: false, error: (r.stderr || r.error).split('\n').slice(0, 3).join(' ').slice(0, 300) };
  return { ok: true, branch: c.branch, remote: origin, output: (r.stderr || r.stdout).slice(0, 500) };
}

/**
 * Would merging their branch into the project's current branch conflict?
 *
 * Uses `git merge-tree --write-tree`, which computes the merge in the object
 * database and touches no working tree — so asking this question cannot leave
 * the owner's checkout half-merged, which a trial `git merge` very much can.
 */
export async function mergePreview(id) {
  const c = loadAll().find(x => x.id === id);
  if (!c) return { ok: false, error: 'No such collaborator' };
  const project = getProject(c.project);
  if (!project) return { ok: false, error: 'Project not found' };

  const head = await git(['rev-parse', '--abbrev-ref', 'HEAD'], project.path);
  const target = head.ok ? head.stdout.trim() : 'HEAD';

  const ahead = await git(['rev-list', '--count', `${target}..${c.branch}`], project.path);
  const commits = ahead.ok ? parseInt(ahead.stdout.trim(), 10) || 0 : 0;

  const mt = await git(['merge-tree', '--write-tree', '--name-only', target, c.branch], project.path);
  // Exit 0 = clean, 1 = conflicts, anything else = it could not answer.
  if (mt.ok) return { ok: true, clean: true, conflicts: [], commits, target, branch: c.branch };
  const lines = mt.stdout.split('\n').map(s => s.trim()).filter(Boolean);
  // The first line is the tree oid; the rest are conflicted paths.
  const conflicts = lines.slice(1).filter(l => !/^[0-9a-f]{40}$/.test(l));
  if (!conflicts.length && !mt.stdout.trim()) {
    return { ok: false, error: (mt.stderr || mt.error || 'Could not compare the branches').slice(0, 300) };
  }
  return { ok: true, clean: false, conflicts, commits, target, branch: c.branch };
}

/**
 * Merge their branch into the project's current branch.
 *
 * Refuses on conflict rather than leaving the owner's working tree in a
 * conflicted state for them to discover. Resolving someone else's conflicts is
 * a decision, and it belongs in a real checkout, not behind an approve button.
 */
export async function mergeBranch(id) {
  const pre = await mergePreview(id);
  if (!pre.ok) return pre;
  if (!pre.clean) {
    return { ok: false, error: `Merging would conflict in ${pre.conflicts.length} file(s). Ask them to rebase onto ${pre.target} first.`, conflicts: pre.conflicts };
  }
  if (!pre.commits) return { ok: false, error: 'Nothing to merge — their branch has no new commits.' };

  const c = loadAll().find(x => x.id === id);
  const project = getProject(c.project);
  const status = await git(['status', '--porcelain'], project.path);
  if (status.ok && status.stdout.trim()) {
    return { ok: false, error: 'Your own checkout has uncommitted changes. Commit or stash them before merging.' };
  }
  const r = await git(['merge', '--no-ff', '-m', `Merge collaborator work from ${c.name} (${c.branch})`, c.branch], project.path);
  if (!r.ok) return { ok: false, error: (r.stderr || r.error).slice(0, 300) };
  return { ok: true, merged: c.branch, into: pre.target, commits: pre.commits };
}

// ─── Records ───

/** Everything about a collaborator except the parts that must never leave. */
function publicView(c) {
  const { passHash, passSalt, apiKey, ...rest } = c;
  return { ...rest, hasPasscode: !!passHash, expired: isExpired(c) };
}

export function isExpired(c) {
  return !c || !!c.revoked || (!!c.expiresAt && Date.now() >= c.expiresAt);
}

export function list() {
  return loadAll().map(publicView);
}

/**
 * The scoped API key for an invitation.
 *
 * Separate from get() because publicView deliberately strips it: the key is a
 * credential, and a route that returns a collaborator record must not be one
 * keystroke away from returning their key too.
 */
export function apiKeyOf(id) {
  const c = loadAll().find(x => x.id === id);
  return c && !isExpired(c) ? (c.apiKey || '') : '';
}

export function get(id) {
  const c = loadAll().find(x => x.id === id);
  return c ? publicView(c) : null;
}

/**
 * Create a collaborator and their worktree.
 *
 * @param {object} o
 * @param {string} o.name             display name, also the branch slug
 * @param {string} o.project          project alias to share
 * @param {number} [o.hours]           how long access lasts, in hours
 * @param {number} [o.days]            same, in days (hours wins if both given)
 * @param {string} [o.telegram]       @username, without the @
 * @param {boolean} [o.withPasscode]  also issue a passcode
 */
export async function createMany({ name, projects = [], days, hours, telegram = '', withPasscode = true } = {}) {
  const list = [...new Set((projects || []).map(p => String(p || '').toLowerCase()).filter(Boolean))];
  if (!list.length) return { ok: false, error: 'Pick at least one project' };
  const made = [];
  const failed = [];
  const needsGit = [];
  let passcode = '';
  for (const p of list) {
    // Sequential, not parallel: each create consults the records written by the
    // one before it to decide whether this person already has a passcode, and
    // running them together would issue several.
    const r = await create({ name, project: p, days, hours, telegram, withPasscode });
    if (r.ok) { made.push(r.collaborator); if (r.passcode) passcode = r.passcode; }
    else { failed.push(`${p}: ${r.error}`); if (r.needsGit) needsGit.push(p); }
  }
  if (!made.length) return { ok: false, error: failed.join('; '), needsGit };
  return { ok: true, collaborators: made, passcode, failed, needsGit };
}

export async function create({ name, project, days, hours, telegram = '', withPasscode = true } = {}) {
  const nm = String(name || '').trim();
  if (!nm) return { ok: false, error: 'A name is required' };
  if (!getProject(project)) return { ok: false, error: `Project "${project}" not found` };
  const tg = String(telegram || '').trim().replace(/^@/, '').toLowerCase();
  if (!tg && !withPasscode) return { ok: false, error: 'Give them a Telegram username, a passcode, or both — otherwise there is no way in.' };

  const ttl = toHours({ hours, days });

  const wt = await provisionWorktree(project, nm);
  if (!wt.ok) return wt;

  // A person invited to a second project keeps the passcode they already
  // have. Issuing a new one per project would mean handing someone three
  // phrases for three folders and hoping they file them correctly, and any of
  // them would reach all three anyway — the session is bound to the person.
  const identity = tg ? `tg:${tg}` : `pc:${nm.toLowerCase()}`;
  const sibling = loadAll().find(c => !isExpired(c) && identityOf(c) === identity && c.passHash);
  let passcode = '';
  let pass = { salt: '', hash: '' };
  if (sibling) {
    pass = { salt: sibling.passSalt, hash: sibling.passHash };
  } else if (withPasscode) {
    passcode = generatePasscode();
    pass = hashPasscode(passcode);
  }

  const rec = {
    id: genId(),
    name: nm,
    project: String(project).toLowerCase(),
    telegram: tg,
    passSalt: pass.salt,
    passHash: pass.hash,
    // Distinct from the internal key on purpose: this one is checked against
    // COLLABORATOR_MCP_TOOLS, the internal one reaches everything.
    apiKey: sibling ? sibling.apiKey : randomBytes(32).toString('hex'),
    worktreePath: wt.path,
    branch: wt.branch,
    baseCommit: wt.baseCommit,
    createdAt: Date.now(),
    expiresAt: Date.now() + ttl * 60 * 60 * 1000,
    ttlHours: ttl,           // what was asked for, for "extend by the same again"

    revoked: false,
    seenAt: 0,
    briefed: false,   // has the first-login orientation been delivered
  };

  const all = loadAll();
  all.push(rec);
  saveAll(all);
  // The passcode is returned exactly once. It is not recoverable afterwards —
  // only re-issuable — which is the property that makes storing a hash worth
  // anything at all.
  return {
    ok: true,
    collaborator: publicView(rec),
    passcode,
    // Empty passcode with sharesExisting set means "they already have one" —
    // otherwise the UI would show a blank box and imply something went wrong.
    sharesExisting: !!sibling,
  };
}

export function update(id, patch = {}) {
  const all = loadAll();
  const c = all.find(x => x.id === id);
  if (!c) return { ok: false, error: 'No such collaborator' };
  if (patch.name !== undefined) c.name = String(patch.name).trim() || c.name;
  if (patch.telegram !== undefined) c.telegram = String(patch.telegram).trim().replace(/^@/, '').toLowerCase();
  if (patch.days !== undefined || patch.hours !== undefined) {
    // Extending is measured from NOW, not from the old expiry: "give them
    // another day" said on Tuesday means Wednesday, not a day after whenever
    // the original happened to run out.
    const ttl = toHours({ hours: patch.hours, days: patch.days });
    c.expiresAt = Date.now() + ttl * 60 * 60 * 1000;
    c.ttlHours = ttl;
  }
  if (patch.revoked !== undefined) c.revoked = !!patch.revoked;
  saveAll(all);
  return { ok: true, collaborator: publicView(c) };
}

/** Issue a fresh passcode, invalidating the old one. */
export function reissuePasscode(id) {
  const all = loadAll();
  const c = all.find(x => x.id === id);
  if (!c) return { ok: false, error: 'No such collaborator' };
  const passcode = generatePasscode();
  const p = hashPasscode(passcode);
  // One passcode per PERSON, so re-issuing has to move every invitation they
  // hold — otherwise the new phrase would open one project and the old one
  // would still open the rest, which is the opposite of what re-issuing is for.
  const key = identityOf(c);
  for (const x of all) {
    if (identityOf(x) === key) { x.passSalt = p.salt; x.passHash = p.hash; }
  }
  saveAll(all);
  return { ok: true, passcode, name: c.name };
}

export async function remove(id) {
  const all = loadAll();
  const i = all.findIndex(x => x.id === id);
  if (i < 0) return { ok: false, error: 'No such collaborator' };
  const c = all[i];
  const cleanup = await removeWorktree(c.project, c);
  all.splice(i, 1);
  saveAll(all);
  return { ok: true, notes: cleanup.notes || [] };
}

export function markSeen(idOrIdentity) {
  const all = loadAll();
  const hits = all.filter(x => x.id === idOrIdentity || identityOf(x) === idOrIdentity);
  if (!hits.length) return;
  for (const c of hits) c.seenAt = Date.now();
  saveAll(all);
}

/**
 * Mark a person as having seen the orientation. Returns true the first time.
 *
 * Keyed by the person, not the invitation: someone added to a second project
 * has already read it, and showing it again would teach them to skip it.
 */
export function markBriefed(id) {
  const all = loadAll();
  const rec = all.find(x => x.id === id);
  if (!rec) return false;
  const key = identityOf(rec);
  const mine = all.filter(x => identityOf(x) === key);
  if (mine.some(x => x.briefed)) return false;
  for (const c of mine) c.briefed = true;
  saveAll(all);
  return true;
}

// ─── Lookups used by the auth paths ───

/** The live record behind a session, or null if it lapsed. Checked per request. */
export function active(id) {
  const c = loadAll().find(x => x.id === id);
  return c && !isExpired(c) ? c : null;
}

/** The identity key behind a Telegram username, if it has any live invitation. */
export function identityByTelegram(username) {
  const u = String(username || '').replace(/^@/, '').toLowerCase();
  if (!u) return null;
  return loadAll().some(x => !isExpired(x) && x.telegram === u) ? `tg:${u}` : null;
}

/**
 * Authenticate a name + passcode pair, returning the identity key.
 *
 * Checks the passcode against EVERY live invitation under that name, because
 * a person's invitations share a passcode and any of their records carries a
 * valid hash for it.
 */
export function identityByPasscode(name, passcode) {
  const n = String(name || '').trim().toLowerCase();
  if (!n || !passcode) return null;
  const candidates = loadAll().filter(x => !isExpired(x) && x.name.toLowerCase() === n);
  const hit = candidates.find(c => verifyPasscode(passcode, c));
  return hit ? identityOf(hit) : null;
}

/** The identity behind a collaborator API key. */
export function identityByApiKey(key) {
  if (!key) return null;
  const c = loadAll().find(x => x.apiKey && x.apiKey === key && !isExpired(x));
  return c ? identityOf(c) : null;
}

// ─── Reclaiming disk ───
//
// A worktree is a full checkout, so a handful of lapsed collaborators is real
// disk. Two kinds are safe to delete:
//
//   orphan  — a directory under the worktree root that no record points at,
//             left behind by a crash mid-create or a hand-edited file
//   lapsed  — the collaborator expired or was revoked, so nobody can reach it
//
// Neither deletes a BRANCH. The checkout is reproducible from the branch; the
// commits on it are not reproducible from anything. Losing someone's work
// because their week ran out would be a bug, not a cleanup.

/** Directories under the worktree root that are safe to delete, with sizes. */
export function staleWorktrees() {
  const all = loadAll();
  const byPath = new Map(all.filter(c => c.worktreePath).map(c => [c.worktreePath, c]));
  let dirs = [];
  try {
    dirs = readdirSync(ROOT(), { withFileTypes: true })
      .filter(d => d.isDirectory())
      .map(d => join(ROOT(), d.name));
  } catch { return []; }

  const out = [];
  for (const path of dirs) {
    const c = byPath.get(path);
    if (c && !isExpired(c)) continue;          // in use by a live collaborator
    out.push({
      path,
      bytes: dirBytes(path),
      reason: c ? (c.revoked ? 'revoked' : 'expired') : 'orphan',
      name: c ? c.name : '',
      project: c ? c.project : '',
      collaboratorId: c ? c.id : '',
      branch: c ? c.branch : '',
    });
  }
  return out;
}

/** Recursive byte count. Cheap enough for a handful of checkouts. */
function dirBytes(path) {
  let total = 0;
  const walk = (p) => {
    let ents = [];
    try { ents = readdirSync(p, { withFileTypes: true }); } catch { return; }
    for (const e of ents) {
      const full = join(p, e.name);
      if (e.isSymbolicLink()) continue;        // never follow: a link out would double-count the host
      if (e.isDirectory()) walk(full);
      else { try { total += statSync(full).size; } catch { /* vanished mid-walk */ } }
    }
  };
  walk(path);
  return total;
}

/**
 * Delete every stale worktree, returning the bytes recovered.
 *
 * Goes through `git worktree remove` where a record still names the project,
 * so git's own bookkeeping is updated rather than left pointing at a directory
 * that is no longer there; falls back to a plain delete plus `worktree prune`.
 */
export async function reclaimStale() {
  const stale = staleWorktrees();
  let freed = 0;
  const notes = [];
  const all = loadAll();
  for (const s of stale) {
    const rec = all.find(c => c.id === s.collaboratorId);
    const before = s.bytes;
    if (rec) {
      const r = await removeWorktree(rec.project, rec);
      for (const n of (r.notes || [])) notes.push(n);
      rec.worktreePath = '';            // the branch stays; the checkout does not
    } else {
      try { rmSync(s.path, { recursive: true, force: true }); } catch { /* ignore */ }
    }
    if (!existsSync(s.path)) freed += before;
    else notes.push(`could not remove ${s.path}`);
  }
  if (stale.some(s => s.collaboratorId)) saveAll(all);
  return { ok: true, freed, count: stale.length, notes };
}

export const worktreeRoot = ROOT;
