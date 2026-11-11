package mv.obitech.crundi.voice

import android.content.Context
import android.util.Log
import com.k2fsa.sherpa.onnx.SpeakerEmbeddingExtractor
import com.k2fsa.sherpa.onnx.SpeakerEmbeddingExtractorConfig
import kotlinx.serialization.Serializable
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest
import kotlin.math.sqrt

/** The owner's voice in a "Hey Cru" profile: his five takes as voice embeddings (unit length). */
@Serializable
data class WakeVoiceData(val model: String = "", val clips: List<List<Double>> = emptyList()) {
    fun valid() = model == WakeVoice.MODEL_ID && clips.size >= WakeVoice.K && clips.all { c -> c.size == WakeVoice.DIM && c.all { it.isFinite() } }
}

/**
 * "Hey Cru" for the owner only (12 Nov, the owner: "owner detection should be
 * there for hey cru as well, client side"). The phrase match and the model
 * ([WakeProfile]) say "Hey Cru" was said; this says the owner said it. As the
 * PC's app/voice-wake/owner.js: the model the server checks the owner's voice
 * with (3D-Speaker ERes2Net, 26 MB), here run by sherpa-onnx's own extractor,
 * downloaded once into the app's files, checked by size and SHA-256; a wake is
 * his when the mean of its [K] best cosines with his takes reaches [THRESHOLD].
 * Without the model there is no verdict, and the wake stands, as before.
 */
object WakeVoice {
    const val MODEL_ID = "eres2net-voxceleb"
    private const val FILE = "3dspeaker_speech_eres2net_sv_en_voxceleb_16k.onnx"
    private const val URL_ = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_eres2net_sv_en_voxceleb_16k.onnx"
    private const val BYTES = 26485263L
    private const val SHA256 = "c59158379255ad66e161679cca6af8d52d51e389e3224ab7d7a7baae295c2db5"
    const val DIM = 192
    const val K = 3
    /** As owner.js OWNER_THRESHOLD (measured on the PC's recordings: every owner's own calls at 0.43 or more). */
    const val THRESHOLD = 0.42
    /** Under this much sound there is too little to tell. */
    const val MIN_SECONDS = 0.3
    private const val TAG = "CrundiWake"

    fun file(ctx: Context) = File(ctx.filesDir, "voice-wake/speaker/$FILE")
    fun present(ctx: Context) = file(ctx).let { it.isFile && it.length() == BYTES }

    /** The model on this phone, fetched once (blocking: call off the main thread). Null, with the reason logged, when it can't be had. */
    fun ensure(ctx: Context): File? {
        val f = file(ctx)
        if (present(ctx)) return f
        return try {
            f.parentFile?.mkdirs()
            val tmp = File(f.parentFile, f.name + ".part")
            val c = URL(URL_).openConnection() as HttpURLConnection
            c.connectTimeout = 15_000; c.readTimeout = 30_000; c.instanceFollowRedirects = true
            try {
                if (c.responseCode !in 200..299) error("HTTP ${c.responseCode}")
                val md = MessageDigest.getInstance("SHA-256")
                c.inputStream.use { input -> tmp.outputStream().use { out ->
                    val buf = ByteArray(64 * 1024)
                    while (true) { val n = input.read(buf); if (n < 0) break; md.update(buf, 0, n); out.write(buf, 0, n) }
                } }
                val hex = md.digest().joinToString("") { "%02x".format(it) }
                if (tmp.length() != BYTES || hex != SHA256) { tmp.delete(); error("the download failed its check") }
                if (!tmp.renameTo(f)) error("could not keep the download")
            } finally { c.disconnect() }
            f
        } catch (e: Exception) { Log.w(TAG, "voice model unavailable: ${e.message}"); null }
    }

    /** The extractor for a model file; null when the native library can't run it. */
    class Extractor private constructor(private val ex: SpeakerEmbeddingExtractor) {
        fun embed(samples: FloatArray): FloatArray? = try {
            val s = ex.createStream()
            try {
                s.acceptWaveform(samples, WakeWordEngine.RATE)
                s.inputFinished()
                if (ex.isReady(s)) ex.compute(s) else null
            } finally { s.release() }
        } catch (e: Throwable) { Log.w(TAG, "voice embedding failed: ${e.message}"); null }

        fun release() = runCatching { ex.release() }

        companion object {
            fun load(model: File): Extractor? = try {
                Extractor(SpeakerEmbeddingExtractor(config = SpeakerEmbeddingExtractorConfig(model = model.absolutePath, numThreads = 1, debug = false, provider = "cpu")))
            } catch (e: Throwable) { Log.w(TAG, "voice model could not be loaded: $e"); null }
        }
    }

    fun normalise(v: FloatArray): DoubleArray {
        var s = 0.0; for (x in v) s += x.toDouble() * x
        val k = 1 / (sqrt(s).takeIf { it > 0 } ?: 1.0)
        return DoubleArray(v.size) { v[it] * k }
    }

    private fun cosine(a: DoubleArray, b: List<Double>): Double {
        var d = 0.0; var x = 0.0; var y = 0.0
        for (i in a.indices) { d += a[i] * b[i]; x += a[i] * a[i]; y += b[i] * b[i] }
        return d / (sqrt(x * y).takeIf { it > 0 } ?: 1.0)
    }

    /** The profile's voice from the takes' embeddings (rounded, as owner.js keeps it). */
    fun enrol(embeddings: List<FloatArray>): WakeVoiceData =
        WakeVoiceData(MODEL_ID, embeddings.map { e -> normalise(e).map { Math.round(it * 1e4) / 1e4 } })

    /** The mean of the [K] best cosines with the takes (the server's wordScore). */
    fun score(voice: WakeVoiceData, embedding: FloatArray): Double {
        val v = normalise(embedding)
        val c = voice.clips.map { cosine(v, it) }.sortedDescending()
        val n = minOf(K, c.size)
        return if (n == 0) Double.NaN else c.take(n).sum() / n
    }

    /** true (the owner), false (another voice), null (no verdict: no voice, too short, no embedding). */
    fun verdict(voice: WakeVoiceData?, embedding: FloatArray?, seconds: Double): Boolean? {
        if (voice == null || !voice.valid() || embedding == null || seconds < MIN_SECONDS) return null
        val s = score(voice, embedding)
        return if (s.isFinite()) s >= THRESHOLD else null
    }
}
