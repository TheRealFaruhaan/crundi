// ─── The PC lock: the window locked behind the owner's PIN (F114) ───
// src/webapp-html.js applyPcLock (#pc-lock) and app/main.js pcLockAsk: the
// tray's "Lock (visible)" / "Lock (full)", Ctrl+Shift+L and Super+Shift+L,
// the PIN (the keypad or typed), a lock ended from the phone (the server's
// pc-lock event, no other action), and the auto lock after the PC's idle time.

export async function run(T, t) {
  const D = T.D;
  const L = (p) => D(p).lock;
  const menu = (p) => D(p).tray.menu;
  const item = (p, id) => menu(p).find((m) => m.id === id) || {};
  await T.api.http('POST', '/api/voice/kid/pin', { pin: '2468' });
  await T.ui('tab', 'inbox');
  let p = await T.state();
  t.check('the tray: "Lock (visible)" and "Lock (full)" after Open', JSON.stringify(menu(p).slice(4, 7).map((m) => m.label)) === JSON.stringify(['Open', 'Lock (visible)', 'Lock (full)'])
    && item(p, 'lock-visible').enabled && item(p, 'lock-full').enabled, menu(p));

  // The full lock, from the tray, a dialog left open first: the lock draws over it and leaves it
  // as it was (the web's lock, 8990 over .input-modal 210; F212, F218).
  await T.act('test-ask');
  await T.until('a dialog left open', (p) => !!p.dialog, 5000);
  await T.act('tray-pick', 'lock-full');
  p = await T.until('the tray\'s Lock (full): the opaque lock screen, the window locked', (p) => L(p).on && L(p).mode === 'full' && D(p).windowLocked, 15000);
  t.check('…the dialog left open stays, under the lock (not cancelled)', !!p.dialog && p.dialog.title === 'Left open', p.dialog);
  await t.sleep(400);
  // A press where the dialog's scrim is (which would dismiss it) lands on the lock instead.
  await T.mouse('click', 12, 200);
  await t.sleep(400);
  p = await T.state();
  t.check('…a press over the dialog\'s scrim is the lock\'s: the dialog is still open', !!p.dialog && L(p).on, p.dialog);
  // Its keys are the lock's (F228; the web drops every key outside the lock, WH:6533): Enter on
  // the lock answers no dialog under it, one left open or one opened while locked.
  await T.key('Enter');
  await t.sleep(400);
  p = await T.state();
  t.check('…Enter on the lock does not answer the dialog under it', !!p.dialog && L(p).on, p.dialog);
  await T.act('test-ask');
  await t.sleep(400);
  await T.key('Enter');
  await t.sleep(400);
  p = await T.state();
  t.check('…nor one opened while locked', !!p.dialog && p.dialog.title === 'Left open' && L(p).on, p.dialog);
  t.check('…the tray says "Locked"; the locks and Quit are off', item(p, 'kid').label === 'Locked' && !item(p, 'kid').enabled && !item(p, 'lock-visible').enabled && !item(p, 'lock-full').enabled && !item(p, 'quit').enabled, menu(p));
  await t.sleep(400);
  await T.snap('desktop', 'pc-lock-full');
  await T.ui('window-close');
  await t.sleep(300);
  t.check('…the window\'s × keeps it', D(await T.state()).tray.hidden === false);
  for (const k of ['1', '1', '1', '1', 'OK']) await T.act('lock-key', k);
  p = await T.until('a wrong PIN on the keypad: "That is not the PIN", the box shakes', (p) => /not the PIN/.test(L(p).error) && L(p).shakes >= 1 && L(p).on, 10000);
  await T.snap('desktop', 'pc-lock-wrong');
  // F248/F253: Tab and Tab Tab stay in the lock (the web drops keys outside #pc-lock, WH:6533):
  // the focus never reaches the title bar's Minimize or Maximize, and the PIN still goes in.
  await tabsKeepPin(T, t, L, 'the full lock');
  // F233: the PIN typed on the keyboard reaches the box (the web lets keys inside the lock
  // through, WH:6524): the right one and Enter unlock.
  await T.key('2468');
  await T.key('Enter');
  p = await T.until('the right PIN typed with real keys and Enter unlocks it, and the window is let go', (p) => !L(p).on && !D(p).windowLocked, 10000);
  t.check('…and the dialog is there as it was', !!p.dialog && p.dialog.title === 'Left open', p.dialog);
  await T.ui('dialog-answer', 'false');
  await T.until('…answered', (p) => !p.dialog, 5000);
  t.check('…the tray\'s items again', item(p, 'kid').label === 'Kid mode' && item(p, 'lock-full').enabled && item(p, 'quit').enabled, menu(p));

  // The visible lock, from Ctrl+Shift+L; ended from the phone (the server's event alone).
  await T.key('l', ['ctrl', 'shift']);
  p = await T.until('Ctrl+Shift+L: the visible lock', (p) => L(p).on && L(p).mode === 'visible', 15000);
  await t.sleep(400);
  await T.snap('desktop', 'pc-lock-visible');
  await T.api.http('POST', '/api/lock/end', {});
  await T.until('ended from the phone: unlocked with no other action', (p) => !L(p).on && !D(p).windowLocked, 10000);
  // The windowed lock has no keypad: the keyboard is the way in at the PC (F233).
  await T.key('l', ['ctrl', 'shift']);
  await T.until('the visible lock again', (p) => L(p).on && L(p).mode === 'visible', 15000);
  await t.sleep(400);
  await tabsKeepPin(T, t, L, 'the visible lock');
  await T.key('2468');
  await T.key('Enter');
  await T.until('…the PIN typed with real keys and Enter unlocks the visible lock', (p) => !L(p).on && !D(p).windowLocked, 10000);
  await T.key('l', ['super', 'shift']);
  await T.until('Super+Shift+L: the full lock', (p) => L(p).on && L(p).mode === 'full', 15000);
  await T.api.http('POST', '/api/lock/end', {});
  await T.until('…ended', (p) => !L(p).on, 10000);

  // Auto lock: the setting comes with the server's event; the PC's idle time (as the OS would say it).
  await T.api.http('POST', '/api/lock/auto', { on: true, minutes: 1, mode: 'visible' });
  await T.until('auto lock on: the idle watch runs', (p) => L(p).auto.on && L(p).auto.minutes === 1 && L(p).idleWatch, 10000);
  await T.act('lock-idle', '30');
  await t.sleep(800);
  t.check('…30 s idle: not yet', !L(await T.state()).on);
  await T.act('lock-idle', '61');
  await T.until('…a minute idle: locked, in the setting\'s mode', (p) => L(p).on && L(p).mode === 'visible', 10000);
  await T.api.http('POST', '/api/lock/end', {});
  await T.until('…ended', (p) => !L(p).on, 10000);
  await T.act('lock-idle', '90');
  await t.sleep(800);
  t.check('…once per idle stretch', !L(await T.state()).on);
  await T.act('lock-idle', '1');
  await T.act('lock-idle', '0', 'true');
  await T.until('…the OS\'s screen locked: locked', (p) => L(p).on, 10000);
  await T.api.http('POST', '/api/lock/end', {});
  await T.api.http('POST', '/api/lock/auto', { on: false });
  await T.act('lock-idle', '');
  await T.until('auto lock off: the watch stops', (p) => !L(p).on && !L(p).auto.on, 10000);
  await T.ui('tab', 'workbench');
}

// Tab, then Tab Tab, then Shift+Tab: the PIN box keeps the keys each time (Minimize and Maximize,
// the title bar's tab stops, never get them), and the keys typed after a Tab still reach it.
async function tabsKeepPin(T, t, L, which) {
  const pin = async () => L(await T.state()).pinFocused === true;
  t.check(`${which}: the PIN box has the keys`, await pin(), L(await T.state()));
  await T.key('Tab');
  await t.sleep(250);
  t.check(`…after Tab the PIN box still has them (not Minimize)`, await pin(), L(await T.state()));
  await T.key('Tab');
  await T.key('Tab');
  await t.sleep(250);
  t.check(`…after Tab Tab too (not Maximize)`, await pin(), L(await T.state()));
  await T.key('Tab', ['shift']);
  await t.sleep(250);
  t.check(`…and after Shift+Tab`, await pin(), L(await T.state()));
  await T.key('Tab');
}
