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

  // The full lock, from the tray.
  await T.act('tray-pick', 'lock-full');
  p = await T.until('the tray\'s Lock (full): the opaque lock screen, the window locked', (p) => L(p).on && L(p).mode === 'full' && D(p).windowLocked, 15000);
  t.check('…the tray says "Locked"; the locks and Quit are off', item(p, 'kid').label === 'Locked' && !item(p, 'kid').enabled && !item(p, 'lock-visible').enabled && !item(p, 'lock-full').enabled && !item(p, 'quit').enabled, menu(p));
  await t.sleep(400);
  await T.snap('desktop', 'pc-lock-full');
  await T.ui('window-close');
  await t.sleep(300);
  t.check('…the window\'s × keeps it', D(await T.state()).tray.hidden === false);
  for (const k of ['1', '1', '1', '1', 'OK']) await T.act('lock-key', k);
  p = await T.until('a wrong PIN on the keypad: "That is not the PIN", the box shakes', (p) => /not the PIN/.test(L(p).error) && L(p).shakes >= 1 && L(p).on, 10000);
  await T.snap('desktop', 'pc-lock-wrong');
  await T.act('lock-pin', '2468');
  p = await T.until('the right PIN unlocks it, and the window is let go', (p) => !L(p).on && !D(p).windowLocked, 10000);
  t.check('…the tray\'s items again', item(p, 'kid').label === 'Kid mode' && item(p, 'lock-full').enabled && item(p, 'quit').enabled, menu(p));

  // The visible lock, from Ctrl+Shift+L; ended from the phone (the server's event alone).
  await T.key('l', ['ctrl', 'shift']);
  p = await T.until('Ctrl+Shift+L: the visible lock', (p) => L(p).on && L(p).mode === 'visible', 15000);
  await t.sleep(400);
  await T.snap('desktop', 'pc-lock-visible');
  await T.api.http('POST', '/api/lock/end', {});
  await T.until('ended from the phone: unlocked with no other action', (p) => !L(p).on && !D(p).windowLocked, 10000);
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
