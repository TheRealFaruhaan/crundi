/**
 * mobile-push.js — notifications to the phone app.
 *
 * A phone only takes a push that Apple or Google will carry, and they only
 * carry one signed by whoever published the app. A self-hosted server is not
 * that, and must not hold those keys. So there is a relay in between, run by
 * the publisher, and this file is this server's side of it.
 *
 *   server ──(ciphertext, device token, relay key)──▶ relay ──▶ Google / Apple ──▶ phone
 *
 * Three things keep that honest:
 *
 *   - The relay never sees a notification. Each one is encrypted here for one
 *     phone, with a key the phone made and only it holds (the same RFC 8291
 *     scheme browsers use, see web-push.js). The relay is handed ciphertext.
 *   - The relay does not serve strangers. Every request carries this server's
 *     own key, issued by hand by the publisher and entered in Settings. No
 *     key, no push. A server without one loses nothing else: the app still
 *     works while it is open, and the other channels still deliver.
 *   - A phone registers itself. The device a token belongs to is taken from
 *     the login making the request, never from what the request says.
 *
 * The wire format is written down in android/RELAY.md. This file and the
 * relay are released separately, so that document is the contract.
 */

import { readFileSync, writeFileSync, mkdirSync, existsSync, renameSync } from 'node:fs';
import { join } from 'node:path';
import { config } from './config.js';
import { encryptFor } from './web-push.js';

// Where the published app's relay lives.
//
// A relay only works with the app whose push keys it holds: Google issues a
// device token for one Firebase project and Apple for one app and developer
// account, and neither will take a push for it from anyone else. So this is
// not a setting for choosing between relays for the same app. Another relay
// means another build of the app, published by whoever runs that relay, and
// its users ask THEM for a key. android/RELAY.md has the whole of it.
export const DEFAULT_RELAY_URL = 'https://cr-rl-pub1.obitech.mv';
// Who to ask for a key to the published relay. Shown in Settings next to the
// empty field. A custom relay says for itself who to ask (GET /v1/info).
export const KEY_CONTACT = Object.freeze({ telegram: 'TheRealFaruhaan', url: 'https://t.me/TheRealFaruhaan', text: '' });

const FILE = () => join(config.dataDir, 'mobile-push.json');
const PLATFORMS = new Set(['android', 'ios']);
const KEY_RE = /^[A-Za-z0-9._~-]{20,200}$/;
// Google and Apple both stop at 4096 bytes for the whole message. The
// ciphertext travels as base64 inside it, so the text is cut well short.
const MAX_BODY_BYTES = 1800;
const TTL_S = 4 * 60 * 60;
const TIMEOUT_MS = 10_000;
const BATCH = 20;

let store = null;
// Why the last send did not work, for Settings. In memory: it is about now.
let lastError = '';

function load() {
  if (store) return store;
  try {
    if (existsSync(FILE())) {
      const d = JSON.parse(readFileSync(FILE(), 'utf-8'));
      if (d && typeof d === 'object') { store = { relayUrl: '', relayKey: '', relayInfo: null, devices: [], ...d }; return store; }
    }
  } catch { /* start empty */ }
  store = { relayUrl: '', relayKey: '', relayInfo: null, devices: [] };
  return store;
}

function save() {
  try {
    mkdirSync(config.dataDir, { recursive: true });
    const tmp = FILE() + '.tmp';
    // 0600: the relay key is a credential, and so is a device token to
    // anyone who also has the key.
    writeFileSync(tmp, JSON.stringify(store, null, 2), { mode: 0o600 });
    renameSync(tmp, FILE());
    return true;
  } catch (err) {
    console.error('[mobile-push] Could not save:', err.message);
    return false;
  }
}

/** A relay address we are willing to send a key to, tidied, or ''. */
export function cleanRelayUrl(input) {
  let u;
  try { u = new URL(String(input || '').trim()); } catch { return ''; }
  // The key goes in a header on every request. Plain http would hand it to
  // anyone on the path, so it is allowed only to this machine, for testing.
  const local = u.hostname === 'localhost' || u.hostname === '127.0.0.1';
  if (u.protocol !== 'https:' && !(u.protocol === 'http:' && local)) return '';
  if (u.username || u.password || u.search || u.hash) return '';
  return (u.origin + u.pathname).replace(/\/+$/, '');
}

