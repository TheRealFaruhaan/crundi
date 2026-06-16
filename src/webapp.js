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
import { createReadStream, readFileSync, writeFileSync, readdirSync, statSync, existsSync, mkdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { createHmac, randomBytes, createHash } from 'node:crypto';
import { basename, join, dirname, resolve } from 'node:path';
import { homedir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { WebSocketServer } from 'ws';
import { getWebappHtml } from './webapp-html.js';
import { getAllServiceStatus, startService, stopService, restartService, getServiceLogs, deleteService } from './services.js';
import { registerService, listRegisteredForProject, updateRegistered } from './service-registry.js';
import { startTunnel, startNamedTunnel, stopTunnel, getTunnelInfo, getAllTunnelInfo, waitForTunnel } from './tunnel.js';
import * as browserMod from './browser.js';
import * as terminalsMod from './terminals.js';
import { listProjects, getProject, registerProject, removeProject, getProjectMode, importFromOldData, importServicesFromOldData } from './project-store.js';
import * as kanban from './kanban-store.js';
import * as secrets from './secrets-store.js';
import * as mindmap from './mindmap-store.js';
import * as usage from './usage.js';
import { getOldAppDataDir, isFreshInstall, envPath } from './config.js';
import { ensureGitignore } from './claude-terminals.js';

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

// Web app manifest — makes Crundi installable as a PWA.
const WEB_MANIFEST = JSON.stringify({
  name: 'Crundi',
  short_name: 'Crundi',
  description: 'Claude Code terminal in your browser',
  start_url: '/',
  scope: '/',
  display: 'standalone',
  orientation: 'any',
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
const CACHE = 'crundi-shell-v5';
const SHELL = ['/', '/manifest.webmanifest', '/assets/icon_128x128.png', '/assets/icon_256x256.png'];

self.addEventListener('install', (e) => {
  e.waitUntil(caches.open(CACHE).then((c) => c.addAll(SHELL)).then(() => self.skipWaiting()));
});

self.addEventListener('activate', (e) => {
  e.waitUntil(
    caches.keys().then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim())
  );
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
const TOKEN_TTL_MS = 24 * 60 * 60 * 1000; // 24 hours (webapp sessions last longer)

const startedAt = Date.now();

/**
 * Create the Crundi web app server.
 *
 * @param {{ config: object, claudeTerminals: object, bot?: object, mcpDispatch?: function }} deps
 *   bot — grammy Bot instance (for Telegram notifications from MCP tools)
 *   mcpDispatch — optional handler for MCP tool calls from stdio servers
 */
export function createWebApp({ config, claudeTerminals, bot, mcpDispatch, serverLogs, onServerLog, getChatId, setChatId }) {
  let server = null;
  let wss = null;
  let port = null;
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

  // Session tokens: token → { username, createdAt }
  const tokens = new Map();

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

  function createToken(username) {
    const tok = randomBytes(32).toString('hex');
    tokens.set(tok, { username, createdAt: Date.now() });
    return tok;
  }

  function validateToken(req) {
    // Check Authorization header
    const auth = req.headers['authorization'];
    if (auth?.startsWith('Bearer ')) {
      const tok = auth.slice(7);
      const entry = tokens.get(tok);
      if (entry && Date.now() - entry.createdAt < TOKEN_TTL_MS) return true;
      if (entry) tokens.delete(tok);
    }
    // Check query param (for WebSocket/SSE)
    const url = new URL(req.url, 'http://localhost');
    const tok = url.searchParams.get('token');
    if (tok) {
      const entry = tokens.get(tok);
      if (entry && Date.now() - entry.createdAt < TOKEN_TTL_MS) return true;
      if (entry) tokens.delete(tok);
    }
    return false;
  }

  function extractToken(req) {
    const auth = req.headers['authorization'];
    if (auth?.startsWith('Bearer ')) return auth.slice(7);
    const url = new URL(req.url, 'http://localhost');
    return url.searchParams.get('token');
  }

  // ─── SSE ───
  const sseClients = new Set();

  function broadcastSSE(event, data) {
    const payload = `event: ${event}\ndata: ${JSON.stringify(data)}\n\n`;
    for (const client of sseClients) {
      try { client.res.write(payload); } catch { sseClients.delete(client); }
    }
  }

  function broadcastState() {
    if (!sseClients.size) return;
    const projects = listProjects();
    const services = getAllServiceStatus().map(s => ({
      key: s.key, name: s.name, alias: s.alias, command: s.command,
      projectPath: s.projectPath, status: s.status, pid: s.pid,
      memory: s.memoryBytes,
      uptime: s.startedAt ? formatUptime(Date.now() - new Date(s.startedAt).getTime()) : null,
      tunnelUrl: s.tunnel?.url || null,
    }));
    const terminals = claudeTerminals ? claudeTerminals.list() : [];
    const userTerminals = terminalsMod.listTerminals();
    broadcastSSE('state', {
      uptime: formatUptime(Date.now() - startedAt),
      projects,
      services,
      terminals,
      userTerminals: userTerminals.terminals || [],
    });
  }

  // ─── Kanban live updates ───
  function broadcastKanban(projectAlias) {
    broadcastSSE('kanban', { project: String(projectAlias || '').toLowerCase() });
  }

  // ─── Mindmap live updates ───
  function broadcastMindmap() {
    broadcastSSE('mindmap', {});
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
      try {
        const chatId = getChatId ? getChatId() : null;
        if (chatId && bot) {
          const from = projectAlias ? ` (project: ${projectAlias})` : '';
          const why = reason ? `\nReason: ${reason}` : '';
          bot.api.sendMessage(chatId,
            `🔐 Claude is requesting access to secret "${secretName}"${from}.${why}\n\nOpen Crundi → Secrets to review and approve.`
          ).catch(() => { /* non-fatal */ });
        }
      } catch { /* non-fatal */ }
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
        res.writeHead(200, {
          'Content-Type': 'application/octet-stream',
          'Content-Length': st.size,
          'Content-Disposition': `attachment; filename="${entry.filename.replace(/"/g, '\\"')}"`,
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
        const token = createToken(result.user.username);
        return json(res, { ok: true, token, user: result.user });
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
      const token = createToken(result.user.username);
      res.writeHead(302, { Location: '/#token=' + token });
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

      const token = createToken(username);
      return json(res, { ok: true, token, user });
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
      const token = createToken(config.allowedUsername || 'local');
      return json(res, { ok: true, token });
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
    // MCP call endpoint uses its own X-Api-Key auth (not session tokens)
    if (path !== '/api/mcp/call' && !validateToken(req)) return json(res, { error: 'Unauthorized' }, 401);

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

      // Close any running Claude terminals for this project.
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
    if (path === '/api/terminals' && req.method === 'GET') {
      return json(res, { terminals: claudeTerminals ? claudeTerminals.list() : [] });
    }

    // Create a new terminal for a project (multiple per project allowed).
    if (path === '/api/terminals/create' && req.method === 'POST') {
      if (!claudeTerminals) return json(res, { ok: false, error: 'Terminal manager not available' });
      const body = JSON.parse(await readBody(req));
      if (!body.project) return json(res, { ok: false, error: 'project is required' }, 400);
      const result = await claudeTerminals.create(body.project, body);
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

    // ─── Services ───
    if (path === '/api/services' && req.method === 'GET') {
      const all = getAllServiceStatus().map(s => ({
        key: s.key, name: s.name, alias: s.alias, command: s.command,
        projectPath: s.projectPath, status: s.status, pid: s.pid,
        memory: s.memoryBytes,
        uptime: s.startedAt ? formatUptime(Date.now() - new Date(s.startedAt).getTime()) : null,
        tunnelPort: s.tunnelPort || 0,
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
          // Enable/disable (or change the port of) a service's Cloudflare tunnel.
          // port 0 = disable. Persists to the registry so it also auto-starts
          // with the service next time.
          const body = JSON.parse(await readBody(req));
          const port = parseInt(body.port, 10) || 0;
          const upd = updateRegistered(key, { tunnelPort: port });
          if (!upd.ok) result = upd;
          else if (port > 0) { startTunnel(key, port); result = { ok: true, enabled: true, port }; }
          else { stopTunnel(key); result = { ok: true, enabled: false }; }
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
        return json(res, { ok: true, settings, envPath, chatId: chatId ? String(chatId) : '' });
      } catch (err) { return json(res, { ok: false, error: err.message }); }
    }

    if (path === '/api/settings' && req.method === 'POST') {
      try {
        const body = JSON.parse(await readBody(req));
        const settings = body.settings || {};
        const KEYS = ['TELEGRAM_BOT_TOKEN', 'ALLOWED_USERNAME', 'PROJECTS_DIR', 'WEB_PORT', 'CLOUDFLARE_TUNNEL_TOKEN', 'CLOUDFLARE_TUNNEL_URL', 'DATA_DIR'];
        let content = '';
        for (const key of KEYS) {
          const val = settings[key] !== undefined ? settings[key] : '';
          content += `${key}=${val}\n`;
        }
        const dir = dirname(envPath);
        if (!existsSync(dir)) mkdirSync(dir, { recursive: true });
        writeFileSync(envPath, content, 'utf-8');
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
        return json(res, { ok: true, restartRequired: true });
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

    if (path === '/api/files/list' && req.method === 'GET') {
      const alias = url.searchParams.get('project');
      const relDir = url.searchParams.get('dir') || '';
      const project = getProject(alias);
      if (!project) return json(res, { ok: false, error: 'Project not found' }, 404);
      const fullPath = resolve(project.path, relDir);
      if (!fullPath.startsWith(project.path)) return json(res, { ok: false, error: 'Invalid path' }, 403);
      if (!existsSync(fullPath)) return json(res, { ok: false, error: 'Directory not found' }, 404);
      try {
        const entries = readdirSync(fullPath, { withFileTypes: true })
          .map(e => {
            const type = e.isDirectory() ? 'dir' : 'file';
            let size = 0;
            if (type === 'file') { try { size = statSync(join(fullPath, e.name)).size; } catch {} }
            return { name: e.name, type, size };
          })
          .sort((a, b) => a.type === b.type ? a.name.localeCompare(b.name) : a.type === 'dir' ? -1 : 1);
        return json(res, { ok: true, entries, path: relDir || '.' });
      } catch (err) { return json(res, { ok: false, error: err.message }); }
    }

    if (path === '/api/files/read' && req.method === 'GET') {
      const alias = url.searchParams.get('project');
      const relFile = url.searchParams.get('file') || '';
      const project = getProject(alias);
      if (!project) return json(res, { ok: false, error: 'Project not found' }, 404);
      const fullPath = resolve(project.path, relFile);
      if (!fullPath.startsWith(project.path)) return json(res, { ok: false, error: 'Invalid path' }, 403);
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
      const fullPath = resolve(project.path, body.file || '');
      if (!fullPath.startsWith(project.path)) return json(res, { ok: false, error: 'Invalid path' }, 403);
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
      const fullPath = resolve(project.path, relFile);
      if (!fullPath.startsWith(project.path)) { res.writeHead(403); res.end('Invalid path'); return; }
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
      const fullPath = resolve(project.path, body.file || '');
      if (!fullPath.startsWith(project.path)) return json(res, { ok: false, error: 'Invalid path' });
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
      const targetDir = resolve(project.path, relDir);
      if (!targetDir.startsWith(project.path)) return json(res, { ok: false, error: 'Invalid path' }, 403);
      if (!body.name || !body.data) return json(res, { ok: false, error: 'Missing name or data' }, 400);
      try {
        if (!existsSync(targetDir)) mkdirSync(targetDir, { recursive: true });
        const filePath = join(targetDir, basename(body.name));
        if (!filePath.startsWith(project.path)) return json(res, { ok: false, error: 'Invalid path' }, 403);
        const buf = Buffer.from(body.data, 'base64');
        writeFileSync(filePath, buf);
        return json(res, { ok: true, size: buf.length });
      } catch (err) { return json(res, { ok: false, error: err.message }); }
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
          case 'kanban_move_task': r = kanban.moveTask(alias, a.taskId, a.status, a.index); break;
          case 'kanban_delete_task': r = kanban.deleteTask(alias, a.taskId); break;
          case 'kanban_restore_task': r = kanban.restoreTask(alias, a.taskId); break;
          case 'kanban_add_todo': r = kanban.addTodo(alias, a.taskId, a.text); break;
          case 'kanban_update_todo': r = kanban.updateTodo(alias, a.taskId, a.todoId, { text: a.text, done: a.done }); break;
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
          case 'mindmap_add_node': r = mindmap.addNode({ text: a.text, parentId: a.parentId, note: a.note, notes: a.notes, project: a.project || a.alias, taskId: a.taskId, todoId: a.todoId, scope: a.alias }); break;
          case 'mindmap_update_node': r = mindmap.updateNode(a.id, { text: a.text, note: a.note, notes: a.notes }); break;
          case 'mindmap_add_note': r = mindmap.addNote(a.id, a.text); break;
          case 'mindmap_remove_note': r = mindmap.removeNote(a.id, a.index); break;
          case 'mindmap_move_node': r = mindmap.moveNode(a.id, a.parentId, a.index); break;
          case 'mindmap_link_node': r = mindmap.linkNode(a.id, { project: a.project || a.alias, taskId: a.taskId, todoId: a.todoId }); break;
          case 'mindmap_unlink_node': r = mindmap.unlinkNode(a.id); break;
          case 'mindmap_delete_node': r = mindmap.deleteNode(a.id); break;
          default: return json(res, { ok: false, error: `Unknown mindmap tool: ${body.tool}` }, 404);
        }
        const mindmapReadOnly = ['mindmap_list', 'mindmap_search', 'mindmap_get_subtree', 'mindmap_get_children', 'mindmap_get_ancestors'].includes(body.tool);
        if (r.ok && !mindmapReadOnly) broadcastMindmap();
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

      ws.on('message', (raw) => {
        let msg;
        try { msg = JSON.parse(raw.toString()); } catch { return; }

        // Server log subscription
        if (msg.type === 'subscribe-logs') {
          logSubscribers.add(ws);
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
        logSubscribers.delete(ws);
      });
    });
  }

  // ─── Lifecycle ───

  return {
    async start(listenPort = 0) {
      if (server) throw new Error('Web app already running');

      await new Promise((resolve, reject) => {
        server = createServer((req, res) => {
          handleRequest(req, res).catch(err => {
            console.error('[webapp] Request error:', err.message);
            try { json(res, { error: 'Internal error' }, 500); } catch { /* ignore */ }
          });
        });
        server.listen(listenPort, '0.0.0.0', () => {
          port = server.address().port;
          console.log(`[webapp] HTTP server on 0.0.0.0:${port}`);
          resolve();
        });
        server.on('error', reject);
      });

      setupWebSocket();

      // Start Cloudflare tunnel (named if token configured, otherwise quick).
      // Set DISABLE_TUNNEL=1 to run localhost-only (no Cloudflare at all).
      if (process.env.DISABLE_TUNNEL === '1') {
        console.log('[webapp] Tunnel disabled (DISABLE_TUNNEL=1) — localhost only');
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

      return { port, tunnelUrl };
    },

    stop() {
      for (const client of sseClients) {
        try { client.res.end(); } catch { /* ignore */ }
      }
      sseClients.clear();
      if (wss) { wss.close(); wss = null; }
      stopTunnel(TUNNEL_KEY);
      if (server) { server.close(); server = null; }
      port = null;
      tunnelUrl = null;
      tokens.clear();
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
