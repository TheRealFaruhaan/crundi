// Saved passwords: offered after a sign-in that worked, saved behind the
// PIN, filled, shown with the PIN (native-gpui/test/desktop.mjs "Saved
// passwords…"). v2's own: the password bar is a leaf of its own beside the
// page; the Passwords panel draws from its copy.
//
// No desktop keyring under the test's own D-Bus session: the app takes its
// key from CRUNDI_PW_TEST_KEY (honoured only with the driver on), so the
// app is started again with one.

import { pageServer, dr, evalTab, mcpOf, relaunch } from './drawer.mjs';

export async function run(T, t) {
  const PAGE = await pageServer();
  const D = (p) => dr(T, p);
  const PW = (p) => T.D(p).passwords;
  const mcp = mcpOf(T);
  const key = process.env.CRUNDI_PW_TEST_KEY || Array.from({ length: 32 }, () => Math.floor(Math.random() * 256).toString(16).padStart(2, '0')).join('');
  await relaunch(T, t, { CRUNDI_PW_TEST_KEY: key });

  // ─── Saved passwords: offered after a sign-in that worked, saved behind the PIN, filled, shown with the PIN ───
  await T.api.http('POST', '/api/voice/kid/pin', { pin: '2468' });
  await T.until('the passwords\' key is ready', (p) => PW(p).ready && PW(p).available, 15000);
  if (!D(await T.state()).open) await T.act('browser');
  const nt = await T.act('new-tab', `${PAGE}/login`);
  const tab = nt.id;
  // (Long: the first page after a start waits on Chromium's keyring look-up under dbus-run-session.)
  await T.until('a sign-in page in a tab', (p) => D(p).tabs.some((x) => x.id === tab && /login/.test(x.title)), 60000);
  await t.sleep(800);
  await evalTab(T, t, tab, "document.getElementById('u').value='me@example.com';document.getElementById('p').value='s3cret-pw!';document.getElementById('go').click();true");
  let p = await T.until('a sign-in that worked offers to save', (p) => PW(p).bars[tab]?.kind === 'save' && PW(p).bars[tab].username === 'me@example.com', 30000);
  await T.until('…the password bar drawn (a leaf of its own)', (p) => (p.renders || {})['drawer-pw'] > 0, 5000);
  await T.snap('desktop', 'pw-offer');
  await T.act('pw-save', 'me@example.com');
  p = await T.until('Save keeps it (the PIN is set)', (p) => PW(p).logins.length === 1 && !PW(p).bars[tab]);
  const id = PW(p)?.logins[0]?.id;
  let r = await mcp('browser_eval', { key: `drawer:${tab}`, code: "'the password is s3cret-pw!'" });
  t.check('a chat never reads it back from the tab', r.ok && r.result === 'the password is ••••••••', r);
  await T.act('go', `${PAGE}/login`);
  p = await T.until('back on the sign-in page: the fill bar', (p) => PW(p).bars[tab]?.kind === 'fill' && PW(p).bars[tab].logins[0].id === id, 15000);
  await T.snap('desktop', 'pw-fill');
  await T.act('pw-fill', id);
  let v;
  for (let i = 0; i < 20; i++) { v = await evalTab(T, t, tab, "document.getElementById('u').value+'|'+document.getElementById('p').value"); if (v.result === 'me@example.com|s3cret-pw!') break; await t.sleep(300); }
  t.check('Fill puts the login in the page', v.result === 'me@example.com|s3cret-pw!', v);
  await T.act('panel', 'passwords');
  await T.until('the Passwords panel', (p) => D(p).panel === 'passwords' && (p.renders || {})['drawer-body'] > 0);
  await T.act('pw-show', id);
  await T.until('Show asks for the PIN first', (p) => PW(p).pinFor === id);
  await T.act('pw-unlock', '0000');
  await t.sleep(800);
  t.check('…a wrong PIN shows nothing', !PW(await T.state()).revealed.includes(id));
  await T.act('pw-unlock', '2468');
  await T.until('…the PIN shows it', (p) => PW(p).revealed.includes(id));
  await T.until('(the wrong PIN\'s toast has gone)', (p) => !p.toast, 10000);
  await T.snap('desktop', 'pw-panel');
  await T.act('pw-remove', id);
  await T.until('Delete asks first', (p) => /Delete this saved password/.test(p.dialog?.title || JSON.stringify(p.dialog || '')));
  await T.ui('dialog-answer', 'true');
  await T.until('…and deletes it', (p) => PW(p).logins.length === 0);
  await T.act('panel-close');
  await T.act('close-tab', tab);
}
