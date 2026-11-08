#!/usr/bin/env node
/**
 * test-auth-telegram.mjs — Telegram as one sign-in method and one channel.
 *
 * Telegram used to live in Settings > Configuration, with its token shown in
 * the page. Now the bot (token + allowed username) is set under Sign-in, the
 * chat it sends to under Notification Channels, and the token is write-only.
 * What can go wrong quietly:
 *   - an older client loads /api/settings and posts every field back: it
 *     must not wipe (or corrupt) the token it was not shown;
 *   - the token reaching the browser through any of the new routes;
 *   - the Configuration form still writing Telegram keys, or a blank chat ID;
 *   - removing the bot when it is the only way to sign in.
 *
 * Boots a real server (isolated: its own data dir, .env and a loopback port)
 * and talks to it over HTTP.
 *
 * Run: node scripts/test-auth-telegram.mjs
 */

import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const { hashPassword, generateTotpSecret, totpNow } = await import('../src/auth-password.js');

let failures = 0;
const check = (label, ok, detail) => {
  if (!ok) failures++;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${label}${detail === undefined ? '' : ` -> ${detail}`}`);
};

// ─── Static: the page ───
const html = readFileSync(join(root, 'src', 'webapp-html.js'), 'utf8');
const saveFn = html.slice(html.indexOf('async function saveSettings()'), html.indexOf('async function saveSystemPrompts()'));
check('Configuration has no Telegram fields any more', !/id="set-bot-token"|id="set-username"|id="set-chat-id"/.test(html));
check('Configuration no longer says Telegram is for login and notifications', !/Used for login and notifications|Only this Telegram user can access Crundi/.test(html));
check('the Configuration save sends no Telegram keys', saveFn.length > 0 && !/TELEGRAM_BOT_TOKEN|ALLOWED_USERNAME/.test(saveFn));
check('the Configuration save sends no chat ID (it would blank it)', !/chatId/.test(saveFn));
check('Sign-in has a Telegram block with a write-only token', /id="auth-telegram-token" type="password"/.test(html) && /telegram-save/.test(html));
check('Channels > Telegram has the chat ID and the bot state', /id="tg-chat-id"/.test(html) && /telegramChannelExtra/.test(html));
check('the login page has a Sign in with Telegram button', /id="oauth-btn-telegram"/.test(html) && /Sign in with Telegram/.test(html));
check('the Telegram button uses the official widget API (Telegram.Login.auth)', /window\.Telegram\.Login\.auth\(\{ bot_id: botId, request_access: 'write' \}/.test(html));
check('it posts the user to /api/auth as the widget did', /JSON\.stringify\(\{ telegramLogin: user \}\)/.test(html));
check('no hand-built oauth.telegram.org redirect is left', !/location\.href = 'https:\/\/oauth\.telegram\.org/.test(html));
check('the embedded widget stays the fallback (no bot id, desktop app, popup failed)', /telegramFallback\(/.test(html) && /!isDesktopApp\(\)/.test(html));

// ─── A real, isolated server ───
const tmp = mkdtempSync(join(tmpdir(), 'crundi-tg-'));
const dataDir = join(tmp, 'data'); mkdirSync(dataDir, { recursive: true });
mkdirSync(join(tmp, 'projects'), { recursive: true });
mkdirSync(join(tmp, '.config'), { recursive: true });
const envFile = join(tmp, 'env');
const TOKEN = '123456789:AAFakeTokenForTestsOnly_abcdefghijk';
writeFileSync(envFile, `TELEGRAM_BOT_TOKEN=${TOKEN}\nALLOWED_USERNAME=owner_one\nTLS_MODE=off\nKEEP_ME=yes\n`);
const totp = generateTotpSecret();
writeFileSync(join(dataDir, 'auth.json'), JSON.stringify({ password: { hash: hashPassword('test-password-1234'), totpSecret: totp, enabled: true } }), { mode: 0o600 });

const port = await new Promise((res) => { const s = createServer(); s.listen(0, '127.0.0.1', () => { const p = s.address().port; s.close(() => res(p)); }); });
const env = { ...process.env };
for (const k of ['TELEGRAM_BOT_TOKEN', 'ALLOWED_USERNAME', 'CLOUDFLARE_TUNNEL_TOKEN', 'CLOUDFLARE_TUNNEL_URL', 'CLOUDFLARE_DNS_TOKEN',
  'TLS_MODE', 'TLS_DOMAIN', 'TLS_EMAIL', 'TLS_WILDCARD', 'WEB_PORT', 'CRUNDI_API_URL', 'CRUNDI_API_KEY', 'PROJECTS_DIR',
  'CRUNDI_UI_SESSION', 'CRUNDI_CHAT_ID', 'CRUNDI_PROJECT', 'CRUNDI_TERMINAL_ID', 'CRUNDI_PASSWORD_HASH', 'CRUNDI_TOTP_SECRET']) delete env[k];
Object.assign(env, {
  HOME: tmp, XDG_CONFIG_HOME: join(tmp, '.config'), APPDATA: join(tmp, '.config'),
  DATA_DIR: dataDir, DOTENV_PATH: envFile, PROJECTS_DIR: join(tmp, 'projects'),
  TELEGRAM_BOT_TOKEN: TOKEN, ALLOWED_USERNAME: 'owner_one',   // as the real start-up would have loaded them
  CRUNDI_DEV: '1', DEV_WEB_PORT: String(port),
});
const child = spawn(process.execPath, ['src/index.js', '--dev'], { cwd: root, env, detached: true, stdio: ['ignore', 'pipe', 'pipe'] });
let log = '';
child.stdout.on('data', (d) => { log += d; });
child.stderr.on('data', (d) => { log += d; });
const stop = () => { try { process.kill(-child.pid, 'SIGTERM'); } catch { /* gone */ } };
process.on('exit', stop);

const B = `http://127.0.0.1:${port}`;
let up = false;
for (let i = 0; i < 60 && !up; i++) {
  try { up = (await fetch(B + '/api/auth/methods')).ok; } catch { await new Promise(r => setTimeout(r, 500)); }
}
check('the isolated server came up', up, up ? undefined : log.slice(-400));
if (!up) { stop(); process.exit(1); }

const login = await (await fetch(B + '/api/auth/password', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ password: 'test-password-1234', code: totpNow(totp) }) })).json();
check('signed in with the password', !!login.token);
const H = { Authorization: `Bearer ${login.token}`, 'Content-Type': 'application/json' };
const get = async (p) => { const r = await fetch(B + p, { headers: H }); return { status: r.status, text: await r.text() }; };
const post = async (p, body) => { const r = await fetch(B + p, { method: 'POST', headers: H, body: JSON.stringify(body) }); return { status: r.status, body: await r.json() }; };
const envNow = () => readFileSync(envFile, 'utf8');
const envVal = (k) => (envNow().match(new RegExp(`^${k}=(.*)$`, 'm')) || [])[1];

