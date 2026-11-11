/**
 * homepool-store.js — what homepool remembers: networks, nodes, access keys,
 * requests from chats, usage and the change log. See docs/homepool.md.
 *
 * Nothing here decides a route; src/homepool.js does. This is only the record,
 * kept in <dataDir>/homepool.json (mode 600). Secrets are stored as SHA-256
 * hashes: a node key or an access key is shown once, when it is made.
 *
 * Only the owner changes this (the routes check the role). A chat can add a
 * REQUEST, never a key: a key is made when the owner approves.
 */

import { readFileSync, writeFileSync, renameSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { randomBytes, createHash, timingSafeEqual } from 'node:crypto';

/** Built in, never stored: the server's own connection. */
export const SERVER_NETWORK = 'server';
/** Built in while the standalone homepool relay runs: its nodes, as one network. */
export const POOL_NETWORK = 'pool';
export const BUILTIN_NETWORKS = Object.freeze([SERVER_NETWORK, POOL_NETWORK]);

const LOG_KEEP = 500;
const ID_RE = /^[a-z0-9][a-z0-9_-]{0,39}$/;

const hash = (s) => createHash('sha256').update(String(s)).digest('hex');
const sameHash = (a, b) => {
  const x = Buffer.from(String(a || ''), 'hex');
  const y = Buffer.from(String(b || ''), 'hex');
  return x.length === 32 && x.length === y.length && timingSafeEqual(x, y);
};
const rid = (n = 6) => randomBytes(n).toString('hex');
const today = (now = Date.now()) => new Date(now).toISOString().slice(0, 10);

/** "Mum's flat" → "mums-flat"; unique against `taken`. */
export function slugify(name, taken = new Set()) {
  let base = String(name || '').toLowerCase().normalize('NFKD').replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 30) || 'net';
  if (BUILTIN_NETWORKS.includes(base)) base = `${base}-1`;
  let id = base;
  for (let i = 2; taken.has(id); i++) id = `${base}-${i}`;
  return id;
}

