package mv.obitech.crundi.voice

import android.content.Context
import kotlinx.serialization.Serializable
import mv.obitech.crundi.api.CrundiJson
import java.io.File
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.floor
import kotlin.math.ln
import kotlin.math.log10
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.roundToInt
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * "Hey Cru", set up from the owner's own voice: the same rules and numbers
 * as the PC's app/voice-wake/enroll.js (WakeProfileTest checks them against
 * a fixture made there). With no Android in it, apart from [WakeProfiles].
 *
 * The keyword model can't be retrained on the phone. Training means:
 *  1. Tuning: the spotter is run over five recordings of the owner at
 *     stricter and stricter thresholds; the profile keeps two steps looser
 *     than the strictest that hears all five.
 *  2. A voice match: each recording is kept as MFCC templates; when the
 *     spotter fires, the last two seconds are compared with them (dynamic
 *     time warping) and a wake unlike all of them is ignored.
 * Not a speaker check: someone else saying it the same way may pass. It
 * keeps out "crew" on the television and the near misses.
 */
@Serializable
data class WakeProfile(
    val version: Int = 1,
    val keyword: String = CruWake.KEYWORD,
    val keywordsScore: Double = CruWake.KEYWORDS_SCORE,
    val keywordsThreshold: Double = 0.2,
    val limit: Double = 0.0,
    val templates: List<List<List<Double>>> = emptyList(),
    val trainedAt: String = "",
) {
    fun valid() = version == 1 && templates.size >= 3 && limit > 0 && keywordsThreshold > 0 && keyword.isNotBlank()

    /** The keyword line with this profile's boost and threshold (sherpa-onnx's "tokens :score #threshold @name"). */
    fun keywordLine() = CruWake.line(keywordsThreshold, keywordsScore)

    /** The spotter fired: is it the owner's "Hey Cru"? [recent] is the last [CruWake.VERIFY_SECONDS] of audio. */
    fun verify(recent: FloatArray): CruWake.Verdict {
        val clip = CruWake.trim(recent, 60) ?: recent
        val f = CruWake.mfcc(clip)
        val t = templates.minOf { CruWake.distance(it.map { r -> r.toDoubleArray() }, f) }
        return CruWake.Verdict(t <= limit, (t * 1000).roundToInt() / 1000.0)
    }
}

object CruWake {
    const val RATE = 16000
    const val SAMPLES_NEEDED = 5
    /** The wake phrase as the model's BPE tokens ("HEY CRU" -> ▁HE Y ▁C RU). */
    const val TOKENS = "▁HE Y ▁C RU"
    const val KEYWORD = "$TOKENS @HEY_CRU"
    const val KEYWORDS_SCORE = 1.5
    /** Trigger thresholds tried, strictest first. */
    val THRESHOLDS = listOf(0.35, 0.3, 0.25, 0.2, 0.15, 0.1)
    private const val LIMIT_MARGIN = 1.6
    const val VERIFY_SECONDS = 2.0

    fun line(threshold: Double, score: Double = KEYWORDS_SCORE) = "$TOKENS :$score #$threshold @HEY_CRU"

    data class Verdict(val ok: Boolean, val score: Double)

    // ─── features ───

    private const val FRAME = 400
    private const val HOP = 160
    private const val NFFT = 512
    private const val MELS = 26
    private const val CEPS = 12

    private val hamming = DoubleArray(FRAME) { 0.54 - 0.46 * cos(2 * PI * it / (FRAME - 1)) }
    private val melBank: List<DoubleArray> by lazy {
        fun mel(f: Double) = 2595 * log10(1 + f / 700)
        fun hz(m: Double) = 700 * (10.0.pow(m / 2595) - 1)
        val lo = mel(20.0); val hi = mel(7600.0)
        val bins = (0 until MELS + 2).map { floor((NFFT + 1) * hz(lo + (hi - lo) * it / (MELS + 1)) / RATE).toInt() }
        (1..MELS).map { m ->
            val row = DoubleArray(NFFT / 2 + 1)
            for (k in bins[m - 1] until bins[m]) row[k] = (k - bins[m - 1]).toDouble() / max(1, bins[m] - bins[m - 1])
            for (k in bins[m] until bins[m + 1]) row[k] = (bins[m + 1] - k).toDouble() / max(1, bins[m + 1] - bins[m])
            row
        }
    }
    private val dct: List<DoubleArray> by lazy {
        (1..CEPS).map { c -> DoubleArray(MELS) { m -> sqrt(2.0 / MELS) * cos(PI * c * (m + 0.5) / MELS) } }
    }

