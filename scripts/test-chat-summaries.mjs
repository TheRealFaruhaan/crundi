#!/usr/bin/env node
/**
 * test-chat-summaries.mjs — the summaries a chat's own Claude Code makes.
 *
 * Verified against the real CLI (2.1.302, and again on 2.1.310) before this was written:
 *   side_question {question}            -> {response, synthetic}, also mid-turn
 *                                          and on a resumed session before its
 *                                          first turn
 *   generate_session_title {description, persist} -> {title} (null without a
 *                                          description; with persist, a session
 *                                          that already has a saved title gets
 *                                          that one back, whatever the description)
 *   CLAUDE_CODE_EMIT_TOOL_USE_SUMMARIES=1 -> tool_use_summary {summary,
 *                                          preceding_tool_use_ids}
 *   CLAUDE_CODE_AGENT_PROGRESS_SUMMARIES=1 + initialize agentProgressSummaries
 *                                       -> task_progress.summary every ~30s
 * The away summary itself never runs headless, so the recap is the same
 * instruction asked as a side question.
 *
 * This guards the wiring: the switches on the spawn, a tool summary folded
 * onto the rows it covers, side questions and titles that never hang, and a
 * recap that is made once, at the right time, and never shown stale.
 *
 * Run: node scripts/test-chat-summaries.mjs
 */
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const src = readFileSync(join(root, 'src', 'claude-ui.js'), 'utf8');
const lines = src.split(/\r?\n/);

