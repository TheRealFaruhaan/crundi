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
 * The keyword model can't be retrained on the phone, and may not hear "Hey
 * Cru" the way the owner says it at all. So the five recordings are what
 * it learns from:
 *  1. The owner's own phrase: each recording kept as MFCC templates; every
 *     burst of speech on its own ([Segmenter]) is compared with them
 *     (dynamic time warping) and wakes when close, at a strict limit,
 *     whatever the model made of it.
 *  2. The model, tuned: the spotter (Cru, Kru, Crew spellings) at two steps
 *     looser than the strictest threshold that hears as many takes as any;
 *     its hits are checked against the templates at a looser limit.
 * No take is refused because the model didn't hear it.
 * Not a speaker check: someone else saying it the same way may pass. It
 * keeps out "crew" on the television and the near misses.
 */
@Serializable
data class WakeProfile(
    val version: Int = 4,
    val keyword: String = CruWake.KEYWORD,
    val keywordsScore: Double = CruWake.KEYWORDS_SCORE,
    val keywordsThreshold: Double = 0.2,
    val limit: Double = 0.0,
    /** For the owner's phrase alone (nothing else vouches for it); 0 in a version 1 profile. */
    val strictLimit: Double = 0.0,
    /** How many takes the model itself heard. */
    val modelHeard: Int = -1,
    /** The owner's takes' usual length, 10 ms frames. */
    val phraseFrames: Int = 0,
    /** The first [takes] templates are the takes; the rest, the same takes heard other ways. */
    val takes: Int = 0,
    val templates: List<List<List<Double>>> = emptyList(),
    val trainedAt: String = "",
) {
    // Version 4: deltas in the features; anything older must be trained again.
    fun valid() = version == 4 && phraseFrames > 0 && templates.size >= 3 && limit > 0 && keywordsThreshold > 0 && keyword.isNotBlank()

    /** The keyword lines with this profile's boost and threshold (sherpa-onnx's "tokens :score #threshold @name"), every spelling. */
    fun keywordLine() = CruWake.lines(keywordsThreshold, keywordsScore)

    private val strict get() = if (strictLimit > 0) strictLimit else limit / CruWake.LIMIT_MARGIN * CruWake.STRICT_MARGIN
    private val feats by lazy { templates.map { t -> t.map { it.toDoubleArray() } } }

    /**
     * A burst of speech on its own ([CruWake.Segmenter]): the owner's "Hey
     * Cru"? As long as their takes, voiced (speech, not a noise), close to
     * them compared whole and both ways, and (when the model can hear this
     * voice at all) [confirm]ed by the model, if only faintly. As enroll.js's matchPhrase.
     */
    fun matchPhrase(segment: FloatArray, confirm: ((FloatArray) -> Boolean)? = null): CruWake.Verdict {
        val no = CruWake.Verdict(false, Double.POSITIVE_INFINITY)
        val c = CruWake.checkSample(segment) as? CruWake.Sample.Ok ?: return no
        val frames = phraseFrames
        val len = (c.clip.size - 400) / 160.0 + 1
        if (len < frames * 0.65 || len > frames * 1.5) return no
        if (CruWake.voicedShare(c.clip) < 0.3) return no
        val f = CruWake.mfcc(c.clip)
        val t = feats.minOf { maxOf(CruWake.fullDistance(it, f), CruWake.fullDistance(f, it)) }
        val score = (t * 1000).roundToInt() / 1000.0
        if (t > strict) return CruWake.Verdict(false, score)
        if (confirm != null && modelHeard >= 3 && !confirm(c.clip)) return CruWake.Verdict(false, score)
        return CruWake.Verdict(true, score)
    }

    /**
     * A recording judged exactly as listening judges it (Try it): each burst
     * in it through [matchPhrase], with the model's second opinion from
     * [hears]; else the model's hit on the whole, checked by [verify]. Null
     * when neither heard anything like "Hey Cru".
     */
    fun judge(audio: FloatArray, hears: (line: String, clip: FloatArray) -> Boolean): CruWake.Verdict? {
        val seg = CruWake.Segmenter()
        val all = FloatArray(audio.size + CruWake.RATE).also { audio.copyInto(it) } // a second of quiet after, to end the last burst
        var closest: CruWake.Verdict? = null
        var i = 0
        while (i < all.size) {
            seg.accept(all.copyOfRange(i, minOf(all.size, i + 1600)))?.let { burst ->
                val v = matchPhrase(burst) { clip -> hears(CruWake.lines(CruWake.CONFIRM_THRESHOLD, keywordsScore), CruWake.padded(clip)) }
                if (v.ok) return v
                if (v.score.isFinite() && (closest == null || v.score < closest!!.score)) closest = v
            }
            i += 1600
        }
        if (hears(keywordLine(), CruWake.padded(audio))) return verify(audio)
        return closest
    }

    /** The spotter fired: is it the owner's "Hey Cru"? [recent] is the last [CruWake.VERIFY_SECONDS] of audio. */
    fun verify(recent: FloatArray): CruWake.Verdict {
        val clip = CruWake.trim(recent, 60) ?: recent
        val f = CruWake.mfcc(clip)
        val t = feats.minOf { CruWake.distance(it, f) }
        return CruWake.Verdict(t <= limit, (t * 1000).roundToInt() / 1000.0)
    }
}

