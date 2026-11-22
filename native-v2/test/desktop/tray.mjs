// ─── The tray: its menu in the PC client's order and words; Server… shows the server step over the app ───
// (native-gpui/test/desktop.mjs's section; app/main.js updateTrayMenu, with
// "Hey Cru"'s item as voiceWakeTrayItems adds it.) v2: a pick wakes the pump
// from the tray's thread; no pick, no pump.

import { execSync } from 'node:child_process';

const WAKE = 'Listen for "Hey Cru" while plugged in';

export async function run(T, t) {
  const D = T.D;
  const tr = (p) => D(p).tray;
  const p = await T.until('the tray says the connection', (p) => tr(p).tooltip === 'Crundi — Running');
  const labels = tr(p).menu.map((m) => m.label);
  const want = ['Crundi', '', 'Running', '', 'Open', 'Lock (visible)', 'Lock (full)', 'Kid mode', 'Server…', '', WAKE];
  t.check('its menu', JSON.stringify(labels.slice(0, 11)) === JSON.stringify(want) && labels[labels.length - 1] === 'Quit' && labels[labels.length - 2] === '', labels);
  const wake = tr(p).menu.find((m) => m.id === 'wake');
  const trained = !!D(p).wake?.trained;
  t.check('…"Hey Cru" a check item, off until trained', wake && wake.checked === !!D(p).wake?.enabled && wake.enabled === trained
    && (trained || labels.includes('    Train it first in Settings, Voice assistant')), tr(p).menu);
  t.check('…Kid mode and Quit open while the kid screen is not locked', tr(p).menu.find((m) => m.id === 'kid').enabled && tr(p).menu.find((m) => m.id === 'quit').enabled);
  // With a session bus (dbus-run-session), the item is on it.
  if (process.env.DBUS_SESSION_BUS_ADDRESS) {
    t.check('the tray is up', tr(p).shown, tr(p));
    const names = execSync('gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus --method org.freedesktop.DBus.ListNames').toString();
    t.check('…a StatusNotifierItem on the session bus', /org\.kde\.StatusNotifierItem-\d+-\d+/.test(names), names.slice(0, 300));
  } else console.log('  no session bus: the tray is not checked (run under dbus-run-session)');
  // Electron's showServerScreen (F131): the server step over the signed-in
  // app, its box holding the address; Cancel goes back to the app.
  await T.act('tray-pick', 'server');
  await T.until('Server… shows the server step over the app, still signed in', (p) => p.signin?.serverOver === true && /^Connected/.test(p.signin?.serverNote || '') && p.signedIn !== false);
  await T.ui('server-screen-save', 'crundi.example.com');
  await T.until('…an address without http(s):// is refused, as Electron\'s', (p) => p.signin?.serverNote === 'Enter the full server address, including http:// or https://');
  await T.ui('server-screen', 'false');
  await T.until('…Cancel goes back to the app', (p) => p.signin?.serverOver === false);
  await T.act('tray-pick', 'open');
  t.check('Open keeps the client', !T.exited);
  // The window's × with the tray up hides the window to it, as Electron's
  // (F113); the client goes on, and the tray's Open brings it back.
  if (tr(p).shown) {
    await T.ui('window-close');
    await T.until('× hides the window to the tray (the client goes on)', (p) => tr(p).hidden === true && !T.exited);
    await T.act('tray-pick', 'open');
    await T.until('…and the tray\'s Open shows it again', (p) => tr(p).hidden === false);
    // Wayland's way (F127): the window closed, the app going on in the tray,
    // opened again by Open on the same state (here on X11, asked of the driver).
    await T.ui('tab', 'inbox');
    await T.act('hide-how', 'close');
    await T.ui('window-close');
    await T.until('Wayland: × closes the window, the client goes on in the tray', (p) => D(p).window === false && tr(p).hidden === true && !T.exited);
    await T.act('tray-pick', 'open');
    let q = await T.until('…the tray\'s Open opens it again, on the same screen', (p) => D(p).window !== false && tr(p).hidden === false && p.tab === 'inbox', 20000);
    t.check('…signed in, the same project', q.signedIn && q.project === 'demo', { signedIn: q.signedIn, project: q.project });
    await T.act('hide-how', '');
    // Kid-locked, the window stays shown.
    await T.api.http('POST', '/api/voice/kid/pin', { pin: '2468' });
    await T.act('kid-start', 'tray');
    await T.until('kid mode on', (p) => D(p).kid.on, 15000);
    await T.ui('window-close');
    await t.sleep(400);
    t.check('…while kid-locked, the × keeps the window', tr(await T.state()).hidden === false && D(await T.state()).windowLocked);
    await T.act('kid-pin');
    for (const k of ['2', '4', '6', '8', 'OK']) await T.act('kid-key', k);
    await T.until('…kid mode left: the window let go', (p) => !D(p).kid.on && !D(p).windowLocked, 10000);
    await T.ui('tab', 'workbench');
  }
}
