// Which clients this server will talk to.
//
// A server and the things that connect to it are updated separately: the web
// page by a reload, the desktop client by its own updater, a phone app by a
// store. So each side states the oldest version of the other it can work with,
// and refuses below that:
//
//   - the SERVER holds MIN_CLIENT, the oldest client of each kind it accepts.
//     That check is here.
//   - a CLIENT holds the oldest server it accepts, and makes that check itself
//     from what /api/handshake returns. Nothing here can do it for them.
//
// /api/handshake is the one contract every past and future version has to
// understand, because it is what an out-of-date client is told to update BY.
// Its shape is therefore frozen: add fields, never rename or remove one.
//
// This is a compatibility check and not a security one. A client states its
// own version, so anything that matters still depends on the login.
//
// THE RULE (the owner's, from 1.21.1, for every client and server: the web
// page, the desktop apps, the Android app, the phone's chat page, the relay):
// minor changes stay backwards compatible both ways, so that the minimums
// here and in each client can be pinned broadly. Only a major upgrade raises
// them, and the release says so.
//   - A new feature is a NEW endpoint or an optional field, never a changed
//     meaning of an existing one: an older client on a newer server goes on
//     working as it did.
//   - It also adds its name to CAPABILITIES below, so a newer client on an
//     older server can hide or disable it before anyone taps it.
//   - A client that still meets a missing endpoint (404, unknown route) fails
//     that one action gracefully: "This server does not support this yet.
//     Update the server to use it." The rest of the screen keeps working.

import { compareVersions, parseVersion } from './whatsnew.js';

export const HANDSHAKE_PATH = '/api/handshake';

/**
 * The oldest client of each kind this server accepts. '' means any.
 *
 * Raise one ONLY for a real incompatibility (a major upgrade, see THE RULE
 * above): every raise interrupts whoever is below it. Where a newer client can simply do without something, list it in
 * CAPABILITIES instead and let the client hide what is missing.
 */
export const MIN_CLIENT = Object.freeze({
  // 1.20.0-dev.1 is the first build that has this check at all. Starting
  // strict was deliberate: everything from here on can assume the other side
  // knows about the handshake. It is written as the dev build so that the dev
  // builds leading up to 1.20.0 are not refused by their own server; 1.20.0
  // itself sorts above it.
  web: '1.20.0-dev.1',
  desktop: '1.20.0-dev.1',
  // The first Android release is 1.0.0. The oldest build let in is the last
  // dev build before it, the first that knows a notification can be marked
  // quiet: an older one would buzz for everything while the owner is here.
  // Written as the dev build so the build that was tested is not shut out
  // of the server it was tested against; 1.0.0 itself sorts above it.
  android: '1.0.0-dev.10',
  // No release yet. Set this when the first one ships.
  ios: '',
});

/**
 * What this server can do, for clients newer than it. A phone app is updated
 * by a store and a server by hand, so "app newer than server" is the ordinary
 * case, not the exception: the app reads this and hides what is not listed
 * rather than refusing the server outright.
 */
export const CAPABILITIES = Object.freeze([
  'handshake',
  'pairing',
  'mobile-push',
  // The phone app may hold its page's login itself and renew it for every
  // page it shows (POST /api/pair/page-login gives it one), instead of
  // handing the page a refresh token of its own. See src/webapp.js.
  'app-page-login',
  // Webhooks for schedules ("When called": POST /api/schedule-hook/<id>,
  // rotateHook) and signed factory webhook triggers (auth, signatureHeader,
  // timestampHeader, idempotency, ifBusy 'manager', rotate which=secret).
  // 1.21.1 has them but does not list this, so a client also accepts that
  // version (1.21.1-dev.12 and later) or the webhook fields in the data.
  'webhooks',
  // Skills installed from a GitHub link or a pasted "npx skills add" line
  // (POST /api/skills/install-from-link), remembering their source, and
  // updated from it (POST /api/skills/update-from-source).
  'skills-install-link',
  // Factory pause options: POST /api/factory-runs action 'pause' takes mode
  // 'now' (stop the turns in progress; they carry on at resume) and
  // resumeOnReset (carry on by itself once the usage window resets). An older
  // server ignores both and does a plain pause, so clients offer the choices
  // only when this is listed.
  'factory-pause-options',
  // Factory secrets: a factory lists secrets it may use (settings.secrets,
  // ids only); POST /api/factory-runs action 'start' takes secretPins
  // ({ secretId: PIN }, kept in memory for that run), and POST /api/secrets
  // action 'check' says whether a PIN is right without the value. An older
  // server drops the list and the PINs, and every use asks for the PIN as
  // before, so clients show the picker and the start prompts only when this
  // is listed.
  'factory-secrets',
]);

