/**
 * panes.js — parking workbench panes, and bringing them back in place.
 *
 * A parked pane has no process: its chat or terminal was closed, but the pane
 * keeps its id, so the workbench layout (keyed by id) keeps its slot, size and
 * pin. It shows what it was (the chat transcript, or the terminal's last
 * screen) behind a Resume button, and Resume starts the same conversation
 * again under the same id.
 *
 * Two things park a pane:
 *   - idleness: nothing going on in it AND nobody at Crundi, both for the
 *     configured timeout (Settings; 0 = off, otherwise at least 10 minutes).
 *     Plain shells are never parked for idleness — Crundi cannot tell whether
 *     one is running a dev server.
 *   - a restart: every pane open when Crundi stopped (cleanly or not) comes
 *     back parked.
 *
 * For the restart case the record has to be on disk BEFORE anything goes
 * wrong, so live panes are written down continuously (on a timer and on
 * shutdown), and at boot whatever was still marked live becomes parked.
 */

import { readFileSync, writeFileSync, renameSync, existsSync } from 'fs';
import { join } from 'path';

const SCREEN_MAX = 60_000;          // terminal scrollback kept for the preview
const TICK_MS = 30_000;             // how often live panes are written down / idleness checked
export const MIN_TIMEOUT_MIN = 10;
export const DEFAULT_TIMEOUT_MIN = 60;

/**
 * @param {object} o
 * @param {string} o.dataDir
 * @param {object} o.claudeTerminals          terminal manager (meta, getScrollback, create, close, list)
 * @param {object} o.claudeUi                 chat manager (meta, create, close, list)
 * @param {(cwd:string, uuid:string, opts:object) => any} o.readTranscriptHistory
 * @param {(alias:string) => any} o.getProject
 * @param {() => number} o.timeoutMinutes     current setting (0 = off)
 * @param {(ms:number) => boolean} o.awayFor  true if nobody has been at Crundi for ms
 * @param {(id:string) => string} o.terminalState  hook-reported state of a terminal
 * @param {() => void} o.onChange             broadcast state after a park / resume
 */
