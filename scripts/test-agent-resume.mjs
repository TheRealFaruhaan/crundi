#!/usr/bin/env node
/**
 * test-agent-resume.mjs — an agent resumed with a message (SendMessage) is one
 * entry in the chat's agents, not two.
 *
 * The CLI starts the resumed run under the new call's tool_use_id, but its
 * progress and its end arrive under the first call's (same task_id). Before,
 * that left a second, nameless "Agent" entry showing running, and the chat
 * read "waiting" for it.
 *
 * handleTaskEvent and getAgent are lifted out of claude-ui.js by position.
 * Run: node scripts/test-agent-resume.mjs
 */
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const ui = readFileSync(join(root, 'src', 'claude-ui.js'), 'utf8').replace(/\r\n/g, '\n');
const lift = (start) => {
  const i = ui.indexOf(start);
  if (i < 0) throw new Error('Could not find ' + start);
  return ui.slice(i, ui.indexOf('\n  }\n', i) + 5);
};
let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };

const { handleTaskEvent } = new Function(`
  const MAX_LABEL = 120, MAX_TEXT = 4000;
  const toMs = (v) => (typeof v === 'number' ? v : Date.parse(v) || Date.now());
  function trimAgents() {} function scheduleAutoDismiss() {} function emitAgent() {}
  function activeTriggers(s) { let n = 0; for (const a of s.agents.values()) if (!a.endedAt) n++; return n; }
  function setState(s, st) { s.state = st; }
  ${lift('  function getAgent(s, toolUseId, kind) {')}
  ${lift('  function handleTaskEvent(s, msg) {')}
  return { handleTaskEvent };
`)();

const s = { agents: new Map(), agentsByTask: new Map(), state: 'working' };
const ev = (subtype, extra) => handleTaskEvent(s, { type: 'system', subtype, ...extra });
ev('task_started', { tool_use_id: 'tu-agent', task_id: 'A1', description: 'Chat voice mode: web', subagent_type: 'general-purpose' });
ev('task_notification', { tool_use_id: 'tu-agent', task_id: 'A1', status: 'completed', summary: 'done' });
ok(s.agents.size === 1 && s.agents.get('tu-agent').endedAt > 0, 'the first run ends');

// Resumed with a message: started under the SendMessage call's id.
ev('task_started', { tool_use_id: 'tu-send', task_id: 'A1', description: '', subagent_type: 'general-purpose' });
ev('task_progress', { tool_use_id: 'tu-agent', task_id: 'A1', description: 'Reading crundi-voice.js' });
ok(s.agents.size === 1, 'resumed: still one entry, not a nameless second one');
const a = s.agents.get('tu-agent');
ok(a.status === 'running' && !a.endedAt && a.description === 'Chat voice mode: web', 'revived, running again, its name kept');
ok(a.step === 'Reading crundi-voice.js', 'and its progress lands on it');

s.state = 'waiting';
ev('task_notification', { tool_use_id: 'tu-agent', task_id: 'A1', status: 'completed', summary: 'done again' });
ok(a.endedAt > 0 && a.status === 'completed', 'its end ends it');
ok(s.state === 'idle', 'and a chat waiting only on it is idle');

// The end under the new id works too.
ev('task_started', { tool_use_id: 'tu-send2', task_id: 'A1' });
ev('task_notification', { tool_use_id: 'tu-send2', task_id: 'A1', status: 'completed' });
ok(s.agents.size === 1 && a.endedAt > 0, 'an end keyed by the resuming call still ends the one entry');

console.log(failed ? `\n${failed} FAILED` : '\nAll passed');
process.exit(failed ? 1 : 0);
