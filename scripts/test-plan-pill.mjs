#!/usr/bin/env node
/**
 * test-plan-pill.mjs — the plan Claude proposes, shown in the chat's title bar.
 *
 * Verified against the real CLI (2.1.269) before any of this was written. Under
 * Crundi's exact chat flags, in plan mode:
 *
 *   MODE: plan | ExitPlanMode offered: true
 *   >>> can_use_tool: ExitPlanMode
 *       input keys: [ 'plan', 'planFilePath' ]
 *
 * That is the ONLY place a plan reaches us. It arrives as the tool input of a
 * permission request — not as an assistant message, not in the transcript, not
 * in any status event. Miss it there and it is gone.
 *
 * The failures this guards are all quiet ones:
 *
 *  - Reading the wrong input key, so the pill appears with nothing behind it.
 *  - A status the server can emit that the header has no label for: the pill
 *    renders blank or uncoloured, and only for the rarest state (rejected),
 *    which is exactly the one nobody clicks through by hand.
 *  - The live-update chain breaking. A plan change moves no agent state, so it
 *    rides no state broadcast. It reaches the header only via
 *      setPlan -> meta event -> chat view -> onChatMeta -> updateCellHead.
 *    Drop any link and the pill still works on reload, which is precisely how
 *    this would ship unnoticed.
 *
 * Run: node scripts/test-plan-pill.mjs
 */

import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const read = (...p) => readFileSync(join(root, ...p), 'utf8');

const ui = read('src', 'claude-ui.js');
const html = read('src', 'webapp-html.js');
const chat = read('app', 'vendor', 'claude-chat.js');

let failures = 0;
const check = (label, ok, detail) => {
  if (!ok) failures++;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${label}${detail === undefined ? '' : ` -> ${detail}`}`);
};

// ── Capture ───────────────────────────────────────────────────────────────
// The plan rides ExitPlanMode's permission request, keyed `plan`/`planFilePath`.
const capture = ui.match(/tool_name === 'ExitPlanMode'[\s\S]{0,600}?\n {6}\}/);
check('claude-ui captures the plan off ExitPlanMode', !!capture);
if (capture) {
  const c = capture[0];
  check('  reads req.input.plan', /req\.input\?\.plan\b/.test(c));
  check('  reads req.input.planFilePath', /req\.input\?\.planFilePath\b/.test(c));
  check("  opens at status 'proposed'", /status: 'proposed'/.test(c));
  check('  goes through setPlan (so subscribers hear it)', /setPlan\(s,/.test(c));
}

// ── The state machine ─────────────────────────────────────────────────────
check("approving ExitPlanMode promotes it to 'executing'",
  /entry\.toolName === 'ExitPlanMode'[\s\S]{0,300}?'rejected' : 'executing'/.test(ui));
check("leaving plan mode with a proposal outstanding also promotes it",
  /leftPlanMode && s\.plan && s\.plan\.status === 'proposed'[\s\S]{0,160}?status: 'executing'/.test(ui));

// Every status the server can put on a plan — gathered from the setPlan call
// sites themselves, so a status introduced in a ternary still counts. This is
// the whole point: the set below is what the two UIs must be able to label.
const emitted = new Set();
for (const call of ui.matchAll(/setPlan\(s,[\s\S]{0,240}?\);/g)) {
  for (const m of call[0].matchAll(/'(proposed|executing|rejected)'/g)) emitted.add(m[1]);
}
check('server emits all three plan statuses', emitted.size === 3, [...emitted].sort().join(','));

// ── Every emitted status must be labelled on both surfaces ────────────────
const tagFor = html.match(/function planTagFor\(t\)[\s\S]*?\n {4}\}/);
check('webapp-html has planTagFor', !!tagFor);
if (tagFor) {
  for (const st of emitted) {
    check(`  header labels '${st}'`, new RegExp(`st === '${st}'`).test(tagFor[0]));
  }
  check("  header also covers plan mode with no plan yet",
    /t\.permissionMode === 'plan'/.test(tagFor[0]));
  // Only readable states may carry a click target, or the click is a dead end.
  const planning = tagFor[0].match(/t\.permissionMode === 'plan'\) return \{[^}]*\}/);
  check('  the "planning" pill is not clickable', !!planning && /read: false/.test(planning[0]));
}

const label = chat.match(/function planStatusLabel\(st\)[\s\S]*?\n {4}\}/);
check('chat component has planStatusLabel', !!label);
if (label) {
  // 'proposed' is the fallthrough default, so it needs no branch of its own.
  for (const st of [...emitted].filter(s => s !== 'proposed')) {
    check(`  panel labels '${st}'`, new RegExp(`st === '${st}'`).test(label[0]));
  }
}

// ── Exposure to clients ───────────────────────────────────────────────────
check('list() carries planStatus (initial render + broadcasts)', /planStatus: s\.plan \? s\.plan\.status : ''/.test(ui));
check('snapshot() carries the plan itself (reload restores it)', /plan: planSnapshot\(s\)/.test(ui));
check('setPlan emits a meta event', /function setPlan\(s, plan\)[\s\S]{0,260}?emitter\.emit\('event', \{ type: 'meta', plan:/.test(ui));

// Deliberately NOT persisted: replaying "executing" into a chat that merely
// resumed the transcript would claim work is underway when nothing is running.
const head = ui.match(/const head = \{[\s\S]*?\};/);
check('the plan is not written to the persisted transcript', !!head && !/plan/.test(head[0]));

// ── The live-update chain ─────────────────────────────────────────────────
check('chat view applies plan from history', /plan = session\.plan \|\| null;\s*\r?\n\s*renderPlan\(\);/.test(chat));
check("chat view applies plan from meta", /if \('plan' in ev\) \{ plan = ev\.plan \|\| null; renderPlan\(\); notifyMeta/.test(chat));
check('chat view tells the host on plan change', /notifyMeta\(\{ plan: plan \}\)/.test(chat));
check('chat view tells the host on mode change', /notifyMeta\(\{ permissionMode: ev\.permissionMode \}\)/.test(chat));
check('chat view exposes togglePlan', /togglePlan: togglePlan,/.test(chat));
check('host passes onChatMeta into the chat', /onChatMeta: \(patch\) => applyChatMeta\(t\.id, patch\)/.test(html));
check('host patches the cached record and redraws the head',
  /function applyChatMeta\(tid, patch\)[\s\S]{0,700}?updateCellHead\(el, rec\)/.test(html));
check('updateCellHead maintains the pill in place', /term-plan-tag[\s\S]{0,600}?kindTag\.before\(ptag\)/.test(html));

// ── The click ─────────────────────────────────────────────────────────────
check('the pill emits a chat-plan action', /data-action="chat-plan"/.test(html));
check('the dispatcher handles chat-plan', /case 'chat-plan':/.test(html));
check('chat-plan reaches the mounted view', /case 'chat-plan':[\s\S]{0,220}?chatViews\.get\(d\.tid\)[\s\S]{0,120}?togglePlan\(\)/.test(html));

console.log(failures ? `\n${failures} failure(s)` : '\nAll plan-pill checks passed.');
process.exit(failures ? 1 : 0);
