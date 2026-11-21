// ─── Kid mode: started from the tray, left only with the PIN ───
// Ported from native-gpui/test/desktop.mjs ("Kid mode…"), with v2's check:
// the PIN pad's keys re-render Cru's view alone.

const delta = (r0, r1) => Object.fromEntries(Object.keys(r1 || {}).map((k) => [k, (r1[k] || 0) - ((r0 || {})[k] || 0)]).filter(([, v]) => v));

export async function run(T, t) {
  const D = T.D;
  const K = (p) => D(p).kid;
  await T.api.http('POST', '/api/voice/kid/pin', { pin: '2468' });
  // On the Inbox (the workbench's empty launcher pane animates by design).
  await T.ui('tab', 'inbox');
  // A dialog left open (as Cru's open:true card move leaves one): the kid screen covers it (F212)
  // and leaves it as it was, as the web's (F231); its keys are the kid screen's (F228).
  await T.act('test-ask');
  await T.until('a dialog left open', (p) => !!p.dialog, 5000);
  await T.act('kid-start', 'tray');
  const k0 = await T.until('the kid screen opens over everything', (p) => K(p).on && D(p).cru.open, 15000);
  t.check('…the dialog left open stays, under it (not cancelled)', !!k0.dialog && k0.dialog.title === 'Left open', k0.dialog);
  await T.key('Enter');
  await t.sleep(400);
  t.check('…Enter does not answer it', !!(await T.state()).dialog);
  await t.sleep(500);
  await T.snap('desktop', 'cru-kid');
  // Its keyboard button (F194): typing; back to the voice needs a Deepgram key, which this
  // server has not: it says so and stays in keyboard mode (keyboard(false), VJ:949).
  await T.act('kid-keyboard', 'true');
  await T.until('the kid keyboard button: typing to Cru', (p) => D(p).cru.keyboard);
  await T.act('kid-keyboard', 'false');
  await T.until('…back to the voice with no Deepgram key: it says so', (p) => /Deepgram key: add one in Settings, Voice assistant/.test(D(p).cru.notice || ''));
  t.check('…and stays in keyboard mode', D(await T.state()).cru.keyboard);
  await T.key('Escape');
  await t.sleep(300);
  t.check('Esc does not leave it', K(await T.state()).on);
  await T.act('kid-pin');
  await T.until('the PIN pad', (p) => K(p).pin);
  await t.sleep(800);
  const s0 = await T.state();
  for (const k of ['1', '1', '1', '1']) await T.act('kid-key', k);
  await T.until('…four digits', (p) => K(p).pinLen === 4);
  await t.sleep(500);
  {
    const r = delta(s0.renders, (await T.state()).renders);
    t.check('v2: the PIN\'s keys re-render Cru\'s view only', r.cru >= 1 && Object.keys(r).every((k) => k === 'cru' || k === 'inbox'), r);
  }
  await T.act('kid-key', 'OK');
  await T.until('a wrong PIN: "That is not the PIN"', (p) => /not the PIN/.test(K(p).pinError) && K(p).on, 10000);
  await T.snap('desktop', 'cru-kid-pin');
  for (const k of ['2', '4', '6', '8', 'OK']) await T.act('kid-key', k);
  {
    const p = await T.until('the right PIN leaves kid mode', (p) => !K(p).on && !D(p).cru.open, 10000);
    t.check('…and the dialog is there as it was', !!p.dialog && p.dialog.title === 'Left open', p.dialog);
    await T.ui('dialog-answer', 'false');
    await T.until('…answered', (p) => !p.dialog, 5000);
  }
  await T.ui('tab', 'workbench');
}
