#!/usr/bin/env node
/**
 * test-session-claims.mjs — who owns which conversation, and when we fork.
 *
 * The failure this prevents leaves no trace at the time it happens. Two Claude
 * processes resuming one transcript both append to the same file; neither
 * errors, neither warns, and the turns that lost the race are simply not there
 * the next time anyone reads it back. It surfaces days later as "it forgot".
 *
 * So the rules are tested directly: a claim is held for exactly as long as the
 * process lives, a fork claims nothing (it is not attached to the original),
 * and releasing is keyed to the owner rather than the id — because two owners
 * CAN legitimately hold the same id once one of them has forked away.
 *
 * Run: node scripts/test-session-claims.mjs
 */

import { mkdtempSync, rmSync, mkdirSync, writeFileSync, utimesSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// newestTranscriptId reads ~/.claude/projects; os.homedir() honours $HOME on
// POSIX, so point it at a scratch tree rather than the real one.
const home = mkdtempSync(join(tmpdir(), 'crundi-home-'));
process.env.HOME = home;

const { claimSession, releaseSession, isSessionClaimed, claimedSessionIds, _reset }
  = await import('../src/claude-sessions.js');
const { newestTranscriptId } = await import('../src/claude-terminals.js');

let failures = 0;
const check = (label, ok, detail) => {
  if (!ok) failures++;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${label}${detail === undefined ? '' : ` -> ${detail}`}`);
};

// ─── Claims ───

_reset();
check('nothing is claimed to begin with', !isSessionClaimed('abc'));
check('an empty id is never claimed', !isSessionClaimed(''));

claimSession('chat:1', 'abc');
check('a claimed id reads as claimed', isSessionClaimed('abc'));
check('the claimant does not collide with itself', !isSessionClaimed('abc', 'chat:1'));
check('but anyone else does', isSessionClaimed('abc', 'term:9'));
check('an unrelated id is unaffected', !isSessionClaimed('def'));

// Re-claiming moves an owner rather than accumulating: a chat that forked and
// learned its real id at system/init must stop claiming the parent, or the
// parent stays locked for the rest of the process's life.
claimSession('chat:1', 'xyz');
check('re-claiming releases the previous id', !isSessionClaimed('abc'));
check('and holds the new one', isSessionClaimed('xyz'));

// Two owners on one id is legitimate state — it is what a missed claim looks
// like — so release must be per-owner, not per-id.
claimSession('term:2', 'xyz');
releaseSession('chat:1');
check('releasing one owner leaves the other holding it', isSessionClaimed('xyz'));
releaseSession('term:2');
check('releasing the last owner frees it', !isSessionClaimed('xyz'));

claimSession('chat:3', '');
check('claiming an empty id claims nothing', claimedSessionIds().size === 0, claimedSessionIds().size);

claimSession('chat:4', 'p');
claimSession('term:5', 'q');
check('the picker can see every claimed id', claimedSessionIds().size === 2);
releaseSession('nobody');
check('releasing an unknown owner is harmless', claimedSessionIds().size === 2);

// ─── newestTranscriptId ───
//
// This is what a terminal claims when it passes --continue, since a PTY never
// gets told its session id. Wrong here means claiming a conversation that is
// not the one being opened.

const projectPath = join(home, 'proj');
mkdirSync(projectPath, { recursive: true });
const encoded = projectPath.replace(/[^a-zA-Z0-9]/g, '-');
const sessDir = join(home, '.claude', 'projects', encoded);
mkdirSync(sessDir, { recursive: true });

check('no transcripts means no id', newestTranscriptId(projectPath) === '', newestTranscriptId(projectPath));

const put = (name, ageSeconds, body = '{"cwd":"' + projectPath + '"}\n') => {
  const f = join(sessDir, name + '.jsonl');
  writeFileSync(f, body);
  const t = Date.now() / 1000 - ageSeconds;
  utimesSync(f, t, t);
};

put('older-session', 600);
put('newest-session', 5);
put('middle-session', 300);
check('the newest transcript wins', newestTranscriptId(projectPath) === 'newest-session',
  newestTranscriptId(projectPath));

// An empty file is a session that never got anywhere; --continue skips it, and
// claiming it would leave the conversation actually opened unprotected.
put('empty-session', 1, '');
check('a zero-byte transcript is ignored', newestTranscriptId(projectPath) === 'newest-session',
  newestTranscriptId(projectPath));

check('a directory with no session tree at all is not an error',
  newestTranscriptId(join(home, 'nothing-here')) === '');

rmSync(home, { recursive: true, force: true });

if (failures) {
  console.error(`\n${failures} failure(s).`);
  process.exit(1);
}
console.log('\nOne live process per conversation; the rest fork.');
