// T10 (FINDINGS-ELECTRON.md, F11): a chat's embed pages, end to end. Two
// show_embed items (a silent page and one playing a video) in a chat pane,
// pages from a loopback server of this suite's own:
//   - far off (more than two viewport heights, the list scrolled with the
//     wheel), the silent one is unloaded at once; back near, it loads again
//     in a box of the same height;
//   - the playing one lives through the grace (shortened here) and is
//     unloaded after it; scrolled back before the grace ends, the timer is
//     cancelled (the same page, playing on);
//   - closing the chat pane closes every page of it at once.
//
// On a fresh stub server (its chats never run the real Claude):
//   native-v2/tools/stub-server.sh 4530
//   CRUNDI_TEST_SERVER=http://localhost:4530 CRUNDI_TEST_PORT=4531 EMBED_PAGE_PORT=4532 \
//     dbus-run-session -- xvfb-run -a -s "-screen 0 1440x900x24" node native-v2/test/embeds.mjs
//   kill $(cat /tmp/crundi-devtest-4530/server.pid)

import http from 'node:http';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import * as t from './lib.mjs';

const GRACE = 10;
process.env.CRUNDI_TEST_EMBED_GRACE_SECS = String(GRACE);
const PAGE_PORT = Number(process.env.EMBED_PAGE_PORT || 4532);
const PAGES = {
  '/silent': '<!doctype html><body style="margin:0;background:#234;color:#fff;font:20px sans-serif">A silent page</body>',
  // A video that plays on its own (muted autoplay is allowed): a canvas's stream.
  '/play': '<!doctype html><body style="margin:0;background:#000"><canvas id=c width=64 height=64></canvas><video id=v autoplay muted playsinline width=200 height=120></video><script>const c=document.getElementById("c"),x=c.getContext("2d");let n=0;setInterval(()=>{x.fillStyle="hsl("+(n++%360)+",80%,50%)";x.fillRect(0,0,64,64)},40);const v=document.getElementById("v");v.srcObject=c.captureStream(25);v.play().catch(()=>{});</script></body>',
};
const srv = http.createServer((req, res) => { const b = PAGES[req.url]; res.writeHead(b ? 200 : 404, { 'content-type': 'text/html' }); res.end(b || 'no'); });
await new Promise((r) => srv.listen(PAGE_PORT, '127.0.0.1', r));
const SILENT = `http://127.0.0.1:${PAGE_PORT}/silent`;
const PLAY = `http://127.0.0.1:${PAGE_PORT}/play`;

const T = await t.begin('embeds');
const D = (s) => s.areas?.desktop || {};
const ev = (e) => T.ui('chat-event', JSON.stringify(e));
const fr = (s, url) => (D(s).chatFrames || []).find((f) => f.key.endsWith(`|${url}`));
const row = (s, id) => s.log?.rows?.find((r) => r.id === id);

await T.ui('new-chat');
let s = await T.until('a chat pane', (s) => s.chat && s.panes?.includes(`live:${s.chat}`), 30000);
const chat = s.chat;
const leaf = s.leaves.find((l) => l.key === `live:${chat}`);
const over = { x: leaf.x + leaf.w / 2, y: leaf.y + leaf.h / 3 };
const wheel = async (dy, n) => { for (let i = 0; i < n; i++) { await T.mouse('wheel', over.x, over.y, { dy }); await t.sleep(12); } };

const embed = (id, src) => ev({ type: 'entry', entry: { id, kind: 'tool', name: 'mcp__crundi__show_embed', toolUseId: `tu-${id}`, input: {}, status: 'done', result: JSON.stringify({ items: [{ kind: 'embed', src, w: 640, h: 360 }] }) } });
await ev({ type: 'entry', entry: { id: 'a0', kind: 'assistant-text', text: 'Here are two pages.' } });
await embed('em1', SILENT);
await embed('em2', PLAY);
s = await T.until('both embeds loaded, near, the video playing', (s) => fr(s, SILENT)?.page != null && fr(s, PLAY)?.page != null && fr(s, PLAY)?.frames > 0, 40000);
const h1 = row(s, 'em1')?.h;
const playPage = fr(s, PLAY)?.page;
t.check('the silent embed\'s row has its height', h1 > 100, row(s, 'em1'));

// Far off: rows after them, the log following its tail to the foot.
const para = Array.from({ length: 12 }, (_, i) => `Line ${i + 1} of a long answer that pushes the pages far up the list.`).join('\n\n');
for (let i = 0; i < 24; i++) await ev({ type: 'entry', entry: { id: `f${i}`, kind: 'assistant-text', text: `Part ${i + 1}\n\n${para}` } });
await T.ui('log-jump');
await wheel(-200, 40);
const t0 = Date.now();
s = await T.until('…far off, the silent page is unloaded (it plays nothing)', (s) => fr(s, SILENT) && fr(s, SILENT).page == null && !fr(s, SILENT).near, 15000);
s = await T.until('…the playing one is still alive, asked and playing', (s) => fr(s, PLAY)?.playing === true, 8000);
t.check('…the same page as before', fr(s, PLAY)?.page === playPage, fr(s, PLAY));
t.check('…well inside the grace', (Date.now() - t0) / 1000 < GRACE, (Date.now() - t0) / 1000);

