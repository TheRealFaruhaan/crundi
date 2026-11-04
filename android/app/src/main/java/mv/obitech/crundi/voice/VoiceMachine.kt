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
 * - While Crundi speaks, the microphone stream is paused, so Deepgram never
 *   transcribes Crundi's own voice. The phone still listens locally: Master
 *   talking over it (BargeIn) stops the speech and resumes the stream.
 * - What Master says while Crundi is thinking is kept and sent next.
 * - Quiet for [silenceMs] while listening ends the session: back to sleep.
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
    /** When things last happened, for the silence timeout. */
    val quietSince: Long = 0,
)

sealed interface VoiceEvent {
    data class Opened(val locked: Boolean, val now: Long) : VoiceEvent
    data class Ready(val now: Long) : VoiceEvent
    data class Interim(val text: String, val now: Long) : VoiceEvent
    data class Utterance(val text: String, val now: Long) : VoiceEvent
    /** Claude's answer, or why there is none. */
    data class Reply(val ok: Boolean, val speak: String, val show: String, val error: String, val now: Long) : VoiceEvent
    data class SpeechDone(val now: Long) : VoiceEvent
    /** Master talked over Crundi. */
    data class BargeIn(val now: Long) : VoiceEvent
    data class StreamLost(val why: String, val now: Long) : VoiceEvent
    data class Unlocked(val now: Long) : VoiceEvent
    data class Tick(val now: Long) : VoiceEvent
    data object Close : VoiceEvent
}

sealed interface VoiceEffect {
    data class Ask(val text: String) : VoiceEffect
    data class Say(val text: String) : VoiceEffect
    data object StopSaying : VoiceEffect
    data object PauseStream : VoiceEffect
    data object ResumeStream : VoiceEffect
    /** The stream is gone: listen with the phone's own recogniser from here on. */
    data object UseFallback : VoiceEffect
    data object Finish : VoiceEffect
}

