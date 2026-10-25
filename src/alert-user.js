/**
 * alert-user.js — the alert_user MCP tool: getting the person's attention now.
 *
 * send_message_to_user is for news. This is for the few things that cannot
 * wait: a production alert, a long job that needs a decision, "wake me when
 * it is done". It goes to the paired phones as a push of its own kind
 * ("alert"), and an urgent one rings there on the app's Urgent channel until
 * it is seen (android/URGENT-ALERTS.md says what the phone does, and why it
 * stops short of full-screen intents and fake calls).
 *
 * A chat in a loop must not be able to ring somebody all night, so the
 * server counts:
 *
 *   - At most URGENT_PER_HOUR urgent alerts ring in any rolling hour. Past
 *     that an urgent alert is still sent, as a normal one that does not ring
 *     (marked `downgraded` so the phone's log can say why), and the tool says
 *     when urgent is possible again.
 *   - At most ALERTS_PER_HOUR alerts of any level go out in a rolling hour.
 *     Past that the tool refuses, and says when it can send again.
 *
 * The counts are kept in memory. A restart forgets them, which costs at most
 * one more hour's allowance; it is not worth a file.
 *
 * With no phone to ring (no relay key, nobody paired, or phone notifications
 * turned off in Settings) the text still goes out, through whichever
 * notification channels are on, and the result says it could not ring a
 * phone and why.
 */

import { randomBytes } from 'node:crypto';

export const URGENT_PER_HOUR = 3;
export const ALERTS_PER_HOUR = 20;
export const WINDOW_MS = 60 * 60 * 1000;
export const MAX_TITLE = 120;
export const MAX_BODY = 1500;
// After an urgent alert rings, how long that chat's own notifications (its
// "finished" a second later) are sent quietly. Android plays one sound at a
// time: a notification that sounds or buzzes stops the alert's ringing.
export const HUSH_MS = 2 * 60 * 1000;
// Waiting for the answer to an urgent alert. The phone says it is ringing,
// and for how long (its "Ring for" setting, 1 to 15 minutes); the call then
// waits that long, plus a little for the answer to travel.
export const ACK_MS = 60 * 1000;            // no "ringing" from the phone by then: it cannot report (an older app, or offline)
export const ANSWER_GRACE_MS = 20 * 1000;
export const RING_MIN_MS = 60 * 1000;
export const RING_MAX_MS = 15 * 60 * 1000;
// However the phone is set, a caller never waits longer than this from the
// moment it starts waiting: 15 minutes is the longest an alert rings.
export const WAIT_MAX_MS = RING_MAX_MS;
const ANSWERS = new Set(['attended', 'dismissed', 'snoozed']);
export const SNOOZE_MINUTES = [5, 15, 30, 60];

const LEVELS = new Set(['normal', 'urgent']);

function clean(s, max) {
  return String(s ?? '').replace(/\r\n?/g, '\n').trim().slice(0, max);
}

// "in 12 minutes", for a tool result read by Claude and passed on.
function inWords(ms) {
  const min = Math.max(1, Math.ceil(ms / 60000));
  return `in ${min} minute${min === 1 ? '' : 's'}`;
}

/**
 * @param {object} deps
 * @param {() => ({ ok: boolean, reason?: string })} deps.phone  Whether a phone can be rung, and if not, why.
 * @param {(text: string, meta: object) => Promise<boolean>} deps.push  mobile-push send(): true if the relay took it.
 * @param {(text: string, meta: object, opts?: object) => Promise<Array<{id: string, ok: boolean}>>} deps.deliver  notify-channels deliver().
 * @param {() => number} [deps.now]
 */