// What the browser is given.
const methods = await get('/api/auth/methods');
const mj = JSON.parse(methods.text);
check('/api/auth/methods: Telegram is on', mj.telegram === true);
check('/api/auth/methods: the bot id is the public number only', mj.telegramBotId === '123456789' && !methods.text.includes('AAFake'));
const settings = await get('/api/settings');
const sj = JSON.parse(settings.text);
check('/api/settings never carries the bot token', !settings.text.includes(TOKEN) && !settings.text.includes('AAFake'));
check('/api/settings gives a placeholder where a token is set', !!sj.settings.TELEGRAM_BOT_TOKEN && sj.settings.TELEGRAM_BOT_TOKEN !== TOKEN);
const cfg = await get('/api/auth/config');
const cj = JSON.parse(cfg.text);
check('/api/auth/config never carries the bot token', !cfg.text.includes('AAFake'));
check('/api/auth/config telegramBot: set, the username, a status', cj.telegramBot && cj.telegramBot.tokenSet === true && cj.telegramBot.allowedUsername === 'owner_one'
  && ['connected', 'offline', 'not-set-up'].includes(cj.telegramBot.status) && cj.telegramBot.restartRequired === false, JSON.stringify(cj.telegramBot));
const ch = await get('/api/notify/channels');
const tgc = JSON.parse(ch.text).channels.find((c) => c.id === 'telegram');
check('/api/notify/channels: the Telegram entry has chatId and botStatus', tgc && tgc.telegram && typeof tgc.telegram.chatId === 'string' && !!tgc.telegram.botStatus && tgc.telegram.tokenSet === true);
check('/api/notify/channels never carries the bot token', !ch.text.includes('AAFake'));

