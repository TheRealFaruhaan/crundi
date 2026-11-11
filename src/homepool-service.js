/**
 * homepool-service.js — homepool as Crundi serves it: the owner's routes, the
 * node socket, the MCP tools and the request → approval flow. The engine is
 * src/homepool.js, the record src/homepool-store.js; docs/homepool.md says why.
 *
 * Routes (all owner only: collaborators are refused by the access policy
 * before they get here, and every handler checks the role again):
 *   GET  /api/homepool              everything the owner's screen shows
 *   POST /api/homepool  {action}    changes (see ACTIONS below)
 *   WS   /api/homepool/node         a node, signed in with its node key
 *   GET  /api/homepool/node.mjs     the standalone node (public: it is code, not data)
 */

import http from 'node:http';
import https from 'node:https';
import tls from 'node:tls';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { randomBytes } from 'node:crypto';
import { WebSocketServer } from 'ws';
import { createHomepoolStore, SERVER_NETWORK, publicKey } from './homepool-store.js';
import { createHomepool } from './homepool.js';
import { nodeCode, NODE_PATH } from '../app/homepool-node.js';
import { checkChatToken } from './chat-token.js';
import { toSvg as qrSvg } from './qr.js';

const NODE_SCRIPT = fileURLToPath(new URL('../app/homepool-node.js', import.meta.url));
const REQUEST_WAIT_MS = 10 * 60 * 1000;
const SESSION_TTL_MS = 12 * 3600 * 1000;

export const HOMEPOOL_MCP_TOOLS = Object.freeze(['homepool_status', 'homepool_request', 'homepool_proxy', 'homepool_fetch']);

