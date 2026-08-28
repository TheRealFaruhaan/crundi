/**
 * webapp.js — Always-on web server for Crundi.
 *
 * Serves the terminal UI, handles WebSocket connections for terminal I/O,
 * authenticates via Telegram Login Widget, and provides REST API for
 * projects, services, and file downloads.
 *
 * Unlike the old dashboard, this server never shuts down on idle.
 * It's the primary user interface — Telegram is only for login + notifications.
 */

import { createServer } from 'node:http';
import { createServer as createHttpsServer } from 'node:https';
import * as tls from './tls.js';
import { createReadStream, readFileSync, writeFileSync, readdirSync, statSync, existsSync, mkdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { createHmac, randomBytes, createHash } from 'node:crypto';
import { basename, join, dirname, resolve, isAbsolute, sep } from 'node:path';
import { homedir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { WebSocketServer } from 'ws';
import { getWebappHtml } from './webapp-html.js';
import { getAllServiceStatus, startService, stopService, restartService, getServiceLogs, deleteService } from './services.js';
import { registerService, listRegisteredForProject, updateRegistered, getRegistered } from './service-registry.js';
import { startTunnel, startNamedTunnel, stopTunnel, getTunnelInfo, getAllTunnelInfo, waitForTunnel } from './tunnel.js';
import * as browserMod from './browser.js';
import * as terminalsMod from './terminals.js';
import { listProjects, getProject, registerProject, removeProject, getProjectMode, importFromOldData, importServicesFromOldData, setProjectOrder } from './project-store.js';
import * as kanban from './kanban-store.js';
import * as secrets from './secrets-store.js';
import * as mindmap from './mindmap-store.js';
import * as media from './media-store.js';
import * as schedule from './schedule-store.js';
import * as usage from './usage.js';
import { getOldAppDataDir, isFreshInstall, envPath } from './config.js';
import { ensureGitignore } from './claude-terminals.js';
import { listResumable, latestTranscript, isHeavyResume, HEAVY_TOKENS, HEAVY_AGE_HOURS } from './claude-ui.js';
import { createLimitWarmer } from './limit-warmer.js';
import * as authConfig from './auth-config.js';
import telegramify from 'telegramify-markdown';
import * as channels from './notify-channels.js';
import * as serverUpdate from './server-update.js';
import * as forwards from './forwards.js';
import * as webPush from './web-push.js';
import { createChatSchedule } from './chat-schedule.js';

const __dirname = dirname(fileURLToPath(import.meta.url));

// Single per-project folder for uploads/screenshots/pasted images.
export const ATTACHMENTS_DIRNAME = 'crundi_attachments';
function ensureAttachmentsDir(projectPath) {
  const dir = join(projectPath, ATTACHMENTS_DIRNAME);
  if (!existsSync(dir)) mkdirSync(dir, { recursive: true });
  ensureGitignore(projectPath); // keep the attachments folder (and .mcp.json) out of git
  return dir;
}

// ─── Vendor files (xterm.js, addon-fit) ───
const VENDOR_DIR = join(__dirname, '..', 'app', 'vendor');
const VENDOR_MIME = {
  '.js': 'application/javascript',
  '.css': 'text/css',
  '.map': 'application/json',
};

// ─── PWA assets (icons) ───
// Dev: <repo>/assets (../assets from src). Packaged: electron-builder copies
// assets to resources/assets via extraResources, which is ../../assets from
// the unpacked src dir. Pick whichever exists.
const ASSETS_DIR = [
  join(__dirname, '..', 'assets'),
  join(__dirname, '..', '..', 'assets'),
].find(p => existsSync(p)) || join(__dirname, '..', 'assets');
const ASSET_MIME = {
  '.png': 'image/png',
  '.ico': 'image/x-icon',
  '.svg': 'image/svg+xml',
};

// App version (drives the SW cache name so each release is detected as an update).
const APP_VERSION = (() => {
  try { return JSON.parse(readFileSync(join(__dirname, '..', 'package.json'), 'utf8')).version || '0'; }
  catch { return '0'; }
})();

// Web app manifest — makes Crundi installable as a PWA.
const WEB_MANIFEST = JSON.stringify({
  name: 'Crundi',
  short_name: 'Crundi',
  description: 'Claude Code terminal in your browser',
  start_url: '/',
  scope: '/',
  display: 'standalone',
  // No `orientation` key: a standalone PWA that declares an orientation (even
  // 'any') overrides the device's auto-rotate lock. Omitting it makes Crundi
  // follow the OS rotation setting like every other app.
  background_color: '#0a0a0f',
  theme_color: '#0a0a0f',
  icons: [
    { src: '/assets/icon_128x128.png', sizes: '128x128', type: 'image/png', purpose: 'any' },
    { src: '/assets/icon_192x192.png', sizes: '192x192', type: 'image/png', purpose: 'any' },
    { src: '/assets/icon_256x256.png', sizes: '256x256', type: 'image/png', purpose: 'any maskable' },
    { src: '/assets/icon_512x512.png', sizes: '512x512', type: 'image/png', purpose: 'any maskable' },
  ],
});

// Minimal service worker — registers a fetch handler (required for
// installability) and serves the cached app shell when offline. API, MCP,
// WebSocket, SSE, and download routes are always passed straight to the
// network so live data is never stale.
const SERVICE_WORKER = `
const VERSION = '${APP_VERSION}';
const CACHE = 'crundi-shell-' + VERSION;
const SHELL = ['/', '/manifest.webmanifest', '/assets/icon_128x128.png', '/assets/icon_256x256.png'];

self.addEventListener('install', (e) => {
  // Precache the shell but do NOT skipWaiting: let the new worker wait so the
  // page can prompt the user, then apply on demand (or auto on next launch).
  // add() each, NOT addAll(): addAll is all-or-nothing, so one 404 in the
  // shell list rejects the whole install and the worker NEVER activates. That
  // is not a degraded cache, it is a dead service worker - no push, no offline,
  // and navigator.serviceWorker.ready hanging forever with nothing logged. A
  // missing icon must not be able to do that.
  e.waitUntil(caches.open(CACHE).then((c) => Promise.all(
    SHELL.map((u) => c.add(u).catch((err) => {
      console.warn('[sw] could not precache ' + u + ': ' + (err && err.message));
    })),
  )));
});

self.addEventListener('activate', (e) => {
  e.waitUntil(
    caches.keys().then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim())
  );
});

// The page tells a waiting worker to take over (user clicked Update, or a fresh
// launch with a pending update).
self.addEventListener('message', (e) => {
  if (e.data && e.data.type === 'SKIP_WAITING') self.skipWaiting();
});

// ─── Push ───
// Same worker serves an ordinary browser tab and the desktop app; both are
// Chromium, so one implementation covers both.
self.addEventListener('push', (e) => {
  let d = {};
  try { d = e.data ? e.data.json() : {}; } catch (err) { d = { body: e.data ? e.data.text() : '' }; }
  if (!d.body) return;
  e.waitUntil(self.registration.showNotification(d.title || 'Crundi', {
    body: d.body,
    icon: '/assets/icon_256x256.png',
    badge: '/assets/icon_128x128.png',
    // Same tag replaces rather than stacks, so a busy session does not bury
    // the phone in near-identical lines.
    tag: d.tag || 'crundi',
    data: { url: d.url || '/' },
  }));
});

self.addEventListener('notificationclick', (e) => {
  e.notification.close();
  const target = (e.notification.data && e.notification.data.url) || '/';
  // Focus an existing window rather than opening yet another one.
  e.waitUntil(self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then((list) => {
    for (const c of list) {
      if ('focus' in c) { c.navigate ? c.navigate(target) : null; return c.focus(); }
    }
    return self.clients.openWindow ? self.clients.openWindow(target) : undefined;
  }));
});

self.addEventListener('fetch', (e) => {
  const req = e.request;
  if (req.method !== 'GET') return;
  const url = new URL(req.url);
  if (url.origin !== self.location.origin) return;
  // Never cache live/dynamic endpoints.
  if (/^\\/(api|ws|dl|vendor)\\b/.test(url.pathname) || url.pathname.startsWith('/auth/')) return;

  if (req.mode === 'navigate') {
    // App shell: network-first, fall back to cached '/'.
    e.respondWith(
      fetch(req).then((res) => {
        caches.open(CACHE).then((c) => c.put('/', res.clone()));
        return res;
      }).catch(() => caches.match('/'))
    );
    return;
  }
  // Static assets: cache-first.
  e.respondWith(caches.match(req).then((hit) => hit || fetch(req)));
});
`;

const TUNNEL_KEY = '__webapp__';
// Short-lived access token + long-lived rotating refresh token, so a session
// survives indefinitely while it is being used but a stolen access token is
// only useful for minutes. Previously a single 24h token expired on an absolute
// timer with no renewal path, which silently bricked the Electron window (it
// authenticates once from ?key= at load and cannot re-run that flow).
const ACCESS_TTL_MS = 15 * 60 * 1000;               // 15 minutes
const REFRESH_TTL_MS = 30 * 24 * 60 * 60 * 1000;    // 30 days, sliding

const startedAt = Date.now();

/**
 * Create the Crundi web app server.
 *
 * @param {{ config: object, claudeTerminals: object, claudeUi?: object, bot?: object, mcpDispatch?: function }} deps
 *   claudeUi — Claude chat-session manager (UI mode); optional
 *   bot — grammy Bot instance (for Telegram notifications from MCP tools)
 *   mcpDispatch — optional handler for MCP tool calls from stdio servers
 */
export function createWebApp({ config, claudeTerminals, claudeUi, bot, mcpDispatch, serverLogs, onServerLog, getChatId, setChatId }) {
  let server = null;
  let acmeServer = null;
  let wss = null;
  let port = null;
  // Plain-HTTP loopback listener, only when TLS has taken the main port.
  let localPort = null;
  let localServer = null;
  let tunnelUrl = null;

  // Internal API key for MCP stdio servers — persisted so it survives restarts
  // (otherwise .mcp.json files in project dirs become stale)
  const keyFile = join(config.dataDir, '.api-key');
  let internalApiKey;
  try {
    if (existsSync(keyFile)) {
      internalApiKey = readFileSync(keyFile, 'utf-8').trim();
    }
  } catch { /* ignore */ }
  if (!internalApiKey) {
    internalApiKey = randomBytes(32).toString('hex');
    try { writeFileSync(keyFile, internalApiKey); } catch { /* ignore */ }
  }

  // Access tokens: token → { username, familyId, expiresAt }
  const tokens = new Map();
  // Refresh tokens: token → { username, familyId, expiresAt, used }
  // A "family" is one login. Rotation issues a new refresh token per use and
  // marks the old one used; presenting a used token means it leaked, so the
  // whole family is revoked rather than just that token.
  const refreshTokens = new Map();

  // Temporary file shares: token → { filePath, filename, expiresAt }
  const sharedFiles = new Map();
  let shareCleanupTimer = null;

  function cleanExpiredShares() {
    const now = Date.now();
    for (const [tok, entry] of sharedFiles) {
      if (now >= entry.expiresAt) sharedFiles.delete(tok);
    }
  }

  function shareFile(filePath, ttlMinutes = 30) {
    const tok = randomBytes(32).toString('hex');
    sharedFiles.set(tok, {
      filePath,
      filename: basename(filePath),
      expiresAt: Date.now() + ttlMinutes * 60 * 1000,
    });
    if (!shareCleanupTimer) {
      shareCleanupTimer = setInterval(cleanExpiredShares, 60_000);
      shareCleanupTimer.unref?.();
    }
    return tok;
  }

  function getFileShareUrl(token) {
    const base = tunnelUrl || (port ? `http://localhost:${port}` : null);
    if (!base) return null;
    return base + '/dl/' + token;
  }

  let htmlCache = getWebappHtml(config.botUsername || '');

  // ─── Auth: Telegram Login Widget ───

  /**
   * Validate Telegram Login Widget data.
   * https://core.telegram.org/widgets/login#checking-authorization
   */
  function validateTelegramLogin(data) {
    if (!data || !data.hash) return { valid: false, error: 'Missing hash' };

    const { hash, ...rest } = data;
    // Build check string: sorted key=value pairs
    const checkString = Object.keys(rest).sort()
      .map(k => `${k}=${rest[k]}`).join('\n');

    // secret = SHA256(bot_token)
    const secret = createHash('sha256').update(config.botToken).digest();
    const computed = createHmac('sha256', secret).update(checkString).digest('hex');

    if (computed !== hash) return { valid: false, error: 'Invalid hash' };

    // Check auth_date not too old (1 hour)
    const authDate = parseInt(data.auth_date, 10);
    if (authDate && (Date.now() / 1000 - authDate) > 3600) {
      return { valid: false, error: 'Auth data expired' };
    }

    // Check authorized user
    const allowed = config.allowedUsername.replace(/^@/, '').toLowerCase();
    if (data.username?.toLowerCase() !== allowed) {
      return { valid: false, error: 'Unauthorized user' };
    }

    return { valid: true, user: data };
  }

  /** Drop every token belonging to one login. Used on logout and on reuse detection. */
  function revokeFamily(familyId) {
    for (const [t, e] of tokens) if (e.familyId === familyId) tokens.delete(t);
    for (const [t, e] of refreshTokens) if (e.familyId === familyId) refreshTokens.delete(t);
  }

  /**
   * Mint an access/refresh pair. `familyId` continues an existing login (on
   * rotation); omitting it starts a new one.
   *
   * @returns {{ token: string, refreshToken: string, expiresIn: number }}
   */
  function createSession(username, familyId = randomBytes(16).toString('hex')) {
    const now = Date.now();
    const token = randomBytes(32).toString('hex');
    const refreshToken = randomBytes(32).toString('hex');
    tokens.set(token, { username, familyId, expiresAt: now + ACCESS_TTL_MS });
    refreshTokens.set(refreshToken, { username, familyId, expiresAt: now + REFRESH_TTL_MS, used: false });
    return { token, refreshToken, expiresIn: Math.floor(ACCESS_TTL_MS / 1000) };
  }

  /** Back-compat shim: callers that only need an access token. */
  function createToken(username) { return createSession(username).token; }

  /**
   * Exchange a refresh token for a fresh pair, rotating it.
   *
   * @returns {{ ok: true, token, refreshToken, expiresIn } | { ok: false, error: string }}
   */
  function rotateRefresh(rt) {
    const entry = rt && refreshTokens.get(rt);
    if (!entry) return { ok: false, error: 'Invalid refresh token' };
    if (entry.used) {
      // Replay of an already-rotated token — assume the token leaked and cut
      // the whole login, forcing a real re-auth.
      revokeFamily(entry.familyId);
      return { ok: false, error: 'Refresh token reused' };
    }
    if (Date.now() >= entry.expiresAt) {
      refreshTokens.delete(rt);
      return { ok: false, error: 'Refresh token expired' };
    }
    // Kept (not deleted) with used=true so a later replay is distinguishable
    // from an unknown token and can trip the revocation above. Swept once it
    // passes expiresAt.
    entry.used = true;
    return { ok: true, ...createSession(entry.username, entry.familyId) };
  }

  /** Spent refresh tokens are retained for replay detection; drop them at expiry. */
  function sweepTokens() {
    const now = Date.now();
    for (const [t, e] of tokens) if (now >= e.expiresAt) tokens.delete(t);
    for (const [t, e] of refreshTokens) if (now >= e.expiresAt) refreshTokens.delete(t);
  }
  const tokenSweepTimer = setInterval(sweepTokens, 60 * 60 * 1000);
  if (tokenSweepTimer.unref) tokenSweepTimer.unref();

  function checkAccess(tok) {
    const entry = tok && tokens.get(tok);
    if (!entry) return false;
    if (Date.now() >= entry.expiresAt) { tokens.delete(tok); return false; }
    return true;
  }

  // ─── Forward cookie ───
  //
  // A forward lives on its own hostname, so the browser has no access token for
  // it: localStorage belongs to Crundi's origin, not to myapp.crundi.example.com.
  // A cookie scoped to the parent domain is the only thing that travels there.
  //
  // Deliberately a SEPARATE credential from the access token, and one the API
  // never looks at. validateToken reads the Authorization header and the token
  // query parameter and nothing else, so this cookie cannot be used to make an
  // authenticated Crundi API call from a forwarded page — which matters, since
  // those pages are running code we do not control.
  const forwardTokens = new Map();   // token -> expiresAt
  const FORWARD_TTL_MS = 7 * 24 * 60 * 60 * 1000;

  function mintForwardToken() {
    const t = randomBytes(32).toString('hex');
    forwardTokens.set(t, Date.now() + FORWARD_TTL_MS);
    // Cheap sweep; this map only grows on sign-in.
    if (forwardTokens.size > 200) {
      const now = Date.now();
      for (const [k, exp] of forwardTokens) if (exp <= now) forwardTokens.delete(k);
    }
    return t;
  }

  /** Attach the forward cookie to a successful sign-in response. */
  function setForwardCookie(res) {
    const base = forwards.baseDomain();
    if (!base) return;                     // no forward domain configured
    const token = mintForwardToken();
    const secure = tls.enabled() ? '; Secure' : '';
    res.setHeader('Set-Cookie',
      `crundi_fwd=${token}; Domain=.${base}; Path=/; Max-Age=${Math.floor(FORWARD_TTL_MS / 1000)}`
      + `; HttpOnly; SameSite=Lax${secure}`);
  }

  function hasForwardCookie(req) {
    const raw = req.headers.cookie;
    if (!raw) return false;
    const m = /(?:^|;\s*)crundi_fwd=([a-f0-9]+)/.exec(raw);
    if (!m) return false;
    const exp = forwardTokens.get(m[1]);
    if (!exp) return false;
    if (Date.now() >= exp) { forwardTokens.delete(m[1]); return false; }
    return true;
  }

  function validateToken(req) {
    // Authorization header (normal API calls)
    const auth = req.headers['authorization'];
    if (auth?.startsWith('Bearer ') && checkAccess(auth.slice(7))) return true;
    // Query param (WebSocket/SSE, which can't set headers). Long-lived sockets
    // are only authorized at connect; the client reconnects with a fresh token.
    const url = new URL(req.url, 'http://localhost');
    return checkAccess(url.searchParams.get('token'));
  }

  function extractToken(req) {
    const auth = req.headers['authorization'];
    if (auth?.startsWith('Bearer ')) return auth.slice(7);
    const url = new URL(req.url, 'http://localhost');
    return url.searchParams.get('token');
  }

  // ─── SSE ───
  const sseClients = new Set();

  // ─── Claude agent state (driven by Claude Code lifecycle hooks) ───
  // terminalId → 'working' | 'needs-input' | 'idle'. Liveness is the source of
  // truth: states are reconciled against the live terminal list (see
  // broadcastState), so a crash/force-close that never fires a Stop hook can't
  // leave a stale state.
  const agentStates = new Map();
  // Last event timestamp applied per terminal, so out-of-order hook deliveries
  // (independent short-lived processes under load) can't let a stale state win.
  const agentStateTs = new Map();
  // Per-event Telegram notification policy: 'always' | 'away' | 'never'.
  //   always → notify on every occurrence
  //   away   → notify only when no client is focused (see anyClientPresent)
  //   never  → don't notify
  // Persisted in .crundi-state.json under `notifyPrefs`; migrated from the old
  // boolean notifyAgentStatus and the interim notifyFinished/notifyNeedsInput.
  const NOTIFY_MODES = ['always', 'away', 'never'];
  const NOTIFY_DEFAULTS = {
    finished: 'away', needsInput: 'away',
    kanbanTask: 'never', kanbanSubtask: 'never',
    scheduleRun: 'away',
    serviceDown: 'always', serviceUp: 'away',
    mindmapAdd: 'never', mindmapDelete: 'never',
    browserLaunch: 'never', browserStop: 'never',
    secretRequest: 'always',
  };
  const notifyPrefs = { ...NOTIFY_DEFAULTS };
  try {
    const sf = join(config.dataDir, '.crundi-state.json');
    if (existsSync(sf)) {
      const st = JSON.parse(readFileSync(sf, 'utf-8')) || {};
      const saved = (st.notifyPrefs && typeof st.notifyPrefs === 'object') ? st.notifyPrefs : {};
      for (const k of Object.keys(NOTIFY_DEFAULTS)) {
        if (NOTIFY_MODES.includes(saved[k])) notifyPrefs[k] = saved[k];
      }
      // Migrate interim flat keys, then the original boolean (off → never).
      if (NOTIFY_MODES.includes(st.notifyFinished)) notifyPrefs.finished = st.notifyFinished;
      if (NOTIFY_MODES.includes(st.notifyNeedsInput)) notifyPrefs.needsInput = st.notifyNeedsInput;
      if (st.notifyAgentStatus === false && !st.notifyPrefs && !('notifyFinished' in st)) {
        notifyPrefs.finished = 'never'; notifyPrefs.needsInput = 'never';
      }
    }
  } catch { /* defaults */ }

  function persistNotifyPrefs() {
    try {
      const sf = join(config.dataDir, '.crundi-state.json');
      let st = {};
      try { if (existsSync(sf)) st = JSON.parse(readFileSync(sf, 'utf-8')) || {}; } catch { st = {}; }
      st.notifyPrefs = { ...notifyPrefs };
      delete st.notifyAgentStatus; delete st.notifyFinished; delete st.notifyNeedsInput; // superseded
      writeFileSync(sf, JSON.stringify(st));
    } catch { /* non-fatal */ }
  }

  // WebSocket connections that have reported the user is actively at the window
  // (window focused AND page visible). Used to skip the agent-status Telegram
  // ping when the user is already looking — they don't need a phone alert. Each
  // entry carries an expiry: the client refreshes it on a heartbeat, so a
  // silently-dropped socket's presence lapses instead of muting alerts forever.
  // A window left open on a LOCKED PC reports not-present (the OS lock drops
  // window focus → document.hasFocus() is false), so those still get the ping.
  const presentClients = new Map(); // ws → expiry epoch ms
  // Generous TTL vs the client's heartbeat (~25s): during a busy session the
  // client main thread janks (timers slip) and the WS flaps, so heartbeats get
  // delayed/dropped. A tight TTL would expire presence and leak "away" pings
  // while the user is actually here. ~6 missed beats of slack avoids that.
  const PRESENCE_TTL = 150_000;
  function anyClientPresent() {
    const now = Date.now();
    for (const [ws, exp] of presentClients) {
      if (ws.readyState === 1 && exp > now) return true;
      if (ws.readyState !== 1) presentClients.delete(ws); // tidy dead sockets
    }
    return false;
  }

  // Telegram's hard limit. MarkdownV2 escaping ADDS characters, so a body that
  // fit before conversion can overflow after it.
  const TG_HARD_MAX = 4096;

  // telegramify-markdown double-escapes inside TABLE CELLS: it emits two
  // backslashes before a reserved character where it means one. Telegram
  // consumes backslashes in pairs, so the character is left bare and the whole
  // message is rejected with "Character '-' is reserved". Every other
  // construct escapes correctly; this is upstream and still in 1.3.3, the
  // current release.
  //
  // An EVEN run of backslashes before a reserved character is always wrong -
  // there is no valid MarkdownV2 in which it renders - so dropping one to make
  // the run odd is a safe repair, while a correct odd run is left alone.
  const TG_RESERVED = '_*[]()~`>#+-=|{}.!';
  const TG_ESCAPE_RUN = /(\\+)([_*[\]()~`>#+\-=|{}.!])/g;

  function repairDoubleEscapes(s) {
    return s.replace(TG_ESCAPE_RUN, (m, slashes, ch) =>
      (slashes.length % 2 === 0 ? slashes.slice(1) : slashes) + ch);
  }

  function toTelegramMarkdown(text) {
    let src = String(text);
    for (let i = 0; i < 4; i++) {
      const out = repairDoubleEscapes(telegramify(src, 'escape'));
      if (out.length <= TG_HARD_MAX) return out;
      src = src.slice(0, Math.floor(src.length * 0.7)) + '\n\n[…truncated, open Crundi for the rest]';
    }
    return repairDoubleEscapes(telegramify(src.slice(0, 1500), 'escape'));
  }

  // ─── Notification channels ───
  // Registered once here; notifyEvent fans out to whichever are enabled. Adding
  // another channel later means writing its send(), not touching call sites.
  channels.register({
    id: 'telegram',
    label: 'Telegram',
    enabledByDefault: true,
    available: () => !!(bot && getChatId && getChatId()),
    unavailableReason: () => (!bot
      ? 'No Telegram bot is configured.'
      : 'Send /start to your bot once so it knows where to reach you.'),
    describe: () => 'Messages to your Telegram chat.',
    send: async (text, meta = {}) => {
      const chatId = getChatId ? getChatId() : null;
      if (!chatId || !bot) return false;
      // Claude answers in markdown, which Telegram shows literally unless we
      // translate it into its own dialect. That parser is strict — one stray
      // character rejects the whole message — so a failure falls back to the
      // plain text rather than costing the user the notification.
      if (meta.markdown) {
        try {
          await bot.api.sendMessage(chatId, toTelegramMarkdown(text), { parse_mode: 'MarkdownV2' });
          return true;
        } catch (err) {
          console.warn('[channels] Telegram rejected the formatted message, sending plain:', err.message);
        }
      }
      await bot.api.sendMessage(chatId, text);
      return true;
    },
  });

  // The desktop app cannot receive Web Push: Electron's Chromium has no push
  // service behind it, so PushManager.subscribe never succeeds there. It does
  // hold a live socket to this server though, so the notification is simply
  // sent down that and shown natively by the app.
  channels.register({
    id: 'desktop',
    label: 'Desktop app',
    enabledByDefault: true,
    available: () => browserMod.hostSupports('notify'),
    unavailableReason: () => (browserMod.hasNativeHost()
      ? 'The attached desktop app is too old to show notifications. Update it.'
      : 'No Crundi desktop app is attached to this server.'),
    describe: () => 'Native notifications on the machine running the desktop app.',
    send: (text, meta = {}) => browserMod.notifyHost({
      title: meta.title || 'Crundi',
      body: String(text).slice(0, 400),
    }),
  });

  channels.register({
    id: 'webpush',
    label: 'Browser notifications',
    enabledByDefault: false,
    available: () => webPush.subscriptions().length > 0,
    unavailableReason: () => 'No browser has been allowed to receive notifications yet.',
    describe: () => {
      const n = webPush.subscriptions().length;
      return n ? `${n} browser${n === 1 ? '' : 's'} subscribed.`
        : 'For browsers. The desktop app cannot do Web Push - use the Desktop app channel.';
    },
    send: (text, meta) => webPush.send(text, meta),
  });

  // Single entry point for every Telegram notification. Honors the per-event
  // policy ('always' | 'away' | 'never') and the presence gate for 'away'.
  function notifyEvent(key, message, meta = {}) {
    const mode = notifyPrefs[key];
    if (mode !== 'always' && mode !== 'away') return;   // 'never' / unknown
    if (mode === 'away' && anyClientPresent()) return;  // user is already here
    // Fire and forget: a slow or dead channel must not hold up whatever was
    // being notified about.
    channels.deliver(message, { tag: key, ...meta }).catch(() => { /* non-fatal */ });
  }

  // Detect service start / crash-stop transitions, independent of any connected
  // UI (broadcastState early-returns when no SSE clients, but alerts matter most
  // when the user is away). Polled on an interval from start().
  const prevServiceStatus = new Map(); // key → last seen status
  function checkServiceTransitions() {
    let list;
    try { list = getAllServiceStatus(); } catch { return; }
    const seen = new Set();
    for (const s of list) {
      seen.add(s.key);
      const prev = prevServiceStatus.get(s.key);
      prevServiceStatus.set(s.key, s.status);
      if (prev === undefined || prev === s.status) continue; // baseline / no change
      const name = s.name || s.key;
      const proj = s.alias ? ` (${s.alias})` : '';
      if (s.status === 'running' && prev !== 'running') {
        notifyEvent('serviceUp', `▶️ Service "${name}"${proj} started.`);
      } else if (prev === 'running' && s.status !== 'running') {
        notifyEvent('serviceDown', `⏹️ Service "${name}"${proj} ${s.status === 'error' ? 'crashed' : 'stopped'}.`);
      }
    }
    for (const k of [...prevServiceStatus.keys()]) if (!seen.has(k)) prevServiceStatus.delete(k);
  }

  // Apply a hook-reported state for a terminal; notify on transitions to
  // done(idle)/needs-input. Unknown terminals are ignored.
  // Telegram's hard cap is 4096; leave room for the header and the notice.
  const TG_OUTPUT_MAX = 3500;

  /**
   * What to say when a session goes idle.
   *
   * For a CHAT session we can do better than "finished": the turn's final
   * assistant message is the answer itself, which is usually the whole reason
   * you wanted telling. Terminal cells are a PTY — there is no clean "final
   * message" to lift out of a screen buffer, so they keep the plain line.
   */
  function finishedMessage(term, name, proj) {
    if (term && term.kind === 'ui' && claudeUi && claudeUi.lastTurnOutput) {
      try {
        const out = claudeUi.lastTurnOutput(term.id);
        if (out) {
          const body = out.length > TG_OUTPUT_MAX
            ? out.slice(0, TG_OUTPUT_MAX) + '\n\n[…truncated, open Crundi for the rest]'
            : out;
          // Claude writes markdown, so say so and let each channel decide what
          // to do with it - Telegram renders it, plainer channels do not.
          return { text: `✅ ${name}${proj}\n\n${body}`, markdown: true };
        }
      } catch { /* fall through to the plain line */ }
    }
    // Nothing was said this turn (interrupted, or pure tool work) — the plain
    // line is honest; echoing an older message would read as a fresh answer.
    return { text: `✅ ${name} finished${proj}.`, markdown: false };
  }

  function handleAgentState(tid, state, ts = 0) {
    // Terminal cells report via the lifecycle hook; UI (chat) cells report
    // straight off the stream-json message flow. Both land here so the status
    // dots and Telegram pings behave identically for either kind.
    const term = (claudeTerminals ? claudeTerminals.list().find(t => t.id === tid) : null)
      || (claudeUi ? claudeUi.list().find(s => s.id === tid) : null);
    if (!term) return;
    // Hook events come from independent, short-lived processes with no delivery
    // ordering. Under load (busy session + running services), Node startup jitter
    // can make a stale 'idle' arrive after a fresh 'working' and wrongly pin the
    // dot to idle. Ignore any report older than the last one applied here.
    const lastTs = agentStateTs.get(tid) || 0;
    if (ts && ts < lastTs) return;
    if (ts) agentStateTs.set(tid, ts);
    const prev = agentStates.get(tid);
    agentStates.set(tid, state);
    if (state === 'working' || state === 'needs-input') limitWarmer.markActivity();
    // working -> idle is a turn ending. Edge-triggered off `prev` so a repeated
    // 'idle' report (hooks fire from independent processes) cannot fire twice.
    if (state === 'idle' && prev === 'working' && term.project) {
      chatSchedule.onTurnEnd(term.project);
    }
    if (state !== prev && (state === 'idle' || state === 'needs-input')) {
      const name = term.title || 'Claude';
      const proj = term.project ? ` (${term.project})` : '';
      if (state === 'needs-input') notifyEvent('needsInput', `⏳ ${name} needs your input${proj}.`);
      else {
        const fin = finishedMessage(term, name, proj);
        notifyEvent('finished', fin.text, { markdown: fin.markdown });
      }
    }
    broadcastState();
  }

  // Pre-start the rolling 5-hour window while idle (opt-in). handleAgentState is
  // the single choke point every Claude session's activity passes through, so it
  // is the honest place to answer "has any real work happened lately?".
  // Constructed BEFORE the state subscription below: handleAgentState reads this
  // binding, and a session changing state in between would hit the const's
  // temporal dead zone.
  const limitWarmer = createLimitWarmer({
    getUsage: (opts) => usage.getUsage(opts || {}),
    isBusy: () => [...agentStates.values()].some(v => v === 'working' || v === 'needs-input'),
  });

  // Deferred chat messages ("send this when the turn ends / the window resets").
  const chatSchedule = createChatSchedule({
    claudeUi,
    getLatestUsage: () => usage.getLatestStored(),
  });

  // Chat sessions push their agent state as it changes. These arrive in-order
  // from a single process, so no timestamp reordering guard is needed.
  if (claudeUi) claudeUi.onAnyStateChange((id, state) => handleAgentState(id, state, Date.now()));

  function broadcastSSE(event, data) {
    const payload = `event: ${event}\ndata: ${JSON.stringify(data)}\n\n`;
    for (const client of sseClients) {
      try { client.res.write(payload); } catch { sseClients.delete(client); }
    }
  }

  function broadcastState() {
    if (!sseClients.size) return;
    broadcastSSE('state', buildState());
  }

  /**
   * The same payload the SSE 'state' event carries.
   *
   * Also served over HTTP, because a browser that has just loaded knows nothing
   * until the first push arrives - it renders an empty workbench, and someone
   * looking at a project whose chat is already running quite reasonably starts
   * a second one. Built in one place so the two can never disagree.
   */
  function buildState() {
    const projects = listProjects();
    const services = getAllServiceStatus().map(s => ({
      key: s.key, name: s.name, alias: s.alias, command: s.command,
      projectPath: s.projectPath, status: s.status, pid: s.pid,
      memory: s.memoryBytes,
      uptime: s.startedAt ? formatUptime(Date.now() - new Date(s.startedAt).getTime()) : null,
      tunnelPort: s.tunnelPort || 0,
      tunnelEnabled: !!s.tunnelEnabled,
      tunnelStatus: s.tunnel?.status || null,
      tunnelUrl: s.tunnel?.url || null,
    }));
    const liveTerms = claudeTerminals ? claudeTerminals.list() : [];
    // Chat sessions carry their own kind/agentState (derived from the message
    // stream), so they need no hook-state bookkeeping — only inclusion.
    const uiSessions = claudeUi ? claudeUi.list() : [];
    // Reconcile agent states against the live cells: drop states for cells that
    // are gone or no longer running (crash/force-close safety).
    const liveIds = new Set([...liveTerms.map(t => t.id), ...uiSessions.map(s => s.id)]);
    for (const k of [...agentStates.keys()]) if (!liveIds.has(k)) { agentStates.delete(k); agentStateTs.delete(k); }
    for (const t of liveTerms) if (t.status !== 'running') { agentStates.delete(t.id); agentStateTs.delete(t.id); }
    // A running terminal with no hook state yet is "idle" (alive → green).
    const terminals = [
      ...liveTerms.map(t => ({
        ...t, kind: 'terminal',
        agentState: t.status === 'running' ? (agentStates.get(t.id) || 'idle') : null,
      })),
      ...uiSessions,
    ];
    const userTerminals = terminalsMod.listTerminals();
    // Project aliases that have at least one enabled schedule (for the sidebar
    // "upcoming schedule" clock indicator).
    const scheduled = [...new Set(schedule.listSchedules().filter(s => s.enabled).map(s => String(s.project || '').toLowerCase()))];
    return {
      uptime: formatUptime(Date.now() - startedAt),
      projects,
      services,
      terminals,
      userTerminals: userTerminals.terminals || [],
      scheduled,
    };
  }

  // ─── Kanban live updates ───
  function broadcastKanban(projectAlias) {
    broadcastSSE('kanban', { project: String(projectAlias || '').toLowerCase() });
  }

  // ─── Mindmap live updates ───
  function broadcastMindmap() {
    broadcastSSE('mindmap', {});
  }

  // ─── Media live updates ───
  function broadcastMedia() {
    broadcastSSE('media', {});
  }

  /**
   * Public view of a media item, enriched with its link's live status so the UI
   * can render a "jump to source" button (and a "deleted" badge when the linked
   * task / subtask / node is gone). projectName is added for display.
   */
  function enrichMedia(item) {
    const proj = item.project ? getProject(item.project) : null;
    let linkStatus = 'none', linkLabel = null;
    const l = item.link;
    if (l) {
      linkStatus = 'deleted'; // assume gone until proven alive
      try {
        if (l.type === 'task' || l.type === 'todo') {
          const r = kanban.getTask(item.project, l.taskId, { includeDeleted: true });
          if (r.ok && !r.task.deleted) {
            if (l.type === 'task') { linkStatus = 'alive'; linkLabel = r.task.title; }
            else {
              const td = (r.task.todos || []).find(t => t.id === l.todoId);
              if (td && !td.deleted) { linkStatus = 'alive'; linkLabel = td.text; }
            }
          }
        } else if (l.type === 'node') {
          const n = mindmap.getNode(l.nodeId);
          if (n) { linkStatus = 'alive'; linkLabel = n.text; }
        }
      } catch { /* treat as deleted */ }
    }
    return {
      id: item.id, originalName: item.originalName, mime: item.mime, ext: item.ext,
      size: item.size, kind: item.kind, project: item.project,
      projectName: proj ? proj.name : (item.project || null),
      link: l || null, linkStatus, linkLabel, createdAt: item.createdAt,
      path: media.mediaFilePath(item), // absolute on-disk path (for drag-insert + copy-path)
    };
  }

  // ─── Claude usage (real account-wide limits) ───
  // Always fetch (so the usage HISTORY / graph keeps updating even with no
  // viewer); getUsage is cached (~1 real Anthropic call per minute). Push to SSE
  // clients only when some are connected.
  async function broadcastUsage(force = false) {
    try {
      const u = await usage.getUsage({ force });
      if (sseClients.size) broadcastSSE('usage', u);
    } catch { /* non-fatal */ }
  }

  /**
   * Attach to each task the mindmap nodes that link to it, so both the UI and
   * Claude (via kanban_list) can see a task's brainstorming nodes. The reverse
   * direction (a node's linked task details) is already on each mindmap node.
   */
  function enrichBoardWithMindmap(alias, board) {
    const tag = (t) => { t.mindmapNodes = mindmap.getNodesForTask(alias, t.id); };
    (board.tasks || []).forEach(tag);
    (board.deletedTasks || []).forEach(tag);
    return board;
  }

  // ─── Secret access requests (Claude → user approval) ───
  // Each entry: reqId → { id, secretId, secretName, projectAlias, reason, createdAt, fulfill, reject, timer }
  const pendingSecretRequests = new Map();
  const SECRET_REQUEST_TIMEOUT_MS = 3 * 60 * 1000; // 3 minutes per request

  /** Public-safe view of pending requests (never includes the secret value). */
  function publicSecretRequests() {
    return [...pendingSecretRequests.values()].map(r => ({
      id: r.id, secretId: r.secretId, secretName: r.secretName,
      project: r.projectAlias, reason: r.reason, createdAt: r.createdAt,
    }));
  }

  function broadcastSecretRequests() {
    broadcastSSE('secret-requests', { requests: publicSecretRequests() });
  }

  /**
   * Register a pending secret access request and return a Promise that resolves
   * once the user approves (with the correct PIN), denies, or it times out.
   * The value is never cached — every request is independent and needs its own
   * PIN entry.
   */
  function waitForSecretApproval({ secretId, secretName, projectAlias, reason }) {
    return new Promise((resolve) => {
      const id = randomBytes(8).toString('hex');
      const timer = setTimeout(() => {
        pendingSecretRequests.delete(id);
        broadcastSecretRequests();
        resolve({ ok: false, error: 'Request timed out — the user did not approve within 3 minutes.' });
      }, SECRET_REQUEST_TIMEOUT_MS);

      pendingSecretRequests.set(id, {
        id, secretId, secretName, projectAlias, reason,
        createdAt: new Date().toISOString(),
        timer,
        fulfill: (value) => {
          clearTimeout(timer);
          pendingSecretRequests.delete(id);
          broadcastSecretRequests();
          resolve({ ok: true, name: secretName, value });
        },
        reject: (error) => {
          clearTimeout(timer);
          pendingSecretRequests.delete(id);
          broadcastSecretRequests();
          resolve({ ok: false, error });
        },
      });

      broadcastSecretRequests();

      // Ping the user over Telegram so they know to come approve.
      const from = projectAlias ? ` (project: ${projectAlias})` : '';
      const why = reason ? `\nReason: ${reason}` : '';
      notifyEvent('secretRequest', `🔐 Claude is requesting access to secret "${secretName}"${from}.${why}\n\nOpen Crundi → Secrets to review and approve.`);
    });
  }

  // ─── Request Handling ───

  function readBody(req) {
    return new Promise((resolve) => {
      let body = '';
      req.on('data', (c) => { body += c; });
      req.on('end', () => resolve(body));
    });
  }

  function json(res, data, status = 200) {
    res.writeHead(status, {
      'Content-Type': 'application/json',
      'Access-Control-Allow-Origin': '*',
      'Access-Control-Allow-Headers': 'Content-Type, Authorization',
    });
    res.end(JSON.stringify(data));
  }

  function html(res, content) {
    res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
    res.end(content);
  }

  async function handleRequest(req, res) {
    const url = new URL(req.url, 'http://localhost');
    const path = url.pathname;

    // ─── Subdomain forwards ───
    // Checked first: this hostname belongs to somebody else's app, so none of
    // Crundi's own routing applies to it.
    // Path mode: /tunnel/<name>/… . Checked before anything else for the same
    // reason as the subdomain case — this request belongs to another app.
    const pathFwd = forwards.matchPath(req.url);
    if (pathFwd) {
      if (!pathFwd.forward.public && !validateToken(req) && !hasForwardCookie(req)) {
        res.writeHead(401, { 'Content-Type': 'text/plain; charset=utf-8' });
        res.end('This forward is private. Sign in to Crundi first, or recreate it as public.\n');
        return;
      }
      forwards.proxy(pathFwd.forward, req, res, pathFwd.upstreamPath);
      return;
    }

    const fwd = forwards.match(req.headers.host);
    if (fwd) {
      // Private by default. A quick tunnel is public by construction; a forward
      // is not, because "let me look at this on my phone" is the common case and
      // quietly publishing a dev database is not a default worth having.
      if (!fwd.public && !validateToken(req) && !hasForwardCookie(req)) {
        res.writeHead(401, { 'Content-Type': 'text/plain; charset=utf-8' });
        res.end('This forward is private. Sign in to Crundi first, or recreate it as public.\n');
        return;
      }
      forwards.proxy(fwd, req, res);
      return;
    }

    // CORS preflight
    if (req.method === 'OPTIONS') {
      res.writeHead(204, {
        'Access-Control-Allow-Origin': '*',
        'Access-Control-Allow-Methods': 'GET, POST, DELETE, OPTIONS',
        'Access-Control-Allow-Headers': 'Content-Type, Authorization',
      });
      res.end();
      return;
    }

    // Serve web app
    if (path === '/' && req.method === 'GET') {
      html(res, htmlCache);
      return;
    }

    // Serve vendor files (xterm.js, addon-fit.js, xterm.css)
    const vendorMatch = path.match(/^\/vendor\/([a-z0-9._-]+)$/i);
    if (vendorMatch && req.method === 'GET') {
      const filename = vendorMatch[1];
      const filePath = join(VENDOR_DIR, filename);
      if (!existsSync(filePath)) { res.writeHead(404); res.end('Not found'); return; }
      const ext = filename.substring(filename.lastIndexOf('.'));
      const mime = VENDOR_MIME[ext] || 'application/octet-stream';
      try {
        const st = statSync(filePath);
        res.writeHead(200, {
          'Content-Type': mime,
          'Content-Length': st.size,
          'Cache-Control': 'public, max-age=86400',
        });
        createReadStream(filePath).pipe(res);
      } catch {
        res.writeHead(500); res.end('Error reading file');
      }
      return;
    }

    // ─── PWA: manifest + service worker ───
    if (path === '/manifest.webmanifest' && req.method === 'GET') {
      res.writeHead(200, {
        'Content-Type': 'application/manifest+json; charset=utf-8',
        'Cache-Control': 'public, max-age=3600',
      });
      res.end(WEB_MANIFEST);
      return;
    }
    if (path === '/sw.js' && req.method === 'GET') {
      res.writeHead(200, {
        'Content-Type': 'application/javascript; charset=utf-8',
        'Cache-Control': 'no-cache',
        // Allow the SW to control the whole origin.
        'Service-Worker-Allowed': '/',
      });
      res.end(SERVICE_WORKER);
      return;
    }

    // Serve PWA / app icons from assets/
    const assetMatch = path.match(/^\/assets\/([a-z0-9._-]+)$/i);
    if (assetMatch && req.method === 'GET') {
      const filename = assetMatch[1];
      const filePath = join(ASSETS_DIR, filename);
      if (!existsSync(filePath)) { res.writeHead(404); res.end('Not found'); return; }
      const ext = filename.substring(filename.lastIndexOf('.')).toLowerCase();
      const mime = ASSET_MIME[ext] || 'application/octet-stream';
      try {
        const st = statSync(filePath);
        res.writeHead(200, {
          'Content-Type': mime,
          'Content-Length': st.size,
          'Cache-Control': 'public, max-age=86400',
        });
        createReadStream(filePath).pipe(res);
      } catch {
        res.writeHead(500); res.end('Error reading file');
      }
      return;
    }

    // Temporary file download — token in URL IS the auth
    const dlMatch = path.match(/^\/dl\/([a-f0-9]{64})$/);
    if (dlMatch && req.method === 'GET') {
      const entry = sharedFiles.get(dlMatch[1]);
      if (!entry || Date.now() >= entry.expiresAt) {
        res.writeHead(410, { 'Content-Type': 'text/plain' });
        res.end('This download link has expired.');
        return;
      }
      try {
        const st = statSync(entry.filePath);
        // ?inline=1 lets the in-app viewer render images/PDFs in place instead of
        // forcing a download.
        const inline = url.searchParams.get('inline') === '1';
        const ext = entry.filename.slice(entry.filename.lastIndexOf('.')).toLowerCase();
        const CT = {
          '.png': 'image/png', '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg', '.gif': 'image/gif',
          '.webp': 'image/webp', '.bmp': 'image/bmp', '.ico': 'image/x-icon', '.svg': 'image/svg+xml',
          '.avif': 'image/avif', '.pdf': 'application/pdf',
        };
        res.writeHead(200, {
          'Content-Type': (inline && CT[ext]) ? CT[ext] : 'application/octet-stream',
          'Content-Length': st.size,
          'Content-Disposition': (inline ? 'inline' : 'attachment') + `; filename="${entry.filename.replace(/"/g, '\\"')}"`,
          'Access-Control-Allow-Origin': '*',
        });
        createReadStream(entry.filePath).pipe(res);
      } catch {
        res.writeHead(404, { 'Content-Type': 'text/plain' });
        res.end('File no longer available.');
      }
      return;
    }

    // ─── Auth endpoint ───
    if (path === '/api/auth' && req.method === 'POST') {
      const body = await readBody(req);
      let parsed;
      try { parsed = JSON.parse(body); } catch { /* ignore */ }

      if (parsed?.telegramLogin) {
        const result = validateTelegramLogin(parsed.telegramLogin);
        if (!result.valid) return json(res, { ok: false, error: result.error }, 403);
        const session = createSession(result.user.username);
        return json(res, { ok: true, ...session, user: result.user });
      }

      return json(res, { ok: false, error: 'Missing auth data' }, 400);
    }

    // ─── Telegram Login redirect callback (data-auth-url flow) ───
    // The widget's JS callback (data-onauth) relies on a cross-site popup and
    // postMessage, which Microsoft Edge's tracking prevention blocks. The
    // redirect flow does a top-level navigation here with the auth fields as
    // query params and works in every browser. We validate, mint a token, and
    // bounce back to the app with the token in the URL fragment.
    if (path === '/auth/telegram/callback' && req.method === 'GET') {
      const data = Object.fromEntries(url.searchParams.entries());
      const result = validateTelegramLogin(data);
      if (!result.valid) {
        res.writeHead(302, { Location: '/?auth_error=' + encodeURIComponent(result.error) });
        res.end();
        return;
      }
      const session = createSession(result.user.username);
      res.writeHead(302, {
        Location: '/#token=' + session.token + '&refresh=' + session.refreshToken,
      });
      res.end();
      return;
    }

    // ─── Telegram WebApp auth (opened inside Telegram) ───
    if (path === '/api/auth/webapp' && req.method === 'POST') {
      const body = await readBody(req);
      let parsed;
      try { parsed = JSON.parse(body); } catch { /* ignore */ }
      const initData = parsed?.initData;
      if (!initData) return json(res, { ok: false, error: 'Missing initData' }, 400);

      // Parse initData query string
      const params = new URLSearchParams(initData);
      const hash = params.get('hash');
      if (!hash) return json(res, { ok: false, error: 'Missing hash' }, 403);

      // Build check string: sorted key=value pairs (excluding hash)
      const entries = [];
      for (const [k, v] of params) { if (k !== 'hash') entries.push(`${k}=${v}`); }
      entries.sort();
      const checkString = entries.join('\n');

      // WebApp secret = HMAC_SHA256("WebAppData", bot_token)
      const secretKey = createHmac('sha256', 'WebAppData').update(config.botToken).digest();
      const computed = createHmac('sha256', secretKey).update(checkString).digest('hex');

      if (computed !== hash) return json(res, { ok: false, error: 'Invalid hash' }, 403);

      // Extract user
      let user;
      try { user = JSON.parse(params.get('user')); } catch { /* ignore */ }
      const username = user?.username?.toLowerCase() || '';
      const allowed = config.allowedUsername.replace(/^@/, '').toLowerCase();
      if (username !== allowed) return json(res, { ok: false, error: 'Unauthorized user' }, 403);

      setForwardCookie(res);
      return json(res, { ok: true, ...createSession(username), user });
    }

    // ─── Local auth (Electron / localhost only) ───
    if (path === '/api/auth/local' && req.method === 'POST') {
      const remoteAddr = req.socket.remoteAddress || '';
      const isLocal = remoteAddr === '127.0.0.1' || remoteAddr === '::1' || remoteAddr === '::ffff:127.0.0.1';
      if (!isLocal) return json(res, { ok: false, error: 'Local auth only available from localhost' }, 403);
      const body = await readBody(req);
      let parsed;
      try { parsed = JSON.parse(body); } catch { /* ignore */ }
      if (parsed?.key !== internalApiKey) return json(res, { ok: false, error: 'Invalid key' }, 403);
      setForwardCookie(res);
      return json(res, { ok: true, ...createSession(config.allowedUsername || 'local') });
    }

    // ─── Password + TOTP login ───
    // The alternative to Telegram. Both factors are required: see config.js for
    // why a password alone is not offered as a login method.
    if (path === '/api/auth/password' && req.method === 'POST') {
      if (!authConfig.methods().password) {
        return json(res, { ok: false, error: 'Password login is not set up on this server' }, 400);
      }
      let parsed;
      try { parsed = JSON.parse(await readBody(req)); } catch { /* ignore */ }
      if (!authConfig.checkPasswordLogin(parsed?.password, parsed?.code)) {
        // Deliberately does not say WHICH was wrong: that would let someone
        // confirm a password using any six digits.
        await new Promise(r => setTimeout(r, 400 + Math.floor(Math.random() * 300)));
        console.warn(`[crundi] Failed password login from ${req.socket.remoteAddress || 'unknown'}`);
        return json(res, { ok: false, error: 'Incorrect password or code' }, 401);
      }
      console.log('[crundi] Password login succeeded');
      setForwardCookie(res);
      return json(res, { ok: true, ...createSession(config.localUsername) });
    }

    // Which sign-in methods this server offers. Read before login, so it is
    // deliberately outside the auth gate and says nothing a stranger could use.
    if (path === '/api/auth/methods' && req.method === 'GET') {
      const m = authConfig.methods();
      return json(res, {
        ok: true,
        telegram: m.telegram,
        password: m.password,
        setupRequired: !m.anyConfigured,
        botUsername: m.telegram ? (config.botUsername || '') : '',
      });
    }

    // First-run setup. Reachable unauthenticated ONLY while nothing is
    // configured; once a method exists this falls through to the auth gate and
    // becomes an ordinary authenticated settings change.
    if (path === '/api/auth/setup' && req.method === 'POST') {
      const open = authConfig.isOpen();
      if (!open && !validateToken(req)) return json(res, { error: 'Unauthorized' }, 401);
      let body;
      try { body = JSON.parse(await readBody(req)); } catch { body = null; }
      if (!body) return json(res, { ok: false, error: 'Bad request body' }, 400);

      if (body.method === 'password') {
        const r = authConfig.setPassword(body.password);
        if (!r.ok) return json(res, r, 400);
        console.log('[crundi] Password sign-in configured');
        // Hand back a session so whoever just set it up is not locked out by
        // their own change, and the enrolment secret for their authenticator.
        return json(res, { ...r, ...createSession(config.localUsername) });
      }

      if (body.method === 'telegram') {
        const botToken = String(body.botToken || '').trim();
        const username = String(body.username || '').replace(/^@/, '').trim();
        if (!botToken || !username) return json(res, { ok: false, error: 'Both a bot token and a username are needed' }, 400);
        const r = writeEnvKeys({ TELEGRAM_BOT_TOKEN: botToken, ALLOWED_USERNAME: username });
        if (!r.ok) return json(res, r, 500);
        console.log('[crundi] Telegram sign-in configured — restart required');
        return json(res, { ok: true, restartRequired: true });
      }

      return json(res, { ok: false, error: 'Unknown method' }, 400);
    }

    // ─── Refresh ───
    // Deliberately above the auth gate below: the access token is expected to
    // be expired here, so requiring one would defeat the purpose.
    if (path === '/api/auth/refresh' && req.method === 'POST') {
      const body = await readBody(req);
      let parsed;
      try { parsed = JSON.parse(body); } catch { /* ignore */ }
      const result = rotateRefresh(parsed?.refreshToken);
      return json(res, result, result.ok ? 200 : 401);
    }

    // ─── SSE endpoint ───
    if (path === '/api/events' && req.method === 'GET') {
      if (!validateToken(req)) {
        res.writeHead(401); res.end('Unauthorized');
        return;
      }
      res.writeHead(200, {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache',
        'Connection': 'keep-alive',
        'Access-Control-Allow-Origin': '*',
      });
      // Flush through Cloudflare buffering
      const padding = ':' + ' '.repeat(2048) + '\n';
      for (let i = 0; i < 24; i++) res.write(padding);
      const client = { res };
      sseClients.add(client);
      req.on('close', () => sseClients.delete(client));
      broadcastState();
      broadcastUsage();
      return;
    }

    // All other API routes require auth
    if (!path.startsWith('/api/')) return json(res, { error: 'Not found' }, 404);

    // ─── Setup mode ───
    // With no sign-in method configured the server is unauthenticated, so it
    // does exactly ONE thing: let someone configure one. Everything else is
    // refused, authenticated or not. Without this the "open until set up" state
    // would be a fully open shell on the host, which is not a state worth
    // having however briefly.
    if (authConfig.isOpen()) {
      const setupAllowed = path === '/api/auth/methods' || path === '/api/auth/setup';
      if (!setupAllowed) {
        return json(res, {
          error: 'Set up a sign-in method first — nothing else is available until then.',
          setupRequired: true,
        }, 403);
      }
    }

    // MCP call + hook status endpoints use their own X-Api-Key auth (not tokens)
    if (!authConfig.isOpen()
        && path !== '/api/mcp/call' && path !== '/api/terminal-status'
        && !validateToken(req)) return json(res, { error: 'Unauthorized' }, 401);

    // Claude Code lifecycle hooks report a terminal's agent state here.
    if (path === '/api/terminal-status' && req.method === 'POST') {
      if (req.headers['x-api-key'] !== internalApiKey) return json(res, { error: 'Invalid API key' }, 403);
      const body = JSON.parse(await readBody(req));
      const tid = String(body.terminal || '');
      const state = ['working', 'needs-input', 'idle'].includes(body.state) ? body.state : null;
      if (!tid || !state) return json(res, { ok: false, error: 'terminal and valid state required' }, 400);
      handleAgentState(tid, state, Number(body.ts) || 0);
      return json(res, { ok: true });
    }

    // ─── Import (from old Claude Telegram Bot) ───
    if (path === '/api/import/check' && req.method === 'GET') {
      const oldDir = getOldAppDataDir();
      return json(res, { available: !!oldDir && isFreshInstall() });
    }

    if (path === '/api/import' && req.method === 'POST') {
      const oldDir = getOldAppDataDir();
      if (!oldDir) return json(res, { ok: false, error: 'No old app data found' });
      const projResult = importFromOldData(oldDir);
      const svcResult = importServicesFromOldData(oldDir);
      broadcastState();
      return json(res, {
        ok: true,
        projects: projResult.ok ? projResult.imported : 0,
        services: svcResult.ok,
      });
    }

    // ─── Status ───
    if (path === '/api/status' && req.method === 'GET') {
      const services = getAllServiceStatus();
      const projects = listProjects();
      return json(res, {
        uptime: formatUptime(Date.now() - startedAt),
        totalServices: services.length,
        runningServices: services.filter(s => s.status === 'running').length,
        projectCount: projects.length,
        botConnected: !!config.botUsername,
        botUsername: config.botUsername || '',
      });
    }

    // ─── Projects ───
    if (path === '/api/projects/config' && req.method === 'GET') {
      return json(res, getProjectMode());
    }

    if (path === '/api/projects/check-path' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      const checkPath = body.path;
      if (!checkPath) return json(res, { exists: false });
      return json(res, { exists: existsSync(checkPath) });
    }

    if (path === '/api/projects' && req.method === 'GET') {
      return json(res, { projects: listProjects() });
    }

    // Persist the sidebar order (array of aliases).
    if (path === '/api/projects/reorder' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      const result = setProjectOrder(body.order || []);
      if (result.ok) broadcastState();
      return json(res, result);
    }

    if (path === '/api/projects' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      let { alias, path: projectPath, name, create } = body;
      const { mode, projectsDir } = getProjectMode();

      // Single mode: construct path from projectsDir + alias if no path given
      if (mode === 'single' && !projectPath && alias && projectsDir) {
        projectPath = join(projectsDir, alias);
        create = true;
      }

      if (!alias || !projectPath) return json(res, { ok: false, error: 'alias and path required' }, 400);
      const result = registerProject(alias.toLowerCase(), projectPath, name, { create: !!create });
      broadcastState();
      return json(res, result);
    }

    // Remove a project reference (keeps files on disk). Closes its terminal and
    // stops + deletes all services registered to it. Only registered projects
    // can be removed — auto-discovered ones would just reappear.
    const projDelMatch = path.match(/^\/api\/projects\/([^/]+)$/);
    if (projDelMatch && req.method === 'DELETE') {
      const alias = decodeURIComponent(projDelMatch[1]).toLowerCase();

      const removal = removeProject(alias);
      if (!removal.ok) return json(res, removal, 400);

      // Close any running Claude terminals / chat sessions for this project.
      if (claudeUi) {
        try { claudeUi.closeProject(alias); } catch { /* ignore */ }
      }
      if (claudeTerminals) {
        try { claudeTerminals.closeProject(alias); } catch { /* ignore */ }
      }

      // Stop and delete every service registered to this project.
      let servicesRemoved = 0;
      for (const svc of listRegisteredForProject(alias)) {
        try {
          stopService(svc.key);     // no-op if not running
          deleteService(svc.key);   // succeeds now that it's stopped
          servicesRemoved++;
        } catch { /* keep going */ }
      }

      setTimeout(broadcastState, 500);
      return json(res, { ok: true, servicesRemoved });
    }

    // ─── Kanban (project-scoped) ───
    if (path === '/api/kanban' && req.method === 'GET') {
      const project = url.searchParams.get('project');
      if (!project) return json(res, { ok: false, error: 'project is required' }, 400);
      const includeDeleted = url.searchParams.get('includeDeleted') === '1';
      return json(res, { ok: true, board: enrichBoardWithMindmap(project, kanban.getBoard(project, { includeDeleted })) });
    }

    if (path === '/api/kanban' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      const { action, project } = body;
      if (!project) return json(res, { ok: false, error: 'project is required' }, 400);
      let result;
      switch (action) {
        case 'addTask': result = kanban.addTask(project, { title: body.title, description: body.description, status: body.status, todos: body.todos }); break;
        case 'updateTask': result = kanban.updateTask(project, body.taskId, { title: body.title, description: body.description, status: body.status }); break;
        case 'moveTask': result = kanban.moveTask(project, body.taskId, body.status, body.index); break;
        case 'deleteTask': result = kanban.deleteTask(project, body.taskId); break;
        case 'restoreTask': result = kanban.restoreTask(project, body.taskId); break;
        case 'addTodo': result = kanban.addTodo(project, body.taskId, body.text); break;
        case 'updateTodo': result = kanban.updateTodo(project, body.taskId, body.todoId, { text: body.text, done: body.done }); break;
        case 'deleteTodo': result = kanban.deleteTodo(project, body.taskId, body.todoId); break;
        case 'restoreTodo': result = kanban.restoreTodo(project, body.taskId, body.todoId); break;
        default: return json(res, { ok: false, error: `Unknown kanban action: ${action}` }, 400);
      }
      if (result.ok) broadcastKanban(project);
      return json(res, result);
    }

    // ─── Secrets (global) ───
    if (path === '/api/secrets' && req.method === 'GET') {
      return json(res, { ok: true, secrets: secrets.listSecrets(), requests: publicSecretRequests() });
    }

    if (path === '/api/secrets' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      const { action } = body;
      switch (action) {
        case 'add': {
          const r = secrets.addSecret({ name: body.name, description: body.description, value: body.value, pin: body.pin });
          return json(res, r);
        }
        case 'updateMeta': {
          const r = secrets.updateSecretMeta(body.id, { name: body.name, description: body.description });
          return json(res, r);
        }
        case 'delete': {
          const r = secrets.deleteSecret(body.id);
          return json(res, r);
        }
        case 'reveal': {
          // The authenticated web user views a secret's value themselves.
          const r = secrets.decryptSecret(body.id, body.pin);
          return json(res, r);
        }
        case 'approve': {
          // Approve a pending Claude request: decrypt with the supplied PIN and,
          // on success, hand the value to the waiting MCP call.
          const reqEntry = pendingSecretRequests.get(body.reqId);
          if (!reqEntry) return json(res, { ok: false, error: 'Request no longer pending (it may have timed out)' });
          const dec = secrets.decryptSecret(reqEntry.secretId, body.pin);
          if (!dec.ok) return json(res, dec); // wrong PIN — keep request pending for retry
          reqEntry.fulfill(dec.value);
          return json(res, { ok: true });
        }
        case 'deny': {
          const reqEntry = pendingSecretRequests.get(body.reqId);
          if (reqEntry) reqEntry.reject('The user denied this secret access request.');
          return json(res, { ok: true });
        }
        default:
          return json(res, { ok: false, error: `Unknown secrets action: ${action}` }, 400);
      }
    }

    // ─── Mindmap (global) ───
    if (path === '/api/mindmap' && req.method === 'GET') {
      return json(res, { ok: true, mindmap: mindmap.getMindmap() });
    }

    if (path === '/api/mindmap' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      const { action } = body;
      let result;
      switch (action) {
        case 'addNode': result = mindmap.addNode({ text: body.text, parentId: body.parentId, note: body.note, notes: body.notes, project: body.project, taskId: body.taskId, todoId: body.todoId }); break;
        case 'updateNode': result = mindmap.updateNode(body.id, { text: body.text, note: body.note, notes: body.notes }); break;
        case 'addNote': result = mindmap.addNote(body.id, body.text); break;
        case 'removeNote': result = mindmap.removeNote(body.id, body.index); break;
        case 'moveNode': result = mindmap.moveNode(body.id, body.parentId, body.index); break;
        case 'linkNode': result = mindmap.linkNode(body.id, { project: body.project, taskId: body.taskId, todoId: body.todoId }); break;
        case 'scopeProject': result = mindmap.setNodeProject(body.id, body.project); break;
        case 'unlinkNode': result = mindmap.unlinkNode(body.id); break;
        case 'deleteNode': result = mindmap.deleteNode(body.id); break;
        default: return json(res, { ok: false, error: `Unknown mindmap action: ${action}` }, 400);
      }
      if (result.ok) broadcastMindmap();
      return json(res, result);
    }

    // ─── Scheduled tasks ───
    if (path === '/api/schedules' && req.method === 'GET') {
      const project = url.searchParams.get('project');
      return json(res, { ok: true, schedules: schedule.listSchedules(project || null), services: getAllServiceStatus().map(s => ({ key: s.key, name: s.name, alias: s.alias, status: s.status })) });
    }
    if (path === '/api/schedules' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      const { action } = body;
      let result;
      switch (action) {
        case 'add': result = schedule.addSchedule(body.schedule || {}); break;
        case 'update': result = schedule.updateSchedule(body.id, body.schedule || {}); break;
        case 'enable': result = schedule.setEnabled(body.id, body.enabled); break;
        case 'delete': result = schedule.deleteSchedule(body.id); break;
        default: return json(res, { ok: false, error: `Unknown schedule action: ${action}` }, 400);
      }
      if (result.ok) broadcastSSE('schedule', {});
      return json(res, result);
    }

    // ─── Claude usage (real, account-wide) ───
    if (path === '/api/usage' && req.method === 'GET') {
      const force = url.searchParams.get('force') === '1';
      return json(res, await usage.getUsage({ force }));
    }
    // Last stored sample — instant, no network call (shown on load).
    if (path === '/api/usage/latest' && req.method === 'GET') {
      return json(res, usage.getLatestStored());
    }
    // Time series for the chart. range = 5h | day | week | month | all
    if (path === '/api/usage/history' && req.method === 'GET') {
      const range = url.searchParams.get('range') || 'week';
      const spans = { '5h': 5 * 3600e3, day: 24 * 3600e3, week: 7 * 24 * 3600e3, month: 30 * 24 * 3600e3, all: 3650 * 24 * 3600e3 };
      const records = usage.getHistory(spans[range] || spans.week);
      return json(res, { ok: true, range, records });
    }

    // ─── Claude Terminals ───
    // What SSE would push, fetched once at startup so the first paint is right.
    if (path === '/api/state' && req.method === 'GET') {
      return json(res, buildState());
    }

    if (path === '/api/terminals' && req.method === 'GET') {
      return json(res, { terminals: claudeTerminals ? claudeTerminals.list() : [] });
    }

    // Create a new terminal for a project (multiple per project allowed).
    if (path === '/api/terminals/create' && req.method === 'POST') {
      if (!claudeTerminals) return json(res, { ok: false, error: 'Terminal manager not available' });
      const body = JSON.parse(await readBody(req));
      if (!body.project) return json(res, { ok: false, error: 'project is required' }, 400);
      const result = await claudeTerminals.create(body.project, body);
      // A terminal continues the same conversation somewhere we can't observe,
      // so the transcript we stored for UI replay is about to become a stale
      // prefix. Drop it — a later UI message rewrites it. Shell-only terminals
      // don't run Claude, so they leave it alone.
      if (result?.ok && !body.shell && claudeUi?.clearHistory) claudeUi.clearHistory(body.project);
      broadcastState();
      return json(res, result);
    }

    // Reorder a project's terminals: body { project, order: [id, …] }.
    if (path === '/api/terminals/reorder' && req.method === 'POST') {
      if (!claudeTerminals) return json(res, { ok: false, error: 'Terminal manager not available' });
      const body = JSON.parse(await readBody(req));
      const result = claudeTerminals.setOrder(body.project, body.order);
      broadcastState();
      return json(res, result);
    }

    // Per-terminal actions keyed by terminal id.
    const termMatch = path.match(/^\/api\/terminals\/([^/]+)\/(close|resize|rename)$/);
    if (termMatch && req.method === 'POST') {
      const termId = decodeURIComponent(termMatch[1]);
      const action = termMatch[2];
      if (!claudeTerminals) return json(res, { ok: false, error: 'Terminal manager not available' });

      if (action === 'close') {
        const result = claudeTerminals.close(termId);
        broadcastState();
        return json(res, result);
      }
      if (action === 'resize') {
        const body = JSON.parse(await readBody(req));
        const result = claudeTerminals.resize(termId, body.cols, body.rows);
        return json(res, result);
      }
      if (action === 'rename') {
        const body = JSON.parse(await readBody(req));
        const result = claudeTerminals.rename(termId, body.title);
        broadcastState();
        return json(res, result);
      }
    }

    // ─── Claude UI (chat) sessions ───
    // The chat counterpart to /api/terminals. Same {project,title,order} shape,
    // but the conversation itself streams over the WebSocket rather than bytes.
    if (path === '/api/ui-sessions' && req.method === 'GET') {
      return json(res, { sessions: claudeUi ? claudeUi.list() : [] });
    }

    // ─── Deferred chat messages ───
    if (path === '/api/chat-schedule' && req.method === 'GET') {
      const alias = String(url.searchParams.get('project') || '').toLowerCase();
      if (!alias) return json(res, { ok: false, error: 'A project is required' }, 400);
      return json(res, {
        ok: true,
        items: chatSchedule.list(alias),
        recent: chatSchedule.recent(alias),
      });
    }

    if (path === '/api/chat-schedule' && req.method === 'POST') {
      let body;
      try { body = JSON.parse(await readBody(req)); } catch { body = null; }
      if (!body) return json(res, { ok: false, error: 'Bad request body' }, 400);
      const alias = String(body.project || '').toLowerCase();
      if (!getProject(alias)) return json(res, { ok: false, error: 'Unknown project' }, 400);
      const r = chatSchedule.add({ project: alias, text: body.text, trigger: body.trigger });
      return json(res, r, r.ok ? 200 : 400);
    }

    const schedDel = path.match(/^\/api\/chat-schedule\/([a-f0-9]+)$/i);
    if (schedDel && req.method === 'DELETE') {
      const r = chatSchedule.remove(schedDel[1]);
      return json(res, r, r.ok ? 200 : 404);
    }

    // Prior Claude Code conversations for a project, newest first, so the UI can
    // offer a "resume" picker. These are on-disk transcripts, not live sessions.
    if (path === '/api/ui-sessions/resumable' && req.method === 'GET') {
      const alias = String(url.searchParams.get('project') || '').toLowerCase();
      const project = alias ? getProject(alias) : null;
      if (!project) return json(res, { ok: false, error: 'Unknown project' }, 400);
      return json(res, { ok: true, sessions: listResumable(project.path) });
    }

    // What would auto-continue pick up, and is loading it expensive? Answered
    // BEFORE spawning, because a stream-json resume always loads the whole
    // transcript and the CLI never offers its interactive "resume from summary"
    // choice over this protocol.
    if (path === '/api/ui-sessions/preflight' && req.method === 'GET') {
      const alias = String(url.searchParams.get('project') || '').toLowerCase();
      const project = alias ? getProject(alias) : null;
      if (!project) return json(res, { ok: false, error: 'Unknown project' }, 400);
      const latest = latestTranscript(project.path);
      return json(res, {
        ok: true,
        latest,
        heavy: isHeavyResume(latest),
        thresholds: { tokens: HEAVY_TOKENS, ageHours: HEAVY_AGE_HOURS },
      });
    }

    if (path === '/api/ui-sessions/create' && req.method === 'POST') {
      if (!claudeUi) return json(res, { ok: false, error: 'Chat manager not available' });
      const body = JSON.parse(await readBody(req));
      if (!body.project) return json(res, { ok: false, error: 'project is required' }, 400);
      const result = await claudeUi.create(body.project, body);
      broadcastState();
      return json(res, result);
    }

    if (path === '/api/ui-sessions/reorder' && req.method === 'POST') {
      if (!claudeUi) return json(res, { ok: false, error: 'Chat manager not available' });
      const body = JSON.parse(await readBody(req));
      const result = claudeUi.setOrder(body.project, body.order);
      broadcastState();
      return json(res, result);
    }

    const uiMatch = path.match(/^\/api\/ui-sessions\/([^/]+)\/(send|respond|interrupt|close|rename|permission-mode|model|history)$/);
    if (uiMatch) {
      const sid = decodeURIComponent(uiMatch[1]);
      const action = uiMatch[2];
      if (!claudeUi) return json(res, { ok: false, error: 'Chat manager not available' });

      if (action === 'history' && req.method === 'GET') {
        const h = claudeUi.history(sid);
        return h ? json(res, { ok: true, session: h }) : json(res, { ok: false, error: 'No such session' }, 404);
      }
      if (req.method !== 'POST') return json(res, { ok: false, error: 'Method not allowed' }, 405);

      if (action === 'interrupt') return json(res, claudeUi.interrupt(sid));
      if (action === 'close') {
        const result = claudeUi.close(sid);
        broadcastState();
        return json(res, result);
      }

      const body = JSON.parse(await readBody(req) || '{}');
      if (action === 'send') return json(res, claudeUi.sendMessage(sid, body.text));
      if (action === 'respond') return json(res, claudeUi.respond(sid, body));
      if (action === 'rename') {
        const result = claudeUi.rename(sid, body.title);
        broadcastState();
        return json(res, result);
      }
      if (action === 'permission-mode') {
        const result = claudeUi.setPermissionMode(sid, body.mode);
        broadcastState();
        return json(res, result);
      }
      if (action === 'model') {
        const result = claudeUi.setModel(sid, body.model);
        broadcastState();
        return json(res, result);
      }
    }

    // ─── Services ───
    if (path === '/api/services' && req.method === 'GET') {
      const all = getAllServiceStatus().map(s => ({
        key: s.key, name: s.name, alias: s.alias, command: s.command,
        projectPath: s.projectPath, status: s.status, pid: s.pid,
        memory: s.memoryBytes,
        uptime: s.startedAt ? formatUptime(Date.now() - new Date(s.startedAt).getTime()) : null,
        tunnelPort: s.tunnelPort || 0,
        tunnelEnabled: !!s.tunnelEnabled,
        tunnelStatus: s.tunnel?.status || null,
        tunnelUrl: s.tunnel?.url || null,
      }));
      return json(res, { services: all });
    }

    // Register a new service
    if (path === '/api/services' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      if (!body.name || !body.command) return json(res, { ok: false, error: 'name and command are required' }, 400);
      const alias = body.alias || currentProject || '';
      const result = registerService({
        alias,
        name: body.name,
        command: body.command,
        // cwd is optional in the UI — default to the project's directory
        cwd: body.cwd || getProject(alias)?.path || '',
        stopCommand: body.stopCommand || '',
        tunnelPort: body.tunnelPort || 0,
      });
      return json(res, result);
    }

    const svcMatch = path.match(/^\/api\/services\/([^/]+)\/(start|stop|restart|logs|delete|tunnel)$/);
    if (svcMatch) {
      const key = decodeURIComponent(svcMatch[1]);
      const action = svcMatch[2];
      if (action === 'logs' && req.method === 'GET') {
        return json(res, { logs: getServiceLogs(key, 100) });
      }
      if (req.method === 'POST') {
        let result;
        if (action === 'start') result = startService(key);
        else if (action === 'stop') result = stopService(key);
        else if (action === 'restart') result = restartService(key);
        else if (action === 'tunnel') {
          // Tunnel PORT and ON/OFF are two separate states. Body may carry
          // { port } (set the port) and/or { enabled } (toggle). Enabling is
          // blocked unless a port is set. The tunnel only actually runs while the
          // service is running; the enabled flag persists and auto-applies on start.
          const body = JSON.parse(await readBody(req));
          const reg = getRegistered(key);
          if (!reg) { result = { ok: false, error: 'Not registered' }; }
          else {
            const updates = {};
            if (body.port !== undefined) updates.tunnelPort = parseInt(body.port, 10) || 0;
            const newPort = updates.tunnelPort !== undefined ? updates.tunnelPort : (reg.tunnelPort || 0);
            let newEnabled = reg.tunnelEnabled !== undefined ? reg.tunnelEnabled : (reg.tunnelPort > 0);
            if (body.enabled !== undefined) {
              if (body.enabled && newPort <= 0) {
                result = { ok: false, error: 'Set a tunnel port before turning the tunnel on' };
              } else {
                newEnabled = !!body.enabled; updates.tunnelEnabled = newEnabled;
              }
            }
            // Clearing the port also turns the tunnel off.
            if (updates.tunnelPort === 0) { updates.tunnelEnabled = false; newEnabled = false; }
            if (!result) {
              const upd = updateRegistered(key, updates);
              if (!upd.ok) result = upd;
              else {
                const isRunning = getAllServiceStatus().some(s => s.key === key && s.status === 'running');
                stopTunnel(key); // also restarts cleanly on a port change
                if (newEnabled && newPort > 0 && isRunning) startTunnel(key, newPort);
                result = { ok: true, enabled: newEnabled, port: newPort };
              }
            }
          }
        }
        else if (action === 'delete') {
          // Always stop the service before removing it so we never orphan a
          // running process. stopService is a no-op if it isn't running.
          stopService(key);
          result = deleteService(key);
        }
        setTimeout(broadcastState, 500);
        return json(res, result || { ok: false, error: 'Unknown action' });
      }
    }

    // ─── Browsers ───
    if (path === '/api/browsers' && req.method === 'GET') {
      const alias = url.searchParams.get('alias') || '';
      return json(res, { browsers: browserMod.listBrowsers(alias) });
    }

    if (path === '/api/browsers/open' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      if (!body.url) return json(res, { ok: false, error: 'url is required' }, 400);
      const result = await browserMod.openBrowser({
        alias: body.alias || '',
        name: body.name || 'default',
        url: body.url,
        width: body.width || 1280,
        height: body.height || 720,
      });
      return json(res, result);
    }

    const browserCloseMatch = path.match(/^\/api\/browsers\/([^/]+)\/close$/);
    if (browserCloseMatch && req.method === 'POST') {
      const key = decodeURIComponent(browserCloseMatch[1]);
      const result = await browserMod.closeBrowser(key);
      return json(res, result);
    }

    const browserScreenshotMatch = path.match(/^\/api\/browsers\/([^/]+)\/screenshot$/);
    if (browserScreenshotMatch && req.method === 'GET') {
      const key = decodeURIComponent(browserScreenshotMatch[1]);
      const result = await browserMod.screenshotBrowser(key);
      return json(res, result);
    }

    const browserNavMatch = path.match(/^\/api\/browsers\/([^/]+)\/navigate$/);
    if (browserNavMatch && req.method === 'POST') {
      const key = decodeURIComponent(browserNavMatch[1]);
      const body = JSON.parse(await readBody(req));
      const result = await browserMod.navigateBrowser(key, body.url);
      return json(res, result);
    }

    // ─── User Terminals ───
    if (path === '/api/user-terminals' && req.method === 'GET') {
      const alias = url.searchParams.get('alias') || '';
      const result = terminalsMod.listTerminals(alias);
      return json(res, result);
    }

    // ─── Server Logs ───
    if (path === '/api/server-logs' && req.method === 'GET') {
      return json(res, { logs: serverLogs || [] });
    }

    // ─── Tunnel Status ───
    if (path === '/api/tunnel' && req.method === 'GET') {
      return json(res, { tunnels: getAllTunnelInfo(), tunnelUrl });
    }

    // ─── Settings ───

    // Write specific keys into .env without disturbing the rest. The settings
    // form rewrites the whole file from its own field list, which is fine when
    // it owns every field — this is for the setup path, which knows only two.
    function writeEnvKeys(pairs) {
      try {
        const dir = dirname(envPath);
        if (!existsSync(dir)) mkdirSync(dir, { recursive: true });
        let content = existsSync(envPath) ? readFileSync(envPath, 'utf-8') : '';
        for (const [key, value] of Object.entries(pairs)) {
          const line = `${key}=${value}`;
          const re = new RegExp(`^${key}=.*$`, 'm');
          content = re.test(content) ? content.replace(re, line) : (content.replace(/\n*$/, '\n') + line + '\n');
        }
        writeFileSync(envPath, content, 'utf-8');
        return { ok: true };
      } catch (err) { return { ok: false, error: err.message }; }
    }

    // ─── Subdomain forwards ───
    if (path === '/api/forwards' && req.method === 'GET') {
      return json(res, { ok: true, forwards: forwards.list(), domain: forwards.baseDomain() });
    }

    // What ways of exposing a port exist here, and what each costs. Read before
    // choosing, so the caller is not guessing.
    if (path === '/api/forwards/options' && req.method === 'GET') {
      return json(res, { ok: true, ...forwards.options() });
    }

    if (path === '/api/forwards' && req.method === 'POST') {
      let body;
      try { body = JSON.parse(await readBody(req)); } catch { body = null; }
      if (!body) return json(res, { ok: false, error: 'Bad request body' }, 400);
      // Path mode needs no domain at all — it hangs off whatever address Crundi
      // is already reached on.
      if (body.mode !== 'path' && !forwards.baseDomain()) {
        return json(res, {
          ok: false,
          error: 'No domain is configured for subdomain forwards. Use mode "path", or set TLS_DOMAIN / FORWARD_DOMAIN.',
        }, 400);
      }
      const r = forwards.add({
        name: body.name, port: body.port, mode: body.mode,
        isPublic: !!body.public, description: body.description,
      });
      return json(res, r, r.ok ? 200 : 400);
    }

    const fwdDel = path.match(/^\/api\/forwards\/([a-z0-9-]+)$/i);
    if (fwdDel && req.method === 'DELETE') {
      const r = forwards.remove(fwdDel[1]);
      return json(res, r, r.ok ? 200 : 404);
    }

    // ─── Notification channels ───
    if (path === '/api/notify/channels' && req.method === 'GET') {
      return json(res, { ok: true, channels: channels.list() });
    }

    if (path === '/api/notify/channels' && req.method === 'POST') {
      let body;
      try { body = JSON.parse(await readBody(req)); } catch { body = null; }
      if (!body?.id) return json(res, { ok: false, error: 'Which channel?' }, 400);
      const r = channels.setEnabled(body.id, !!body.enabled);
      return json(res, r, r.ok ? 200 : 400);
    }

    // ─── Server updates ───
    // The desktop app has electron-updater; a server does not, so it asks
    // GitHub and reports here. Applying is a separate, explicit call because it
    // restarts the server and takes every running session with it.
    if (path === '/api/update/status' && req.method === 'GET') {
      const force = /[?&]force=1/.test(req.url || '');
      if (force) await serverUpdate.check({ force: true }).catch(() => {});
      return json(res, { ok: true, update: serverUpdate.status(), log: serverUpdate.readLog() });
    }

    if (path === '/api/update/apply' && req.method === 'POST') {
      const r = await serverUpdate.apply();
      return json(res, r, r.ok ? 200 : 400);
    }

    // The browser needs this public key to subscribe. Public by design.
    if (path === '/api/push/key' && req.method === 'GET') {
      return json(res, { ok: true, key: webPush.publicKey() });
    }

    if (path === '/api/push/subscribe' && req.method === 'POST') {
      let body;
      try { body = JSON.parse(await readBody(req)); } catch { body = null; }
      const r = webPush.addSubscription(body?.subscription ? { ...body.subscription, label: body.label } : body);
      if (r.ok) {
        // Subscribing is the act of asking for these, so turn the channel on
        // rather than making it a second, easily-missed step.
        channels.setEnabled('webpush', true);
        console.log(`[crundi] Browser subscribed to push (${r.count} total)`);
      }
      return json(res, r, r.ok ? 200 : 400);
    }

    if (path === '/api/push/unsubscribe' && req.method === 'POST') {
      let body;
      try { body = JSON.parse(await readBody(req)); } catch { body = null; }
      return json(res, webPush.removeSubscription(body?.endpoint));
    }

    if (path === '/api/push/test' && req.method === 'POST') {
      const ok = await webPush.send('Notifications are working.', { title: 'Crundi', tag: 'test' });
      return json(res, ok
        ? { ok: true }
        : { ok: false, error: 'Nothing was delivered — no live subscription.' });
    }

    // ─── Sign-in methods (authenticated) ───
    if (path === '/api/auth/config' && req.method === 'GET') {
      return json(res, { ok: true, ...authConfig.status() });
    }

    if (path === '/api/auth/config' && req.method === 'POST') {
      let body;
      try { body = JSON.parse(await readBody(req)); } catch { body = null; }
      if (!body) return json(res, { ok: false, error: 'Bad request body' }, 400);

      // Setting a password always (re)mints the TOTP secret, so the response
      // carries the new one to enrol.
      if (body.action === 'set-password') {
        const r = authConfig.setPassword(body.password);
        return json(res, r, r.ok ? 200 : 400);
      }
      if (body.action === 'enable') {
        const r = body.method === 'telegram'
          ? authConfig.setTelegramEnabled(true)
          : authConfig.setPasswordEnabled(true);
        return json(res, r, r.ok ? 200 : 400);
      }
      // Refused when it would leave nothing to sign in with — the server must
      // never fall back to the open state once it has left it.
      if (body.action === 'disable') {
        const r = body.method === 'telegram'
          ? authConfig.setTelegramEnabled(false)
          : authConfig.setPasswordEnabled(false);
        return json(res, r, r.ok ? 200 : 400);
      }
      return json(res, { ok: false, error: 'Unknown action' }, 400);
    }

    if (path === '/api/settings' && req.method === 'GET') {
      try {
        const content = existsSync(envPath) ? readFileSync(envPath, 'utf-8') : '';
        const settings = {};
        const KEYS = ['TELEGRAM_BOT_TOKEN', 'ALLOWED_USERNAME', 'PROJECTS_DIR', 'WEB_PORT', 'CLOUDFLARE_TUNNEL_TOKEN', 'CLOUDFLARE_TUNNEL_URL', 'DATA_DIR'];
        for (const key of KEYS) {
          const m = content.match(new RegExp(`^${key}=(.*)$`, 'm'));
          settings[key] = m ? m[1].trim() : '';
        }
        const chatId = getChatId ? getChatId() : '';
        return json(res, { ok: true, settings, envPath, chatId: chatId ? String(chatId) : '', notifyPrefs, limitWarmup: limitWarmer.status() });
      } catch (err) { return json(res, { ok: false, error: err.message }); }
    }

    if (path === '/api/settings' && req.method === 'POST') {
      try {
        const body = JSON.parse(await readBody(req));
        // Only rewrite .env when env settings are actually supplied — this lets
        // lightweight callers (e.g. live notification-pref changes) POST without
        // wiping the env file with blanks.
        let restartRequired = false;
        if (body.settings) {
          const settings = body.settings;
          const KEYS = ['TELEGRAM_BOT_TOKEN', 'ALLOWED_USERNAME', 'PROJECTS_DIR', 'WEB_PORT', 'CLOUDFLARE_TUNNEL_TOKEN', 'CLOUDFLARE_TUNNEL_URL', 'DATA_DIR'];
          let content = '';
          for (const key of KEYS) {
            const val = settings[key] !== undefined ? settings[key] : '';
            content += `${key}=${val}\n`;
          }
          const dir = dirname(envPath);
          if (!existsSync(dir)) mkdirSync(dir, { recursive: true });
          writeFileSync(envPath, content, 'utf-8');
          restartRequired = true;
        }
        // Save chat ID to state file and update live
        if (body.chatId !== undefined) {
          const stateFile = join(config.dataDir, '.crundi-state.json');
          let state = {};
          try { if (existsSync(stateFile)) state = JSON.parse(readFileSync(stateFile, 'utf-8')); } catch {}
          const newId = body.chatId ? Number(body.chatId) : null;
          state.chatId = newId;
          writeFileSync(stateFile, JSON.stringify(state));
          if (setChatId) setChatId(newId);
        }
        // Live update (no restart): per-event notification policy.
        if (body.limitWarmup !== undefined) limitWarmer.setEnabled(!!body.limitWarmup);
        if (body.notifyPrefs && typeof body.notifyPrefs === 'object') {
          for (const k of Object.keys(NOTIFY_DEFAULTS)) {
            if (NOTIFY_MODES.includes(body.notifyPrefs[k])) notifyPrefs[k] = body.notifyPrefs[k];
          }
          persistNotifyPrefs();
        }
        return json(res, { ok: true, restartRequired });
      } catch (err) { return json(res, { ok: false, error: err.message }); }
    }

    // ─── Clipboard Image Upload ───

    if (path === '/api/clipboard/paste-image' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      const project = getProject(body.project);
      if (!project) return json(res, { ok: false, error: 'Project not found' }, 404);
      if (!body.data) return json(res, { ok: false, error: 'No image data' }, 400);
      try {
        const dir = ensureAttachmentsDir(project.path);
        const name = 'screenshot-' + Date.now() + '.png';
        const filePath = join(dir, name);
        writeFileSync(filePath, Buffer.from(body.data, 'base64'));
        return json(res, { ok: true, path: filePath, name });
      } catch (err) { return json(res, { ok: false, error: err.message }); }
    }

    // ─── Attachment Upload (file picker → crundi_attachments) ───
    if (path === '/api/attachments/upload' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      const project = getProject(body.project);
      if (!project) return json(res, { ok: false, error: 'Project not found' }, 404);
      if (!body.name || !body.data) return json(res, { ok: false, error: 'Missing name or data' }, 400);
      try {
        const dir = ensureAttachmentsDir(project.path);
        // unique, filesystem-safe name: <ts>-<sanitized original>
        const safe = basename(String(body.name)).replace(/[^\w.\-]+/g, '_').slice(-80) || 'file';
        const name = Date.now() + '-' + safe;
        const filePath = join(dir, name);
        const buf = Buffer.from(body.data, 'base64');
        writeFileSync(filePath, buf);
        return json(res, { ok: true, path: filePath, name, size: buf.length });
      } catch (err) { return json(res, { ok: false, error: err.message }); }
    }

    // ─── Resolve dropped file path ───

    if (path === '/api/resolve-file-path' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      const { name, size, project: projAlias } = body;
      if (!name) return json(res, { ok: false, error: 'name required' }, 400);
      const searchDirs = [];
      const proj = projAlias ? getProject(projAlias) : null;
      if (proj?.path) searchDirs.push(proj.path);
      const home = homedir();
      searchDirs.push(join(home, 'Desktop'), join(home, 'Downloads'), join(home, 'Documents'), home);
      for (const dir of searchDirs) {
        const candidate = join(dir, name);
        try {
          const st = statSync(candidate);
          if (st.isFile() && (size == null || st.size === size)) {
            return json(res, { ok: true, path: candidate });
          }
        } catch { /* not found */ }
      }
      // Shallow search (depth 1) in project dir
      if (proj?.path) {
        try {
          for (const entry of readdirSync(proj.path, { withFileTypes: true })) {
            if (entry.isDirectory() && !entry.name.startsWith('.') && entry.name !== 'node_modules') {
              const candidate = join(proj.path, entry.name, name);
              try {
                const st = statSync(candidate);
                if (st.isFile() && (size == null || st.size === size)) {
                  return json(res, { ok: true, path: candidate });
                }
              } catch { /* not found */ }
            }
          }
        } catch { /* ignore */ }
      }
      return json(res, { ok: true, path: null });
    }

    // ─── File Browser ───

    // Resolve a browse path: absolute paths are used as-is (lets the file
    // browser go above the project root); relative paths resolve under the
    // project. Used for read-only ops (list/read/download). Write & delete stay
    // jailed to the project (see those handlers).
    function resolveFsPath(project, p) {
      if (!p || p === '.') return project.path;
      return isAbsolute(p) ? resolve(p) : resolve(project.path, p);
    }
    function isInsideProject(project, fullPath) {
      // Normalise BOTH sides. project.path is whatever was registered, which may
      // use forward slashes on Windows, while resolve() always returns
      // backslashes — comparing them raw silently reports "outside the project"
      // for paths that are plainly inside it.
      const root = resolve(project.path);
      const p = resolve(fullPath);
      return p === root || p.startsWith(root + sep);
    }

    if (path === '/api/files/list' && req.method === 'GET') {
      const alias = url.searchParams.get('project');
      const project = getProject(alias);
      if (!project) return json(res, { ok: false, error: 'Project not found' }, 404);
      const fullPath = resolveFsPath(project, url.searchParams.get('dir') || '');
      if (!existsSync(fullPath)) return json(res, { ok: false, error: 'Directory not found' }, 404);
      try {
        const entries = readdirSync(fullPath, { withFileTypes: true })
          .map(e => {
            const type = e.isDirectory() ? 'dir' : 'file';
            let size = 0;
            if (type === 'file') { try { size = statSync(join(fullPath, e.name)).size; } catch {} }
            return { name: e.name, type, size, path: join(fullPath, e.name) };
          })
          .sort((a, b) => a.type === b.type ? a.name.localeCompare(b.name) : a.type === 'dir' ? -1 : 1);
        const parent = dirname(fullPath);
        // Clickable breadcrumb segments with their absolute paths (built here so
        // the client doesn't have to do OS-specific path math).
        const winPath = fullPath.includes('\\');
        const psep = winPath ? '\\' : '/';
        const segs = fullPath.split(/[\\/]+/).filter(Boolean);
        const crumbs = [];
        let acc = '';
        for (let i = 0; i < segs.length; i++) {
          acc = i === 0 ? (winPath ? segs[i] + psep : psep + segs[i]) : acc.replace(/[\\/]+$/, '') + psep + segs[i];
          crumbs.push({ name: segs[i], path: acc });
        }
        return json(res, {
          ok: true, entries, crumbs, path: fullPath, root: project.path,
          parent: parent === fullPath ? null : parent,
          inside: isInsideProject(project, fullPath),
        });
      } catch (err) { return json(res, { ok: false, error: err.message }); }
    }

    if (path === '/api/files/search' && req.method === 'GET') {
      const alias = url.searchParams.get('project');
      const q = (url.searchParams.get('q') || '').trim().toLowerCase();
      const project = getProject(alias);
      if (!project) return json(res, { ok: false, error: 'Project not found' }, 404);
      if (!q) return json(res, { ok: true, entries: [], truncated: false, gitignoreApplied: false });
      const CAP = 400;
      try {
        const root = project.path;
        let isGit = false;
        try { isGit = gitExec(root, ['rev-parse', '--is-inside-work-tree']).trim() === 'true'; } catch { isGit = false; }
        // Collect candidate {rel, type} entries (files + their ancestor folders).
        const seen = new Set(); // rel paths already added
        const items = []; // { rel, type }
        const addFile = (rel) => {
          rel = rel.replace(/\\/g, '/').replace(/\/+$/, '');
          if (!rel) return;
          const parts = rel.split('/');
          let acc = '';
          for (let i = 0; i < parts.length; i++) {
            acc = acc ? acc + '/' + parts[i] : parts[i];
            const type = i === parts.length - 1 ? 'file' : 'dir';
            if (!seen.has(acc)) { seen.add(acc); items.push({ rel: acc, type }); }
          }
        };
        if (isGit) {
          // Tracked + untracked, excluding everything .gitignore (and friends) ignore.
          const out = gitExec(root, ['ls-files', '--cached', '--others', '--exclude-standard']);
          for (const line of out.split('\n')) { if (line.trim()) addFile(line.trim()); }
        } else {
          // No git: plain recursive walk (skip .git only).
          const walk = (dir, rel) => {
            if (items.length > CAP * 20) return;
            let ents = [];
            try { ents = readdirSync(dir, { withFileTypes: true }); } catch { return; }
            for (const e of ents) {
              if (e.name === '.git') continue;
              const childRel = rel ? rel + '/' + e.name : e.name;
              if (e.isDirectory()) { if (!seen.has(childRel)) { seen.add(childRel); items.push({ rel: childRel, type: 'dir' }); } walk(join(dir, e.name), childRel); }
              else { if (!seen.has(childRel)) { seen.add(childRel); items.push({ rel: childRel, type: 'file' }); } }
            }
          };
          walk(root, '');
        }
        const matches = [];
        for (const it of items) {
          const base = it.rel.split('/').pop().toLowerCase();
          if (base.includes(q)) {
            matches.push({ name: it.rel.split('/').pop(), rel: it.rel, type: it.type, path: join(root, it.rel) });
            if (matches.length >= CAP) break;
          }
        }
        matches.sort((a, b) => a.type === b.type ? a.rel.localeCompare(b.rel) : a.type === 'dir' ? -1 : 1);
        return json(res, { ok: true, entries: matches, truncated: matches.length >= CAP, gitignoreApplied: isGit });
      } catch (err) { return json(res, { ok: false, error: err.message }); }
    }

    if (path === '/api/files/read' && req.method === 'GET') {
      const alias = url.searchParams.get('project');
      const relFile = url.searchParams.get('file') || '';
      const project = getProject(alias);
      if (!project) return json(res, { ok: false, error: 'Project not found' }, 404);
      const fullPath = resolveFsPath(project, relFile);
      if (!existsSync(fullPath)) return json(res, { ok: false, error: 'File not found' }, 404);
      try {
        const st = statSync(fullPath);
        if (st.size > 1024 * 1024) return json(res, { ok: false, error: 'File too large (>1MB)' });
        const content = readFileSync(fullPath, 'utf-8');
        return json(res, { ok: true, content, size: st.size });
      } catch (err) { return json(res, { ok: false, error: err.message }); }
    }

    if (path === '/api/files/write' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      const project = getProject(body.project);
      if (!project) return json(res, { ok: false, error: 'Project not found' }, 404);
      // Same helpers the READ side uses. Raw resolve() mishandles the absolute
      // paths the client actually sends when project.path is stored with
      // forward slashes, so saving failed outright; and a bare startsWith()
      // has no separator boundary, so a sibling directory sharing the project's
      // name as a prefix would pass.
      const fullPath = resolveFsPath(project, body.file || '');
      if (!isInsideProject(project, fullPath)) return json(res, { ok: false, error: 'Invalid path' }, 403);
      try {
        writeFileSync(fullPath, body.content || '', 'utf-8');
        return json(res, { ok: true });
      } catch (err) { return json(res, { ok: false, error: err.message }); }
    }

    if (path === '/api/files/download' && req.method === 'GET') {
      const alias = url.searchParams.get('project');
      const relFile = url.searchParams.get('file') || '';
      const project = getProject(alias);
      if (!project) { res.writeHead(404); res.end('Project not found'); return; }
      const fullPath = resolveFsPath(project, relFile);
      if (!existsSync(fullPath)) { res.writeHead(404); res.end('File not found'); return; }
      try {
        const st = statSync(fullPath);
        if (st.isDirectory()) { res.writeHead(400); res.end('Cannot download directory'); return; }
        const fname = basename(fullPath);
        res.writeHead(200, {
          'Content-Type': 'application/octet-stream',
          'Content-Disposition': 'attachment; filename="' + fname.replace(/"/g, '\\"') + '"',
          'Content-Length': st.size,
        });
        createReadStream(fullPath).pipe(res);
      } catch (err) { res.writeHead(500); res.end(err.message); }
      return;
    }

    if (path === '/api/files/download-link' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      const project = getProject(body.project);
      if (!project) return json(res, { ok: false, error: 'Project not found' });
      const fullPath = resolveFsPath(project, body.file || '');
      if (!existsSync(fullPath)) return json(res, { ok: false, error: 'File not found' });
      const tok = shareFile(fullPath, 5); // 5 min expiry
      const dlUrl = '/dl/' + tok;
      return json(res, { ok: true, url: dlUrl });
    }

    if (path === '/api/files/upload' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      const project = getProject(body.project);
      if (!project) return json(res, { ok: false, error: 'Project not found' }, 404);
      const relDir = body.dir || '.';
      const targetDir = resolveFsPath(project, relDir);
      if (!isInsideProject(project, targetDir)) return json(res, { ok: false, error: 'Invalid path' }, 403);
      if (!body.name || !body.data) return json(res, { ok: false, error: 'Missing name or data' }, 400);
      try {
        if (!existsSync(targetDir)) mkdirSync(targetDir, { recursive: true });
        const filePath = join(targetDir, basename(body.name));
        if (!isInsideProject(project, filePath)) return json(res, { ok: false, error: 'Invalid path' }, 403);
        const buf = Buffer.from(body.data, 'base64');
        writeFileSync(filePath, buf);
        return json(res, { ok: true, size: buf.length });
      } catch (err) { return json(res, { ok: false, error: err.message }); }
    }

    if (path === '/api/files/delete' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      const project = getProject(body.project);
      if (!project) return json(res, { ok: false, error: 'Project not found' }, 404);
      const fullPath = resolveFsPath(project, body.file || '');
      if (!isInsideProject(project, fullPath)) return json(res, { ok: false, error: 'Invalid path' }, 403);
      if (fullPath === resolve(project.path)) return json(res, { ok: false, error: 'Cannot delete project root' }, 403);
      if (!existsSync(fullPath)) return json(res, { ok: false, error: 'Not found' }, 404);
      try {
        const { rmSync } = await import('node:fs');
        rmSync(fullPath, { recursive: true, force: true });
        return json(res, { ok: true });
      } catch (err) { return json(res, { ok: false, error: err.message }); }
    }

    // ─── Media library ───

    // List media, enriched with link liveness. Filters: project, kind
    // (kanban/mindmap/unlinked) or an exact target (type + ids), and
    // includeGeneral (also show unlinked general items when scoped to a project).
    if (path === '/api/media/list' && req.method === 'GET') {
      const sp = url.searchParams;
      const project = sp.get('project') || null;
      let linkFilter = null;
      const kind = sp.get('kind');
      if (kind) linkFilter = { kind };
      else if (sp.get('linkType')) {
        const type = sp.get('linkType');
        if (type === 'task') linkFilter = { type: 'task', taskId: sp.get('taskId') };
        else if (type === 'todo') linkFilter = { type: 'todo', taskId: sp.get('taskId'), todoId: sp.get('todoId') };
        else if (type === 'node') linkFilter = { type: 'node', nodeId: sp.get('nodeId') };
      }
      const items = media.listMedia({
        project,
        includeGeneral: sp.get('includeGeneral') === '1',
        linkFilter,
      }).map(enrichMedia);
      return json(res, { ok: true, items });
    }

    // Upload media (base64). Optional project + link { type, ... }.
    if (path === '/api/media/upload' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      if (!body.name || !body.data) return json(res, { ok: false, error: 'Missing name or data' }, 400);
      let project = body.project || null;
      // A link to a kanban task/todo forces the item's project to that task's
      // project; a node link forces it to the node's effective project.
      const link = body.link || null;
      try {
        if (link && (link.type === 'task' || link.type === 'todo') && project) {
          const r = kanban.getTask(project, link.taskId, { includeDeleted: true });
          if (!r.ok) return json(res, { ok: false, error: 'Linked task not found' }, 404);
        } else if (link && link.type === 'node') {
          const n = mindmap.getNode(link.nodeId);
          if (!n) return json(res, { ok: false, error: 'Linked idea not found' }, 404);
          project = n.effectiveProject || null;
        }
        const buf = Buffer.from(body.data, 'base64');
        const r = media.addMediaFromBuffer({ name: body.name, buffer: buf, project, link });
        if (!r.ok) return json(res, r);
        broadcastMedia();
        return json(res, { ok: true, item: enrichMedia(r.item) });
      } catch (err) { return json(res, { ok: false, error: err.message }); }
    }

    if (path === '/api/media/delete' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      const r = media.deleteMedia(body.id);
      if (r.ok) broadcastMedia();
      return json(res, r);
    }

    // Serve a media file's bytes. Auth is the normal session token (header or
    // ?token= query — so <img>/<video>/<iframe> tags work). ?download=1 forces a
    // download; otherwise it's served inline for in-app preview.
    if (path.startsWith('/api/media/raw/') && req.method === 'GET') {
      const id = path.slice('/api/media/raw/'.length);
      const item = media.getMedia(id);
      const fp = item && media.mediaFilePath(item);
      if (!item || !fp || !existsSync(fp)) { res.writeHead(404); res.end('Not found'); return; }
      try {
        const st = statSync(fp);
        const download = url.searchParams.get('download') === '1';
        const fname = item.originalName.replace(/"/g, '\\"');
        res.writeHead(200, {
          'Content-Type': download ? 'application/octet-stream' : item.mime,
          'Content-Length': st.size,
          'Content-Disposition': (download ? 'attachment' : 'inline') + '; filename="' + fname + '"',
          'Cache-Control': 'private, max-age=3600',
          'Access-Control-Allow-Origin': '*',
        });
        createReadStream(fp).pipe(res);
      } catch (err) { res.writeHead(500); res.end(err.message); }
      return;
    }

    // ─── User Terminals (API for Terminals tab) ───

    if (path === '/api/terminals/spawn' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      const result = await terminalsMod.spawnTerminal(body.alias || 'default', body.alias || 'default', body.name, { command: body.command, cwd: body.cwd });
      broadcastState();
      return json(res, result);
    }

    if (path === '/api/terminals/input' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      const result = await terminalsMod.writeTerminal(body.alias || 'default', body.name, body.input);
      return json(res, result);
    }

    if (path === '/api/terminals/output' && req.method === 'GET') {
      const alias = url.searchParams.get('alias') || 'default';
      const name = url.searchParams.get('name');
      const result = await terminalsMod.getTerminalOutput(alias, name, {
        start: parseInt(url.searchParams.get('start') || '0', 10),
        end: url.searchParams.get('end') ? parseInt(url.searchParams.get('end'), 10) : undefined,
      });
      return json(res, result);
    }

    if (path === '/api/terminals/close' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      const result = await terminalsMod.closeTerminal(body.alias || 'default', body.name);
      broadcastState();
      return json(res, result);
    }

    // ─── Git ───

    function gitExec(cwd, args) {
      return execFileSync('git', args, { cwd, encoding: 'utf8', timeout: 15000, windowsHide: true, maxBuffer: 5 * 1024 * 1024 }).trimEnd();
    }

    if (path === '/api/git/info' && req.method === 'GET') {
      const alias = url.searchParams.get('project');
      const project = getProject(alias);
      if (!project) return json(res, { ok: false, error: 'Project not found' }, 404);
      try {
        try { gitExec(project.path, ['update-index', '--refresh']); } catch {}
        const branch = gitExec(project.path, ['rev-parse', '--abbrev-ref', 'HEAD']);
        const statusRaw = gitExec(project.path, ['status', '--porcelain', '-u']);
        const files = [];
        if (statusRaw) for (const line of statusRaw.split('\n')) {
          if (line.length < 4) continue;
          const x = line[0], y = line[1];
          let fStart = 2;
          while (fStart < line.length && line[fStart] === ' ') fStart++;
          const file = line.slice(fStart);
          if (x !== ' ' && x !== '?') {
            const st = x === 'A' ? 'added' : x === 'D' ? 'deleted' : x === 'R' ? 'renamed' : 'modified';
            files.push({ file, xy: x + y, staged: true, status: st });
          }
          if (y !== ' ' || x === '?') {
            const st = x === '?' ? 'untracked' : y === 'D' ? 'deleted' : 'modified';
            files.push({ file, xy: x + y, staged: false, status: st });
          }
        }
        const parseNumstat = (raw, map) => {
          if (raw) for (const l of raw.split('\n')) {
            const m = l.match(/^(\d+|-)\t(\d+|-)\t(.+)$/);
            if (m) map[m[3]] = { add: m[1] === '-' ? 0 : +m[1], del: m[2] === '-' ? 0 : +m[2] };
          }
        };
        const unstagedStats = {}, stagedStats = {};
        try { parseNumstat(gitExec(project.path, ['diff', '--numstat']), unstagedStats); } catch {}
        try { parseNumstat(gitExec(project.path, ['diff', '--cached', '--numstat']), stagedStats); } catch {}
        for (const f of files) {
          const s = f.staged ? stagedStats[f.file] : unstagedStats[f.file];
          if (s) { f.add = s.add; f.del = s.del; }
        }
        let ahead = 0, behind = 0;
        try {
          const ab = gitExec(project.path, ['rev-list', '--left-right', '--count', 'HEAD...@{upstream}']);
          const p = ab.split(/\s+/); ahead = +p[0] || 0; behind = +p[1] || 0;
        } catch {}
        return json(res, { ok: true, branch, files, ahead, behind });
      } catch (err) { return json(res, { ok: false, error: err.message }); }
    }

    if (path === '/api/git/diff' && req.method === 'GET') {
      const alias = url.searchParams.get('project');
      const file = url.searchParams.get('file');
      const cached = url.searchParams.get('cached') === '1';
      const project = getProject(alias);
      if (!project) return json(res, { ok: false, error: 'Project not found' }, 404);
      try {
        const args = cached ? ['diff', '--cached', '--', file] : ['diff', '--', file];
        const diff = gitExec(project.path, args);
        let oldContent = '', newContent = '';
        if (cached) {
          try { oldContent = gitExec(project.path, ['show', `HEAD:${file}`]); } catch {}
          try { newContent = gitExec(project.path, ['show', `:${file}`]); } catch {}
        } else {
          try { oldContent = gitExec(project.path, ['show', `:${file}`]); } catch {
            try { oldContent = gitExec(project.path, ['show', `HEAD:${file}`]); } catch {}
          }
          try { newContent = readFileSync(join(project.path, file), 'utf-8'); } catch {}
        }
        return json(res, { ok: true, diff, old: oldContent, new: newContent, file });
      } catch (err) { return json(res, { ok: false, error: err.message }); }
    }

    if (path === '/api/git/stage' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      const project = getProject(body.project);
      if (!project) return json(res, { ok: false, error: 'Project not found' }, 404);
      try { gitExec(project.path, ['add', '--', ...(body.files || [])]); return json(res, { ok: true }); }
      catch (err) { return json(res, { ok: false, error: err.message }); }
    }

    if (path === '/api/git/unstage' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      const project = getProject(body.project);
      if (!project) return json(res, { ok: false, error: 'Project not found' }, 404);
      try {
        gitExec(project.path, ['restore', '--staged', '--', ...(body.files || [])]);
        return json(res, { ok: true });
      } catch {
        try { gitExec(project.path, ['reset', 'HEAD', '--', ...(body.files || [])]); return json(res, { ok: true }); }
        catch (err) { return json(res, { ok: false, error: err.message }); }
      }
    }

    if (path === '/api/git/commit' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      const project = getProject(body.project);
      if (!project || !body.message) return json(res, { ok: false, error: 'Missing project or message' }, 400);
      try { gitExec(project.path, ['commit', '-m', body.message]); return json(res, { ok: true }); }
      catch (err) { return json(res, { ok: false, error: err.message }); }
    }

    if (path === '/api/git/push' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      const project = getProject(body.project);
      if (!project) return json(res, { ok: false, error: 'Project not found' }, 404);
      try { gitExec(project.path, ['push']); return json(res, { ok: true }); }
      catch (err) { return json(res, { ok: false, error: err.message }); }
    }

    if (path === '/api/git/pull' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      const project = getProject(body.project);
      if (!project) return json(res, { ok: false, error: 'Project not found' }, 404);
      try { const out = gitExec(project.path, ['pull']); return json(res, { ok: true, output: out }); }
      catch (err) { return json(res, { ok: false, error: err.message }); }
    }

    if (path === '/api/git/discard' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      const project = getProject(body.project);
      if (!project) return json(res, { ok: false, error: 'Project not found' }, 404);
      try {
        const status = gitExec(project.path, ['status', '--porcelain', '--', body.file]);
        if (status.startsWith('??')) {
          const fullPath = resolve(project.path, body.file);
          if (existsSync(fullPath)) { const { rmSync } = await import('node:fs'); rmSync(fullPath, { recursive: true, force: true }); }
        } else {
          gitExec(project.path, ['checkout', 'HEAD', '--', body.file]);
        }
        return json(res, { ok: true });
      } catch (err) { return json(res, { ok: false, error: err.message }); }
    }

    if (path === '/api/git/stageHunk' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      const project = getProject(body.project);
      if (!project || !body.patch) return json(res, { ok: false, error: 'Missing project or patch' }, 400);
      try {
        execFileSync('git', ['apply', '--cached', '--recount', '--allow-empty', '-'], {
          cwd: project.path, input: body.patch, encoding: 'utf8', timeout: 15000, windowsHide: true,
        });
        return json(res, { ok: true });
      } catch (err) { return json(res, { ok: false, error: err.message }); }
    }

    // ─── Internal MCP dispatch (from stdio MCP servers) ───
    if (path === '/api/mcp/call' && req.method === 'POST') {
      // Auth via internal API key (not user token)
      const apiKey = req.headers['x-api-key'];
      if (apiKey !== internalApiKey) return json(res, { error: 'Invalid API key' }, 403);
      const body = JSON.parse(await readBody(req));
      if (!body.tool) return json(res, { ok: false, error: 'Missing tool name' }, 400);
      const a = body.args || {};

      // ─── Kanban tools (project-scoped via args.alias) ───
      if (body.tool.startsWith('kanban_')) {
        const alias = a.alias;
        if (!alias) return json(res, { ok: false, error: 'No project context for kanban tool' });
        let r;
        switch (body.tool) {
          case 'kanban_list': r = { ok: true, board: enrichBoardWithMindmap(alias, kanban.getBoard(alias, { includeDeleted: !!a.includeDeleted })) }; break;
          case 'kanban_get_task': {
            r = kanban.getTask(alias, a.taskId, { includeDeleted: !!a.includeDeleted });
            if (r.ok) r.task.mindmapNodes = mindmap.getNodesForTask(alias, r.task.id);
            break;
          }
          case 'kanban_list_column': {
            r = kanban.getColumn(alias, a.status, { includeDeleted: !!a.includeDeleted });
            if (r.ok) r.tasks.forEach(t => { t.mindmapCount = mindmap.getNodesForTask(alias, t.id).length; });
            break;
          }
          case 'kanban_add_task': r = kanban.addTask(alias, { title: a.title, description: a.description, status: a.status, todos: a.todos }); break;
          case 'kanban_update_task': r = kanban.updateTask(alias, a.taskId, { title: a.title, description: a.description, status: a.status }); break;
          case 'kanban_move_task':
            r = kanban.moveTask(alias, a.taskId, a.status, a.index);
            if (r.ok) { const gt = kanban.getTask(alias, a.taskId); notifyEvent('kanbanTask', `📋 Task "${gt.ok ? gt.task.title : a.taskId}" → ${a.status}${alias ? ` (${alias})` : ''}.`); }
            break;
          case 'kanban_delete_task': r = kanban.deleteTask(alias, a.taskId); break;
          case 'kanban_restore_task': r = kanban.restoreTask(alias, a.taskId); break;
          case 'kanban_add_todo': r = kanban.addTodo(alias, a.taskId, a.text); break;
          case 'kanban_update_todo':
            r = kanban.updateTodo(alias, a.taskId, a.todoId, { text: a.text, done: a.done });
            if (r.ok && typeof a.done === 'boolean') notifyEvent('kanbanSubtask', `☑️ Subtask ${a.done ? 'completed' : 'reopened'}${alias ? ` (${alias})` : ''}.`);
            break;
          case 'kanban_delete_todo': r = kanban.deleteTodo(alias, a.taskId, a.todoId); break;
          case 'kanban_restore_todo': r = kanban.restoreTodo(alias, a.taskId, a.todoId); break;
          case 'kanban_history': r = { ok: true, history: kanban.getHistory(alias) }; break;
          default: return json(res, { ok: false, error: `Unknown kanban tool: ${body.tool}` }, 404);
        }
        const kanbanReadOnly = ['kanban_list', 'kanban_history', 'kanban_get_task', 'kanban_list_column'].includes(body.tool);
        if (r.ok && !kanbanReadOnly) broadcastKanban(alias);
        return json(res, r);
      }

      // ─── Mindmap tools (global; alias used as default project for links) ───
      if (body.tool.startsWith('mindmap_')) {
        let r;
        switch (body.tool) {
          case 'mindmap_list': r = { ok: true, mindmap: mindmap.getMindmap(a.alias || null, { compact: !!a.compact }) }; break;
          case 'mindmap_search': r = mindmap.searchMindmap(a.query, a.alias || null, a.limit); break;
          case 'mindmap_get_subtree': r = mindmap.getSubtree(a.id, a.alias || null); break;
          case 'mindmap_get_children': r = mindmap.getChildren(a.id || null, a.alias || null); break;
          case 'mindmap_get_ancestors': r = mindmap.getAncestors(a.id, a.alias || null); break;
          case 'mindmap_add_node':
            r = mindmap.addNode({ text: a.text, parentId: a.parentId, note: a.note, notes: a.notes, project: a.project || a.alias, taskId: a.taskId, todoId: a.todoId, scope: a.alias });
            if (r.ok) notifyEvent('mindmapAdd', `🧠 Idea added: "${String(a.text || '').slice(0, 60)}"${a.alias ? ` (${a.alias})` : ''}.`);
            break;
          case 'mindmap_update_node': r = mindmap.updateNode(a.id, { text: a.text, note: a.note, notes: a.notes }); break;
          case 'mindmap_add_note': r = mindmap.addNote(a.id, a.text); break;
          case 'mindmap_remove_note': r = mindmap.removeNote(a.id, a.index); break;
          case 'mindmap_move_node': r = mindmap.moveNode(a.id, a.parentId, a.index); break;
          case 'mindmap_link_node': r = mindmap.linkNode(a.id, { project: a.project || a.alias, taskId: a.taskId, todoId: a.todoId }); break;
          case 'mindmap_unlink_node': r = mindmap.unlinkNode(a.id); break;
          case 'mindmap_delete_node': {
            const gn = mindmap.getNode ? mindmap.getNode(a.id) : null;
            r = mindmap.deleteNode(a.id);
            if (r.ok) notifyEvent('mindmapDelete', `🧠 Idea deleted${gn && gn.text ? `: "${String(gn.text).slice(0, 60)}"` : ''}${a.alias ? ` (${a.alias})` : ''}.`);
            break;
          }
          default: return json(res, { ok: false, error: `Unknown mindmap tool: ${body.tool}` }, 404);
        }
        const mindmapReadOnly = ['mindmap_list', 'mindmap_search', 'mindmap_get_subtree', 'mindmap_get_children', 'mindmap_get_ancestors'].includes(body.tool);
        if (r.ok && !mindmapReadOnly) broadcastMindmap();
        return json(res, r);
      }

      // ─── Schedule tools (strictly project-scoped via args.alias) ───
      if (body.tool.startsWith('schedule_')) {
        const alias = String(a.alias || '').toLowerCase();
        if (!alias) return json(res, { ok: false, error: 'No project context for schedule tool' });
        // For id-based ops, confirm the schedule belongs to THIS project.
        const ownsId = (id) => { const s = schedule.getSchedule(id); return s && String(s.project).toLowerCase() === alias ? s : null; };
        let r;
        switch (body.tool) {
          case 'schedule_list': r = { ok: true, schedules: schedule.listSchedules(alias) }; break;
          case 'schedule_get': r = ownsId(a.id) ? { ok: true, schedule: ownsId(a.id) } : { ok: false, error: 'Schedule not found in this project' }; break;
          case 'schedule_add':
            // project is forced to this project — agents can't target another.
            r = schedule.addSchedule({ name: a.name, when: a.when, action: a.action, conditions: a.conditions, enabled: a.enabled, project: alias });
            break;
          case 'schedule_update':
            if (!ownsId(a.id)) { r = { ok: false, error: 'Schedule not found in this project' }; break; }
            r = schedule.updateSchedule(a.id, { name: a.name, when: a.when, action: a.action, conditions: a.conditions, enabled: a.enabled });
            break;
          case 'schedule_set_enabled':
            r = ownsId(a.id) ? schedule.setEnabled(a.id, a.enabled !== false) : { ok: false, error: 'Schedule not found in this project' };
            break;
          case 'schedule_delete':
            r = ownsId(a.id) ? schedule.deleteSchedule(a.id) : { ok: false, error: 'Schedule not found in this project' };
            break;
          default: return json(res, { ok: false, error: `Unknown schedule tool: ${body.tool}` }, 404);
        }
        const schedReadOnly = ['schedule_list', 'schedule_get'].includes(body.tool);
        if (r.ok && !schedReadOnly) broadcastSSE('schedule', {});
        return json(res, r);
      }

      // ─── Media tools (strictly project-scoped via args.alias) ───
      if (body.tool.startsWith('media_')) {
        const alias = String(a.alias || '').toLowerCase();
        if (!alias) return json(res, { ok: false, error: 'No project context for media tool' });
        // MCP view: include the local on-disk path so an agent can open the file.
        const mcpView = (item) => {
          const e = enrichMedia(item);
          return { ...e, path: media.mediaFilePath(item) };
        };
        const ownsId = (id) => { const it = media.getMedia(id); return it && it.project === alias ? it : null; };
        let r;
        switch (body.tool) {
          case 'media_list':
            r = { ok: true, items: media.listMedia({ project: alias }).map(mcpView) };
            break;
          case 'media_get': {
            const it = ownsId(a.id);
            r = it ? { ok: true, item: mcpView(it) } : { ok: false, error: 'Media not found in this project' };
            break;
          }
          case 'media_add_path': {
            // Optional link to a task/todo/node in THIS project.
            let link = null;
            if (a.taskId && a.todoId) link = { type: 'todo', taskId: a.taskId, todoId: a.todoId };
            else if (a.taskId) link = { type: 'task', taskId: a.taskId };
            else if (a.nodeId) link = { type: 'node', nodeId: a.nodeId };
            if (link && (link.type === 'task' || link.type === 'todo')) {
              const t = kanban.getTask(alias, link.taskId, { includeDeleted: true });
              if (!t.ok) { r = { ok: false, error: 'Linked task not found in this project' }; break; }
            } else if (link && link.type === 'node') {
              const n = mindmap.getNode(link.nodeId);
              if (!n || (n.effectiveProject || null) !== alias) { r = { ok: false, error: 'Linked idea not found in this project' }; break; }
            }
            const add = media.addMediaFromPath({ path: a.path, name: a.name, project: alias, link });
            r = add.ok ? { ok: true, item: mcpView(add.item) } : add;
            break;
          }
          case 'media_delete':
            r = ownsId(a.id) ? media.deleteMedia(a.id) : { ok: false, error: 'Media not found in this project' };
            break;
          default: return json(res, { ok: false, error: `Unknown media tool: ${body.tool}` }, 404);
        }
        const mediaReadOnly = ['media_list', 'media_get'].includes(body.tool);
        if (r.ok && !mediaReadOnly) broadcastMedia();
        return json(res, r);
      }

      // ─── Claude usage (real, account-wide limits) ───
      if (body.tool === 'get_usage') {
        return json(res, await usage.getUsage({ force: !!a.force }));
      }

      // ─── Secret search (no approval — metadata only, never the value) ───
      if (body.tool === 'secret_search') {
        return json(res, { ok: true, results: secrets.searchSecrets(a.query || '') });
      }

      // ─── Secret access (requires user approval + PIN; blocks until then) ───
      if (body.tool === 'secret_get') {
        // Resolve which secret: by id, else by exact (case-insensitive) name.
        let meta = a.id ? secrets.getSecretMeta(a.id) : null;
        if (!meta && a.name) {
          const byName = secrets.searchSecrets(a.name)
            .filter(s => s.name.toLowerCase() === String(a.name).toLowerCase());
          if (byName.length > 1) {
            return json(res, { ok: false, error: `Multiple secrets named "${a.name}" — call secret_get with the id instead.`, matches: byName });
          }
          meta = byName[0] || null;
        }
        if (!meta) return json(res, { ok: false, error: 'Secret not found. Use secret_search to find the right name/id.' });

        const result = await waitForSecretApproval({
          secretId: meta.id,
          secretName: meta.name,
          projectAlias: a.alias || '',
          reason: a.reason || '',
        });
        return json(res, result);
      }

      // Built-in notification tools
      if (body.tool === 'send_message_to_user' && bot) {
        try {
          const chatId = getChatId ? getChatId() : null;
          if (!chatId) return json(res, { ok: false, error: 'No chat ID configured' });
          await bot.api.sendMessage(chatId, body.args?.message || '(empty)');
          return json(res, { ok: true });
        } catch (err) {
          return json(res, { ok: false, error: err.message });
        }
      }

      if (body.tool === 'send_photo_to_user' && bot) {
        try {
          const chatId = getChatId ? getChatId() : null;
          if (!chatId) return json(res, { ok: false, error: 'No chat ID configured' });
          const { InputFile } = await import('grammy');
          await bot.api.sendPhoto(chatId, new InputFile(body.args?.path));
          return json(res, { ok: true });
        } catch (err) {
          return json(res, { ok: false, error: err.message });
        }
      }

      if (body.tool === 'send_file_to_user') {
        const filePath = body.args?.path;
        if (!filePath || !existsSync(filePath)) return json(res, { ok: false, error: 'File not found' });
        const tok = shareFile(filePath);
        const url = getFileShareUrl(tok);
        return json(res, { ok: true, url });
      }

      // Service tools
      if (body.tool === 'list_services') {
        return json(res, { ok: true, services: getAllServiceStatus() });
      }
      if (body.tool === 'start_service') {
        return json(res, startService(body.args?.key));
      }
      if (body.tool === 'stop_service') {
        return json(res, stopService(body.args?.key));
      }
      if (body.tool === 'restart_service') {
        return json(res, restartService(body.args?.key));
      }
      if (body.tool === 'delete_service') {
        return json(res, deleteService(body.args?.key));
      }
      if (body.tool === 'get_service_logs') {
        return json(res, { ok: true, logs: getServiceLogs(body.args?.key, body.args?.lines || 50) });
      }
      if (body.tool === 'register_service') {
        const r = registerService({
          alias: body.args?.alias || 'default',
          name: body.args?.name,
          command: body.args?.command,
          cwd: body.args?.cwd,
          stopCommand: body.args?.stopCommand,
        });
        return json(res, r);
      }
      if (body.tool === 'enable_tunnel') {
        const r = startTunnel(body.args?.key, body.args?.port);
        return json(res, r);
      }
      if (body.tool === 'disable_tunnel') {
        stopTunnel(body.args?.key);
        return json(res, { ok: true });
      }

      // Delegate to external dispatch handler (for browser, screenshots, etc.)
      if (mcpDispatch) {
        try {
          const result = await mcpDispatch(body.tool, body.args || {});
          return json(res, result);
        } catch (err) {
          return json(res, { ok: false, error: err.message });
        }
      }

      return json(res, { ok: false, error: `Unknown tool: ${body.tool}` }, 404);
    }

    json(res, { error: 'Not found' }, 404);
  }

  // ─── WebSocket: Terminal I/O ───

  function setupWebSocket() {
    wss = new WebSocketServer({ noServer: true });

    server.on('upgrade', (req, socket, head) => {
      // A forward's own WebSocket — hot reload, dev tooling, whatever the app
      // uses. Without this every dev server's HMR dies at the first hop, which
      // is the sort of thing you notice ten minutes later and blame elsewhere.
      const pathFwd = forwards.matchPath(req.url);
      if (pathFwd) {
        if (!pathFwd.forward.public && !validateToken(req) && !hasForwardCookie(req)) { socket.destroy(); return; }
        forwards.proxyUpgrade(pathFwd.forward, req, socket, head, pathFwd.upstreamPath);
        return;
      }

      const fwd = forwards.match(req.headers.host);
      if (fwd) {
        if (!fwd.public && !validateToken(req) && !hasForwardCookie(req)) { socket.destroy(); return; }
        forwards.proxyUpgrade(fwd, req, socket, head);
        return;
      }

      const url = new URL(req.url, 'http://localhost');
      if (url.pathname !== '/ws') {
        socket.destroy();
        return;
      }
      // Auth check
      if (!validateToken(req)) {
        socket.write('HTTP/1.1 401 Unauthorized\r\n\r\n');
        socket.destroy();
        return;
      }
      wss.handleUpgrade(req, socket, head, (ws) => {
        wss.emit('connection', ws, req);
      });
    });

    // Stream server logs to all WebSocket clients that opt in
    const logSubscribers = new Set();
    if (onServerLog) {
      onServerLog((entry) => {
        const msg = JSON.stringify({ type: 'server-log', ...entry });
        for (const ws of logSubscribers) {
          if (ws.readyState === 1) ws.send(msg);
        }
      });
    }

    wss.on('connection', (ws) => {
      // One socket can watch several terminals at once: id → output handler.
      const subs = new Map();
      // …and several chat sessions: id → event handler.
      const uiSubs = new Map();
      // Set when this socket is acting as the server's native host (a desktop
      // app lending us its GUI for the browser panel).
      let detachHost = null;

      ws.on('message', (raw) => {
        let msg;
        try { msg = JSON.parse(raw.toString()); } catch { return; }

        // ─── Native host ───
        // A desktop app offering its GUI to this server. Only meaningful for a
        // server with no Electron parent of its own (a container, or a headless
        // install); browser.js prefers the parent channel when there is one, so
        // this cannot hijack an all-in-one desktop install.
        if (msg.type === 'register-native-host') {
          if (detachHost) return;                       // already registered
          detachHost = browserMod.setRemoteHost({
            send: (m) => { if (ws.readyState === 1) ws.send(JSON.stringify({ type: 'native-host', payload: m })); },
            // What this app version can do. Absent from older clients, which is
            // exactly what it is here to tell us.
            capabilities: Array.isArray(msg.capabilities) ? msg.capabilities : [],
          });
          console.log('[crundi] Desktop app attached as native host');
          ws.send(JSON.stringify({ type: 'native-host-ready' }));
          return;
        }
        // A reply coming back from that desktop app.
        if (msg.type === 'native-host-result') {
          browserMod.handleHostMessage(msg.payload);
          return;
        }

        // Server log subscription
        if (msg.type === 'subscribe-logs') {
          logSubscribers.add(ws);
          return;
        }

        // Presence: is the user actively at this window right now? Gates the
        // agent-status Telegram ping (see anyClientPresent / handleAgentState).
        if (msg.type === 'presence') {
          if (msg.active) presentClients.set(ws, Date.now() + PRESENCE_TTL);
          else presentClients.delete(ws);
          return;
        }

        // Chat-session stream: one socket can watch several chats at once, the
        // same way it can watch several terminals.
        if (msg.type === 'subscribe-ui' || msg.type === 'unsubscribe-ui') {
          if (!claudeUi) return;
          const sid = msg.id;
          if (!sid) return;
          if (uiSubs.has(sid)) { claudeUi.off(sid, uiSubs.get(sid)); uiSubs.delete(sid); }
          if (msg.type === 'unsubscribe-ui') return;
          // Replay the conversation so a reconnecting client re-renders it, and
          // so any prompt parked while the client was away is re-armed.
          const h = claudeUi.history(sid);
          if (h) ws.send(JSON.stringify({ type: 'ui-history', id: sid, session: h }));
          const handler = (ev) => {
            if (ws.readyState === 1) ws.send(JSON.stringify({ type: 'ui-event', id: sid, event: ev }));
          };
          uiSubs.set(sid, handler);
          claudeUi.on(sid, handler);
          return;
        }

        if (!claudeTerminals) return;
        const id = msg.id;

        switch (msg.type) {
          case 'subscribe': {
            if (!id) break;
            // Replace an existing subscription for this id (re-subscribe is safe).
            if (subs.has(id)) { claudeTerminals.off(id, subs.get(id)); subs.delete(id); }
            // Send existing scrollback
            const scrollback = claudeTerminals.getScrollback(id);
            if (scrollback) {
              ws.send(JSON.stringify({ type: 'output', id, data: scrollback }));
            }
            // Subscribe to new output
            const handler = (data) => {
              if (ws.readyState === 1) {
                ws.send(JSON.stringify({ type: 'output', id, data }));
              }
            };
            subs.set(id, handler);
            claudeTerminals.on(id, handler);
            break;
          }
          case 'unsubscribe': {
            if (id && subs.has(id)) { claudeTerminals.off(id, subs.get(id)); subs.delete(id); }
            break;
          }
          case 'input': {
            if (id && msg.data) claudeTerminals.write(id, msg.data);
            break;
          }
          case 'resize': {
            if (id && msg.cols && msg.rows) claudeTerminals.resize(id, msg.cols, msg.rows);
            break;
          }
        }
      });

      ws.on('close', () => {
        for (const [id, handler] of subs) claudeTerminals.off(id, handler);
        subs.clear();
        if (claudeUi) for (const [id, handler] of uiSubs) claudeUi.off(id, handler);
        uiSubs.clear();
        logSubscribers.delete(ws);
        presentClients.delete(ws);
        if (detachHost) {
          detachHost();
          detachHost = null;
          console.log('[crundi] Desktop app detached as native host');
        }
      });
    });
  }

  // ─── Lifecycle ───

  return {
    async start(listenPort = 0) {
      if (server) throw new Error('Web app already running');

      const onRequest = (req, res) => {
        handleRequest(req, res).catch(err => {
          console.error('[webapp] Request error:', err.message);
          try { json(res, { error: 'Internal error' }, 500); } catch { /* ignore */ }
        });
      };

      // ─── TLS ───
      // Off by default: behind the Cloudflare tunnel, or any proxy that
      // terminates TLS, this machine only ever sees plain HTTP and should.
      const creds = tls.init();
      if (tls.enabled() && creds) {
        await new Promise((resolve, reject) => {
          server = createHttpsServer({ key: creds.key, cert: creds.cert }, onRequest);
          server.listen(config.tlsPort, '0.0.0.0', () => {
            port = server.address().port;
            console.log(`[webapp] HTTPS server on 0.0.0.0:${port}`);
            resolve();
          });
          server.on('error', reject);
        });
        // Everything on this machine that talks to the API speaks plain HTTP:
        // the MCP server Claude loads from .mcp.json, and the lifecycle hooks.
        // Once TLS owns the port, http://localhost:443 is a TLS socket being
        // fed an HTTP request, which is the "socket hang up" every MCP call
        // died with. Bind a loopback-only HTTP listener for them. 127.0.0.1,
        // never 0.0.0.0 — this is deliberately not reachable off the box.
        await new Promise((resolve) => {
          localServer = createServer(onRequest);
          localServer.listen(config.webPort, '127.0.0.1', () => {
            localPort = localServer.address().port;
            console.log(`[webapp] Local HTTP API on 127.0.0.1:${localPort} (for MCP and hooks)`);
            resolve();
          });
          localServer.on('error', (err) => {
            console.warn('[webapp] Could not bind the local HTTP API:', err.message);
            localServer = null;
            resolve();
          });
        });

        // Renewal swaps the certificate into the live server. setSecureContext
        // applies to connections made from then on, so nothing is dropped and
        // no restart is needed.
        tls.startRenewal((next) => {
          try {
            server.setSecureContext({ key: next.key, cert: next.cert });
            console.log('[webapp] Certificate reloaded without a restart.');
          } catch (err) {
            console.error('[webapp] Could not reload the certificate:', err.message);
          }
        });
      } else {
        if (tls.enabled() && !creds && tls.mode() === 'letsencrypt') {
          console.log('[webapp] Starting on HTTP while the first certificate is obtained.');
        }
        await new Promise((resolve, reject) => {
          server = createServer(onRequest);
          server.listen(listenPort, '0.0.0.0', () => {
            port = server.address().port;
            console.log(`[webapp] HTTP server on 0.0.0.0:${port}`);
            resolve();
          });
          server.on('error', reject);
        });
      }

      // The ACME listener. Separate from the main server because the CA fetches
      // the challenge over plain port 80 and will not follow a redirect — so
      // this has to answer there even when everything else is on 443.
      if (tls.mode() === 'letsencrypt') {
        try {
          acmeServer = createServer((req, res) => {
            if (tls.handleAcmeChallenge(req, res)) return;
            const host = req.headers.host ? String(req.headers.host).replace(/:\d+$/, '') : config.tlsDomain;
            res.writeHead(301, { Location: `https://${host}${req.url || '/'}` });
            res.end();
          });
          acmeServer.on('error', (err) => console.warn(`[webapp] Port ${config.tlsHttpPort} unavailable (${err.message}) — ACME renewal will fail.`));
          acmeServer.listen(config.tlsHttpPort, '0.0.0.0', () => {
            console.log(`[webapp] ACME challenge + redirect listener on 0.0.0.0:${config.tlsHttpPort}`);
          });
          // If HTTPS is not up yet the renewal loop has not been started above.
          if (!creds) {
            tls.startRenewal(() => {
              console.log('[webapp] Certificate obtained — restart to serve HTTPS.');
            });
          }
        } catch (err) {
          console.warn('[webapp] Could not start the ACME listener:', err.message);
        }
      }

      setupWebSocket();

      // Start Cloudflare tunnel (named if token configured, otherwise quick).
      // Set DISABLE_TUNNEL=1 to run localhost-only (no Cloudflare at all).
      if (process.env.DISABLE_TUNNEL === '1') {
        console.log('[webapp] Tunnel disabled (DISABLE_TUNNEL=1) — localhost only');
      } else if (config.tlsMode !== 'off' && config.tlsDomain) {
        // This server is already reachable at its own name over its own TLS.
        // A quick tunnel would publish a SECOND, unauthenticated-looking URL to
        // the same thing, which is both redundant and a wider door than the one
        // the user chose to open.
        console.log(`[webapp] Reachable at https://${config.tlsDomain} — not starting a tunnel`);
      } else if (config.tunnelToken) {
        const tunnelResult = startNamedTunnel(TUNNEL_KEY, config.tunnelToken, config.tunnelUrl);
        if (!tunnelResult.ok) {
          console.warn('[webapp] Named tunnel failed:', tunnelResult.error, '— running on localhost only');
        } else {
          try {
            await waitForTunnel(TUNNEL_KEY, 30000);
            tunnelUrl = config.tunnelUrl || null;
            console.log(`[webapp] Named tunnel connected${tunnelUrl ? `: ${tunnelUrl}` : ''}`);
          } catch (err) {
            console.warn('[webapp] Named tunnel not ready:', err.message, '— running on localhost only');
            stopTunnel(TUNNEL_KEY);
          }
        }
      } else {
        const tunnelResult = startTunnel(TUNNEL_KEY, port);
        if (!tunnelResult.ok) {
          console.warn('[webapp] Tunnel failed:', tunnelResult.error, '— running on localhost only');
        } else {
          try {
            tunnelUrl = await waitForTunnel(TUNNEL_KEY, 30000);
            console.log(`[webapp] Tunnel ready: ${tunnelUrl}`);
          } catch (err) {
            console.warn('[webapp] Tunnel not ready:', err.message, '— running on localhost only');
            stopTunnel(TUNNEL_KEY);
          }
        }
      }

      // SSE state broadcast every 5s
      setInterval(() => {
        if (sseClients.size) broadcastState();
      }, 5000);

      // Real Claude usage refresh every 60s (usage.js cache = 60s, error
      // backoff = 5 min, and a 15s anti-burst floor on forced fetches)
      setInterval(() => { broadcastUsage(); }, 60_000);

      // Service start / crash-stop notifications (independent of SSE clients).
      checkServiceTransitions(); // baseline current statuses (no alerts on first pass)
      setInterval(checkServiceTransitions, 7000);

      // Ask GitHub whether there is a newer release, now and every six hours.
      // Only ever reports — applying is an explicit request from Settings.
      serverUpdate.start();

      return { port, tunnelUrl, localPort };
    },

    stop() {
      for (const client of sseClients) {
        try { client.res.end(); } catch { /* ignore */ }
      }
      sseClients.clear();
      if (wss) { wss.close(); wss = null; }
      stopTunnel(TUNNEL_KEY);
      if (server) { server.close(); server = null; }
      if (localServer) { try { localServer.close(); } catch { /* ignore */ } localServer = null; }
      localPort = null;
      port = null;
      tunnelUrl = null;
      tokens.clear();
      refreshTokens.clear();
      console.log('[webapp] Stopped');
    },

    isRunning() { return server !== null; },
    getUrl() { return tunnelUrl || (port ? `http://localhost:${port}` : null); },
    getPort() { return port; },

    /** Register a file for temporary download via /dl/:token. */
    shareFile,
    getFileShareUrl,

    /** Internal API key for MCP stdio servers. */
    getInternalApiKey() { return internalApiKey; },

    /** Regenerate the HTML cache with an updated bot username (call after bot.init). */
    refreshHtml(botUsername) { htmlCache = getWebappHtml(botUsername); },

    /** Broadcast state update to all connected SSE clients. */
    broadcastState,

    /** Fire a Telegram notification for an event key, honoring its policy + presence. */
    notifyEvent,
  };
}

function formatUptime(ms) {
  const s = Math.floor(ms / 1000);
  if (s < 60) return s + 's';
  if (s < 3600) return Math.floor(s / 60) + 'm ' + (s % 60) + 's';
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return h + 'h ' + m + 'm';
}
