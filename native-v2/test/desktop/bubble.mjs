// ─── The chat bubble: a chat in a floating window of its own (6.3) ───
// (native-gpui/test/desktop.mjs's section, on v2's chat views.) v2: the
// bubble's log redraws with the chat; its bar only when the title or the
// state moved; collapsed, a new message redraws the round head alone; idle,
// no frames.

const drawn = (r0, r1) => Object.fromEntries(Object.keys(r1 || {}).map((k) => [k, r1[k] - ((r0 || {})[k] || 0)]).filter(([, n]) => n > 0));

export async function run(T, t) {
  const D = T.D;
  const B = (p) => D(p).bubble;
  const shotDir = process.env.SCREENS_DIR || '/tmp';
  const id = (await T.api.http('POST', '/api/ui-sessions/create', { project: 'demo', title: 'Bubble chat' })).body?.id;
  t.check('a chat to float', !!id);
  await T.until('…with its pane', (p) => (p.panes || []).includes(`live:${id}`), 30000);
  await T.act('bubble-open', id);
  let p = await T.until('the bubble opens on it, expanded', (p) => B(p).open && B(p).expanded && B(p).chat === id && B(p).title === 'Bubble chat');
  const n0 = B(p).entries;
  await T.act('bubble-send', 'Hello from the bubble');
  p = await T.until('a message sent from the bubble goes into the chat', (p) => B(p).entries > n0, 30000);
  await t.sleep(1500);
  await T.act('bubble-snap', `${shotDir}/bubble.png`);

  // v2: idle with the bubble open: no frames (once the chat is quiet).
  let last = -1;
  await T.until('…the chat quiet again', (p) => { const n = B(p).entries; const ok = B(p).state === 'idle' && n === last; last = n; return ok; }, 30000);
  // (A turn's last updates, its cost and usage, come after its state: quiet = no view drawn for 2 s.)
  for (let k = 0, prev = null; k < 20; k++) {
    const r = JSON.stringify((await T.state()).renders);
    if (r === prev) break;
    prev = r;
    await t.sleep(2000);
  }
  await T.settle(); // (F70)
  let i0 = await T.state();
  let m = T.frames.mark();
  await t.sleep(2000);
  let i1 = await T.state();
  t.check('idle with the bubble open: no frames', T.frames.since(m).frames === 0, { frames: T.frames.since(m), drawn: drawn(i0.renders, i1.renders), pumps: [D(i0).pumps, D(i1).pumps], b: B(i1) });

  await T.act('bubble-toggle');
  p = await T.until('the head collapses it', (p) => B(p).open && !B(p).expanded && !B(p).unread);
  const n1 = B(p).entries;
  await t.sleep(500);
  const r0 = (await T.state()).renders;
  await T.ui('send', 'A message from the main window');
  p = await T.until('…a new message while collapsed: the unread dot', (p) => B(p).entries > n1 && B(p).unread, 30000);
  const d = drawn(r0, p.renders);
  t.check('…the bubble\'s bar not redrawn for it', !d['bubble-bar'], d);
  t.check('…its round head redrawn once for the dot', d['bubble-head'] === 1, d);
  await t.sleep(500);
  await T.act('bubble-snap', `${shotDir}/bubble-head.png`);
  await T.act('bubble-toggle');
  await T.until('expanded again, read', (p) => B(p).expanded && !B(p).unread);
  await T.act('bubble-close');
  await T.until('× closes it', (p) => !B(p).open);
}
