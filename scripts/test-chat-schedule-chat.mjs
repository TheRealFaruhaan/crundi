#!/usr/bin/env node
/**
 * test-chat-schedule-chat.mjs — a scheduled message goes to the chat it was
 * written in, not to whichever chat of the project is idle.
 *
 * Covers: two chats in one project, the message bound to one, delivered only
 * there even while the other is idle; waiting while its own chat is busy or
 * closed; found again by its conversation after a restart gives the chat a
 * new id; each chat's panel listing only its own (plus older unbound ones);
 * a turn end in another chat not firing it; older items without a chat still
 * going to any idle chat; a chat with a message to come kept awake; a
 * parked chat woken (resumed in place) and the message delivered there.
 *
 * Run: node scripts/test-chat-schedule-chat.mjs
 */
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const dir = mkdtempSync(join(tmpdir(), 'chat-sched-chat-'));
process.env.DATA_DIR = dir;
process.env.CRUNDI_DATA_DIR = dir;
const { createChatSchedule } = await import('../src/chat-schedule.js');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };

const sessions = [
  { id: 'A', project: 'crundi', status: 'running', agentState: 'working', sessionId: 'conv-A', title: 'Native app' },
  { id: 'B', project: 'crundi', status: 'running', agentState: 'idle', sessionId: 'conv-B', title: 'Other chat' },
];
const sent = [];
const claudeUi = {
  list: () => sessions,
  meta: (id) => { const s = sessions.find(x => x.id === id); return s ? { id: s.id, sessionId: s.sessionId, title: s.title } : null; },
  sendMessage: (id, text) => { sent.push({ id, text }); return { ok: true }; },
};
const cs = createChatSchedule({ claudeUi, getLatestUsage: () => null });

const r = cs.add({ project: 'crundi', text: 'for chat A', trigger: { type: 'turn-end' }, chatId: 'A' });
ok(r.ok && r.item.chat.id === 'A' && r.item.chat.conversation === 'conv-A', 'add: bound to the chat it was written in, by id and conversation');
ok(!cs.add({ project: 'crundi', text: 'x', trigger: { type: 'turn-end' }, chatId: 'Z' }).ok, 'add: refused for a chat that is not open');

cs.onTurnEnd('crundi', 'B');
ok(sent.length === 0, 'turn end in another chat: not delivered there');
ok(cs.list('crundi', 'A').length === 1 && cs.list('crundi', 'B').length === 0, 'list: each chat shows only its own');

// A busy: waits. A idle: delivered to A.
cs.onTurnEnd('crundi', 'A');   // A reported idle by the hook, but our fake still says working
ok(sent.length === 0, 'its chat busy: it waits, and never goes to the idle chat B');
sessions[0].agentState = 'idle';
cs.onTurnEnd('crundi', 'A');
ok(sent.length === 1 && sent[0].id === 'A' && sent[0].text === 'for chat A', 'its chat free: delivered there');

// After a restart: the chat comes back with a new id, same conversation.
const r2 = cs.add({ project: 'crundi', text: 'after restart', trigger: { type: 'turn-end' }, chatId: 'A' });
sessions[0] = { ...sessions[0], id: 'A2', agentState: 'idle' };
cs.onTurnEnd('crundi', 'A2');
ok(r2.ok && sent.length === 2 && sent[1].id === 'A2', 'restart: found again by its conversation');

// Closed: waits.
const r3 = cs.add({ project: 'crundi', text: 'while closed', trigger: { type: 'turn-end' }, chatId: 'A2' });
sessions.splice(0, 1);
cs.onTurnEnd('crundi', 'B');
ok(r3.ok && sent.length === 2 && cs.list('crundi').some(i => i.text === 'while closed'), 'its chat closed: it waits, not sent to B');

// Older items without a chat: any idle chat, as before.
const r4 = cs.add({ project: 'crundi', text: 'unbound', trigger: { type: 'turn-end' } });
cs.onTurnEnd('crundi', 'B');
ok(r4.ok && sent.at(-1).text === 'unbound' && sent.at(-1).id === 'B', 'unbound (older) items: any idle chat, as before');

// Asleep: the chat with 'while closed' was parked. A schedule that knows the
// panes wakes it and delivers once it is back (under a new id, same conversation).
const parked = new Set(['A2']);
let resumed = 0;
const panes = {
  has: (id) => parked.has(id),
  resume: async (id) => { resumed++; parked.delete(id); sessions.push({ id: 'A3', project: 'crundi', status: 'running', agentState: 'idle', sessionId: 'conv-A', title: 'Native app' }); return { ok: true, id: 'A3' }; },
};
const cs2 = createChatSchedule({ claudeUi, getLatestUsage: () => null, panes });
ok(cs2.hasPendingFor('A2'), 'keep awake: a chat with a message to come says so');
const r5 = cs2.add({ project: 'crundi', text: 'later for B', trigger: { type: 'time', at: new Date(Date.now() + 3600_000).toISOString() }, chatId: 'B' });
ok(r5.ok && cs2.hasPendingFor('B') && !cs2.hasPendingFor('nobody'), 'keep awake: only the chats with something pending');
cs2.tick();
cs2.tick();
ok(resumed === 1, 'parked: woken once, not once per tick');
await new Promise(res => setTimeout(res, 6000));
ok(sent.at(-1).text === 'while closed' && sent.at(-1).id === 'A3', 'parked: delivered to the woken chat');
ok(!cs2.hasPendingFor('A3'), 'parked: nothing left pending for it');

rmSync(dir, { recursive: true, force: true });
console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
