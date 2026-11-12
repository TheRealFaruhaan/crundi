// Cru moves the owner's Crundi page (voice/ui-nav.js): which page, what is
// checked before anything is sent, and what Cru is told back.
import assert from 'node:assert/strict';
import { createUiTarget, createAcks, makeUiNavTools, TARGET_MS, UI_NAV_TOOL_NAMES } from '../src/voice/ui-nav.js';

const keep = setInterval(() => {}, 1000); // the ack timer is unref'd
let clock = 1_000_000;
const target = createUiTarget({ now: () => clock });
const acks = createAcks({ timeoutMs: 80 });
const sent = [];
let reply = (msg) => ({ ok: true });
const send = (msg) => { sent.push(msg); setTimeout(() => { const r = reply(msg); if (r) acks.answer(msg.id, r); }, 5); };
const [tool] = makeUiNavTools({
  target, acks, send,
  projects: () => [{ alias: 'crundi', name: 'Crundi' }, { alias: 'shop', name: 'Dhivo Shop' }],
  chats: () => [{ id: 'c1', project: 'shop', title: 'API' }],
});
assert.deepEqual(UI_NAV_TOOL_NAMES, [tool.name]);

// No page yet: nothing is sent.
let r = await tool.run({ tab: 'kanban' });
assert.equal(r.ok, false); assert.match(r.error, /PC app or the web/); assert.equal(sent.length, 0);

// The phone: refused, and says so.
target.note({ page: '', deviceId: 'android', device: 'Android phone, the Cru bubble' });
r = await tool.run({ tab: 'kanban' });
assert.equal(r.ok, false); assert.match(r.error, /phone/); assert.equal(sent.length, 0);

// A page id from a phone does not make it a page.
target.note({ page: 'pgX', deviceId: 'android', device: 'phone' });
assert.ok(target.get().phone);

// The web page: the move goes to that page.
target.note({ page: 'pg1', deviceId: 'web', device: 'web browser, Chrome on Windows' });
r = await tool.run({ project: 'dhivo shop', tab: 'kanban', highlight: '  Backlog ', note: 'here' });
assert.equal(r.ok, true); assert.equal(r.found, true);
assert.deepEqual({ ...sent[0], id: undefined }, { id: undefined, page: 'pg1', project: 'shop', tab: 'kanban', highlight: 'Backlog', note: 'here' });

// A card puts the board up; a chat the workbench and its project.
r = await tool.run({ card: 't9' });
assert.equal(sent.at(-1).tab, 'kanban'); assert.equal(sent.at(-1).card, 't9');
assert.equal(sent.at(-1).open, undefined); // shown, not opened, unless asked
r = await tool.run({ node: 'n4' });
assert.equal(sent.at(-1).tab, 'mindmap'); assert.equal(sent.at(-1).open, undefined);
r = await tool.run({ node: 'n4', open: true });
assert.equal(sent.at(-1).node, 'n4'); assert.equal(sent.at(-1).open, true);
r = await tool.run({ card: 't9', open: true });
assert.equal(sent.at(-1).open, true);
r = await tool.run({ chat: 'c1' });
assert.equal(sent.at(-1).project, 'shop'); assert.equal(sent.at(-1).tab, 'workbench');

// Checked before sending.
const n = sent.length;
for (const bad of [{}, { project: 'nope' }, { tab: 'nowhere' }, { chat: 'zz' }, { chat: 'c1', project: 'crundi' }, { chat: 'c1', tab: 'git' }, { card: 't1', tab: 'git' }, { file: '/etc/passwd' }, { file: '../x' }, { card: 't1', node: 'n1' }, { tab: 'kanban', open: true }]) {
  r = await tool.run(bad);
  assert.equal(r.ok, false, JSON.stringify(bad));
}
assert.equal(sent.length, n);

// A file goes to the files tab.
r = await tool.run({ file: 'src/index.js' });
assert.equal(r.ok, true); assert.equal(sent.at(-1).tab, 'files');

// Not found on the page; the page could not; the page did not answer.
reply = () => ({ ok: true, found: false });
r = await tool.run({ highlight: 'Nothing like this' });
assert.equal(r.ok, true); assert.equal(r.found, false); assert.match(r.note, /not found/);
reply = () => ({ ok: false, error: 'No project is open on the page' });
r = await tool.run({ tab: 'git' });
assert.equal(r.ok, false); assert.match(r.error, /No project/);
reply = () => null;
r = await tool.run({ tab: 'git' });
assert.equal(r.ok, true); assert.equal(r.unconfirmed, true);
// An answer nobody waits for is not taken.
assert.equal(acks.answer('nobody', { ok: true }), false);

// The PC app counts; a target is forgotten after half an hour.
target.note({ page: 'pg2', deviceId: 'pc-app', device: 'PC app' });
assert.equal(target.get().page, 'pg2');
clock += TARGET_MS + 1;
assert.equal(target.get(), null);

clearInterval(keep);
console.log('voice ui-nav: ok');
