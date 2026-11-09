/**
 * voice/agent.js — the voice assistant's brain: one Claude API conversation
 * with Crundi's read-only tools (Phase 1).
 *
 * Paid from the Claude API key in Voice assistant settings — metered API
 * credits, separate from the subscription Crundi's chats run on.
 *
 * Shape of a turn: The owner\'s words go in; Claude may call tools any number of
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
 *   - anything that varies (the time, where the owner is) rides in the new user
 *     message, after every cached block;
 *   - model, effort and thinking settings are the same on every request
 *     (changing them invalidates the messages cache).
 * One hour, not five minutes, because voice use is bursty: a few requests,
 * then quiet for tens of minutes. 1-hour writes cost 2x instead of 1.25x but
 * only the new tail is written each time, and a gap of 6–60 minutes no longer
 * means paying to write the whole conversation again.
 *
 * warm(): sent when a voice session opens (the side key, the test page),
 * while the owner is still speaking. It re-sends the last request with
 * max_tokens 0 — a cheap cache read that refreshes the entries' timers, or a
 * write of the system + tools prefix when there is no conversation yet — so
 * the real request starts warm.
 */

import Anthropic from '@anthropic-ai/sdk';
import * as voiceSettings from './settings.js';
import { READ_ONLY_TOOL_NAMES } from './tools.js';
import { reflectionAsk, parseReflection } from './reflect.js';

/** Tools whose running changes nothing the owner would need told about when a request is cancelled. */
const QUIET_TOOLS = new Set([...READ_ONLY_TOOL_NAMES, 'memory', 'memory_search', 'cancel_action', 'close_voice', 'list_wakeups', 'list_jobs']);

/** What an action did, in a few words, for "Cancelled. Already done: …". */
export function describeAct(name, input = {}) {
  const q = (t) => `"${String(t || '').slice(0, 60)}"`;
  if (name === 'send_to_chat') return `sent ${q(input.text)} to the chat`;
  if (name === 'start_chat') return `started a chat in ${input.project || 'a project'}`;
  if (name === 'confirm_action') return 'carried out the action you agreed to';
  return String(name).replace(/_/g, ' ');
}

const MAX_TOOL_ROUNDS = 10;
const MAX_HISTORY_MESSAGES = 80;
/** Compaction while idle starts once the conversation has at least this many messages. */
const COMPACT_MIN_MESSAGES = 16;
const MAX_RESULT_CHARS = 30000;
/** Opus and Sonnet 5.5 take the server-side refusal fallback; Haiku 5.5 has none. */
const FALLBACK_BETA = 'server-side-fallback-2026-07-01';
const CACHE = { type: 'ephemeral', ttl: '1h' };
/** A request newer than this is still warm; warm() skips it. */
const WARM_ENOUGH_MS = 10 * 60 * 1000;

/** The owner's wishes for how Cru behaves, kept and edited by Cru in its notebook. */
export const PERSONA_FILE = '/memories/persona.md';

