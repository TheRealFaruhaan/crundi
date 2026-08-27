/**
 * forwards.js — expose a local port on a subdomain of this server.
 *
 * `myapp.crundi.example.com` → `localhost:3000`, routed inside the server we
 * already run. No second cloudflared process, no random hostname that changes
 * every restart, and nothing to wait for.
 *
 * A SUBDOMAIN rather than a path. Path-based proxying looks tidier until you
 * point it at a real dev server: Vite and friends emit root-absolute asset
 * URLs, set cookies at Path=/, and open their HMR socket at /. Under /f/myapp/
 * every one of those misses, and rewriting HTML on the way past is a losing
 * game. On its own hostname the app is at / and none of it arises.
 *
 * ─── Who can reach these ───
 *
 * Private by default: a forward sits behind the same session check as the rest
 * of Crundi, so it is reachable by you and not by the internet. That is the
 * opposite of a quick tunnel, which is public by construction — deliberately,
 * because "let me look at this on my phone" is the common case and quietly
 * publishing someone's dev database is not a default worth having.
 *
 * `public: true` opts out for the cases that need it — a webhook from Stripe,
 * a preview for someone with no account. Those get an unguessable hostname
 * rather than the pretty name, so the URL itself is the secret.
 */

import { readFileSync, writeFileSync, mkdirSync, existsSync, renameSync } from 'fs';
import { join } from 'path';
import { randomBytes } from 'crypto';
import { request as httpRequest } from 'http';
import { config } from './config.js';

const FILE = () => join(config.dataDir, 'forwards.json');
const NAME_RE = /^[a-z0-9](?:[a-z0-9-]{0,30}[a-z0-9])?$/;

// Names that would shadow infrastructure people usually have on a domain. Kept
// deliberately short: pointing the MAIN domain here means a wildcard DNS record,
// and an explicit record for mail or www beats a wildcard anyway — this is
// belt-and-braces, not a policy. "api" and "admin" are not on the list because
// they are perfectly ordinary things to call a forward.
const RESERVED = new Set(['www', 'mail', 'ns', 'ns1', 'ns2', 'mx', 'autodiscover', 'autoconfig']);

let store = null;

function load() {
  if (store) return store;
  try {
    if (existsSync(FILE())) {
      const d = JSON.parse(readFileSync(FILE(), 'utf-8'));
      if (d && Array.isArray(d.forwards)) { store = d; return store; }
    }
  } catch { /* defaults */ }
  store = { forwards: [] };
  return store;
}

function save() {
  try {
    mkdirSync(config.dataDir, { recursive: true });
    const tmp = FILE() + '.tmp';
    writeFileSync(tmp, JSON.stringify(store, null, 2));
    renameSync(tmp, FILE());
    return true;
  } catch (err) {
    console.error('[forwards] Could not save:', err.message);
    return false;
  }
}

/** The domain forwards live under, e.g. crundi.example.com. */
export function baseDomain() {
  return String(config.tlsDomain || config.forwardDomain || '').trim().toLowerCase();
}

/**
 * Warn when the chosen domain is probably not covered by a certificate.
 *
 * Cloudflare's free Universal SSL covers example.com and *.example.com — ONE
 * level. Nested under a subdomain, *.crundi.example.com is not covered, and the
 * failure is invisible until you try: DNS resolves, the edge answers, ping is
 * perfectly happy, and only the TLS handshake fails. Saying so at startup is
 * cheaper than discovering it from a browser error.
 *
 * Deliberately a hint rather than a rule: the label count cannot distinguish
 * example.co.uk from crundi.example.com without a public-suffix list, which is
 * more machinery than a warning deserves.
 */
export function warnIfLikelyUncovered() {
  const base = baseDomain();
  if (!base) return;
  const labels = base.split('.').length;
  if (labels < 3) return;                    // example.com — fine
  if (config.tlsWildcard && config.cfDnsToken) return;   // issuing our own wildcard
  console.warn('');
  console.warn(`[forwards] Forwards are set up under *.${base}`);
  console.warn('[forwards] If that domain sits behind Cloudflare, the free Universal SSL');
  console.warn('[forwards] certificate covers only one level of subdomain, so these will');
  console.warn(`[forwards] resolve and proxy but fail the TLS handshake ("not covered by a`);
  console.warn('[forwards] certificate").');
  console.warn('[forwards]');
  console.warn('[forwards] A certificate on THIS machine does not fix that. While a record is');
  console.warn('[forwards] proxied there are two TLS legs: browser->Cloudflare uses');
  console.warn(`[forwards] Cloudflare's certificate, and only Cloudflare->origin uses ours.`);
  console.warn('[forwards] The handshake fails on the first leg, so the request never arrives');
  console.warn('[forwards] here and our certificate is never offered.');
  console.warn('[forwards]');
  console.warn('[forwards] Fixes: move the forwards one level up so the existing wildcard');
  console.warn('[forwards] covers them, buy Advanced Certificate Manager, or set the records');
  console.warn('[forwards] to DNS-only and use TLS_WILDCARD=1 — which exposes this origin IP.');
  console.warn('');
}

