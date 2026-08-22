/**
 * limit-warmer.js — keep the 5-hour usage window pre-started.
 *
 * Claude's 5-hour allowance is a ROLLING window that begins on your first
 * message, not at a fixed clock time. So the moment you start real work you
 * also start the clock, and the window will expire five hours later — very
 * possibly in the middle of a long session.
 *
 * With this enabled, Crundi watches for the window lapsing while you are NOT
 * working and sends a single throwaway "hi" to open the next one immediately.
 * The window then advances on a predictable cadence, anchored to idle time
 * rather than to whenever you happened to sit down.
 *
 * Deliberately conservative, because it spends the user's quota unasked:
 *   - off unless explicitly enabled
 *   - never while any Claude session is working or waiting on input
 *   - never if real work has already opened a window since the last reset
 *     (that window is the user's own, and better than one we manufactured)
 *   - at most one ping per window, with a hard floor between attempts
 *   - cheapest model available, one word, no tools, no project context
 */

import { spawn } from 'child_process';
import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { config } from './config.js';
import { resolveClaudeBin } from './claude-ui.js';

const TICK_MS = 60_000;        // the user's "not checked for longer than a minute"
const MIN_GAP_MS = 10 * 60_000; // floor between pings, whatever else happens
const PING_TIMEOUT_MS = 90_000;