export function createAlerter({ phone, push, deliver, now = Date.now, timers = { set: setTimeout, clear: clearTimeout } }) {
  // When each one went out, oldest first. Only alerts that were sent count.
  const sent = [];      // every alert, any level
  const rang = [];      // urgent alerts sent to ring
  const rangFor = new Map();   // chat id -> when an urgent alert from it was sent to ring
  const waiting = new Map();   // alert id -> { finish, ringMs, timer, early } while a call waits for its answer

  const prune = (list, t) => { while (list.length && t - list[0] >= WINDOW_MS) list.shift(); };

  /** What the counters say now, for tests and for a status line. */
  function usage() {
    const t = now();
    prune(sent, t); prune(rang, t);
    return { alerts: sent.length, urgent: rang.length, alertsLimit: ALERTS_PER_HOUR, urgentLimit: URGENT_PER_HOUR };
  }

  /**
   * @param {{ title?: string, body?: string, level?: string }} args
   * @param {{ chat?: object, url?: string }} [where]  The chat the alert is about, when the call came from one.
   */
  async function alert(args = {}, where = {}) {
    const title = clean(args.title, MAX_TITLE);
    if (!title) return { ok: false, error: 'alert_user needs a title: one short line saying what needs the person.' };
    const body = clean(args.body, MAX_BODY);
    const wanted = args.level === undefined || args.level === null || args.level === '' ? 'urgent' : String(args.level).toLowerCase();
    if (!LEVELS.has(wanted)) return { ok: false, error: 'level must be "normal" or "urgent".' };

    const t = now();
    prune(sent, t); prune(rang, t);
    if (sent.length >= ALERTS_PER_HOUR) {
      return {
        ok: false,
        error: `Not sent: ${ALERTS_PER_HOUR} alerts have gone out in the last hour, which is the limit. The next one can go ${inWords(sent[0] + WINDOW_MS - t)}. For an ordinary update use send_message_to_user.`,
      };
    }

    let level = wanted;
    let downgraded = false;
    let note = '';
    if (wanted === 'urgent' && rang.length >= URGENT_PER_HOUR) {
      level = 'normal';
      downgraded = true;
      note = `${URGENT_PER_HOUR} urgent alerts have already rung in the last hour, so this one was sent as a normal alert that does not ring. Urgent is possible again ${inWords(rang[0] + WINDOW_MS - t)}.`;
    }

    const chat = where.chat && where.chat.id ? where.chat : null;
    const url = where.url || (chat ? `/chat/${chat.id}` : '/');
    const alertId = randomBytes(8).toString('hex');
    const meta = {
      title,
      tag: 'alert',
      url,
      alertId,
      ...(chat ? { chat } : {}),
      ...(level === 'urgent' ? { urgent: true } : {}),
      ...(downgraded ? { downgraded: true } : {}),
    };
    // A notification must say something to be shown at all (an app shows
    // nothing for an empty body), so a title alone is repeated as the body.
    const text = body || title;

    const p = phone();
    let pushed = false;
    let why = p.ok ? '' : (p.reason || 'No phone can be reached.');
    if (p.ok) {
      try { pushed = await push(text, meta); } catch (err) { why = `Sending to the phone failed: ${err.message}`; }
      if (!pushed && !why) why = 'The push relay did not take it (see Settings, Notifications, for the reason).';
    }

    if (pushed) {
      sent.push(t);
      if (level === 'urgent') {
        rang.push(t);
        if (chat) rangFor.set(chat.id, t);
      }
      return {
        ok: true,
        delivered: 'phone',
        level,
        // Only an alert that rings has an answer to wait for (waitForAnswer).
        ...(level === 'urgent' ? { alertId } : {}),
        ...(downgraded ? { downgraded: true } : {}),
        note: note || (level === 'urgent'
          ? 'Sent to the paired phone, where it rings until the person sees it (unless they have turned urgent alerts off for this server).'
          : 'Sent to the paired phone as a normal alert.'),
      };
    }

    // No phone: the other channels, so the words still reach somebody.
    // The phone channel is left out: it is the one that just failed.
    let results = [];
    try {
      results = await deliver(title === text ? title : `${title}\n${text}`, { title, tag: 'alert', url, ...(chat ? { chat } : {}) }, { except: 'mobile' });
    } catch { results = []; }
    const through = results.filter((r) => r.ok && r.id !== 'mobile').map((r) => r.id);
    if (!through.length) {
      return { ok: false, error: `Could not ring a phone (${why}) and no other notification channel is turned on, so the alert reached nobody. Say it in your reply instead.` };
    }
    sent.push(t);
    return {
      ok: true,
      delivered: 'channels',
      channels: through,
      level: 'normal',
      note: `Could not ring a phone: ${why} It was sent as an ordinary notification through ${through.join(', ')} instead.${note ? ' ' + note : ''}`,
    };
  }

  /**
   * Did this chat send an urgent alert that rang in the last HUSH_MS? Its own
   * notifications should then not sound, or they would cut the ringing short.
   */
  function hushing(chatId) {
    const t = now();
    for (const [id, at] of rangFor) if (t - at >= HUSH_MS) rangFor.delete(id);
    return !!chatId && rangFor.has(String(chatId));
  }

  /**
   * The phone says what happened to an alert: "ringing" (with ringMs, how
   * long it will ring), then "attended", "dismissed" or "snoozed" (with
   * minutes). False for an alert nobody is waiting on (already answered, or
   * the call gave up).
   */
  function answer(alertId, a = {}) {
    const w = waiting.get(String(alertId || ''));
    const action = String(a.action || '');
    if (!w) return false;
    if (action === 'ringing') {
      const ringMs = Math.min(RING_MAX_MS, Math.max(RING_MIN_MS, Number(a.ringMs) || 0));
      w.ringMs = ringMs;
      w.arm(Math.min(ringMs + ANSWER_GRACE_MS, Math.max(0, WAIT_MAX_MS - (now() - w.started))), 'no answer');
      return true;
    }
    if (!ANSWERS.has(action)) return false;
    const minutes = action === 'snoozed' && SNOOZE_MINUTES.includes(Number(a.minutes)) ? Number(a.minutes) : undefined;
    w.finish({ answer: action, ...(minutes ? { snoozeMinutes: minutes } : {}) });
    return true;
  }

  /**
   * Wait for the answer to an urgent alert (its alertId from alert()). Ends
   * with the person's answer, "no answer" once the phone's ring time is up,
   * or "not confirmed" when the phone never said it was ringing.
   */
  function waitForAnswer(alertId) {
    const id = String(alertId || '');
    return new Promise((resolve) => {
      const started = now();
      let timer = null;
      const w = {
        ringMs: 0,
        started,
        finish(r) {
          if (!waiting.has(id)) return;
          waiting.delete(id);
          if (timer) timers.clear(timer);
          resolve({ ...r, ...(w.ringMs ? { ringMinutes: Math.round(w.ringMs / 60000) } : {}), waitedSeconds: Math.round((now() - started) / 1000) });
        },
        arm(ms, outcome) {
          if (timer) timers.clear(timer);
          timer = timers.set(() => w.finish({ answer: outcome }), ms);
        },
      };
      waiting.set(id, w);
      w.arm(ACK_MS, 'not confirmed');
    });
  }

  return { alert, usage, hushing, answer, waitForAnswer };
}

/** The sentence a caller reads for an answer, from waitForAnswer's result. */
export function describeAnswer(r = {}) {
  switch (r.answer) {
    case 'attended': return 'The person attended the alert (opened it).';
    case 'dismissed': return 'The person dismissed the alert without opening it.';
    case 'snoozed': return `The person snoozed the alert for ${r.snoozeMinutes || '?'} minutes; it will ring again then.`;
    case 'no answer': return `Nobody answered: the alert rang for ${r.ringMinutes || '?'} minutes and stopped (the phone keeps a missed-alert notice).`;
    case 'not confirmed': return 'The phone did not confirm it was ringing within a minute (an older app, or the phone is offline), so how it was answered is not known.';
    default: return '';
  }
}