/**
 * What ways of exposing a port are available here, and what each costs.
 *
 * Presented as a choice rather than decided for the user: the right answer
 * depends on whether the thing needs to be public, whether it copes with not
 * being at the root of its origin, and whether they are willing to spend money
 * or expose an IP. None of that is knowable from here.
 */
export function options() {
  const base = baseDomain();
  const labels = base ? base.split('.').length : 0;
  const nested = labels >= 3;
  // One level up, so the free wildcard covers it. Crude — it cannot tell
  // example.co.uk from crundi.example.com — so it is offered as a suggestion
  // for the user to confirm, never applied silently.
  const suggested = nested ? base.split('.').slice(-2).join('.') : base;
  const ownWildcard = !!(config.tlsWildcard && config.cfDnsToken);

  return {
    domain: base,
    choices: [
      {
        id: 'path',
        label: 'A path on this server',
        example: `${base ? scheme() + '://' + base : 'http://<this server>'}${PATH_PREFIX}/myapp/`,
        available: true,
        setup: 'None. Works immediately, including through a tunnel.',
        caveat: 'The app is not at the root of its origin. Fine for APIs, webhooks and '
              + 'simple servers; dev servers that emit root-absolute asset URLs (Vite, CRA) '
              + 'will not load correctly.',
      },
      {
        id: 'subdomain',
        label: 'Its own subdomain',
        example: base ? `${scheme()}://myapp.${nested ? suggested : base}` : '(needs a domain)',
        available: !!base,
        setup: base
          ? (nested && !ownWildcard
              ? `A wildcard DNS record for *.${suggested}. Using *.${base} instead would need `
                + `paid Advanced Certificate Manager, because Cloudflare's free certificate covers `
                + `only one level of subdomain — or set those records to DNS-only and let Crundi `
                + `issue a Let's Encrypt wildcard, which exposes this machine's IP.`
              : `A wildcard DNS record for *.${base}.`)
          : 'Set TLS_DOMAIN or FORWARD_DOMAIN first.',
        caveat: 'The app is at the root of its own hostname, so anything works. '
              + 'Needs DNS, and certificate coverage for the name you choose.',
        suggestedDomain: nested ? suggested : base,
        nested,
      },
      {
        id: 'tunnel',
        label: 'A Cloudflare quick tunnel',
        example: 'https://<random-words>.trycloudflare.com',
        available: true,
        setup: 'None, but it starts another cloudflared process and takes a few seconds.',
        caveat: 'Public to anyone with the link, and the hostname is random and changes '
              + 'every time. Good for a one-off share or a webhook; poor for anything you '
              + 'want to keep reaching.',
      },
    ],
  };
}

export function list() {
  return load().forwards.map(f => ({ ...f, url: urlFor(f) }));
}

function urlFor(f) {
  const base = baseDomain();
  if (f.mode === 'path') {
    // Relative to whatever address Crundi is already reached on, so this works
    // through a tunnel, over plain HTTP, from a LAN address — anywhere.
    return `${base ? `${scheme()}://${base}` : ''}${PATH_PREFIX}/${f.host}/`;
  }
  if (!base) return `http://localhost:${f.port}`;
  return `${scheme()}://${f.host}.${base}`;
}

/**
 * Which scheme the world reaches these on.
 *
 * Not the same question as whether WE terminate TLS. Behind a Cloudflare tunnel
 * or a reverse proxy the origin speaks plain HTTP while the public URL is https,
 * so deriving this from TLS_MODE alone reported http:// for a URL that is only
 * reachable over https.
 */
function scheme() {
  if (config.forwardScheme) return config.forwardScheme;
  if (config.tlsMode && config.tlsMode !== 'off') return 'https';
  if (config.tunnelToken || config.tunnelUrl) return 'https';   // fronted by Cloudflare
  return 'http';
}

export function add({ name, port, mode = '', isPublic = false, description = '' } = {}) {
  const p = Number(port);
  if (!Number.isInteger(p) || p < 1 || p > 65535) return { ok: false, error: `Not a usable port: ${port}` };

  let host = String(name || '').toLowerCase().trim();
  if (!host) host = 'app' + randomBytes(2).toString('hex');
  if (!NAME_RE.test(host)) {
    return { ok: false, error: 'Use lowercase letters, digits and hyphens (max 32), not starting or ending with a hyphen.' };
  }
  if (RESERVED.has(host)) return { ok: false, error: `"${host}" is reserved.` };

  const s = load();
  if (s.forwards.some(f => f.host === host || (isPublic && f.publicHost === host))) {
    return { ok: false, error: `"${host}" is already in use.` };
  }

  const f = {
    host,
    port: p,
    // 'subdomain' needs DNS and certificate coverage; 'path' needs neither and
    // works the moment it is created, at the cost of the app having to cope
    // with not being at the root of its origin.
    mode: mode === 'path' ? 'path' : 'subdomain',
    // A public forward is reachable by anyone who knows the hostname, so the
    // hostname carries the entropy rather than the pretty name.
    public: !!isPublic,
    publicHost: isPublic ? `${host}-${randomBytes(8).toString('hex')}` : '',
    description: String(description || '').slice(0, 200),
    createdAt: Date.now(),
  };
  if (isPublic) f.host = f.publicHost;

  s.forwards.push(f);
  if (!save()) return { ok: false, error: 'Could not save' };
  console.log(`[forwards] ${f.host} -> localhost:${p}${isPublic ? ' (public)' : ''}`);
  return { ok: true, forward: { ...f, url: urlFor(f) } };
}