// Back near before the grace ends: the timer is cancelled.
await wheel(400, 80);
s = await T.until('scrolled back: both near', (s) => fr(s, SILENT)?.near && fr(s, PLAY)?.near, 10000);
s = await T.until('…the silent page loads again', (s) => fr(s, SILENT)?.page != null && fr(s, SILENT)?.frames > 0, 20000);
t.check('…its row the same height (no jump)', Math.abs((row(s, 'em1')?.h || 0) - h1) < 1, { before: h1, after: row(s, 'em1')?.h });
await t.sleep((GRACE + 3) * 1000);
s = await T.state();
t.check('…the playing page outlived the grace: the timer was cancelled (same page)', fr(s, PLAY)?.page === playPage, fr(s, PLAY));

// Far off again, past the grace: unloaded, the video (and its sound) gone.
const t1 = Date.now();
await T.ui('log-jump');
await wheel(-200, 40);
s = await T.until('far off again: still playing for now', (s) => !fr(s, PLAY)?.near && fr(s, PLAY)?.page === playPage, 8000);
s = await T.until('…after the grace, the playing page is unloaded', (s) => fr(s, PLAY) && fr(s, PLAY).page == null, (GRACE + 15) * 1000);
t.check('…not before the grace', (Date.now() - t1) / 1000 >= GRACE - 0.5, (Date.now() - t1) / 1000);

// Back near: both load; then the chat closed: every page goes at once.
await wheel(400, 80);
s = await T.until('back near: both pages loaded again', (s) => fr(s, SILENT)?.page != null && fr(s, PLAY)?.page != null && fr(s, PLAY).page !== playPage, 30000);

// ─── T15 (F18): an attachment's path in a message, clicked: the viewer ───
// (Attachments go to the project's crundi_attachments folder: src/webapp.js.)
{
  const dir = join(T.projDir, 'crundi_attachments');
  mkdirSync(dir, { recursive: true });
  // A 1x1 PNG, and a text file.
  writeFileSync(join(dir, 'shot.png'), Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==', 'base64'));
  writeFileSync(join(dir, 'notes.txt'), 'hello from the attachment\n');
  const W = (s) => s.workspace || {};
  const clickPath = async (id, what) => {
    await T.ui('log-jump');
    let q = await T.until(`the message with ${what} laid out`, (s) => row(s, id)?.h > 0, 8000);
    const r = row(q, id);
    // The pointer along the bubble's middle line, right to left, until the log says a link is under it.
    let hit = null;
    for (let x = leaf.x + leaf.w - 30; x > leaf.x + 40 && !hit; x -= 12) {
      await T.mouse('move', x, r.y + r.h / 2);
      await t.sleep(30);
      if ((await T.state()).log?.hot) hit = { x: x - 80, y: r.y + r.h / 2 };
    }
    t.check(`…the pointer finds ${what} as a link`, !!hit, r);
    if (hit) { await T.mouse('move', hit.x, hit.y); await t.sleep(60); t.check(`…still on ${what} inside it`, !!(await T.state()).log?.hot); await T.mouse('click', hit.x, hit.y); }
    return !!hit;
  };
  const before = W(await T.state()).wins?.length || 0;
  await ev({ type: 'entry', entry: { id: 'u-png', kind: 'user', text: `${join(dir, 'shot.png')}` } });
  if (await clickPath('u-png', 'the picture\'s path')) {
    await T.until('…clicked: the picture opens in the viewer', (s) => (W(s).wins || []).some((w) => w.kind === 'view' && /shot\.png$/.test(w.title || w.file || '')), 10000);
  }
  await ev({ type: 'entry', entry: { id: 'u-txt', kind: 'user', text: `${join(dir, 'notes.txt')}` } });
  if (await clickPath('u-txt', 'the text file\'s path')) {
    await T.until('…clicked: the file opens in the editor', (s) => (W(s).wins || []).some((w) => w.kind === 'edit' && /notes\.txt$/.test(w.title || w.file || '')), 10000);
  }
  t.check('…two windows more', (W(await T.state()).wins?.length || 0) >= before + 2);
  if (!process.env.KEEP) for (const w of W(await T.state()).wins || []) await T.ui('workspace:win-close', String(w.id)).catch(() => {});
}

await T.ui('pane-close', `live:${chat}`);
await T.until('…its × armed ("Sure?")', (s) => s.leaves?.find((l) => l.key === `live:${chat}`)?.armed, 3000);
await T.ui('pane-close', `live:${chat}`);
s = await T.until('the chat closed: no page of it is left in the browser host', (s) => !s.panes?.includes(`live:${chat}`) && !(D(s).chatFrames || []).some((f) => f.key.startsWith(`chat:${chat}|`)), 5000);
t.check('…and no chat frame is live', (s.log?.frames ?? 0) === 0 || !s.log, s.log?.frames);

srv.close();
await T.end();
