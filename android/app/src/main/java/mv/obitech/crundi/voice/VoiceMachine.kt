package mv.obitech.crundi.voice

/**
 * The voice assistant's conversation rules, with no Android in them, so they
 * are tested on the JVM (VoiceMachineTest). VoiceController feeds events in
 * and carries out the effects that come back.
 *
 *   Starting ──Ready──▶ Listening ──Utterance──▶ Thinking ──Reply──▶ Speaking
 *                          ▲                        │ (more said: kept)  │
 *                          └──────── SpeechDone / BargeIn ◀─────────────┘
 *
 * - Cru's own voice never goes to the server. While it speaks, or a cue
 *   plays, the stream is paused and the microphone runs into the phone's own
 *   gate ([OwnerGate]: louder than Cru's echo, for a moment, and voiced).
 *   When that hears the owner ([VoiceEvent.VoiceHeard]) Cru stops at once and
 *   the stream resumes from a pre-roll of the last second and a bit, so his
 *   first words are kept. On a server that knows barge-in ([VoiceState.live],
 *   'ready' echoFilter) it is also told what Cru is saying ([VoiceEffect.Saying]
 *   / [VoiceEffect.Said]): its transcript filter is the second net. An older
 *   server gets the same pause, with the older level detector.
 * - What the owner says while Crundi is thinking is kept and sent the moment
 *   the answer comes (the answer is shown, not said: he has moved on).
 * - What the owner said can be taken back until the answer comes: the ×
 *   by his words ([VoiceEvent.CancelTapped]) or "scratch that" / "never mind"
 *   / "ignore that" ([VoiceEvent.Discard], from the server or [Speech.isDiscard]).
 *   Listening, the words are dropped ([VoiceEffect.DiscardStream]); thinking,
 *   the turn is cancelled on the server ([VoiceEffect.CancelTurn]), its cue
 *   stops, and its answer, when it comes, is neither said nor shown: only
 *   "Cancelled." (and what was already done). Turns are numbered
 *   ([VoiceState.turn]) so a late answer never lands on the next question.
 * - The moment something is asked, a short cue ("One moment, Sir.") plays
 *   ([VoiceEffect.Cue]), not for "I'm here" or reports.
 * - The microphone stays on for [silenceMs] after the last thing that
 *   happened (something said and sent, an answer, a tap), then the session
 *   ends: back to sleep. Words heard meanwhile (a television, a room) don't
 *   keep it on; only a sentence still being spoken holds it for [graceMs].
 * - A stream that can't open or drops moves to the phone's own recogniser
 *   (offline fallback, D37); everything else works the same.
 */
enum class Phase { Starting, Listening, Thinking, Speaking, Ended }

data class VoiceState(
    val phase: Phase = Phase.Starting,
    /** Words heard so far (live), or the last thing said. */
    val heard: String = "",
    /** What went to Claude last. */
    val asked: String = "",
    /** The spoken part of the last answer. */
    val reply: String = "",
    /** The on-screen part of the last answer (never spoken). */
    val detail: String = "",
    /** A line under everything: an error, "offline", "unlock to do that". */
    val notice: String = "",
    /** Said while Crundi was thinking: sent once the answer has been spoken. */
    val pending: String = "",
    /** Using the phone's own recogniser instead of the stream. */
    val offline: Boolean = false,
    /** The phone is locked: answers are read-only (spec §4). */
    val locked: Boolean = false,
    /** When words were last heard (or anything happened). */
    val quietSince: Long = 0,
    /** When the owner or Crundi last did something (said and sent, answered, tapped), for the microphone's timeout. */
    val activeSince: Long = 0,
    /** What Crundi last said aloud, and when it stopped: heard again just after, it is an echo. */
    val lastSaid: String = "",
    val saidUntil: Long = 0,
    /** A question waiting for the owner's yes (Phase 4): the screen shows Yes / No. */
    val question: String = "",
    /** The last answer needed the phone unlocked. */
    val needsUnlock: Boolean = false,
    /** The owner is done: close once the goodbye has been said. */
    val closing: Boolean = false,
    /** News that came while Cru was busy: said once it is free (speak to show). */
    val reports: List<Pair<String, String>> = emptyList(),
    /** The server drops Cru's own words: the stream stays on while it speaks (barge-in). */
    val live: Boolean = false,
    /** The cue playing now ("One moment, Sir."), or "". */
    val cue: String = "",
    /** The server can cancel a turn (capability "voice-cancel"): the × is offered. */
    val cancellable: Boolean = false,
    /** The number of the turn asked last: an answer to another one is not this one's. */
    val turn: Int = 0,
)

