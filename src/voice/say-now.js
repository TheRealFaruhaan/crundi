/**
 * voice/say-now.js — Cru says something now, as its own message, and carries
 * on with its turn (capability 'voice-say-now', the owner, 12 Nov 2026).
 *
 * Before, Cru spoke once per turn: the reply at the end. Now, during a turn,
 * say_now puts a line in front of the owner straight away, shown and spoken
 * as a separate message: a short update while it works ("Checking the
 * server…"), one step of something it walks him through (say a step, ring
 * the button with show_in_crundi, say the next), or anything else better said
 * before the final answer, which still comes at the end as usual.
 *
 * How it reaches him:
 *  - an owner's turn from a screen that can show it (POST /api/voice/turn
 *    with sayNow: true and a requestId): { type: 'update', requestId, id,
 *    speak } on /api/voice/listen; the screen whose request it is shows it as
 *    its own message and says it, and answers POST /api/voice/update/ack
 *    { id, how: 'said' | 'shown' | 'cut' } once it has been said (or only
 *    shown: typed, muted, he was speaking). The tool waits for that answer
 *    (a bounded time), so what Cru does next (the next ring, the next step)
 *    lines up with what he has just heard;
 *  - a report or a wake-up (nobody waits on a turn): kept, and said before
 *    the report itself, each as its own message (webapp.js cruDeliver);
 *  - an older screen (no sayNow): kept, and put in front of the reply.
 * Every line is also in the turn's result (updates: [{ id, speak, at }]) and
 * kept with the turn, so the chat view shows each as its own message.
 *
 * [point] rings something at the same time (show_in_crundi's own arguments):
 * the ring and the words of a step arrive together.
 */

export const SAY_NOW_TOOL_NAMES = ['say_now'];
export const SAY_MAX_CHARS = 400;
/** At most this many lines in one turn: an update is not a running commentary. */
export const SAY_MAX_PER_TURN = 12;
/** How long the tool waits for the screen to say a line: a base plus its length at a slow talking speed, within a cap. */
export function heardWithinMs(text) { return Math.min(25000, 4000 + String(text || '').length * 90); }

const clip = (s, n) => String(s == null ? '' : s).replace(/\s+/g, ' ').trim().slice(0, n);

/** The screens' answers, by line id (as ui-nav.js createAcks, with a timeout of each line's own). */
export function createSayAcks() {
  const waiting = new Map();
  return {
    wait(id, ms) {
      return new Promise((resolve) => {
        const timer = setTimeout(() => { waiting.delete(id); resolve(null); }, ms);
        if (timer.unref) timer.unref();
        waiting.set(id, (r) => { clearTimeout(timer); waiting.delete(id); resolve(r); });
      });
    },
    /** false when nobody was waiting for that id (late, or another server's). */
    answer(id, r) { const f = waiting.get(String(id || '')); if (!f) return false; f(r || {}); return true; },
    get size() { return waiting.size; },
  };
}

/**
 * @param {object} o
 * @param {(msg: object) => number} o.send   to the open voice screens; answers how many got it
 * @param {{wait: Function}} o.acks          createSayAcks()
 * @param {(args: object) => Promise<object>} [o.point]  show_in_crundi's run
 * @param {() => number} [o.now]
 */
export function makeSayNowTools({ send, acks, point = null, now = () => Date.now() }) {
  let seq = 0;
  return [{
    name: 'say_now',
    description: 'Say something to the owner now, as its own message, and carry on with this turn. Use it whenever something is better said before your final answer than after it: '
      + 'a short update while a request takes several steps ("Checking the server…", "Found it: the disk is full. Clearing the logs now."), each step of something you walk him through '
      + '(say a step and ring what to click with point, wait for this to return, then the next step), a heads-up before something slow, or a question you can carry on without. '
      + 'Each call is shown and spoken on his screen (PC app, web, phone) as a separate message, in order; it returns once he has heard it (or it was only shown), so what you do next lines up with what he heard. '
      + 'Your final answer still comes at the end as usual: do not repeat in it what you already said (it may be short, or just the last step). '
      + 'One or two plain sentences each, never markdown; not for every tool you call, and not for a quick answer (just answer). '
      + 'point (optional) rings something on his Crundi page at the same moment, with show_in_crundi\'s own arguments (tab, highlight, card, ...); not on the phone.',
    input_schema: {
      type: 'object', additionalProperties: false,
      properties: {
        text: { type: 'string', description: 'What to say now: one or two short sentences' },
        point: {
          type: 'object',
          description: 'Optional: ring something on his Crundi page as this is said (as show_in_crundi: project, tab, chat, card, node, file, widget, highlight, note)',
          properties: {
            project: { type: 'string' }, tab: { type: 'string' }, chat: { type: 'string' }, card: { type: 'string' }, node: { type: 'string' },
            file: { type: 'string' }, widget: { type: 'string' }, highlight: { type: 'string' }, note: { type: 'string' },
          },
        },
      },
      required: ['text'],
    },
    run: async (input = {}, signals = null) => {
      const text = clip(input.text, SAY_MAX_CHARS);
      if (!text) return { ok: false, error: 'Nothing to say: give text' };
      const list = signals && Array.isArray(signals.updates) ? signals.updates : null;
      if (list && list.length >= SAY_MAX_PER_TURN) return { ok: false, error: `That is ${SAY_MAX_PER_TURN} already in this turn: put the rest in your answer` };
      // The ring first, so the step's words and its ring come together.
      let pointed = null;
      if (input.point && typeof input.point === 'object' && Object.keys(input.point).length) {
        if (!point) pointed = { ok: false, error: 'Pointing is not available here' };
        else { try { pointed = await point(input.point); } catch (e) { pointed = { ok: false, error: String(e.message || e) }; } }
      }
      const update = { id: `say${now().toString(36)}${(++seq).toString(36)}`, speak: text, at: now() };
      const out = { ok: true, update };
      if (pointed) out.pointed = pointed.ok === false ? { ok: false, error: pointed.error } : { ok: true, ...(pointed.found === false ? { found: false } : {}), ...(pointed.unconfirmed ? { unconfirmed: true } : {}) };
      // A report or a wake-up, or a screen that cannot show it: said with the answer, in front of it.
      if (!signals || signals.report || !signals.requestId || !signals.sayNow) {
        out.note = signals && signals.report ? 'Kept: it is told just before your report, as its own message.' : 'Kept: this screen shows it with your answer, in front of it.';
        return out;
      }
      const heard = acks.wait(update.id, heardWithinMs(text));
      const got = send({ type: 'update', requestId: signals.requestId, id: update.id, speak: text });
      if (!got) { out.note = 'No voice screen is open to say it now: it comes with your answer.'; return out; }
      const r = await heard;
      if (!r) out.note = 'Sent; the screen did not say when it was heard. Carry on.';
      else out.heard = r.how === 'shown' ? 'shown (not said aloud: he typed, Cru is muted, or he was speaking)' : r.how === 'cut' ? 'he cut in while it was said: listen to him in your answer' : 'said';
      return out;
    },
  }];
}

/**
 * The reply for a screen that cannot show lines on their own (no sayNow): the
 * lines said during the turn put in front of its words.
 */
export function foldUpdates(r) {
  if (!r || !r.ok || !Array.isArray(r.updates) || !r.updates.length) return r;
  const said = r.updates.map((u) => String(u.speak || '').trim()).filter(Boolean);
  return { ...r, speak: [...said, String(r.speak || '').trim()].filter(Boolean).join(' ') };
}