export const relayUrl = () => load().relayUrl || DEFAULT_RELAY_URL;
export const hasKey = () => !!load().relayKey;
export const devices = () => load().devices.map(({ id, name, platform, addedAt, lastOkAt }) => ({ id, name, platform, addedAt, lastOkAt: lastOkAt || 0 }));

// What a relay says about who runs it is shown to the owner, so it is cut
// down to things that are safe to show: a Telegram name, an https link, a
// short line of plain text.
function cleanContact(c) {
  const telegram = /^[A-Za-z0-9_]{4,32}$/.test(String(c?.telegram || '').replace(/^@/, '')) ? String(c.telegram).replace(/^@/, '') : '';
  let url = '';
  try { const u = new URL(String(c?.url || '')); if (u.protocol === 'https:' && u.href.length <= 200) url = u.href; } catch { /* no link */ }
  const text = String(c?.text || '').replace(/[\u0000-\u001f\u007f<>]/g, ' ').replace(/\s+/g, ' ').trim().slice(0, 120);
  return telegram || url || text ? { telegram, url: url || (telegram ? `https://t.me/${telegram}` : ''), text } : null;
}

/** Who to ask for a key to the relay in use, or null if that relay has not said. */
export function contact() {
  const s = load();
  return s.relayUrl ? (s.relayInfo?.contact || null) : KEY_CONTACT;
}

/** One sentence telling the owner how to get a key, for wherever one is missing. */
export function keyHelp() {
  const c = contact();
  if (c?.telegram) return `Ask @${c.telegram} on Telegram for one, then enter it in Settings.`;
  if (c?.text) return `${c.text} Then enter the key in Settings.`;
  if (c?.url) return `Ask for one at ${c.url}, then enter it in Settings.`;
  let host = relayUrl();
  try { host = new URL(host).host; } catch { /* shown as it is */ }
  return `Ask whoever runs ${host} for one, then enter it in Settings.`;
}

/** Everything Settings shows. Never the key itself, only enough to recognise it. */
export function status() {
  const s = load();
  return {
    hasKey: !!s.relayKey,
    keyHint: s.relayKey ? '…' + s.relayKey.slice(-4) : '',
    relayUrl: relayUrl(),
    customRelay: !!s.relayUrl,
    relayName: s.relayUrl ? (s.relayInfo?.name || '') : '',
    contact: contact(),
    keyHelp: keyHelp(),
    devices: devices(),
    lastError,
  };
}

/**
 * Point this server at another relay, or back at the published one ('').
 *
 * For a custom build of the app only: see the note on DEFAULT_RELAY_URL. The
 * key is dropped on any change. A key belongs to the relay that issued it,
 * and sending it to a different one would hand that relay's operator a
 * working key to somebody else's.
 */
export function setRelay(url) {
  const s = load();
  const wanted = String(url ?? '').trim();
  let next = '';
  if (wanted) {
    const clean = cleanRelayUrl(wanted);
    if (!clean) return { ok: false, error: 'The relay address must be an https address.' };
    next = clean === DEFAULT_RELAY_URL ? '' : clean;
  }
  if (next !== s.relayUrl) { s.relayUrl = next; s.relayKey = ''; s.relayInfo = null; lastError = ''; }
  return save() ? { ok: true } : { ok: false, error: 'Could not save the relay address' };
}

/**
 * Ask a custom relay who runs it, so Settings can say who to ask for a key.
 * No key is sent: this is what is read BEFORE there is one.
 */
export async function refreshRelayInfo({ fetchImpl = fetch } = {}) {
  const s = load();
  if (!s.relayUrl) return { ok: true };
  try {
    const res = await fetchImpl(s.relayUrl + '/v1/info', { method: 'GET', headers: {}, signal: AbortSignal.timeout(TIMEOUT_MS) });
    const d = res.status === 200 ? await res.json() : null;
    if (!d?.ok) return { ok: false, error: `That address did not answer as a Crundi push relay (${res.status}).` };
    s.relayInfo = { name: String(d.name || '').replace(/[\u0000-\u001f\u007f<>]/g, ' ').trim().slice(0, 60), contact: cleanContact(d.contact) };
    save();
    return { ok: true };
  } catch (err) {
    return { ok: false, error: `Could not reach that relay: ${err.message}` };
  }
}