export function createHomepoolStore({ dataDir, now = () => Date.now(), file } = {}) {
  const FILE = file || join(dataDir, 'homepool.json');
  let d = blank();

  function blank() {
    return {
      version: 1,
      settings: {
        bindTtlMs: 6 * 3600 * 1000,
        poolBridge: { enabled: true, statusUrl: 'http://127.0.0.1:1099/', relayHost: '', relayPort: 1080 },
      },
      networks: [{ id: 'home', name: 'Home', createdAt: now() }],
      nodes: [],
      keys: [],
      requests: [],
      usage: { day: today(now()), keys: {}, nodes: {} },
      log: [],
    };
  }

  try {
    const raw = JSON.parse(readFileSync(FILE, 'utf8'));
    d = { ...blank(), ...raw, settings: { ...blank().settings, ...(raw.settings || {}), poolBridge: { ...blank().settings.poolBridge, ...(raw.settings?.poolBridge || {}) } } };
  } catch { /* first run */ }

  let saveTimer = null;
  function save(immediate = false) {
    const write = () => {
      saveTimer = null;
      try {
        mkdirSync(dataDir || join(FILE, '..'), { recursive: true });
        writeFileSync(`${FILE}.tmp`, JSON.stringify(d, null, 2), { mode: 0o600 });
        renameSync(`${FILE}.tmp`, FILE);
      } catch (err) { console.error('[homepool] could not save:', err.message); }
    };
    if (immediate) { clearTimeout(saveTimer); return write(); }
    if (!saveTimer) saveTimer = setTimeout(write, 1000);
    saveTimer.unref?.();
  }

  function log(what, fields = {}) {
    d.log.push({ at: now(), what, ...fields });
    if (d.log.length > LOG_KEEP) d.log.splice(0, d.log.length - LOG_KEEP);
  }

  function rollDay() {
    const t = today(now());
    if (d.usage.day !== t) d.usage = { day: t, keys: {}, nodes: {} };
  }

  // ── networks ──
  const networkIds = () => new Set([...BUILTIN_NETWORKS, ...d.networks.map(n => n.id)]);
  function addNetwork(name) {
    const n = String(name || '').trim();
    if (!n) throw new Error('A network needs a name.');
    const id = slugify(n, networkIds());
    d.networks.push({ id, name: n.slice(0, 60), createdAt: now() });
    log('network-added', { network: id });
    save(true);
    return id;
  }
  function renameNetwork(id, name) {
    const net = d.networks.find(n => n.id === id);
    if (!net) throw new Error('No such network.');
    net.name = String(name || '').trim().slice(0, 60) || net.name;
    log('network-renamed', { network: id });
    save(true);
  }
  function removeNetwork(id) {
    const i = d.networks.findIndex(n => n.id === id);
    if (i < 0) throw new Error('No such network.');
    d.networks.splice(i, 1);
    // Its nodes are kept, unassigned (they carry nothing until moved); keys
    // simply lose it.
    for (const n of d.nodes) if (n.network === id) n.network = '';
    for (const k of d.keys) k.networks = k.networks.filter(x => x !== id);
    log('network-removed', { network: id });
    save(true);
  }

  // ── nodes ──
  /** A new node: returns its key once. `network` may be '' (unassigned). */
  function addNode({ name, network = '', kind = 'standalone', deviceId = '' } = {}) {
    const n = String(name || '').trim().slice(0, 60);
    if (!n) throw new Error('A node needs a name.');
    if (network && !d.networks.some(x => x.id === network)) throw new Error('No such network.');
    const id = `n${rid(5)}`;
    const secret = randomBytes(24).toString('base64url');
    d.nodes.push({ id, name: n, network, kind, deviceId: String(deviceId || ''), keyHash: hash(secret), createdAt: now(), lastSeenAt: 0, enabled: true });
    log('node-added', { node: id, network });
    save(true);
    return { id, key: `hpn_${id}.${secret}` };
  }
  function nodeByKey(key) {
    const m = /^hpn_(n[0-9a-f]+)\.([A-Za-z0-9_-]+)$/.exec(String(key || ''));
    if (!m) return null;
    const node = d.nodes.find(n => n.id === m[1]);
    return node && node.enabled !== false && sameHash(node.keyHash, hash(m[2])) ? node : null;
  }
  function updateNode(id, patch = {}) {
    const node = d.nodes.find(n => n.id === id);
    if (!node) throw new Error('No such node.');
    if (patch.name != null) node.name = String(patch.name).trim().slice(0, 60) || node.name;
    if (patch.network != null) {
      if (patch.network && !d.networks.some(x => x.id === patch.network)) throw new Error('No such network.');
      if (node.network !== patch.network) log('node-moved', { node: id, from: node.network, to: patch.network });
      node.network = patch.network;
    }
    if (patch.enabled != null) { node.enabled = !!patch.enabled; log(node.enabled ? 'node-enabled' : 'node-disabled', { node: id }); }
    save(true);
    return node;
  }
  function reissueNode(id) {
    const node = d.nodes.find(n => n.id === id);
    if (!node) throw new Error('No such node.');
    const secret = randomBytes(24).toString('base64url');
    node.keyHash = hash(secret);
    log('node-key-reissued', { node: id });
    save(true);
    return { id, key: `hpn_${id}.${secret}` };
  }
  function removeNode(id) {
    const i = d.nodes.findIndex(n => n.id === id);
    if (i < 0) throw new Error('No such node.');
    d.nodes.splice(i, 1);
    log('node-removed', { node: id });
    save(true);
  }
  function seenNode(id, fields = {}) {
    const node = d.nodes.find(n => n.id === id);
    if (!node) return;
    node.lastSeenAt = now();
    if (fields.platform) node.platform = String(fields.platform).slice(0, 40);
    save();
  }

  // ── access keys ──
  function cleanKeyInput(k, prev = {}) {
    const kind = k.kind ?? prev.kind;
    if (!['chat', 'project', 'software'].includes(kind)) throw new Error('kind must be chat, project or software.');
    const nets = Array.isArray(k.networks) ? k.networks : (prev.networks || [SERVER_NETWORK]);
    const known = networkIds();
    const networks = [...new Set(nets.map(String))].filter(n => known.has(n));
    if (!networks.length) throw new Error('Give the key at least one network.');
    const out = {
      kind,
      name: String(k.name ?? prev.name ?? '').trim().slice(0, 60),
      chatId: kind === 'chat' ? String(k.chatId ?? prev.chatId ?? '') : '',
      project: kind === 'project' || kind === 'chat' ? String(k.project ?? prev.project ?? '').toLowerCase() : '',
      networks,
      fallbackToServer: !!(k.fallbackToServer ?? prev.fallbackToServer),
      enabled: k.enabled == null ? (prev.enabled ?? true) : !!k.enabled,
      expiresAt: k.expiresAt === null ? null : (Number(k.expiresAt ?? prev.expiresAt) || null),
      dailyCapMb: Math.max(0, Number(k.dailyCapMb ?? prev.dailyCapMb ?? 0) || 0),
      slots: Math.min(32, Math.max(0, Number(k.slots ?? prev.slots ?? 8) || 0)),
      // A port block: for software that cannot send a proxy login. Only the
      // sources listed in allowFrom may use it.
      portBlock: !!(k.portBlock ?? prev.portBlock),
      allowFrom: (Array.isArray(k.allowFrom) ? k.allowFrom : (prev.allowFrom || [])).map(s => String(s).trim()).filter(Boolean).slice(0, 10),
    };
    if (kind === 'chat' && !out.chatId) throw new Error('A chat key needs the chat.');
    if (kind === 'project' && !out.project) throw new Error('A project key needs the project.');
    if (!out.name) out.name = kind === 'software' ? 'software' : (out.project || out.chatId);
    if (out.portBlock && !out.allowFrom.length) out.allowFrom = ['loopback'];
    return out;
  }

  function addKey(input, { by = 'owner', requestId = '' } = {}) {
    const k = cleanKeyInput(input);
    const id = `k${rid(5)}`;
    const secret = randomBytes(18).toString('base64url');
    const rec = { id, ...k, secretHash: hash(secret), createdAt: now(), createdBy: by, requestId, ports: null };
    d.keys.push(rec);
    log('key-added', { key: id, by, networks: k.networks });
    save(true);
    return { key: rec, secret };
  }
  function updateKey(id, patch) {
    const rec = d.keys.find(k => k.id === id);
    if (!rec) throw new Error('No such key.');
    Object.assign(rec, cleanKeyInput(patch, rec));
    log('key-changed', { key: id });
    save(true);
    return rec;
  }
  function reissueKey(id) {
    const rec = d.keys.find(k => k.id === id);
    if (!rec) throw new Error('No such key.');
    const secret = randomBytes(18).toString('base64url');
    rec.secretHash = hash(secret);
    log('key-reissued', { key: id });
    save(true);
    return { key: rec, secret };
  }
  function removeKey(id) {
    const i = d.keys.findIndex(k => k.id === id);
    if (i < 0) throw new Error('No such key.');
    d.keys.splice(i, 1);
    log('key-revoked', { key: id });
    save(true);
  }
  function setKeyPorts(id, ports) {
    const rec = d.keys.find(k => k.id === id);
    if (rec) { rec.ports = ports; save(); }
  }
  /** Is this key usable right now? Returns a reason when not. */
  function keyRefusal(rec) {
    if (!rec) return 'no such key';
    if (rec.enabled === false) return 'switched off by the owner';
    if (rec.expiresAt && now() >= rec.expiresAt) return 'expired';
    if (rec.dailyCapMb > 0) {
      rollDay();
      const u = d.usage.keys[rec.id];
      if (u && u.bytes >= rec.dailyCapMb * 1024 * 1024) return `today's data cap (${rec.dailyCapMb} MB) is used up`;
    }
    return '';
  }
  function keyBySecret(id, secret) {
    const rec = d.keys.find(k => k.id === id);
    return rec && sameHash(rec.secretHash, hash(secret)) ? rec : null;
  }
  /** The key a chat may use: its own, else its project's. */
  function keyForChat(chatId, project) {
    const live = (k) => !keyRefusal(k);
    return d.keys.find(k => k.kind === 'chat' && chatId && k.chatId === chatId && live(k))
      || d.keys.find(k => k.kind === 'project' && project && k.project === project && live(k))
      || d.keys.find(k => k.kind === 'chat' && chatId && k.chatId === chatId)
      || d.keys.find(k => k.kind === 'project' && project && k.project === project)
      || null;
  }

  // ── requests (a chat asks; the owner decides) ──
  function addRequest({ chatId = '', project = '', kind = 'chat', name = '', networks = [SERVER_NETWORK], reason = '' } = {}) {
    if (!['chat', 'project', 'software'].includes(kind)) kind = 'chat';
    const pending = d.requests.find(r => r.status === 'pending' && r.chatId === chatId && r.kind === kind && r.name === String(name || ''));
    if (pending) return pending;
    const r = {
      id: `r${rid(5)}`, status: 'pending', chatId: String(chatId), project: String(project), kind,
      name: String(name || '').slice(0, 60), networks: [...new Set((networks || []).map(String))],
      reason: String(reason || '').slice(0, 500), createdAt: now(), decidedAt: 0, keyId: '',
    };
    d.requests.push(r);
    // Old decided ones go; pending ones stay until answered.
    const decided = d.requests.filter(x => x.status !== 'pending');
    if (decided.length > 100) d.requests = d.requests.filter(x => x.status === 'pending' || decided.slice(-100).includes(x));
    log('request', { request: r.id, chatId, project, kind });
    save(true);
    return r;
  }
  function decideRequest(id, { approve, changes = {} } = {}) {
    const r = d.requests.find(x => x.id === id);
    if (!r) throw new Error('No such request.');
    if (r.status !== 'pending') throw new Error(`That request was already ${r.status}.`);
    r.decidedAt = now();
    if (!approve) {
      r.status = 'refused';
      log('request-refused', { request: id });
      save(true);
      return { request: r };
    }
    const kind = changes.kind || r.kind;
    const made = addKey({
      kind, name: changes.name ?? (r.name || (kind === 'software' ? 'software' : '')),
      chatId: r.chatId, project: r.project,
      networks: changes.networks || r.networks,
      ...pick(changes, ['fallbackToServer', 'expiresAt', 'dailyCapMb', 'slots', 'portBlock', 'allowFrom']),
    }, { by: 'owner (approved a request)', requestId: id });
    r.status = 'approved';
    r.keyId = made.key.id;
    // The secret of an approved request is handed to the asking chat once,
    // and then forgotten (see takeApprovedSecret).
    pendingSecrets.set(id, made.secret);
    log('request-approved', { request: id, key: made.key.id });
    save(true);
    return { request: r, ...made };
  }
  const pendingSecrets = new Map();
  function takeApprovedSecret(requestId) {
    const s = pendingSecrets.get(requestId);
    pendingSecrets.delete(requestId);
    return s || '';
  }

  // ── usage ──
  function addUsage({ keyId = '', nodeId = '', bytes = 0, conn = false, refused = false }) {
    rollDay();
    if (keyId) {
      const u = d.usage.keys[keyId] || (d.usage.keys[keyId] = { bytes: 0, conns: 0, refused: 0, lastUsedAt: 0 });
      u.bytes += bytes; if (conn) u.conns++; if (refused) u.refused++;
      u.lastUsedAt = now();
    }
    if (nodeId) {
      const u = d.usage.nodes[nodeId] || (d.usage.nodes[nodeId] = { bytes: 0, conns: 0 });
      u.bytes += bytes; if (conn) u.conns++;
    }
    save();
  }

  function setSettings(patch = {}) {
    if (patch.bindTtlMs != null) d.settings.bindTtlMs = Math.max(60000, Number(patch.bindTtlMs) || d.settings.bindTtlMs);
    if (patch.poolBridge) d.settings.poolBridge = { ...d.settings.poolBridge, ...pick(patch.poolBridge, ['enabled', 'statusUrl', 'relayHost', 'relayPort']) };
    log('settings-changed');
    save(true);
  }

  return {
    file: FILE,
    get data() { return d; },
    settings: () => d.settings,
    networks: () => d.networks,
    nodes: () => d.nodes,
    keys: () => d.keys,
    requests: () => d.requests,
    usage: () => { rollDay(); return d.usage; },
    recentLog: (n = 100) => d.log.slice(-n),
    networkIds, addNetwork, renameNetwork, removeNetwork,
    addNode, nodeByKey, updateNode, reissueNode, removeNode, seenNode,
    addKey, updateKey, reissueKey, removeKey, setKeyPorts, keyRefusal, keyBySecret, keyForChat,
    addRequest, decideRequest, takeApprovedSecret,
    addUsage, setSettings, log, save,
  };
}

function pick(o, keys) {
  const out = {};
  for (const k of keys) if (o && o[k] !== undefined) out[k] = o[k];
  return out;
}

/** What may be shown of a key (never its hash). */
export function publicKey(k) {
  if (!k) return null;
  const { secretHash, ...rest } = k;
  return rest;
}
export function publicNode(n) {
  if (!n) return null;
  const { keyHash, ...rest } = n;
  return rest;
}

export { ID_RE };
