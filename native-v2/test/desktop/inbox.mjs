// ─── The Inbox and notifications (native-gpui/test/desktop.mjs's section) ───
// A message to the owner (send_message_to_user, as a chat calls it): the
// sidebar's count, this PC's notification, the Inbox page with its thread,
// read on opening, a new message live, a message's ×, Delete thread (asked
// first). v2: a live inbox event re-renders the inbox's views alone; idle on
// the Inbox draws no frames; a notification's click goes to its chat.

import { readFileSync } from 'node:fs';

/** The server's tools, as a chat calls them (/api/mcp/call with the internal key). */
function mcpOf(T) {
  const KEY = readFileSync(`/tmp/crundi-devtest-${new URL(T.SERVER).port}/data/.api-key`, 'utf8').trim();
  const unwrap = (r) => {
    if (!r || !Array.isArray(r.content)) return r;
    const txt = r.content.find((c) => c.type === 'text');
    let o = {};
    try { o = JSON.parse(txt?.text || '{}'); } catch { o = { text: txt?.text }; }
    return { ok: r.ok, ...o };
  };
  return (tool, args = {}) => fetch(T.SERVER + '/api/mcp/call', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Api-Key': KEY }, body: JSON.stringify({ tool, args: { alias: 'demo', ...args } }) })
    .then((r) => r.json()).then(unwrap).catch((e) => ({ ok: false, error: String(e) }));
}

const INBOX_VIEWS = ['inbox', 'inbox-head', 'inbox-threads', 'inbox-conv'];
/** The views drawn between two states' render counts. */
const drawn = (r0, r1) => Object.fromEntries(Object.keys(r1 || {}).map((k) => [k, r1[k] - ((r0 || {})[k] || 0)]).filter(([, n]) => n > 0));

export async function run(T, t) {
  const D = T.D;
  const mcp = mcpOf(T);
  if (D(await T.state()).drawer?.open) {
    await T.act('browser');
    await T.until('the Browser button closes the drawer again', (p) => !D(p).drawer.open);
  }
  await T.until('the inbox loads', (p) => D(p).inbox.state === 'ok');
  let r = await mcp('send_message_to_user', { message: 'The build finished: 12 tests passed.' });
  t.check('a message to the owner', r.ok !== false, r);
  await T.until('the sidebar counts it', (p) => D(p).inbox.unread >= 1);
  await T.until('…and this PC shows a notification', (p) => (D(p).notifications || []).some((n) => /build finished/.test(n.body)), 10000);
  await T.act('inbox');
  let p = await T.until('the sidebar\'s Inbox opens the Inbox page with its thread', (p) => p.tab === 'inbox' && D(p).inbox.threads.length >= 1);
  await T.snap('desktop', 'inbox-list');
  const th = D(p).inbox.threads[0].key;
  await T.act('inbox-open', th);
  await T.until('opening a thread shows its messages and reads them', (p) => D(p).inbox.open === th && D(p).inbox.items.length >= 1 && D(p).inbox.unread === 0);
  await T.snap('desktop', 'inbox-thread');

  // v2: idle on the Inbox: no frames, no pumps (once the window has settled: F70).
  await t.sleep(1500);
  await T.settle();
  let s0 = await T.state();
  let m = T.frames.mark();
  await t.sleep(2500);
  let s1 = await T.state();
  t.check('idle on the Inbox: no frames', T.frames.since(m).frames === 0, T.frames.since(m));
  t.check('…and the area\'s pump sleeps', D(s1).pumps === D(s0).pumps, { before: D(s0).pumps, after: D(s1).pumps });

  // v2: a live event re-renders the inbox's views and nothing else.
  s0 = await T.state();
  r = await mcp('send_message_to_user', { message: 'A second one.' });
  p = await T.until('a new message lands in the open thread live', (p) => D(p).inbox.items.length >= 2);
  await t.sleep(600);
  s1 = await T.state();
  const d = drawn(s0.renders, s1.renders);
  // (The sidebar redraws too for its Inbox count, the web's ibxBadge: up with
  // the new message, down again as the open thread reads it; at most twice.)
  t.check('…redrawing the inbox\'s views alone (and the sidebar\'s count)', Object.keys(d).length > 0 && Object.keys(d).every((k) => INBOX_VIEWS.includes(k) || (k === 'sidebar' && d[k] <= 2)), d);
  t.check('…the head and the thread list stay cached for the conversation\'s change', !d['inbox-head'] || d['inbox-head'] <= 2, d);

  // A notification's click: the window to the front, its chat's pane (the
  // gdbus monitor's path; no notification daemon on the run's bus).
  await T.act('notify-click', '');
  t.check('a click on a notification with no chat keeps the page', (await T.state()).tab === 'inbox');

  const id = D(await T.state()).inbox.items[0].id;
  await T.act('inbox-delete', id);
  await T.until('a message\'s × deletes it', (p) => !D(p).inbox.items.some((x) => x.id === id));
  await T.act('inbox-delete-thread');
  await T.until('Delete thread asks first', (p) => /Delete this whole conversation/.test(p.dialog?.title || ''));
  await T.ui('dialog-answer', 'true');
  await T.until('…and deletes it', (p) => !D(p).inbox.threads.some((x) => x.key === th));
  await T.snap('desktop', 'inbox-empty');
  await T.ui('tab', 'workbench');
}
