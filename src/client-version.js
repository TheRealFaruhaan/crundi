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

import { compareVersions, parseVersion } from './whatsnew.js';

export const HANDSHAKE_PATH = '/api/handshake';

/**
 * The oldest client of each kind this server accepts. '' means any.
 *
 * Raise one ONLY for a real incompatibility: every raise interrupts whoever is
 * below it. Where a newer client can simply do without something, list it in
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
  // No release yet. Set these when the first one ships.
  android: '',
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
