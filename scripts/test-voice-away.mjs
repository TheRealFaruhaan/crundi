#!/usr/bin/env node
/**
 * test-voice-away.mjs — Cru on its own: what runs unattended, what goes to the
 * owner's phone, the heartbeat skipping an empty checklist, jobs, the log.
 * Run: node scripts/test-voice-away.mjs
 */
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
const dir = mkdtempSync(join(tmpdir(), 'cru-away-'));
process.env.DATA_DIR = dir;
const settings = await import('../src/voice/settings.js');
const A = await import('../src/voice/autonomy.js');
const { makeActionTools, createPending } = await import('../src/voice/actions.js');
const { createWakeupStore, createWakeupEngine } = await import('../src/voice/wakeups.js');
const { createJobStore, jobToolDefs, jobPrompt } = await import('../src/voice/jobs.js');
const { createNotebook } = await import('../src/voice/notebook.js');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };

// ─── the rules ───
ok(settings.publicSettings().away.on === false, 'starts off');
ok(!A.awayAllows('move_kanban_task', { status: 'done' }, { on: false, allow: ['kanban'] }), 'off: nothing runs unattended');
ok(A.awayAllows('move_kanban_task', { status: 'done' }, { on: true, allow: ['kanban'] }), 'on + ticked: a card move runs');
ok(!A.awayAllows('move_kanban_task', { status: 'todo' }, { on: true, allow: ['kanban'] }), 'a card to To do is always asked');
ok(!A.awayAllows('start_chat', { mode: 'bypass' }, { on: true, allow: ['chats'] }), 'a bypass chat is always asked');
ok(!A.awayAllows('service_control', { op: 'stop' }, { on: true, allow: ['services'] }), 'stopping a service is always asked');
ok(A.awayAllows('service_control', { op: 'restart' }, { on: true, allow: ['services'] }), 'restarting one runs when ticked');
ok(!A.awayAllows('create_project', {}, { on: true, allow: A.AWAY_CLASS_IDS }), 'what is in no class is always asked');
ok(A.awayAllows('watch_chat', {}, { on: false }), 'following a chat is always fine');
ok(settings.updateSettings({ away: { on: true, allow: ['kanban', 'nonsense'] } }).ok && JSON.stringify(settings.publicSettings().away) === JSON.stringify({ on: true, allow: ['kanban'] }), 'the setting is cleaned');

// ─── the gate ───
const moved = [];
const asked = [];
const ctx = { pending: createPending(), locked: () => false, unrecognised: () => false, unattended: () => true, away: () => settings.getSettings().away };
const deps = {
  listProjects: () => [{ alias: 'p' }], chats: () => [], projectsDir: () => '',
  kanban: { moveTask: (...a) => { moved.push(a); return { ok: true }; }, getTask: () => ({ task: { title: 'Card', status: 'backlog', todos: [] } }), addTask: () => ({ ok: true }) },
  services: () => [{ key: 'web', name: 'Web' }], serviceOp: (k, op) => ({ ok: true, k, op }),
  askOwner: (q) => { asked.push(q); return { ok: true, id: 'cru-1' }; },
  awayOff: () => ({ ok: true }),
};
const tools = makeActionTools(deps, ctx);
const t = (n) => tools.find((x) => x.name === n);
let r = await t('move_kanban_task').run({ project: 'p', taskId: 'x', status: 'done' });
ok(moved.length === 1 && r.ok !== false, 'unattended, ticked: runs without a yes');
r = await t('move_kanban_task').run({ project: 'p', taskId: 'x', status: 'todo' });
ok(moved.length === 1 && r.askedOwner && asked.length === 1, 'unattended, To do: the owner is asked on his phone, nothing done');
r = await t('service_control').run({ service: 'web', op: 'restart', intent: 'explicit' });
ok(r.askedOwner && asked.length === 2, 'unattended, services not ticked: asked');
const ran = await asked[1].run();
ok(ran.ok && ran.op === 'restart', 'the approved action runs server-side');
r = await t('confirm_action').run({ id: 'a1' });
ok(r.done === false && /Nobody has spoken/.test(r.error), 'confirm_action never works unattended');
ctx.unattended = () => false;
r = await t('move_kanban_task').run({ project: 'p', taskId: 'x', status: 'todo' });
ok(r.needsConfirmation, 'attended: the spoken yes as before');
r = await t('on_my_own_off').run({ everything: true });
ok(r.ok, 'on_my_own_off runs without a yes');