    private fun fft(re: DoubleArray, im: DoubleArray) {
        val n = re.size
        var j = 0
        for (i in 1 until n) {
            var bit = n shr 1
            while (j and bit != 0) { j = j xor bit; bit = bit shr 1 }
            j = j xor bit
            if (i < j) { var t = re[i]; re[i] = re[j]; re[j] = t; t = im[i]; im[i] = im[j]; im[j] = t }
        }
        var len = 2
        while (len <= n) {
            val a = -2 * PI / len; val wr = cos(a); val wi = sin(a)
            var i = 0
            while (i < n) {
                var cr = 1.0; var ci = 0.0
                for (k in 0 until len / 2) {
                    val ur = re[i + k]; val ui = im[i + k]
                    val vr = re[i + k + len / 2] * cr - im[i + k + len / 2] * ci
                    val vi = re[i + k + len / 2] * ci + im[i + k + len / 2] * cr
                    re[i + k] = ur + vr; im[i + k] = ui + vi
                    re[i + k + len / 2] = ur - vr; im[i + k + len / 2] = ui - vi
                    val nr = cr * wr - ci * wi; ci = cr * wi + ci * wr; cr = nr
                }
                i += len
            }
            len = len shl 1
        }
    }

    /** 12 MFCCs per 10 ms frame, quiet under -30 dB of the loudest band floored, the clip's mean taken away. */
    fun mfcc(samples: FloatArray): List<DoubleArray> {
        val mels = ArrayList<DoubleArray>()
        val re = DoubleArray(NFFT); val im = DoubleArray(NFFT)
        var loudest = 0.0
        var start = 0
        while (start + FRAME <= samples.size) {
            re.fill(0.0); im.fill(0.0)
            var prev = if (start > 0) samples[start - 1].toDouble() else 0.0
            for (i in 0 until FRAME) { val x = samples[start + i].toDouble(); re[i] = (x - 0.97 * prev) * hamming[i]; prev = x }
            fft(re, im)
            val e = DoubleArray(MELS)
            for (m in 0 until MELS) {
                var s = 0.0; val row = melBank[m]
                for (k in 0..NFFT / 2) if (row[k] != 0.0) s += row[k] * (re[k] * re[k] + im[k] * im[k]) / NFFT
                e[m] = s; if (s > loudest) loudest = s
            }
            mels += e
            start += HOP
        }
        val fl = max(loudest * 1e-3, 1e-10)
        val out = mels.map { e -> DoubleArray(CEPS) { j -> var s = 0.0; val row = dct[j]; for (m in 0 until MELS) s += row[m] * ln(max(e[m], fl)); s } }
        if (out.isNotEmpty()) {
            val mean = DoubleArray(CEPS)
            for (f in out) for (j in 0 until CEPS) mean[j] += f[j] / out.size
            for (f in out) for (j in 0 until CEPS) f[j] -= mean[j]
        }
        return out
    }

    /** The spoken part: a little before the first loud 10 ms to a little after the last; null when nothing stands out. */
    fun trim(samples: FloatArray, padMs: Int = 100): FloatArray? {
        val step = RATE / 100; val n = samples.size / step
        if (n < 5) return null
        val rms = DoubleArray(n) { i -> var s = 0.0; for (k in 0 until step) { val x = samples[i * step + k].toDouble(); s += x * x }; sqrt(s / step) }
        val sorted = rms.sorted()
        val floorRms = sorted[(n * 0.1).toInt()]; val peak = sorted[n - 1]
        if (peak < 0.01 || peak < floorRms * 4) return null
        val thr = max(peak * 0.12, floorRms * 2.5)
        var a = 0; var b = n - 1
        while (a < n && rms[a] < thr) a++
        while (b > a && rms[b] < thr) b--
        val pad = (padMs / 10.0).roundToInt()
        val from = max(0, a - pad) * step; val to = min(n, b + 1 + pad) * step
        return samples.copyOfRange(from, to)
    }

    /** DTW of [template] against the best stretch of [clip], as the mean distance per step. */
    fun distance(template: List<DoubleArray>, clip: List<DoubleArray>): Double {
        val m = template.size; val n = clip.size
        if (m == 0 || n == 0) return Double.POSITIVE_INFINITY
        var pc = DoubleArray(n + 1); var pl = DoubleArray(n + 1)
        var cc = DoubleArray(n + 1); var cl = DoubleArray(n + 1)
        for (i in 1..m) {
            cc[0] = Double.POSITIVE_INFINITY; cl[0] = 0.0
            val t = template[i - 1]
            for (j in 1..n) {
                val c = clip[j - 1]
                var s = 0.0; for (k in t.indices) { val d = t[k] - c[k]; s += d * d }
                val x = sqrt(s)
                var best = pc[j - 1]; var len = pl[j - 1]
                if (cc[j - 1] < best) { best = cc[j - 1]; len = cl[j - 1] }
                if (pc[j] < best) { best = pc[j]; len = pl[j] }
                cc[j] = best + x; cl[j] = len + 1
            }
            val a = pc; pc = cc; cc = a
            val b = pl; pl = cl; cl = b
        }
        var best = Double.POSITIVE_INFINITY
        for (j in 1..n) if (pl[j] > 0) best = min(best, pc[j] / max(pl[j], m.toDouble()))
        return best
    }

