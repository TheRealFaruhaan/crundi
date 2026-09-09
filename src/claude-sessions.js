/**
 * claude-sessions.js — who is currently attached to which conversation.
 *
 * ─── The problem ───
 *
 * A Claude Code transcript is a file, and `--resume <id>` opens it for writing.
 * Two processes resuming the same id both write to it. Nothing errors. The
 * transcript ends up interleaved, and the loser's turns are gone — noticed, if
 * at all, hours later as "it forgot what we were doing".
 *
 * Crundi makes this easy to walk into: the same project can have several chats
 * and several terminals open at once, and the resume picker is happy to offer a
 * conversation that is already on screen in another cell.
 *
 * ─── The fix ───
 *
 * Every live session claims the conversation it is attached to. When a launch
 * asks for one that is already claimed, we pass --fork-session: the CLI copies
 * the history into a NEW session id and leaves the original alone. The user
 * gets what they asked for — that conversation, continued — without the two
 * cells fighting over one file.
 *
 * Claims are in-memory only. A restart drops them all, which is correct: after
 * a restart there are no live sessions to collide with.
 *
 * ─── Why claims are approximate, and why that is fine ───
 *
 * With --resume we know the id outright. With --continue the CLI picks the
 * newest transcript itself and only reveals the id later (chat) or never
 * (terminal), so the claim starts as our best guess from the transcript
 * directory and is corrected once the CLI says otherwise.
 *
 * The cost of guessing wrong is asymmetric and both directions are survivable:
 * a false claim forks a conversation that did not need forking (the user keeps
 * their history, in a new id); a missed claim is the status quo. Neither loses
 * work, which is the only outcome worth avoiding.
 */

/** owner key (e.g. 'chat:abc123' / 'term:def456') → session id */
const claims = new Map();

/** Attach an owner to a conversation id. Passing '' just clears the claim. */
export function claimSession(owner, sessionId) {
  if (!owner) return;
  const id = String(sessionId || '').trim();
  if (id) claims.set(owner, id);
  else claims.delete(owner);
}

/** Drop an owner's claim — call this when the process exits, not before. */
export function releaseSession(owner) {
  if (owner) claims.delete(owner);
}

/**
 * Is this conversation already claimed by someone other than `owner`?
 * @param sessionId  the id about to be resumed
 * @param owner      the claimant to ignore (itself, on a re-check)
 */
export function isSessionClaimed(sessionId, owner = '') {
  const id = String(sessionId || '').trim();
  if (!id) return false;
  for (const [k, v] of claims) {
    if (v === id && k !== owner) return true;
  }
  return false;
}

/** Every claimed conversation id. Used to mark rows in the resume picker. */
export function claimedSessionIds() {
  return new Set(claims.values());
}

/** Test seam. */
export function _reset() { claims.clear(); }
