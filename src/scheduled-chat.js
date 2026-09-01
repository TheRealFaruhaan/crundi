/**
 * scheduled-chat.js — run a Claude chat on a schedule, then tidy up after it.
 *
 * A scheduled chat opens a real chat cell, sends its prompt, and closes itself
 * once the work has genuinely finished. It is "background" in the sense that
 * you never start it — not in the sense that it is hidden. It stays visible
 * while it runs so you can watch it, or step in when it wants something.
 *
 * ─── When it is safe to close ───
 *
 * Two rules, both learned the hard way:
 *
 * A turn ending is NOT the work finishing. A turn that parks on a background
 * command or a Monitor ends and then resumes on its own; Crundi reports that
 * as 'waiting'. Closing there would kill the job mid-flight, and the evidence
 * would be a job that "ran" and did nothing.
 *
 * And idle is not instantly trustworthy either. The CLI passes through idle
 * between turns, so a close fired the moment it goes idle can land in a gap.
 * Closing waits for the session to STAY idle for a while, and any move back to
 * working or waiting cancels it.
 *
 * Anything that is not a clean finish — it needs input, it errored, it ran too
 * long — leaves the chat open and says so. Closing a session that wanted
 * something from you would throw the work away silently, which is the one
 * outcome worse than a chat left on screen.
 */

const SETTLE_MS = 20000;             // idle must hold this long before closing
const MAX_RUN_MS = 60 * 60 * 1000;   // an hour, then stop waiting and report

/** Prompt preamble telling the job how to change what it does next time. */
export function jobPreamble(schedule) {
  return [
    `[Scheduled job "${schedule.name || schedule.id}" (id: ${schedule.id})]`,
    'You are running unattended on a schedule. Nobody is watching this turn, so',
    'do not ask questions — - either do the work or report why you could not.',
    'To change what you do on the NEXT run, call the schedule_update MCP tool',
    `with id "${schedule.id}" and a new action.prompt. That is how you leave`,
    'notes for your future self.',
    '',
  ].join('\n');
}

/**
 * Run one scheduled chat.
 *
 * Resolves once the job has finished or been given up on. Never throws: a
 * scheduler tick should not die because one job did.
 *
 * @returns {Promise<{ok:boolean, outcome:string, sessionId?:string, output?:string, error?:string}>}
 */
export async function runScheduledChat({
  schedule, claudeUi, notify = () => {}, settleMs = SETTLE_MS, maxRunMs = MAX_RUN_MS,
}) {
  const a = schedule.action || {};
  const label = schedule.name || 'Scheduled chat';
  const prompt = String(a.prompt || '').trim();
  if (!prompt) return { ok: false, outcome: 'no-prompt', error: 'This schedule has no prompt' };

  const created = await claudeUi.create(schedule.project, {
    title: label,
    cwd: a.cwd || '',
    model: a.model || '',
    effort: a.effort || '',
    skipPermissions: a.mode === 'skip',
    // 'resume' reattaches the one conversation this job owns; anything else
    // starts clean so runs cannot contaminate each other.
    sessionMode: a.session === 'resume' && a.sessionId ? 'resume' : 'new',
    resumeId: a.session === 'resume' ? (a.sessionId || '') : '',
    background: true,
  });
  if (!created.ok) return { ok: false, outcome: 'start-failed', error: created.error };

  const id = created.id;
  const result = await watchToCompletion({ id, claudeUi, settleMs, maxRunMs, send: () => {
    claudeUi.sendMessage(id, jobPreamble(schedule) + prompt);
  } });

  const output = (() => {
    try { return claudeUi.lastTurnOutput(id) || ''; } catch { return ''; }
  })();

  if (result.outcome === 'finished') {
    try { await claudeUi.close(id); } catch { /* it may already be gone */ }
    notify('finished', label, output);
    return { ok: true, outcome: 'finished', sessionId: id, output };
  }

  // Everything else stays open on purpose.
  notify(result.outcome, label, output || result.error || '');
  return { ok: false, outcome: result.outcome, sessionId: id, output, error: result.error || '' };
}

/**
 * Watch a session until it has clearly finished, needs a human, or overruns.
 * Split out so the close rules can be tested without spawning a CLI.
 */
export function watchToCompletion({ id, claudeUi, settleMs = SETTLE_MS, maxRunMs = MAX_RUN_MS, send }) {
  return new Promise((resolve) => {
    let settleTimer = null;
    let done = false;

    const finish = (outcome, error) => {
      if (done) return;
      done = true;
      clearTimeout(settleTimer);
      clearTimeout(hardStop);
      try { claudeUi.off(id, onEvent); } catch { /* already detached */ }
      resolve({ outcome, error: error || '' });
    };

    const onEvent = (ev) => {
      if (!ev) return;
      if (ev.type === 'exit') return finish('exited', 'The session exited');
      if (ev.type !== 'state') return;

      // Any sign of life cancels a pending close. 'waiting' especially: the
      // turn ended, but it armed something and will come back by itself.
      if (ev.state !== 'idle') {
        clearTimeout(settleTimer);
        settleTimer = null;
      }
      if (ev.state === 'needs-input') return finish('needs-input');
      if (ev.state === 'idle' && !settleTimer) {
        // Idle has to HOLD. The CLI dips through idle between turns, and a
        // close fired on the first sighting lands in that gap.
        settleTimer = setTimeout(() => finish('finished'), settleMs);
      }
    };

    const hardStop = setTimeout(() => finish('overran', `Still going after ${Math.round(maxRunMs / 60000)} minutes`), maxRunMs);

    claudeUi.on(id, onEvent);
    try { send && send(); } catch (err) { finish('send-failed', err?.message || String(err)); }
  });
}
