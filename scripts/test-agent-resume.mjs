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

const { handleTaskEvent, retireAgents, runningTasks, setDone } = new Function(`
  const MAX_LABEL = 120, MAX_TEXT = 4000;
  const toMs = (v) => (typeof v === 'number' ? v : Date.parse(v) || Date.now());
  function trimAgents() {} function scheduleAutoDismiss() {} function emitAgent() {} function cancelAutoDismiss() {}
  let doneIds = null;
  function finishedTasks() { return doneIds; }
  function setState(s, st) { s.state = st; }
  const AGENT_SILENT_MS = 15 * 60 * 1000;
  const TASK_CAP_MS = 6 * 60 * 60 * 1000;
  ${lift('  function isStale(a, now = Date.now()) {')}
  ${lift('  function runningTasks(s) {')}
  function activeTriggers(s) { return runningTasks(s); }
  ${lift('  function endRunning(s, which) {')}
  ${lift('  function retireAgents(s) {')}
  ${lift('  function revive(s, a) {')}
  ${lift('  function getAgent(s, toolUseId, kind) {')}
  ${lift('  function handleTaskEvent(s, msg) {')}
  return { handleTaskEvent, retireAgents, runningTasks, setDone: (d) => { doneIds = d; } };
`)();

const s = { agents: new Map(), agentsByTask: new Map(), state: 'working', proc: {} };
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

// Ends that never reached us (the stuck "Agent running" entries).
{
  const t = { agents: new Map(), agentsByTask: new Map(), state: 'waiting', proc: {} };
  const e = (subtype, extra) => handleTaskEvent(t, { type: 'system', subtype, ...extra });
  e('task_started', { tool_use_id: 'x1', task_id: 'X1', subagent_type: 'general-purpose' });
  e('task_started', { tool_use_id: 'x2', task_id: 'X2', subagent_type: 'general-purpose' });
  e('task_started', { tool_use_id: 'b1', task_id: 'B1', task_type: 'local_bash' });
  ok(runningTasks(t) === 3, 'three running');
  setDone(new Set(['X1']));
  retireAgents(t);
  ok(t.agents.get('x1').endedAt > 0 && !t.agents.get('x2').endedAt, 'an agent finished per the transcript is ended');
  t.agents.get('x2').lastAt = Date.now() - 16 * 60 * 1000;
  t.agents.get('b1').lastAt = Date.now() - 16 * 60 * 1000;
  ok(runningTasks(t) === 1, 'a silent agent no longer counts; a quiet command still does');
  retireAgents(t);
  ok(t.agents.get('x2').status === 'stopped' && !t.agents.get('b1').endedAt, 'the silent agent is ended, the command is not');
  e('task_progress', { tool_use_id: 'x2', task_id: 'X2', description: 'Still here' });
  ok(!t.agents.get('x2').endedAt && t.agents.get('x2').status === 'running', 'a later sign of life revives a guessed end');
  t.proc = null;
  retireAgents(t);
  ok(runningTasks(t) === 0 && t.state === 'idle', 'with no process left nothing runs, and the chat is idle');
  setDone(null);
}

console.log(failed ? `\n${failed} FAILED` : '\nAll passed');
process.exit(failed ? 1 : 0);