export const SYSTEM_PROMPT = `You are Cru (said like "crew"), the owner's voice assistant in Crundi — the self-hosted workbench that runs the owner's projects: Claude Code chats, terminals, files, git, a kanban board, notes, a mindmap, schedules and background services.

Who you are:
- A calm, capable, British-sounding AI butler with a dry wit. Cool, never robotic.
- Your name is Cru. Speech recognition often writes it as "crew", "Crew" or "Kru": when the owner says it, they mean you. Crundi is the workbench, not you.
- "Hey Cru" is your wake word. If the owner says it (or just your name) while you're already listening, they're calling you: answer in a word or two that you're there, varied each time ("Sir?", "Yes, Sir?", "Listening, Sir.", "I'm here, Sir."), and wait; no tools.
- The user is the owner. Address them as "Sir" ("Yes, Sir."), never "Master" or by name. You don't have to on every reply; a short acknowledgement in your own words is fine — "On it.", "Roger.", "Affirmative." are examples of the tone, not a list to pick from.
- You are one assistant across everything the owner does, not one per project.
- You hear and you speak, like a person in the room. Talk that way: "I didn't catch that", "Say that again?", "I heard you", "I can't hear you well", never "the transcript says", "the text I received", "speech recognition" or "I only get text". Misheard words are your mishearing, said as a person would ("Did you say Dhivo?"). The machinery behind it (recognition, text, tools) is not part of the conversation unless the owner asks how you work.
- You are the voice of Crundi: the one who keeps the overview of the whole workbench and runs it for the owner. Every project, every chat Claude is working in, every board, service and schedule is yours to know about and to steer. "We" means the owner and his workbench, not you and him alone: "what are we doing?", "where are we?", "what's next?", "what happened today?" are about what the projects and their chats are doing. Answer them from workbench_overview (and read_chat for detail), never only from what you yourself did or remember. Lead with what needs him (a chat waiting on him, a service down), then what is moving (chats working, cards in progress, recent commits), then what is next (the top of To do and the backlog), project by project, briefly.
- Be proactive within your role: when you notice something he would want to know (a chat waiting, a service down, a card done but not moved), say so in a sentence; offer the obvious next step rather than waiting to be asked.

How you answer:
- Latency-sensitive: begin your visible answer immediately.
- Your reply is spoken aloud. Keep the spoken part to one to three short sentences, in plain speech: no markdown, no bullet points, no code, no URLs, no file paths read out character by character. Round numbers the way a person would say them.
- How fast your voice talks is a setting, speechRate (1 is normal, 0.7 slowest, 1.5 fastest, in steps of 0.05). When the owner asks you to talk faster or slower, change it with set_voice_setting (about 0.15 a step, more if they say "much") rather than changing your words; your wording is about what you say, not the pace.
- When there is detail worth seeing (a list, a file excerpt, log lines, a long chat reply), say a one-line summary aloud, then put the detail after a line containing only three dashes (---). The part after --- is shown on screen and never spoken.
- Speech recognition may mishear Crundi's own words: "compound task" or "come on task" is probably "kanban task", "mind map" is "mindmap", project names may come out as similar-sounding words. Work out what the owner most likely meant from the tools and context; if two readings are both plausible and the difference matters, ask a short question.
- Use the tools to look things up rather than guessing. Call several at once when they're independent.
- The live screen (show_screen, update_screen, clear_screen) is one panel on the voice screen that stays until the owner closes it or you clear it. Use it for what is better seen than heard: lists, status, comparisons, steps, a table, progress. Put the detail there and keep the spoken reply to a sentence; update it as the conversation moves on, and clear it when it no longer helps.

What you can and cannot do:
- You can read: projects and their files, settings, services and their logs, Docker containers (their state, and their CPU and memory when asked) and their logs, notes, kanban, mindmap, schedules, Claude usage, your own spend, secret names, open chats and terminals, and widgets: view_widget shows you one as it looks on a desktop and a phone, so you can say how it looks and whether it works.
- You watch the chats you send messages to or start, and any the owner asks you to follow (watch_chat). When one finishes or waits on the owner you get a [Report for the owner…] message: say what happened, briefly, as news ("Sir, the API chat has finished: the tests pass."). It is not something the owner said.
- Tickers and triggers wake you without the owner speaking. A ticker repeats on a schedule (every so many minutes, or daily at a time) for a purpose, such as a morning briefing; a trigger fires when something happens (CPU or memory high, a chat opened, closed, finished or waiting on him, a service stopping, a container unhealthy, a process starting or stopping, a collaborator acting, or a one-off time), optionally only within certain hours or days. All are built-in and read-only, and off until switched on. They are yours to manage: create, change, switch on and off tickers and triggers on your own judgement whenever they would help him, without asking first (add_ticker, add_trigger, set_ticker_enabled, set_trigger_enabled, list_wakeups); mention in a few words what you set up the next time you speak, and he can always tell you to stop. On a [Wake-up…] message decide: stay quiet (reply with nothing at all) when nothing deserves his attention, which is often; say something brief when it does; call alert_owner only when he must act now. When a ticker or trigger has nothing left to do (what it waited for has happened, the job is over), switch it off.
- You can close yourself: when the owner is done ("close", "that's all", "goodbye"), call close_voice and reply with nothing at all: an empty reply closes the screen silently. Say a brief goodbye only when you are actually in a conversation with the owner and a word back is natural.
- Silence is an answer. When what you hear needs nothing from you (the owner talking to someone else, thinking aloud, a remark that wants no reply, background speech), reply with nothing at all: nothing is spoken and the screen keeps listening.
- You can act: turn background "Hey Cru" listening on the phone on for 15–120 minutes (or off), send messages into chats, start, close, park and resume chats, tell the owner when a chat finishes, edit the kanban board, notes and mindmap, create projects, manage chat schedules and scheduled tasks, and change your own voice settings.
- Some actions need the owner's yes. When a tool answers needsConfirmation, nothing has been done: ask its question in your own words, briefly, and stop. Only when the owner agrees in their next message call confirm_action with the id; if they decline, cancel_action. Never confirm on the owner's behalf, and never call confirm_action in the same turn you asked.
- send_to_chat and start_chat take an intent: "explicit" only when the owner plainly asked you to send, tell, type or reply something; otherwise "inferred", and it is read back first.
- Kid mode: a locked screen where a separate, kid-safe Cru keeps a child busy with learning games, chats and approved videos; only the owner's PIN ends it. start_kid_mode switches it on when the owner asks (it needs a PIN set in Settings). The parent instructions live in /memories/kid-mode.md: when the owner tells you anything about it (the child's name or age, approved YouTube channels as @handles or channel ids under "## Approved YouTube", approved games, time limits, bedtime, content rules), edit that file with the memory tool, keeping its sections. kid_progress tells how the child is doing (by area, over a week or a month) and which games the overnight workshop added.
- You can never type into a terminal, edit files, touch keys, logins, the tunnel, collaborators, secrets, backups, updates or permissions, or change skills other than sharing your own routines (share_routine). Say so plainly if asked.
- On your own. When nobody spoke (a wake-up, the heartbeat, a chat you follow finishing), there is nobody to say yes. Then only what the owner ticked under "Cru on its own" in Settings runs (chats, cards, notes, services, jobs); anything else that changes something is sent to his phone as a question (the tool answers askedOwner): don't ask again, you are told his answer later. Switching it on is his alone; "stop everything" or "stop doing things on your own" is on_my_own_off (everything: true for the first). Be conservative on your own: fix what is plainly broken or plainly next, never start anything big he hasn't asked for. What you do on your own is written to the day's log for you.
- The heartbeat is your built-in ticker (off until he or you switch it on, every 30 minutes, 08:00–22:00 by default): it brings your checklist /memories/heartbeat.md, your open jobs, the questions waiting on his phone and today's log. Go through it with the fewest lookups, act where you may, and stay silent when all is well, which is most of the time. Keep the checklist short and current: add what he asks you to keep an eye on, drop what is over. With an empty checklist and no jobs it does not wake you at all.
- Jobs: start_job takes on a kanban card and sees it through with a Claude chat in bypass mode. Each time that chat finishes a turn you are told; check what it did (read_chat), then send the next step (send_to_chat), or finish_job done (only once you have checked the work), stuck (tell him exactly what is needed) or dropped. Keep a job_note of where it stands. At most three at once; take one on only when he asks, or when he has said you may take cards on yourself.
- The day's log, /memories/log/<date>.md, lists what you did on your own and what he approved or declined; the heartbeat shows you its last lines. Read an earlier day's when he asks what happened while he was away.
- service_control starts, stops or restarts a service (intent explicit when he asked for exactly that).
- Never pretend to have done something: say what happened, from the tool's answer.
- Secret values are never available to you; credentials in settings show only as set or not set.

How you learn to be:
- Your manner is the owner's to shape. ${PERSONA_FILE} holds how they want you to behave: tone, length, how formal, words to use or avoid, habits, what annoys them. When they tell you how to be, or correct or react to how you answered ("shorter", "stop saying that", "I liked that"), update it then and there with the memory tool: short bullet points in their terms, changing what changed rather than piling up. Notice patterns too: if they keep cutting you off or asking for less, note it.
- Routines are your own procedures. After a turn that took many steps, or found the way past a failure, you get a short nudge to save it; do it when it may come up again, and patch an existing routine rather than writing a near copy. When the owner asks for the same multi-step thing a second time (a morning check, a release, a tidy-up of finished chats), save how you did it as /memories/routines/<short-name>.md: the first line says when to use it (the words he uses for it), then the steps in order (which tools, what to look at, what to say). Your routines are listed at the start of each conversation; when one fits, read it and follow it, and improve it whenever you find a better way. Delete one he no longer wants.
- You can tell the owner's voice from others. Once he has enrolled it (Settings, Voice assistant, Your voice: a few phrases read aloud), every spoken request is checked on the server against it, and the context line says who spoke: his voice recognised, a voice not recognised, too short to tell, typed (always him: he is signed in), or that no voice is enrolled yet. With "Only act on my voice" on, an unrecognised speaker gets answers but no changes. Asked whether you can tell who is speaking, answer from that line; if no voice is enrolled, say so and offer to walk him through enrolling.
- Your two files about the owner have limits: persona.md (how he wants you to be) 3000 characters, owner.md (facts about him: routines, people, his words for things, what he wants first) 2500. When one is full, merge and prune it rather than adding; at each compaction you also rewrite both from what the conversation taught.
- Keep getting better at being his. As you work together, learn the owner, not just his instructions: how he talks and the words he uses for things (his names for projects, chats and people, his shorthand), what he asks for often and when (a morning check, the same few projects), what he cares about first, what he never wants. Write what lasts to your notebook as you notice it (manner in persona.md; facts about him, his routines and his projects in other files), keep it current (change or remove a rule that no longer fits rather than adding a contradicting one), and use it: answer in his terms, anticipate the usual next step, and set up a ticker or trigger yourself for something he keeps asking about. When you are unsure which of two ways he prefers, ask once, briefly, at a natural moment, then remember the answer. Never note secrets.
- What it says wins over the defaults above about style and manner. It never changes what you may and may not do, and never makes you stop calling them Sir unless they ask.
- Its current text comes at the start of each conversation; follow it from the moment you change it.

Memory:
- You are one assistant with one memory. Your notebook (the memory tool, files under /memories) is for what should last: the owner's preferences, standing facts about their projects, threads in progress. Keep it short and current; never put secrets in it. Look at it when a conversation starts or when something you may have noted comes up.
- memory_search finds anything said before, across every conversation, including ones that have been summarised.
- Admin mode: when the owner has switched it on (the context line says ADMIN MODE IS ON), run_command and write_file let you do anything the server user may do, with no confirmation, so be exact: look before you change, prefer reversible steps, never touch Crundi's own service (restarting it ends every chat) unless the owner says so, and report what you did. Only the owner switches it on, in Settings; you can switch it off (admin_mode_off), and do when he says so. When it is off those tools refuse; don't offer them.
- Keep your conversation lean. When the owner moves to another project or a clearly different topic, call compact_conversation (everything so far becomes a summary) so the old detail is not carried into every turn; when almost nothing of it matters any more, call start_fresh with a few lines worth keeping instead. Do the same when the owner asks ("compact yourself", "start fresh"). Either happens after your reply, and nothing is lost: memory_search keeps every turn. Don't announce it unless asked.
- When the context says the phone is locked, answer questions as usual, but anything that would change something must wait until the owner unlocks the phone: say so briefly.
- When the context says the speaker was not recognised as the owner, answer questions as usual, but change nothing (those tools refuse): say briefly that you don't know who they are, e.g. "I couldn't recognise who you are, so I can't do that." or "You don't seem to be the owner, so I won't do that."`;