    // ─── training ───

    sealed interface Sample {
        data class Ok(val clip: FloatArray) : Sample
        data class Bad(val why: String) : Sample
    }

    /** One recording, checked as it is made. */
    fun checkSample(samples: FloatArray): Sample {
        val clip = trim(samples) ?: return Sample.Bad("Nothing was heard. Speak a little closer.")
        val ms = clip.size * 1000.0 / RATE
        if (ms < 350) return Sample.Bad("That was too short. Say the whole of \"Hey Cru\".")
        if (ms > 2200) return Sample.Bad("That was too long. Say just \"Hey Cru\".")
        return Sample.Ok(clip)
    }

    sealed interface Trained {
        data class Ok(val profile: WakeProfile) : Trained
        data class Redo(val why: String, val which: List<Int>) : Trained
    }

    /**
     * The profile from five recordings. [hears] says whether the spotter,
     * with the keyword line given, hears "Hey Cru" in a clip (the engine is
     * Android's; tests pass their own).
     */
    fun train(recordings: List<FloatArray>, now: String, hears: (line: String, clip: FloatArray) -> Boolean): Trained {
        val checked = recordings.map { checkSample(it) }
        val bad = checked.indices.filter { checked[it] is Sample.Bad }
        if (bad.isNotEmpty()) return Trained.Redo("Some recordings had nothing clear in them.", bad)
        val clips = checked.map { (it as Sample.Ok).clip }

        var strictest = -1; var missed = emptyList<Int>()
        for ((t, th) in THRESHOLDS.withIndex()) {
            val miss = clips.indices.filter { !hears(line(th), padded(clips[it])) }
            if (miss.isEmpty()) { strictest = t; break }
            if (t == THRESHOLDS.lastIndex) missed = miss
        }
        if (strictest < 0) return Trained.Redo(
            if (missed.size == clips.size) "It could not hear \"Hey Cru\" in any of them. Say it clearly, with a small pause between \"Hey\" and \"Cru\"."
            else "It could not hear \"Hey Cru\" in some of them. Say those again.",
            missed,
        )
        val threshold = THRESHOLDS[min(THRESHOLDS.lastIndex, strictest + 2)]

        val templates = clips.map { mfcc(it) }
        val nearest = templates.indices.map { i -> templates.indices.filter { it != i }.minOf { distance(templates[it], templates[i]) } }
        val median = nearest.sorted()[nearest.size / 2]
        val odd = nearest.indices.filter { nearest[it] > median * 2 }
        if (odd.isNotEmpty() && odd.size < templates.size / 2.0) return Trained.Redo("One recording sounds unlike the others. Say it again the way you normally would.", odd)
        val limit = nearest.max() * LIMIT_MARGIN
        return Trained.Ok(WakeProfile(
            keywordsThreshold = threshold,
            limit = (limit * 1000).roundToInt() / 1000.0,
            templates = templates.map { t -> t.map { f -> f.map { (it * 1000).roundToInt() / 1000.0 } } },
            trainedAt = now,
        ))
    }

    /** A clip as the spotter should hear it: 0.3 s of quiet before, a second after. */
    fun padded(clip: FloatArray): FloatArray {
        val out = FloatArray((RATE * 0.3).toInt() + clip.size + RATE)
        clip.copyInto(out, (RATE * 0.3).toInt())
        return out
    }
}

/** The owner's profile on this phone: a file only the app can read. */
object WakeProfiles {
    private fun file(ctx: Context) = File(ctx.filesDir, "hey-cru-profile.json")

    @Volatile private var cached: WakeProfile? = null
    @Volatile private var loaded = false

    fun get(ctx: Context): WakeProfile? {
        if (!loaded) {
            cached = runCatching { CrundiJson.decodeFromString(WakeProfile.serializer(), file(ctx).readText()) }.getOrNull()?.takeIf { it.valid() }
            loaded = true
        }
        return cached
    }

    fun trained(ctx: Context) = get(ctx) != null

    fun save(ctx: Context, p: WakeProfile) {
        val f = file(ctx)
        val tmp = File(f.parentFile, f.name + ".part")
        tmp.writeText(CrundiJson.encodeToString(WakeProfile.serializer(), p))
        tmp.renameTo(f)
        cached = p; loaded = true
    }

    fun forget(ctx: Context) {
        runCatching { file(ctx).delete() }
        cached = null; loaded = true
    }
}