const KINDS = new Set(Object.keys(MIN_CLIENT));

/**
 * MIN_CLIENT, with anything in CRUNDI_MIN_CLIENT laid over it
 * ("web=1.20.0,desktop=1.19.30"). That variable is for trying the refusal out
 * on a dev server without editing this file; a release sets MIN_CLIENT.
 */
export function readMinimums(env = process.env) {
  const out = { ...MIN_CLIENT };
  for (const part of String(env.CRUNDI_MIN_CLIENT || '').split(',')) {
    const [kind, version] = part.split('=').map((x) => (x || '').trim());
    if (KINDS.has(kind) && parseVersion(version)) out[kind] = version;
  }
  return out;
}
const ACTIVE = readMinimums();

// How each client names itself in its User-Agent. The desktop ones are what
// Electron writes unasked (productName/version), which is why clients released
// before this file existed can still be told apart and gated.
const UA_MARKS = [
  [/\bCrundiAndroid\/(\S+)/, 'android'],
  [/\bCrundiIOS\/(\S+)/, 'ios'],
  [/\bCrundiClient\/(\S+)/, 'desktop'],
  [/\bCrundi\/(\S+)(?=.*\bElectron\/)/, 'desktop'],
];

/** 'android-play/1.2.0' -> { kind: 'android', flavor: 'play', version: '1.2.0' }, or null. */
export function parseClientTag(tag) {
  const m = /^([a-z]+)(?:-([a-z0-9]+))?\/([0-9A-Za-z.+-]{1,40})$/.exec(String(tag || '').trim());
  if (!m || !KINDS.has(m[1])) return null;
  return { kind: m[1], flavor: m[2] || '', version: m[3] };
}

/**
 * Who is calling, from what the request says about itself.
 *
 * @returns {{ kind: string, flavor: string, version: string, web: string }}
 *   kind is '' for a caller that does not say (curl, the MCP bridge, a hook, a
 *   page from before this existed). Those are never refused: there is nothing
 *   to compare, and refusing them would break every script on the machine.
 *   `web` is the version of the PAGE, which a desktop or phone shell carries as
 *   well as its own, since the page inside it can be stale independently.
 */
export function identifyClient(req) {
  const headers = (req && req.headers) || {};
  let query = null;
  try { query = new URL(req.url, 'http://localhost').searchParams; } catch { /* no url */ }

  // An explicit tag wins: a header for ordinary calls, a query parameter for
  // sockets and for anything that cannot set a header.
  const tagged = parseClientTag(headers['x-crundi-client']) || parseClientTag(query && query.get('client'));
  const web = cleanVersion(headers['x-crundi-web'] || (query && query.get('web')) || '');

  if (tagged && tagged.kind !== 'web') return { ...tagged, web };
  if (tagged) return { kind: 'web', flavor: '', version: tagged.version, web: tagged.version };

  const ua = String(headers['user-agent'] || '');
  for (const [re, kind] of UA_MARKS) {
    const m = re.exec(ua);
    if (m) {
      const flavor = kind === 'android' ? ((/\bCrundiAndroid\/\S+ \((play|github)\)/.exec(ua) || [])[1] || '') : '';
      return { kind, flavor, version: cleanVersion(m[1]), web };
    }
  }
  if (web) return { kind: 'web', flavor: '', version: web, web };
  return { kind: '', flavor: '', version: '', web: '' };
}