// ─── owner asks ───
let done = [];
let clock = 1000;
const asks = A.createOwnerAsks({ now: () => clock, onDone: (a, c) => done.push([a.id, c]) });
const one = asks.ask({ title: 'Restart Web?', run: () => ({ ok: true }) });
ok(one.ok && asks.pending().length === 1, 'an ask waits');
ok(asks.ask({ title: 'Restart Web?', run: () => ({}) }).already, 'the same question is not asked twice');
ok(asks.answer(one.id, 'maybe') === null, 'only yes or no');
ok(asks.answer(one.id, 'yes') && done[0][1] === 'yes', 'answered yes');
asks.ask({ title: 'B', run: () => ({}) });
clock += A.ASK_TIMEOUT_MS + 1;
asks.expire();
ok(done[1][1] === null && !asks.pending().length, 'unanswered: expires, told as no answer');

// ─── heartbeat ───
ok(A.checklistEmpty('# Heartbeat\n\n## x\n') && !A.checklistEmpty(A.HEARTBEAT_DEFAULT), 'an empty checklist is recognised');
const store = createWakeupStore({ builtins: [{ id: A.HEARTBEAT_ID, name: 'Heartbeat', every: 30, purpose: 'look round' }] });
ok(store.find(A.HEARTBEAT_ID)?.builtin && !store.find(A.HEARTBEAT_ID).enabled, 'the heartbeat is built in and off');
let threw = false; try { store.remove(A.HEARTBEAT_ID); } catch { threw = true; }
ok(threw, 'the heartbeat cannot be deleted');
store.setEnabled(A.HEARTBEAT_ID, true);
const notes = [];
let extra = null;
const eng = createWakeupEngine({ store, report: async (n) => { notes.push(n); return { ok: true }; }, prepare: () => extra });
r = await eng.fire(store.find(A.HEARTBEAT_ID));
ok(r.skipped && !notes.length && eng.firedTimes.length === 0, 'nothing to check: no call, not counted');
extra = '\n[Your checklist: services]';
r = await eng.fire({ ...store.find(A.HEARTBEAT_ID), lastFiredAt: 0 });
ok(notes.length === 1 && notes[0].includes('[Your checklist: services]'), 'with a checklist: Cru wakes with it');

// ─── jobs ───
const jobs = createJobStore({ file: join(dir, 'jobs.json') });
const sent = [], watched = [], moves = [];
const jdefs = jobToolDefs({
  jobs, kanban: { getTask: () => ({ task: { title: 'Fix login', description: 'It fails', status: 'backlog', todos: [{ text: 'add test', done: false }] } }), moveTask: (...a) => moves.push(a) },
  project: (a) => ({ alias: a }), createChat: async () => ({ ok: true, id: 'c1' }), sendMessage: (id, text) => sent.push([id, text]),
  watch: (...a) => watched.push(a), unwatch: () => {}, chats: () => [{ id: 'c1', status: 'running', title: 'Fix login' }], log: () => {},
});
const jt = (n) => jdefs.find((x) => x.name === n);
r = await jt('start_job').run({ project: 'p', taskId: 't1' });
ok(r.ok && sent[0][1].includes('Fix login') && sent[0][1].includes('add test') && watched[0][3].follow && moves[0][2] === 'in_progress', 'start_job: a chat on the card, followed, card in progress');
ok(jobs.forChat('c1')?.id === r.job, 'the chat is known as the job\'s');
let dup = false; try { jobs.add({ project: 'p', taskId: 't1', title: 'x', chatId: 'c2' }); } catch { dup = true; }
ok(dup, 'one job per card');
ok(jt('start_job').confirm({ project: 'p', taskId: 't1' }) && !jt('start_job').confirm({ project: 'p', taskId: 't1', intent: 'explicit' }), 'asked unless the owner asked for it');
r = jt('finish_job').run({ job: 't1', outcome: 'done', summary: 'fixed' });
ok(r.ok && moves[1][2] === 'done' && !jobs.list().length, 'finish_job done: card to Done, job closed');
ok(jobPrompt({ title: 'T' }).includes('summary'), 'the chat is asked to end with a summary');
jobs.add({ project: 'p', taskId: 't2', title: 'Two', chatId: 'c3' });
ok(jobs.pauseAll() === 1 && jobs.list()[0].status === 'paused', 'stop everything pauses jobs');