export function createHomepoolService({ dataDir, basePort, publicUrl = () => '', notifyOwner = () => {}, log = (...a) => console.log('[homepool]', ...a), chatInfo = () => null } = {}) {
  const store = createHomepoolStore({ dataDir });
  // Logins a chat got from homepool_proxy: in memory, tied to its key.
  const sessions = new Map();   // pass → { keyId, chatId, expiresAt }
  const engine = createHomepool({
    store, basePort, log,
    checkLogin: (keyId, pass) => {
      const s = sessions.get(pass);
      if (s && s.keyId === keyId && Date.now() < s.expiresAt) return store.keys().find(k => k.id === keyId) || null;
      return store.keyBySecret(keyId, pass);
    },
  });
  const nodeWss = new WebSocketServer({ noServer: true, maxPayload: 4 * 1024 * 1024 });
  nodeWss.on('connection', (ws, req) => engine.attachNode(ws, req.homepoolNode));

  // ── the node socket ──
  function handleUpgrade(req, socket, head) {
    const url = new URL(req.url, 'http://localhost');
    if (url.pathname !== NODE_PATH) return false;
    const auth = req.headers.authorization || '';
    const key = auth.startsWith('Bearer hpn_') ? auth.slice(7) : url.searchParams.get('key');
    const node = store.nodeByKey(key);
    if (!node) { socket.write('HTTP/1.1 401 Unauthorized\r\n\r\n'); socket.destroy(); return true; }
    req.homepoolNode = node;
    nodeWss.handleUpgrade(req, socket, head, (ws) => nodeWss.emit('connection', ws, req));
    return true;
  }

  /** Public, before sign-in: the standalone node's code. */
  function handlePublic(req, res, path) {
    if (path !== '/api/homepool/node.mjs' || req.method !== 'GET') return false;
    try {
      const body = readFileSync(NODE_SCRIPT);
      res.writeHead(200, { 'Content-Type': 'text/javascript; charset=utf-8', 'Cache-Control': 'no-cache' });
      res.end(body);
    } catch { res.writeHead(404); res.end(); }
    return true;
  }

  // ── what the owner is shown about one node / key ──
  function nodeSetup(id, key) {
    const server = publicUrl();
    const code = server ? nodeCode(server, key) : '';
    return {
      id, key, code, server,
      qr: code ? (() => { try { return qrSvg(code); } catch { return ''; } })() : '',
      docker: code ? `docker run -d --name crundi-homepool-node --restart unless-stopped -e HOMEPOOL_CODE='${code}' node:22-alpine sh -c 'wget -qO /n.mjs ${server}/api/homepool/node.mjs && exec node /n.mjs'` : '',
    };
  }

  function keyUsage(rec, secret = '') {
    const ov = engine.overview();
    const hosts = ov.ports.hosts;
    const docker = hosts.find(h => h === '172.17.0.1') || hosts.find(h => h !== '127.0.0.1') || '';
    const pw = secret || '<secret>';
    const out = {
      socks: `socks5h://${rec.id}:${pw}@127.0.0.1:${ov.ports.socks}`,
      http: `http://${rec.id}:${pw}@127.0.0.1:${ov.ports.http}`,
      status: `http://${rec.id}:${pw}@127.0.0.1:${ov.ports.http}/status`,
      login: `user ${rec.id} (add @<network> to pin a network, #<n> for slot n), password: the key's secret`,
      fromDocker: docker ? `host.docker.internal (with extra_hosts: ["host.docker.internal:host-gateway"]) or ${docker}` : '',
    };
    if (rec.portBlock && rec.ports) {
      out.block = {
        any: `socks5://host.docker.internal:${rec.ports.base}`,
        slots: `${rec.ports.base + 1}-${rec.ports.base + rec.slots} (slot N = ${rec.ports.base} + N)`,
        status: `http://host.docker.internal:${rec.ports.status}/`,
        from: rec.allowFrom,
        compose: [
          `PROXY_URL: socks5://host.docker.internal:${rec.ports.base}`,
          `PROXY_STATUS_URL: http://host.docker.internal:${rec.ports.status}/`,
          'extra_hosts: ["host.docker.internal:host-gateway"]',
        ],
      };
    }
    return out;
  }

  function overview() {
    const ov = engine.overview();
    ov.keys = ov.keys.map(k => ({ ...k, use: keyUsage(k) }));
    ov.publicUrl = publicUrl();
    return ov;
  }

  // ── the owner's changes ──
  async function act(body) {
    const a = String(body.action || '');
    const ok = (extra = {}) => ({ ok: true, ...extra, overview: overview() });
    switch (a) {
      case 'network-add': return ok({ id: store.addNetwork(body.name) });
      case 'network-rename': store.renameNetwork(String(body.id), body.name); return ok();
      case 'network-remove': {
        const id = String(body.id);
        const moved = store.nodes().filter(n => n.network === id).map(n => n.id);
        store.removeNetwork(id);
        for (const n of moved) engine.nodeChanged(n);
        for (const k of store.keys()) engine.keyChanged(k.id);
        return ok();
      }
      case 'node-add': {
        const r = store.addNode({ name: body.name, network: String(body.network || ''), kind: body.kind === 'crundi-app' ? 'crundi-app' : 'standalone' });
        return ok({ node: nodeSetup(r.id, r.key) });
      }
      // This device, from the Crundi app or desktop app: its own node, made or
      // given a fresh key (the old one stops working).
      case 'self-node': {
        const deviceId = String(body.deviceId || '').slice(0, 80);
        if (!deviceId) throw new Error('deviceId is required.');
        const existing = store.nodes().find(n => n.deviceId === deviceId);
        let r;
        if (existing) {
          r = store.reissueNode(existing.id);
          if (body.network != null) store.updateNode(existing.id, { network: String(body.network) });
          if (body.name) store.updateNode(existing.id, { name: body.name });
          engine.nodeChanged(existing.id, { kick: true });
        } else {
          r = store.addNode({ name: body.name || 'This device', network: String(body.network || ''), kind: 'crundi-app', deviceId });
        }
        return ok({ node: nodeSetup(r.id, r.key) });
      }
      case 'node-update': {
        store.updateNode(String(body.id), { name: body.name, network: body.network, enabled: body.enabled });
        engine.nodeChanged(String(body.id));
        return ok();
      }
      case 'node-reissue': {
        const r = store.reissueNode(String(body.id));
        engine.nodeChanged(r.id, { kick: true });
        return ok({ node: nodeSetup(r.id, r.key) });
      }
      case 'node-remove': store.removeNode(String(body.id)); engine.nodeChanged(String(body.id)); return ok();
      case 'key-add': {
        const r = store.addKey(body.key || {}, { by: 'owner' });
        engine.keyChanged(r.key.id);
        const rec = store.keys().find(k => k.id === r.key.id);
        return ok({ key: publicKey(rec), secret: r.secret, use: keyUsage(rec, r.secret) });
      }
      case 'key-update': {
        store.updateKey(String(body.id), body.key || {});
        engine.keyChanged(String(body.id));
        return ok();
      }
      case 'key-reissue': {
        const r = store.reissueKey(String(body.id));
        for (const [p, s] of sessions) if (s.keyId === r.key.id) sessions.delete(p);
        engine.keyChanged(r.key.id);
        return ok({ key: publicKey(r.key), secret: r.secret, use: keyUsage(r.key, r.secret) });
      }
      case 'key-remove': {
        const id = String(body.id);
        store.removeKey(id);
        for (const [p, s] of sessions) if (s.keyId === id) sessions.delete(p);
        engine.keyChanged(id);
        return ok();
      }
      case 'request-decide': {
        const r = store.decideRequest(String(body.id), { approve: !!body.approve, changes: body.changes || {} });
        if (r.key) engine.keyChanged(r.key.id);
        return ok({ request: r.request });
      }
      case 'settings': {
        store.setSettings(body.settings || {});
        engine.pollPool();
        return ok();
      }
      default: throw new Error(`Unknown action "${a}".`);
    }
  }

  async function handleApi(req, res, path, { principal, readBody, json, isOwner }) {
    if (!path.startsWith('/api/homepool')) return false;
    if (!isOwner(principal)) { json(res, { ok: false, error: 'Only the owner manages homepool.' }, 403); return true; }
    if (path === '/api/homepool' && req.method === 'GET') { json(res, { ok: true, ...overview() }); return true; }
    if (path === '/api/homepool' && req.method === 'POST') {
      let body = {};
      try { body = JSON.parse(await readBody(req) || '{}'); } catch { json(res, { ok: false, error: 'Bad JSON.' }, 400); return true; }
      try { json(res, await act(body)); }
      catch (err) { json(res, { ok: false, error: err.message }, 400); }
      return true;
    }
    json(res, { ok: false, error: 'Not found.' }, 404);
    return true;
  }

  // ── MCP ──
  /** Which chat is this, really? Null if the bridge's token does not match. */
  function chatOf(args) {
    const chatId = String(args.sessionId || '');
    const project = String(args.project || '').toLowerCase();
    if (!chatId) return { error: 'homepool is for Crundi chats: this call does not come from one.' };
    if (!checkChatToken(chatId, project, args.chatToken)) return { error: 'This chat could not be verified (no chat token). Start a new chat in Crundi and try again.' };
    return { chatId, project };
  }

  const networkList = () => [SERVER_NETWORK, ...store.networks().map(n => n.id), 'pool']
    .map(id => ({ id, name: engine.networkName(id) }));

  async function mcp(tool, args = {}) {
    const who = chatOf(args);
    if (who.error) return { ok: false, error: who.error };
    const key = store.keyForChat(who.chatId, who.project);
    const refusal = key ? store.keyRefusal(key) : 'not granted';
    const notGranted = {
      ok: false, granted: false,
      error: key ? `This chat's homepool key is unusable: ${refusal}. The owner can change it in Settings › Homepool.`
        : 'This chat has no homepool access. Ask for it with homepool_request (the owner approves it).',
      networks: networkList(),
    };

    if (tool === 'homepool_status') {
      const base = { networks: networkList(), pending: store.requests().filter(r => r.status === 'pending' && r.chatId === who.chatId).map(r => ({ id: r.id, kind: r.kind, networks: r.networks })) };
      if (!key) return { ok: true, granted: false, ...base, note: 'No access yet. Use homepool_request to ask the owner.' };
      return { ok: true, granted: !refusal, refusal: refusal || undefined, key: { id: key.id, name: key.name, kind: key.kind, networks: key.networks, fallbackToServer: key.fallbackToServer, expiresAt: key.expiresAt, dailyCapMb: key.dailyCapMb }, status: engine.statusFor(key), ...base };
    }

    if (tool === 'homepool_request') {
      const kind = ['chat', 'project', 'software'].includes(args.kind) ? args.kind : 'chat';
      const known = new Set(networkList().map(n => n.id));
      const nets = (Array.isArray(args.networks) && args.networks.length ? args.networks : [SERVER_NETWORK]).map(String).filter(n => known.has(n));
      if (!nets.length) return { ok: false, error: `Unknown network. Known: ${[...known].join(', ')}` };
      const r = store.addRequest({ chatId: who.chatId, project: who.project, kind, name: String(args.name || ''), networks: nets, reason: String(args.reason || '') });
      const chat = chatInfo(who.chatId) || {};
      try {
        notifyOwner({
          title: 'Homepool: a chat asks for access',
          body: `${chat.title || 'A chat'}${who.project ? ` (${who.project})` : ''} asks for ${kind === 'software' ? `a software key "${r.name || 'software'}"` : kind === 'project' ? 'access for its whole project' : 'access'} through ${nets.map(n => engine.networkName(n)).join(', ')}.${r.reason ? ` Why: ${r.reason}` : ''} Approve or refuse in Settings › Homepool.`,
          chat: { id: who.chatId, project: who.project, title: chat.title || '' },
        });
      } catch { /* the request is still listed */ }
      if (args.wait === false) return { ok: true, status: 'pending', requestId: r.id, note: 'Sent to the owner. Call homepool_status later to see whether it was approved.' };
      const t0 = Date.now();
      while (Date.now() - t0 < REQUEST_WAIT_MS) {
        const cur = store.requests().find(x => x.id === r.id);
        if (!cur || cur.status !== 'pending') {
          if (!cur || cur.status === 'refused') return { ok: false, status: 'refused', error: 'The owner refused the request.' };
          const k = store.keys().find(x => x.id === cur.keyId);
          const out = { ok: true, status: 'approved', key: k ? { id: k.id, name: k.name, kind: k.kind, networks: k.networks, fallbackToServer: k.fallbackToServer, expiresAt: k.expiresAt, dailyCapMb: k.dailyCapMb } : null };
          if (k && k.kind === 'software') {
            const secret = store.takeApprovedSecret(r.id);
            out.secret = secret;
            out.use = keyUsage(k, secret);
            out.note = 'This secret is shown once. Keep it with the software (for example in a secret or its .env), not in a file you commit.';
          } else {
            out.note = 'Use homepool_proxy for a proxy URL, or homepool_fetch for a single request.';
          }
          return out;
        }
        await new Promise(res => setTimeout(res, 1500));
      }
      return { ok: true, status: 'pending', requestId: r.id, note: 'The owner has not answered yet. It stays in his list; call homepool_status later.' };
    }

    if (!key || refusal) return notGranted;

    if (tool === 'homepool_proxy') {
      const net = String(args.network || '');
      if (net && !key.networks.includes(net)) return { ok: false, error: `This chat's key has no access to "${net}". It may use: ${key.networks.join(', ')}.` };
      const pass = randomBytes(18).toString('base64url');
      sessions.set(pass, { keyId: key.id, chatId: who.chatId, expiresAt: Date.now() + SESSION_TTL_MS });
      for (const [p, s] of sessions) if (Date.now() >= s.expiresAt) sessions.delete(p);
      const user = `${key.id}${net ? '@' + net : ''}${args.slot ? '#' + Number(args.slot) : ''}`;
      const ov = engine.overview();
      return {
        ok: true,
        socks: `socks5h://${encodeURIComponent(user)}:${pass}@127.0.0.1:${ov.ports.socks}`,
        http: `http://${encodeURIComponent(user)}:${pass}@127.0.0.1:${ov.ports.http}`,
        networks: key.networks, expiresInHours: SESSION_TTL_MS / 3600000,
        note: 'For curl: --proxy <socks>. For Playwright/Chromium use the http one (Chromium cannot send a SOCKS login). Works on this machine only; ends when the owner revokes the key.',
      };
    }

    if (tool === 'homepool_fetch') return fetchVia(key, args);
    return { ok: false, error: `Unknown tool ${tool}` };
  }

  async function fetchVia(key, args) {
    let u;
    try { u = new URL(String(args.url || '')); } catch { return { ok: false, error: 'url must be a full http(s) URL.' }; }
    if (!/^https?:$/.test(u.protocol)) return { ok: false, error: 'Only http and https.' };
    const port = Number(u.port) || (u.protocol === 'https:' ? 443 : 80);
    const net = String(args.network || '');
    let opened;
    try { opened = await engine.open(key, { host: u.hostname, port, network: net, slot: Number(args.slot) || 0 }); }
    catch (err) { return { ok: false, error: err.message }; }
    const { upstream, exit } = opened;
    const sock = u.protocol === 'https:' ? tls.connect({ socket: upstream, servername: u.hostname }) : upstream;
    const timeoutMs = Math.min(120000, Number(args.timeoutMs) || 30000);
    const mod = u.protocol === 'https:' ? https : http;
    return new Promise((resolve) => {
      const headers = { 'user-agent': 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130 Safari/537.36', ...(args.headers || {}) };
      const req = mod.request({ method: String(args.method || 'GET').toUpperCase(), host: u.hostname, path: u.pathname + u.search, headers, createConnection: () => sock, timeout: timeoutMs });
      const chunks = []; let size = 0;
      const limit = 512 * 1024;
      req.on('response', (res) => {
        res.on('data', (d) => { size += d.length; if (size <= limit) chunks.push(d); });
        res.on('end', () => {
          store.addUsage({ keyId: key.id, nodeId: exit.kind === 'node' ? exit.link.node.id : '', bytes: size });
          const buf = Buffer.concat(chunks);
          const type = String(res.headers['content-type'] || '');
          const text = /^(text\/|application\/(json|xml|javascript|x-www-form-urlencoded))|charset=/i.test(type) || !type;
          resolve({
            ok: true, status: res.statusCode, headers: res.headers,
            exit: exit.kind === 'node' ? `${exit.link.node.name} (${engine.networkName(exit.network)})` : engine.networkName(exit.network),
            truncated: size > limit, bytes: size,
            ...(text ? { body: buf.toString('utf8') } : { bodyBase64: buf.toString('base64') }),
          });
        });
      });
      req.on('timeout', () => req.destroy(new Error('timed out')));
      req.on('error', (e) => resolve({ ok: false, error: e.message }));
      if (args.body != null) req.write(typeof args.body === 'string' ? args.body : JSON.stringify(args.body));
      req.end();
    });
  }

  return {
    store, engine, handleUpgrade, handlePublic, handleApi, mcp, overview,
    start: () => engine.start(), stop: () => engine.stop(),
  };
}
