// ─── Cru: the PC client's screen (app/vendor/crundi-voice.js), part by part ───
// Ported from native-gpui/test/desktop.mjs ("Cru: the PC client's screen…"),
// with v2's checks: Cru closed, an idle window draws nothing; open, its
// animation is capped; a change of Cru's re-renders Cru's view alone.
import { writeFileSync, rmSync } from 'node:fs';

/** Renders of each counted view since `r0`. */
const delta = (r0, r1) => Object.fromEntries(Object.keys(r1 || {}).map((k) => [k, (r1[k] || 0) - ((r0 || {})[k] || 0)]).filter(([, v]) => v));

export async function run(T, t) {
  const D = T.D;
  const C = (p) => D(p).cru;
  const U = (p) => D(p).cru.ui || {};
  const dr = (p) => D(p).drawer;
  const snap = (f) => T.snap('desktop', f);
  const key = (text, mods, phase) => T.d.send({ cmd: 'key', text, mods, phase });
  const projDir = `/tmp/crundi-devtest-${new URL(T.SERVER).port}/proj`;
  // Projects for the field's cells.
  for (const a of ['shop', 'blog']) await T.api.http('POST', '/api/projects', { alias: a, name: a, path: `${projDir}-${a}`, create: true });
  await t.sleep(1500);

  // v2: Cru closed, nothing of Cru's draws or wakes. (On the Inbox: the workbench's empty
  // launcher pane animates by design.)
  await T.ui('tab', 'inbox');
  await t.sleep(2500);
  {
    const s0 = await T.state(); const m = T.frames.mark();
    await t.sleep(2500);
    const s1 = await T.state(); const f = T.frames.since(m);
    t.check('Cru closed: an idle window draws no frames', f.frames === 0, f);
    // (In a whole run the drawer's tabs and the browser host may still have
    // deadlines of their own, the sleep and idle-stop rules: those may pump.)
    const others = ['web', 'drawer', 'notify'].some((k) => D(s0).due?.[k] != null && D(s0).due[k] < 2600);
    t.check('…and Cru and "Hey Cru" have no deadline', D(s0).due?.cru == null && D(s1).due?.cru == null && D(s0).due?.wake == null && D(s1).due?.wake == null, D(s1).due);
    t.check('…and the area\'s pump does not run (unless another part fell due)', others || D(s1).pumps === D(s0).pumps, { before: D(s0).pumps, after: D(s1).pumps, due: D(s0).due });
    t.check('…and Cru\'s view does not render', !delta(s0.renders, s1.renders).cru, delta(s0.renders, s1.renders));
  }
  await T.ui('tab', 'workbench');

  // A panel that held the keys closes (the drawer, its address field focused): the window's
  // shortcuts still work (the focus goes back to the window's root, not to nothing).
  if (!dr(await T.state()).open) await T.act('browser');
  await T.until('the drawer is open', (p) => dr(p).open && dr(p).tabs.length > 0);
  const fr = await T.act('drawer-focus');
  t.check('…its address field has the keys', fr.ok, fr);
  await T.act('drawer', 'false');
  await T.until('…the drawer closes', (p) => !dr(p).open);
  await t.sleep(500);
  await key('Space', ['ctrl'], 'down');
  await T.until('after it, Ctrl+Space still opens Cru\'s push to talk', (p) => C(p).open && C(p).ptt);
  await key('Space', ['ctrl'], 'up');
  await T.until('…let go', (p) => !C(p).ptt);
  await T.key('Escape');
  await T.until('…and Esc closes it', (p) => !C(p).open);
  await T.act('cru');
  let p = await T.until('Cru\'s screen opens from the top bar\'s mark', (p) => C(p).open && U(p).w > 0 && C(p).unavailable !== undefined);
  p = await T.until('…its settings read: a server with no Claude key says so (Unavailable, in red)', (p) => /no Claude API key/.test(C(p).unavailable) && U(p).word === 'Unavailable' && U(p).tone === 'problem');
  t.check('…400 by 600', U(p).w === 400 && U(p).h === 600, U(p));
  const an = U(p).anchor;
  t.check('…under the mark, at its right', an && Math.abs(U(p).top - (an.bottom + 8)) < 1 && Math.abs((U(p).viewportW - U(p).right) - (an.right + 8)) < 1, U(p));
  t.check('the header: the wordmark, ⋯ and close', U(p).wordmark && U(p).more && U(p).close, U(p));
  p = await T.until('the usage line: Claude and Deepgram spent of their budgets', (p) => C(p).legend.some((x) => /^Claude \$\d+\.\d\d \/ \$\d/.test(x)) && C(p).legend.some((x) => /^Deepgram \$\d+\.\d\d \/ \$\d/.test(x)));
  t.check('the chat view is the default, empty: "No messages yet."', U(p).view === 'chat' && C(p).chatView && U(p).empty && U(p).bubbles === 0, U(p));
  t.check('the hint', U(p).hint === 'Hold Ctrl+Space to talk · click the mic to keep this open', U(p).hint);
  t.check('the liquid mic in the chat view (64)', U(p).mic && U(p).micBox === 64 && !U(p).typingBox, U(p));
  t.check('it moves while open', U(p).animating, U(p));
  {
    // v2: the mic's frames are capped (T::ANIM_FPS 24; the top bar's mark breathes on a timer of its own).
    const m = T.frames.mark(); const s0 = await T.state();
    await t.sleep(2000);
    const f = T.frames.since(m); const s1 = await T.state();
    const r = delta(s0.renders, s1.renders);
    t.check(`…capped: ${f.frames} frames in 2 s, Cru rendered ${r.cru || 0} times`, f.frames > 10 && (r.cru || 0) <= 2 * 24 + 4, { f, r });
  }
  await t.sleep(600);
  await snap('cru-chat-empty');

  // The ⋯ menu.
  await T.act('cru-menu-open', 'true');
  p = await T.until('the ⋯ menu: Voice view, Mute Cru\'s voice, Compact the conversation', (p) => JSON.stringify(U(p).menuItems) === JSON.stringify(['Voice view', 'Mute Cru\'s voice', 'Compact the conversation']));
  await snap('cru-menu');
  await T.act('cru-menu-open', 'false');
  await T.act('cru-menu', 'compact');
  await T.until('Compact asks first', (p) => C(p).confirm);
  await T.act('cru-menu', 'compact-no');
  await T.until('…Cancel', (p) => !C(p).confirm);
  await T.act('cru-menu', 'voice-off');
  await T.until('Mute Cru\'s voice', (p) => C(p).voiceOff);
  await T.act('cru-menu', 'voice-off');
  await T.until('…and back', (p) => !C(p).voiceOff);

  // The conversation as bubbles (Cru's history on the server).
  {
    const { DatabaseSync } = await import('node:sqlite');
    const db = new DatabaseSync(`/tmp/crundi-devtest-${new URL(T.SERVER).port}/data/voice-memory/index.db`);
    const ins = db.prepare("INSERT INTO entries (kind, at, ref, text) VALUES ('turn', ?, '', ?)");
    const now = Date.now();
    [['What is running in Demo?', 'Demo has one chat open and nothing else running. The last run listed an empty folder.'],
      ['Any services down?', 'No, Sir. No services are registered yet, so nothing is down.'],
      ['Remind me what changed today.', 'Three new projects were added: shop, blog and api-server. Nothing has run in them yet.']]
      .forEach(([s, c], i) => ins.run(now - (3 - i) * 60000, `Owner: ${s}\nCru: ${c}`));
    db.close();
  }
  await T.act('cru-history');
  p = await T.until('the chat view shows the conversation as bubbles', (p) => U(p).bubbles === 6 && !U(p).empty);
  await t.sleep(600);
  await snap('cru-chat');

  // The voice view: the field with the projects as cells, the server's message in amber.
  await T.act('cru-menu', 'chat-view');
  p = await T.until('the voice view: the field (the head, the contours, the cells)', (p) => U(p).view === 'voice' && U(p).field && U(p).micBox === 112);
  t.check('…the server\'s message in amber under it', /no Claude API key/.test(U(p).note) && U(p).noteColor === 'amber', U(p));
  await t.sleep(800);
  await snap('cru-voice');
  await snap('cru-amber');

  // The mic's states (MIC_COLORS) and the words at the right.
  const words = { listen: '', think: '', speak: '', waking: 'Waking', ended: 'Asleep', offline: 'Offline', problem: 'Unavailable' };
  const tones = { listen: 'listen', think: 'think', speak: 'speak', waking: 'start', ended: 'ended', offline: 'offline', problem: 'problem' };
  for (const [ph, wd] of Object.entries(words)) {
    await T.act('cru-state', ph);
    await T.until(`the mic ${ph}: ${tones[ph]}${wd ? `, "${wd}"` : ', no word'}`, (p) => U(p).tone === tones[ph] && U(p).word === wd);
  }

  // The × by the words being heard (canCancel): they go.
  await T.act('cru-state', 'listen');
  await T.act('cru-hear', 'turn on the lights');
  await T.until('words being heard: the × to cancel them by them', (p) => U(p).cancelX && C(p).canCancel);
  await T.act('cru-cancel');
  await T.until('…the × takes them back', (p) => !C(p).interim && !U(p).cancelX);
  // Said while Cru thinks: queued, "Queued, answered next", taken back with the turn.
  await T.act('cru-state', 'think');
  await T.act('cru-utter', 'and the blog?');
  p = await T.until('said while Cru thinks: queued, "Queued, answered next" under the words', (p) => C(p).queued === 'and the blog?' && U(p).queuedLine === 'and the blog?' && U(p).cancelX);
  await T.act('cru-cancel');
  await T.until('…the × cancels the turn with what was queued ("Cancelled.")', (p) => !C(p).pending && C(p).phase !== 'think' && /^Cancelled/.test(C(p).notice));

  // Esc closes; Ctrl+Space held opens it to talk, let go sends what was heard.
  await T.key('Escape');
  await T.until('Esc closes it', (p) => !C(p).open);
  {
    // v2: closed again, nothing of Cru's keeps drawing.
    await T.ui('tab', 'inbox');
    await t.sleep(1500);
    await T.settle(); // (F70)
    const m = T.frames.mark();
    await t.sleep(2000);
    t.check('closed: no frames', T.frames.since(m).frames === 0, T.frames.since(m));
    await T.ui('tab', 'workbench');
  }
  await key('Space', ['ctrl'], 'down');
  p = await T.until('Ctrl+Space held: Cru opens to talk (push to talk)', (p) => C(p).open && C(p).ptt && C(p).how === 'ptt');
  await t.sleep(1500);
  await T.act('cru-state', 'listen');
  await T.act('cru-hear', 'what is running in demo');
  const before = C(await T.state()).turns;
  await t.sleep(300);
  t.check('…held, nothing goes yet', C(await T.state()).turns === before);
  await key('Space', ['ctrl'], 'up');
  await T.until('…let go: what was heard goes to Cru', (p) => !C(p).ptt && C(p).turns === before + 1 && C(p).heard === 'what is running in demo', 10000);
  await T.key('Escape');
  await T.until('closed', (p) => !C(p).open);
  // The shortcut from Cru's settings (per device).
  writeFileSync(`${T.configDir}/ptt.json`, JSON.stringify({ ctrl: true, alt: true, shift: false, meta: false, key: 'k' }));
  await key('Space', ['ctrl']);
  await t.sleep(800);
  t.check('…another device shortcut: Ctrl+Space no longer opens it', !C(await T.state()).open);
  await key('k', ['ctrl', 'alt'], 'down');
  p = await T.until('…Ctrl+Alt+K held opens it, the hint says so', (p) => C(p).open && C(p).ptt && U(p).hint === 'Hold Ctrl+Alt+K to talk · click the mic to keep this open');
  await key('k', ['ctrl', 'alt'], 'up');
  await T.until('…let go', (p) => !C(p).ptt);
  rmSync(`${T.configDir}/ptt.json`, { force: true });
  // (Let go, what is heard goes 700 ms later, pttRelease: past it before hearing more.)
  await t.sleep(1000);

  // The keyboard: the typing box only while it is on (the chat view, as the web's picture).
  await T.act('cru-menu', 'chat-view');
  await T.until('back to the chat view', (p) => U(p).view === 'chat' && U(p).bubbles > 0);
  // The chat view's faded bubbles: queued, and being heard, each with its ×.
  await T.act('cru-state', 'think');
  await T.act('cru-utter', 'what about shop');
  await T.until('queued while Cru thinks: a faded bubble, "Queued: Cru answers it next"', (p) => U(p).queuedBubble && C(p).queued === 'what about shop');
  await T.act('cru-drop', 'queued');
  await T.until('…its × drops it: never asked', (p) => !U(p).queuedBubble && !C(p).pending);
  await T.act('cru-state', 'listen');
  await T.act('cru-hear', 'hello there');
  await T.until('words being heard: a faded bubble', (p) => U(p).hearingBubble);
  await T.act('cru-drop', 'hearing');
  await T.until('…its × drops them', (p) => !U(p).hearingBubble && !C(p).interim);
  await T.act('cru-keyboard', 'true');
  p = await T.until('the keyboard on: the typing box and send take the mic\'s place, no hint', (p) => C(p).keyboard && U(p).typingBox && !U(p).mic && U(p).hint === '');
  await t.sleep(600);
  await snap('cru-keyboard');
  {
    // v2: keyboard mode, Cru asleep (the mark still, nothing animating): a change of Cru's
    // (the ⋯ menu shown) re-renders Cru's view alone, and then nothing draws.
    await T.ui('tab', 'inbox');
    await T.act('cru-state', 'ended');
    await t.sleep(3000);
    const s0 = await T.state(); const m0 = T.frames.mark();
    await t.sleep(1500);
    const s1 = await T.state();
    t.check('keyboard mode, asleep: still (no frames)', T.frames.since(m0).frames === 0, { f: T.frames.since(m0), r: delta(s0.renders, s1.renders) });
    await T.act('cru-menu-open', 'true');
    await T.until('…the ⋯ menu shows', (p) => U(p).menuItems?.length === 3);
    await t.sleep(500);
    const s2 = await T.state();
    const r = delta(s1.renders, s2.renders);
    // (The tab's root, a layout view, re-renders with any view under the Shell: GPUI marks the
    // ancestors dirty; ARCHITECTURE.md rule 3. Its cached leaves do not.)
    t.check('…it re-rendered Cru\'s view, and no other (but the tab\'s layout root)', r.cru >= 1 && Object.keys(r).every((k) => k === 'cru' || k === 'inbox'), r);
    await T.act('cru-menu-open', 'false');
    await T.ui('tab', 'workbench');
  }
  await T.act('cru-keyboard', 'false');
  await T.until('…off needs a Deepgram key here: it says so and stays', (p) => C(p).keyboard && /Deepgram key/.test(C(p).notice));
  await T.key('Escape');
  await T.until('Esc closes it', (p) => !C(p).open);
}