/** The × by the owner's words: while they can still be taken back. */
val VoiceState.canCancel: Boolean get() = cancellable && heard.isNotBlank() && (phase == Phase.Listening || phase == Phase.Thinking)

sealed interface VoiceEvent {
    /** [cancellable]: the server has "voice-cancel". */
    data class Opened(val locked: Boolean, val now: Long, val cancellable: Boolean = false) : VoiceEvent
    /** [live]: the server drops Cru's own words (barge-in with the stream on). */
    data class Ready(val now: Long, val live: Boolean = false) : VoiceEvent
    data class Interim(val text: String, val now: Long) : VoiceEvent
    data class Utterance(val text: String, val now: Long) : VoiceEvent
    /** Claude's answer, or why there is none. */
    /** [turn]: which turn it answers (-1: the one in progress); [cancelled]: it was cancelled, [acted] what it had already done. */
    data class Reply(val ok: Boolean, val speak: String, val show: String, val error: String, val now: Long, val question: String = "", val needsUnlock: Boolean = false, val close: Boolean = false, val turn: Int = -1, val cancelled: Boolean = false, val acted: List<String> = emptyList(), val silent: Boolean = false) : VoiceEvent
    /** The speaker on a reply in the chat view: say [text] now (only while just listening). */
    data class SayAgain(val text: String, val now: Long) : VoiceEvent
    /** The owner tapped the × by his words. */
    data class CancelTapped(val now: Long) : VoiceEvent
    /** The X on a faded bubble in the chat view: that phrase only ([queued]: the one waiting; else the words being heard). */
    data class DropLive(val queued: Boolean, val now: Long) : VoiceEvent
    /** The server dropped what he said ("scratch that"): { type: 'discard' }. */
    data class Discard(val now: Long) : VoiceEvent
    /** POST /api/voice/cancel answered: what the cancelled turn had already done. */
    data class Cancelled(val acted: List<String>, val now: Long) : VoiceEvent
    /** The owner tapped Yes or No (or typed): asked like something said. */
    data class Typed(val text: String, val now: Long) : VoiceEvent
    data class SpeechDone(val now: Long) : VoiceEvent
    /** The owner talked over Crundi. */
    data class BargeIn(val now: Long) : VoiceEvent
    /** The level detector heard a voice over Crundi's (it may be a cough, or Crundi's own echo). */
    data class VoiceHeard(val now: Long) : VoiceEvent
    /** The cue started playing / finished (or was stopped). */
    data class CueStarted(val text: String, val now: Long) : VoiceEvent
    data class CueDone(val now: Long) : VoiceEvent
    data class StreamLost(val why: String, val now: Long) : VoiceEvent
    data class Unlocked(val now: Long) : VoiceEvent
    data class Tick(val now: Long) : VoiceEvent
    data object Close : VoiceEvent
    /** Cru has news, unasked: said now if it is just listening, else once it is free. */
    data class Report(val speak: String, val show: String, val now: Long) : VoiceEvent
    /** The owner is doing something that is not words heard (typing in keyboard mode): the microphone's timeout starts again. */
    data class Active(val now: Long) : VoiceEvent
}

sealed interface VoiceEffect {
    data class Ask(val text: String) : VoiceEffect
    data class Say(val text: String) : VoiceEffect
    data object StopSaying : VoiceEffect
    data object PauseStream : VoiceEffect
    data object ResumeStream : VoiceEffect
    /** A live stream: Crundi starts saying [text] (the server drops it as an echo). */
    data class Saying(val text: String) : VoiceEffect
    /** A live stream: Crundi stopped talking. */
    data object Said : VoiceEffect
    /** Play a short "one moment" cue now, unless one is playing. */
    data object Cue : VoiceEffect
    data object StopCue : VoiceEffect
    /** Tell the stream to drop what it has heard so far ({ type: 'discard' }). */
    data object DiscardStream : VoiceEffect
    /** POST /api/voice/cancel: the turn in progress. */
    data object CancelTurn : VoiceEffect
    /** The stream is gone: listen with the phone's own recogniser from here on. */
    data object UseFallback : VoiceEffect
    data object Finish : VoiceEffect
    /** The owner asked Cru to close: the screen goes (not just to sleep). */
    data object Dismiss : VoiceEffect
}

