// Pairing a phone (or any other client) with this server.
//
// Someone who is already signed in asks for a code. It is shown as a QR code;
// the app scans it and swaps the code for a login of its own. So nobody types
// a password or a 2FA code on a phone, and the phone never holds either.
//
// A code is a bearer token for whoever asked for it, which is why it is
// narrow: random, single use, minutes long, and kept in memory only, so a
// restart forgets every one of them.
//
// A spent code is remembered for a little while longer, only so the screen
// that is showing it can be told "that worked, it was this device" and play
// its ending instead of sitting there until it times out.

import { randomBytes } from 'node:crypto';

export const CODE_TTL_MS = 5 * 60 * 1000;
// How long a spent code can still be asked about.
export const CLAIMED_TTL_MS = 2 * 60 * 1000;
// Far more than anyone needs at once. It is a ceiling on what a signed-in
// caller in a loop can make the server hold, nothing else.
const MAX_PENDING = 50;

// 96 random bits. Enough several times over for something that works once and
// dies in five minutes, and short enough to leave the Crundi code room for the
// server's name (crundi-code.js).
const CODE_RE = /^[a-f0-9]{24}$/;
const PLATFORMS = new Set(['android', 'ios', 'desktop']);

/**
 * The link a QR code carries: https://server/pair#<code>.
 *
 * An ordinary https link, so a phone's own camera opens the server's page
 * (which offers the app) rather than showing a string nobody can use. The code
 * is in the FRAGMENT, which a browser does not send: it never reaches an
 * access log, a proxy or the Referer of the next page.
 */
export function pairingLink(baseUrl, code) {
  return `${String(baseUrl || '').replace(/\/+$/, '')}/pair#${code}`;
}

/** { baseUrl, code } from a scanned link, or null if it is not one of ours. */
export function parsePairingLink(link) {
  let u;
  try { u = new URL(String(link || '')); } catch { return null; }
  if (!/^https?:$/.test(u.protocol) || u.pathname.replace(/\/+$/, '') !== '/pair') return null;
  const code = u.hash.replace(/^#/, '');
  if (!CODE_RE.test(code)) return null;
  return { baseUrl: u.origin, code };
}

/** What a device says about itself, cut down to what is safe to store and show. */
export function cleanDevice(d) {
  const name = String((d && d.name) || '').replace(/[\u0000-\u001f\u007f]/g, ' ').replace(/\s+/g, ' ').trim().slice(0, 60);
  const platform = PLATFORMS.has(d && d.platform) ? d.platform : '';
  return { name: name || 'Unnamed device', platform };
}

export function createPairing({ now = () => Date.now(), ttlMs = CODE_TTL_MS } = {}) {
  const pending = new Map();   // code -> { username, collabKey, expiresAt, device: null | { name, platform } }

  function sweep() {
    const t = now();
    for (const [code, e] of pending) if (t >= e.expiresAt) pending.delete(code);
  }

  /** Codes still waiting to be used. */
  function waiting() { sweep(); let n = 0; for (const e of pending.values()) if (!e.device) n++; return n; }

  return {
    /**
     * A code for this signed-in caller. Whoever claims it becomes them: same
     * username, and the same confinement if they are a collaborator.
     */
    start({ username, collabKey = '' }) {
      sweep();
      if (!username) return { ok: false, error: 'Not signed in' };
      if (waiting() >= MAX_PENDING) return { ok: false, error: 'Too many pairing codes are waiting. Try again in a few minutes.' };
      const code = randomBytes(12).toString('hex');
      const expiresAt = now() + ttlMs;
      pending.set(code, { username, collabKey, expiresAt, device: null });
      return { ok: true, code, expiresAt, expiresIn: Math.floor(ttlMs / 1000) };
    },

    /**
     * Spend a code. Works once: the entry is gone before the caller is told
     * anything, so two claims racing cannot both win.
     * @returns {{ username: string, collabKey: string } | null}
     */
    claim(code, device) {
      sweep();
      const key = String(code || '');
      if (!CODE_RE.test(key)) return null;
      const e = pending.get(key);
      if (!e || e.device) return null;
      e.device = cleanDevice(device);
      e.expiresAt = now() + CLAIMED_TTL_MS;
      return { username: e.username, collabKey: e.collabKey, device: e.device };
    },

    /**
     * What became of a code, for the screen that is showing it. Only whoever
     * asked for the code gets an answer; to anyone else it does not exist,
     * the same as one that never did.
     * @returns {{ state: 'waiting', expiresIn: number } | { state: 'paired', device: object } | { state: 'expired' }}
     */
    status(code, { username, collabKey = '' }) {
      sweep();
      const e = pending.get(String(code || ''));
      if (!e || e.username !== username || e.collabKey !== collabKey) return { state: 'expired' };
      if (e.device) return { state: 'paired', device: { ...e.device } };
      return { state: 'waiting', expiresIn: Math.max(0, Math.ceil((e.expiresAt - now()) / 1000)) };
    },

    /** Withdraw a code: one shown and not used, or one whose claim fell through. */
    cancel(code) { return pending.delete(String(code || '')); },

    size: waiting,
  };
}
