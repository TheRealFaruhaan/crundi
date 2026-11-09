package mv.obitech.crundi.voice

import android.content.Context
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.exp
import kotlin.math.log10
import kotlin.math.sin
import kotlin.math.sqrt
import kotlin.math.tanh

/**
 * Is it someone talking, not a noise? The speech detector that decides
 * barge-in (the owner talking over Cru): Silero VAD v5 (MIT,
 * github.com/snakers4/silero-vad), run here in plain Kotlin, as the web
 * client runs it in JavaScript (crundi-voice.js createVad: the same rules,
 * the same weights file, app/vendor/silero-vad-16k.bin, copied into the
 * assets at build time; scripts/make-vad-weights.py makes it).
 *
 * It hears any voice, Cru's own echo too: [OwnerGate]'s level rule keeps
 * that out. What it adds is that a door, a cough, a fan, a hum or music never
 * counts: only [talking], at least [SPEECH_MS] of unbroken speech that rises
 * and falls as speech does.
 *
 * Fed on the recording thread; not thread-safe.
 */
class SpeechDetector(private val w: Weights) {
    /** The model's weights, float16 in the file. */
    class Weights internal constructor(
        internal val enc: List<Conv>,
        internal val wih: FloatArray,
        internal val whh: FloatArray,
        internal val bl: FloatArray,
        internal val wo: FloatArray,
        internal val bo: Float,
    )

    internal class Conv(val o: Int, val c: Int, val stride: Int, val w: FloatArray, val b: FloatArray)

    private val buf = FloatArray(CTX + FRAME)
    private var fill = CTX
    private val h = FloatArray(128)
    private val c = FloatArray(128)
    private val xp = FloatArray(640)
    private val mag = FloatArray(129 * 4)
    private val g = FloatArray(512)
    private val db = FloatArray(64)

    /** The latest frame's speech probability. */
    var prob = 0f; private set
    /** Speech now (with hysteresis: on at [ON], off below [OFF]). */
    var speech = false; private set
    /** How long it has been speech without a break. */
    var speechMs = 0L; private set
    /** Frames (32 ms) run so far. */
    var frames = 0L; private set
    /** Someone talking: [SPEECH_MS] of speech, rising and falling as speech does. */
    var talking = false; private set

    /** Run it [WARM] frames before trusting it: a sound out of silence reads as speech for a moment. */
    val warm: Boolean get() = frames >= WARM

    /** 16-bit little-endian PCM at 16 kHz, as [MicStream] gives it. */
    fun push(pcm: ByteArray) {
        var i = 0
        while (i + 1 < pcm.size) {
            buf[fill++] = ((pcm[i + 1].toInt() shl 8) or (pcm[i].toInt() and 0xff)).toShort() / 32768f
            if (fill == buf.size) frame()
            i += 2
        }
    }

    /** Samples -1..1 at 16 kHz. */
    fun push(pcm: FloatArray, from: Int = 0, to: Int = pcm.size) {
        for (i in from until to) { buf[fill++] = pcm[i]; if (fill == buf.size) frame() }
    }

    fun reset() {
        buf.fill(0f); fill = CTX; h.fill(0f); c.fill(0f); db.fill(0f)
        prob = 0f; speech = false; speechMs = 0L; talking = false
    }

    private fun frame() {
        var en = 0.0
        for (a in CTX until buf.size) en += buf[a] * buf[a]
        db[(frames % 64).toInt()] = (10 * log10(en / FRAME + 1e-10)).toFloat()
        System.arraycopy(buf, 0, xp, 0, 576)
        for (j in 0 until 64) xp[576 + j] = buf[574 - j]   // reflect
        for (t in 0 until 4) for (k in 0 until 129) {
            var re = 0f; var im = 0f; val base = k * 256; val off = t * 128
            for (n in 0 until 256) { val v = xp[off + n]; re += COS[base + n] * v; im += SIN[base + n] * v }
            mag[k * 4 + t] = sqrt(re * re + im * im)
        }
        var x = mag; var len = 4
        for (l in w.enc) { val r = conv(l, x, len); x = r.first; len = r.second }
        for (r in 0 until 512) {
            var s = w.bl[r]; val wi = r * 128
            for (q in 0 until 128) s += w.wih[wi + q] * x[q] + w.whh[wi + q] * h[q]
            g[r] = s
        }
        var out = w.bo
        for (u in 0 until 128) {
            c[u] = sigm(g[128 + u]) * c[u] + sigm(g[u]) * tanh(g[256 + u])
            h[u] = sigm(g[384 + u]) * tanh(c[u])
            if (h[u] > 0) out += h[u] * w.wo[u]
        }
        val p = sigm(out)
        prob = p; frames++
        speech = if (speech) p >= OFF else p >= ON
        speechMs = if (speech) speechMs + 32 else 0L
        val run = minOf(64L, speechMs / 32).toInt()
        var lo = Float.POSITIVE_INFINITY; var hi = Float.NEGATIVE_INFINITY
        for (y in 0 until run - SKIP) { val d = db[((frames - 1 - y + 64) % 64).toInt()]; if (d < lo) lo = d; if (d > hi) hi = d }
        talking = speechMs >= SPEECH_MS && run - SKIP >= 5 && hi - lo >= SWING_DB
        System.arraycopy(buf, FRAME, buf, 0, CTX)
        fill = CTX
    }

