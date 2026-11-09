/**
 * attribution.js — sign the commits and PRs Claude makes from Crundi.
 *
 * Claude Code ends every commit with "Co-Authored-By: Claude …" and every PR
 * with "Generated with Claude Code". Its `attribution` setting replaces those
 * texts, so the sessions Crundi starts pass one saying the work was made with
 * Crundi instead.
 *
 * The person's own choice wins. Someone who set `attribution` (or the older
 * `includeCoAuthoredBy`) in their user or project settings has already said
 * what they want — often that they want nothing — and a flag from Crundi would
 * silently override it, because --settings outranks every file but managed
 * policy. So Crundi only adds its own when none of those files mention it.
 *
 * Passed as a file, not inline JSON: a terminal on Windows goes through cmd,
 * which mangles the double quotes JSON is made of.
 */

import { readFileSync, writeFileSync, mkdirSync } from 'fs';
import { join } from 'path';
import { homedir } from 'os';

const SITE = 'https://crundi.faruhaan.com';

export const CRUNDI_ATTRIBUTION = Object.freeze({
  commit: `Made with Crundi (${SITE})`,
  pr: `Made with [Crundi](${SITE})`,
});

function readJson(file) {
  try { return JSON.parse(readFileSync(file, 'utf-8')); } catch { return null; }
}

/** The settings files Claude Code would read for a session in `cwd`. */
function settingsFiles(cwd) {
  const userDir = process.env.CLAUDE_CONFIG_DIR || join(homedir(), '.claude');
  const files = [join(userDir, 'settings.json')];
  if (cwd) files.push(join(cwd, '.claude', 'settings.json'), join(cwd, '.claude', 'settings.local.json'));
  return files;
}

/** Has the person set attribution themselves, anywhere this session would see it? */
export function userSetsAttribution(cwd) {
  return settingsFiles(cwd).some((f) => {
    const s = readJson(f);
    return !!s && typeof s === 'object' && (s.attribution !== undefined || s.includeCoAuthoredBy !== undefined);
  });
}

/** Crundi's attribution for a session in `cwd`, or null when the person has their own. */
export function attributionFor(cwd) {
  return userSetsAttribution(cwd) ? null : { ...CRUNDI_ATTRIBUTION };
}

/**
 * The flags that apply it: ['--settings', <file>] or [] when the person has
 * their own. The file is rewritten each time so a change of text ships with an
 * update of Crundi rather than waiting on a stale copy.
 */
export function attributionArgs(cwd, dataDir) {
  const attribution = attributionFor(cwd);
  if (!attribution || !dataDir) return [];
  try {
    mkdirSync(dataDir, { recursive: true });
    const file = join(dataDir, 'claude-attribution.json');
    writeFileSync(file, JSON.stringify({ attribution }, null, 2));
    return ['--settings', file];
  } catch {
    // Unsigned commits are no reason to refuse to start a session.
    return [];
  }
}