export function createLimitWarmer({ getUsage, isBusy, onChange } = {}) {
  const stateFile = () => join(config.dataDir, 'limit-warmer.json');

  let state = {
    enabled: false,
    lastKnownReset: null,  // ISO string of the reset we last observed
    lastActivityAt: 0,     // last moment any Claude session was doing work
    lastWarmAt: 0,
    lastWarmResult: '',
  };
  let timer = null;
  let warming = false;
  // config.dataDir is shared between a released Crundi and a --dev run on the
  // same machine (--dev isolates the port, not the data dir), so a dev instance
  // would load `enabled: true` from the same file, start its own loop, and spend
  // the user's quota alongside production. Dev never warms.
  const isDev = process.env.CRUNDI_DEV === '1';

  function load() {
    try {
      if (!existsSync(stateFile())) return;
      const d = JSON.parse(readFileSync(stateFile(), 'utf-8'));
      if (d && typeof d === 'object') state = { ...state, ...d };
    } catch { /* defaults are fine */ }
  }

  /**
   * Re-read only the fields another process could have moved. Belt and braces
   * alongside the dev guard: two instances sharing the file each held
   * lastWarmAt in memory, so the MIN_GAP_MS floor was per-process and both
   * could ping in the same minute.
   */
  function reloadShared() {
    try {
      if (!existsSync(stateFile())) return;
      const d = JSON.parse(readFileSync(stateFile(), 'utf-8'));
      if (!d || typeof d !== 'object') return;
      if (typeof d.lastWarmAt === 'number' && d.lastWarmAt > (state.lastWarmAt || 0)) {
        state.lastWarmAt = d.lastWarmAt;
        state.lastWarmResult = d.lastWarmResult || state.lastWarmResult;
      }
      if (d.lastKnownReset) state.lastKnownReset = d.lastKnownReset;
    } catch { /* keep what we have */ }
  }

  function save() {
    try {
      mkdirSync(config.dataDir, { recursive: true });
      writeFileSync(stateFile(), JSON.stringify(state));
    } catch { /* best effort */ }
  }

  load();

  /**
   * Any Claude session doing anything at all counts as real usage. Persisted
   * (throttled) because otherwise a restart resets it to 0 and the "real work
   * already opened a window" guard silently stops guarding.
   */
  let lastActivitySaveAt = 0;
  function markActivity() {
    const now = Date.now();
    state.lastActivityAt = now;
    if (now - lastActivitySaveAt > 30_000) { lastActivitySaveAt = now; save(); }
  }

  function setEnabled(on) {
    const next = !!on;
    if (next === state.enabled) return status();
    state.enabled = next;
    save();
    if (next && !isDev) schedule(); else stop();
    return status();
  }

  function status() {
    return {
      enabled: state.enabled,
      lastKnownReset: state.lastKnownReset,
      lastWarmAt: state.lastWarmAt || null,
      lastWarmResult: state.lastWarmResult || '',
      suspended: isDev,
    };
  }

  /**
   * Send one cheap message so the account opens a new 5-hour window.
   * Runs in a temp dir with no settings so it cannot pick up project hooks,
   * MCP servers or CLAUDE.md and turn a one-word ping into real work.
   */
  function ping() {
    return new Promise((resolve) => {
      const bin = resolveClaudeBin();
      if (!bin) return resolve({ ok: false, error: 'claude binary not found' });
      const cwd = join(tmpdir(), 'crundi-warm');
      try { mkdirSync(cwd, { recursive: true }); } catch { /* ignore */ }
      const proc = spawn(bin, [
        '-p', 'hi',
        '--model', 'haiku',
        '--setting-sources=',          // no user/project settings: no hooks, no MCP
        '--output-format', 'text',
      ], { cwd, stdio: ['ignore', 'pipe', 'pipe'] });

      let out = '', err = '', done = false;
      const finish = (r) => { if (done) return; done = true; clearTimeout(kill); resolve(r); };
      const kill = setTimeout(() => {
        try { proc.kill(); } catch { /* ignore */ }
        finish({ ok: false, error: 'timed out' });
      }, PING_TIMEOUT_MS);

      proc.stdout.on('data', d => { out += d; });
      proc.stderr.on('data', d => { err += d; });
      proc.on('error', e => finish({ ok: false, error: e.message }));
      proc.on('exit', code => finish(code === 0
        ? { ok: true, reply: out.trim().slice(0, 200) }
        : { ok: false, error: (err.trim() || `exit ${code}`).slice(0, 200) }));
    });
  }

  async function tick() {
    if (!state.enabled || warming) return;
    // Guard where the spending happens, not merely where the interval is set
    // up. Gating schedule() alone left tick() callable and a dev instance would
    // have spent real quota — caught by testing exactly that.
    if (isDev) return;
    // Claim the slot BEFORE the first await. Set later (just around the ping)
    // it guards nothing: the interval is 60s and a ping may take up to 90s, so
    // two ticks would sit in the awaits together, both find the window shut and
    // both spend a message. Observed exactly that in testing.
    warming = true;
    try {
      // Someone is mid-turn — their own work is opening the window.
      if (isBusy && isBusy()) { markActivity(); return; }
      reloadShared();

      const lastReset = state.lastKnownReset ? Date.parse(state.lastKnownReset) : 0;
      // Real work since the last reset already started a window. Leave it be.
      if (lastReset && state.lastActivityAt > lastReset) return;
      if (Date.now() - (state.lastWarmAt || 0) < MIN_GAP_MS) return;

      const usage = await getUsage({});        // cached ~60s inside usage.js
      if (!usage || !usage.ok) return;

      const resetsAt = usage.fiveHour && usage.fiveHour.resetsAt;
      const open = resetsAt && Date.parse(resetsAt) > Date.now();
      if (open) {
        // A window is already running; just remember when it ends.
        if (state.lastKnownReset !== resetsAt) { state.lastKnownReset = resetsAt; save(); }
        return;
      }

      const r = await ping();
      state.lastWarmAt = Date.now();
      state.lastWarmResult = r.ok ? 'started a new window' : ('failed: ' + r.error);
      console.log(`[limit-warmer] ${state.lastWarmResult}`);
      // Re-read so the new window's reset time is recorded, not the stale one.
      if (r.ok) {
        const fresh = await getUsage({ force: true });
        const na = fresh && fresh.ok && fresh.fiveHour && fresh.fiveHour.resetsAt;
        if (na) state.lastKnownReset = na;
      }
      save();
      if (onChange) { try { onChange(status()); } catch { /* ignore */ } }
    } catch (err) {
      console.warn('[limit-warmer] tick failed:', err?.message || err);
    } finally {
      warming = false;
    }
  }

  function schedule() {
    stop();
    timer = setInterval(tick, TICK_MS);
    if (timer.unref) timer.unref();
    tick();
  }

  function stop() { clearInterval(timer); timer = null; }

  if (state.enabled && !isDev) schedule();

  return { markActivity, setEnabled, status, stop, tick };
}