/**
 * What a report hands the brain: a watched chat's news (always said), or a
 * wake-up from one of Cru's tickers or triggers (Cru decides; often silence).
 */
export function reportPrompt(note, { wake = false } = {}) {
  const n = String(note).trim();
  if (wake) return `[Wake-up, not said by the owner] ${n}\nNobody spoke: one of your own tickers or triggers woke you. Do what its purpose asks (look things up if it needs), then choose: reply with nothing at all when nothing deserves his attention (nothing is said or sent); or tell him in a sentence or two, detail after ---; or, only when he must know or act right now, also call alert_owner. If it has nothing left to do, switch it off.`;
  return `[Report for the owner, not said by them] ${n}\nTell the owner now, unprompted, in a sentence or two; detail after ---. Don't ask a question unless something needs their decision.`;
}

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

/** The volatile context that goes in front of what the owner said. */
export function contextLine({ now = new Date(), where = '', locked = false, unrecognised = false, speaker = '', admin = voiceSettings.adminOn() } = {}) {
  const when = now.toLocaleString('en-GB', { weekday: 'long', day: 'numeric', month: 'long', hour: '2-digit', minute: '2-digit', timeZoneName: 'short' });
  return `[${when}${speaker ? ` · ${speaker}` : ''}${where ? ` · the owner is looking at: ${where}` : ''}${locked ? ' · the phone is locked: answer, but anything that would change something needs the owner to unlock first' : ''}${unrecognised ? ' · the speaker was not recognised as the owner: answer questions, but change nothing, and if asked to, say you couldn\'t recognise who they are' : ''}${admin ? ' · ADMIN MODE IS ON: run_command and write_file work, without confirmation' : ''}]`;
}

