#!/usr/bin/env node
/**
 * test-waiting-state.mjs — replay a REAL parked turn through the real handlers.
 *
 * This exists because the "waiting" badge was fixed wrongly twice, both times
 * because the fix was tested against hand-built messages that matched what the
 * author assumed the wire looked like.
 *
 * test/fixtures/parked-turn.stream.jsonl is not hand-written. It is a captured
 * `claude --output-format stream-json --input-format stream-json` session that
 * launched a background command, ended its turn, and then resumed by itself
 * when the command finished. It contains the one fact the whole feature turns
 * on: the <task-notification> announcing completion is written to the CLI's own
 * transcript but is NEVER emitted over stream-json. Nothing Crundi receives
 * announces that a background task finished.
 *
 * The expected sequence is therefore:
 *   working -> waiting -> working -> idle
 * with nothing left outstanding. Ending on "waiting" is the bug.
 *
 * Run: node scripts/test-waiting-state.mjs
 */

import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
// Split on either line ending. A Windows checkout gives CRLF, and comparing
// those lines against '  }' matches nothing — which turned the scan below into
// an infinite loop and hung every Windows CI job for 25 minutes before it was
// caught. Normalise once, here.
const lines = readFileSync(join(root, 'src', 'claude-ui.js'), 'utf8').split(/\r?\n/);
const src = lines.join('\n');

// The handlers are closures inside createClaudeUi, so they are lifted out by
// position rather than imported. Anchored on declarations, which move far less
// often than line numbers.
const at = (needle) => {
  const i = lines.findIndex((l) => l.includes(needle));
  if (i < 0) throw new Error(`Could not find "${needle}" in claude-ui.js — the test needs updating.`);
  return i;
};
const iAssist = at('  function handleAssistant(s, msg) {');
const iBlocks = at("   * A message's content blocks") - 1;
const iUser = at('  function handleUser(s, msg) {');
const iTrig = at('  // ─── Waiting on a trigger ───');
const iResult = at('  function handleResult(s, msg) {');
// Bounded regardless: a scan with no stopping condition is a hang waiting to
// happen, and a test that hangs is worse than a test that fails.
let iEnd = iResult + 1;
while (iEnd < lines.length && lines[iEnd] !== '  }') iEnd++;
if (iEnd >= lines.length) throw new Error('Could not find the end of handleResult — the test needs updating.');
const seg = (a, b) => lines.slice(a, b).join('\n');
const setStateFn = src.match(/function setState\(s, state\) \{[^]*?\n {2}\}/)[0];

const { handleMessage } = new Function(`
  const MAX_TEXT = 4000, MAX_MESSAGES = 500;
  let n = 0; const nextSeq = () => ++n; const genId = () => 'i' + (++n);
  function handleAgentAssistant(){} function handleAgentUser(){} function handleHookFeedback(){}
  function patchEntry(){} function emitGoal(){} function emitEntry(){}
  ${setStateFn}
  ${seg(iAssist, iBlocks)}
  ${seg(iBlocks, iUser)}
  ${seg(iUser, iTrig)}
  ${seg(iTrig, iResult)}
  ${seg(iResult, iEnd + 1)}
  function handleMessage(s, msg) {
    switch (msg.type) {
      case 'assistant': return handleAssistant(s, msg);
      case 'user':      return handleUser(s, msg);
      case 'result':    return handleResult(s, msg);
    }
  }
  return { handleMessage };
`)();

const states = [];
const session = {
  state: 'idle', waiting: new Map(), messages: [], blocks: new Map(), streamedQueue: [],
  goal: null, emitter: { emit() {} }, onStateChange: (_id, st) => states.push(st),
};

const fixture = readFileSync(join(root, 'test', 'fixtures', 'parked-turn.stream.jsonl'), 'utf8');
let replayed = 0;
for (const line of fixture.split(/\r?\n/)) {
  if (!line.trim().startsWith('{')) continue;
  handleMessage(session, JSON.parse(line));
  replayed++;
}

const seen = states.join(' -> ');
const want = 'working -> waiting -> working -> idle';
let failures = 0;
const check = (label, ok, detail) => {
  if (!ok) failures++;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${label}${detail === undefined ? '' : ` -> ${detail}`}`);
};

console.log(`replayed ${replayed} real wire messages`);
check('state sequence', seen === want, seen);
check('does not end stuck on waiting', session.state === 'idle', session.state);
check('nothing left outstanding', session.waiting.size === 0, session.waiting.size);

if (failures) {
  console.error(`\n${failures} failure(s). Expected: ${want}`);
  process.exit(1);
}
console.log('\nParked turns settle correctly.');
