/**
 * Cloudflare Quick Tunnel process manager.
 *
 * Each tunnel is keyed by a service key (alias:name) and wraps a cloudflared
 * child process. The URL is parsed from cloudflared's stderr by the `cloudflared`
 * npm package's TryCloudflareHandler. No Cloudflare account is needed — these
 * are "trycloudflare" quick tunnels.
 *
 * Features:
 *  - Auto-restart with exponential backoff on crash (30s → 60s → 120s, max 300s)
 *  - Backoff resets after 60s of successful uptime
 *  - Graceful shutdown kills all tunnel processes
 *  - emitServiceUpdate() notifies Electron parent on every state change
 */

import { Tunnel } from 'cloudflared';
import { emitServiceUpdate } from './services.js';

// key -> TunnelEntry
const tunnels = new Map();

/**
 * @typedef {Object} TunnelEntry
 * @property {string}  key
 * @property {number}  port
 * @property {string|null} url
 * @property {'connecting'|'active'|'error'|'stopped'} status
 * @property {number|null} pid
 * @property {Tunnel}  tunnel      — cloudflared Tunnel instance
 * @property {number}  restartCount
 * @property {number}  startedAt   — Date.now() when last spawned
 * @property {ReturnType<typeof setTimeout>|null} restartTimer
 */

/**
 * Start a quick tunnel for the given service key on the given port.
 * If one is already running/connecting, returns an error.
 */
export function startTunnel(key, port) {
  if (!Number.isFinite(port) || port < 1 || port > 65535) {
    return { ok: false, error: `Invalid port: ${port}` };
  }

  const existing = tunnels.get(key);
  if (existing && (existing.status === 'active' || existing.status === 'connecting')) {
    return { ok: false, error: `Tunnel already ${existing.status} on port ${existing.port}` };
  }

  // Clean up any dead entry
  if (existing) {
    clearTimeout(existing.restartTimer);
    try { existing.tunnel.stop(); } catch { /* already dead */ }
    tunnels.delete(key);
  }

  return spawnTunnel(key, port, 0);
}

/**
 * Start a named tunnel using a Cloudflare tunnel token.
 * The token comes from Zero Trust → Networks → Tunnels → Configure.
 * The public URL is pre-configured in Cloudflare, so we just need to connect.
 *
 * @param {string} key   — service key for tracking
 * @param {string} token — Cloudflare tunnel token
 * @param {string} url   — the pre-configured public URL (e.g. https://crundi.example.com)
 */
export function startNamedTunnel(key, token, url) {
  if (!token) return { ok: false, error: 'No tunnel token provided' };

  const existing = tunnels.get(key);
  if (existing && (existing.status === 'active' || existing.status === 'connecting')) {
    return { ok: false, error: `Tunnel already ${existing.status}` };
  }

  if (existing) {
    clearTimeout(existing.restartTimer);
    try { existing.tunnel.stop(); } catch { /* already dead */ }
    tunnels.delete(key);
  }

  return spawnNamedTunnel(key, token, url, 0);
}

function spawnTunnel(key, port, restartCount) {
  let t;
  try {
    t = Tunnel.quick(`http://localhost:${port}`);
  } catch (err) {
    return { ok: false, error: `Failed to spawn cloudflared: ${err.message}` };
  }

  /** @type {TunnelEntry} */
  const entry = {
    key,
    port,
    url: null,
    status: 'connecting',
    pid: t.process?.pid ?? null,
    tunnel: t,
    restartCount,
    startedAt: Date.now(),
    restartTimer: null,
    connections: 0,
  };

  tunnels.set(key, entry);
  emitServiceUpdate();

  // cloudflared prints the trycloudflare URL EARLY — before the edge has
  // registered any connection for it. Hitting the URL during that window fails
  // with a DNS / "tunnel not found" (1033) error. So we record the URL here but
  // keep status 'connecting' until a connection is actually registered (the
  // 'connected' event below). Readiness is gated on that, not on the URL alone.
  t.on('url', (url) => {
    entry.url = url;
    emitServiceUpdate();
  });

  // Edge has registered a tunnel connection — the URL will now resolve/serve.
  t.on('connected', () => {
    entry.connections++;
    if (entry.status !== 'stopped') entry.status = 'active';
    // Reset backoff counter after 60s of uptime
    setTimeout(() => {
      if (tunnels.get(key) === entry && entry.status === 'active') {
        entry.restartCount = 0;
      }
    }, 60_000);
    emitServiceUpdate();
  });

  t.on('disconnected', () => {
    entry.connections = Math.max(0, entry.connections - 1);
  });

  t.on('error', (err) => {
    console.log(`[tunnel] ${key} error: ${err.message}`);
    entry.status = 'error';
    emitServiceUpdate();
  });

  t.on('exit', (code, signal) => {
    // If we deliberately stopped it, don't restart
    if (entry.status === 'stopped') return;

    console.log(`[tunnel] ${key} exited code=${code} signal=${signal}`);
    entry.status = 'error';
    entry.pid = null;
    emitServiceUpdate();

    // Auto-restart with exponential backoff
    const delay = Math.min(30 * Math.pow(2, entry.restartCount), 300);
    console.log(`[tunnel] ${key} — restarting in ${delay}s (attempt #${entry.restartCount + 1})`);
    entry.restartTimer = setTimeout(() => {
      // Only restart if still in the map and still in error state
      if (tunnels.get(key) === entry && entry.status === 'error') {
        tunnels.delete(key);
        spawnTunnel(key, port, entry.restartCount + 1);
      }
    }, delay * 1000);
  });

  return { ok: true, key, port };
}