function cleanVersion(v) {
  const s = String(v || '').trim().replace(/^v/, '');
  return parseVersion(s) ? s : '';
}

const below = (version, minimum) => !!minimum && (!version || compareVersions(version, minimum) < 0);

/**
 * Why this client is refused, or null if it is welcome.
 *
 * A shell is checked twice: itself against its own minimum, and the page it is
 * showing against the web minimum. The shell is reported first, since updating
 * it is the slower of the two and the one somebody has to do by hand.
 */
export function refusalFor(client, minimums = ACTIVE) {
  if (!client || !client.kind) return null;
  if (client.kind !== 'web' && below(client.version, minimums[client.kind])) {
    return { kind: client.kind, flavor: client.flavor || '', version: client.version, minimum: minimums[client.kind] };
  }
  if (client.web && below(client.web, minimums.web)) {
    return { kind: 'web', flavor: '', version: client.web, minimum: minimums.web };
  }
  return null;
}

// What an out-of-date client must still be able to reach: the page and its
// files (the refusal is drawn by the page), the handshake, and signing in.
// Everything else under /api, and the socket, is closed to it.
const OPEN_PREFIXES = ['/api/auth/'];

/** Does the gate apply to this path at all? */
export function isGated(path) {
  if (path === HANDSHAKE_PATH) return false;
  if (path === '/ws') return true;
  if (!path.startsWith('/api/')) return false;
  return !OPEN_PREFIXES.some((p) => path.startsWith(p));
}

/**
 * The ways of signing in that are not pairing: Telegram, a password, a
 * collaborator's phrase, first-run setup. Renewing, signing out and asking
 * which methods exist are not sign-ins and are not listed.
 */
const SIGN_IN_PATHS = new Set(['/api/auth', '/auth/telegram/callback', '/api/auth/webapp', '/api/auth/password', '/api/auth/collab', '/api/auth/setup']);

/**
 * Is this the phone app trying to sign in some way other than pairing?
 *
 * The app is two logins tied to one device: the page it shows, and its own,
 * which is what notifications, bubbles and replies run on. Pairing makes
 * both. A password typed into the page makes only the first, and what you
 * get is an app that looks signed in and does half its job: no notification
 * arrives, every bubble says the chat cannot be found. So the app has one
 * way in, and anything else is refused here rather than left to half-work.
 */
export function isUnpairedSignIn(req, path) {
  return SIGN_IN_PATHS.has(path) && identifyClient(req).kind === 'android';
}

export const PAIR_REQUIRED = {
  ok: false,
  code: 'pair_required',
  error: 'The Crundi app signs in by pairing. On another device, open Settings, then Connect a phone, and scan the picture with this app.',
};

/** The body of a refusal, the same on HTTP and in the handshake. */
export function refusalBody(refusal, serverVersion) {
  return {
    ok: false,
    error: 'client_too_old',
    client: { kind: refusal.kind, flavor: refusal.flavor, version: refusal.version },
    minimum: refusal.minimum,
    server: serverVersion,
  };
}

/** What /api/handshake answers. Frozen shape: add, never rename or remove. */
export function handshake(req, serverVersion, minimums = ACTIVE) {
  const client = identifyClient(req);
  const refusal = refusalFor(client, minimums);
  return {
    ok: true,
    server: { version: serverVersion, minClient: { ...minimums }, capabilities: [...CAPABILITIES] },
    client: {
      kind: client.kind, flavor: client.flavor, version: client.version, web: client.web,
      accepted: !refusal,
      refused: refusal ? { kind: refusal.kind, version: refusal.version, minimum: refusal.minimum } : null,
    },
  };
}