class VoiceMachine(
    /** The microphone stays on this long after the last thing that happened. */
    val silenceMs: Long = 30_000,
    /** Words heard this recently hold the timeout: The owner may be mid-sentence. */
    val graceMs: Long = 3_000,
    /** Starting this long without the stream: try the phone's recogniser. */
    val startMs: Long = 8_000,
    /** How long after Crundi speaks its own words heard back count as an echo. */
    val echoMs: Long = 5_000,
    /** What to say when called while already listening: one of [Speech.HERE], never the same twice running. */
    val here: () -> String = Speech.hereLine(),
) {
    fun on(s: VoiceState, e: VoiceEvent): Pair<VoiceState, List<VoiceEffect>> {
        if (s.phase == Phase.Ended) return s to emptyList()
        return when (e) {
            is VoiceEvent.Opened -> s.copy(phase = Phase.Starting, locked = e.locked, quietSince = e.now, activeSince = e.now, cancellable = e.cancellable) to emptyList()
            is VoiceEvent.CancelTapped -> takeBack(s, e.now, fromServer = false)
            is VoiceEvent.DropLive -> dropLive(s, e.queued, e.now)
            is VoiceEvent.Discard -> takeBack(s, e.now, fromServer = true)
            is VoiceEvent.Cancelled -> if (s.phase == Phase.Listening && s.heard.isEmpty()) s.copy(notice = cancelledNotice(e.acted)) to emptyList() else s to emptyList()
            is VoiceEvent.Ready -> if (s.phase != Phase.Starting) s.copy(live = e.live && !s.offline) to emptyList() else {
                val ready = s.copy(phase = Phase.Listening, quietSince = e.now, activeSince = e.now, live = e.live && !s.offline)
                // News that came while it was starting: said now.
                if (ready.reports.isEmpty()) ready to emptyList() else ready.reports.first().let { (sp, sh) -> tell(ready.copy(reports = ready.reports.drop(1)), sp, sh, e.now) }
            }
            is VoiceEvent.Interim -> when {
                // Talking over Crundi: cut it off the moment his words come.
                s.phase == Phase.Speaking -> if (s.live && Speech.talksOver(e.text, s.lastSaid)) {
                    val (n, fx) = cutIn(s, e.now)
                    n.copy(heard = e.text) to fx
                } else s to emptyList()
                echoOf(s, e.text, e.now) -> s to emptyList()
                s.phase == Phase.Starting || s.phase == Phase.Listening -> s.copy(phase = Phase.Listening, heard = e.text, quietSince = e.now) to emptyList()
                s.phase == Phase.Thinking -> {
                    val n = s.copy(heard = join(s.pending, e.text), quietSince = e.now)
                    // Talking over the cue stops it.
                    if (s.cue.isNotEmpty() && Speech.talksOver(e.text, s.cue)) n.copy(cue = "") to (listOf<VoiceEffect>(VoiceEffect.StopCue) + doneFx(s))
                    else n to emptyList()
                }
                else -> s to emptyList()
            }
            is VoiceEvent.Utterance -> {
                val t = e.text.trim()
                when {
                    t.isEmpty() -> s to emptyList()
                    echoOf(s, t, e.now) -> s.copy(heard = "") to emptyList()
                    // "Scratch that" alone (an older server passes it on): take back, never ask it.
                    Speech.isDiscard(t) && (s.phase == Phase.Listening || s.phase == Phase.Thinking || s.phase == Phase.Starting) -> takeBack(s, e.now, fromServer = true)
                    // Over Cru: it stops, and nothing is asked.
                    Speech.isDiscard(t) && s.phase == Phase.Speaking -> if (s.live && Speech.talksOver(t, s.lastSaid, 1)) cutIn(s, e.now) else s to emptyList()
                    s.phase == Phase.Thinking -> s.copy(pending = join(s.pending, t), heard = join(s.pending, t), quietSince = e.now) to emptyList()
                    // Said over Crundi (the stream stayed on): cut it off and ask that now.
                    s.phase == Phase.Speaking -> if (s.live && Speech.talksOver(t, s.lastSaid, 1)) {
                        val (cut, fx0) = cutIn(s, e.now)
                        val (n, fx) = if (Speech.isCalling(t)) calledHere(cut, t, e.now) else ask(cut, t, e.now)
                        n to (fx0 + fx)
                    } else s to emptyList()
                    // "Hey Cru" while already awake: here at once, without asking Claude.
                    Speech.isCalling(t) -> calledHere(s, t, e.now)
                    else -> ask(s, t, e.now)
                }
            }
            is VoiceEvent.Reply -> {
                // Cancelled: never said or shown as an error; just what had already been done.
                if (e.cancelled) return (if (s.phase == Phase.Listening && s.heard.isEmpty()) s.copy(notice = cancelledNotice(e.acted)) else s) to emptyList()
                if (s.phase != Phase.Thinking || (e.turn >= 0 && e.turn != s.turn)) return s to emptyList()
                // [silent]: typed, so the answer is read, not said (no speech made: Cru works without Deepgram).
                val say = if (e.silent) "" else if (e.ok) e.speak.trim() else e.error.trim()
                val next = s.copy(reply = if (e.ok) e.speak.trim() else "", detail = if (e.ok) e.show.trim() else "", notice = if (e.ok) (if (e.needsUnlock) "Unlock the phone to do that." else "") else e.error.trim(), quietSince = e.now, activeSince = e.now, question = e.question, needsUnlock = e.needsUnlock, closing = e.ok && e.close, cue = "")
                // The owner spoke again while Crundi thought: that goes now; the answer stays on screen, unsaid.
                if (s.pending.isNotBlank()) {
                    val (n, fx) = ask(next.copy(pending = "", closing = false), s.pending, e.now, keepReply = true)
                    return n to fx
                }
                if (say.isEmpty()) afterSpeech(next, e.now)
                else next.copy(phase = Phase.Speaking, lastSaid = say) to sayFx(next, say)
            }
            is VoiceEvent.Typed -> when (s.phase) {
                Phase.Ended -> s to emptyList()
                Phase.Thinking -> s.copy(pending = join(s.pending, e.text)) to emptyList()
                Phase.Speaking -> {
                    val (n, fx) = ask(s.copy(phase = Phase.Listening), e.text, e.now)
                    n to (listOf(VoiceEffect.StopSaying) + doneFx(s) + fx)
                }
                else -> ask(s, e.text, e.now)
            }
            is VoiceEvent.SayAgain -> if (s.phase != Phase.Listening || s.heard.isNotEmpty() || s.pending.isNotBlank() || e.text.isBlank()) s to emptyList() else {
                val t = e.text.trim()
                val n = s.copy(phase = Phase.Speaking, lastSaid = t, quietSince = e.now, activeSince = e.now)
                n to sayFx(n, t)
            }
            is VoiceEvent.SpeechDone -> if (s.phase == Phase.Speaking) afterSpeech(s, e.now) else s to emptyList()
            is VoiceEvent.BargeIn -> if (s.phase == Phase.Speaking) cutIn(s, e.now) else s to emptyList()
            // The stream is on: wait for his words (a cough is not an interruption). Paused: this is all there is.
            // The phone's gate heard the owner over Cru: stop it here and now, and listen from his first word.
            is VoiceEvent.VoiceHeard -> when {
                s.phase == Phase.Speaking -> cutIn(s, e.now)
                !s.live && s.cue.isEmpty() -> s to emptyList()
                s.cue.isNotEmpty() -> s.copy(cue = "") to (listOf<VoiceEffect>(VoiceEffect.StopCue) + doneFx(s))
                else -> s to listOf(VoiceEffect.ResumeStream)
            }
            is VoiceEvent.CueStarted -> if (s.phase != Phase.Thinking) s to emptyList() else {
                // Its own words heard back are not the owner's.
                s.copy(cue = e.text, lastSaid = e.text, saidUntil = e.now + CUE_ECHO_MS) to holdFx(s, e.text)
            }
            is VoiceEvent.CueDone -> if (s.cue.isEmpty()) s to emptyList() else {
                s.copy(cue = "") to (if (s.phase == Phase.Thinking) doneFx(s) else emptyList())
            }
            is VoiceEvent.StreamLost -> if (s.offline) s to emptyList() else {
                s.copy(offline = true, live = false, notice = "Live speech recognition is unavailable (${e.why}). Using the phone's recogniser.", phase = if (s.phase == Phase.Starting) Phase.Listening else s.phase, quietSince = e.now) to listOf(VoiceEffect.UseFallback)
            }
            is VoiceEvent.Unlocked -> s.copy(locked = false, notice = "", quietSince = e.now, activeSince = e.now) to emptyList()
            is VoiceEvent.Tick -> when {
                s.phase == Phase.Starting && !s.offline && e.now - s.quietSince >= startMs ->
                    on(s, VoiceEvent.StreamLost("no answer from the server", e.now))
                s.phase == Phase.Listening && s.pending.isEmpty() && e.now - s.activeSince >= silenceMs && e.now - s.quietSince >= graceMs ->
                    s.copy(phase = Phase.Ended) to listOf(VoiceEffect.Finish)
                else -> s to emptyList()
            }
            VoiceEvent.Close -> s.copy(phase = Phase.Ended) to listOf(VoiceEffect.StopSaying, VoiceEffect.Finish)
            is VoiceEvent.Active -> s.copy(activeSince = e.now, quietSince = e.now) to emptyList()
            is VoiceEvent.Report -> if (s.phase == Phase.Listening && s.pending.isEmpty() && s.heard.isEmpty()) tell(s, e.speak, e.show, e.now)
                else s.copy(reports = s.reports + (e.speak to e.show)) to emptyList()
        }
    }

    /**
     * The owner takes back what he said: listening, the words go (the stream
     * is told, unless it was the server that dropped them); thinking, the turn
     * is cancelled, with what he said meanwhile, and its cue stops.
     */
    private fun takeBack(s: VoiceState, now: Long, fromServer: Boolean): Pair<VoiceState, List<VoiceEffect>> = when (s.phase) {
        Phase.Starting, Phase.Listening -> s.copy(heard = "", quietSince = now, activeSince = now) to (if (fromServer) emptyList() else listOf(VoiceEffect.DiscardStream))
        Phase.Thinking -> {
            val fx = ArrayList<VoiceEffect>()
            if (!fromServer) fx += VoiceEffect.DiscardStream
            if (s.cue.isNotEmpty()) { fx += VoiceEffect.StopCue; fx += doneFx(s) }
            fx += VoiceEffect.CancelTurn
            // The turn's number moves on: its answer, if it still comes, is not the next one's.
            s.copy(phase = Phase.Listening, heard = "", pending = "", cue = "", notice = "Cancelled.", turn = s.turn + 1, quietSince = now, activeSince = now) to fx
        }
        else -> s to emptyList()
    }

    /**
     * The X on a faded bubble (the owner, 10 Nov): only that phrase goes,
     * before it reaches Cru; the answer being worked on is not touched.
     * Queued: never asked (words still coming in stay). Being heard: the
     * stream drops it. As crundi-voice.js dropLive.
     */
    private fun dropLive(s: VoiceState, queued: Boolean, now: Long): Pair<VoiceState, List<VoiceEffect>> {
        if (queued) {
            if (CruChat.queued(s).isEmpty()) return s to emptyList()
            val rest = CruChat.hearing(s)
            return s.copy(pending = "", heard = rest.ifEmpty { s.asked }, activeSince = now) to emptyList()
        }
        if (CruChat.hearing(s).isEmpty()) return s to emptyList()
        val heard = if (s.phase == Phase.Thinking) s.pending.trim().ifEmpty { s.asked } else ""
        return s.copy(heard = heard, quietSince = now, activeSince = now) to listOf(VoiceEffect.DiscardStream)
    }

    private fun cancelledNotice(acted: List<String>) =
        if (acted.isEmpty()) "Cancelled." else "Cancelled. Already done: " + acted.joinToString("; ") + "."

    /** [keepReply]: the last answer stays on screen (cut short by what the owner said meanwhile). */
    private fun ask(s: VoiceState, text: String, now: Long, keepReply: Boolean = false): Pair<VoiceState, List<VoiceEffect>> =
        s.copy(phase = Phase.Thinking, turn = s.turn + 1, asked = text, heard = text, reply = if (keepReply) s.reply else "", detail = if (keepReply) s.detail else "", notice = if (s.offline) s.notice else "", quietSince = now, activeSince = now, question = "", needsUnlock = false) to listOf(VoiceEffect.Ask(text), VoiceEffect.Cue)

    /** Called by name while already awake: "Yes, Sir?" at once, no Claude, no cue. */
    private fun calledHere(s: VoiceState, t: String, now: Long): Pair<VoiceState, List<VoiceEffect>> = here().let { h ->
        val n = s.copy(phase = Phase.Speaking, heard = "", asked = t, reply = h, detail = "", notice = "", lastSaid = h, quietSince = now, activeSince = now)
        n to sayFx(n, h)
    }

    /** Crundi stops talking now and listens (the owner talked over it, or tapped). */
    private fun cutIn(s: VoiceState, now: Long): Pair<VoiceState, List<VoiceEffect>> =
        s.copy(phase = Phase.Listening, heard = "", quietSince = now, activeSince = now, saidUntil = now) to (listOf(VoiceEffect.StopSaying) + doneFx(s))

    /** Start saying [text]: a live stream is told what Crundi says, an older one is paused. */
    private fun sayFx(s: VoiceState, text: String): List<VoiceEffect> = holdFx(s, text) + VoiceEffect.Say(text)

    /** Cru is about to make a sound: the stream pauses (the gate listens); a live one is told what. */
    private fun holdFx(s: VoiceState, text: String): List<VoiceEffect> =
        if (s.live) listOf(VoiceEffect.Saying(text), VoiceEffect.PauseStream) else listOf(VoiceEffect.PauseStream)

    /** Crundi has stopped talking: the stream resumes (a live one is told). */
    private fun doneFx(s: VoiceState): List<VoiceEffect> = if (s.live) listOf(VoiceEffect.Said, VoiceEffect.ResumeStream) else listOf(VoiceEffect.ResumeStream)

    /** The answer has been said: listen again, or send what was said meanwhile. */
    private fun afterSpeech(s: VoiceState, now: Long): Pair<VoiceState, List<VoiceEffect>> {
        if (s.closing) return s.copy(phase = Phase.Ended) to listOf(VoiceEffect.Dismiss)
        if (s.reports.isNotEmpty() && s.pending.isBlank()) {
            val (sp, sh) = s.reports.first()
            return tell(s.copy(reports = s.reports.drop(1)), sp, sh, now)
        }
        val resumed = s.copy(phase = Phase.Listening, heard = "", quietSince = now, activeSince = now, saidUntil = now)
        if (s.pending.isBlank()) return resumed to doneFx(s)
        val (next, fx) = ask(resumed.copy(pending = ""), s.pending, now)
        return next to (doneFx(s) + fx)
    }

    /** News said unasked: shown, and spoken with the stream paused. */
    private fun tell(s: VoiceState, speak: String, show: String, now: Long): Pair<VoiceState, List<VoiceEffect>> {
        val next = s.copy(reply = speak, detail = show, heard = "", notice = "", quietSince = now, activeSince = now, question = "", needsUnlock = false)
        return if (speak.isBlank()) next.copy(phase = Phase.Listening) to emptyList()
        else next.copy(phase = Phase.Speaking, lastSaid = speak) to sayFx(next, speak)
    }

    /** Heard while Crundi speaks or within [echoMs] after, and mostly Crundi's own words. */
    private fun echoOf(s: VoiceState, heard: String, now: Long) =
        s.lastSaid.isNotEmpty() && (s.phase == Phase.Speaking || now - s.saidUntil <= echoMs) && Speech.isEcho(heard, s.lastSaid)

    private fun join(a: String, b: String) = listOf(a.trim(), b.trim()).filter { it.isNotEmpty() }.joinToString(" ")

    companion object {
        /** About how long a cue lasts: its echo is looked for from its end. */
        const val CUE_ECHO_MS = 1_500L
    }
}

