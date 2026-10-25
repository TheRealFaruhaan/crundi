/**
 * chat-secrets.js — secrets pre-authorised for ONE chat. Pure apart from timers.
 *
 * The factory's idea (factory-secrets.js), for an ordinary chat: the person
 * picks one or more secrets and how long, types each one's PIN once, and the
 * chat's secret_run then uses that secret without asking again. Keyed by the
 * chat's id, never by project: a second chat in the same project still asks.
 *
 * What is kept is the PIN, checked against its secret before it is kept, not
 * the value: the value is decrypted at each use, exactly as without a grant,
 * so the plaintext lives only in the frame that runs the command and in the
 * command's environment.
 *
 * Only for RUNNING. secret_get (Claude reading the value) asks the person every
 * time whatever is kept here; nothing in this file is consulted for it.
 *
 * Nothing here is written anywhere: not to disk, a log, the journal, a
 * transcript, an event or a response. There is no method that returns a PIN
 * except pin(), which secret_run's own frame calls; list() gives names and
 * expiry times only. A grant is gone when its time runs out, it is revoked,
 * the chat ends (its Claude exits or the chat is closed), or the process
 * stops.
 */

/** The durations clients offer. 0 minutes means "until this chat ends". */
export const PREAUTH_PRESETS = Object.freeze([
  Object.freeze({ key: '15m', minutes: 15, label: '15 minutes' }),
  Object.freeze({ key: '1h', minutes: 60, label: '1 hour' }),
  Object.freeze({ key: '8h', minutes: 480, label: '8 hours' }),
  Object.freeze({ key: 'chat', minutes: 0, label: 'Until this chat ends' }),
]);
/** The longest timed grant a client may ask for (a day). */
export const MAX_PREAUTH_MINUTES = 24 * 60;
/** How many secrets one chat may hold at once. */
export const MAX_CHAT_GRANTS = 20;

const ID_RE = /^[A-Za-z0-9_-]{1,80}$/;
const PIN_RE = /^\d{6}$/;

/**
 * How long, from a request body: { untilChatEnds: true } or { minutes: n }
 * (1..MAX_PREAUTH_MINUTES), or a preset key in { duration }.
 * @returns {{ok:true, untilChatEnds:boolean, ms:number} | {ok:false, error:string}}
 */
export function parseDuration(o) {
  const b = o && typeof o === 'object' ? o : {};
  if (typeof b.duration === 'string') {
    const p = PREAUTH_PRESETS.find((x) => x.key === b.duration);
    if (!p) return { ok: false, error: 'Unknown duration.' };
    return p.minutes ? { ok: true, untilChatEnds: false, ms: p.minutes * 60000 } : { ok: true, untilChatEnds: true, ms: 0 };
  }
  if (b.untilChatEnds === true) return { ok: true, untilChatEnds: true, ms: 0 };
  const m = Number(b.minutes);
  if (!Number.isFinite(m) || m < 1 || m > MAX_PREAUTH_MINUTES) return { ok: false, error: `Choose a time between 1 minute and ${MAX_PREAUTH_MINUTES / 60} hours, or until the chat ends.` };
  return { ok: true, untilChatEnds: false, ms: Math.round(m) * 60000 };
}

/**
 * The grants of every chat, in memory. chatId -> Map(secretId -> grant).
 *
 * @param {object} [o]
 * @param {() => number} [o.now]                 clock (tests)
 * @param {(chatId:string) => void} [o.onChange] a chat's list changed by itself (a grant ran out)
 * @param {boolean} [o.timers]                   arm a timer per timed grant (default true)
 */