// The old Configuration save, as an older client does it.
let r = await post('/api/settings', { settings: { ...sj.settings }, chatId: '' });
check('an old client posting back the placeholder keeps the token', r.body.ok && envVal('TELEGRAM_BOT_TOKEN') === TOKEN, envVal('TELEGRAM_BOT_TOKEN'));
check('and keeps keys the form does not own', envVal('KEEP_ME') === 'yes');
r = await post('/api/settings', { settings: { ...sj.settings, TELEGRAM_BOT_TOKEN: '555:BBOldPathTokenForTests_abcdefghijk', ALLOWED_USERNAME: 'owner_two' } });
check('the old save path still sets a new token and username', r.body.ok && envVal('TELEGRAM_BOT_TOKEN') === '555:BBOldPathTokenForTests_abcdefghijk' && envVal('ALLOWED_USERNAME') === 'owner_two');
r = await post('/api/settings', { settings: { PROJECTS_DIR: join(tmp, 'projects'), WEB_PORT: '', CLOUDFLARE_TUNNEL_TOKEN: '', CLOUDFLARE_TUNNEL_URL: '', DATA_DIR: '' } });
check('the new Configuration save leaves the Telegram keys alone', r.body.ok && envVal('TELEGRAM_BOT_TOKEN') === '555:BBOldPathTokenForTests_abcdefghijk' && envVal('ALLOWED_USERNAME') === 'owner_two');

// Sign-in > Telegram.
r = await post('/api/auth/config', { action: 'telegram-save', botToken: '', allowedUsername: '@owner_one' });
check('telegram-save with a blank token keeps the token', r.body.ok && envVal('TELEGRAM_BOT_TOKEN') === '555:BBOldPathTokenForTests_abcdefghijk' && envVal('ALLOWED_USERNAME') === 'owner_one', JSON.stringify(r.body));
check('telegram-save says a restart is needed when the token differs from the running one', r.body.restartRequired === true);
r = await post('/api/auth/config', { action: 'telegram-save', botToken: TOKEN, allowedUsername: 'owner_one' });
check('telegram-save sets the token', r.body.ok && envVal('TELEGRAM_BOT_TOKEN') === TOKEN);
check('back to what is running: no restart needed', r.body.restartRequired === false);
r = await post('/api/auth/config', { action: 'telegram-save', botToken: 'not a token', allowedUsername: 'owner_one' });
check('a malformed token is refused', r.status === 400 && envVal('TELEGRAM_BOT_TOKEN') === TOKEN);
r = await post('/api/auth/config', { action: 'telegram-save', botToken: TOKEN + '\nINJECTED=1', allowedUsername: 'owner_one' });
check('a token with a line break cannot add .env lines', r.status === 400 && !/^INJECTED=/m.test(envNow()));
r = await post('/api/auth/config', { action: 'telegram-save', botToken: '', allowedUsername: 'bad name!' });
check('a malformed username is refused', r.status === 400);

// Channels > Telegram: the chat ID.
r = await post('/api/notify/telegram/chat', { chatId: '424242' });
const ch2 = JSON.parse((await get('/api/notify/channels')).text).channels.find((c) => c.id === 'telegram');
check('the chat ID saves and reads back', r.body.ok && ch2.telegram.chatId === '424242');
r = await post('/api/notify/telegram/chat', { chatId: 'abc' });
check('a chat ID that is not a number is refused', r.status === 400);

// The last-method rule reaches the bot too.
r = await post('/api/auth/config', { action: 'disable', method: 'password' });
check('with Telegram on, password can be turned off', r.body.ok);
r = await post('/api/auth/config', { action: 'telegram-clear' });
check('the bot cannot be removed while it is the only way in', r.status === 400 && envVal('TELEGRAM_BOT_TOKEN') === TOKEN);
r = await post('/api/auth/config', { action: 'enable', method: 'password' });
check('password back on', r.body.ok);
r = await post('/api/auth/config', { action: 'telegram-clear' });
check('then the bot can be removed', r.body.ok && envVal('TELEGRAM_BOT_TOKEN') === '' && r.body.restartRequired === true);

stop();
rmSync(tmp, { recursive: true, force: true });
console.log(failures ? `\n${failures} FAILED` : '\nAll Telegram sign-in and channel checks passed.');
process.exit(failures ? 1 : 0);
