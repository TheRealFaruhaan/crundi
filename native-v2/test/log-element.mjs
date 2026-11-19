#!/usr/bin/env node
// The chat log's block element (F31; ARCHITECTURE.md, "What a scroll frame
// costs"): Claude's paragraphs, headings, quotes, items and rules are each
// one element, laid out once per width. This suite checks that everything a
// person does with that text still works. No server: it runs the frame
// floor's seeded chats (crates/app/src/floor.rs), under Xvfb only.
//
//   native-v2/tools/build.sh                (builds crundi-v2-floor too)
//   FLOOR_BIN=$HOME/.cache/crundi-native-target/debug/crundi-v2-floor CRUNDI_TEST_PORT=4885 \
//     xvfb-run -a -s "-screen 0 1920x1080x24" node native-v2/test/log-element.mjs
//
// Checks: the texts are reported where they are drawn; a drag selects
// across blocks; Ctrl+C copies; a double-click selects a word, a
// triple-click a block; a click clears; a shift-click selects from the
// click, and a second extends it (F25); a click on a link opens it; the
// selection survives a scroll; while a chat streams, its rows grow and the
// new text is drawn; idle draws no frames.
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync, existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { homedir, tmpdir } from 'node:os';
import { driver, sleep, check } from './lib.mjs';
import { FrameMeter } from '../tools/frames.mjs';

const BIN = process.env.FLOOR_BIN || join(homedir(), '.cache/crundi-native-target/debug/crundi-v2-floor');
const PORT = Number(process.env.CRUNDI_TEST_PORT || 4885);
let failed = false;
const ok = (name, cond, extra) => { if (!check(name, cond, extra)) failed = true; return cond; };

async function start(mode) {
  const config = mkdtempSync(join(tmpdir(), `crundi-v2-logel-${mode}-`));
  const env = { ...process.env, CRUNDI_TEST_PORT: String(PORT), CRUNDI_CONFIG_DIR: config, CRUNDI_OPEN_URL_FILE: join(config, '..', `open-url-${process.pid}.txt`), ZED_MEASUREMENTS: '1' };
  // Only on the virtual display: never a Wayland session the shell carries.
  delete env.WAYLAND_DISPLAY;
  if (!env.DISPLAY || env.DISPLAY === ':0') { console.error('Run under xvfb-run (DISPLAY is not a virtual display).'); process.exit(2); }
  const app = spawn(BIN, [mode], { env, stdio: ['ignore', 'ignore', 'pipe'] });
  let dead = false;
  app.on('exit', () => { dead = true; });
  const frames = new FrameMeter(app.stderr);
  const d = await driver(PORT, () => !dead);
  await d.send({ cmd: 'resize', w: 1920, h: 1040 });
  await sleep(2500);
  const ui = (n, ...args) => d.send({ cmd: 'ui', name: n, args });
  const mouse = (kind, x, y, extra = {}) => d.send({ cmd: 'mouse', kind, x, y, ...extra });
  const state = async () => (await d.send({ cmd: 'state' })).state || {};
  const stop = async () => {
    d.close();
    if (!dead) { process.kill(app.pid); await new Promise((r) => { if (dead) r(); else app.once('exit', r); }); }
    rmSync(config, { recursive: true, force: true });
    rmSync(env.CRUNDI_OPEN_URL_FILE, { force: true });
  };
  return { d, ui, mouse, state, stop, frames, urlFile: env.CRUNDI_OPEN_URL_FILE };
}

