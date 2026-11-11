/**
 * voice/autonomy.js — Cru on its own (9 Nov 2026, after OpenClaw and Hermes).
 *
 * When nobody spoke (a wake-up, a heartbeat, a chat Cru follows finishing),
 * there is nobody to say yes. So, for those turns only ("unattended"):
 *
 *  - what the owner ticked in Settings › Voice assistant › "Cru on its own"
 *    runs without a yes, logged and listed in the day's log;
 *  - anything else that changes something becomes a question on his phone
 *    (Approve / Decline, in the approvals inbox). Approved, the server runs
 *    it and tells Cru the result; declined or left, Cru is told that too;
 *  - read-only tools and Cru's own safe ones (its tickers, notes to itself)
 *    run as always.
 *
 * Switched on by hand only (like admin mode); Cru can switch it off, and
 * "Stop everything" switches it off, pauses every ticker and trigger, drops
 * the questions waiting and pauses Cru's jobs. The rules that hold everywhere
 * still hold here: no terminal typing (D21), no critical settings (D17), no
 * secret values (D16).
 */

/** What may be ticked, and the tools each covers. */
export const AWAY_CLASSES = {
  chats: { label: 'Message, start, park, resume and close Claude chats (an open chat is closed only after Cru has read it; a parked one is judged by its title, when and why it was parked)', tools: ['send_to_chat', 'start_chat', 'park_chat', 'resume_chat'] },
  kanban: { label: 'Add, change and move kanban cards (never to To do)', tools: ['add_kanban_task', 'update_kanban_task', 'move_kanban_task', 'add_kanban_todo', 'check_kanban_todo'] },
  notes: { label: 'Write notes and mindmap nodes', tools: ['create_note', 'append_to_note', 'add_mindmap_node', 'update_mindmap_node'] },
  services: { label: 'Start, stop and restart services', tools: ['service_control'] },
  jobs: { label: 'Take on cards: a Claude chat in bypass mode sees each through', tools: ['start_job', 'finish_job'] },
  schedules: { label: 'Schedule and cancel messages into chats (e.g. carry on when the usage limit resets)', tools: ['add_chat_schedule', 'delete_chat_schedule'] },
};
export const AWAY_CLASS_IDS = Object.keys(AWAY_CLASSES);
/** Ticked when the owner first switches it on. */
export const AWAY_DEFAULT_ALLOW = ['chats', 'kanban', 'notes'];
/** Harmless anywhere: following a chat. */
const ALWAYS_OK = new Set(['watch_chat', 'unwatch_chat', 'job_note']);

/** The class a tool belongs to, or ''. */
export function classOf(tool) {
  for (const [id, c] of Object.entries(AWAY_CLASSES)) if (c.tools.includes(tool)) return id;
  return '';
}

/**
 * May [tool] with [input] run unattended without asking, under [away]
 * ({ on, allow: [] })? Some are always asked even when their class is ticked:
 * a card to To do (it starts a factory run), a chat in bypass mode that is
 * not one of Cru's jobs, a service stopped.
 */
export function awayAllows(tool, input = {}, away = {}) {
  if (ALWAYS_OK.has(tool)) return true;
  if (!away || away.on !== true) return false;
  const cls = classOf(tool);
  if (!cls || !(away.allow || []).includes(cls)) return false;
  if (tool === 'move_kanban_task' && input.status === 'todo') return false;
  if (tool === 'start_chat' && input.mode === 'bypass') return false;
  if (tool === 'service_control' && input.op === 'stop') return false;
  return true;
}

/**
 * May Cru close a chat without asking (10 Nov 2026, the owner: the yes for
 * closing was a nuisance)? With "Cru on its own" on and chats ticked, yes,
 * attended or not, once it has read what the chat is doing ([readFresh]).
 * Without that read it asks as before.
 */
export function closeAllowed(away = {}, readFresh = false) {
  return !!(away && away.on === true && (away.allow || []).includes('chats') && readFresh);
}

/** How recently Cru must have read a chat (read_chat) to close it on its own. */
export const CLOSE_READ_MS = 15 * 60 * 1000;

/** The chats Cru has read (read_chat), and when: what closing on its own checks. */
export function createChatReads({ now = () => Date.now(), freshMs = CLOSE_READ_MS } = {}) {
  const at = new Map();
  return {
    mark(id) { if (id) at.set(String(id), now()); if (at.size > 500) at.delete(at.keys().next().value); },
    fresh(id) { const t = at.get(String(id || '')); return !!t && now() - t < freshMs; },
    forget(id) { at.delete(String(id || '')); },
  };
}

/** Clean an away setting from a request. */
export function cleanAway(v, prev = {}) {
  const on = v && v.on !== undefined ? v.on === true : prev.on === true;
  const src = v && Array.isArray(v.allow) ? v.allow : (Array.isArray(prev.allow) ? prev.allow : AWAY_DEFAULT_ALLOW);
  const allow = [...new Set(src.map(String))].filter((x) => AWAY_CLASS_IDS.includes(x));
  return { on, allow };
}

export const ASK_TIMEOUT_MS = 12 * 60 * 60 * 1000;
export const ASK_MAX = 10;
export const ASKS_PER_HOUR = 6;

