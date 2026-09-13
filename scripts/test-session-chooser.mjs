#!/usr/bin/env node
/**
 * test-session-chooser.mjs — every Claude launch asks which conversation.
 *
 * Before: UI Mode quietly continued the newest conversation (asking only when
 * it was heavy), Terminal did the same without asking at all, and choosing a
 * specific earlier conversation was a separate "Resume a conversation…" button
 * whose chat/terminal tabs GUESSED the mode from the last launch — dropping a
 * skip-permissions choice on the floor. The heavy prompt's "Continue anyway"
 * used --continue, which attaches to the NEWEST transcript, not the one picked.
 *
 * Now: every Claude button opens the chooser in exactly its own mode, with a
 * New session option first. A conversation already open elsewhere still
 * opens — the server forks it (--fork-session) rather than letting two
 * processes write one transcript.
 *
 * Run: node scripts/test-session-chooser.mjs
 */

import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const read = (...p) => readFileSync(join(root, ...p), 'utf8');
const html = read('src', 'webapp-html.js');
const ui = read('src', 'claude-ui.js');
const terms = read('src', 'claude-terminals.js');

let failures = 0;
const check = (label, ok, detail) => {
  if (!ok) failures++;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${label}${detail === undefined ? '' : ` -> ${detail}`}`);
};

// ── The launcher ──────────────────────────────────────────────────────────
check('Claude launch buttons open the chooser', /openSessionChooser\(d\.lid, d\.mode, btn\);/.test(html));
check('only an empty shell skips it', /if \(d\.mode === 'shell'\) \{[\s\S]{0,160}?launchTerminal\('shell', d\.lid, undefined, undefined, rel\);/.test(html));

// Every other way into launchTerminal must carry an explicit choice. A call
// with no session mode is how "quietly continue the newest" came back before.
// Calls only: the definition is excluded, and arguments cannot span braces or
// lines — an earlier version of this regex ran from the definition's "(" into
// its body and reported the function itself as an unchosen launch.
const calls = [...html.matchAll(/(?<!function )launchTerminal\(([^;{}\n]*?)\);/g)].map(m => m[1]);
const unchosen = calls.filter(a => !/^'shell'/.test(a) && !/'new'|'resume'|'compact'/.test(a));
check('found the launch call sites at all', calls.length >= 5, calls.length);
check('no Claude launch reaches the server without a chosen session', unchosen.length === 0, JSON.stringify(unchosen));

// ── The old paths ─────────────────────────────────────────────────────────
for (const gone of ['launchChatWithPreflight', 'showResumeChoice', 'openChatResume', 'crMode', 'data-action="chat-resume"']) {
  check(`removed: ${gone}`, !html.includes(gone));
}

// ── The chooser ───────────────────────────────────────────────────────────
check('it keeps the pressed mode, including skip permissions', /crLaunchMode = CR_MODE_LABEL\[mode\] \? mode : 'chat';/.test(html)
  && /'chat-skip': 'UI Mode/.test(html) && /'skip': 'Terminal/.test(html));
check('New session is offered first', /let h = '<button class="cr-item cr-new" data-action="cr-new">'/.test(html));
check('New session starts fresh', /case 'cr-new': \{[\s\S]{0,260}?launchTerminal\(mode, lid, '', 'new', rel\);/.test(html));
check('with nothing to resume it starts new instead of asking', /!crList\.length\) \{[\s\S]{0,160}?launchTerminal\(crLaunchMode, localId, '', 'new', rel\)/.test(html));
check('an open conversation is labelled as opening a copy', /s\.inUse \? .{0,80}open elsewhere/.test(html));
check('picking a conversation resumes that one', /launchTerminal\(mode, lid, s\.id, 'resume', rel\);/.test(html));
check('a heavy one in UI Mode offers compact or full — for the PICKED id',
  /data-how="compact" data-sid="' \+ escHtml\(s\.id\)/.test(html)
  && /launchTerminal\(mode, lid, d\.sid, d\.how === 'compact' \? 'compact' : 'resume', rel\);/.test(html));

// ── Already open elsewhere: the server forks ──────────────────────────────
check('a chat resuming a claimed conversation forks', /if \(isSessionClaimed\(String\(resumeId\)\)\) forking = true;/.test(ui)
  && /if \(forking\) args\.push\('--fork-session'\);/.test(ui));
check('a terminal resuming a claimed conversation forks', /const forking = !!attachedTo && isSessionClaimed\(attachedTo\);/.test(terms)
  && /if \(forking\) flagList\.push\('--fork-session'\);/.test(terms));

console.log(failures ? `\n${failures} failure(s)` : '\nAll session-chooser checks passed.');
process.exit(failures ? 1 : 0);