object CruWake {
    const val RATE = 16000
    const val SAMPLES_NEEDED = 5
    /** The wake phrase as the model's BPE tokens, spelt the ways it may hear it ("HEY CRU", "HEY KRU", "HEY CREW"). */
    val SPELLINGS = listOf("▁HE Y ▁C RU", "▁HE Y ▁K RU", "▁HE Y ▁C RE W")
    const val KEYWORD = "▁HE Y ▁C RU @HEY_CRU"
    const val KEYWORDS_SCORE = 1.5
    /** Trigger thresholds tried, strictest first. */
    val THRESHOLDS = listOf(0.35, 0.3, 0.25, 0.2, 0.15, 0.1, 0.06)
    const val LIMIT_MARGIN = 1.6
    const val STRICT_MARGIN = 1.3
    /** The loosest keyword lines, for the model's second opinion on a burst the phrase matched. */
    const val CONFIRM_THRESHOLD = 0.04
    const val VERIFY_SECONDS = 2.0

    /** Every spelling, each with its boost and threshold, one per line. */
    fun lines(threshold: Double, score: Double = KEYWORDS_SCORE) = SPELLINGS.joinToString("\n") { "$it :$score #$threshold @HEY_CRU" }
    fun line(threshold: Double, score: Double = KEYWORDS_SCORE) = lines(threshold, score)

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
        // Noise reduction (spectral subtraction): the steady background in each band, from the
        // clip's quietest fifth, taken away before anything is compared, in training and listening alike.
        if (mels.size >= 5) {
            val q = mels.indices.sortedBy { mels[it].sum() }.take(max(1, mels.size / 5)).map { mels[it] }
            val noise = DoubleArray(MELS)
            for (e in q) for (m in 0 until MELS) noise[m] += e[m] / q.size
            for (e in mels) for (m in 0 until MELS) e[m] = max(e[m] - 1.5 * noise[m], e[m] * 0.05)
        }
        val fl = max(loudest * 1e-3, 1e-10)
        val out = mels.map { e -> DoubleArray(CEPS) { j -> var s = 0.0; val row = dct[j]; for (m in 0 until MELS) s += row[m] * ln(max(e[m], fl)); s } }
        if (out.isEmpty()) return out
        val mean = DoubleArray(CEPS)
        for (f in out) for (j in 0 until CEPS) mean[j] += f[j] / out.size
        for (f in out) for (j in 0 until CEPS) f[j] -= mean[j]
        // How the sound moves (deltas) counts for more than its colour, which the room and the phone change.
        return out.indices.map { t ->
            val a = out[max(0, t - 1)]; val b = out[min(out.size - 1, t + 1)]; val f = out[t]
            DoubleArray(CEPS * 2) { j -> if (j < CEPS) STATIC_WEIGHT * f[j] else DELTA_WEIGHT * (b[j - CEPS] - a[j - CEPS]) / 2 }
        }
    }

    private const val STATIC_WEIGHT = 0.6
    private const val DELTA_WEIGHT = 1.8

    /** The same take heard another way: muffled and echoing, and brighter (enroll.js variants). */
    fun variants(clip: FloatArray): List<FloatArray> {
        fun peakOf(a: FloatArray) = a.maxOfOrNull { abs(it) } ?: 0f
        val p0 = peakOf(clip).takeIf { it > 0f } ?: 1f
        fun norm(a: FloatArray): FloatArray { val k = p0 / (peakOf(a).takeIf { it > 0f } ?: 1f); for (i in a.indices) a[i] *= k; return a }
        val far = FloatArray(clip.size)
        var y = 0.0
        for (i in clip.indices) { y += 0.45 * (clip[i] - y); far[i] = y.toFloat() }
        fun at(a: FloatArray, i: Int) = if (i >= 0) a[i] else 0f
        val echo = FloatArray(clip.size) { i -> (far[i] + 0.35 * at(far, i - 480) + 0.2 * at(far, i - 1120) + 0.1 * at(far, i - 1900)).toFloat() }
        val bright = FloatArray(clip.size) { i -> (clip[i] - 0.6 * at(clip, i - 1)).toFloat() }
        return listOf(norm(echo), norm(bright))
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

    /** DTW of [a] against [b], both ends held, as the mean distance per step (enroll.js fullDistance). */
    fun fullDistance(a: List<DoubleArray>, b: List<DoubleArray>): Double {
        val m = a.size; val n = b.size
        if (m == 0 || n == 0) return Double.POSITIVE_INFINITY
        var pc = DoubleArray(n + 1) { Double.POSITIVE_INFINITY }; var pl = DoubleArray(n + 1)
        var cc = DoubleArray(n + 1); var cl = DoubleArray(n + 1)
        pc[0] = 0.0
        for (i in 1..m) {
            cc[0] = Double.POSITIVE_INFINITY; cl[0] = 0.0
            val t = a[i - 1]
            for (j in 1..n) {
                val c = b[j - 1]
                var s = 0.0; for (k in t.indices) { val d = t[k] - c[k]; s += d * d }
                val x = sqrt(s)
                var best = pc[j - 1]; var len = pl[j - 1]
                if (cc[j - 1] < best) { best = cc[j - 1]; len = cl[j - 1] }
                if (pc[j] < best) { best = pc[j]; len = pl[j] }
                cc[j] = best + x; cl[j] = len + 1
            }
            val x1 = pc; pc = cc; cc = x1
            val x2 = pl; pl = cl; cl = x2
        }
        return if (pl[n] > 0) pc[n] / pl[n] else Double.POSITIVE_INFINITY
    }

    /** The share of a clip's loud 30 ms frames with a clear pitch, 70 to 400 Hz (enroll.js voicedShare). */
    fun voicedShare(clip: FloatArray): Double {
        val n = 480; val h = 240
        var peak = 0.0
        var i = 0
        while (i + n <= clip.size) { var e = 0.0; for (k in 0 until n) e += clip[i + k].toDouble() * clip[i + k]; peak = max(peak, e); i += h }
        if (peak == 0.0) return 0.0
        var loud = 0; var voiced = 0
        i = 0
        while (i + n <= clip.size) {
            var e = 0.0; for (k in 0 until n) e += clip[i + k].toDouble() * clip[i + k]
            if (e >= peak * 0.05) {
                loud++
                var best = 0.0
                for (lag in 40..228) {
                    var c = 0.0; var e1 = 0.0; var e2 = 0.0
                    var k = 0
                    while (k + lag < n) { val a = clip[i + k].toDouble(); val b = clip[i + k + lag].toDouble(); c += a * b; e1 += a * a; e2 += b * b; k++ }
                    val r = c / sqrt(if (e1 * e2 == 0.0) 1.0 else e1 * e2)
                    if (r > best) best = r
                }
                if (best > 0.45) voiced++
            }
            i += h
        }
        return if (loud > 0) voiced.toDouble() / loud else 0.0
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

        val templates = clips.map { mfcc(it) }
        val nearest = templates.indices.map { i -> templates.indices.filter { it != i }.minOf { distance(templates[it], templates[i]) } }
        val median = nearest.sorted()[nearest.size / 2]
        val odd = nearest.indices.filter { nearest[it] > median * 2 }
        if (odd.isNotEmpty() && odd.size < templates.size / 2.0) return Trained.Redo("One recording sounds unlike the others. Say it again the way you normally would.", odd)
        val spread = nearest.max()
        // For the phrase match: each take against the others, whole to whole, both ways.
        val near = templates.indices.map { i -> templates.indices.filter { it != i }.minOf { maxOf(fullDistance(templates[it], templates[i]), fullDistance(templates[i], templates[it])) } }
        val typical = near.sorted()[near.size / 2]
        val frames = templates.map { it.size }.sorted()[templates.size / 2]

        // The model, tuned: the strictest threshold that hears as many takes as any, two steps looser.
        // Takes it never hears are fine: the owner's phrase wakes for those.
        var best = THRESHOLDS.lastIndex; var most = 0
        for ((t, th) in THRESHOLDS.withIndex()) {
            val n = clips.count { hears(lines(th), padded(it)) }
            if (n > most) { most = n; best = t }
            if (n == clips.size) break
        }
        val threshold = THRESHOLDS[min(THRESHOLDS.lastIndex, best + 2)]
        return Trained.Ok(WakeProfile(
            keyword = lines(threshold),
            keywordsThreshold = threshold,
            limit = (spread * LIMIT_MARGIN * 1000).roundToInt() / 1000.0,
            strictLimit = (typical * STRICT_MARGIN * 1000).roundToInt() / 1000.0,
            modelHeard = most,
            phraseFrames = frames,
            takes = templates.size,
            templates = (templates + clips.flatMap { c -> variants(c).map { mfcc(it) } }).map { t -> t.map { f -> f.map { (it * 1000).roundToInt() / 1000.0 } } },
            trainedAt = now,
        ))
    }

    /**
     * Bursts of speech with a pause either side, as they end: feed it the
     * microphone in pieces; it gives back the burst (with the piece before)
     * once 0.3 s of quiet follows, else null. Over 2.5 s (talk, not a call)
     * is dropped. The quiet level follows the room. As enroll.js's createSegmenter.
     */
    class Segmenter {
        private var floor = 0.002; private var active = false; private var long = false
        private var quietMs = 0.0; private var loudMs = 0.0
        private val parts = ArrayList<FloatArray>(); private var pre: FloatArray? = null

        fun accept(samples: FloatArray): FloatArray? {
            var sum = 0.0
            for (x in samples) sum += x.toDouble() * x
            val rms = sqrt(sum / max(1, samples.size))
            val ms = samples.size * 1000.0 / RATE
            val loud = rms > max(0.006, floor * 3)
            if (!loud) floor = if (rms < floor) floor * 0.7 + rms * 0.3 else floor * 0.995 + rms * 0.005
            if (!active) {
                if (loud) { active = true; long = false; quietMs = 0.0; loudMs = ms; parts.clear(); pre?.let { parts += it }; parts += samples.copyOf() }
                else pre = samples.copyOf()
                return null
            }
            parts += samples.copyOf()
            if (loud) { quietMs = 0.0; loudMs += ms } else quietMs += ms
            if (loudMs + quietMs > 2500) { long = true; parts.clear() }
            if (quietMs < 300) return null
            active = false; pre = null
            if (long) return null
            val out = FloatArray(parts.sumOf { it.size }); var o = 0
            for (p in parts) { p.copyInto(out, o); o += p.size }
            parts.clear()
            return out
        }

        fun reset() { active = false; long = false; quietMs = 0.0; loudMs = 0.0; parts.clear(); pre = null }
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
