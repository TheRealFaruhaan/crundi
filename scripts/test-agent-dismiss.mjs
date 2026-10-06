// Finished agents and background commands put themselves away.
//
// The chat used to float one pill per subagent AND per background command over
// the conversation, with no cap on running ones: a turn that fanned out to ten
// agents and five commands covered most of a phone screen, and every pill had
// to be closed by hand. Now the client shows two badges - one count of agents,
// one of commands - and the server dismisses either five minutes after it ends.
//
// The server half is tested by lifting the real functions out of claude-ui.js
// and running them against a fake clock, the way test-waiting-state.mjs does:
// they live in a closure that otherwise needs a live Claude process.
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const read = (...p) => readFileSync(join(root, ...p), 'utf8').replace(/\r\n/g, '\n');
let failed = 0;
const ok = (cond, name, extra) => {
  console.log(`${cond ? 'PASS' : 'FAIL'}  ${name}${cond || extra === undefined ? '' : ' -> ' + JSON.stringify(extra)}`);
  if (!cond) failed++;
};

// ─── The server's countdown ───
const ui = read('src', 'claude-ui.js');
const from = ui.indexOf('  const AGENT_LINGER_MS');
const to = ui.indexOf('\n  }\n', ui.indexOf('  function scheduleAutoDismiss(')) + 5;
if (from < 0 || to < 5) throw new Error('Could not find the auto-dismiss block in claude-ui.js — the test needs updating.');
const block = ui.slice(from, to);

function world() {
  let now = 1_800_000_000_000;
  const timers = [];
  const emitted = [];
  const sessions = new Map();
  const fake = {
    setTimeout: (fn, ms) => { const t = { fn, at: now + ms, unref() {} }; timers.push(t); return t; },
    clearTimeout: (t) => { const i = timers.indexOf(t); if (i >= 0) timers.splice(i, 1); },
    Date: { now: () => now, parse: Date.parse },
  };
  const api = new Function('sessions', 'emitAgent', 'setTimeout', 'clearTimeout', 'Date',
    block + '\n  return { AGENT_LINGER_MS, toMs, cancelAutoDismiss, scheduleAutoDismiss };',
  )(sessions, (s, a) => emitted.push({ id: a.toolUseId, dismissed: !!a.dismissed }), fake.setTimeout, fake.clearTimeout, fake.Date);
  const s = { id: 's1', agents: new Map() };
  sessions.set('s1', s);
  const agent = (id, o = {}) => { const a = { toolUseId: id, kind: 'agent', status: 'running', endedAt: 0, ...o }; s.agents.set(id, a); return a; };
  const advance = (ms) => {
    now += ms;
    for (const t of timers.filter(x => x.at <= now)) { timers.splice(timers.indexOf(t), 1); t.fn(); }
  };
  return { api, s, sessions, agent, advance, emitted, timers, now: () => now };
}
const MIN = 60_000;

{
  const w = world();
  ok(w.api.AGENT_LINGER_MS === 5 * MIN, 'a finished agent lingers for five minutes', w.api.AGENT_LINGER_MS);

  const a = w.agent('a1');
  w.api.scheduleAutoDismiss(w.s, a);
  w.advance(60 * MIN);
  ok(!a.dismissed && !w.timers.length, 'a running agent is never dismissed, however long it runs');

  a.status = 'completed';
  w.api.scheduleAutoDismiss(w.s, a);
  ok(a.endedAt === w.now() && w.timers.length === 1, 'finishing starts the countdown and stamps the end time');
  w.api.scheduleAutoDismiss(w.s, a); w.api.scheduleAutoDismiss(w.s, a);
  ok(w.timers.length === 1, 'later events for the same agent do not stack timers');
  w.advance(5 * MIN - 1);
  ok(!a.dismissed, 'it is still on show just under five minutes later');
  w.advance(1);
  ok(a.dismissed === true && a.autoDismissed === true, 'and dismissed at five minutes');
  ok(w.emitted.length === 1 && w.emitted[0].id === 'a1' && w.emitted[0].dismissed, 'the open clients are told', w.emitted);
}
{
  const w = world();
  const failedAgent = w.agent('f', { status: 'failed' });
  w.api.scheduleAutoDismiss(w.s, failedAgent);
  w.advance(5 * MIN);
  ok(failedAgent.dismissed === true, 'a failed agent is put away too');

  const task = w.agent('t', { kind: 'task', status: 'completed' });
  w.api.scheduleAutoDismiss(w.s, task);
  w.advance(5 * MIN - 1); const taskEarly = task.dismissed;
  w.advance(1);
  ok(!taskEarly && task.dismissed === true, 'a finished background command is put away after five minutes as well');
  const monitor = w.agent('mon', { kind: 'task', status: 'running' });   // armed: reports events, has not ended
  w.api.scheduleAutoDismiss(w.s, monitor);
  w.advance(60 * MIN);
  ok(!monitor.dismissed && !monitor.endedAt, 'a command or Monitor that is still running is never put away');

  const manual = w.agent('m', { status: 'completed' });
  w.api.scheduleAutoDismiss(w.s, manual);
  manual.dismissed = true; w.api.cancelAutoDismiss(w.s, 'm');
  const before = w.emitted.length;
  w.advance(10 * MIN);
  ok(w.emitted.length === before && !manual.autoDismissed, 'one dismissed by hand is not dismissed again');

  const again = w.agent('r', { status: 'completed' });
  w.api.scheduleAutoDismiss(w.s, again);
  w.advance(2 * MIN);
  again.status = 'running';            // the CLI resumed it
  w.api.scheduleAutoDismiss(w.s, again);
  w.advance(30 * MIN);
  ok(!again.dismissed, 'an agent that starts running again has its countdown cancelled');

  const closed = w.agent('c', { status: 'completed' });
  w.api.scheduleAutoDismiss(w.s, closed);
  w.sessions.delete('s1');
  const n = w.emitted.length;
  w.advance(10 * MIN);
  ok(!closed.dismissed && w.emitted.length === n, 'a countdown that outlives its chat does nothing');
}
{
  // Coming back from disk.
  const w = world();
  const old = w.agent('old', { status: 'completed', endedAt: w.now() - 6 * MIN });
  ok(w.api.scheduleAutoDismiss(w.s, old) === true && old.dismissed === true && !w.timers.length, 'restored after its five minutes ran out: dismissed at once, before any client sees it');
  ok(w.emitted.length === 0, 'without announcing it');
  const recent = w.agent('recent', { status: 'completed', endedAt: w.now() - 4 * MIN });
  w.api.scheduleAutoDismiss(w.s, recent);
  w.advance(MIN - 1); const early = recent.dismissed;
  w.advance(1);
  ok(!early && recent.dismissed === true, 'restored part-way through: the countdown resumes where it was, not from the top');

  ok(w.api.toMs(1_800_000_000) === 1_800_000_000_000, 'an end time in seconds is read as seconds');
  ok(w.api.toMs(1_800_000_000_000) === 1_800_000_000_000, 'one in milliseconds as milliseconds');
  ok(w.api.toMs('2027-01-15T08:00:00Z') === Date.parse('2027-01-15T08:00:00Z'), 'and ISO text as a date');
  ok(w.api.toMs('garbage') === w.now() && w.api.toMs(0) === w.now(), 'anything unreadable means "now", never "long ago"');
}

