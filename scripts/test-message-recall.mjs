#!/usr/bin/env node
/**
 * test-message-recall.mjs — taking back a message the CLI has not read yet.
 *
 * Crundi used to tell the user a handed-over message "can no longer be taken
 * back". That was never a CLI limitation; it was an assumption. The CLI exposes
 * exactly this, and its own descriptions define the contract:
 *
 *   cancel_async_message  "Drops a pending async user message from the command
 *                          queue by uuid. No-op if already dequeued."
 *   result                "cancelled=false means the message was not in the
 *                          queue (already dequeued or never enqueued)."
 *   command_lifecycle     queued -> started -> completed|cancelled|...
 *                         keyed by the CLIENT-supplied uuid; a message sent
 *                         WITHOUT one emits no lifecycle events at all.
 *
 * Verified against the real CLI before any of this was written:
 *   LIFECYCLE queued uuid=OURS / LIFECYCLE cancelled uuid=OURS
 *   CONTROL_RESPONSE {"subtype":"success","response":{"cancelled":true}}
 *
 * What this file guards is the wiring around that contract, where the failures
 * are quiet ones: a message sent without a uuid (no lifecycle, so the drawer
 * never becomes recallable), a recall that reports success when the CLI said
 * cancelled=false (the user thinks it is gone, then watches Claude answer it),
 * and a waiter that only resolves on success (the button hangs forever).
 *
 * Run: node scripts/test-message-recall.mjs
 */

import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const lines = readFileSync(join(root, 'src', 'claude-ui.js'), 'utf8').split(/\r?\n/);

let failures = 0;
const check = (label, ok, detail) => {
  if (!ok) failures++;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${label}${detail === undefined ? '' : ` -> ${detail}`}`);
};

const at = (needle) => {
  const i = lines.findIndex((l) => l.includes(needle));
  if (i < 0) throw new Error(`Could not find "${needle}" in claude-ui.js — the test needs updating.`);
  return i;
};
const endOf = (start) => {
  let i = start + 1;
  while (i < lines.length && lines[i] !== '  }') i++;
  if (i >= lines.length) throw new Error('Could not find the end of a lifted function.');
  return i;
};
const lift = (needle) => { const a = at(needle); return lines.slice(a, endOf(a) + 1).join('\n'); };

const { handleCommandLifecycle, cancelMessage, handleControlResponse, sessions, sent } = new Function(`
  const sessions = new Map();
  const sent = [];
  let n = 0;
  const genId = () => 'g' + (++n);
  const send = (s, obj) => { sent.push(obj); return s.writable !== false; };
  ${lift('  function handleCommandLifecycle(s, msg) {')}
  ${lift('  function cancelMessage(id, uuid) {')}
  ${lift('  function handleControlResponse(s, msg) {')}
  return { handleCommandLifecycle, cancelMessage, handleControlResponse, sessions, sent };
`)();

const mkSession = () => {
  const events = [];
  const s = {
    id: 's1', proc: {}, writable: true,
    pendingInjections: [{ text: 'message 2', at: Date.now(), uuid: 'u-1', state: 'sent' }],
    pendingControls: new Map(),
    emitter: { emit: (_n, ev) => events.push(ev) },
  };
  sessions.set('s1', s);
  return { s, events };
};

// ─── The lifecycle drives what the drawer is allowed to claim ───
{
  const { s, events } = mkSession();
  handleCommandLifecycle(s, { type: 'command_lifecycle', command_uuid: 'u-1', state: 'queued' });
  check('queued marks it recallable', s.pendingInjections[0].state === 'queued', s.pendingInjections[0].state);
  check('and the UI is told', events.some(e => e.type === 'queued-state' && e.state === 'queued'));

  handleCommandLifecycle(s, { type: 'command_lifecycle', command_uuid: 'u-1', state: 'started' });
  check('started shuts the window', s.pendingInjections[0].state === 'started', s.pendingInjections[0].state);

  handleCommandLifecycle(s, { type: 'command_lifecycle', command_uuid: 'u-1', state: 'cancelled' });
  check('a terminal state drops it', s.pendingInjections.length === 0, s.pendingInjections.length);
}
// A lifecycle frame for someone else's message must not touch ours.
{
  const { s } = mkSession();
  handleCommandLifecycle(s, { type: 'command_lifecycle', command_uuid: 'someone-else', state: 'cancelled' });
  check('an unrelated uuid is ignored', s.pendingInjections.length === 1, s.pendingInjections.length);
}

// ─── The recall request ───
{
  const { s } = mkSession();
  sent.length = 0;
  const p = cancelMessage('s1', 'u-1');
  const req = sent[sent.length - 1];
  check('sends a cancel_async_message', req?.request?.subtype === 'cancel_async_message', req?.request?.subtype);
  check('names the message by uuid', req?.request?.message_uuid === 'u-1', req?.request?.message_uuid);
  check('carries a request id to match the answer to', typeof req?.request_id === 'string' && req.request_id.length > 2);

  handleControlResponse(s, { response: { subtype: 'success', request_id: req.request_id, response: { cancelled: true } } });
  const r = await p;
  check('resolves as cancelled', r.ok === true && r.cancelled === true, JSON.stringify(r));
  check('and stops tracking it', s.pendingInjections.length === 0, s.pendingInjections.length);
}

// cancelled=false is the "too late" answer, and must NOT read as success.
{
  const { s } = mkSession();
  sent.length = 0;
  const p = cancelMessage('s1', 'u-1');
  const req = sent[sent.length - 1];
  handleControlResponse(s, { response: { subtype: 'success', request_id: req.request_id, response: { cancelled: false } } });
  const r = await p;
  check('too late is reported honestly', r.ok === true && r.cancelled === false, JSON.stringify(r));
  check('and the message stays pending', s.pendingInjections.length === 1, s.pendingInjections.length);
}

// An error answer must resolve the waiter, not strand it.
{
  const { s } = mkSession();
  sent.length = 0;
  const p = cancelMessage('s1', 'u-1');
  const req = sent[sent.length - 1];
  handleControlResponse(s, { response: { subtype: 'error', request_id: req.request_id, error: 'nope' } });
  const r = await p;
  check('an error still resolves', r.ok === false && /nope/.test(r.error), JSON.stringify(r));
}

// Nothing to recall.
{
  mkSession();
  const r = await cancelMessage('s1', 'not-a-known-uuid');
  check('an unknown message is refused', r.ok === false, JSON.stringify(r));
}

// ─── The uuid has to go out with the message ───
//
// Without it the CLI emits no lifecycle events, the drawer never learns it is
// recallable, and the feature silently does not exist.
{
  const a = at('  function sendMessage(id, text) {');
  const body = lines.slice(a, endOf(a) + 1).join('\n');
  check('sendMessage mints a uuid', /randomUUID\(\)/.test(body));
  check('and puts it on the outbound envelope', /\n\s*uuid,/.test(body));
  check('and remembers it against the pending message', /pendingInjections\.push\(\{[^}]*uuid/.test(body));
}

if (failures) {
  console.error(`\n${failures} failure(s).`);
  process.exit(1);
}
console.log('\nA message can be taken back until the CLI says it cannot.');