/** Store the key the publisher issued (and, for a fork, its own relay address). */
export function setKey(key, url) {
  const k = String(key || '').trim();
  if (!KEY_RE.test(k)) return { ok: false, error: 'That does not look like a push key. Paste it exactly as it was given to you.' };
  // Given with a relay address, the two are set together: the key is for
  // that relay. setRelay drops whatever key was there if the address changes.
  if (url !== undefined && url !== null) {
    const r = setRelay(url);
    if (!r.ok) return r;
  }
  const s = load();
  s.relayKey = k;
  lastError = '';
  return save() ? { ok: true } : { ok: false, error: 'Could not save the key' };
}

export function clearKey() {
  const s = load();
  s.relayKey = '';
  lastError = '';
  save();
  return { ok: true };
}

/**
 * A phone saying where to reach it. `deviceId` and `name` come from the
 * caller's login (see pairDevice in webapp.js), not from the phone's word.
 */
export function registerDevice({ deviceId, name }, reg) {
  if (!deviceId) return { ok: false, error: 'Only a paired device can register for push' };
  const platform = PLATFORMS.has(reg?.platform) ? reg.platform : '';
  const token = String(reg?.token || '');
  const p256dh = String(reg?.keys?.p256dh || ''), auth = String(reg?.keys?.auth || '');
  if (!platform || token.length < 16 || token.length > 4096) return { ok: false, error: 'That is not a usable push registration' };
  // An uncompressed P-256 point is 65 bytes and the secret 16: 87 and 22
  // characters of unpadded base64url.
  if (!/^[A-Za-z0-9_-]{87}$/.test(p256dh) || !/^[A-Za-z0-9_-]{22}$/.test(auth)) return { ok: false, error: 'That is not a usable push registration' };
  const s = load();
  const prev = s.devices.find((d) => d.id === deviceId);
  const record = { id: deviceId, name: String(name || 'Unnamed device').slice(0, 60), platform, token, keys: { p256dh, auth }, addedAt: prev?.addedAt || Date.now(), lastOkAt: prev?.lastOkAt || 0 };
  s.devices = s.devices.filter((d) => d.id !== deviceId);
  s.devices.push(record);
  save();
  return { ok: true };
}

/** Forget a device: it unregistered, or its login was revoked. */
export function removeDevice(deviceId) {
  const s = load();
  const before = s.devices.length;
  s.devices = s.devices.filter((d) => d.id !== deviceId);
  if (s.devices.length !== before) save();
  return { ok: true, removed: before - s.devices.length };
}

// Cut text to a byte length without splitting a character.
function clip(text, maxBytes) {
  let out = String(text);
  if (Buffer.byteLength(out) <= maxBytes) return out;
  out = Buffer.from(out).subarray(0, maxBytes - 3).toString();
  return out.replace(/�+$/, '') + '…';
}

/** What the phone decrypts. `v` is this shape's version; add fields, keep these. */
export function buildPayload(text, meta = {}) {
  const c = meta.chat;
  return JSON.stringify({
    v: 1,
    title: clip(meta.title || 'Crundi', 120),
    body: clip(text, MAX_BODY_BYTES),
    kind: String(meta.tag || 'crundi').slice(0, 40),
    url: String(meta.url || '/').slice(0, 300),
    at: Date.now(),
    // A change of a chat's state, for the phone's live card: `state` is
    // working, needs-input, waiting or idle, `since` is when this run of work
    // began. `silent` means draw it and make no sound; an app that does not
    // know about it sees an empty body and shows nothing.
    ...(meta.silent ? { silent: true, state: String(meta.state || '').slice(0, 20), since: Number(meta.since) || 0 } : {}),
    // Kinds of request that have all been answered (kind "clear"): the phone
    // takes those notifications down.
    ...(Array.isArray(meta.clear) ? { clear: meta.clear.map((k) => String(k).slice(0, 40)).slice(0, 10) } : {}),
    // The chat's suggested next message (kind "suggestion"), to offer on its
    // notification. Empty takes the offer away.
    ...(meta.suggestion !== undefined ? { suggestion: clip(meta.suggestion || '', 300) } : {}),
    // One permission request the chat is waiting on, when there is exactly
    // one: what the phone needs to offer Approve and Deny.
    ...(meta.prompt && meta.prompt.requestId ? { prompt: {
      requestId: String(meta.prompt.requestId).slice(0, 120),
      tool: String(meta.prompt.tool || '').slice(0, 60),
      summary: clip(meta.prompt.summary || '', 400),
    } } : {}),
    // Present when the notification is about one chat. It is what lets the
    // phone file it under that chat's bubble, name it and draw its head,
    // without asking the server anything first.
    ...(c && c.id ? { chat: {
      id: String(c.id).slice(0, 80),
      title: clip(c.title || 'Chat', 120),
      project: clip(c.project || '', 80),
      mark: c.mark ? { initials: String(c.mark.initials || '').slice(0, 4), color: String(c.mark.color || '').slice(0, 9), colorDeep: String(c.mark.colorDeep || '').slice(0, 9) } : null,
    } } : {}),
  });
}

