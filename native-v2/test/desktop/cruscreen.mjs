// ─── Cru's live screen: blocks, a page in a frame (talking both ways), the full view ───
// Ported from native-gpui/test/desktop.mjs ("Cru's live screen…"), with v2's
// check: a screen event re-renders Cru's view and no other.

const delta = (r0, r1) => Object.fromEntries(Object.keys(r1 || {}).map((k) => [k, (r1[k] || 0) - ((r0 || {})[k] || 0)]).filter(([, v]) => v));

export async function run(T, t) {
  const D = T.D;
  const CS = (p) => D(p).cruScreen;
  const snap = (f) => T.snap('desktop', f);
  // On the Inbox (the workbench's empty launcher pane animates by design).
  await T.ui('tab', 'inbox');
  await t.sleep(2000);
  await T.act('cru-keyboard', 'true');
  await T.act('cru');
  await T.until('Cru opens, with its events socket', (p) => D(p).cru.open && CS(p).socket);
  // Keyboard mode, Cru asleep: nothing animates (the mark still).
  await T.until('…asleep (no Claude key here)', (p) => D(p).cru.phase === 'ended' && D(p).cru.unavailable);
  await t.sleep(1200);
  const blocks = [
    { type: 'heading', text: 'Your day' }, { type: 'text', text: 'Three things, one done.' },
    { type: 'checklist', items: [{ text: 'Walk', done: true }, { text: 'Write the report' }, { text: 'Call Mum' }] },
    { type: 'table', headers: ['When', 'What'], rows: [['9:00', 'Stand-up'], ['14:00', 'Dentist']] },
    { type: 'progress', label: 'Report', value: 40 },
    { type: 'chips', items: [{ text: 'on time', tone: 'ok' }, { text: 'busy', tone: 'warn' }, { text: 'late', tone: 'bad' }] },
  ];
  const s0 = await T.state();
  await T.act('cru-event', JSON.stringify({ type: 'screen', screen: { title: 'Today', blocks } }));
  await T.until('blocks on the live screen', (p) => CS(p).screen?.title === 'Today' && !CS(p).frame);
  await t.sleep(500);
  {
    const r = delta(s0.renders, (await T.state()).renders);
    // (The tab's root, a layout view, re-renders with any view under the Shell; its leaves do not.)
    t.check('v2: the screen event re-rendered Cru\'s view and no other (but the tab\'s layout root)', r.cru >= 1 && Object.keys(r).every((k) => k === 'cru' || k === 'inbox'), r);
    await t.sleep(1000);
    const m = T.frames.mark();
    await t.sleep(1500);
    const b = await T.state();
    // (Keyboard mode: the typing box has the keys, its caret blinks twice a second; nothing else.)
    t.check('…then nothing draws but the typing box\'s caret', T.frames.since(m).frames <= 4, { f: T.frames.since(m), phase: D(b).cru.phase, r: delta(s0.renders, b.renders) });
  }
  await snap('cru-screen-blocks');
  const doc = `<!doctype html><html><body style="margin:0;font:28px sans-serif;background:#fde68a;display:flex;align-items:center;justify-content:center;height:100vh"><div>A game for you</div>
<script>addEventListener('message',function(e){if(e.data&&e.data.cru==='volume'){document.title='volume '+e.data.value;parent.postMessage({cru:'event',kind:'start',detail:'loaded'},'*');}});</script></body></html>`;
  await T.act('cru-event', JSON.stringify({ type: 'screen', screen: { title: 'A game', doc } }));
  await T.until('a page in its own frame, painted', (p) => CS(p).frame && CS(p).painted, 30000);
  await T.until('…Cru sends it the volume on load', (p) => CS(p).title === 'volume 1', 15000);
  await T.until('…and hears it back (the message bridge)', (p) => CS(p).lastMessage?.cru === 'event' && CS(p).lastMessage?.detail === 'loaded', 15000);
  await t.sleep(500);
  {
    // v2: the still page's frames redraw Cru's view (its frame) only.
    const a = await T.state();
    await t.sleep(1500);
    const b = await T.state();
    const r = delta(a.renders, b.renders);
    // (The page's game said a line: Cru is live, so the top bar's mark breathes.)
    t.check('v2: with the page up, only Cru\'s views render (and the mark while Cru is live)', Object.keys(r).every((k) => ['cru', 'cru-frame', 'inbox'].includes(k) || (k === 'top-bar' && D(b).cru.phase !== 'ended')), { r, phase: D(b).cru.phase });
  }
  await snap('cru-screen-frame');
  await T.act('cru-screen-full');
  await T.until('Full view: Cru\'s whole view', (p) => CS(p).full);
  await t.sleep(500);
  await snap('cru-full');
  await T.act('cru-screen-normal');
  await T.until('Normal view: the panel again', (p) => !CS(p).full && CS(p).screen);
  await T.act('cru-screen-close');
  await T.until('× takes it down', (p) => !CS(p).screen && !CS(p).frame);
  await T.act('cru');
  await T.until('Cru closes', (p) => !D(p).cru.open);
  await T.act('cru-keyboard', 'false');
  await T.ui('tab', 'workbench');
}