// ─── closing a chat on its own (10 Nov): chats ticked, and read first ───
{
  let clock = 1000;
  const reads = A.createChatReads({ now: () => clock });
  const closed = [], acted = [];
  let away = { on: true, allow: ['chats'] };
  let unattended = false;
  const cctx = { pending: createPending(), locked: () => false, unrecognised: () => false, unattended: () => unattended, away: () => away };
  const cdeps = {
    listProjects: () => [{ alias: 'p' }], projectsDir: () => '',
    chats: () => [{ id: 'c9', title: 'Old fix', project: 'p', status: 'idle' }],
    closeChat: async (id) => { closed.push(id); return { ok: true }; },
    chatRead: (id) => reads.fresh(id), chatReadForget: (id) => reads.forget(id),
    onAwayAct: (tool) => acted.push(tool), askOwner: () => ({ ok: true, id: 'cru-x' }),
  };
  const ct = makeActionTools(cdeps, cctx).find((x) => x.name === 'close_chat');
  let c = await ct.run({ chat: 'c9' });
  ok(c.needsConfirmation && !closed.length, 'not read yet: closing still asks');
  reads.mark('c9');
  c = await ct.run({ chat: 'c9' });
  ok(c.ok && c.onMyOwn && closed.join() === 'c9' && acted.includes('close_chat'), 'chats ticked and read: closed at once, no yes, logged');
  ok(!reads.fresh('c9'), 'a closed chat is forgotten');
  reads.mark('c9'); clock += A.CLOSE_READ_MS + 1;
  c = await ct.run({ chat: 'c9' });
  ok(c.needsConfirmation && closed.length === 1, 'read too long ago: asks again');
  reads.mark('c9'); away = { on: true, allow: ['kanban'] };
  c = await ct.run({ chat: 'c9' });
  ok(c.needsConfirmation && closed.length === 1, 'chats not ticked: asks');
  away = { on: false, allow: ['chats'] };
  c = await ct.run({ chat: 'c9' });
  ok(c.needsConfirmation && closed.length === 1, '"Cru on its own" off: asks');
  away = { on: true, allow: ['chats'] }; unattended = true;
  c = await ct.run({ chat: 'c9' });
  ok(c.ok && closed.length === 2, 'unattended (a wake-up) too, once read');
  cctx.unrecognised = () => true; reads.mark('c9');
  c = await ct.run({ chat: 'c9' });
  ok(c.unrecognised && closed.length === 2, 'another voice still changes nothing');
  ok(A.AWAY_CLASSES.chats.label.includes('close'), 'the "on its own" tick says it covers closing');
  const VT = await import('../src/voice/tools.js');
  const seen = [];
  const rc = VT.makeTools({ listProjects: () => [], chats: () => [], chatHistory: (id) => (id === 'c9' ? { state: 'idle', messages: [] } : null), onReadChat: (id) => seen.push(id) }).find((x) => x.name === 'read_chat');
  await rc.run({ id: 'c9' });
  ok(seen.join() === 'c9', 'read_chat records that Cru checked the chat');
}

// ─── Cru's background-listening order reaches the phone (6 Nov) ───
{
  const told = [];
  const bctx = { pending: createPending(), locked: () => false, unrecognised: () => false, unattended: () => false, away: () => null };
  const bt = makeActionTools({ listProjects: () => [], chats: () => [], projectsDir: () => '', broadcast: (tool, input) => told.push([tool, input]) }, bctx);
  const sbl = bt.find((x) => x.name === 'set_background_listening'), conf = bt.find((x) => x.name === 'confirm_action');
  bctx.pending.newTurn?.();
  const asked = await sbl.run({ minutes: 0 });
  ok(asked.needsConfirmation && !told.length, 'off is asked first');
  bctx.pending.newTurn?.();
  const done = await conf.run({ id: asked.id });
  ok(done.ok && done.phoneAction && done.phoneAction.type === 'background-listening' && done.phoneAction.minutes === 0, 'confirmed: the reply carries the order for the phone');
  ok(told.length === 1 && told[0][0] === 'set_background_listening' && told[0][1].minutes === 0, 'and it is broadcast, so a phone gets it when the yes came from elsewhere');
}

