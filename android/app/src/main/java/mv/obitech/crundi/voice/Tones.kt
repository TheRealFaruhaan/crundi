package mv.obitech.crundi.voice

import kotlin.math.PI
import kotlin.math.pow
import kotlin.math.roundToInt
import kotlin.math.sin

/**
 * Cru's calm tones, in place of the spoken "One moment, Sir" cues: soft sine
 * notes with an exponential envelope, as the web plays them (crundi-voice.js,
 * TONES and tone()). Made once as PCM and played by [Speaker.tone], at Cru's
 * volume and on its voice's audio session.
 *
 *   listen  Cru wakes and starts listening
 *   sent    something said (or typed) went to Cru
 *   think   1.4 s after sending, then every 4 s while Cru still thinks
 *
 * No Android in it (TonesTest).
 */
object Tones {
    /** One note: [freq] Hz from [start] s for [dur] s, rising to [peak] gain over [rise] s, then falling away. */
    data class Note(val freq: Double, val start: Double, val dur: Double, val peak: Double, val rise: Double = 0.012)

    val LISTEN = listOf(Note(659.25, 0.0, 0.11, 0.16), Note(880.0, 0.09, 0.16, 0.16))
    val SENT = listOf(Note(880.0, 0.0, 0.08, 0.13), Note(659.25, 0.07, 0.14, 0.13))
    val THINK = listOf(Note(523.25, 0.0, 0.9, 0.07, 0.18), Note(783.99, 0.0, 0.9, 0.05, 0.18))

    /** The first think tone after sending, and the time between them. */
    const val THINK_FIRST_MS = 1_400L
    const val THINK_EVERY_MS = 4_000L

    const val RATE = 24_000
    /** Where the envelope starts and ends (Web Audio's exponential ramps cannot reach 0). */
    const val FLOOR = 0.0001
    /** Each oscillator runs this long past its note, as the web's stop(at + dur + 0.05). */
    const val TAIL = 0.05

    /**
     * A note's gain [t] seconds after it starts: exponentially from [FLOOR]
     * up to its peak over its rise, then exponentially down to [FLOOR] at its end.
     */
    fun gain(n: Note, t: Double): Double = when {
        t < 0 -> 0.0
        t <= n.rise -> FLOOR * (n.peak / FLOOR).pow(t / n.rise)
        t <= n.dur -> n.peak * (FLOOR / n.peak).pow((t - n.rise) / (n.dur - n.rise))
        t <= n.dur + TAIL -> FLOOR
        else -> 0.0
    }

    /** How many samples [notes] take at [rate]. */
    fun length(notes: List<Note>, rate: Int = RATE): Int = ((notes.maxOfOrNull { it.start + it.dur + TAIL } ?: 0.0) * rate).roundToInt()

    /** [notes] as 16-bit mono PCM at [rate]: the sum of the sines, each under its envelope. */
    fun pcm(notes: List<Note>, rate: Int = RATE): ShortArray {
        val out = ShortArray(length(notes, rate))
        for (i in out.indices) {
            val t = i.toDouble() / rate
            var v = 0.0
            for (n in notes) {
                val local = t - n.start
                if (local < 0 || local > n.dur + TAIL) continue
                v += sin(2 * PI * n.freq * local) * gain(n, local)
            }
            out[i] = (v * Short.MAX_VALUE).roundToInt().coerceIn(Short.MIN_VALUE.toInt(), Short.MAX_VALUE.toInt()).toShort()
        }
        return out
    }

    private val cache = HashMap<List<Note>, ShortArray>()

    /** [pcm], made once. */
    @Synchronized fun of(notes: List<Note>): ShortArray = cache.getOrPut(notes) { pcm(notes) }
}