/**
 * Questions for the owner from an unattended turn. Each is a yes/no on every
 * signed-in device; [onDone](ask, 'yes'|'no'|null) when answered or expired.
 * The action itself is [ask.run], kept server-side: Cru never answers its own.
 */
export function createOwnerAsks({ now = () => Date.now(), onAsk = () => {}, onDone = () => {}, timeoutMs = ASK_TIMEOUT_MS } = {}) {
  const asks = new Map();
  let made = [];
  let seq = 0;
  const expire = () => {
    const t = now();
    for (const a of [...asks.values()]) if (t - a.createdAt >= timeoutMs) { asks.delete(a.id); try { onDone(a, null); } catch { /* best effort */ } }
  };
  return {
    ask({ title = '', detail = '', tool = '', input = {}, run } = {}) {
      expire();
      const tl = String(title || '').replace(/\s+/g, ' ').trim().slice(0, 200);
      if (!tl || typeof run !== 'function') return { ok: false, error: 'Nothing to ask' };
      const same = [...asks.values()].find((a) => a.title === tl);
      if (same) return { ok: true, id: same.id, already: true };
      if (asks.size >= ASK_MAX) return { ok: false, error: `${ASK_MAX} questions are already waiting for the owner; leave this until he has answered some.` };
      const t = now();
      made = made.filter((x) => t - x < 3600 * 1000);
      if (made.length >= ASKS_PER_HOUR) return { ok: false, error: 'You have asked the owner a lot this hour; leave it for now and mention it when he is back.' };
      made.push(t);
      const a = { id: `cru-${t.toString(36)}-${++seq}`, title: tl, detail: String(detail || '').slice(0, 400), tool, input, run, createdAt: t, choices: [{ id: 'yes', label: 'Approve' }, { id: 'no', label: 'Decline' }] };
      asks.set(a.id, a);
      try { onAsk(a); } catch { /* best effort */ }
      return { ok: true, id: a.id };
    },
    answer(id, choice) {
      expire();
      const a = asks.get(String(id || ''));
      if (!a || !['yes', 'no'].includes(choice)) return null;
      asks.delete(a.id);
      try { onDone(a, choice); } catch { /* best effort */ }
      return a;
    },
    pending() { expire(); return [...asks.values()]; },
    expire,
    clear() { const all = [...asks.values()]; asks.clear(); return all.length; },
  };
}

/** /memories/log/YYYY-MM-DD.md for [date] (local time). */
export function logPath(date = new Date()) {
  const p = (n) => String(n).padStart(2, '0');
  return `/memories/log/${date.getFullYear()}-${p(date.getMonth() + 1)}-${p(date.getDate())}.md`;
}
export const LOG_KEEP_DAYS = 30;
export const LOG_MAX = 20000;

/**
 * The day's log: one line per thing that happened, appended. Kept within
 * LOG_MAX (oldest lines go first); logs older than LOG_KEEP_DAYS are deleted.
 * @param notebook  createNotebook(): read(path), save(path, text), list()
 */
export function createDailyLog(notebook, { now = () => new Date() } = {}) {
  let lastPrune = '';
  return {
    add(line) {
      const d = now();
      const path = logPath(d);
      const hm = d.toTimeString().slice(0, 5);
      const one = `- ${hm} ${String(line || '').replace(/\s+/g, ' ').trim().slice(0, 300)}`;
      if (one.length < 10) return;
      try {
        let text = notebook.read(path) || `# ${path.slice(14, 24)}\n`;
        text = `${text.trimEnd()}\n${one}\n`;
        while (text.length > LOG_MAX) { const lines = text.split('\n'); lines.splice(1, 1); text = lines.join('\n'); }
        notebook.save(path, text);
      } catch { /* best effort: a secret-looking line is simply not logged */ }
      if (lastPrune !== path) { lastPrune = path; this.prune(d); }
    },
    /** The last [n] lines of today's log, for the heartbeat. */
    tail(n = 15) {
      try { return (notebook.read(logPath(now())) || '').split('\n').filter((l) => l.startsWith('- ')).slice(-n).join('\n'); } catch { return ''; }
    },
    prune(d = now()) {
      const cut = d.getTime() - LOG_KEEP_DAYS * 86400000;
      try {
        for (const f of notebook.list()) {
          const m = f.path.match(/^\/memories\/log\/(\d{4})-(\d{2})-(\d{2})\.md$/);
          if (m && new Date(+m[1], +m[2] - 1, +m[3]).getTime() < cut) notebook.save(f.path, '');
        }
      } catch { /* best effort */ }
    },
  };
}

/** The heartbeat's built-in ticker. */
export const HEARTBEAT_ID = 'heartbeat';
export const HEARTBEAT_FILE = '/memories/heartbeat.md';
export const HEARTBEAT_DEFAULT = `# Heartbeat checklist
Go through this each heartbeat. Say nothing unless something needs the owner.
- Any service that should be running but has stopped or crashed?
- Any Claude chat waiting on the owner (a question or a permission) for more than 15 minutes?
- My jobs (list_jobs): finished, stuck, or needing the next step?
- Anything the owner asked me to keep an eye on (add it here).
`;

/** Is a checklist empty (only headings and blank lines)? Then the heartbeat skips its call. */
export function checklistEmpty(text) {
  return !String(text || '').split('\n').some((l) => l.trim() && !/^#+(\s|$)/.test(l.trim()));
}