    companion object {
        const val ON = 0.5f
        const val OFF = 0.35f
        /** Barge-in needs this much unbroken speech. */
        const val SPEECH_MS = 250L
        const val WARM = 30L
        /** Within the run (its first [SKIP] frames aside: a sound's onset), loudest and quietest frames this far apart. */
        const val SWING_DB = 10f
        const val SKIP = 3
        const val FRAME = 512
        const val CTX = 64
        const val WEIGHTS = 243_073
        const val ASSET = "vad/silero-vad-16k.bin"

        private val COS = FloatArray(129 * 256)
        private val SIN = FloatArray(129 * 256)
        init {
            for (k in 0 until 129) for (n in 0 until 256) {
                val win = 0.5 - 0.5 * cos(2 * PI * n / 256); val a = 2 * PI * k * n / 256
                COS[k * 256 + n] = (cos(a) * win).toFloat(); SIN[k * 256 + n] = (sin(a) * win).toFloat()
            }
        }

        private fun sigm(x: Float) = 1f / (1f + exp(-x))

        /** Kernel 3, padding 1, then ReLU; [x] channel-major, [len] long. */
        private fun conv(l: Conv, x: FloatArray, len: Int): Pair<FloatArray, Int> {
            val to = (len - 1) / l.stride + 1
            val out = FloatArray(l.o * to)
            for (o in 0 until l.o) for (t in 0 until to) {
                var s = l.b[o]; val t0 = t * l.stride - 1
                for (ci in 0 until l.c) {
                    val wi = (o * l.c + ci) * 3; val xi = ci * len
                    for (k in 0 until 3) { val tt = t0 + k; if (tt in 0 until len) s += l.w[wi + k] * x[xi + tt] }
                }
                out[o * to + t] = if (s > 0) s else 0f
            }
            return out to to
        }

        private fun half(v: Int): Float {
            val s = if (v and 0x8000 != 0) -1f else 1f; val e = (v shr 10) and 31; val f = v and 1023
            return when (e) {
                0 -> s * f * 5.9604645e-8f
                31 -> if (f != 0) Float.NaN else s * Float.POSITIVE_INFINITY
                else -> s * Math.scalb(1f + f / 1024f, e - 15)
            }
        }

        /** The weights file: "SVAD", version 1, float16. Null when it is not that. */
        fun parse(bytes: ByteArray): Weights? {
            if (bytes.size != 8 + 2 * WEIGHTS) return null
            if (bytes[0] != 'S'.code.toByte() || bytes[1] != 'V'.code.toByte() || bytes[2] != 'A'.code.toByte() || bytes[3] != 'D'.code.toByte()) return null
            val version = (bytes[4].toInt() and 0xff) or ((bytes[5].toInt() and 0xff) shl 8) or ((bytes[6].toInt() and 0xff) shl 16) or ((bytes[7].toInt() and 0xff) shl 24)
            if (version != 1) return null
            var at = 8
            fun take(n: Int) = FloatArray(n) { val v = (bytes[at].toInt() and 0xff) or ((bytes[at + 1].toInt() and 0xff) shl 8); at += 2; half(v) }
            val shapes = listOf(128 to 129, 64 to 128, 64 to 64, 128 to 64)
            val enc = shapes.mapIndexed { i, (o, ci) -> Conv(o, ci, if (i == 1 || i == 2) 2 else 1, take(o * ci * 3), take(o)) }
            return Weights(enc, take(512 * 128), take(512 * 128), take(512), take(128), take(1)[0])
        }

        @Volatile private var cached: Weights? = null

        /** A detector from the app's assets; null when they are not there (the voicing check stands in). */
        fun load(ctx: Context): SpeechDetector? {
            val ws = cached ?: runCatching { ctx.assets.open(ASSET).use { parse(it.readBytes()) } }.getOrNull()?.also { cached = it }
            return ws?.let { SpeechDetector(it) }
        }
    }
}

/**
 * Barge-in's speech check, fed every microphone chunk: the [SpeechDetector]
 * once it has run a moment; before that, or with no weights, the old
 * voicing check over the latest [VoiceRing] (a pitch, as speech has).
 */
class SpeechCheck(private val detector: SpeechDetector?, private val ring: VoiceRing = VoiceRing()) {
    fun push(pcm: ByteArray) { detector?.push(pcm); ring.add(pcm) }

    /** Someone talking now. */
    fun talking(): Boolean = detector?.takeIf { it.warm }?.talking ?: (CruWake.voicedShare(ring.snapshot()) >= 0.3)

    /** For a rule on the level alone: true only when the detector, warmed up, says it is not someone talking. */
    fun notTalking(): Boolean = detector?.let { it.warm && !it.talking } ?: false
}