class VoiceMachine(
    /** Quiet this long while listening, and the session ends. */
    val silenceMs: Long = 30_000,
    /** Starting this long without the stream: try the phone's recogniser. */
    val startMs: Long = 8_000,
) {
    fun on(s: VoiceState, e: VoiceEvent): Pair<VoiceState, List<VoiceEffect>> {
        if (s.phase == Phase.Ended) return s to emptyList()
        return when (e) {
            is VoiceEvent.Opened -> s.copy(phase = Phase.Starting, locked = e.locked, quietSince = e.now) to emptyList()
            is VoiceEvent.Ready -> if (s.phase == Phase.Starting) s.copy(phase = Phase.Listening, quietSince = e.now) to emptyList() else s to emptyList()
            is VoiceEvent.Interim -> when (s.phase) {
                Phase.Starting, Phase.Listening -> s.copy(phase = Phase.Listening, heard = e.text, quietSince = e.now) to emptyList()
                Phase.Thinking -> s.copy(heard = join(s.pending, e.text), quietSince = e.now) to emptyList()
                else -> s to emptyList()
            }
            is VoiceEvent.Utterance -> {
                val t = e.text.trim()
                when {
                    t.isEmpty() -> s to emptyList()
                    s.phase == Phase.Thinking -> s.copy(pending = join(s.pending, t), heard = join(s.pending, t), quietSince = e.now) to emptyList()
                    s.phase == Phase.Speaking -> s to emptyList()
                    else -> ask(s, t, e.now)
                }
            }
            is VoiceEvent.Reply -> {
                if (s.phase != Phase.Thinking) return s to emptyList()
                val say = if (e.ok) e.speak.trim() else e.error.trim()
                val next = s.copy(reply = if (e.ok) e.speak.trim() else "", detail = if (e.ok) e.show.trim() else "", notice = if (e.ok) "" else e.error.trim(), quietSince = e.now)
                if (say.isEmpty()) afterSpeech(next, e.now)
                else next.copy(phase = Phase.Speaking) to listOf(VoiceEffect.PauseStream, VoiceEffect.Say(say))
            }
            is VoiceEvent.SpeechDone -> if (s.phase == Phase.Speaking) afterSpeech(s, e.now) else s to emptyList()
            is VoiceEvent.BargeIn -> if (s.phase == Phase.Speaking) {
                s.copy(phase = Phase.Listening, heard = "", quietSince = e.now) to listOf(VoiceEffect.StopSaying, VoiceEffect.ResumeStream)
            } else s to emptyList()
            is VoiceEvent.StreamLost -> if (s.offline) s to emptyList() else {
                s.copy(offline = true, notice = "Live speech recognition is unavailable (${e.why}). Using the phone's recogniser.", phase = if (s.phase == Phase.Starting) Phase.Listening else s.phase, quietSince = e.now) to listOf(VoiceEffect.UseFallback)
            }
            is VoiceEvent.Unlocked -> s.copy(locked = false, notice = "", quietSince = e.now) to emptyList()
            is VoiceEvent.Tick -> when {
                s.phase == Phase.Starting && !s.offline && e.now - s.quietSince >= startMs ->
                    on(s, VoiceEvent.StreamLost("no answer from the server", e.now))
                s.phase == Phase.Listening && s.pending.isEmpty() && e.now - s.quietSince >= silenceMs ->
                    s.copy(phase = Phase.Ended) to listOf(VoiceEffect.Finish)
                else -> s to emptyList()
            }
            VoiceEvent.Close -> s.copy(phase = Phase.Ended) to listOf(VoiceEffect.StopSaying, VoiceEffect.Finish)
        }
    }

    private fun ask(s: VoiceState, text: String, now: Long): Pair<VoiceState, List<VoiceEffect>> =
        s.copy(phase = Phase.Thinking, asked = text, heard = text, reply = "", detail = "", notice = if (s.offline) s.notice else "", quietSince = now) to listOf<VoiceEffect>(VoiceEffect.Ask(text))

    /** The answer has been said: listen again, or send what was said meanwhile. */
    private fun afterSpeech(s: VoiceState, now: Long): Pair<VoiceState, List<VoiceEffect>> {
        val resumed = s.copy(phase = Phase.Listening, heard = "", quietSince = now)
        if (s.pending.isBlank()) return resumed to listOf(VoiceEffect.ResumeStream)
        val (next, fx) = ask(resumed.copy(pending = ""), s.pending, now)
        return next to (listOf<VoiceEffect>(VoiceEffect.ResumeStream) + fx)
    }

    private fun join(a: String, b: String) = listOf(a.trim(), b.trim()).filter { it.isNotEmpty() }.joinToString(" ")
}

/** Text helpers for speaking a reply. */
object Speech {
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
     * The voice to use: the one Master chose if the phone still has it, else
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
 * Hears Master talking over Crundi from the microphone level alone, while the
 * stream to the server is paused. The level is measured with the phone's echo
 * cancelling on, so Crundi's own voice mostly doesn't count; on top of that it
 * must be well above the room's noise (learnt while listening) for [holdMs].
 */
class BargeInDetector(
    private val holdMs: Long = 280,
    /** Never quieter than this (0..1 of full scale) counts, however still the room. */
    private val floorMin: Float = 0.02f,
    private val overNoise: Float = 3.2f,
) {
    private var noise = 0.01f
    private var loudFor = 0L

    /** While listening: learn how loud the room is. */
    fun learn(level: Float) {
        noise = (noise * 0.95f + level * 0.05f).coerceIn(0.002f, 0.2f)
    }

    /** While speaking: true once Master has been talking for [holdMs]. */
    fun hear(level: Float, ms: Long): Boolean {
        val gate = maxOf(floorMin, noise * overNoise)
        loudFor = if (level >= gate) loudFor + ms else 0L
        return loudFor >= holdMs
    }

    fun reset() { loudFor = 0L }
}