/** Text helpers for speaking a reply. */
object Speech {
    private fun words(t: String) = t.lowercase().split(Regex("[^\\p{L}\\p{N}']+")).filter { it.isNotEmpty() }

    /** What Cru says when called while it is already listening, one at random. */
    val HERE = listOf("Sir?", "Yes?", "Yes, Sir?", "I'm here, Sir.", "Listening, Sir.", "Here, Sir.", "At your service, Sir.", "Yes, Sir. I'm listening.", "Go ahead, Sir.")

    /** A picker over [HERE] that never says the same line twice in a row. */
    fun hereLine(random: kotlin.random.Random = kotlin.random.Random.Default): () -> String {
        var last = ""
        return { HERE.filter { it != last }.random(random).also { last = it } }
    }
    /** Phrases that take back what was just said (the server's list, src/voice/stream.js). Not "cancel": that answers Cru's questions. */
    private val DISCARD = listOf(listOf("scratch", "that"), listOf("never", "mind"), listOf("nevermind"), listOf("ignore", "that"))

    /** [text] is only, or ends with, "scratch that" / "never mind" / "ignore that" (case and punctuation aside). */
    fun isDiscard(text: String): Boolean {
        val w = words(text)
        return DISCARD.any { p -> w.size >= p.size && w.takeLast(p.size) == p }
    }

