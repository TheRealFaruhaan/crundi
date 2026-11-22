#!/usr/bin/env node
// Turn "launch at login" on in a running Electron Crundi, the way its own
// setting does (app.setLoginItemSettings / the autostart file), through the
// main process's inspector (the app started with --inspect=<port>). Prints
// what Electron then reports, so the upgrade test checks the entry Electron
// itself made, not one this test guessed.
//
//   node electron-login.mjs <inspector port>
const port = process.argv[2] || '9230';
const deadline = Date.now() + 90_000;
let target;
while (!target && Date.now() < deadline) {
  try { target = (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json())[0]; } catch { await new Promise((r) => setTimeout(r, 1000)); }
}
if (!target) { console.error('no inspector on port ' + port); process.exit(1); }
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
let id = 0; const pending = new Map();
ws.onmessage = (m) => { const d = JSON.parse(m.data); if (pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); } };
const call = (method, params) => new Promise((r) => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const body = `const app = e.app;
  if (process.platform === 'linux') {
    const fs = process.getBuiltinModule('fs'), path = process.getBuiltinModule('path'), os = process.getBuiltinModule('os');
    const file = path.join(process.env.XDG_CONFIG_HOME || path.join(os.homedir(), '.config'), 'autostart', app.getName() + '.desktop');
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, ['[Desktop Entry]', 'Type=Application', 'Name=' + app.getName(), 'Exec="' + app.getPath('exe') + '"', 'Terminal=false', 'X-GNOME-Autostart-enabled=true', ''].join('\\n'));
    return JSON.stringify({ file, name: app.getName(), exe: app.getPath('exe') });
  }
  app.setLoginItemSettings({ openAtLogin: true });
  return JSON.stringify({ name: app.getName(), exe: app.getPath('exe'), login: app.getLoginItemSettings() });`;
const tries = [
  `(async () => { const e = await import('electron'); ${body} })()`,
  `(() => { const e = require('electron'); ${body} })()`,
  `(() => { const e = process.mainModule.require('electron'); ${body} })()`,
];
for (const expression of tries) {
  const r = await call('Runtime.evaluate', { expression, includeCommandLineAPI: true, awaitPromise: true, returnByValue: true });
  if (r.result?.result?.type === 'string') { console.log(r.result.result.value); process.exit(0); }
  console.error('try failed:', JSON.stringify(r.result?.exceptionDetails?.exception?.description || r.error || r.result).slice(0, 300));
}
process.exit(1);
