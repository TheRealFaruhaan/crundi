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

export const URGENT_PER_HOUR = 3;
export const ALERTS_PER_HOUR = 20;
export const WINDOW_MS = 60 * 60 * 1000;
export const MAX_TITLE = 120;
export const MAX_BODY = 1500;

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
export function createAlerter({ phone, push, deliver, now = Date.now }) {
  // When each one went out, oldest first. Only alerts that were sent count.
  const sent = [];      // every alert, any level
  const rang = [];      // urgent alerts sent to ring

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
    const meta = {
      title,
      tag: 'alert',
      url,
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
      if (level === 'urgent') rang.push(t);
      return {
        ok: true,
        delivered: 'phone',
        level,
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

  return { alert, usage };
}
