#!/usr/bin/env node
/**
 * test-scheduled-chat-notify.mjs — one run of a chat schedule, one report.
 *
 * A single run of "Claude Code changelog watch" reached Telegram four times:
 *
 *   09:00  Claude's own message                (the prompt asks for it — wanted)
 *   09:01  ✅ <name> (crundi) + last reply       generic "Claude finished" ping
 *   09:01  ✅ "<name>" finished + SAME reply     the runner's report
 *   09:01  ⏰ Schedule "<name>" ran              the scheduler's fire ping
 *
 * The runner's report is the one meant to speak for a chat schedule. The
 * generic ping fired because the scheduler passed background:true and nothing
 * stored it; the fire ping fired for every schedule regardless of kind.
 *
 * Removing the fire ping made the runner the ONLY thing reporting a chat
 * schedule — so it must also report the runs that never started, which it used
 * to return from silently.
 *
 * Run: node scripts/test-scheduled-chat-notify.mjs
 */

import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { runScheduledChat } from '../src/scheduled-chat.js';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const read = (...p) => readFileSync(join(root, ...p), 'utf8');

let failures = 0;
const check = (label, ok, detail) => {
  if (!ok) failures++;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${label}${detail === undefined ? '' : ` -> ${detail}`}`);
};

/** A claudeUi that plays back a scripted sequence of states. */
function fakeUi({ createOk = true, states = [] } = {}) {
  const calls = { close: 0, background: [], created: null };
  let handler = null;
  return {
    calls,
    async create(project, opts) { calls.created = opts; return createOk ? { ok: true, id: 'job1' } : { ok: false, error: 'boom' }; },
    on(id, h) { handler = h; },
    off() { handler = null; },
    sendMessage() {
      let t = 0;
      for (const st of states) { t += 5; setTimeout(() => handler && handler({ type: 'state', state: st }), t); }
    },
    lastTurnOutput() { return 'the reply'; },
    close() { calls.close++; return { ok: true }; },
    setBackground(id, on) { calls.background.push(on); return { ok: true }; },
  };
}

const schedule = (prompt = 'do the thing') => ({ name: 'Job', project: 'p', action: { kind: 'chat', prompt } });

async function run(ui, sch = schedule()) {
  const sent = [];
  const r = await runScheduledChat({ schedule: sch, claudeUi: ui, settleMs: 20, maxRunMs: 2000,
    notify: (outcome, label, text) => sent.push({ outcome, text }) });
  return { r, sent };
}

// A run that finishes cleanly.
{
  const ui = fakeUi({ states: ['working', 'idle'] });
  const { r, sent } = await run(ui);
  check('a finished run reports exactly once', sent.length === 1 && sent[0].outcome === 'finished', JSON.stringify(sent));
  check('  and closes its chat', ui.calls.close === 1);
  check('  and is created as a background job', ui.calls.created && ui.calls.created.background === true);
  check('  outcome finished', r.outcome === 'finished');
}

// A run that stops to wait for you: reported once, then handed back.
{
  const ui = fakeUi({ states: ['working', 'needs-input'] });
  const { sent } = await run(ui);
  check('a run waiting for you reports exactly once', sent.length === 1 && sent[0].outcome === 'needs-input', JSON.stringify(sent));
  check('  and hands the chat back (background cleared)', ui.calls.background.length === 1 && ui.calls.background[0] === false);
  check('  and leaves it open', ui.calls.close === 0);
}

// A run that could not start used to return silently.
{
  const ui = fakeUi({ createOk: false });
  const { sent } = await run(ui);
  check('a run that could not start is reported', sent.length === 1 && sent[0].outcome === 'start-failed', JSON.stringify(sent));
}
{
  const ui = fakeUi();
  const { sent } = await run(ui, schedule(''));
  check('a schedule with no prompt is reported', sent.length === 1 && sent[0].outcome === 'no-prompt', JSON.stringify(sent));
}

// The other two senders, which must stay quiet for a chat schedule.
const webapp = read('src', 'webapp.js');
const index = read('src', 'index.js');
const ui = read('src', 'claude-ui.js');
check('the generic "Claude finished" ping skips background sessions',
  /cur === 'needs-input'\) && !term\.background(?: && !term\.collaborator)?\)/.test(webapp));
check('sessions actually store the background flag', /background: !!background,/.test(ui));
check('and expose it to the ping', /background: !!s\.background,/.test(ui));
check('"Schedule ran" is not sent for chat schedules',
  /onFire: \(sch\) => \{[\s\S]{0,300}?if \(sch\.action && sch\.action\.kind === 'chat'\) return;/.test(index));

// Collaborator chats outliving their access.
check('revoking closes their open chats', /case 'revoke': \{[\s\S]{0,500}?closeLapsedCollaboratorChats\(\);/.test(webapp));
check('removing closes their chats before the worktree goes',
  /case 'remove': \{[\s\S]{0,300}?closeLapsedCollaboratorChats\(body\.id\);\s*\n\s*const r = await collaborators\.remove/.test(webapp));
check('expiry is swept, since nothing happens when a timestamp passes',
  /setInterval\(\(\) => \{\s*\n\s*try \{ closeLapsedCollaboratorChats\(\); \}/.test(webapp));

console.log(failures ? `\n${failures} failure(s)` : '\nAll scheduled-chat notify checks passed.');
process.exit(failures ? 1 : 0);
