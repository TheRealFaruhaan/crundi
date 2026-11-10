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
        // Matched on the words alone: a file's line here is shorter than the server's copy of it.
        val known = got.map { CruAttach.plain(it.said) to it.speak }.toSet()
        return got + current.filter { it.live && (CruAttach.plain(it.said) to it.speak) !in known }
    }

    /** An exchange that just happened, at the end; nothing when it is empty. */
    fun add(current: List<CruMsg>, said: String, speak: String, show: String, at: Long): List<CruMsg> {
        val m = CruMsg(at, said.trim(), speak.trim(), show.trim(), live = true, key = "l$at-${current.size}")
        return if (m.said.isEmpty() && m.speak.isEmpty() && m.show.isEmpty()) current else current + m
    }

    /** The words being heard now, shown as a faded bubble of the owner's: while listening or thinking. */
    fun hearing(s: VoiceState): String =
        if ((s.phase == Phase.Listening || s.phase == Phase.Thinking) && s.heard.isNotBlank()) s.heard.trim() else ""

    /** Cru is working on an answer: a typing bubble at the end of the list, where the reply will appear. */
    fun thinking(s: VoiceState): Boolean = s.phase == Phase.Thinking

    /** The message whose reply is being said now (the last with those words), or -1: the sound sign goes beside it. */
    fun speakingIndex(list: List<CruMsg>, s: VoiceState): Int {
        if (s.phase != Phase.Speaking) return -1
        val said = s.lastSaid.trim()
        if (said.isEmpty()) return -1
        return list.indexOfLast { it.speak == said }
    }

    /** Where to page back from: the oldest message's time, or 0 for the newest. */
    fun before(current: List<CruMsg>): Long = current.firstOrNull()?.at ?: 0L
}

/** The chat view's Thinking and Speaking signs, as numbers from an animation's [t] (0 to 1, looping). */
object VoiceIndicators {
    /** How far up dot [k] (0, 1, 2) of the thinking bubble is, 0 to 1: each rises in turn, a third of a loop apart. */
    fun dot(t: Float, k: Int): Float {
        val x = ((t - k / 3f) % 1f + 1f) % 1f
        return if (x < 0.4f) kotlin.math.sin(x / 0.4f * Math.PI.toFloat()).coerceIn(0f, 1f) else 0f
    }

    /** How lit wave [k] (0 inner, 1 outer) of the speaking speaker is, 0.25 to 1: the inner first, then the outer. */
    fun wave(t: Float, k: Int): Float {
        val x = ((t - k * 0.3f) % 1f + 1f) % 1f
        return if (x < 0.6f) (0.25f + 0.75f * kotlin.math.sin(x / 0.6f * Math.PI.toFloat())).coerceIn(0.25f, 1f) else 0.25f
    }
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