// ─── One chat (a long mixed-markdown reply) ───
{
  const A = await start('chatlog');
  await A.ui('log-scroll', '-100000');
  await sleep(400);
  const lt = await A.ui('log-texts');
  const texts = (lt.texts || []).filter((x) => x.w > 4 && x.h > 4 && x.y > 60 && x.y + x.h < 900).sort((a, b) => a.row - b.row || a.part - b.part);
  ok('the log reports the texts it drew (block elements and the rest)', texts.length >= 8, lt);
  const para = texts.find((x) => x.text.startsWith('A hash map stores values'));
  const item = texts.find((x) => x.text.startsWith('the hash function spreads'));
  const head = texts.find((x) => x.text.startsWith('Section 1 of the reply'));
  ok('a paragraph, a heading and a list item are each reported once, at their place', para && item && head && head.y < para.y && para.y < item.y && item.x > para.x, { para, item, head });
  if (para && item && head) {
    const a = head, z = texts.filter((x) => x.y + x.h < 860).at(-1), mid = para;
    await A.mouse('drag', a.x + 1, a.y + a.h / 2, { toX: z.x + z.w - 1, toY: z.y + z.h / 2, steps: 12 });
    await sleep(250);
    const sel = await A.ui('log-texts');
    ok('a drag selects across blocks (a middle block whole, line breaks between)', sel.selection.startsWith(a.text.slice(0, 10)) && sel.selection.includes(mid.text) && sel.selection.includes('\n'), { sel: sel.selection.slice(0, 200) });
    await A.d.send({ cmd: 'key', text: 'c', mods: ['ctrl'] });
    const cp = await A.ui('log-texts');
    ok('Ctrl+C copies the selection', cp.clipboard === sel.selection && cp.clipboard.length > 0, { clip: (cp.clipboard || '').slice(0, 120) });
    // The selection is kept as places in the text: it survives a scroll.
    await A.ui('log-scroll', '200'); await sleep(200); await A.ui('log-scroll', '-200'); await sleep(250);
    ok('the selection survives a scroll', (await A.ui('log-texts')).selection === sel.selection);
    await A.mouse('dblclick', para.x + 4, para.y + 10);
    await sleep(200);
    const w = await A.ui('log-texts');
    ok('a double-click selects a word', w.selection === 'A', { sel: w.selection });
    await A.mouse('dblclick', para.x + 30, para.y + 10);
    await sleep(200);
    const w2 = await A.ui('log-texts');
    ok('a double-click inside a word selects it whole', w2.selection === 'hash', { sel: w2.selection });
    await A.mouse('tripleclick', item.x + 4, item.y + item.h / 2);
    await sleep(200);
    ok('a triple-click selects the block', (await A.ui('log-texts')).selection === item.text);
    await A.mouse('click', item.x + 4, item.y + item.h / 2);
    await sleep(200);
    ok('a click clears the selection', (await A.ui('log-texts')).selection === '');
    // F25: a click places the anchor; a shift-click selects up to its point.
    await A.mouse('click', head.x + 1, head.y + head.h / 2);
    await sleep(150);
    await A.mouse('click', para.x + para.w - 1, para.y + para.h - 4, { mods: ['shift'] });
    await sleep(200);
    const sh = await A.ui('log-texts');
    ok('a shift-click selects from the click to its point', sh.selection.startsWith(head.text.slice(0, 8)) && sh.selection.includes(para.text.slice(0, 60)), { sel: sh.selection.slice(0, 200) });
    await A.mouse('click', item.x + item.w - 1, item.y + item.h / 2, { mods: ['shift'] });
    await sleep(200);
    const sh2 = await A.ui('log-texts');
    ok('a second shift-click extends the same selection', sh2.selection.startsWith(sh.selection.slice(0, 20)) && sh2.selection.length > sh.selection.length && sh2.selection.endsWith(item.text), { first: sh.selection.length, second: sh2.selection.length, end: sh2.selection.slice(-60) });
    await A.mouse('click', item.x + 4, item.y + item.h / 2);
    await sleep(150);
    // A click on a link opens it: "the docs" at the paragraph's end. Its
    // place in the wrapped text is not reported, so the paragraph's last
    // line is clicked along until a link opens.
    let opened = '';
    for (let x = para.x + 2; x < para.x + para.w && !opened; x += 6) {
      await A.mouse('click', x, para.y + para.h - 8);
      if (existsSync(A.urlFile)) opened = readFileSync(A.urlFile, 'utf8').trim();
    }
    ok('a click on a link opens it', opened.includes('doc.rust-lang.org/std/collections'), { opened });
    ok('a click on a link selects nothing', (await A.ui('log-texts')).selection === '');
  }
  await sleep(1000); // the last click's frame (software Vulkan: ~70 ms)
  const m = A.frames.mark(); await sleep(3000);
  ok('idle: no frames', A.frames.since(m).frames === 0, A.frames.since(m));
  await A.stop();
}

// ─── Five chats, the first streaming ───
{
  const B = await start('stream5');
  // Focus the first pane (the streaming chat): a press on its log.
  await B.mouse('click', 200, 500);
  await sleep(300);
  const s0 = await B.state();
  ok('the streaming chat is focused', s0.chat === 'floor-1', { chat: s0.chat });
  const t0 = await B.ui('log-texts');
  await sleep(2500);
  const s1 = await B.state();
  const t1 = await B.ui('log-texts');
  const streamed = (t1.texts || []).filter((x) => x.text.includes('of the reply 9'));
  ok('while it streams, its rows grow', (s1.rows || 0) > (s0.rows || 0), { before: s0.rows, after: s1.rows });
  ok('the streamed text is drawn (following the end)', streamed.length > 0 && s1.logFollowing === true, { texts: (t1.texts || []).slice(-3), following: s1.logFollowing, before: (t0.texts || []).length });
  const m = B.frames.mark(); await sleep(1500);
  ok('frames are drawn while it streams', B.frames.since(m).frames > 10, B.frames.since(m));
  await B.stop();
}

process.exit(failed ? 1 : 0);
