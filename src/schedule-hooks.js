/**
 * schedule-hooks.js — scheduled chats started by a webhook instead of a clock.
 *
 * A schedule whose `when.mode` is 'webhook' has a public URL,
 * POST /api/schedule-hook/<id>, secured like a factory webhook trigger (its key,
 * or an HMAC-SHA256 signature with its secret; see verifyWebhook in
 * factory-triggers.js). Made for alerts: Grafana calls it, a background chat
 * looks into it.
 *
 * - The prompt may read the call: {{payload.path}}, {{headers.name}},
 *   {{query.name}}, {{date}}, {{time}}. Anything missing comes out empty and is
 *   named; it never stops the run. The chat always gets the whole request too.
 * - An idempotency key chosen by the person ({{payload.groupKey}}) means a
 *   repeat is answered and not processed. Remembered 30 days.
 * - While a run is going (or its chat was left open for you), a new call goes
 *   into that chat as a message marked "Mid-run", queued behind its turn,
 *   rather than starting a second chat. With "One ongoing chat" every run
 *   comes back to the same conversation, so it knows what the earlier calls
 *   were about.
 * - Chat and service actions only. A command would put caller-controlled text
 *   in a shell; an agent terminal cannot take a mid-run message.
 */

import { readFileSync, writeFileSync, existsSync, mkdirSync, renameSync } from 'fs';
import { join } from 'path';
import { varsFor, fill, missingIn, idemKeyFor, verifyWebhook, requestRecord, bodyPreview } from './factory-triggers.js';

const SEEN_MAX = 1000;
const SEEN_TTL_MS = 30 * 24 * 3600_000;
const INLINE_MAX = 30000;           // the request shown in the message itself

export const HOOK_ACTIONS = ['chat', 'service'];

/**
 * @param {object} o
 * @param {string} o.dataDir
 * @param {(id:string) => object|null} o.getSchedule
 * @param {(id:string, patch:object) => void} o.setRuntime
 * @param {(sch:object, opts:{prompt:string, onStarted:(id:string)=>void}) => Promise<object>} o.runChat
 * @param {(sch:object) => Promise<void>} [o.runService]
 * @param {(cond:object, project:string) => boolean} [o.conditionMet]
 * @param {object} o.claudeUi   list(), sendMessage(id, text)
 */
