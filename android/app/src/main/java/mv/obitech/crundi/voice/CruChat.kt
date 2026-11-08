package mv.obitech.crundi.voice

import mv.obitech.crundi.api.VoiceTurn

/**
 * One exchange in the voice screen's chat view: what the owner [said] (""
 * when Cru spoke up on its own: a report, a ticker, a trigger) and Cru's
 * answer, [speak] said aloud and [show] only shown. [live]: it happened
 * while this screen was open, not loaded from the server's history.
 */
data class CruMsg(val at: Long, val said: String, val speak: String, val show: String, val live: Boolean = false, val key: String = "")

/** The chat view's list, with no Android in it (CruChatTest). */
object CruChat {
    /** GET /api/voice/history's turns (oldest first) as messages. */
    fun fromTurns(turns: List<VoiceTurn>): List<CruMsg> =
        turns.map { CruMsg(it.at, if (it.report) "" else it.said.trim(), it.speak.trim(), it.show.trim(), key = "t" + it.id + "-" + it.at) }
            .filter { it.said.isNotEmpty() || it.speak.isNotEmpty() || it.show.isNotEmpty() }

    /**
     * [got] from the server joined to what is shown: [older] pages go before
     * it; a fresh load replaces it, keeping the exchanges made here since
     * that the server's list does not have yet.
     */
    fun merge(current: List<CruMsg>, got: List<CruMsg>, older: Boolean): List<CruMsg> {
        if (older) return got + current
        val known = got.map { it.said to it.speak }.toSet()
        return got + current.filter { it.live && (it.said to it.speak) !in known }
    }

    /** An exchange that just happened, at the end; nothing when it is empty. */
    fun add(current: List<CruMsg>, said: String, speak: String, show: String, at: Long): List<CruMsg> {
        val m = CruMsg(at, said.trim(), speak.trim(), show.trim(), live = true, key = "l$at-${current.size}")
        return if (m.said.isEmpty() && m.speak.isEmpty() && m.show.isEmpty()) current else current + m
    }

    /** The words being heard now, shown as a faded bubble of the owner's: while listening or thinking. */
    fun hearing(s: VoiceState): String =
        if ((s.phase == Phase.Listening || s.phase == Phase.Thinking) && s.heard.isNotBlank()) s.heard.trim() else ""

    /** Where to page back from: the oldest message's time, or 0 for the newest. */
    fun before(current: List<CruMsg>): Long = current.firstOrNull()?.at ?: 0L
}

/** Voice or keyboard: which the voice screen opens in (CruModeTest). */
object CruMode {
    /**
     * Voice can't work here: no microphone permission, no Deepgram key on the
     * server, or Deepgram's limit reached and no recogniser on the phone to
     * take over. [deepgramKeySet] null: not known yet (never fetched).
     */
    fun voiceUnavailable(micGranted: Boolean, deepgramKeySet: Boolean?, deepgramLimitReached: Boolean, deviceRecogniser: Boolean): Boolean =
        !micGranted || deepgramKeySet == false || (deepgramLimitReached && !deviceRecogniser)

    /** Keyboard when voice can't work; otherwise the owner's last choice on this device ([saved] null: never chosen, voice). */
    fun startInKeyboard(saved: Boolean?, voiceUnavailable: Boolean): Boolean = voiceUnavailable || saved == true
}