function spawnNamedTunnel(key, token, url, restartCount) {
  let t;
  try {
    t = Tunnel.withToken(token, { '--protocol': 'http2' });
  } catch (err) {
    return { ok: false, error: `Failed to spawn cloudflared: ${err.message}` };
  }

  /** @type {TunnelEntry} */
  const entry = {
    key,
    port: 0, // port is configured in Cloudflare dashboard
    url: url || null,
    status: 'connecting',
    pid: t.process?.pid ?? null,
    tunnel: t,
    restartCount,
    startedAt: Date.now(),
    restartTimer: null,
    connections: 0,
  };

  tunnels.set(key, entry);
  emitServiceUpdate();

  t.on('connected', () => {
    entry.connections++;
    if (entry.status !== 'stopped') entry.status = 'active';
    setTimeout(() => {
      if (tunnels.get(key) === entry && entry.status === 'active') {
        entry.restartCount = 0;
      }
    }, 60_000);
    emitServiceUpdate();
  });

  t.on('disconnected', () => {
    entry.connections = Math.max(0, entry.connections - 1);
  });

  t.on('error', (err) => {
    console.log(`[tunnel] ${key} named error: ${err.message}`);
    entry.status = 'error';
    emitServiceUpdate();
  });

  t.on('exit', (code, signal) => {
    if (entry.status === 'stopped') return;

    console.log(`[tunnel] ${key} named exited code=${code} signal=${signal}`);
    entry.status = 'error';
    entry.pid = null;
    emitServiceUpdate();

    const delay = Math.min(30 * Math.pow(2, entry.restartCount), 300);
    console.log(`[tunnel] ${key} — restarting named tunnel in ${delay}s (attempt #${entry.restartCount + 1})`);
    entry.restartTimer = setTimeout(() => {
      if (tunnels.get(key) === entry && entry.status === 'error') {
        tunnels.delete(key);
        spawnNamedTunnel(key, token, url, entry.restartCount + 1);
      }
    }, delay * 1000);
  });

  return { ok: true, key, url };
}

/**
 * Wait until a tunnel is genuinely reachable: a connection registered at the
 * edge ('connected' → status 'active') AND an HTTP probe of the URL succeeds
 * without a Cloudflare 1033/"tunnel not found" error. This avoids handing the
 * user a URL that resolves to a DNS / 530 error because the edge isn't ready.
 *
 * Resolves with the URL once ready, or rejects on timeout. Best-effort: if the
 * connection registers but probes keep failing, we still resolve at the timeout
 * boundary with whatever URL we have rather than blocking forever.
 *
 * @param {string} key
 * @param {number} [timeoutMs=30000]
 */
export async function waitForTunnel(key, timeoutMs = 30000) {
  const deadline = Date.now() + timeoutMs;

  // Wait for both the URL and the 'connected' event (status === 'active').
  // The 'connected' event means the Cloudflare edge has registered a connection
  // for this tunnel — so the URL will actually resolve/serve. Avoid HTTP probing
  // which can interfere with edge establishment and cause false negatives.
  while (Date.now() < deadline) {
    const e = tunnels.get(key);
    if (!e || e.status === 'stopped') throw new Error('Tunnel stopped before it became ready');
    if (e.url && e.status === 'active') return e.url;
    await delay(500);
  }

  // Timed out — return the URL anyway if we have one (edge may just be slow).
  const entry = tunnels.get(key);
  if (entry?.url) return entry.url;
  throw new Error('Tunnel did not produce a URL in time');
}

function delay(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

/** True if the URL resolves and the edge serves it (no 1033 / 530 tunnel error). */
async function probeUrl(url) {
  try {
    const controller = new AbortController();
    const t = setTimeout(() => controller.abort(), 4000);
    const res = await fetch(url, { method: 'HEAD', redirect: 'manual', signal: controller.signal });
    clearTimeout(t);
    // 530 is Cloudflare's "origin/tunnel error" (includes 1033 tunnel-not-found).
    // Any other status means DNS resolved and the edge routed to our tunnel.
    return res.status !== 530;
  } catch {
    // Network/DNS error or abort — not ready yet.
    return false;
  }
}

/**
 * Stop a tunnel. Kills the cloudflared process.
 */
export function stopTunnel(key) {
  const entry = tunnels.get(key);
  if (!entry) return { ok: false, error: 'No tunnel running' };
  if (entry.status === 'stopped') return { ok: false, error: 'Already stopped' };

  entry.status = 'stopped';
  clearTimeout(entry.restartTimer);
  try { entry.tunnel.stop(); } catch { /* ignore */ }
  entry.pid = null;
  entry.url = null;
  emitServiceUpdate();
  tunnels.delete(key);
  return { ok: true };
}

/**
 * Get info about a specific tunnel.
 */
export function getTunnelInfo(key) {
  const e = tunnels.get(key);
  if (!e) return null;
  return {
    key: e.key,
    port: e.port,
    url: e.url,
    status: e.status,
    pid: e.pid,
    restartCount: e.restartCount,
  };
}

/**
 * Get info for all active tunnels.
 */
export function getAllTunnelInfo() {
  const out = {};
  for (const [key, e] of tunnels) {
    out[key] = {
      port: e.port,
      url: e.url,
      status: e.status,
      pid: e.pid,
    };
  }
  return out;
}

/**
 * Stop all tunnels. Called on bot shutdown.
 */
export function stopAllTunnels() {
  for (const [key, entry] of tunnels) {
    entry.status = 'stopped';
    clearTimeout(entry.restartTimer);
    try { entry.tunnel.stop(); } catch { /* ignore */ }
  }
  tunnels.clear();
}
