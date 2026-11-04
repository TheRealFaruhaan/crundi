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
 * The system prompt and tool list never change during a run, so they are a
 * cached prefix; anything that varies (the time, today's projects) rides in
 * the user message instead.
 */

import Anthropic from '@anthropic-ai/sdk';
import * as voiceSettings from './settings.js';

const MAX_TOOL_ROUNDS = 10;
const MAX_HISTORY_MESSAGES = 80;
const MAX_RESULT_CHARS = 30000;
/** Opus and Sonnet 5.5 take the server-side refusal fallback; Haiku 5.5 has none. */
const FALLBACK_BETA = 'server-side-fallback-2026-07-01';

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

What you can and cannot do (right now):
- You can read: projects and their files, settings, services and their logs, notes, kanban, mindmap, schedules, Claude usage, your own spend, secret names, open chats and terminals.
- You cannot change anything yet: no editing files, notes, kanban, mindmap or settings, no typing into chats or terminals, no starting or closing anything. If Master asks for a change, say plainly that you can't do that yet and what you would need. Never pretend to have done something.
- Secret values are never available to you; credentials in settings show only as set or not set.`;

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
export function contextLine({ now = new Date(), where = '' } = {}) {
  const when = now.toLocaleString('en-GB', { weekday: 'long', day: 'numeric', month: 'long', hour: '2-digit', minute: '2-digit', timeZoneName: 'short' });
  return `[${when}${where ? ` · Master is looking at: ${where}` : ''}]`;
}

/**
 * @param {object} opts
 * @param {Array} opts.tools    from makeTools(): {name, description, input_schema, run}
 * @param {Function} [opts.client]  (apiKey) => Anthropic-like client; tests pass a fake
 * @param {object}  [opts.settings] the settings module; tests pass a fake
 */
export function createVoiceAgent({ tools, client = (apiKey) => new Anthropic({ apiKey }), settings = voiceSettings } = {}) {
  const byName = new Map(tools.map(t => [t.name, t]));
  const toolDefs = tools
    .map(({ name, description, input_schema }) => ({ name, description, input_schema }))
    .sort((a, b) => a.name.localeCompare(b.name));
  let history = [];
  let busy = false;

  async function runTool(block) {
    const t = byName.get(block.name);
    if (!t) return { type: 'tool_result', tool_use_id: block.id, is_error: true, content: `No tool called ${block.name}` };
    try {
      const out = await t.run(block.input || {});
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
  async function turn(text, { where = '', now = new Date() } = {}) {
    const said = String(text || '').trim();
    if (!said) return { ok: false, error: 'Nothing was said' };
    const s = settings.getSettings();
    if (!s.anthropicKey) return { ok: false, error: 'No Claude API key is set. Add one in Settings › Voice assistant.' };
    if (settings.overLimit(now)) {
      return { ok: false, limitReached: true, error: `This month's voice spend has reached the $${s.monthlyLimitUsd} limit. Raise it in Settings › Voice assistant, or wait for next month.` };
    }
    if (busy) return { ok: false, error: 'Still answering the last request' };
    busy = true;
    const model = s.model || settings.DEFAULT_MODEL;
    const api = client(s.anthropicKey);
    const used = [];
    let cost = 0;
    try {
      if (history.length > MAX_HISTORY_MESSAGES) history = [];
      history.push({ role: 'user', content: `${contextLine({ now, where })}\n${said}` });
      settings.countTurn(now);
      for (let round = 0; round < MAX_TOOL_ROUNDS; round++) {
        const req = {
          model,
          max_tokens: 4096,
          system: SYSTEM_PROMPT,
          tools: toolDefs,
          messages: history,
          cache_control: { type: 'ephemeral' },
          output_config: { effort: 'low' },
        };
        const fallback = model !== 'claude-haiku-5-5';
        const res = fallback
          ? await api.beta.messages.create({ ...req, betas: [FALLBACK_BETA], fallbacks: 'default' })
          : await api.messages.create(req);
        settings.addClaudeSpend(model, res.usage, now);
        cost += settings.claudeCost(model, res.usage);
        history.push({ role: 'assistant', content: res.content });

        if (res.stop_reason === 'refusal') {
          return { ok: true, speak: "I'm afraid I can't help with that one, Master.", show: '', tools: used, costUsd: cost };
        }
        if (res.stop_reason === 'tool_use') {
          const calls = res.content.filter(b => b.type === 'tool_use');
          used.push(...calls.map(c => c.name));
          const results = await Promise.all(calls.map(runTool));
          history.push({ role: 'user', content: results });
          continue;
        }
        const reply = res.content.filter(b => b.type === 'text').map(b => b.text).join('\n').trim();
        if (res.stop_reason === 'max_tokens' && !reply) {
          return { ok: false, error: 'The answer ran out of room before it was finished', tools: used, costUsd: cost };
        }
        return { ok: true, ...splitReply(reply), tools: used, costUsd: cost };
      }
      return { ok: false, error: 'Too many lookups for one request; try asking more specifically', tools: used, costUsd: cost };
    } catch (e) {
      if (e instanceof Anthropic.AuthenticationError) return { ok: false, error: 'The Claude API key was refused. Check it in Settings › Voice assistant.' };
      if (e instanceof Anthropic.RateLimitError) return { ok: false, error: 'The Claude API is rate-limiting; try again in a moment.' };
      if (e instanceof Anthropic.APIError) return { ok: false, error: `Claude API error ${e.status || ''}: ${e.message}` };
      return { ok: false, error: String(e.message || e) };
    } finally {
      busy = false;
    }
  }

  return {
    turn,
    /** Start a fresh conversation. */
    reset() { history = []; },
    /** Tests and diagnostics. */
    get history() { return history; },
    get toolNames() { return toolDefs.map(t => t.name); },
  };
}