export function createScheduleHooks({ dataDir, getSchedule, setRuntime = () => {}, runChat, runService = async () => {}, conditionMet = () => true, claudeUi }) {
  const FILE = join(dataDir, 'schedule-hooks.json');
  let seen = null;   // id -> [{ key, at }]
  const load = () => {
    if (seen) return seen;
    try { seen = existsSync(FILE) ? JSON.parse(readFileSync(FILE, 'utf-8')) || {} : {}; } catch { seen = {}; }
    return seen;
  };
  const save = () => {
    try {
      mkdirSync(dataDir, { recursive: true });
      writeFileSync(FILE + '.tmp', JSON.stringify(seen));
      renameSync(FILE + '.tmp', FILE);
    } catch (err) { console.warn('[schedule-hooks] could not save:', err.message); }
  };

  // Schedule id -> { chatId, pending: [text], finished } for the run going now.
  const active = new Map();

  const alive = (chatId) => {
    try { return !!(claudeUi && claudeUi.list().some(s => s.id === chatId && s.status === 'running')); } catch { return false; }
  };

  /** Claim an idempotency key: false if it was already seen. Synchronous. */
  function claim(id, key) {
    const all = load();
    const cutoff = Date.now() - SEEN_TTL_MS;
    const list = (all[id] || []).filter(x => Date.parse(x.at) >= cutoff);
    if (list.some(x => x.key === key)) { all[id] = list; return false; }
    all[id] = [...list, { key, at: new Date().toISOString() }].slice(-SEEN_MAX);
    save();
    return true;
  }
  function release(id, key) {
    const all = load();
    all[id] = (all[id] || []).filter(x => x.key !== key);
    save();
  }

  /** What the chat is told about the call, after the job's own (filled) prompt. */
  function requestText(sch, { request, idem, missing }) {
    const wh = sch.webhook || {};
    const lines = [];
    if (wh.idempotency) lines.push(idem ? `Idempotency key: ${idem} (from ${wh.idempotency}). A repeat with this key will not reach you.` : `No idempotency key was found in this call (it is read from ${wh.idempotency}), so repeats of it cannot be told apart.`);
    if (missing.length) lines.push(`Not in this call, so left empty above: ${missing.map(m => '{{' + m + '}}').join(', ')}. Look in the request below for what came instead.`);
    let full;
    try { full = JSON.stringify(request, null, 2); } catch { full = String(request); }
    if (full.length > INLINE_MAX) {
      let file = '';
      try {
        const dir = join(dataDir, 'schedule-hooks', sch.id);
        mkdirSync(dir, { recursive: true });
        file = join(dir, `${Date.now()}-${(idem || 'request').replace(/[^\w.\-]+/g, '_').slice(0, 60)}.json`);
        writeFileSync(file, full);
      } catch { file = ''; }
      lines.push(`The full request is ${full.length} characters${file ? `, saved in ${file}` : ''}. Its body, clipped:`, '```', bodyPreview(request.body, INLINE_MAX - 2000), '```');
    } else {
      lines.push('The full request (headers, query and body; credentials removed):', '```json', full, '```');
    }
    return lines.join('\n');
  }

  function deliver(id, text) {
    const a = active.get(id);
    if (!a) return false;
    if (!a.chatId) { a.pending.push(text); return true; }   // the chat is still being opened
    const r = claudeUi.sendMessage(a.chatId, text);
    return !(r && r.ok === false);
  }

  /**
   * A POST to a schedule's webhook.
   * @returns {Promise<{ok:boolean, status?:number, error?:string, duplicate?:boolean, skipped?:boolean, midRun?:boolean, started?:boolean}>}
   */
  async function onWebhook(id, req = {}) {
    const sch = getSchedule(id);
    if (!sch || !sch.when || sch.when.mode !== 'webhook') return { ok: false, status: 404, error: 'No such webhook.' };
    const v = verifyWebhook({ webhook: sch.webhook || {} }, { raw: req.raw || '', headers: req.headers || {}, key: req.key || '' });
    if (!v.ok) return v;
    if (!sch.enabled) return { ok: false, status: 409, error: 'This schedule is turned off.' };
    const kind = (sch.action || {}).kind;
    if (!HOOK_ACTIONS.includes(kind)) return { ok: false, status: 409, error: 'A webhook can run a background chat or a service action only.' };

    const vars = varsFor('webhook', { body: req.body, headers: req.headers, query: req.query });
    const extra = (sch.conditions || []).filter(c => c.type !== 'time');
    const unmet = extra.filter(c => { try { return !conditionMet(c, sch.project); } catch { return true; } });
    // Not processed, so its key is not claimed: the same call later still counts.
    if (unmet.length) return { ok: true, skipped: true, reason: `Waiting on: ${unmet.map(c => c.type).join(', ')}` };

    let idem = '';
    try { idem = idemKeyFor({ webhook: sch.webhook || {} }, vars); } catch { idem = ''; }
    if (idem && !claim(id, idem)) return { ok: true, duplicate: true };

    if (kind === 'service') {
      try { await runService(sch); setRuntime(id, { lastRun: new Date().toISOString() }); return { ok: true, started: true }; }
      catch (err) { if (idem) release(id, idem); return { ok: false, status: 500, error: err.message }; }
    }

    const request = requestRecord({ method: req.method || 'POST', headers: req.headers, query: req.query, body: req.body, raw: req.raw }, { schedule: sch.name, ...(idem ? { idempotencyKey: idem } : {}) });
    const tpl = String((sch.action || {}).prompt || '');
    const filled = fill(tpl, vars).trim();
    const missing = missingIn(tpl, vars);
    const about = requestText(sch, { request, idem, missing });

    // A run is going, or its chat was left open: tell that chat.
    const a = active.get(id);
    if (a && (!a.chatId || alive(a.chatId))) {
      const text = [
        `[Webhook] Mid-run: another call to "${sch.name}" arrived (${request.receivedAt}) while you are still on the last one. It is not an instruction by itself.`,
        'Decide whether it needs acting on: the same issue you are already working on (a repeat, an update, a resolution), or something new to look at after, or to report.',
        '',
        'What the job\'s prompt says for this call:',
        filled || '(the prompt came out empty for this call)',
        '',
        about,
      ].join('\n');
      if (deliver(id, text)) return { ok: true, midRun: true };
      // Could not reach it: fall through and start a run.
    }
    if (a) active.delete(id);

    const prompt = [filled || `The webhook of "${sch.name}" was called.`, '', '---', `[Webhook] This run was started by a call to this job's webhook (${request.receivedAt}).`, about].join('\n');
    const entry = { chatId: null, pending: [] };
    active.set(id, entry);
    setRuntime(id, { lastRun: new Date().toISOString() });
    // Not awaited: the caller gets its answer now; the run goes on.
    Promise.resolve().then(() => runChat(sch, {
      prompt,
      onStarted: (chatId) => {
        entry.chatId = chatId;
        for (const t of entry.pending.splice(0)) { try { claudeUi.sendMessage(chatId, t); } catch { /* best effort */ } }
      },
    })).then((r) => {
      if (active.get(id) !== entry) return;
      // Closed itself: the next call starts a new run. Left open (it needs
      // you, overran): later calls still go to it while it is open.
      if (!r || r.outcome === 'finished' || !r.sessionId) active.delete(id);
      else entry.chatId = r.sessionId;
      if (r && r.outcome === 'start-failed' && idem) release(id, idem);
    }).catch((err) => {
      if (active.get(id) === entry) active.delete(id);
      if (idem) release(id, idem);
      console.warn('[schedule-hooks] run failed:', err.message);
    });
    return { ok: true, started: true };
  }

  return { onWebhook, isActive: (id) => active.has(id) };
}
