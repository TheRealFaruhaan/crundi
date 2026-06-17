#!/usr/bin/env node
/**
 * claude-hook.js — Claude Code lifecycle hook for Crundi.
 *
 * Configured per-project in .claude/settings.local.json by claude-terminals.js.
 * Reports the originating terminal's agent state (working / needs-input / idle)
 * to the Crundi server so it can drive per-terminal badges, the project status
 * dot, and notifications.
 *
 * The state is passed as argv[2]. The terminal is identified by CRUNDI_TERMINAL_ID
 * (injected into each spawned terminal's environment — so two Claudes in the SAME
 * folder, sharing this settings file, still report distinct terminals). If those
 * env vars are absent (a Claude run NOT spawned by Crundi), this is a no-op.
 *
 * Hooks pipe a JSON payload on stdin (carries session_id, cwd, hook_event_name);
 * we drain it and forward session_id as a secondary correlation key.
 */

import http from 'node:http';

const state = process.argv[2] || 'idle';
const id = process.env.CRUNDI_TERMINAL_ID;
const apiUrl = process.env.CRUNDI_API_URL;
const apiKey = process.env.CRUNDI_API_KEY;

let input = '';
let done = false;

function finish() {
  if (done) return; done = true;
  // Not a Crundi-managed terminal → do nothing.
  if (!id || !apiUrl || !apiKey) { process.exit(0); return; }
  let payload = {};
  try { payload = JSON.parse(input || '{}'); } catch { /* ignore */ }
  const sessionId = payload.session_id || '';
  // Claude Code's Notification event (which we map to 'needs-input') fires for
  // BOTH a genuine permission/decision prompt AND when the input prompt simply
  // sits idle ~60s ("Claude is waiting for your input"). The latter is just an
  // idle agent between turns — not an actual request — and the Stop hook has
  // already reported 'idle' for it. Suppress that case so a waiting agent
  // doesn't keep pinging "needs your input".
  if (state === 'needs-input') {
    const m = String(payload.message || '').toLowerCase();
    if (m.includes('waiting for your input') || m.includes('waiting for input')) { process.exit(0); return; }
  }
  const body = JSON.stringify({ terminal: id, state, sessionId });
  try {
    const u = new URL('/api/terminal-status', apiUrl);
    const req = http.request(u, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Api-Key': apiKey, 'Content-Length': Buffer.byteLength(body) },
    }, (res) => { res.resume(); res.on('end', () => process.exit(0)); });
    req.on('error', () => process.exit(0));
    req.write(body); req.end();
  } catch { process.exit(0); }
  // Never block Claude for long.
  setTimeout(() => process.exit(0), 1500);
}

process.stdin.on('data', (c) => { input += c; });
process.stdin.on('end', finish);
process.stdin.on('error', finish);
// Fallback in case stdin never closes for some event.
setTimeout(finish, 400);
