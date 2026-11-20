// ─── The tray: its menu in the PC client's order and words; Server… opens the server's settings ───
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
  const want = ['Crundi', '', 'Running', '', 'Open', 'Kid mode', 'Server…', '', WAKE];
  t.check('its menu', JSON.stringify(labels.slice(0, 9)) === JSON.stringify(want) && labels[labels.length - 1] === 'Quit' && labels[labels.length - 2] === '', labels);
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
  await T.act('tray-pick', 'server');
  await T.until('Server… shows the server\'s settings', (p) => p.tab === 'settings');
  await T.ui('tab', 'workbench');
  await T.act('tray-pick', 'open');
  t.check('Open keeps the client', !T.exited);
}
