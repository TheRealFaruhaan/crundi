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
  await T.act('kid-start', 'tray');
  await T.until('the kid screen opens over everything', (p) => K(p).on && D(p).cru.open, 15000);
  await t.sleep(500);
  await T.snap('desktop', 'cru-kid');
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
  await T.until('the right PIN leaves kid mode', (p) => !K(p).on && !D(p).cru.open, 10000);
  await T.ui('tab', 'workbench');
}
