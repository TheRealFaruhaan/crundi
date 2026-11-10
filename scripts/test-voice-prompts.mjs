#!/usr/bin/env node
/**
 * test-voice-prompts.mjs — Cru seeing and answering a chat's questions and
 * permission prompts.
 *
 *   see      describePending / pendingLine: the question with its choices,
 *            the command, the file and the change, the plan; read_chat,
 *            list_panels and workbench_overview carry them
 *   answer   answer_chat_prompt: as the owner says (at once), Cru's own
 *            reading (read back first), on its own in a chat it runs or, for
 *            a permission, in admin mode (logged); questions mapped to the
 *            option labels; collaborator and factory prompts left alone
 *   card     claude-ui respond(by: 'cru') marks the card answeredBy cru
 *
 * Run: node scripts/test-voice-prompts.mjs
 */

import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const V = await import('../src/voice/tools.js');
const A = await import('../src/voice/actions.js');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };

const question = {
  id: 'e1', kind: 'question', requestId: 'rq-1', toolName: 'AskUserQuestion', status: 'pending',
  input: { questions: [{ question: 'Which database should the cache use?', header: 'Database', multiSelect: false, options: [{ label: 'Postgres', description: 'The one already running' }, { label: 'SQLite', description: 'A file, no server' }] }] },
};
const multi = {
  id: 'e2', kind: 'question', requestId: 'rq-2', toolName: 'AskUserQuestion', status: 'pending',
  input: { questions: [{ question: 'Which checks should run?', multiSelect: true, options: [{ label: 'Lint' }, { label: 'Unit tests' }, { label: 'E2E' }] }] },
};
const bash = { id: 'e3', kind: 'permission', requestId: 'rq-3', toolName: 'Bash', displayName: 'Bash', status: 'pending', input: { command: 'npm test -- --watch=false', description: 'Run the tests' } };
const edit = { id: 'e4', kind: 'permission', requestId: 'rq-4', toolName: 'Edit', status: 'pending', input: { file_path: '/p/src/app.js', old_string: 'const a = 1', new_string: 'const a = 2' } };
const plan = { id: 'e5', kind: 'permission', requestId: 'rq-5', toolName: 'ExitPlanMode', status: 'pending', input: { plan: '1. Add the cache\n2. Test it' } };
const collab = { id: 'e6', kind: 'permission', requestId: 'rq-6', toolName: 'Bash', status: 'pending', escalated: true, input: { command: 'rm -rf /' } };

// ─── seeing it ───
{
  const d = V.describePending(question);
  ok(d.kind === 'question' && d.requestId === 'rq-1' && d.questions[0].question === 'Which database should the cache use?' && d.questions[0].options.map(o => o.label).join('|') === 'Postgres|SQLite' && d.questions[0].options[0].description === 'The one already running', 'a question comes whole: its text, its choices and what each means');
  const b = V.describePending(bash);
  ok(b.kind === 'permission' && b.tool === 'Bash' && b.asks.command === 'npm test -- --watch=false' && b.asks.purpose === 'Run the tests', 'a permission says the command it is for, and why');
  const e = V.describePending(edit);
  ok(e.asks.file === '/p/src/app.js' && e.asks.edits[0].replace === 'const a = 1' && e.asks.edits[0].with === 'const a = 2', 'an edit says the file and the change');
  ok(V.describePending(plan).kind === 'plan' && /Add the cache/.test(V.describePending(plan).plan), 'a plan comes with its text');
  ok(!V.describePending(collab).answerable && /collaborator/.test(V.describePending(collab).note), 'a collaborator\'s escalation is shown, but not Cru\'s to answer');
  ok(V.pendingLine(question) === 'a question: "Which database should the cache use?" (Postgres, or SQLite)', 'in a sentence: ' + V.pendingLine(question));
  ok(V.pendingLine(multi) === 'a question: "Which checks should run?" (one or more of: Lint, Unit tests, E2E)', 'a multi-select says so');
  ok(V.pendingLine(bash) === 'permission to run "npm test -- --watch=false" (Run the tests)', 'a command in a sentence');
  ok(V.pendingLine(edit) === 'permission to edit /p/src/app.js' && /^approval of its plan/.test(V.pendingLine(plan)), 'an edit, a plan');

  const chats = [{ id: 'c1', project: 'proj', title: 'API work', status: 'running', agentState: 'needs-input', pending: [question, bash] }];
  const tools = V.makeTools({
    listProjects: () => [{ alias: 'proj', name: 'Proj', path: '/nowhere' }], settings: () => ({}), chats: () => chats, terminals: () => [],
    chatHistory: () => ({ state: 'needs-input', messages: [{ kind: 'user', text: 'add a cache' }, { kind: 'assistant-text', text: 'Which one?' }] }),
    services: () => [], kanban: { getBoard: () => ({ tasks: [] }) },
  });
  const T = Object.fromEntries(tools.map(x => [x.name, x]));
  const rc = await T.read_chat.run({ id: 'c1' });
  ok(rc.waitingOnOwner && rc.waitingOnOwner.length === 2 && rc.waitingOnOwner[0].questions[0].options.length === 2 && rc.waitingOnOwner[1].asks.command && /answer_chat_prompt/.test(rc.howToAnswer), 'read_chat: what it waits on, in full, and how to answer');
  const lp = await T.list_panels.run({});
  ok(lp.chats[0].waitingFor.length === 2 && /Which database/.test(lp.chats[0].waitingFor[0]), 'list_panels: what each chat waits for');
  const ov = V.overview({ listProjects: () => [{ alias: 'proj', path: '' }], chats: () => chats, services: () => [], chatHistory: () => ({ messages: [] }), kanban: { getBoard: () => ({ tasks: [] }) } }, null, { gitLog: () => [] });
  ok(ov.active[0].chats[0].waitingOnOwner && /npm test/.test(ov.active[0].chats[0].waitingFor[1]), 'workbench_overview: the same');
}