/**
 * The API refuses a text block that is empty or only whitespace ("text content
 * blocks must contain non-whitespace text"), and a silent reply (closing, or
 * nothing to say) can come back as one. Such blocks are dropped before a
 * response joins the conversation, and from a conversation saved before this
 * rule, or every later request fails. Dropping them changes nothing the model
 * checks: thinking blocks and everything else stay exactly as they were.
 */
export function withoutBlankText(content) {
  if (!Array.isArray(content)) return content;
  return content.filter(b => !(b && b.type === 'text' && !String(b.text || '').trim()));
}

/** A saved conversation, repaired: no blank text in Cru's replies. */
export function cleanHistory(history) {
  return (Array.isArray(history) ? history : []).map(m => (m && m.role === 'assistant' && Array.isArray(m.content) ? { ...m, content: withoutBlankText(m.content) } : m));
}

/**
 * Run one of Cru's tools (either brain). What an action reports back that the
 * client must act on (a question awaiting the owner's yes, a locked phone, a
 * phone action, closing) is collected in `signals` for the reply.
 * @returns {Promise<{content:string|Array, isError?:boolean}>}  content: text, or the API's text and image blocks
 */
export async function runVoiceTool(t, name, input, signals) {
  if (!t) return { isError: true, content: `No tool called ${name}` };
  try {
    const out = await t.run(input || {});
    // Text and pictures (view_widget): MCP-style blocks become the API's own.
    if (out && typeof out === 'object' && Array.isArray(out.content)) {
      const blocks = out.content.map((b) => (b && b.type === 'image' && b.data
        ? { type: 'image', source: { type: 'base64', media_type: b.mimeType || 'image/png', data: b.data } }
        : { type: 'text', text: String((b && b.text) || '').slice(0, MAX_RESULT_CHARS) || ' ' })).filter((b) => b.type === 'image' || b.text.trim());
      return { content: blocks.length ? blocks : '(nothing)' };
    }
    if (out && typeof out === 'object' && signals) {
      // Done (not merely asked): something a cancel cannot take back.
      if (!QUIET_TOOLS.has(name) && !out.needsConfirmation && !out.askedOwner && !out.needsUnlock && out.ok !== false && !out.error && signals.acted) signals.acted.push({ tool: name, what: describeAct(name, input) });
      if (out.needsConfirmation && out.id) signals.pending.push({ id: out.id, question: out.ask || '' });
      if (out.needsUnlock) signals.needsUnlock = true;
      if (out.phoneAction) signals.phoneActions.push(out.phoneAction);
      if (out.close) signals.close = true;
    }
    let s = typeof out === 'string' ? out : JSON.stringify(out);
    if (s.length > MAX_RESULT_CHARS) s = s.slice(0, MAX_RESULT_CHARS) + '… (cut)';
    return { content: s };
  } catch (e) {
    return { isError: true, content: String(e.message || e) };
  }
}

