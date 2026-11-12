package mv.obitech.crundi.voice

import kotlinx.serialization.json.JsonArray
import mv.obitech.crundi.api.VoiceTurn

/**
 * One exchange in the voice screen's chat view: what the owner [said] (""
 * when Cru spoke up on its own: a report, a ticker, a trigger) and Cru's
 * answer, [speak] said aloud and [show] only shown. [live]: it happened
 * while this screen was open, not loaded from the server's history.
 * [media]: what the reply showed (capability "voice-reply-media"), drawn
 * under it (CruReplyMedia): pictures, players, files, frames, cards.
 */
data class CruMsg(val at: Long, val said: String, val speak: String, val show: String, val live: Boolean = false, val key: String = "", val media: JsonArray = NoMedia) {
    /** Cru's side has something to draw: words, or what it showed. */
    val answered: Boolean get() = speak.isNotEmpty() || show.isNotEmpty() || media.isNotEmpty()
}

/** No media: one shared empty list. */
val NoMedia = JsonArray(emptyList())

/** The chat view's list, with no Android in it (CruChatTest). */
object CruChat {
    /** GET /api/voice/history's turns (oldest first) as messages. */
    fun fromTurns(turns: List<VoiceTurn>): List<CruMsg> =
        turns.flatMap { t ->
            val said = if (t.report) "" else t.said.trim()
            val key = "t" + t.id + "-" + t.at
            val lines = t.updates.map { it.trim() }.filter { it.isNotEmpty() }
            // Lines Cru said during the turn (capability "voice-say-now"): his words, each line, then the answer, each its own message.
            if (lines.isEmpty()) listOf(CruMsg(t.at, said, t.speak.trim(), t.show.trim(), key = key, media = t.media ?: NoMedia))
            else listOf(CruMsg(t.at, said, "", "", key = key)) + lines.mapIndexed { i, l -> CruMsg(t.at, "", l, "", key = "$key-u$i") } +
                CruMsg(t.at, "", t.speak.trim(), t.show.trim(), key = "$key-a", media = t.media ?: NoMedia)
        }.filter { it.said.isNotEmpty() || it.answered }

    /**
     * [got] from the server joined to what is shown: [older] pages go before
     * it; a fresh load replaces it, keeping the exchanges made here since
     * that the server's list does not have yet.
     */
    fun merge(current: List<CruMsg>, got: List<CruMsg>, older: Boolean): List<CruMsg> {
        if (older) return got + current
        // Matched on the words alone: a file's line here is shorter than the server's copy of it.
        val known = got.map { CruAttach.plain(it.said) to it.speak }.toSet()
        // Sent here and not answered yet: gone once the server's newest turns have it (with its answer).
        val recent = got.takeLast(3).map { CruAttach.plain(it.said) }.toSet()
        return got + current.filter { it.live && (CruAttach.plain(it.said) to it.speak) !in known && !(!it.answered && CruAttach.plain(it.said) in recent) }
    }

    /**
     * What the owner just said or typed, shown at once (10 Nov: a typed "Yes"
     * never appeared while its turn ran): his bubble at the end, keyed by
     * [key] so the answer fills it in ([answered]) or a cancel takes it out ([drop]).
     */
    fun asked(current: List<CruMsg>, said: String, at: Long, key: String): List<CruMsg> {
        val t = said.trim()
        if (t.isEmpty() || current.any { it.key == key }) return current
        return current + CruMsg(at, t, "", "", live = true, key = key)
    }

    /** The answer to the message sent as [key]; added at the end when that message is not there. */
    fun answered(current: List<CruMsg>, key: String, said: String, speak: String, show: String, at: Long, media: JsonArray? = null): List<CruMsg> {
        val i = current.indexOfFirst { it.key == key }
        if (i < 0) return add(current, said, speak, show, at, media)
        return current.toMutableList().also { it[i] = it[i].copy(speak = speak.trim(), show = show.trim(), media = media ?: NoMedia) }
    }

    /** A message taken back (cancelled): out of the list. */
    fun drop(current: List<CruMsg>, key: String): List<CruMsg> = current.filterNot { it.key == key && !it.answered }

    /**
     * What waits to be asked next while Cru answers (said or typed meanwhile):
     * shown on the screen as queued, then asked as soon as the answer comes.
     */
    // Also while Cru speaks: words held then stay in sight as queued, never vanish (12 Nov).
    fun queued(s: VoiceState): String = if (s.phase == Phase.Thinking || s.phase == Phase.Speaking) s.pending.trim() else ""

    /** An exchange that just happened, at the end; nothing when it is empty. */
    fun add(current: List<CruMsg>, said: String, speak: String, show: String, at: Long, media: JsonArray? = null): List<CruMsg> {
        val m = CruMsg(at, said.trim(), speak.trim(), show.trim(), live = true, key = "l$at-${current.size}", media = media ?: NoMedia)
        return if (m.said.isEmpty() && !m.answered) current else current + m
    }

    /**
     * The words being heard now, shown as a faded bubble of the owner's: while
     * listening, and while Cru thinks only words still coming in (what was
     * asked is in the list already, what is queued shows as [queued]).
     */
    fun hearing(s: VoiceState): String {
        val h = s.heard.trim()
        if (h.isEmpty()) return ""
        return when (s.phase) {
            Phase.Listening -> h
            Phase.Thinking -> {
                // While Cru thinks, heard is the queue and then the words coming in: only those new words here.
                val p = s.pending.trim()
                val rest = if (p.isNotEmpty() && h.startsWith(p)) h.removePrefix(p).trim() else h
                if (rest == s.asked.trim() || rest == p) "" else rest
            }
            else -> ""
        }
    }

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