    /** Cru's name as speech recognition writes it. */
    private val NAMES = setOf("cru", "crew", "kru", "krew", "crue", "crews")

    /** Just "Hey Cru" (or the name alone, said once or twice): the owner is calling, not asking anything. */
    fun isCalling(text: String): Boolean {
        val w = words(text).dropWhile { it in setOf("hey", "hi", "hello", "ok", "okay", "oi", "yo") }
        return w.isNotEmpty() && w.size <= 2 && w.all { it in NAMES }
    }

    private val FILLER = setOf("um", "uh", "erm", "er", "hmm", "mm", "mhm", "ah", "oh")
    private val STOP_WORDS = setOf("stop", "wait", "cancel", "enough", "quiet", "pause", "hold", "no", "cru", "crew", "sorry", "actually", "hang")

    /**
     * [heard] while Crundi says [said] is the owner talking over it: not an
     * echo, at least [min] words that aren't "um", one of them not Crundi's
     * own; or one word like "stop" or "wait". (crundi-voice.js has the same rule.)
     */
    fun talksOver(heard: String, said: String, min: Int = 2): Boolean {
        val h = words(heard).filter { it !in FILLER }
        if (h.isEmpty() || (said.isNotEmpty() && isEcho(heard, said))) return false
        val s = words(said).toSet()
        val fresh = h.filter { it !in s }
        return (h.size >= min && fresh.isNotEmpty()) || fresh.any { it in STOP_WORDS }
    }

