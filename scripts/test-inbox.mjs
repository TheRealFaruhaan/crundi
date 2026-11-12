#!/usr/bin/env node
/**
 * test-inbox.mjs — Crundi's own inbox.
 *
 *  - the store: add, page, read, delete, media copied (and still there after
 *    the original is gone), retention (read > 90 days, the cap) and 0600.
 *  - the MCP tools (send_message/photo/file_to_user) land in the inbox with
 *    no Telegram at all, and the channels that are on get copies.
 *  - the inbox channel is always on and never sent through.
 *  - the API is the owner's: a collaborator is refused.
 *
 * Run: node scripts/test-inbox.mjs
 */

import { mkdtempSync, rmSync, writeFileSync, readFileSync, existsSync, statSync, unlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const tmp = mkdtempSync(join(tmpdir(), 'crundi-inbox-'));
process.env.DATA_DIR = join(tmp, 'data');

const { createInboxStore, splitText, cleanTitle, safeName, READ_TTL_MS, threadOf } = await import('../src/inbox-store.js');
const { createOwnerMessenger, INBOX_EVENTS } = await import('../src/inbox-send.js');
const channels = await import('../src/notify-channels.js');
const { mayAccess, ROLE_OWNER, ROLE_COLLABORATOR } = await import('../src/access-policy.js');
const { CAPABILITIES } = await import('../src/client-version.js');
const { buildPayload } = await import('../src/mobile-push.js');

let failures = 0;
const ok = (cond, label, detail) => {
  if (!cond) failures++;
  console.log(`${cond ? 'PASS' : 'FAIL'}  ${label}${detail === undefined ? '' : ` -> ${detail}`}`);
};

const PNG = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==', 'base64');

try {
  // ── Text helpers ─────────────────────────────────────────────────────────
  ok(cleanTitle('⏳ Chat needs your input') === 'Chat needs your input', 'a leading pictograph is dropped from a title');
  const sp = splitText('First line\nsecond\nthird');
  ok(sp.title === 'First line' && sp.text === 'second\nthird', 'a message splits into title and the rest');
  ok(safeName('../../etc/passwd') === 'passwd' && safeName('..') === 'file', 'file names cannot climb out');

  // ── The store ─────────────────────────────────────────────────────────────
  let clock = 1_800_000_000_000;
  const dir1 = join(tmp, 'store1');
  const st = createInboxStore({ dataDir: dir1, now: () => clock });
  const events = [];
  st.onChange((e) => events.push(e));
  const a = st.add({ kind: 'message', text: 'Hello there\nMore text', chatId: 'c1', chatTitle: 'Fix login', project: 'demo' });
  clock += 1000;
  ok(a.title === 'Hello there' && a.text === 'More text', 'title is the first line', a.title);
  ok(a.link === '/chat/c1', 'an item from a chat links to it', a.link);
  ok(events.at(-1).type === 'inbox' && events.at(-1).unread === 1 && events.at(-1).item?.id === a.id, 'adding announces { type, unread, item }');
  ok(statSync(st.file).mode % 0o1000 === 0o600 || process.platform === 'win32', 'inbox.json is 0600');

  const src = join(tmp, 'shot.png'); writeFileSync(src, PNG);
  const b = st.add({ kind: 'photo', title: 'A screenshot', files: [{ path: src }] }); clock += 1000;
  ok(b.media.length === 1 && b.media[0].url.startsWith(`/api/inbox/media/${b.id}/`) && b.media[0].mime === 'image/png' && b.media[0].size === PNG.length, 'a picture is copied in with url, name, mime and size');
  unlinkSync(src);
  const mp = st.mediaPath(b.id, 'shot.png');
  ok(mp && existsSync(mp.path) && readFileSync(mp.path).equals(PNG), 'the copy survives the original being deleted');
  ok(st.mediaPath(b.id, '../inbox.json') === null && st.mediaPath('nope', 'shot.png') === null, 'media lookups stay inside the item');
  const c = st.add({ kind: 'nonsense', title: 'odd kind' }); clock += 1000;
  ok(c.kind === 'message', 'an unknown kind becomes a message');

  let l = st.list({ limit: 2 });
  ok(l.items.length === 2 && l.items[0].id === c.id && l.items[1].id === b.id && l.unread === 3, 'newest first, with the unread count');
  l = st.list({ limit: 2, before: b.id });
  ok(l.items.length === 1 && l.items[0].id === a.id, 'before=<id> pages on');
  l = st.list({ before: String(b.at) });
  ok(l.items.length === 1 && l.items[0].id === a.id, 'before=<at> pages on');
  ok(['id', 'at', 'kind', 'title', 'text', 'project', 'chatId', 'chatTitle', 'media', 'read', 'link'].every((k) => k in l.items[0]), 'items carry every contract field');

  ok(st.markRead({ ids: [a.id] }) === 1 && st.unread() === 2, 'mark one read');
  ok(st.markRead({ all: true }) === 2 && st.unread() === 0, 'mark all read');
  ok(st.markRead({ ids: [c.id], read: false }) === 1 && st.unread() === 1, 'and back to unread');
  ok(st.remove({ ids: [b.id] }) === 1 && !existsSync(mp.path), 'deleting an item deletes its files');
  ok(st.remove({ allRead: true }) === 1 && st.list().items.length === 1 && st.list().items[0].id === c.id, 'delete all read keeps the unread');

  const n1 = st.add({ kind: 'system', title: 'Update available', collapse: true });
  const n2 = st.add({ kind: 'system', title: 'Update available', collapse: true });
  ok(!st.get(n1.id) && st.get(n2.id) && st.list({ limit: 200 }).items.filter((x) => x.title === 'Update available').length === 1, 'the same unread notice again replaces the old one');
  st.remove({ ids: [n2.id] });

  // Persisted, and read back by a fresh store.
  const again = createInboxStore({ dataDir: dir1, now: () => clock });
  ok(again.list().items.length === 1 && again.unread() === 1, 'the inbox survives a restart');

  // ── Threads ───────────────────────────────────────────────────────────────
  {
    const dirT = join(tmp, 'storeT');
    const conv = { c1: 'conv-A', c2: 'conv-A' };   // c2 is c1 resumed under a new id
    let tt = 1_800_000_000_000;
    const T = createInboxStore({ dataDir: dirT, now: () => (tt += 1000), convOf: (id) => conv[id] || '' });
    const m1 = T.add({ kind: 'message', title: 'from chat', chatId: 'c1', chatTitle: 'Fix login', project: 'demo' });
    const m2 = T.add({ kind: 'question', title: 'needs you', chatId: 'c1', chatTitle: 'Fix login', project: 'demo' });
    const m3 = T.add({ kind: 'photo', title: 'after resume', chatId: 'c2', chatTitle: 'Fix login', project: 'demo' });
    const p1 = T.add({ kind: 'alert', title: 'Service web crashed', project: 'Demo' });
    const r1 = T.add({ kind: 'report', title: 'Cru says', chatId: 'c1', cru: true });
    const s1 = T.add({ kind: 'system', title: 'Update available' });
    const f1 = T.add({ kind: 'report', title: 'Factory merged', chatId: 'mgr', project: 'demo', thread: 'project:demo' });
    ok(m1.thread === 'chat:c1' && m2.thread === 'chat:c1', 'a chat\'s items share its thread');
    ok(m3.thread === 'chat:c1', 'a chat resumed under a new id keeps its thread (same conversation)', m3.thread);
    ok(p1.thread === 'project:demo', 'an item with a project and no chat goes to the project', p1.thread);
    ok(r1.thread === 'cru' && s1.thread === 'system' && f1.thread === 'project:demo', 'Cru, Crundi, and an explicit thread');
    const th = T.threads();
    ok(th.map((x) => x.key).join() === 'project:demo,system,cru,chat:c1', 'threads newest activity first', th.map((x) => x.key).join());
    const chat = th.find((x) => x.key === 'chat:c1');
    ok(chat.kind === 'chat' && chat.title === 'Fix login' && chat.subtitle === 'demo' && chat.unread === 3 && chat.count === 3 && chat.last.id === m3.id, 'a chat thread: title, subtitle, last, unread, count');
    const cru = th.find((x) => x.key === 'cru'); const sys = th.find((x) => x.key === 'system'); const prj = th.find((x) => x.key === 'project:demo');
    ok(cru.title === 'Cru' && cru.kind === 'cru' && sys.title === 'Crundi' && sys.kind === 'system' && prj.kind === 'project' && prj.title === 'demo' && prj.count === 2, 'the Cru, Crundi and project threads');
    ok(['key', 'kind', 'title', 'subtitle', 'project', 'chatId', 'last', 'unread', 'count'].every((k) => k in chat) && ['id', 'at', 'kind', 'title', 'text'].every((k) => k in chat.last), 'threads carry every contract field');
    const inT = T.list({ thread: 'chat:c1' }).items;
    ok(inT.length === 3 && inT[0].id === m3.id && inT.every((x) => x.thread === 'chat:c1'), 'listing one thread, newest first');
    ok(T.list({ thread: 'chat:c1', limit: 1, before: m3.id }).items[0].id === m2.id, 'and paging inside it');
    const evs = []; T.onChange((e) => evs.push(e));
    ok(T.markRead({ thread: 'chat:c1' }) === 3 && T.threads().find((x) => x.key === 'chat:c1').unread === 0 && evs.at(-1).thread === 'chat:c1', 'mark a thread read (the event names it)');
    T.add({ title: 'x', chatId: 'c9' });
    ok(evs.at(-1).thread === 'chat:c9' && evs.at(-1).item, 'an added item\'s event names its thread');
    ok(T.remove({ thread: 'project:demo' }) === 2 && !T.threads().some((x) => x.key === 'project:demo'), 'delete a whole thread');
    // Items from before threads sort themselves in.
    writeFileSync(join(dirT, 'inbox.json'), JSON.stringify({ v: 1, items: [
      { id: 'old1', at: Date.now() - 1000, kind: 'message', title: 'a', chatId: 'cc', project: 'p', media: [], read: false },
      { id: 'old2', at: Date.now() - 2000, kind: 'alert', title: 'b', project: 'P2', media: [], read: true },
      { id: 'old3', at: Date.now() - 3000, kind: 'system', title: 'c', media: [], read: false },
      { id: 'old4', at: Date.now() - 4000, kind: 'report', title: 'd', chatId: 'cc', media: [], read: false },
    ] }));
    const O = createInboxStore({ dataDir: dirT });
    ok(O.list().items.map((x) => x.thread).join() === 'chat:cc,project:p2,system,cru', 'old items get their thread on read', O.list().items.map((x) => x.thread).join());
    ok(threadOf({ chatId: 'z' }) === 'chat:z' && threadOf({}) === 'system', 'threadOf from the fields');
  }

  // ── Retention ─────────────────────────────────────────────────────────────
  let t2 = 1_700_000_000_000;
  const r = createInboxStore({ dataDir: join(tmp, 'store2'), now: () => t2, maxItems: 5 });
  const oldRead = r.add({ title: 'old read' }); r.markRead({ ids: [oldRead.id] });
  const oldUnread = r.add({ title: 'old unread' });
  t2 += READ_TTL_MS + 1000;
  r.prune();
  const ids = r.list({ limit: 200 }).items.map((x) => x.id);
  ok(!ids.includes(oldRead.id) && ids.includes(oldUnread.id), 'read items older than 90 days go; unread stay');
  const made = [];
  for (let i = 0; i < 6; i++) { t2 += 1000; made.push(r.add({ title: 'n' + i })); }
  r.markRead({ ids: [made[3].id] });
  t2 += 1000; r.add({ title: 'one more' });
  const left = r.list({ limit: 200 }).items.map((x) => x.id);
  ok(left.length === 5, 'the cap holds', left.length);
  ok(!left.includes(made[3].id), 'past the cap, a read item goes before older unread ones');

  // ── Channels: the inbox is always on and never sent through ──────────────
  channels._reset();
  channels.register({ id: 'inbox', label: 'Crundi inbox', alwaysOn: true, passive: true, send: async () => { throw new Error('the inbox must not be sent through'); } });
  ok(channels.isEnabled('inbox'), 'the inbox channel is on');
  ok(!channels.setEnabled('inbox', false).ok && channels.isEnabled('inbox'), 'and cannot be switched off');
  ok(channels.list().find((x) => x.id === 'inbox')?.alwaysOn === true, 'Settings is told it is always on');

  // ── The tools with no Telegram at all ────────────────────────────────────
  const box = createInboxStore({ dataDir: join(tmp, 'store3') });
  const msgr = createOwnerMessenger({ inbox: box, deliver: (t, m, o) => channels.deliver(t, m, o), labelOf: (id) => (channels.list().find((x) => x.id === id) || {}).label || id });
  let res = await msgr.sendMessage({ message: 'Build finished\nAll green.' }, { chat: { id: 'c9', title: 'CI', project: 'demo' } });
  let got = box.list().items[0];
  ok(res.ok && got && got.kind === 'message' && got.title === 'Build finished' && got.chatId === 'c9' && got.project === 'demo', 'send_message_to_user lands in the inbox with no Telegram');
  ok(/waits there/.test(res.note) && res.delivered.join() === 'inbox', 'and says it waits there', res.note);

  const img = join(tmp, 'pic.png'); writeFileSync(img, PNG);
  res = await msgr.sendPhoto({ path: img, caption: 'The new header' });
  got = box.list().items[0];
  ok(res.ok && got.kind === 'photo' && got.title === 'The new header' && got.media[0]?.mime === 'image/png', 'send_photo_to_user lands as a picture');
  res = await msgr.sendPhoto({ data: PNG.toString('base64') });
  ok(res.ok && box.list().items[0].media[0]?.url, 'a picture given as bytes is kept too');
  ok(!(await msgr.sendPhoto({ path: join(tmp, 'missing.png') })).ok, 'a missing picture is an error, not an empty item');

  const doc = join(tmp, 'report.csv'); writeFileSync(doc, 'a,b\n1,2\n');
  res = await msgr.sendFile({ path: doc }, { shareUrl: () => 'https://example.test/dl/x' });
  got = box.list().items[0];
  ok(res.ok && got.kind === 'file' && got.title === 'report.csv' && got.media[0]?.size === 8 && res.url === 'https://example.test/dl/x', 'send_file_to_user lands as a file, with the share link returned');
  ok(!(await msgr.sendFile({ path: join(tmp, 'nope.bin') })).ok, 'a missing file is an error');

  // ── …and the channels that are on get copies ─────────────────────────────
  const copies = [];
  channels.register({ id: 'telegram', label: 'Telegram', enabledByDefault: true, send: async (text, meta) => { copies.push({ id: 'telegram', text, meta }); return true; } });
  channels.register({ id: 'mobile', label: 'Phone app', enabledByDefault: true, send: async (text, meta) => { copies.push({ id: 'mobile', text, meta }); return true; } });
  writeFileSync(img, PNG);
  res = await msgr.sendPhoto({ path: img, caption: 'Look' }, { chat: { id: 'c2', title: 'UI', project: 'demo' } });
  const tg = copies.find((x) => x.id === 'telegram');
  ok(copies.length === 2 && copies.every((x) => x.meta.inboxId === res.inboxId), 'every enabled channel gets a copy naming the inbox item');
  ok(tg && tg.meta.attach?.kind === 'photo' && existsSync(tg.meta.attach.path) && tg.meta.inboxLink === true, 'Telegram is handed the kept picture to attach, and a link to the inbox');
  ok(tg.meta.url === '/chat/c2' && tg.meta.chat?.id === 'c2', 'a copy about a chat opens that chat');
  ok(tg.meta.inboxThread === 'chat:c2', 'and names its inbox thread', tg.meta.inboxThread);
  ok(res.delivered.join() === 'inbox,telegram,mobile' && /Telegram, Phone app/.test(res.note), 'the tool says where it went', res.note);
  copies.length = 0;
  await msgr.sendMessage({ message: 'no chat' });
  ok(copies[0].meta.url.startsWith('/#inbox/'), 'a copy about no chat opens the inbox item', copies[0].meta.url);
  copies.length = 0;
  await msgr.tell({ kind: 'question', text: 'q', meta: { quiet: true }, deliverOpts: { only: ['mobile'] } });
  ok(copies.length === 1 && copies[0].id === 'mobile' && copies[0].meta.quiet, 'an event while the owner is here: inbox, plus a quiet phone copy only');
  ok(box.list().items[0].kind === 'question', 'and it is still in the inbox');

  // ── What lands, and what does not ────────────────────────────────────────
  ok(INBOX_EVENTS.needsInput === 'question' && INBOX_EVENTS.scheduledChat === 'schedule' && INBOX_EVENTS.secretRequest === 'permission', 'questions, permissions and scheduled results land');
  ok(!('finished' in INBOX_EVENTS) && !('serviceUp' in INBOX_EVENTS) && !('kanbanTask' in INBOX_EVENTS), 'live state (a turn finished, a service up, a card moved) stays out');

  // ── The API: the owner's only ─────────────────────────────────────────────
  const owner = { role: ROLE_OWNER, projects: [], roots: {} };
  const collab = { role: ROLE_COLLABORATOR, collabKey: 'pc:sam', projects: ['demo'], roots: { demo: '/tmp/wt' } };
  for (const [m, p] of [['GET', '/api/inbox'], ['POST', '/api/inbox/read'], ['POST', '/api/inbox/delete'], ['GET', '/api/inbox/media/abc123/x.png']]) {
    ok(mayAccess(owner, m, p) && !mayAccess(collab, m, p), `${m} ${p}: owner yes, collaborator no`);
  }
  const web = readFileSync(join(root, 'src', 'webapp.js'), 'utf8');
  ok(/path === '\/api\/inbox' && req\.method === 'GET'/.test(web) && /'\/api\/inbox\/read'/.test(web) && /'\/api\/inbox\/delete'/.test(web) && /\\\/api\\\/inbox\\\/media\\\//.test(web), 'the routes exist');
  ok(/if \(isConfined\(principal\)\) return json\(res, \{ ok: false, error: 'Not available to collaborator access\.'/.test(web), 'and refuse a collaborator themselves too');
  ok(/\\\/api\\\/inbox\\\/\(\[a-z0-9\]\{6,40\}\)\$\/\.exec\(path\)/.test(web) && mayAccess(owner, 'GET', '/api/inbox/abc123') && !mayAccess(collab, 'GET', '/api/inbox/abc123'), 'GET /api/inbox/<id> exists, owner only');
  ok(/^[A-Za-z0-9_.:-]+$/.test(a.id), 'item ids are safe as a push inboxId', a.id);
  ok(/path === '\/api\/inbox\/threads' && req\.method === 'GET'/.test(web) && web.indexOf("'/api/inbox/threads'") < web.indexOf('One item, so a notification'), 'GET /api/inbox/threads, matched before an item id');
  ok(/thread: url\.searchParams\.get\('thread'\)/.test(web) && /inbox\.markRead\(\{ ids, thread,/.test(web) && /inbox\.remove\(\{ ids, thread,/.test(web), 'thread= on list, read and delete');
  ok(mayAccess(owner, 'GET', '/api/inbox/threads') && !mayAccess(collab, 'GET', '/api/inbox/threads'), 'threads: owner yes, collaborator no');
  ok(JSON.parse(buildPayload('x', { inboxId: 'abc', inboxThread: 'chat:c1' })).inboxThread === 'chat:c1' && !('inboxThread' in JSON.parse(buildPayload('x', { inboxThread: 'bad key!' }))), 'a phone push names its thread');
  ok(/broadcastOwnerSSE\('inbox'/.test(web), 'changes go live to the owner\'s pages only');
  ok(!/send_message_to_user' && bot/.test(web) && !/send_photo_to_user' && bot/.test(web), 'the tools no longer need the Telegram bot');
  ok(/ownerMsg\.sendMessage\(/.test(web) && /ownerMsg\.sendPhoto\(/.test(web) && /ownerMsg\.sendFile\(/.test(web), 'the tools go through the inbox');
  ok(/id: 'inbox',\s*\n\s*label: 'Crundi inbox',\s*\n\s*alwaysOn: true/.test(web), 'the inbox is registered as an always-on channel');
  ok(CAPABILITIES.includes('inbox'), 'the server lists the inbox capability');
  ok(JSON.parse(buildPayload('x', { inboxId: 'abc' })).inboxId === 'abc', 'a phone push names its inbox item');

  const html = readFileSync(join(root, 'src', 'webapp-html.js'), 'utf8');
  ok(/id="inbox-btn"/.test(html) && html.indexOf('id="inbox-btn"') > html.indexOf('data-action="add-project" title="Add project"'), 'the envelope sits below "+ Add project"');
  ok(/es\.addEventListener\('inbox'/.test(html), 'the page listens for inbox events');
  ok(!/Telegram Bot Token <span style="color:var\(--red\);">\*<\/span>/.test(html) && !/Bot Token and Allowed Username are required/.test(html), 'Telegram is not marked required');
} finally {
  rmSync(tmp, { recursive: true, force: true });
}

console.log(failures ? `\n${failures} failure(s)` : '\nAll inbox checks passed.');
process.exit(failures ? 1 : 0);
