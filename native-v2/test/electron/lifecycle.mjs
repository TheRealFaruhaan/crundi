// ─── Lifecycle: launch at startup, crundi.log, single instance, system-wide lock keys ───
// app/main.js 1199-1233 (setLaunchAtStartup), 70-101 (writeLogLine), 2805
// (requestSingleInstanceLock, second-instance), 401-406 (globalShortcut).

import { spawn, execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const XKEYS = fileURLToPath(new URL('../tools/xkeys.py', import.meta.url));
const keys = (combo) => execFileSync('python3', ['-I', XKEYS, combo, '--steal-focus'], { encoding: 'utf8' });

export async function run(T, t) {
  const D = T.D;
  const st = (p) => p.areas.ops.settings;

  // Launch at startup: one implementation with Electron's names (the client
  // edition here: no server/ beside the binary), toggled from Settings.
  const entry = join(process.env.XDG_CONFIG_HOME, 'autostart', 'Crundi Client.desktop');
  await T.ops('settings-open', 'desktop');
  await T.until('startup: Settings > Desktop App', (p) => /v\d/.test(st(p).desktop.status));
  t.check('startup: off before it is turned on', !existsSync(entry));
  await T.ops('settings-do', 'launch-toggle', 'true');
  await T.until('startup: Launch at startup on', (p) => st(p).desktop.launch === true);
  const text = existsSync(entry) ? readFileSync(entry, 'utf8') : '';
  t.check('startup: Electron\'s entry, ~/.config/autostart/Crundi Client.desktop', text === `[Desktop Entry]\nType=Application\nName=Crundi Client\nExec="${process.env.NATIVE_BIN}"\nTerminal=false\nX-GNOME-Autostart-enabled=true\n`, text);
  const r = await T.act('startup');
  t.check('startup: the driver reads the same entry (one implementation)', r.enabled === true && r.entry === entry, r);
  t.check('startup: no second entry under another name', !existsSync(join(process.env.XDG_CONFIG_HOME, 'autostart', 'crundi-native.desktop')) && !existsSync(join(process.env.XDG_CONFIG_HOME, 'autostart', 'Crundi.desktop')));
  await T.ops('settings-do', 'launch-toggle', 'false');
  await T.until('startup: off again, the entry gone', (p) => st(p).desktop.launch === false && !existsSync(entry));
  await T.ops('settings-open', 'desktop', 'false');

  // The log file in the config folder.
  let p = await T.state();
  const logFile = D(p).logFile;
  t.check('log: crundi.log in the config folder', logFile === join(T.configDir, 'crundi.log') && existsSync(logFile), logFile);
  const log = existsSync(logFile) ? readFileSync(logFile, 'utf8') : '';
  t.check('log: the boot lines, ISO-stamped', /^\[\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z\] \[lifecycle\] Crundi Client starting$/m.test(log) && /\[lifecycle\] logFile=/.test(log), log.slice(0, 600));
  t.check('log: the startup toggles are in it', /\[startup\] Launch at login enabled/.test(log) && /\[startup\] Launch at login disabled/.test(log));

  // A second launch: the first copy's window comes back, the second goes.
  if (D(p).tray?.shown) {
    await T.ui('window-close');
    await T.until('single instance: the window hidden to the tray', (p) => D(p).tray.hidden === true, 8000);
  }
  const second = spawn(process.env.NATIVE_BIN, [], { env: { ...process.env, CRUNDI_CONFIG_DIR: T.configDir, CRUNDI_TEST_PORT: String(Number(process.env.CRUNDI_TEST_PORT) + 7) }, stdio: 'ignore' });
  const code = await new Promise((res) => { const k = setTimeout(() => { try { second.kill(); } catch { /* gone */ } res('still running'); }, 15000); second.on('exit', (c) => { clearTimeout(k); res(c); }); });
  t.check('single instance: the second launch exits by itself', code === 0, code);
  p = await T.until('single instance: …and the first copy shows its window', (p) => D(p).tray?.hidden !== true && !T.exited, 8000);
  t.check('single instance: logged as second-instance', /second-instance/.test(readFileSync(logFile, 'utf8')));

  // The lock's keys, system-wide: another window has the keyboard; real key presses (XTEST).
  await T.api.http('POST', '/api/voice/kid/pin', { pin: '2468' });
  p = await T.state();
  t.check('keys: both accelerators registered system-wide', JSON.stringify(D(p).globalKeys) === JSON.stringify([{ accelerator: 'CommandOrControl+Shift+L', registered: true }, { accelerator: 'Super+Shift+L', registered: true }]), D(p).globalKeys);
  const out = keys('ctrl+shift+l');
  t.check('keys: another window holds the focus', /focus mine/.test(out), out);
  p = await T.until('keys: Ctrl+Shift+L from another window locks it (visible)', (p) => D(p).lock?.on && D(p).lock.mode === 'visible', 15000);
  await t.sleep(600);
  await T.key('2468');
  await T.key('Enter');
  await T.until('keys: the PIN and Enter unlock it', (p) => !D(p).lock?.on && !D(p).windowLocked, 10000);
  keys('super+shift+l');
  await T.until('keys: Super+Shift+L from another window locks it (full)', (p) => D(p).lock?.on && D(p).lock.mode === 'full', 15000);
  await t.sleep(600);
  await T.key('2468');
  await T.key('Enter');
  await T.until('keys: …and unlocks with the PIN', (p) => !D(p).lock?.on && !D(p).windowLocked, 10000);

  // ─── T20: Mini on/off on this device (Settings > Desktop App, client.json "mini") ───
  await T.ops('settings-open', 'desktop');
  await T.until('T20: Settings > Desktop App shows Mini on by default', (p) => st(p).desktop.mini === true);
  await T.ops('settings-do', 'mini-toggle', 'false');
  await T.until('T20: Show Mini off', (p) => st(p).desktop.mini === false);
  const cj = () => { try { return JSON.parse(readFileSync(join(T.configDir, 'client.json'), 'utf8')); } catch { return {}; } };
  t.check('T20: saved in client.json ("mini": false)', cj().mini === false, cj());
  let m = await T.ui('mini-top', 'party');
  t.check('T20: off, Mini does not come out', m?.ok === false, m);
  await T.ops('settings-do', 'mini-toggle', 'true');
  await T.until('T20: Show Mini on again', (p) => st(p).desktop.mini === true);
  t.check('T20: saved in client.json ("mini": true)', cj().mini === true, cj());
  m = await T.ui('mini-top', 'party');
  t.check('T20: on, Mini comes out again', m?.ok === true, m);
  await T.ops('settings-open', 'desktop', 'false');
}