    /** The cues when the server offers none (an older server): said with the phone's own voice only. The server's list (src/voice/cues.js). */
    val CUES = listOf("One moment, Sir.", "Let me check.", "On it, Sir.", "Just a moment.", "Looking now, Sir.", "Right away, Sir.", "Bear with me, Sir.", "Let me see.")

    /** One of [n] at random, never [last] when there is a choice; -1 when there are none. */
    fun pickCue(n: Int, last: Int, random: kotlin.random.Random = kotlin.random.Random.Default): Int {
        if (n <= 0) return -1
        if (n == 1) return 0
        val i = random.nextInt(n - 1)
        return if (last >= 0 && i >= last) i + 1 else i
    }

    /**
     * [heard] is Crundi's own [said] picked up by the microphone: at least
     * two words, and most of them (60%) words of what was said.
     */
    fun isEcho(heard: String, said: String): Boolean {
        val h = words(heard)
        if (h.size < 2) return h.size == 1 && words(said).contains(h[0]) && words(said).size <= 3
        val s = words(said).toSet()
        return h.count { it in s }.toFloat() / h.size >= 0.6f
    }

    /**
     * A reply in pieces to speak one after another, so the first can play
     * while the next is fetched. Split after . ! ? and before a new line;
     * pieces shorter than [min] characters are joined to the next.
     */
    fun sentences(text: String, min: Int = 24): List<String> {
        val raw = text.trim().split(Regex("(?<=[.!?])\\s+|\\n+")).map { it.trim() }.filter { it.isNotEmpty() }
        val out = ArrayList<String>()
        var acc = ""
        for (p in raw) {
            acc = if (acc.isEmpty()) p else "$acc $p"
            if (acc.length >= min) { out += acc; acc = "" }
        }
        if (acc.isNotEmpty()) { if (out.isEmpty()) out += acc else out[out.size - 1] = out.last() + " " + acc }
        return out
    }
}