export function createChatSecretVault({ now = () => Date.now(), onChange = () => {}, timers = true } = {}) {
  const chats = new Map();
  const live = (g) => g.expiresAt === null || g.expiresAt > now();

  function drop(chatId, secretId) {
    const m = chats.get(chatId);
    const g = m && m.get(secretId);
    if (!g) return false;
    if (g.timer) clearTimeout(g.timer);
    g.pin = '';
    m.delete(secretId);
    if (!m.size) chats.delete(chatId);
    return true;
  }

  const vault = {
    /**
     * Keep a PIN that has ALREADY been checked against its secret.
     * expiresAt: a time in ms, or null for "until this chat ends".
     */
    grant(chatId, secretId, { name = '', pin, expiresAt = null } = {}) {
      if (!ID_RE.test(String(chatId || '')) || !secretId || !PIN_RE.test(String(pin || ''))) return false;
      let m = chats.get(chatId);
      if (!m) { m = new Map(); chats.set(chatId, m); }
      if (!m.has(secretId) && m.size >= MAX_CHAT_GRANTS) return false;
      drop(chatId, secretId);
      if (!chats.has(chatId)) chats.set(chatId, m);
      const g = { secretId, name: String(name || ''), pin: String(pin), expiresAt: expiresAt === null ? null : Number(expiresAt), grantedAt: now(), timer: null };
      if (timers && g.expiresAt !== null) {
        const ms = Math.max(0, g.expiresAt - now());
        // Capped: setTimeout overflows past ~24.8 days, and a grant is a day at most.
        g.timer = setTimeout(() => { if (drop(chatId, secretId)) onChange(chatId); }, Math.min(ms, 2 ** 31 - 1));
        if (g.timer && g.timer.unref) g.timer.unref();
      }
      m.set(secretId, g);
      return true;
    },
    /** The kept PIN for a use from this chat, or '' (none, or run out). */
    pin(chatId, secretId) {
      const g = chats.get(chatId)?.get(secretId);
      if (!g) return '';
      if (!live(g)) { drop(chatId, secretId); onChange(chatId); return ''; }
      return g.pin;
    },
    has(chatId, secretId) { return !!vault.pin(chatId, secretId); },
    /** What a chat has: names and expiry only, never a PIN. */
    list(chatId) {
      const m = chats.get(chatId);
      if (!m) return [];
      const out = [];
      for (const g of [...m.values()]) {
        if (!live(g)) { drop(chatId, g.secretId); continue; }
        out.push({
          secretId: g.secretId, name: g.name,
          expiresAt: g.expiresAt === null ? null : new Date(g.expiresAt).toISOString(),
          untilChatEnds: g.expiresAt === null,
          grantedAt: new Date(g.grantedAt).toISOString(),
        });
      }
      return out.sort((a, b) => a.name.localeCompare(b.name));
    },
    revoke(chatId, secretId) { return drop(chatId, secretId); },
    /** Everything of one chat (it ended, or "revoke all"). The count dropped. */
    revokeChat(chatId) {
      const m = chats.get(chatId);
      if (!m) return 0;
      let n = 0;
      for (const id of [...m.keys()]) if (drop(chatId, id)) n++;
      return n;
    },
    /** A secret deleted or recreated: no chat may keep its old PIN. Chats changed. */
    forgetSecret(secretId) {
      const changed = [];
      for (const chatId of [...chats.keys()]) if (drop(chatId, secretId)) changed.push(chatId);
      return changed;
    },
    /** Chats holding anything. */
    chatIds() { return [...chats.keys()]; },
    clear() { for (const chatId of [...chats.keys()]) vault.revokeChat(chatId); },
    // Kept out of JSON.stringify and util.inspect, so a stray log cannot print a PIN.
    toJSON() { return {}; },
    [Symbol.for('nodejs.util.inspect.custom')]() { return '[chat secret vault]'; },
  };
  return vault;
}

/**
 * Pre-authorise secrets for a chat: check every PIN first and keep none unless
 * all of them are right, so a mistake in one does not leave half a grant.
 *
 * @param {object} o
 * @param {ReturnType<typeof createChatSecretVault>} o.vault
 * @param {string} o.chatId
 * @param {Array<{id:string, pin:string}>} o.secrets
 * @param {object} o.duration           parseDuration's input
 * @param {(id:string, pin:string) => {ok:boolean, name?:string, error?:string}} o.decrypt
 * @param {() => number} [o.now]
 * @returns {{ok:true, granted:string[]} | {ok:false, error:string, secretId?:string}}
 */
export function grantChatSecrets({ vault, chatId, secrets, duration, decrypt, now = () => Date.now() }) {
  const d = parseDuration(duration);
  if (!d.ok) return d;
  const list = Array.isArray(secrets) ? secrets : [];
  if (!list.length) return { ok: false, error: 'Choose at least one secret.' };
  if (list.length > MAX_CHAT_GRANTS) return { ok: false, error: `At most ${MAX_CHAT_GRANTS} secrets per chat.` };
  const checked = [];
  const seen = new Set();
  for (const s of list) {
    const id = String((s && s.id) || '');
    if (!id || seen.has(id)) continue;
    seen.add(id);
    const pin = String((s && s.pin) || '');
    if (!PIN_RE.test(pin)) return { ok: false, error: 'Enter a 6-digit PIN for each secret.', secretId: id };
    const r = decrypt(id, pin);
    if (!r || !r.ok) return { ok: false, error: (r && r.error) || 'Incorrect PIN', secretId: id };
    checked.push({ id, name: r.name || '', pin });
  }
  const expiresAt = d.untilChatEnds ? null : now() + d.ms;
  const granted = [];
  for (const c of checked) if (vault.grant(chatId, c.id, { name: c.name, pin: c.pin, expiresAt })) granted.push(c.id);
  if (!granted.length) return { ok: false, error: 'Nothing could be pre-authorised for this chat.' };
  return { ok: true, granted };
}

/**
 * Wrap secret_run's "ask the person" so a chat's grant answers instead.
 * Used for RUN only. A kept PIN that no longer opens the secret (it was
 * recreated) is dropped and the person is asked as before.
 *
 * @returns {(o:{keepPin:boolean}) => Promise<{ok:boolean, value?:string, pin?:string, error?:string, preauthorised?:boolean}>}
 */
export function preauthorisedAsk({ vault, chatId, secretId, decrypt, askPerson, onRevoked = () => {} }) {
  return async (opts = {}) => {
    const pin = chatId ? vault.pin(chatId, secretId) : '';
    if (pin) {
      const dec = decrypt(secretId, pin);
      if (dec && dec.ok) return { ok: true, value: dec.value, preauthorised: true, ...(opts.keepPin ? { pin } : {}) };
      vault.revoke(chatId, secretId);
      onRevoked(chatId);
    }
    return askPerson(opts);
  };
}