export function createPanes(o) {
  const FILE = join(o.dataDir, 'panes.json');
  // id -> { id, project, kind: 'ui'|'terminal', title, order, sessionId, opts, status: 'live'|'parked',
  //         parkedAt, reason: 'idle'|'restart', screen, cwd }
  let records = load();
  const idleSince = new Map();   // id -> ms since nothing has been going on in it
  let lastWritten = '';
  let timer = null;

  function load() {
    try {
      if (existsSync(FILE)) {
        const d = JSON.parse(readFileSync(FILE, 'utf-8'));
        if (d && typeof d === 'object' && d.panes && typeof d.panes === 'object') return d.panes;
      }
    } catch { /* a bad file means nothing to restore, not a failed boot */ }
    return {};
  }

  function save() {
    const body = JSON.stringify({ v: 1, panes: records });
    if (body === lastWritten) return;
    try {
      const tmp = FILE + '.tmp';
      writeFileSync(tmp, body);
      renameSync(tmp, FILE);
      lastWritten = body;
    } catch (err) {
      console.warn('[panes] Could not save pane records:', err.message);
    }
  }

  /** Is this live pane something we restore at all? */
  function restorableTerminal(m) { return !!(m && m.restorable); }
  function restorableChat(m) { return !!(m && !m.background && !m.collaborator); }

  /** Write down every live pane as it is now (status 'live'). */
  function snapshot() {
    const seen = new Set();
    for (const t of (o.claudeTerminals?.list() || [])) {
      const m = o.claudeTerminals.meta(t.id);
      if (!restorableTerminal(m) || !m.running) continue;
      seen.add(m.id);
      const screen = (o.claudeTerminals.getScrollback(m.id) || '').slice(-SCREEN_MAX);
      records[m.id] = {
        id: m.id, project: m.alias, kind: 'terminal', title: m.title, order: m.order,
        sessionId: m.sessionId || '',
        opts: { shellOnly: !!m.shellOnly, skipPermissions: !!m.skipPermissions, model: m.model || '', effort: m.effort || '' },
        status: 'live', screen,
      };
    }
    for (const s of (o.claudeUi?.list() || [])) {
      const m = o.claudeUi.meta(s.id);
      if (!restorableChat(m) || !m.running) continue;
      seen.add(m.id);
      records[m.id] = {
        id: m.id, project: m.alias, kind: 'ui', title: m.title, order: m.order,
        sessionId: m.sessionId || '', cwd: m.cwd || '',
        opts: {
          model: m.model || '', effort: m.effort || '', skipPermissions: !!m.skipPermissions,
          // bypassPermissions only comes from the launch flag, which skipPermissions carries.
          permissionMode: m.permissionMode && m.permissionMode !== 'bypassPermissions' ? m.permissionMode : '',
        },
        status: 'live',
      };
    }
    // A live record whose pane is gone without being parked or closed by us
    // (it exited, or a path we do not see removed it) is no longer live.
    for (const id of Object.keys(records)) {
      if (records[id].status === 'live' && !seen.has(id)) delete records[id];
    }
    save();
  }

  /** At boot: whatever was live when Crundi stopped comes back parked. */
  function boot() {
    const now = Date.now();
    let n = 0;
    for (const r of Object.values(records)) {
      if (r.status === 'live') { r.status = 'parked'; r.parkedAt = now; r.reason = 'restart'; n++; }
    }
    if (n) console.log(`[panes] ${n} pane(s) from before the restart are parked, ready to resume`);
    save();
  }

  /** Park one live pane now. */
  function park(id, reason) {
    const term = o.claudeTerminals?.meta(id);
    const chat = term ? null : o.claudeUi?.meta(id);
    if (term && !restorableTerminal(term)) return { ok: false, error: 'This terminal cannot be parked' };
    if (chat && !restorableChat(chat)) return { ok: false, error: 'This chat cannot be parked' };
    if (!term && !chat) return { ok: false, error: 'No such pane' };
    snapshot();                              // records[id] now holds its latest state and screen
    const r = records[id];
    if (!r) return { ok: false, error: 'No such pane' };
    r.status = 'parked'; r.parkedAt = Date.now(); r.reason = reason;
    save();
    idleSince.delete(id);
    if (term) o.claudeTerminals.close(id); else o.claudeUi.close(id);
    console.log(`[panes] Parked "${r.title}" (${r.project}, ${id}) — ${reason}`);
    o.onChange();
    return { ok: true };
  }

  /** Resume a parked pane under the same id and order. */
  async function resume(id) {
    const r = records[id];
    if (!r || r.status !== 'parked') return { ok: false, error: 'This pane is not parked' };
    if (!o.getProject(r.project)) return { ok: false, error: `Project "${r.project}" no longer exists` };
    let result;
    if (r.kind === 'terminal') {
      const shell = !!r.opts.shellOnly;
      result = await o.claudeTerminals.create(r.project, {
        id: r.id, order: r.order, title: r.title, shell,
        skipPermissions: !!r.opts.skipPermissions, model: r.opts.model || '', effort: r.opts.effort || '',
        // A shell has nothing to resume: a fresh one in the same folder.
        sessionMode: shell ? null : (r.sessionId ? 'resume' : 'new'),
        resumeId: shell ? '' : (r.sessionId || ''),
      });
    } else {
      result = await o.claudeUi.create(r.project, {
        id: r.id, order: r.order, title: r.title, cwd: r.cwd || '',
        model: r.opts.model || '', effort: r.opts.effort || '',
        permissionMode: r.opts.permissionMode || '', skipPermissions: !!r.opts.skipPermissions,
        sessionMode: r.sessionId ? 'resume' : 'new', resumeId: r.sessionId || '',
      });
    }
    if (!result || !result.ok) return result || { ok: false, error: 'Could not resume' };
    // Ids are only reused when free; say so if it came back under another one.
    delete records[id];
    save();
    idleSince.set(result.id, Date.now());
    o.onChange();
    return { ok: true, id: result.id, sameSlot: result.id === id };
  }

  /** The user closed a parked pane, or closed a live one: forget it. */
  function forget(id) {
    if (!records[id]) return;
    delete records[id];
    idleSince.delete(id);
    save();
  }

  /** Parked panes, shaped like the state broadcast's terminal entries. */
  function listParked() {
    return Object.values(records).filter(r => r.status === 'parked').map(r => ({
      id: r.id, project: r.project, title: r.title, order: r.order,
      kind: 'parked', paneKind: r.kind, shellOnly: !!(r.opts && r.opts.shellOnly),
      status: 'parked', agentState: null, parkedAt: r.parkedAt || 0, reason: r.reason || 'restart',
      hasSession: !!r.sessionId,
    }));
  }

  /** What a parked pane shows behind its Resume button. */
  function preview(id) {
    const r = records[id];
    if (!r || r.status !== 'parked') return { ok: false, error: 'This pane is not parked' };
    if (r.kind === 'terminal') return { ok: true, kind: 'terminal', screen: r.screen || '' };
    let messages = [];
    if (r.sessionId) {
      const project = o.getProject(r.project);
      const cwd = r.cwd || (project && project.path) || '';
      try {
        const h = o.readTranscriptHistory(cwd, r.sessionId, { maxMessages: 200, maxBytes: 400_000 });
        messages = (h && (h.messages || h)) || [];
        if (!Array.isArray(messages)) messages = [];
      } catch { messages = []; }
    }
    return { ok: true, kind: 'ui', messages };
  }

  /** Park what has been idle long enough, while nobody is at Crundi. */
  function tick() {
    snapshot();
    const min = Number(o.timeoutMinutes()) || 0;
    if (!min) return;
    const ms = Math.max(MIN_TIMEOUT_MIN, min) * 60_000;
    const now = Date.now();
    const away = o.awayFor(ms);
    const candidates = [];
    for (const t of (o.claudeTerminals?.list() || [])) {
      const m = o.claudeTerminals.meta(t.id);
      if (!m || !m.running || !m.restorable || m.shellOnly) continue;   // shells: never for idleness
      const st = o.terminalState(t.id) || 'idle';
      candidates.push({ id: t.id, busy: st !== 'idle', since: m.createdAt || now });
    }
    for (const s of (o.claudeUi?.list() || [])) {
      const m = o.claudeUi.meta(s.id);
      if (!m || !m.running || !restorableChat(m)) continue;
      // Working, asking, or waiting on a background trigger — and any
      // background task the user has not dismissed — all count as going on.
      const busy = m.state !== 'idle' || m.busyTasks > 0 || m.pendingInjections > 0;
      candidates.push({ id: s.id, busy, since: now });
    }
    const live = new Set(candidates.map(c => c.id));
    for (const id of [...idleSince.keys()]) if (!live.has(id)) idleSince.delete(id);
    for (const c of candidates) {
      if (c.busy) { idleSince.set(c.id, now); continue; }
      if (!idleSince.has(c.id)) idleSince.set(c.id, Math.min(c.since, now));
      if (away && now - idleSince.get(c.id) >= ms) park(c.id, 'idle');
    }
  }

  /** Something happened in a pane: its idle clock restarts. */
  function touch(id) { if (idleSince.has(id)) idleSince.set(id, Date.now()); }

  function start() {
    boot();
    timer = setInterval(() => { try { tick(); } catch (err) { console.warn('[panes] tick failed:', err.message); } }, TICK_MS);
    if (timer.unref) timer.unref();
  }
  function stop() { if (timer) clearInterval(timer); timer = null; }

  return { start, stop, snapshot, park, resume, forget, listParked, preview, touch, tick, has: (id) => !!records[id] && records[id].status === 'parked' };
}
