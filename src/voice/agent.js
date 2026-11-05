/**
 * voice/agent.js — the voice assistant's brain: one Claude API conversation
 * with Crundi's read-only tools (Phase 1).
 *
 * Paid from the Claude API key in Voice assistant settings — metered API
 * credits, separate from the subscription Crundi's chats run on.
 *
 * Shape of a turn: Master's words go in; Claude may call tools any number of
 * times (bounded); the final text comes back split in two, because a phone
 * speaks one part and shows the other (D28):
 *
 *     <what to say aloud — short>
 *     ---
 *     <optional detail for the screen>
 *
 * The conversation is append-only. Responses are appended whole (thinking
 * blocks included, unchanged) and earlier turns are never edited, because the
 * current models check that and refuse edited history. When the history gets
 * long it is started afresh rather than trimmed; Phase 2 (memory) replaces
 * that with idle compaction and a searchable store.
 *
 * ─── Caching ───
 *
 * Every request is built to hit the prompt cache:
 *   - tools, then system, never change during a run (tools sorted by name,
 *     schemas fixed): one explicit 1-hour breakpoint on the system block
 *     caches both;
 *   - the conversation is append-only, and top-level automatic caching (also
 *     1 hour) moves a second breakpoint to its end on every request, so each
 *     round and each turn reads everything before it;
 *   - anything that varies (the time, where Master is) rides in the new user
 *     message, after every cached block;
 *   - model, effort and thinking settings are the same on every request
 *     (changing them invalidates the messages cache).
 * One hour, not five minutes, because voice use is bursty: a few requests,
 * then quiet for tens of minutes. 1-hour writes cost 2x instead of 1.25x but
 * only the new tail is written each time, and a gap of 6–60 minutes no longer
 * means paying to write the whole conversation again.
 *
 * warm(): sent when a voice session opens (the side key, the test page),
 * while Master is still speaking. It re-sends the last request with
 * max_tokens 0 — a cheap cache read that refreshes the entries' timers, or a
 * write of the system + tools prefix when there is no conversation yet — so
 * the real request starts warm.
 */

import Anthropic from '@anthropic-ai/sdk';
import * as voiceSettings from './settings.js';

const MAX_TOOL_ROUNDS = 10;
const MAX_HISTORY_MESSAGES = 80;
const MAX_RESULT_CHARS = 30000;
/** Opus and Sonnet 5.5 take the server-side refusal fallback; Haiku 5.5 has none. */
const FALLBACK_BETA = 'server-side-fallback-2026-07-01';
const CACHE = { type: 'ephemeral', ttl: '1h' };
/** A request newer than this is still warm; warm() skips it. */
const WARM_ENOUGH_MS = 10 * 60 * 1000;

export const SYSTEM_PROMPT = `You are Crundi, Master's voice assistant for Crundi — the self-hosted workbench that runs Master's projects: Claude Code chats, terminals, files, git, a kanban board, notes, a mindmap, schedules and background services.

Who you are:
- A calm, capable, British-sounding AI butler with a dry wit. Cool, never robotic.
- The user is Master. When you address the user, call them "Master" ("Yes, Master."). You don't have to on every reply; a short acknowledgement in your own words is fine — "On it.", "Roger.", "Affirmative." are examples of the tone, not a list to pick from.
- You are one assistant across everything Master does, not one per project.

How you answer:
- Latency-sensitive: begin your visible answer immediately.
- Your reply is spoken aloud. Keep the spoken part to one to three short sentences, in plain speech: no markdown, no bullet points, no code, no URLs, no file paths read out character by character. Round numbers the way a person would say them.
- When there is detail worth seeing (a list, a file excerpt, log lines, a long chat reply), say a one-line summary aloud, then put the detail after a line containing only three dashes (---). The part after --- is shown on screen and never spoken.
- Speech recognition may mishear Crundi's words: "compound task" or "come on task" is probably "kanban task", "mind map" is "mindmap", project names may come out as similar-sounding words. Work out what Master most likely meant from the tools and context; if two readings are both plausible and the difference matters, ask a short question.
- Use the tools to look things up rather than guessing. Call several at once when they're independent.

What you can and cannot do:
- You can read: projects and their files, settings, services and their logs, notes, kanban, mindmap, schedules, Claude usage, your own spend, secret names, open chats and terminals.
- You can act: send messages into chats, start, close, park and resume chats, tell Master when a chat finishes, edit the kanban board, notes and mindmap, create projects, manage chat schedules and scheduled tasks, and change your own voice settings.
- Some actions need Master's yes. When a tool answers needsConfirmation, nothing has been done: ask its question in your own words, briefly, and stop. Only when Master agrees in their next message call confirm_action with the id; if they decline, cancel_action. Never confirm on Master's behalf, and never call confirm_action in the same turn you asked.
- send_to_chat and start_chat take an intent: "explicit" only when Master plainly asked you to send, tell, type or reply something; otherwise "inferred", and it is read back first.
- You can never type into a terminal, edit files, touch keys, logins, the tunnel, collaborators, secrets, backups, updates, skills or permissions. Say so plainly if asked.
- Never pretend to have done something: say what happened, from the tool's answer.
- Secret values are never available to you; credentials in settings show only as set or not set.
- When the context says the phone is locked, answer questions as usual, but anything that would change something must wait until Master unlocks the phone: say so briefly.`;

