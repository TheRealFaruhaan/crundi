/**
 * app-login.js — signing an app in through the browser, as desktop apps do
 * with an OAuth-enabled service.
 *
 * The app (the native PC client) cannot show Telegram's widget or a provider's
 * pages, and should not have to know each sign-in method. So it asks for a
 * pending login and opens the server's own web page in the person's browser:
 *
 *   1. app:     POST /api/auth/app/start {device, origin}
 *               -> {url: <origin>/?app=<id>, pollId, confirmCode}
 *   2. browser: the web UI at that address; the person signs in there with any
 *               method this server offers (or is signed in already). The page
 *               asks "Sign in Crundi on <device>?", showing confirmCode.
 *               Allow -> POST /api/auth/app/approve {id}; Deny -> .../deny.
 *   3. app:     POST /api/auth/app/poll {pollId} until approved: it gets a
 *               device login of its own (as pairing gives one: listed with
 *               the devices, revocable).
 *
 * Two values, deliberately different: the id in the link (it passes through
 * a browser and its history) only identifies the request and can approve it
 * with a signed-in session; the pollId never leaves the app and is the only
 * way to collect the login. The confirm code lets the person check that the
 * page they approve on is the one their app opened.
 *
 * In memory: a pending login lives ten minutes and is collected once.
 */

import { randomBytes } from 'node:crypto';
import { cleanDevice } from './pairing.js';

export const APP_LOGIN_TTL_MS = 10 * 60 * 1000;
const MAX_PENDING = 100;
// No 0/O, 1/I/L: read off one screen and checked on another.
const CODE_LETTERS = 'ABCDEFGHJKMNPQRSTUVWXYZ23456789';

function confirmCode() {
  const b = randomBytes(8);
  let s = '';
  for (let i = 0; i < 8; i++) s += CODE_LETTERS[b[i] % CODE_LETTERS.length];
  return s.slice(0, 4) + '-' + s.slice(4);
}

export function createAppLogins({ now = () => Date.now(), ttlMs = APP_LOGIN_TTL_MS } = {}) {
  /** id -> {pollId, device, confirmCode, created, status, result} */
  const pending = new Map();

  function sweep() {
    const t = now();
    for (const [id, p] of pending) if (t - p.created > ttlMs) pending.delete(id);
  }

  function byPoll(pollId) {
    for (const [id, p] of pending) if (p.pollId === pollId) return [id, p];
    return [null, null];
  }

  return {
    /** A new pending login for this device. */
    start(device) {
      sweep();
      if (pending.size >= MAX_PENDING) return null;
      const id = randomBytes(12).toString('hex');
      const pollId = randomBytes(24).toString('hex');
      const p = { pollId, device: cleanDevice(device), confirmCode: confirmCode(), created: now(), status: 'waiting', result: null };
      pending.set(id, p);
      return { id, pollId, confirmCode: p.confirmCode, expiresIn: Math.round(ttlMs / 1000) };
    },

    /** What the page shows before Allow: the device and the code (or null). */
    info(id) {
      sweep();
      const p = pending.get(String(id || ''));
      if (!p || p.status !== 'waiting') return null;
      return { device: p.device, confirmCode: p.confirmCode };
    },

    /**
     * Allow it: [makeLogin] (called with the device) gives the app's login.
     * False when there is nothing waiting under this id.
     */
    approve(id, makeLogin) {
      sweep();
      const p = pending.get(String(id || ''));
      if (!p || p.status !== 'waiting') return false;
      p.result = makeLogin(p.device);
      p.status = 'approved';
      return true;
    },

    deny(id) {
      sweep();
      const p = pending.get(String(id || ''));
      if (!p || p.status !== 'waiting') return false;
      p.status = 'denied';
      return true;
    },

    /**
     * The app asking: {status: waiting|denied|expired}, or {status: approved,
     * ...login} once (the login is handed over and forgotten).
     */
    poll(pollId) {
      sweep();
      const [id, p] = byPoll(String(pollId || ''));
      if (!p) return { status: 'expired' };
      if (p.status === 'approved') {
        pending.delete(id);
        return { status: 'approved', ...p.result };
      }
      if (p.status === 'denied') {
        pending.delete(id);
        return { status: 'denied' };
      }
      return { status: 'waiting' };
    },
  };
}