/** A voice the phone's text-to-speech engine has. */
data class DeviceVoice(val name: String, val locale: String, val needsNetwork: Boolean = false, val installed: Boolean = true)

object DeviceVoices {
    /** D34: the default British male voice and its fallbacks. */
    const val PREFERRED = "en-gb-x-rjs-network"
    const val PREFERRED_LOCAL = "en-gb-x-rjs-local"

    /**
     * The voice to use: the one the owner chose if the phone still has it, else
     * en-gb-x-rjs-network, then en-gb-x-rjs-local, then any British English
     * voice (on-device first), else null (the engine's own default).
     */
    fun pick(voices: List<DeviceVoice>, chosen: String?, online: Boolean = true): DeviceVoice? {
        val usable = voices.filter { it.installed && (online || !it.needsNetwork) }
        if (!chosen.isNullOrEmpty()) usable.firstOrNull { it.name == chosen }?.let { return it }
        usable.firstOrNull { it.name == PREFERRED }?.let { return it }
        usable.firstOrNull { it.name == PREFERRED_LOCAL }?.let { return it }
        val gb = usable.filter { it.locale.replace('_', '-').equals("en-GB", ignoreCase = true) }
        return gb.firstOrNull { !it.needsNetwork } ?: gb.firstOrNull()
    }
}