// ─── Wiring: the countdown is actually reached ───
ok(/scheduleAutoDismiss\(s, a\);\s*emitAgent\(s, a\);\s*\}/.test(ui), 'every task event passes through the countdown');
ok(/if \(rec\.status === 'running'\) \{\s*rec\.status = 'stopped'/.test(ui) && /scheduleAutoDismiss\(s, rec\)/.test(ui), 'anything restored as "running" is marked stopped and counted down: its process is gone');
ok(/cancelAutoDismiss\(s, id\)/.test(ui), 'dismissing by hand cancels the countdown');
ok(/s\.agentTimers\.values\(\)\) clearTimeout\(t\)/.test(ui), 'closing a chat clears its countdowns');
ok(/a\.endedAt = toMs\(patch\.end_time\)/.test(ui), 'end times from the CLI are normalised');

// ─── The client: one badge per kind ───
const chat = read('app', 'vendor', 'claude-chat.js');
ok(!/cc-bub\b|drawBubble|trimDock/.test(chat), 'the per-item pills are gone');
ok(/function kindOf\(m\) \{ return m\.kind === 'task' \? 'task' : 'agent'; \}/.test(chat), 'commands and agents are told apart');
const shown = chat.slice(chat.indexOf('function shownOf(kind)'), chat.indexOf('function shownOf(kind)') + 220);
ok(/kindOf\(rec\.meta\) === kind && !rec\.dismissed/.test(shown), 'a badge counts what is undismissed, of its own kind only', shown);
ok(/badgeRow\.appendChild\(taskBadge\);\s*badgeRow\.appendChild\(agentBadge\);/.test(chat), 'the commands badge sits in the same row as the agents badge');
ok(/function syncBadge\(\)[\s\S]{0,300}shownOf\(kind\)/.test(chat) && /function paintAgentList\(\)[\s\S]{0,200}shownOf\(listKind\)/.test(chat), 'a badge and its list draw from the same set');
ok(/badgeEvent\(kind, 'add'\)/.test(chat) && /badgeEvent\(kind, 'done'\)/.test(chat), 'each badge animates when one of its kind is added and when one finishes');
ok(/if \(!agentsLive\) return;/.test(chat), 'but not while a stored conversation is being replayed');
ok(/prefers-reduced-motion: reduce\)\{\.cc-agbadge\.ev-add,\.cc-agbadge\.ev-done\{animation:none\}/.test(chat), 'and not for someone who has asked for reduced motion');
ok(/toggleAgentPanel\(row\.dataset\.id, true\)/.test(chat), 'a row in a list opens that agent\'s transcript or that command\'s output');
ok(/if \(meta\.dismissed\) rec\.dismissed = true;/.test(chat), 'a dismissal made by the server is honoured');
ok(/function dismissAllShown\(kind\)[\s\S]{0,120}shownOf\(kind\)/.test(chat) && !/all: true/.test(chat), 'Dismiss all clears the open list only, not the other kind');

console.log(failed ? `\n${failed} agent check(s) failed` : '\nAll agent dismiss checks passed');
process.exit(failed ? 1 : 0);