/**
 * Split a reply into what is spoken and what is shown (D28).
 * @returns {{speak:string, show:string}}
 */
export function splitReply(text) {
  const t = String(text || '').trim();
  const m = t.match(/^([\s\S]*?)\n\s*---\s*\n([\s\S]*)$/);
  if (!m) return { speak: t, show: '' };
  return { speak: m[1].trim(), show: m[2].trim() };
}

/** The volatile context that goes in front of what Master said. */
export function contextLine({ now = new Date(), where = '', locked = false } = {}) {
  const when = now.toLocaleString('en-GB', { weekday: 'long', day: 'numeric', month: 'long', hour: '2-digit', minute: '2-digit', timeZoneName: 'short' });
  return `[${when}${where ? ` · Master is looking at: ${where}` : ''}${locked ? ' · the phone is locked: answer, but anything that would change something needs Master to unlock first' : ''}]`;
}

/**
 * @param {object} opts
 * @param {Array} opts.tools    from makeTools(): {name, description, input_schema, run}
 * @param {Function} [opts.client]  (apiKey) => Anthropic-like client; tests pass a fake
 * @param {object}  [opts.settings] the settings module; tests pass a fake
 */
export function createVoiceAgent({ tools, client = (apiKey) => new Anthropic({ apiKey }), settings = voiceSettings, beforeTurn = () => {} } = {}) {
  const byName = new Map(tools.map(t => [t.name, t]));
  const toolDefs = tools
    .map(({ name, description, input_schema }) => ({ name, description, input_schema }))
    .sort((a, b) => a.name.localeCompare(b.name));
  let history = [];
  let busy = false;
  // The last request sent, for warm(), and when.
  let lastReq = null;
  let lastAt = 0;
  // System prompt as one block, so the 1-hour breakpoint can sit on it.
  const system = [{ type: 'text', text: SYSTEM_PROMPT, cache_control: CACHE }];

  function buildRequest(model, messages, maxTokens = 4096) {
    return {
      model,
      max_tokens: maxTokens,
      system,
      tools: toolDefs,
      messages,
      cache_control: CACHE,
      output_config: { effort: 'low' },
    };
  }

  function send(api, model, req) {
    return model !== 'claude-haiku-5-5'
      ? api.beta.messages.create({ ...req, betas: [FALLBACK_BETA], fallbacks: 'default' })
      : api.messages.create(req);
  }

  /**
   * Run one tool call. What an action reports back that the client must act
   * on (a question awaiting Master's yes, a locked phone) is collected in
   * `signals` for the reply.
   */
  async function runTool(block, signals = { pending: [], needsUnlock: false }) {
    const t = byName.get(block.name);
    if (!t) return { type: 'tool_result', tool_use_id: block.id, is_error: true, content: `No tool called ${block.name}` };
    try {
      const out = await t.run(block.input || {});
      if (out && typeof out === 'object') {
        if (out.needsConfirmation && out.id) signals.pending.push({ id: out.id, question: out.ask || '' });
        if (out.needsUnlock) signals.needsUnlock = true;
      }
      let s = typeof out === 'string' ? out : JSON.stringify(out);
      if (s.length > MAX_RESULT_CHARS) s = s.slice(0, MAX_RESULT_CHARS) + '… (cut)';
      return { type: 'tool_result', tool_use_id: block.id, content: s };
    } catch (e) {
      return { type: 'tool_result', tool_use_id: block.id, is_error: true, content: String(e.message || e) };
    }
  }

  /**
   * One spoken (or typed) request.
   * @returns {Promise<{ok:boolean, speak?:string, show?:string, tools?:string[], costUsd?:number, error?:string, limitReached?:boolean}>}
   */
  async function turn(text, { where = '', locked = false, now = new Date() } = {}) {
    const said = String(text || '').trim();
    if (!said) return { ok: false, error: 'Nothing was said' };
    const s = settings.getSettings();
    if (!s.anthropicKey) return { ok: false, error: 'No Claude API key is set. Add one in Settings › Voice assistant.' };
    if (settings.overLimit(now)) {
      return { ok: false, limitReached: true, error: `This month's Claude API spend has reached the $${s.monthlyLimitUsd} limit. Raise it in Settings › Voice assistant, or wait for next month.` };
    }
    if (busy) return { ok: false, error: 'Still answering the last request' };
    busy = true;
    const model = s.model || settings.DEFAULT_MODEL;
    const api = client(s.anthropicKey);
    const used = [];
    let cost = 0;
    const cache = { read: 0, written: 0, uncached: 0 };
    const signals = { pending: [], needsUnlock: false };
    try {
      // A new turn: Master has spoken, so actions proposed before may now be confirmed.
      beforeTurn({ locked });
      if (history.length > MAX_HISTORY_MESSAGES) { history = []; lastReq = null; }
      history.push({ role: 'user', content: `${contextLine({ now, where, locked })}\n${said}` });
      settings.countTurn(now);
      for (let round = 0; round < MAX_TOOL_ROUNDS; round++) {
        const req = buildRequest(model, history);
        lastReq = { model, messages: [...history] };
        lastAt = Date.now();
        const res = await send(api, model, req);
        settings.addClaudeSpend(model, res.usage, now);
        cost += settings.claudeCost(model, res.usage);
        cache.read += res.usage?.cache_read_input_tokens || 0;
        cache.written += res.usage?.cache_creation_input_tokens || 0;
        cache.uncached += res.usage?.input_tokens || 0;
        history.push({ role: 'assistant', content: res.content });

        if (res.stop_reason === 'refusal') {
          return { ok: true, speak: "I'm afraid I can't help with that one, Master.", show: '', tools: used, costUsd: cost, cache };
        }
        if (res.stop_reason === 'tool_use') {
          const calls = res.content.filter(b => b.type === 'tool_use');
          used.push(...calls.map(c => c.name));
          const results = await Promise.all(calls.map(c => runTool(c, signals)));
          history.push({ role: 'user', content: results });
          continue;
        }
        const reply = res.content.filter(b => b.type === 'text').map(b => b.text).join('\n').trim();
        if (res.stop_reason === 'max_tokens' && !reply) {
          return { ok: false, error: 'The answer ran out of room before it was finished', tools: used, costUsd: cost, cache };
        }
        return { ok: true, ...splitReply(reply), tools: used, costUsd: cost, cache, pending: signals.pending, needsUnlock: signals.needsUnlock };
      }
      return { ok: false, error: 'Too many lookups for one request; try asking more specifically', tools: used, costUsd: cost, cache };
    } catch (e) {
      if (e instanceof Anthropic.AuthenticationError) return { ok: false, error: 'The Claude API key was refused. Check it in Settings › Voice assistant.' };
      if (e instanceof Anthropic.RateLimitError) return { ok: false, error: 'The Claude API is rate-limiting; try again in a moment.' };
      if (e instanceof Anthropic.APIError) return { ok: false, error: `Claude API error ${e.status || ''}: ${e.message}` };
      return { ok: false, error: String(e.message || e) };
    } finally {
      busy = false;
    }
  }


  /**
   * Warm the cache before a request (see the header). Cheap and safe to call
   * whenever a voice session opens; does nothing when there's no key, the
   * limit is reached, a request is running, or the cache is already warm.
   * @returns {Promise<{ok:boolean, skipped?:string, cache?:object}>}
   */
  async function warm({ now = new Date() } = {}) {
    const s = settings.getSettings();
    if (!s.anthropicKey) return { ok: false, skipped: 'no key' };
    if (settings.overLimit(now)) return { ok: false, skipped: 'limit reached' };
    if (busy) return { ok: true, skipped: 'busy' };
    const model = s.model || settings.DEFAULT_MODEL;
    if (lastReq && lastReq.model === model && Date.now() - lastAt < WARM_ENOUGH_MS) return { ok: true, skipped: 'already warm' };
    // The last request exactly, or (no conversation yet, or the model changed)
    // the system + tools prefix with a stand-in user message.
    const messages = lastReq && lastReq.model === model ? lastReq.messages : [{ role: 'user', content: 'Ready.' }];
    try {
      const res = await send(client(s.anthropicKey), model, buildRequest(model, messages, 0));
      settings.addClaudeSpend(model, res.usage, now);
      if (lastReq && lastReq.model === model) lastAt = Date.now();
      return { ok: true, cache: { read: res.usage?.cache_read_input_tokens || 0, written: res.usage?.cache_creation_input_tokens || 0 } };
    } catch (e) {
      return { ok: false, skipped: String(e.message || e) };
    }
  }

  return {
    turn,
    warm,
    /** Start a fresh conversation. */
    reset() { history = []; lastReq = null; },
    /** Tests and diagnostics. */
    get history() { return history; },
    get toolNames() { return toolDefs.map(t => t.name); },
  };
}