async function post(path, body, { method = 'POST', fetchImpl = fetch } = {}) {
  const s = load();
  const res = await fetchImpl(relayUrl() + path, {
    method,
    headers: { Authorization: `Bearer ${s.relayKey}`, ...(body ? { 'Content-Type': 'application/json' } : {}) },
    body: body ? JSON.stringify(body) : undefined,
    signal: AbortSignal.timeout(TIMEOUT_MS),
  });
  let data = null;
  try { data = await res.json(); } catch { /* a relay that answers with no body */ }
  return { status: res.status, data, retryAfter: Number(res.headers.get('retry-after')) || 0 };
}

// What a refusal from the relay means to whoever is reading Settings.
function explain(status, data) {
  if (status === 401) return 'The relay does not recognise this push key.';
  if (status === 403) return 'This push key has been revoked.';
  if (status === 429) return 'This server has sent too many notifications for now. They will resume shortly.';
  if (status === 426) return 'The relay needs a newer Crundi server.';
  return `The relay answered ${status}${data?.error ? ` (${String(data.error).slice(0, 80)})` : ''}.`;
}

/**
 * Send one notification to every registered phone.
 * @returns {Promise<boolean>} true if the relay took it for at least one.
 */
export async function send(text, meta = {}, { fetchImpl = fetch } = {}) {
  const s = load();
  if (!s.relayKey || !s.devices.length) return false;
  const payload = buildPayload(text, meta);
  let sent = 0;
  const gone = [];
  for (let i = 0; i < s.devices.length; i += BATCH) {
    const group = s.devices.slice(i, i + BATCH);
    const messages = group.map((d) => ({
      platform: d.platform,
      token: d.token,
      payload: encryptFor(payload, d.keys.p256dh, d.keys.auth).toString('base64url'),
      ttl: TTL_S,
      priority: 'high',
      // Per chat where there is one: a second chat finishing must not
      // replace the first one's undelivered notification.
      collapse: (String(meta.tag || 'crundi') + (meta.chat?.id ? '.' + String(meta.chat.id).replace(/[^A-Za-z0-9._-]/g, '') : '')).slice(0, 40),
    }));
    let r;
    try {
      r = await post('/v1/push', { messages }, { fetchImpl });
    } catch (err) {
      lastError = `Could not reach the relay: ${err.message}`;
      console.warn(`[mobile-push] ${lastError}`);
      continue;
    }
    if (r.status !== 200 || !Array.isArray(r.data?.results)) {
      lastError = explain(r.status, r.data);
      console.warn(`[mobile-push] ${lastError}`);
      // A key that is wrong for one phone is wrong for all of them.
      if (r.status === 401 || r.status === 403) break;
      continue;
    }
    r.data.results.forEach((one, k) => {
      const d = group[k];
      if (!d) return;
      if (one?.status === 'sent') { sent++; d.lastOkAt = Date.now(); }
      // The app was uninstalled, or its token was replaced. Stop sending to it.
      else if (one?.status === 'gone') gone.push(d.id);
    });
  }
  if (sent) lastError = '';
  if (gone.length) {
    s.devices = s.devices.filter((d) => !gone.includes(d.id));
    console.log(`[mobile-push] Dropped ${gone.length} device(s) whose registration ended`);
  }
  if (sent || gone.length) save();
  return sent > 0;
}

/**
 * Ask the relay whether the key is good, without sending anything.
 * @returns {Promise<{ ok: boolean, label?: string, error?: string }>}
 */
export async function checkKey({ fetchImpl = fetch } = {}) {
  if (!hasKey()) return { ok: false, error: 'No push key has been entered.' };
  let r;
  try {
    r = await post('/v1/key', null, { method: 'GET', fetchImpl });
  } catch (err) {
    lastError = `Could not reach the relay: ${err.message}`;
    return { ok: false, error: lastError };
  }
  if (r.status !== 200 || !r.data?.ok) {
    lastError = explain(r.status, r.data);
    return { ok: false, error: lastError };
  }
  lastError = '';
  return { ok: true, label: String(r.data.label || '').slice(0, 80) };
}

export function _reset() { store = null; lastError = ''; }
