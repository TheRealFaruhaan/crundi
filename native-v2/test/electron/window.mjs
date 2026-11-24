// ─── The main window and the tray (PARITY-ELECTRON.md §2) ───
// Electron's createWindow: at least 800x500 (its launch size, 1100x750, is
// the unit test electron_window_size: under the driver the window is the
// harness's), frameless, the app's icon and class; the top bar's buttons
// pressed with real clicks ask the window manager what Electron's
// window:minimize / maximize ask (a spy on the root window hears them: no
// window manager on Xvfb), and a double-click on the bar zooms; × hides it
// to the tray, the client going on (window-all-closed stays in the tray);
// the tray's own click (StatusNotifierItem Activate, as the panel sends it)
// shows it again; the tray's icon and tooltip.

import { execFileSync, execSync, spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const SPY = fileURLToPath(new URL('../tools/xwmspy.py', import.meta.url));

/** The root window's ClientMessages while `act` runs. */
async function spied(t, secs, act) {
  const p = spawn('python3', ['-I', SPY, String(secs)], { stdio: ['ignore', 'pipe', 'inherit'] });
  const lines = [];
  let buf = '';
  let ready;
  const up = new Promise((r) => { ready = r; });
  p.stdout.on('data', (b) => {
    buf += b;
    let i;
    while ((i = buf.indexOf('\n')) >= 0) { const l = buf.slice(0, i); buf = buf.slice(i + 1); try { const j = JSON.parse(l); if (j.ready) ready(); else lines.push(j); } catch { /* partial */ } }
  });
  const done = new Promise((r) => p.on('exit', r));
  await Promise.race([up, t.sleep(5000)]);
  await act();
  await t.sleep(800);
  p.kill();
  await done;
  return lines;
}

export async function run(T, t) {
  const D = T.D;
  const tr = (p) => D(p).tray;
  const p0 = await T.until('the window\'s X11 id', (p) => D(p).win?.x11 > 0, 10000);
  const xid = D(p0).win.x11;
  const props = execFileSync('xprop', ['-id', String(xid)]).toString();
  t.check('at least 800x500, as Electron\'s minWidth/minHeight', /program specified minimum size: 800 by 500/.test(props), props.match(/minimum size.*/)?.[0]);
  // Frameless: the app asks for client-side decorations (MWM_HINTS_DECORATIONS set, 0
  // decorations); gpui falls back to a frame (1) only where no compositor runs, as on Xvfb.
  const mwm = /_MOTIF_WM_HINTS\(_MOTIF_WM_HINTS\) = 0x2, 0x0, 0x([01]), 0x0, 0x0/.exec(props);
  t.check('…frameless: the decorations asked of the window manager (none with a compositor)', !!mwm, props.match(/_MOTIF_WM_HINTS.*/)?.[0]);
  // FINDINGS T6: the class is the .desktop entry's StartupWMClass (the client edition
  // here: crundi-client.desktop, StartupWMClass=Crundi Client), so the window groups under it.
  t.check('…the app\'s class (StartupWMClass "Crundi Client") and its icon', /WM_CLASS\(STRING\) = "Crundi Client", "Crundi Client"/.test(props) && /_NET_WM_ICON\(CARDINAL\)/.test(props), props.match(/WM_CLASS.*/)?.[0]);

  // The top bar's buttons, clicked as a person would.
  const at = async (words) => { const b = await T.ui('find', words); return b.ok ? { x: b.x + b.w / 2, y: b.y + b.h / 2 } : null; };
  const min = await at('Minimize');
  const max = await at('Maximize');
  t.check('the top bar\'s Minimize and Maximize buttons', !!min && !!max, { min, max });
  let seen = await spied(t, 4, () => T.mouse('click', min.x, min.y));
  t.check('a click on Minimize asks to iconify (WM_CHANGE_STATE IconicState)', seen.some((m) => m.window === xid && m.type === 'WM_CHANGE_STATE' && m.data[0] === 3), seen);
  seen = await spied(t, 4, () => T.mouse('click', max.x, max.y));
  const maxed = (l) => l.some((m) => m.window === xid && m.type === '_NET_WM_STATE' && m.data.includes('_NET_WM_STATE_MAXIMIZED_VERT') && m.data.includes('_NET_WM_STATE_MAXIMIZED_HORZ'));
  t.check('a click on Maximize asks to maximise (_NET_WM_STATE, both ways)', maxed(seen), seen);
  // An empty spot of the top bar: left of the window buttons, below the tabs' row.
  seen = await spied(t, 4, () => T.mouse('dblclick', min.x - 120, 6));
  t.check('a double-click on the top bar zooms the same way', maxed(seen), seen);

  // ×: hidden to the tray, the client goes on; the tray's click shows it.
  const close = await at('Close');
  t.check('the top bar\'s Close button', !!close, close);
  if (!tr(await T.state()).shown) { console.log('  no tray: × and the tray click not checked'); return; }
  await T.mouse('click', close.x, close.y);
  await T.until('a click on × hides the window to the tray (window-all-closed: the app stays)', (p) => tr(p).hidden === true && !T.exited, 5000);
  const names = execSync('gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus --method org.freedesktop.DBus.ListNames').toString();
  const sni = (names.match(new RegExp(`org\\.kde\\.StatusNotifierItem-${T.app.pid}-\\d+`)) || [])[0];
  t.check('the tray item on the session bus', !!sni, names.slice(0, 300));
  if (!sni) return;
  const prop = (n) => execFileSync('gdbus', ['call', '--session', '--dest', sni, '--object-path', '/StatusNotifierItem', '--method', 'org.freedesktop.DBus.Properties.Get', 'org.kde.StatusNotifierItem', n]).toString();
  t.check('…its icon (icon_32x32, for the panel to scale)', /^\(<\[\(32, 32, \[byte/.test(prop('IconPixmap')), prop('IconPixmap').slice(0, 40));
  t.check('…its tooltip "Crundi — <status>"', /Crundi — Running/.test(prop('ToolTip')), prop('ToolTip').slice(0, 120));
  execFileSync('gdbus', ['call', '--session', '--dest', sni, '--object-path', '/StatusNotifierItem', '--method', 'org.kde.StatusNotifierItem.Activate', '0', '0']);
  await T.until('…a click on the tray icon shows the window again', (p) => tr(p).hidden === false, 5000);
}