// ─── work the owner gave Cru needs no yes; a question goes to the voice screen first (10 Nov) ───
{
  const qjobs = createJobStore({ file: join(dir, 'jobs-q.json') });
  let chatN = 0; const qsent = [], qasked = [];
  let here = false;
  const qctx = { pending: createPending(), locked: () => false, unrecognised: () => false, unattended: () => true, away: () => ({ on: false, allow: [] }) };
  const qdeps = {
    listProjects: () => [{ alias: 'p' }], projectsDir: () => '',
    chats: () => [{ id: 'c1', title: 'Job chat', project: 'p' }, { id: 'c9', title: 'Owner chat', project: 'p' }],
    sendMessage: (id, text) => { qsent.push([id, text]); return { ok: true }; },
    cruRuns: (id) => id === 'c1' || !!qjobs.forChat(id),
    askOwner: (q) => { qasked.push(q); return { ok: true, id: 'cru-q' }; },
    ownerHere: () => here, escalateMs: 60,
    jobDefs: jobToolDefs({
      jobs: qjobs, kanban: { getTask: (a, id) => ({ task: { title: 'Card ' + id, status: 'backlog', todos: [] } }), moveTask: () => {} },
      project: (a) => ({ alias: a }), createChat: async () => ({ ok: true, id: 'j' + (++chatN) }), sendMessage: () => {}, watch: () => {}, unwatch: () => {}, chats: () => [], log: () => {},
    }),
  };
  const qt = makeActionTools(qdeps, qctx);
  const T = (n) => qt.find((x) => x.name === n);
  qctx.unattended = () => false;
  let q = await T('queue_jobs').run({ project: 'p', taskIds: ['k1', 'k2'], intent: 'explicit' });
  ok(q.ok && q.added === 2 && qjobs.queue().map((x) => x.taskId).join() === 'k1,k2', 'the owner queues cards: no question (his word is the yes)');
  qctx.unattended = () => true;
  q = await T('start_job').run({ project: 'p', taskId: 'k1' });
  ok(q.ok && q.job && !qasked.length && qjobs.queue().map((x) => x.taskId).join() === 'k2', 'on a wake-up, a queued card starts without asking; it leaves the queue');
  q = await T('finish_job').run({ job: 'k1', outcome: 'done', summary: 'did it' });
  ok(q.ok && q.next && q.next.taskId === 'k2' && !qasked.length, 'finishing its own job needs no yes, and says what is queued next');
  q = await T('start_job').run({ project: 'p', taskId: 'k2' });
  ok(q.ok && !qasked.length && !qjobs.queue().length, 'and the next queued card starts on its own');
  q = await T('send_to_chat').run({ chat: 'c1', text: 'Now run the tests', intent: 'inferred' });
  ok(q.ok && qsent.length === 1 && !qasked.length, 'the next step in a chat Cru runs needs no yes');
  // Genuinely its own call, and the owner away: his phone, as before.
  q = await T('start_job').run({ project: 'p', taskId: 'x9' });
  ok(q.askedOwner && qasked.length === 1, 'a card he did not queue, the owner away: asked on his phone');
  // The owner at a voice screen: in the conversation first.
  here = true;
  q = await T('send_to_chat').run({ chat: 'c9', text: 'Restart it', intent: 'inferred' });
  ok(q.needsConfirmation && q.id && /voice screen/.test(q.rule) && qasked.length === 1, 'the owner at a voice screen: asked in the conversation, not on his phone');
  await new Promise((r) => setTimeout(r, 120));
  ok(qasked.length === 2 && qasked[1].tool === 'send_to_chat' && !qctx.pending.peek(q.id), 'unanswered there: it goes to his phone');
  const q2 = await T('send_to_chat').run({ chat: 'c9', text: 'Restart it again', intent: 'inferred' });
  qctx.unattended = () => false; qctx.pending.newTurn();
  const yes = await T('confirm_action').run({ id: q2.id });
  await new Promise((r) => setTimeout(r, 120));
  ok(yes.ok && qsent.length === 2 && qasked.length === 2, 'answered there: done, and his phone is never asked');
}

