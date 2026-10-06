/**
 * restore-apply.js — finish a restore at startup, before anything loads.
 *
 * backup.js downloads and decrypts a backup into <appDir>/.restore-staging and
 * marks it READY. The running server cannot swap its own files safely (its
 * stores hold them in memory and save them again on the way out), so the swap
 * happens here, first thing on the next start: index.js imports this module
 * before config.js reads the .env.
 *
 * What is replaced goes to <appDir>/pre-restore-<time>/ first, so a restore
 * can be undone by hand. Things the backup does not carry (the browser
 * profile, worktrees, logs) are left where they are. Claude transcripts are
 * added back only where no file of that name exists.
 *
 * Works the same on Windows: paths come from paths.js, and a move between
 * drives falls back to copy-and-delete.
 */

import { existsSync, readFileSync, writeFileSync, readdirSync, renameSync, cpSync, rmSync, mkdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { homedir } from 'node:os';
import dotenv from 'dotenv';
import { defaultAppDir, resolveEnvPath } from './paths.js';

const KEEP_SAFETY_COPIES = 3;

function move(from, to) {
  mkdirSync(dirname(to), { recursive: true });
  try { renameSync(from, to); }
  catch (err) {
    if (err.code !== 'EXDEV' && err.code !== 'EPERM' && err.code !== 'EBUSY') throw err;
    cpSync(from, to, { recursive: true, force: true });
    rmSync(from, { recursive: true, force: true });
  }
}

export function applyStagedRestore({ appDir = defaultAppDir(), log = console.log } = {}) {
  const stage = join(appDir, '.restore-staging');
  const ready = join(stage, 'READY');
  if (!existsSync(ready)) return null;
  let info = {};
  try { info = JSON.parse(readFileSync(ready, 'utf-8')); } catch { /* still apply */ }

  const envPath = resolveEnvPath(appDir);
  // The data dir as this install has it, not as the backed-up machine had it.
  let dataDirSetting = '';
  if (existsSync(envPath)) {
    try { dataDirSetting = dotenv.parse(readFileSync(envPath)).DATA_DIR || ''; } catch { /* ignore */ }
  }
  const dataDir = process.env.DATA_DIR || dataDirSetting || join(appDir, 'data');

  const when = new Date().toISOString().replace(/[:.]/g, '-');
  const safety = join(appDir, 'pre-restore-' + when);
  mkdirSync(safety, { recursive: true });
  let replaced = 0, transcripts = 0, skills = 0;

  // .env (sign-in and settings). Its DATA_DIR named the other machine's
  // folder; keep this install's.
  const stagedEnv = join(stage, 'env', '.env');
  if (existsSync(stagedEnv)) {
    if (existsSync(envPath)) move(envPath, join(safety, '.env'));
    let text = readFileSync(stagedEnv, 'utf-8');
    if (/^DATA_DIR=.*$/m.test(text)) text = text.replace(/^DATA_DIR=.*$/m, 'DATA_DIR=' + dataDirSetting);
    mkdirSync(dirname(envPath), { recursive: true });
    writeFileSync(envPath, text, { mode: 0o600 });
    replaced++;
  }

  // Data: each top-level file or folder in the backup replaces its namesake.
  const stagedData = join(stage, 'data');
  if (existsSync(stagedData)) {
    mkdirSync(dataDir, { recursive: true });
    for (const name of readdirSync(stagedData)) {
      const target = join(dataDir, name);
      if (existsSync(target)) move(target, join(safety, 'data', name));
      move(join(stagedData, name), target);
      replaced++;
    }
  }

  // Claude transcripts: added back, never overwriting.
  const stagedClaude = join(stage, 'claude');
  if (existsSync(stagedClaude)) {
    const root = join(homedir(), '.claude', 'projects');
    for (const dir of readdirSync(stagedClaude)) {
      for (const f of readdirSync(join(stagedClaude, dir))) {
        const dest = join(root, dir, f);
        if (existsSync(dest)) continue;
        mkdirSync(join(root, dir), { recursive: true });
        try { move(join(stagedClaude, dir, f), dest); transcripts++; } catch { /* skip one */ }
      }
    }
  }

  // Global skills: added back whole, and never over one that is already here.
  // A skill on this machine may be newer than the backup, or be the copy
  // Crundi itself installs; either way it is not the restore's to replace.
  const stagedSkills = join(stage, 'skills');
  if (existsSync(stagedSkills)) {
    const root = join(homedir(), '.claude', 'skills');
    for (const name of readdirSync(stagedSkills)) {
      const dest = join(root, name);
      if (name.startsWith('.') || name === 'synced' || existsSync(dest)) continue;
      try { mkdirSync(root, { recursive: true }); move(join(stagedSkills, name), dest); skills++; } catch { /* skip one */ }
    }
  }

  rmSync(stage, { recursive: true, force: true });

  // Only the newest few safety copies are kept.
  try {
    const olds = readdirSync(appDir).filter(n => n.startsWith('pre-restore-')).sort().reverse();
    for (const n of olds.slice(KEEP_SAFETY_COPIES)) rmSync(join(appDir, n), { recursive: true, force: true });
  } catch { /* ignore */ }

  const result = {
    at: Date.now(), key: info.key || '', backupCreatedAt: info.manifest?.createdAt || '',
    fromHost: info.manifest?.host || '', replaced, transcripts, skills, safetyCopy: safety,
  };
  try { writeFileSync(join(appDir, 'restore-result.json'), JSON.stringify(result, null, 2)); } catch { /* ignore */ }
  log(`[restore] Restored the backup made ${result.backupCreatedAt || '(unknown time)'} on ${result.fromHost || 'another machine'}: ${replaced} item(s) replaced, ${transcripts} transcript(s) and ${skills} skill(s) added. The previous files are in ${safety}`);
  return result;
}

// Run on import: index.js imports this first.
if (!process.env.CRUNDI_NO_RESTORE_APPLY) {
  try { applyStagedRestore(); }
  catch (err) { console.error('[restore] Could not finish the restore:', err.message, '— the backup is still staged and will be tried again at the next start.'); }
}