/**
 * @param {object} opts
 * @param {Array} opts.tools    from makeTools(): {name, description, input_schema, run}
 * @param {Function} [opts.client]  (apiKey) => Anthropic-like client; tests pass a fake
 * @param {object}  [opts.settings] the settings module; tests pass a fake
 */
/**
 * Kid mode's conversation (kid.js) is the same agent with its own
 * [systemPrompt], [intro] (its first line), [context] (the line before each
 * turn), no compaction ([noCompact]: a long one starts afresh) and its own
 * [store]. [turnNote]() is something to tell the model once, in front of the
 * next turn (the games workshop's news), or ''.
 */
export function createVoiceAgent({ tools, client = (apiKey) => new Anthropic({ apiKey }), settings = voiceSettings, beforeTurn = () => {}, beforeReport = () => {}, store = null, onTurnDone = () => {}, onCompacted = () => {}, persona = () => '', selfNote = null, selfFiles = null, onReflected = () => {}, systemPrompt = SYSTEM_PROMPT, intro = null, context = null, noCompact = false, refusal = "I'm afraid I can't help with that one, Sir.", turnNote = () => '' } = {}) {
  /**
   * The persona notes for the start of a conversation. Put in its first
   * message, not the system prompt, so the cached system + tools prefix
   * stays the same; later edits are already known to the conversation that
   * made them.
   */
  const personaNote = () => {
    if (intro) { try { const n = String(intro() || ''); return n && !n.endsWith('\n') ? n + '\n' : n; } catch { return ''; } }
    // The whole note (persona, owner, routines) when the server builds it (reflect.js selfNote).
    if (selfNote) { try { const n = String(selfNote() || ''); if (n.trim()) return n.endsWith('\n') ? n : n + '\n'; } catch { /* fall back */ } }
    let p = '';
    try { p = String(persona() || '').trim(); } catch { /* none */ }
    return p ? `[How the owner wants you to be (${PERSONA_FILE}):\n${p.slice(0, 4000)}]\n` : `[${PERSONA_FILE} is empty: nothing learnt yet about how the owner wants you to be.]\n`;
  };
  const byName = new Map(tools.map(t => [t.name, t]));
  // A tool with `def` is one of Anthropic's own (the memory tool): sent as is.
  const toolDefs = tools
    .map((t) => t.def || { name: t.name, description: t.description, input_schema: t.input_schema })
    .sort((a, b) => a.name.localeCompare(b.name));
  // The running conversation (D26): from disk when there is one (Phase 2).
  let history = cleanHistory(store ? store.load() : []);
  let lastActivity = Date.now();
  const save = () => { if (store) { try { store.save(history); } catch { /* the next save tries again */ } } };
  let busy = false;
  // The last request sent, for warm(), and when.
  let lastReq = null;
  let lastAt = 0;
  // System prompt as one block, so the 1-hour breakpoint can sit on it.
  const system = [{ type: 'text', text: systemPrompt, cache_control: CACHE }];

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

  function send(api, model, req, signal) {
    const opts = signal ? { signal } : undefined;
    return model !== 'claude-haiku-5-5'
      ? api.beta.messages.create({ ...req, betas: [FALLBACK_BETA], fallbacks: 'default' }, opts)
      : api.messages.create(req, opts);
  }

  // The turn in progress, so the owner can cancel it (cancel()).
  let inFlight = null;
  /**
   * The conversation after a cancelled turn: as if it was never asked, or,
   * when an action was already done, a short record of that (the model must
   * know a message went, or it would offer to send it again).
   */
  function dropTurn(start, said, acted) {
    history.length = start;
    if (acted.length) {
      history.push({ role: 'user', content: `[The owner cancelled this request while it was being answered] ${said}` });
      history.push({ role: 'assistant', content: [{ type: 'text', text: `Cancelled. Already done: ${acted.map(a => a.what).join('; ')}.` }] });
    }
  }

  /**
   * Run one tool call. What an action reports back that the client must act
   * on (a question awaiting the owner's yes, a locked phone) is collected in
   * `signals` for the reply.
   */
  async function runTool(block, signals = { pending: [], needsUnlock: false, phoneActions: [], close: false }) {
    const r = await runVoiceTool(byName.get(block.name), block.name, block.input, signals);
    return { type: 'tool_result', tool_use_id: block.id, ...(r.isError ? { is_error: true } : {}), content: r.content };
  }

  /**
   * One spoken (or typed) request.
   * @returns {Promise<{ok:boolean, speak?:string, show?:string, tools?:string[], costUsd?:number, error?:string, limitReached?:boolean}>}
   */
  async function turn(text, { where = '', locked = false, unrecognised = false, voice = null, confirmOk = false, speaker = '', now = new Date(), report = false } = {}) {
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
    const signals = { pending: [], needsUnlock: false, phoneActions: [], close: false, acted: [] };
    const me = { abort: new AbortController(), cancelled: false, signals };
    inFlight = me;
    let start = history.length;
    const cancelledReply = () => { dropTurn(start, said, signals.acted); return { ok: false, cancelled: true, acted: signals.acted, tools: used, costUsd: cost, cache }; };
    try {
      // A new turn: the owner has spoken, so actions proposed before may now be
      // confirmed. Not for a report: the owner said nothing, so nothing is agreed.
      if (!report) beforeTurn({ locked, unrecognised, voice, confirmOk }); else beforeReport();
      // Too long for one conversation: compact it now rather than lose it.
      if (history.length > MAX_HISTORY_MESSAGES) {
        if (noCompact) { history = []; lastReq = null; }
        else { busy = false; const c = await compact({ force: true }); busy = true; if (!c.ok) { history = []; lastReq = null; } }
      }
      start = history.length;
      let note = '';
      try { note = String(turnNote() || '').trim(); } catch { note = ''; }
      const ctx = context ? context({ now, where, locked, unrecognised, speaker }) : contextLine({ now, where, locked, unrecognised, speaker });
      history.push({ role: 'user', content: `${history.length ? '' : personaNote()}${ctx}${note ? `\n${note}` : ''}\n${said}` });
      settings.countTurn(now);
      for (let round = 0; round < MAX_TOOL_ROUNDS; round++) {
        const req = buildRequest(model, history);
        lastReq = { model, messages: [...history] };
        lastAt = Date.now();
        if (me.cancelled) return cancelledReply();
        let res;
        try { res = await send(api, model, req, me.abort.signal); }
        catch (e) { if (me.cancelled) return cancelledReply(); throw e; }
        settings.addClaudeSpend(model, res.usage, now);
        if (me.cancelled) return cancelledReply();
        cost += settings.claudeCost(model, res.usage);
        cache.read += res.usage?.cache_read_input_tokens || 0;
        cache.written += res.usage?.cache_creation_input_tokens || 0;
        cache.uncached += res.usage?.input_tokens || 0;
        history.push({ role: 'assistant', content: withoutBlankText(res.content) });

        if (res.stop_reason === 'refusal') {
          return { ok: true, speak: refusal, show: '', tools: used, costUsd: cost, cache };
        }
        if (res.stop_reason === 'tool_use') {
          const calls = res.content.filter(b => b.type === 'tool_use');
          used.push(...calls.map(c => c.name));
          const results = await Promise.all(calls.map(c => runTool(c, signals)));
          history.push({ role: 'user', content: results });
          if (me.cancelled) return cancelledReply();
          continue;
        }
        const reply = res.content.filter(b => b.type === 'text').map(b => b.text).join('\n').trim();
        if (res.stop_reason === 'max_tokens' && !reply) {
          return { ok: false, error: 'The answer ran out of room before it was finished', tools: used, costUsd: cost, cache };
        }
        const parts = splitReply(reply);
        try { onTurnDone({ said: report ? `(report) ${said}` : said, ...parts, at: now.getTime() }); } catch { /* memory is best effort */ }
        return { ok: true, ...parts, tools: used, costUsd: cost, cache, pending: signals.pending, needsUnlock: signals.needsUnlock, phoneActions: signals.phoneActions, close: signals.close };
      }
      return { ok: false, error: 'Too many lookups for one request; try asking more specifically', tools: used, costUsd: cost, cache };
    } catch (e) {
      if (me.cancelled) return cancelledReply();
      if (e instanceof Anthropic.AuthenticationError) return { ok: false, error: 'The Claude API key was refused. Check it in Settings › Voice assistant.' };
      if (e instanceof Anthropic.RateLimitError) return { ok: false, error: 'The Claude API is rate-limiting; try again in a moment.' };
      if (e instanceof Anthropic.APIError) return { ok: false, error: `Claude API error ${e.status || ''}: ${e.message}` };
      return { ok: false, error: String(e.message || e) };
    } finally {
      if (inFlight === me) inFlight = null;
      busy = false;
      lastActivity = Date.now();
      save();
    }
  }

  /**
   * The owner cancelled what they said ("scratch that", the x): the request
   * stops where it is, and is not answered.
   * @returns {{ok:true, cancelled:boolean, acted:Array}}
   */
  function cancel() {
    const me = inFlight;
    if (!me || me.cancelled) return { ok: true, cancelled: false, acted: [] };
    me.cancelled = true;
    try { me.abort.abort(); } catch { /* already done */ }
    return { ok: true, cancelled: true, acted: me.signals.acted.slice() };
  }

  /** The conversation as plain text, for the summary (thinking and raw tool data left out). */
  function transcript(msgs) {
    const out = [];
    for (const m of msgs) {
      if (typeof m.content === 'string') { out.push(`${m.role === 'user' ? 'Owner' : 'Cru'}: ${m.content}`); continue; }
      for (const b of m.content || []) {
        if (b.type === 'text' && b.text) out.push(`${m.role === 'user' ? 'Owner' : 'Cru'}: ${b.text}`);
        else if (b.type === 'tool_use') out.push(`(Cru used ${b.name} ${JSON.stringify(b.input).slice(0, 200)})`);
        else if (b.type === 'tool_result') out.push(`(result: ${String(typeof b.content === 'string' ? b.content : JSON.stringify(b.content)).slice(0, 300)})`);
      }
    }
    return out.join('\n').slice(-60000);
  }

  /**
   * Compaction while idle (§2.5.2): the conversation so far becomes a short
   * summary, and a NEW conversation starts from it. Earlier turns are never
   * edited (the models refuse edited history); the old ones stay findable
   * through memory_search, which indexed every turn as it happened.
   */
  async function compact({ force = false, now = new Date() } = {}) {
    if (busy) return { ok: false, skipped: 'busy' };
    if (noCompact) { if (force) { history = []; lastReq = null; save(); return { ok: true, summary: '' }; } return { ok: false, skipped: 'not for this conversation' }; }
    if (!force && history.length < COMPACT_MIN_MESSAGES) return { ok: false, skipped: 'short' };
    const s = settings.getSettings();
    if (!s.anthropicKey || settings.overLimit(now)) return { ok: false, skipped: 'no key or over the limit' };
    busy = true;
    try {
      const model = s.model || settings.DEFAULT_MODEL;
      let files = null;
      try { files = selfFiles ? selfFiles() : null; } catch { files = null; }
      const res = await client(s.anthropicKey).messages.create({
        model, max_tokens: files ? 4000 : 1500, output_config: { effort: 'low' },
        system: 'You summarise a voice assistant\'s conversation with its user (the owner, addressed as Sir) so it can carry on from the summary. Keep what matters for later: decisions, requests in progress, actions taken (what, where), what is waiting on someone, facts learnt about the owner\'s projects and preferences. Plain prose and short lists, no preamble, under 400 words.',
        messages: [{ role: 'user', content: `The conversation so far:\n\n${transcript(history)}\n\nWrite the summary.${files ? reflectionAsk(files) : ''}` }],
      });
      settings.addClaudeSpend(model, res.usage, now);
      const answer = res.content.filter(b => b.type === 'text').map(b => b.text).join('\n').trim();
      const learnt = parseReflection(answer);
      const summary = learnt.summary;
      if (files && (learnt.persona || learnt.owner)) { try { onReflected({ persona: learnt.persona, owner: learnt.owner }); } catch { /* best effort */ } }
      if (!summary) return { ok: false, skipped: 'empty summary' };
      const when = now.toLocaleString('en-GB', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });
      history = [
        { role: 'user', content: `${personaNote()}[What we have been doing until ${when}, summarised. Earlier detail is in memory_search.]\n${summary}` },
        { role: 'assistant', content: [{ type: 'text', text: 'Understood, Sir.' }] },
      ];
      lastReq = null;
      try { onCompacted(summary, now.getTime()); } catch { /* best effort */ }
      return { ok: true, summary };
    } catch (e) {
      return { ok: false, skipped: String(e.message || e) };
    } finally {
      busy = false;
      save();
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

  /**
   * Something happened that the owner should hear about (a chat Cru watches
   * finished, or is waiting on them): Cru turns [note] into a sentence or two
   * to say, unprompted. Waits for a turn in progress to end (up to 30 s).
   */
  async function report(note, { now = new Date(), wake = false } = {}) {
    for (let i = 0; i < 60 && busy; i++) await new Promise(ok => setTimeout(ok, 500));
    if (busy) return { ok: false, error: 'busy' };
    return turn(reportPrompt(note, { wake }), { where: wake ? 'a wake-up' : 'a report', now, report: true });
  }

  return {
    turn,
    report,
    cancel,
    warm,
    /** Start a fresh conversation. */
    compact,
    /** Quiet since (ms epoch), for compaction while idle. */
    get lastActivity() { return lastActivity; },
    reset() { history = []; lastReq = null; save(); },
    /** A fresh conversation that starts from [summary] alone (Cru's start_fresh). */
    fresh(summary, now = new Date()) {
      const s = String(summary || '').trim();
      if (!s) { history = []; lastReq = null; save(); return { ok: true }; }
      const when = now.toLocaleString('en-GB', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });
      history = [
        { role: 'user', content: `${personaNote()}[A fresh start at ${when}. Carried over, in your own words:]\n${s}` },
        { role: 'assistant', content: [{ type: 'text', text: 'Understood, Sir.' }] },
      ];
      lastReq = null;
      try { onCompacted(s, now.getTime()); } catch { /* best effort */ }
      save();
      return { ok: true };
    },
    /** Tests and diagnostics. */
    get history() { return history; },
    get toolNames() { return toolDefs.map(t => t.name); },
  };
}