export function remove(host) {
  const s = load();
  const before = s.forwards.length;
  s.forwards = s.forwards.filter(f => f.host !== host);
  if (s.forwards.length === before) return { ok: false, error: 'Not found' };
  save();
  return { ok: true };
}

export const PATH_PREFIX = '/tunnel';

/**
 * Match a request path to a path-mode forward.
 *
 * Returns the forward and the upstream path with the prefix removed, so the app
 * sees the URL it expects rather than one nested under /tunnel/name.
 */
export function matchPath(reqUrl) {
  const u = String(reqUrl || '');
  if (!u.startsWith(PATH_PREFIX + '/')) return null;
  const rest = u.slice(PATH_PREFIX.length + 1);
  const slash = rest.indexOf('/');
  const qmark = rest.indexOf('?');
  const cut = slash === -1 ? (qmark === -1 ? rest.length : qmark) : slash;
  const name = rest.slice(0, cut).toLowerCase();
  if (!name) return null;
  const f = load().forwards.find(x => x.host === name && x.mode === 'path');
  if (!f) return null;
  const tail = rest.slice(cut) || '/';
  return { forward: f, upstreamPath: tail.startsWith('/') ? tail : '/' + tail };
}

/** Match an incoming Host header to a subdomain forward, or null. */
export function match(hostHeader) {
  const base = baseDomain();
  if (!base || !hostHeader) return null;
  const h = String(hostHeader).toLowerCase().replace(/:\d+$/, '');
  if (!h.endsWith('.' + base)) return null;
  const sub = h.slice(0, -(base.length + 1));
  // Only one level: a.b.crundi.example.com is not a forward, and treating it as
  // one would make the wildcard certificate's coverage a lie.
  if (sub.includes('.')) return null;
  return load().forwards.find(f => f.host === sub && f.mode !== 'path') || null;
}

/**
 * Proxy a request to the forwarded port.
 *
 * Headers pass through mostly untouched: the app believes it is at the root of
 * its own hostname, which is the entire reason for doing it this way. Only
 * X-Forwarded-* are added, so an app that cares can tell.
 */
export function proxy(forward, req, res, upstreamPath) {
  const headers = { ...req.headers };
  delete headers['accept-encoding'];   // avoid double-encoding through the hop
  headers['x-forwarded-proto'] = req.socket.encrypted ? 'https' : 'http';
  headers['x-forwarded-host'] = req.headers.host || '';
  headers['x-forwarded-for'] = req.socket.remoteAddress || '';

  const upstream = httpRequest({
    host: '127.0.0.1',
    port: forward.port,
    method: req.method,
    path: upstreamPath || req.url,
    headers,
  }, (up) => {
    res.writeHead(up.statusCode || 502, up.headers);
    up.pipe(res);
  });

  upstream.on('error', (err) => {
    if (res.headersSent) { try { res.destroy(); } catch { /* ignore */ } return; }
    res.writeHead(502, { 'Content-Type': 'text/plain; charset=utf-8' });
    res.end(
      `Nothing is answering on port ${forward.port}.\n\n`
      + `This hostname forwards to localhost:${forward.port} on the machine running Crundi. `
      + `Start whatever should be listening there, or remove the forward.\n\n`
      + `(${err.code || err.message})\n`
    );
  });

  req.pipe(upstream);
}

/**
 * Proxy a WebSocket upgrade.
 *
 * Without this every dev server's hot reload dies at the first hop, which is
 * exactly the thing you notice ten minutes later and blame on something else.
 */
export function proxyUpgrade(forward, req, socket, head, upstreamPath) {
  const upstream = httpRequest({
    host: '127.0.0.1',
    port: forward.port,
    method: req.method,
    path: upstreamPath || req.url,
    headers: req.headers,
  });

  upstream.on('upgrade', (upRes, upSocket, upHead) => {
    const lines = [`HTTP/1.1 ${upRes.statusCode} ${upRes.statusMessage}`];
    for (const [k, v] of Object.entries(upRes.headers)) {
      for (const one of [].concat(v)) lines.push(`${k}: ${one}`);
    }
    socket.write(lines.join('\r\n') + '\r\n\r\n');
    if (upHead?.length) socket.unshift(upHead);
    upSocket.pipe(socket);
    socket.pipe(upSocket);
    upSocket.on('error', () => socket.destroy());
    socket.on('error', () => upSocket.destroy());
  });

  upstream.on('response', () => socket.destroy());   // upstream refused to upgrade
  upstream.on('error', () => socket.destroy());
  if (head?.length) upstream.write(head);
  upstream.end();
}
