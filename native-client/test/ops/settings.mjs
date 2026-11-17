// operations.mjs's "settings" section: run(T, t) with T from t.begin('ops') (test/lib.mjs).
// The Settings tab against the test server (only the test server's settings
// change): the groups load folded; Server and Claude Code; the system
// prompts; the notification matrix and channels; sign-in (password, the
// Telegram bot, Google's block, a method toggle refused); the phone app;
// the session's devices; Configuration; Terminal; Cru's server settings;
// the Desktop App section.

import { totpNow } from '../../../src/auth-password.js';

const st = (p) => p.areas.ops.settings;

/** A throwaway sign-in for the "revoke a device" step, so the test's own API session is never the one revoked.
 * A code is good once: on a refusal, wait for the next 30 s window and try again. Returns its session id. */
async function spareSession(T) {
  for (let i = 0; i < 2; i++) {
    const r = await T.api.http('POST', '/api/auth/password', { password: T.password, code: totpNow(T.secret) });
    if (r.body?.token) {
      const s = await T.api.http('GET', '/api/auth/sessions', null, r.body.token);
      return (s.body?.sessions || []).find((x) => x.current)?.id || '';
    }
    await new Promise((res) => setTimeout(res, 31000 - (Date.now() % 30000)));
  }
  return '';
}

