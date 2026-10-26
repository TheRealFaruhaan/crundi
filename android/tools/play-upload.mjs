#!/usr/bin/env node
/**
 * play-upload.mjs — put a signed Play bundle on Google Play without the
 * Console, through the Android Publisher API.
 *
 *   node android/tools/play-upload.mjs <bundle.aab>                 internal app sharing: prints the install link
 *   node android/tools/play-upload.mjs <bundle.aab> --track internal [--notes notes.txt]
 *                                                                 a release on a testing track, rolled out
 *
 * The service account key (JSON) comes from the environment, never a file
 * argument: CRUNDI_PLAY_SA. Run it through the secrets store so the key is
 * never printed (secret_run, "Crundi Firebase Service Account JSON"). The
 * account needs access to the app in Play Console (Users and permissions).
 */
import { readFileSync } from 'node:fs';
import { createSign } from 'node:crypto';

const PKG = 'mv.obitech.crundi';
const API = 'https://androidpublisher.googleapis.com';

const args = process.argv.slice(2);
const file = args.find((a) => !a.startsWith('--') && /\.aab$/.test(a));
const opt = (name) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : ''; };
const track = opt('--track');
const notesFile = opt('--notes');
if (!file) { console.error('Usage: play-upload.mjs <bundle.aab> [--track internal] [--notes notes.txt]'); process.exit(2); }

let sa;
try { sa = JSON.parse(process.env.CRUNDI_PLAY_SA || ''); } catch { sa = null; }
if (!sa || !sa.client_email || !sa.private_key) { console.error('CRUNDI_PLAY_SA does not hold a service account key (JSON with client_email and private_key).'); process.exit(2); }

const b64u = (b) => Buffer.from(b).toString('base64url');

async function token() {
  const now = Math.floor(Date.now() / 1000);
  const head = b64u(JSON.stringify({ alg: 'RS256', typ: 'JWT' }));
  const claim = b64u(JSON.stringify({ iss: sa.client_email, scope: 'https://www.googleapis.com/auth/androidpublisher', aud: sa.token_uri || 'https://oauth2.googleapis.com/token', iat: now, exp: now + 3600 }));
  const sig = createSign('RSA-SHA256').update(`${head}.${claim}`).sign(sa.private_key, 'base64url');
  const r = await fetch(sa.token_uri || 'https://oauth2.googleapis.com/token', {
    method: 'POST', headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams({ grant_type: 'urn:ietf:params:oauth:grant-type:jwt-bearer', assertion: `${head}.${claim}.${sig}` }),
  });
  const j = await r.json();
  if (!j.access_token) throw new Error(`No access token: ${j.error || r.status} ${j.error_description || ''}`);
  return j.access_token;
}

async function call(tok, method, path, body, type = 'application/json') {
  const r = await fetch(API + path, { method, headers: { Authorization: `Bearer ${tok}`, ...(body ? { 'Content-Type': type } : {}) }, body });
  const text = await r.text();
  let j = {}; try { j = text ? JSON.parse(text) : {}; } catch { j = { raw: text }; }
  if (!r.ok) throw new Error(`${method} ${path.split('?')[0]}: ${r.status} ${(j.error && j.error.message) || text.slice(0, 300)}`);
  return j;
}

const bundle = readFileSync(file);
const tok = await token();

if (!track) {
  const j = await call(tok, 'POST', `/upload/androidpublisher/v3/applications/internalappsharing/${PKG}/artifacts/bundle?uploadType=media`, bundle, 'application/octet-stream');
  console.log(JSON.stringify({ ok: true, via: 'internal app sharing', downloadUrl: j.downloadUrl, sha256: j.sha256 }));
} else {
  const edit = await call(tok, 'POST', `/androidpublisher/v3/applications/${PKG}/edits`, '{}');
  const up = await call(tok, 'POST', `/upload/androidpublisher/v3/applications/${PKG}/edits/${edit.id}/bundles?uploadType=media`, bundle, 'application/octet-stream');
  const notes = notesFile ? readFileSync(notesFile, 'utf8').trim() : '';
  const release = { versionCodes: [String(up.versionCode)], status: 'completed', ...(notes ? { releaseNotes: [{ language: 'en-US', text: notes }] } : {}) };
  await call(tok, 'PUT', `/androidpublisher/v3/applications/${PKG}/edits/${edit.id}/tracks/${track}`, JSON.stringify({ track, releases: [release] }));
  await call(tok, 'POST', `/androidpublisher/v3/applications/${PKG}/edits/${edit.id}:commit`);
  console.log(JSON.stringify({ ok: true, via: `track ${track}`, versionCode: up.versionCode }));
}