// ─── follow-ups to work he gave Cru need no yes either: schedules, card moves (11 Nov) ───
{
  const fjobs = createJobStore({ file: join(dir, 'jobs-f.json') });
  fjobs.add({ project: 'p', taskId: 'k1', title: 'Card k1', chatId: 'c1' });
  fjobs.enqueue([{ project: 'p', taskId: 'k2', title: 'Card k2' }]);
  const fasked = [], scheds = [], moved = [], logged = [];
  const fctx = { pending: createPending(), locked: () => false, unrecognised: () => false, unattended: () => true, away: () => ({ on: false, allow: [] }) };
  const fdeps = {
    listProjects: () => [{ alias: 'p' }], projectsDir: () => '',
    chats: () => [{ id: 'c1', title: 'Job chat', project: 'p' }, { id: 'c9', title: 'Owner chat', project: 'p' }],
    cruRuns: (id) => !!fjobs.forChat(id),
    cruCard: (a, t) => fjobs.list().some((j) => j.project === a && j.taskId === t) || fjobs.queued(a, t),
    askOwner: (q) => { fasked.push(q); return { ok: true, id: 'cru-f' }; },
    onAwayAct: (name) => logged.push(name),
    chatSchedule: {
      list: () => scheds.filter((x) => x.status === 'pending'),
      add: (it) => { const x = { id: 's' + (scheds.length + 1), chat: it.chatId ? { id: it.chatId } : null, status: 'pending', ...it }; scheds.push(x); return { ok: true, item: x }; },
      remove: (id) => { const x = scheds.find((y) => y.id === id); if (x) x.status = 'gone'; return { ok: !!x }; },
    },
    kanban: { moveTask: (a, t, st) => { moved.push([t, st]); return { ok: true }; }, updateTask: () => ({ ok: true }), addTodo: () => ({ ok: true }) },
  };
  const ft = makeActionTools(fdeps, fctx);
  const F = (n) => ft.find((x) => x.name === n);
  let r = await F('add_chat_schedule').run({ project: 'p', chat: 'c1', text: 'The limit has reset: carry on', trigger: 'limit-reset' });
  ok(r.ok && scheds.length === 1 && !fasked.length && logged.includes('add_chat_schedule'), 'the usage guard\'s resume into its job chat: scheduled on its own, and logged');
  r = await F('delete_chat_schedule').run({ id: 's1' });
  ok(r.ok && !fasked.length, 'and cancelling one into its own chat needs no yes');
  r = await F('move_kanban_task').run({ project: 'p', taskId: 'k1', status: 'done' });
  ok(r.ok && moved.length === 1 && !fasked.length, 'moving the card of its own job: no yes');
  r = await F('move_kanban_task').run({ project: 'p', taskId: 'k2', status: 'in_progress' });
  ok(r.ok && moved.length === 2 && !fasked.length, 'nor a card in the owner\'s queue');
  r = await F('move_kanban_task').run({ project: 'p', taskId: 'k1', status: 'todo' });
  ok(r.askedOwner && moved.length === 2, 'but To do (a factory run) is still asked');
  r = await F('add_chat_schedule').run({ project: 'p', chat: 'c9', text: 'Restart it', trigger: 'turn-end' });
  ok(r.askedOwner && scheds.length === 1, 'a schedule into a chat that is not Cru\'s is still asked');
  r = await F('add_chat_schedule').run({ project: 'p', text: 'Start fresh', trigger: 'limit-reset' });
  ok(r.askedOwner && scheds.length === 1, 'and so is one that starts a new chat');
  r = await F('move_kanban_task').run({ project: 'p', taskId: 'z9', status: 'done' });
  ok(r.askedOwner && moved.length === 2, 'and a card that is not its work');
  fctx.away = () => ({ on: true, allow: ['schedules'] });
  r = await F('add_chat_schedule').run({ project: 'p', chat: 'c9', text: 'Restart it later', trigger: 'turn-end' });
  ok(r.ok && scheds.length === 2, 'with schedules ticked under "Cru on its own", any chat schedule runs');
  ok(A.classOf('add_chat_schedule') === 'schedules' && !A.AWAY_DEFAULT_ALLOW.includes('schedules'), 'schedules is its own tick, off by default');
}

// ─── the day's log ───
const nb = createNotebook(join(dir, 'nb'));
const log = A.createDailyLog(nb, { now: () => new Date(2026, 10, 9, 14, 5) });
log.add('Restarted web');
ok(nb.read('/memories/log/2026-11-09.md').includes('- 14:05 Restarted web'), 'a line in today\'s log');
ok(log.tail(5).includes('Restarted web'), 'its tail for the heartbeat');
nb.save('/memories/log/2026-09-01.md', '# old\n- x\n');
log.prune(new Date(2026, 10, 9));
ok(!nb.read('/memories/log/2026-09-01.md'), 'logs older than 30 days go');

// ─── silence, said ───
const AG = await import('../src/voice/agent.js');
ok(AG.splitReply('QUIET').speak === '' && AG.splitReply('[mood: happy] QUIET.').speak === '' && AG.splitReply('HEARTBEAT_OK').speak === '', 'QUIET alone is silence');
ok(AG.splitReply('It is quiet today, Sir.').speak !== '', 'a real sentence with the word is still said');

if (failed) { console.log(`\n${failed} failed`); process.exit(1); }
console.log('\nAll on-my-own checks passed');
