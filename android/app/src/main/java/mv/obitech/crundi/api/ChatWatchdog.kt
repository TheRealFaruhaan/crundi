package mv.obitech.crundi.api

import mv.obitech.crundi.api.model.AgentState

/**
 * Notices a chat screen that has fallen behind the server mid-turn, so it
 * can catch up (CrundiClient.resyncChat). Asked every few seconds while the
 * chat is on screen; true means "resync now".
 *
 * Two signs, only while the screen shows the turn still working:
 * - the server's own list (the event stream's state, pushed every five
 *   seconds) has said for [DISAGREE_MS] that the chat is not working: the
 *   end of the turn was missed;
 * - nothing at all has come for the chat in [QUIET_MS]: the stream may have
 *   gone quiet without closing (and a chat the list leaves out, a factory's
 *   employee, has only this one).
 *
 * Pure, and the clock is passed in, so it is tested as it is.
 */
class ChatWatchdog {
    private var disagreeSince = -1L
    private var lastResync: Long? = null

    /**
     * [shown] is the state the screen has (from the socket), [server] the
     * state in the server's list (null when the list does not have it),
     * [lastMessageAt] when the socket last said anything for this chat.
     */
    fun check(shown: AgentState, server: AgentState?, now: Long, lastMessageAt: Long): Boolean {
        if (shown != AgentState.Working) { disagreeSince = -1; return false }
        val last = lastResync
        if (server != null && server != AgentState.Working) {
            if (disagreeSince < 0) disagreeSince = now
            // Once it has said so for a while (its push may simply be ahead
            // of the socket); after a resync that did not settle it, less often.
            val gap = if (last == null) 0L else RETRY_MS
            if (now - disagreeSince >= DISAGREE_MS && (last == null || now - last >= gap)) return fired(now)
        } else disagreeSince = -1
        val heard = if (last == null) lastMessageAt else maxOf(lastMessageAt, last)
        if (now - heard >= QUIET_MS) return fired(now)
        return false
    }

    private fun fired(now: Long): Boolean {
        lastResync = now
        disagreeSince = now
        return true
    }

    companion object {
        /** A little over one push of the server's state. */
        const val DISAGREE_MS = 8_000L
        /** Between resyncs while the server still disagrees. */
        const val RETRY_MS = 30_000L
        /** A turn with nothing at all for this long (a long tool run is quiet too, so not short). */
        const val QUIET_MS = 120_000L
    }
}