/**
 * Hears the owner talking over Crundi from the microphone level alone, while the
 * stream to the server is paused.
 *
 * Crundi's own voice comes back through the microphone loudly: the phone's
 * echo cancelling only works on call audio, and Crundi plays as speech on the
 * speaker. So for the first moments of each answer the detector learns how
 * much of Crundi's voice ([hear]'s `voice`, its playback level) reaches the
 * microphone, and from then on counts only what is well above that echo, the
 * room's noise and a floor, for [holdMs]. When the playback level can't be
 * measured (the phone's own voice), only a loud voice counts.
 */
class BargeInDetector(
    private val holdMs: Long = 400,
    /** Never quieter than this (0..1 of full scale) counts, however still the room. */
    private val floorMin: Float = 0.03f,
    private val overNoise: Float = 3.2f,
    /** The owner must be this many times louder than Crundi's own echo. */
    private val overEcho: Float = 2.2f,
    /** The start of each answer: learning the echo, never an interruption. */
    private val calibrateMs: Long = 700,
    /** The bar when Crundi's level is unknown. */
    private val unknownGate: Float = 0.45f,
) {
    private var noise = 0.01f
    private var loudFor = 0L
    private var spokeFor = 0L
    /** Microphone level per unit of Crundi's playback level. Kept between answers. */
    private var echo = 0.6f
    private val recent = FloatArray(4)
    private var ri = 0

    /** While listening: learn how loud the room is. */
    fun learn(level: Float) {
        noise = (noise * 0.95f + level * 0.05f).coerceIn(0.002f, 0.2f)
    }

    /** Crundi starts an answer. */
    fun reset() { loudFor = 0L; spokeFor = 0L; recent.fill(0f) }

    /** Heard once: count afresh (the echo learnt is kept). */
    fun rearm() { loudFor = 0L }

    /**
     * While speaking: true once the owner has been talking over Crundi for
     * [holdMs]. [voice] is Crundi's playback level, or null when unknown;
     * [speech] whether it is someone talking ([SpeechCheck]: a noise as loud
     * is not an interruption).
     */
    fun hear(level: Float, voice: Float?, ms: Long, speech: () -> Boolean = { true }): Boolean {
        spokeFor += ms
        val gate = if (voice == null) maxOf(unknownGate, noise * overNoise) else {
            recent[ri] = voice; ri = (ri + 1) % recent.size
            // The playback and the microphone are not in step: use the loudest of the last few.
            val peak = recent.max()
            if (spokeFor <= calibrateMs && peak > 0.05f) echo = maxOf(echo * 0.85f, level / peak).coerceIn(0.05f, 3f)
            maxOf(floorMin, noise * overNoise, echo * peak * overEcho + 0.04f)
        }
        if (spokeFor <= calibrateMs) { loudFor = 0L; return false }
        loudFor = if (level >= gate) loudFor + ms else 0L
        return loudFor >= holdMs && speech()
    }
}