export async function run(T, t) {
  const api = T.api.http;
  const spare = await spareSession(T);
  t.check('settings: a spare sign-in to revoke later', !!spare);
  await T.ui('tab', 'settings');
  let p = await T.until('settings: the tab loads', (p) => st(p).state === 'data');
  if (!p) return;
  t.check('settings: every section starts folded', Object.values(st(p).open).every((x) => x === false), st(p).open);
  await T.snap('settings-folded');

  // Crundi: Server and Claude Code.
  await T.act('settings-open', 'server');
  await T.act('settings-open', 'cc');
  p = await T.until('settings: the server update section loads', (p) => st(p).server.body === 'data' && st(p).claude.body === 'data');
  if (p) {
    t.check('settings: the server version', /^\d+\.\d+/.test(st(p).server.version), st(p).server);
    t.check('settings: the server offers a check or an update', ['check', 'update'].includes(st(p).server.act), st(p).server);
    t.check('settings: Restart server is offered', st(p).server.canRestart === true);
    t.check('settings: Claude Code’s version', st(p).claude.version.length > 0, st(p).claude);
  }
  await T.snap('settings-server');
  // Dev asks first; Cancel keeps production.
  await T.act('settings-do', 'srv-set-channel', '1');
  p = await T.until('settings: switching to dev asks first', (p) => p.dialog && /dev releases/.test(p.dialog.message || ''));
  await T.dialog(false);
  p = await T.until('settings: Cancel keeps production', (p) => st(p).server.channel === 0 && !p.dialog);
  // Restart asks first (and is cancelled: the server stays up).
  await T.act('settings-do', 'restart');
  await T.until('settings: Restart asks first', (p) => p.dialog && /Restart the server/.test(p.dialog.message || p.dialog.title || ''));
  await T.dialog(false);
  await T.act('settings-open', 'server', 'false');
  await T.act('settings-open', 'cc', 'false');

  // What's new: Show it again (the tour is the extras screen's).
  await T.act('settings-open', 'whatsnew');
  await T.act('settings-do', 'whats-new');
  // "Show it again" opens the whole tour (the web's openWhatsNew(true)); close it so it covers nothing after.
  if (await T.until('settings: "Show it again" opens What\'s new', (p) => p.areas.ops.extras.wn?.all === true, 10000)) {
    await T.act('extras-wn-close', 'done');
    await T.until('settings: the tour closes', (p) => p.areas.ops.extras.wn === null, 5000);
  }
  await T.act('settings-open', 'whatsnew', 'false');

  // Claude: the system prompts (everywhere, and the open project's).
  await T.act('settings-open', 'sysprompt');
  p = await T.until('settings: the project prompt is offered for the open project', (p) => st(p).prompt.project === 'Demo');
  await T.act('settings-field', 'sysprompt', 'Prefer small commits.');
  await T.act('settings-field', 'proj-sysprompt', 'Never push to main.');
  await T.act('settings-do', 'save-prompts');
  p = await T.until('settings: "System prompt saved"', (p) => /applies to new conversations/.test(st(p).prompt.status));
  let s = await api('GET', '/api/settings');
  t.check('settings: the server keeps the prompt', s.body?.systemPrompt === 'Prefer small commits.', s.body?.systemPrompt);
  const projs = await api('GET', '/api/projects');
  const demo = (projs.body?.projects || projs.body || []).find?.((x) => x.alias === 'demo');
  t.check('settings: the project keeps its prompt', !demo || demo.systemPrompt === 'Never push to main.', demo);
  await T.act('settings-open', 'sysprompt', 'false');

  // Notifications: the matrix applies at once; channels.
  await T.act('settings-open', 'channels');
  await T.act('settings-open', 'notify');
  p = await T.until('settings: the matrix and the channels', (p) => Object.keys(st(p).notify).length > 10 && st(p).channels.length > 0);
  if (p) {
    const inbox = st(p).channels.find((c) => c.id === 'inbox');
    t.check('settings: the inbox is always on, its switch locked', inbox && inbox.enabled && !inbox.canToggle, inbox);
    t.check('settings: the telegram channel has its extra line', st(p).channels.some((c) => c.telegram), st(p).channels);
    t.check('settings: the matrix shows the server’s choice', st(p).notify.finished === 'away', st(p).notify);
  }
  await T.act('settings-do', 'notify-pref', 'finished', 'always');
  p = await T.until('settings: a lamp lights at once', (p) => st(p).notify.finished === 'always');
  for (let i = 0; i < 20; i++) { s = await api('GET', '/api/settings'); if (s.body?.notifyPrefs?.finished === 'always') break; await t.sleep(250); }
  t.check('settings: the server has the new policy', s.body?.notifyPrefs?.finished === 'always', s.body?.notifyPrefs);
  // "set it up under Sign-in" opens Sign-in.
  await T.act('settings-do', 'goto-signin-telegram');
  await T.until('settings: the Telegram link opens Sign-in', (p) => st(p).open.signin === true);
  await T.act('settings-open', 'channels', 'false');
  await T.act('settings-open', 'notify', 'false');

  // Access: Sign-in.
  p = await T.until('settings: the sign-in methods', (p) => st(p).signin.pwSet === true && st(p).signin.pwOn === true && st(p).signin.oauth === true);
  if (p) {
    t.check('settings: the Telegram bot block (new server)', st(p).signin.tgHas === true && /^Not set up/.test(st(p).signin.tgStatus), st(p).signin);
    t.check('settings: Google not set up, its switch locked', st(p).signin.google.canOn === false && /^Not set up/.test(st(p).signin.google.status), st(p).signin.google);
    t.check('settings: the callback address', /\/api\/auth\/oauth\/google\/callback$/.test(st(p).signin.google.url), st(p).signin.google);
  }
  await T.snap('settings-signin');
  // The last method can't be turned off: the server says why.
  await T.act('settings-do', 'method-toggle', 'password', 'false');
  p = await T.until('settings: turning off the only method is refused', (p) => !!p.toast && !/disabled/.test(p.toast));
  p = await T.props();
  t.check('settings: the password switch stays on', st(p).signin.pwOn === true);
  // An empty password: said, nothing sent.
  await T.act('settings-do', 'set-password');
  await T.until('settings: "Type a password first"', (p) => /Type a password first/.test(p.toast || ''));
  // Google: save a client, and it says to Test.
  await T.act('settings-field', 'google-id', 'test-client.apps.googleusercontent.com');
  await T.act('settings-field', 'google-secret', 'test-secret');
  await T.act('settings-field', 'google-allowed', 'me@example.com');
  await T.act('settings-do', 'oauth-save', 'google');
  p = await T.until('settings: Google saved, not tested', (p) => st(p).signin.google.configured === true && /not tested/.test(st(p).signin.google.status));
  if (p) t.check('settings: the saved client id shows', st(p).signin.googleId === 'test-client.apps.googleusercontent.com', st(p).signin.googleId);
  await T.act('settings-do', 'oauth-clear', 'google');
  await T.until('settings: Remove asks first', (p) => p.dialog && /forget its client ID/.test(p.dialog.message || ''));
  await T.dialog(true);
  await T.until('settings: Google removed', (p) => st(p).signin.google.configured === false);
  // Telegram: a username alone is saved (no token: not set up).
  await T.act('settings-field', 'tg-user', 'someone');
  await T.act('settings-do', 'tg-save');
  await T.until('settings: the Telegram username is kept', (p) => st(p).signin.tgUser === 'someone');
  // A new password: the authenticator secret shows once.
  await T.act('settings-field', 'new-password', 'a-brand-new-password-123');
  await T.act('settings-do', 'set-password');
  p = await T.until('settings: the new authenticator secret shows', (p) => st(p).signin.enrol.length >= 16);
  await T.act('settings-open', 'signin', 'false');

  // Phone app: the push box; the pairing dialog (this server is localhost: it says so).
  await T.act('settings-open', 'phone');
  p = await T.until('settings: the phone push box', (p) => st(p).phone.push === 'data');
  if (p) t.check('settings: no key yet, who to ask', /needs a key/.test(st(p).phone.line) && /^Message @/.test(st(p).phone.contact), st(p).phone);
  await T.act('settings-do', 'phone-connect');
  p = await T.until('settings: Connect a phone opens and answers', (p) => st(p).phone.pairOpen && st(p).modal && st(p).phone.pairLine && !/Getting a code/.test(st(p).phone.pairLine));
  if (p) t.check('settings: New code is offered after the refusal', st(p).phone.pairAgain === true, st(p).phone);
  await T.act('settings-do', 'pair-close');
  await T.until('settings: Close closes it', (p) => !st(p).phone.pairOpen && !st(p).modal);
  await T.act('settings-open', 'phone', 'false');

  // Session: the signed-in devices, another one signed out.
  await T.act('settings-open', 'session');
  await T.act('settings-do', 'sess-toggle');
  p = await T.until('settings: the devices list', (p) => st(p).session.open && st(p).session.state === 'list' && st(p).session.rows.length >= 2);
  if (p) {
    t.check('settings: this device is marked', st(p).session.rows.some((r) => r.current && r.label === 'Desktop app'), st(p).session.rows);
    t.check('settings: the others are counted', st(p).session.others >= 1, st(p).session);
  }
  const other = p && st(p).session.rows.find((r) => r.id === spare);
  if (other) {
    await T.act('settings-do', 'sess-revoke', other.id);
    await T.until('settings: a device is revoked', (p) => /Session revoked/.test(p.toast || '') && !st(p).session.rows.some((r) => r.id === other.id));
  }
  await T.act('settings-do', 'sess-toggle');
  await T.act('settings-open', 'session', 'false');

  // This server: Configuration (saved; restart required).
  await T.act('settings-open', 'config');
  p = await T.until('settings: the .env path', (p) => /\.env$/.test(st(p).config.envPath));
  await T.act('settings-field', 'tunnel-url', 'https://crundi.example.com');
  await T.act('settings-do', 'save-config');
  await T.until('settings: "Saved — restart required"', (p) => /restart required/.test(st(p).config.status));
  s = await api('GET', '/api/settings');
  t.check('settings: the .env has the URL', s.body?.settings?.CLOUDFLARE_TUNNEL_URL === 'https://crundi.example.com', s.body?.settings);
  await T.act('settings-open', 'config', 'false');

  // Terminal: the newline key (this PC) and idle panes (the server).
  await T.act('settings-open', 'terminal');
  await T.act('settings-do', 'set-newline-key', '1');
  await T.until('settings: Ctrl + Enter chosen', (p) => st(p).terminal.newlineKey === 1);
  await T.act('settings-field', 'autopark', '5');
  await T.act('settings-do', 'autopark-commit');
  await T.until('settings: below the least is refused', (p) => /minutes or more/.test(p.toast || '') && st(p).terminal.autopark === '60');
  await T.act('settings-field', 'autopark', '30');
  await T.act('settings-do', 'autopark-commit');
  await T.until('settings: "Idle panes close after 30 minutes"', (p) => /after 30 minutes/.test(p.toast || ''));
  s = await api('GET', '/api/settings');
  t.check('settings: the server keeps 30', s.body?.autoParkMinutes === 30, s.body?.autoParkMinutes);
  await T.act('settings-do', 'set-newline-key', '0');
  await T.act('settings-open', 'terminal', 'false');

  // Cru: the server's settings.
  await T.act('settings-open', 'voice');
  p = await T.until('settings: Cru’s settings load', (p) => st(p).voice.shown && st(p).voice.models > 0);
  if (p) t.check('settings: the spending line', /^Claude API: \$/.test(st(p).voice.spend), st(p).voice.spend);
  await T.act('settings-do', 'vs-pick', 'model', '1');
  await T.until('settings: the model is saved', (p) => /Model saved/.test(st(p).voice.status));
  let vs = await api('GET', '/api/voice/settings');
  t.check('settings: the server has the new model', vs.body?.model === 'claude-sonnet-5-5', vs.body?.model);
  await T.act('settings-field', 'vs-limit', '60');
  await T.act('settings-do', 'vs-save-spend');
  await T.until('settings: spending saved', (p) => st(p).voice.spendNote === 'Saved');
  await T.act('settings-do', 'vs-save-keys');
  await T.until('settings: "Type a key to change it"', (p) => /Type a key/.test(st(p).voice.keysNote));
  await T.act('settings-open', 'voice', 'false');

  // The Desktop App (this client).
  await T.act('settings-open', 'desktop');
  p = await T.until('settings: the Desktop App section', (p) => /v\d/.test(st(p).desktop.status));
  await T.act('settings-do', 'launch-toggle', 'true');
  await T.until('settings: Launch at startup on', (p) => st(p).desktop.launch === true);
  await T.act('settings-do', 'launch-toggle', 'false');
  await T.until('settings: and off again', (p) => st(p).desktop.launch === false);
  await T.act('settings-do', 'upd-set-channel', '1');
  await T.until('settings: the app’s dev channel asks first', (p) => p.dialog && /the desktop app/.test(p.dialog.message || ''));
  await T.dialog(true);
  await T.until('settings: the app follows dev', (p) => st(p).desktop.channel === 1);
  await T.act('settings-do', 'upd-set-channel', '0');
  await T.act('settings-open', 'desktop', 'false');

  // Everything open at once, for the review (the page scrolls).
  for (const k of ['server', 'cc', 'signin']) await T.act('settings-open', k);
  await T.snap('settings-open');
  for (const k of ['server', 'cc', 'signin']) await T.act('settings-open', k, 'false');
}