// ─── answering it ───
function setup({ cards, own = false, admin = false, unattended = false, unrecognised = false, locked = false } = {}) {
  const chats = [{ id: 'c1', project: 'proj', title: 'API work', status: 'running', pending: cards }];
  const answered = [], logged = [], asked = [], started = [];
  let t = 1000;
  const pending = A.createPending({ now: () => t });
  const deps = {
    chats: () => chats, listProjects: () => [{ alias: 'proj', name: 'Proj', path: '/p' }],
    respondChat: (id, a) => { answered.push({ id, ...a }); const c = chats[0]; c.pending = c.pending.filter(p => p.requestId !== a.requestId); return { ok: true }; },
    cruRuns: (id) => own && id === 'c1', adminOn: () => admin, logPromptAnswer: (r) => logged.push(r),
    askOwner: (q) => { asked.push(q); return { ok: true, id: 'ask1' }; }, watch: () => {},
    createChat: async () => ({ ok: true, id: 'c9' }), sendMessage: () => ({ ok: true }), markStarted: (id) => started.push(id),
  };
  const tools = A.makeActionTools(deps, { pending, locked: () => locked, unattended: () => unattended, unrecognised: () => unrecognised, away: () => ({ on: true, allow: ['chats'] }) });
  return { T: Object.fromEntries(tools.map(x => [x.name, x])), answered, logged, asked, started, pending };
}
{
  ok(A.ACTION_TOOL_NAMES.includes('answer_chat_prompt'), 'answer_chat_prompt is one of Cru\'s actions');
  {
    const s = setup({ cards: [{ ...bash }] }); s.pending.newTurn();
    const r = await s.T.answer_chat_prompt.run({ chat: 'API work', decision: 'allow', intent: 'explicit' });
    ok(r.ok && s.answered.length === 1 && s.answered[0].behavior === 'allow' && s.answered[0].requestId === 'rq-3' && s.answered[0].by === 'cru', 'the owner says "allow it": allowed at once, the card marked as Cru\'s answer');
    ok(s.logged.length === 1 && s.logged[0].answer === 'allow' && /as the owner said/.test(s.logged[0].why) && /npm test/.test(s.logged[0].asked), 'and logged: what was asked, what was answered, on whose word');
  }
  {
    const s = setup({ cards: [{ ...bash }] }); s.pending.newTurn();
    const r = await s.T.answer_chat_prompt.run({ chat: 'API work', decision: 'allow', intent: 'inferred' });
    ok(r.needsConfirmation && !s.answered.length && /^Allow the "API work" chat in proj to run "npm test/.test(r.ask), 'Cru\'s own reading in the owner\'s chat: read back first, nothing answered (' + r.ask + ')');
    s.pending.newTurn();
    const y = await s.T.confirm_action.run({ id: r.id });
    ok(y.ok && s.answered.length === 1 && /owner's yes/.test(s.logged[0].why), 'his yes answers it');
  }
  {
    const s = setup({ cards: [{ ...bash }], admin: true }); s.pending.newTurn();
    const r = await s.T.answer_chat_prompt.run({ chat: 'c1', decision: 'allow', intent: 'inferred' });
    ok(r.ok && s.answered.length === 1 && s.logged[0].admin === true && /admin mode/.test(r.how), 'admin mode: a permission in any chat, allowed on Cru\'s own judgement, logged as admin');
    const s2 = setup({ cards: [{ ...question }], admin: true }); s2.pending.newTurn();
    const q = await s2.T.answer_chat_prompt.run({ chat: 'c1', answers: [{ answer: 'SQLite' }], intent: 'inferred' });
    ok(q.needsConfirmation && !s2.answered.length, 'but a question in the owner\'s chat is still his to answer');
  }
  {
    const s = setup({ cards: [{ ...bash }], admin: true, unattended: true });
    const r = await s.T.answer_chat_prompt.run({ chat: 'c1', decision: 'deny', message: 'Not in watch mode', intent: 'inferred' });
    ok(r.ok && s.answered[0].behavior === 'deny' && s.answered[0].message === 'Not in watch mode', 'admin mode, nobody speaking (a wake-up): denied with the reason for Claude');
    const s2 = setup({ cards: [{ ...bash }], unattended: true });
    const r2 = await s2.T.answer_chat_prompt.run({ chat: 'c1', decision: 'allow', intent: 'inferred' });
    ok(r2.askedOwner && !s2.answered.length && s2.asked.length === 1 && /npm test/.test(s2.asked[0].title), 'outside admin mode, nobody speaking: asked on his phone instead');
    await s2.asked[0].run();
    ok(s2.answered.length === 1, 'and answered once he says yes there');
  }
  {
    const s = setup({ cards: [{ ...question }], own: true }); s.pending.newTurn();
    const r = await s.T.answer_chat_prompt.run({ chat: 'c1', answers: [{ question: '1', answer: '2' }], intent: 'inferred' });
    ok(r.ok && s.answered[0].updatedInput.answers['Which database should the cache use?'] === 'SQLite' && s.answered[0].updatedInput.questions, 'a chat Cru runs: its question answered on Cru\'s judgement, "2" is the second option\'s label');
    ok(/chat you run/.test(s.logged[0].why), 'logged as its own call');
    const s2 = setup({ cards: [{ ...multi }], own: true }); s2.pending.newTurn();
    await s2.T.answer_chat_prompt.run({ chat: 'c1', answers: [{ answer: 'lint and e2e' }], intent: 'inferred' });
    ok(s2.answered[0].updatedInput.answers['Which checks should run?'] === 'Lint, E2E', 'a multi-select: several labels, as the chat\'s own card sends them');
    const s3 = setup({ cards: [{ ...question }] }); s3.pending.newTurn();
    await s3.T.answer_chat_prompt.run({ chat: 'c1', answers: [{ answer: 'whichever is faster to set up' }], intent: 'explicit' });
    ok(s3.answered[0].updatedInput.answers['Which database should the cache use?'] === 'whichever is faster to set up', 'his own words, when they are not one of the choices');
    const s4 = setup({ cards: [{ ...question }] }); s4.pending.newTurn();
    await s4.T.answer_chat_prompt.run({ chat: 'c1', answers: [{ answer: 'the second one' }], intent: 'explicit' });
    ok(s4.answered[0].updatedInput.answers['Which database should the cache use?'] === 'SQLite', '"the second one" is the second option');
    const s5 = setup({ cards: [{ ...question }] }); s5.pending.newTurn();
    await s5.T.answer_chat_prompt.run({ chat: 'c1', answers: [{ answer: 'option 1' }], intent: 'explicit' });
    ok(s5.answered[0].updatedInput.answers['Which database should the cache use?'] === 'Postgres', '"option 1" is the first');
  }
  {
    const s = setup({ cards: [{ ...bash }, { ...edit }] }); s.pending.newTurn();
    let e = ''; try { await s.T.answer_chat_prompt.run({ chat: 'c1', decision: 'allow', intent: 'explicit' }); } catch (err) { e = err.message; }
    ok(/2 prompts waiting/.test(e) && !s.answered.length, 'two waiting: it must say which (requestId)');
    const r = await s.T.answer_chat_prompt.run({ chat: 'c1', requestId: 'rq-4', decision: 'allow', intent: 'explicit' });
    ok(r.ok && s.answered[0].requestId === 'rq-4', 'and answers the one named');
  }
  {
    const s = setup({ cards: [{ ...collab }], admin: true }); s.pending.newTurn();
    let e = ''; try { await s.T.answer_chat_prompt.run({ chat: 'c1', decision: 'allow', intent: 'explicit' }); } catch (err) { e = err.message; }
    ok(/not waiting on a question or a permission you can answer/.test(e) && !s.answered.length, 'a collaborator\'s escalation (or a factory\'s) is never answered by Cru, admin mode or not');
  }
  {
    const s = setup({ cards: [{ ...bash }], admin: true, unrecognised: true }); s.pending.newTurn();
    const r = await s.T.answer_chat_prompt.run({ chat: 'c1', decision: 'allow', intent: 'explicit' });
    ok(r.unrecognised && !s.answered.length, 'another voice: nothing answered, admin mode or not');
    const s2 = setup({ cards: [{ ...bash }], locked: true }); s2.pending.newTurn();
    const r2 = await s2.T.answer_chat_prompt.run({ chat: 'c1', decision: 'allow', intent: 'explicit' });
    ok(r2.needsUnlock && !s2.answered.length, 'a locked phone: nothing answered');
    const s3 = setup({ cards: [{ ...bash }], own: true, locked: true }); s3.pending.newTurn();
    const r3 = await s3.T.answer_chat_prompt.run({ chat: 'c1', decision: 'allow', intent: 'inferred' });
    ok(r3.needsUnlock && !s3.answered.length, 'not even in a chat it runs, while he is there with the phone locked');
  }
  {
    const s = setup({ cards: [] }); s.pending.newTurn();
    await s.T.start_chat.run({ project: 'proj', intent: 'explicit' });
    ok(s.started.join() === 'c9', 'a chat Cru starts is marked as one it runs');
  }
}

// ─── the card says Cru answered it ───
{
  const src = readFileSync(join(root, 'src', 'claude-ui.js'), 'utf8').replace(/\r\n/g, '\n');
  const fn = src.match(/\n {2}function respond\(id, \{[^]*?\n {2}\}\n/);
  ok(!!fn, 'respond() found in claude-ui.js');
  const make = new Function('sessions', 'send', 'patchEntry', 'pendingChanged', 'setPlan', 'setState', `${fn[0]}; return respond;`);
  const patches = [];
  const s = { id: 'c1', pending: new Map([['rq-3', { entry: { id: 'e3', toolName: 'Bash' }, request: { tool_name: 'Bash', input: { command: 'ls' }, tool_use_id: 'tu' } }]]) };
  const respond = make(new Map([['c1', s]]), () => true, (_s, _e, p) => patches.push(p), () => {}, () => {}, () => {});
  ok(respond('c1', { requestId: 'rq-3', behavior: 'allow', by: 'cru' }).ok && patches[0].answeredBy === 'cru' && patches[0].status === 'allowed', 'answered by Cru: the card is marked answeredBy cru');
  s.pending.set('rq-7', { entry: { id: 'e7' }, request: { tool_name: 'Bash', input: {} } });
  respond('c1', { requestId: 'rq-7', behavior: 'deny' });
  ok(!('answeredBy' in patches[1]), 'answered by the owner himself: no mark');
}

// ─── both clients show it ───
{
  const web = readFileSync(join(root, 'app', 'vendor', 'claude-chat.js'), 'utf8');
  ok((web.match(/answeredBy === 'cru'/g) || []).length >= 2 && /by Cru/.test(web), 'web and PC: permission and question cards say "by Cru"');
  const kt = readFileSync(join(root, 'android', 'app', 'src', 'main', 'java', 'mv', 'obitech', 'crundi', 'ui', 'chats', 'Cards.kt'), 'utf8');
  ok(/fun byCru/.test(kt) && (kt.match(/byCru\(e\)/g) || []).length >= 2, 'Android: the same, on both cards');
}

// ─── how Cru talks about them ───
{
  const { SYSTEM_PROMPT } = await import('../src/voice/agent.js');
  ok(/answer_chat_prompt/.test(SYSTEM_PROMPT) && /admin mode is on, allow or deny a permission prompt in any chat/.test(SYSTEM_PROMPT), 'the system prompt explains reading prompts out and answering them, and when Cru may on its own');
  ok(/Never say ids, hashes, codes or full version strings aloud/.test(SYSTEM_PROMPT) && /after ---/.test(SYSTEM_PROMPT), 'and never to read ids, hashes or versions aloud: those go on screen');
}

console.log(failed ? `\n${failed} FAILED` : '\nAll passed');
process.exit(failed ? 1 : 0);