let failures = 0;
const check = (label, ok, detail) => {
  if (!ok) failures++;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${label}${detail === undefined || ok ? '' : ` -> ${detail}`}`);
};
const at = (needle) => {
  const i = lines.findIndex((l) => l.includes(needle));
  if (i < 0) throw new Error(`Could not find "${needle}" in claude-ui.js — the test needs updating.`);
  return i;
};
const endOf = (start) => {
  let i = start + 1;
  while (i < lines.length && lines[i] !== '  }') i++;
  return i;
};
const lift = (needle) => { const a = at(needle); return lines.slice(a, endOf(a) + 1).join('\n'); };

// ─── On the spawn ───
check('both summary switches are on the owner spawn', /\.\.\.SUMMARY_ENV,/.test(src));
check('and on a collaborator spawn', /Object\.assign\(env, SUMMARY_ENV\)/.test(src));
check('SUMMARY_ENV names the two switches', /CLAUDE_CODE_EMIT_TOOL_USE_SUMMARIES: '1'/.test(src) && /CLAUDE_CODE_AGENT_PROGRESS_SUMMARIES: '1'/.test(src));
check('initialize asks for agent progress summaries', /subtype: 'initialize'[^\n]*agentProgressSummaries: true/.test(src));
check('tool_use_summary frames are handled', /case 'tool_use_summary': return handleToolUseSummary\(s, msg\);/.test(src));
check('a progress summary is kept apart from the step', /if \(msg\.summary\) a\.progress = /.test(src));
check('the recap is in the snapshot', /recap: s\.recap \|\| null,/.test(src));
check('a message going out retires the recap', /clearSuggestion\(s\);\s*\n\s*\/\/[^\n]*\n\s*clearRecap\(s\);/.test(src));
check('a resumed chat gets a recap once it is up', /if \(s\.resuming && !s\.forking && !s\.pendingFirstMessage && !s\.factory\) makeRecap\(s, 'resumed'\);/.test(src));
check('every turn end schedules the idle recap', /s\.turns = \(s\.turns \|\| 0\) \+ 1;\s*\n\s*scheduleIdleRecap\(s\);/.test(src));

const env = new Function(`
  const sessions = new Map();
  const sent = [];
  let n = 0, recapCb = null;
  const MAX_LABEL = 70, MAX_RECAP = 400, MAX_ASIDE_QUESTION = 2000, MAX_ASIDE_ANSWER = 8000;
  const RECAP_PROMPT = 'RECAP';
  let RECAP_IDLE_MS = 30;
  const genId = () => 'g' + (++n);
  const send = (s, obj) => { sent.push(obj); return s.writable !== false; };
  const patchEntry = (s, e, patch) => { Object.assign(e, patch); s.patches.push([e.id, patch]); };
  ${lift('  function control(s, request, timeoutMs = 30_000) {')}
  ${lift('  async function askAside(id, question, { timeoutMs = 60_000 } = {}) {')}
  ${lift('  async function generateTitle(id, description, { timeoutMs = 30_000 } = {}) {')}
  ${lift('  function clearRecap(s) {')}
  ${lift('  function scheduleIdleRecap(s) {')}
  ${lift('  async function makeRecap(s, why) {')}
  ${lift('  function handleToolUseSummary(s, msg) {')}
  ${lift('  function personText(id, { latest = 5, max = 2000 } = {}) {')}
  ${lift('  async function isDifferentJob(id, title) {')}
  return { sessions, sent, control, askAside, generateTitle, clearRecap, scheduleIdleRecap, makeRecap, handleToolUseSummary, personText, isDifferentJob,
    onRecap: (cb) => { recapCb = cb; } };
`)();

const mk = (over = {}) => {
  const events = [];
  const s = {
    id: 's1', proc: {}, writable: true, state: 'idle', turns: 1, recapTurn: 0, recap: null, recapTimer: null,
    pendingControls: new Map(), messages: [], patches: [],
    emitter: { emit: (_n, ev) => events.push(ev) }, ...over,
  };
  env.sessions.set(s.id, s);
  return { s, events };
};
const answer = (s, response, error) => {
  const req = env.sent[env.sent.length - 1];
  const w = s.pendingControls.get(req.request_id);
  s.pendingControls.delete(req.request_id);
  w(error ? null : response, error || null);
  return req;
};
const tick = (ms = 5) => new Promise((r) => setTimeout(r, ms));

// ─── Tool summaries fold onto the rows they cover ───
{
  const { s } = mk();
  s.messages.push(
    { id: 'e1', kind: 'tool', toolUseId: 't1' }, { id: 'e2', kind: 'text' },
    { id: 'e3', kind: 'tool', toolUseId: 't2' }, { id: 'e4', kind: 'tool', toolUseId: 't3' });
  env.handleToolUseSummary(s, { type: 'tool_use_summary', summary: '  Read a.txt twice, listed files ', preceding_tool_use_ids: ['t1', 't2'] });
  check('each covered tool row gets the label', s.messages[0].groupSummary === 'Read a.txt twice, listed files' && s.messages[2].groupSummary === 'Read a.txt twice, listed files');
  check('and the first row\'s id as the group', s.messages[0].group === 'e1' && s.messages[2].group === 'e1');
  check('rows it does not name are untouched', !('group' in s.messages[3]) && !('group' in s.messages[1]));
  check('the change goes out as patches', s.patches.length === 2);
  s.patches.length = 0;
  env.handleToolUseSummary(s, { summary: '', preceding_tool_use_ids: ['t3'] });
  env.handleToolUseSummary(s, { summary: 'x', preceding_tool_use_ids: [] });
  env.handleToolUseSummary(s, { summary: 'x', preceding_tool_use_ids: ['nope'] });
  check('nothing to say, nothing named, or unknown ids: no change', s.patches.length === 0);
}

// ─── Side questions ───
{
  const { s } = mk();
  let p = env.askAside('s1', '  what did you change?  ');
  let req = env.sent[env.sent.length - 1];
  check('a side_question with the trimmed question', req.request.subtype === 'side_question' && req.request.question === 'what did you change?');
  answer(s, { response: ' Two files. ', synthetic: false });
  let r = await p;
  check('answers with the text', r.ok && r.answer === 'Two files.', JSON.stringify(r));
  p = env.askAside('s1', 'q');
  answer(s, null, 'Unknown control request subtype: side_question');
  r = await p;
  check('an older Claude Code says it cannot', !r.ok && /cannot answer side questions/.test(r.error), r.error);
  check('an empty question is refused without asking', !(await env.askAside('s1', '   ')).ok);
  check('a chat that is not running is refused', !(await env.askAside('nope', 'q')).ok);
  r = await env.askAside('s1', 'q', { timeoutMs: 10 });
  check('no answer in time is an error, not a hang', !r.ok && /did not answer in time/.test(r.error), r.error);
  check('and the waiter is dropped', s.pendingControls.size === 0, s.pendingControls.size);
}

// ─── Titles ───
{
  const { s } = mk();
  let p = env.generateTitle('s1', 'refactor the retry logic');
  const req = env.sent[env.sent.length - 1];
  check('generate_session_title with the description, not persisted (a saved title would come back unchanged)', req.request.subtype === 'generate_session_title' && req.request.description === 'refactor the retry logic' && req.request.persist === false);
  answer(s, { title: ' Payment retry logic ' });
  check('the title, trimmed', (await p) === 'Payment retry logic');
  p = env.generateTitle('s1', 'x');
  answer(s, { title: null });
  check('none given is null', (await p) === null);
}

// ─── Recaps ───
{
  const { s, events } = mk();
  const got = [];
  env.onRecap((id, recap) => got.push([id, recap]));
  const p = env.makeRecap(s, 'idle');
  const req = env.sent[env.sent.length - 1];
  check('a recap is Claude Code\'s recap instruction, asked on the side', req.request.subtype === 'side_question' && req.request.question === 'RECAP');
  answer(s, { response: 'Building the release.\n Next: tag main.', synthetic: false });
  await p;
  check('the recap is kept, on one line, with why', s.recap && s.recap.text === 'Building the release. Next: tag main.' && s.recap.why === 'idle' && s.recap.at > 0, JSON.stringify(s.recap));
  check('clients hear it', events.some((e) => e.type === 'recap' && e.recap && e.recap.text));
  check('and the server\'s listener', got.length === 1 && got[0][0] === 's1');
  env.clearRecap(s);
  check('clearing it tells clients', s.recap === null && events.at(-1).type === 'recap' && events.at(-1).recap === null);
}
{
  // The person spoke while it was being written: it is stale, never shown.
  const { s, events } = mk({ id: 's2' });
  const p = env.makeRecap(s, 'idle');
  s.turns = 2;
  answer(s, { response: 'Old news', synthetic: false });
  await p;
  check('a recap overtaken by a new turn is dropped', s.recap === null && !events.some((e) => e.type === 'recap'));
}
{
  const { s } = mk({ id: 's3' });
  const p = env.makeRecap(s, 'resumed');
  answer(s, { response: 'Nothing here yet', synthetic: true });
  await p;
  check('a made-up (synthetic) answer is not a recap', s.recap === null);
}
{
  // Idle for the whole wait: one recap. A new turn meanwhile: none.
  const { s } = mk({ id: 's4', turns: 3 });
  const before = env.sent.length;
  env.scheduleIdleRecap(s);
  await tick(60);
  const asked = env.sent.slice(before).filter((x) => x.request.subtype === 'side_question').length;
  check('idle after a turn: a recap is asked for', asked === 1, asked);
  check('once per turn', s.recapTurn === 3);
  const { s: busy } = mk({ id: 's5', turns: 1 });
  const b2 = env.sent.length;
  env.scheduleIdleRecap(busy);
  busy.turns = 2; busy.state = 'running';
  await tick(60);
  check('a turn started meanwhile: no recap', env.sent.length === b2);
  const { s: fac } = mk({ id: 's6', factory: { runId: 'r' } });
  env.scheduleIdleRecap(fac);
  check('factory chats are left out', fac.recapTimer === null);
  const { s: bg } = mk({ id: 's7', background: true });
  env.scheduleIdleRecap(bg);
  check('so are scheduled jobs', bg.recapTimer === null);
  const { s: c } = mk({ id: 's8' });
  env.scheduleIdleRecap(c);
  env.clearRecap(c);
  check('a message going out cancels the wait', c.recapTimer === null);
}

// ─── What a retitle is made from, and when it happens ───
{
  const { s } = mk({ id: 'p1' });
  const u = (text, by) => ({ kind: 'user', text, ...(by ? { by } : {}) });
  s.messages.push(u('Fix the login redirect'), { kind: 'assistant-text', text: 'Releasing 2.4.0 now' },
    u('/compact'), u('Crundi ran a command', 'system'), u('  '), u('and the cookie expiry'));
  check('the person\'s own words, not Claude\'s, slash commands or Crundi\'s posts', env.personText('p1') === 'Fix the login redirect\n\nand the cookie expiry', JSON.stringify(env.personText('p1')));
  for (let i = 1; i <= 8; i++) s.messages.push(u('step ' + i));
  const t = env.personText('p1');
  check('the first message, then the latest five', t.startsWith('Fix the login redirect\n\nstep 4') && t.endsWith('step 8') && !t.includes('cookie') && !t.includes('step 3'), JSON.stringify(t));
  s.messages.push(u('x'.repeat(5000)));
  check('kept short', env.personText('p1').length <= 2000);
  check('nothing said yet: nothing to title', env.personText('nope') === '' && (() => { const { s: e } = mk({ id: 'p2' }); return env.personText('p2') === ''; })());
}
{
  const { s } = mk({ id: 'j1' });
  let p = env.isDifferentJob('j1', 'Crundi 1.21.3-dev.1');
  const q = env.sent[env.sent.length - 1].request;
  check('the job question is asked on the side, naming the title', q.subtype === 'side_question' && q.question.includes('"Crundi 1.21.3-dev.1"') && /newer version number/.test(q.question) && /SAME or DIFFERENT/.test(q.question));
  answer(s, { response: 'SAME' });
  check('SAME keeps the title', (await p) === false);
  p = env.isDifferentJob('j1', 'x'); answer(s, { response: 'Different.' });
  check('DIFFERENT renames', (await p) === true);
  p = env.isDifferentJob('j1', 'x'); answer(s, null, 'Unknown control request subtype: side_question');
  check('no answer keeps the title', (await p) === false);
  p = env.isDifferentJob('j1', 'x'); answer(s, { response: 'It is not different, the same job' });
  check('only a clear DIFFERENT counts', (await p) === false);
}

// ─── The server side ───
const web = readFileSync(join(root, 'src', 'webapp.js'), 'utf8');
check('routes: aside and dismiss-recap', /dismiss-suggestion\|dismiss-recap\|aside\)\$/.test(web) && /claudeUi\.askAside\(sid, body\.question\)/.test(web) && /claudeUi\.dismissRecap\(sid\)/.test(web));
check('the namer asks the chat first', /fromChat: \(id, text\) => claudeUi\.generateTitle\(id, text\)/.test(web));
check('an idle recap retitles, never over the person\'s title', /recap\.why !== 'idle'/.test(web) && /m\.titleByUser\) return;/.test(web));
check('from the person\'s messages, never the recap', /claudeUi\.generateTitle\(id, said\)/.test(web) && !/generateTitle\(id, recap\.text\)/.test(web));
check('only when it is a different job, and not if renamed meanwhile', /if \(!\(await claudeUi\.isDifferentJob\(id, m\.title\)\)\) return;/.test(web) && /now\.title !== m\.title\) return;/.test(web));
const caps = readFileSync(join(root, 'src', 'client-version.js'), 'utf8');
check('capabilities: chat-recap and chat-aside', /'chat-recap',/.test(caps) && /'chat-aside',/.test(caps));

console.log(failures ? `\n${failures} FAILED` : '\nAll chat summary checks passed.');
process.exit(failures ? 1 : 0);
