package mv.obitech.crundi.voice

import android.content.Context
import android.media.AudioAttributes
import android.media.MediaPlayer
import android.media.audiofx.Visualizer
import android.os.Bundle
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import mv.obitech.crundi.api.VoiceApi
import java.io.File
import java.util.Locale
import kotlin.coroutines.resume
import kotlin.math.abs
import kotlin.math.sin
import kotlin.random.Random

/**
 * Says Crundi's answers: in the Deepgram voice chosen on the server
 * ([deepgramVoice], fetched through /api/voice/speak) or, when none is chosen
 * or Deepgram can't be reached, in the phone's own voice (D34).
 *
 * A reply is said a sentence at a time and the next sentence is fetched while
 * one plays, so Crundi starts talking after the first sentence, not the
 * whole answer. [level] (0..1) follows the voice for the screen's visual.
 *
 * Main thread only.
 */
class Speaker(private val ctx: Context, private val scope: CoroutineScope) {
    /** The server to fetch Deepgram speech from; null = the phone's voice only. */
    var api: VoiceApi? = null
    /** A Deepgram voice name, or "" for the phone's own. */
    var deepgramVoice: String = ""

    private val _level = MutableStateFlow(0f)
    val level: StateFlow<Float> = _level

    private var job: Job? = null
    private var player: MediaPlayer? = null
    private var viz: Visualizer? = null
    private val attrs = AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_ASSISTANT).setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build()

    private var tts: TextToSpeech? = null
    private var ttsReady: Boolean? = null
    private var ttsDone: ((Boolean) -> Unit)? = null

    val speaking: Boolean get() = job?.isActive == true

    /** Say [text]; [onDone] runs once when it has all been said (not after [stop]). */
    fun say(text: String, onDone: () -> Unit) {
        stop()
        val parts = Speech.sentences(text)
        if (parts.isEmpty()) { onDone(); return }
        job = scope.launch {
            var cloud = deepgramVoice.isNotEmpty() && api != null
            var next: Deferred<ByteArray?>? = if (cloud) fetch(parts[0]) else null
            for ((i, part) in parts.withIndex()) {
                if (cloud) {
                    val audio = next?.await()
                    next = if (i + 1 < parts.size) fetch(parts[i + 1]) else null
                    if (audio != null && play(audio)) continue
                    // Deepgram failed: the phone's voice for the rest.
                    cloud = false
                    next?.cancel()
                }
                if (!device(part)) break
            }
            _level.value = 0f
            job = null
            onDone()
        }
    }

    /** Stop at once (Master talked over it, or the session closed). */
    fun stop() {
        job?.cancel(); job = null
        releasePlayer()
        tts?.stop()
        ttsDone?.invoke(false); ttsDone = null
        _level.value = 0f
    }

    fun release() {
        stop()
        tts?.shutdown(); tts = null; ttsReady = null
    }

    private fun fetch(text: String): Deferred<ByteArray?> = scope.async {
        runCatching { api?.speak(text, deepgramVoice) ?: error("no server") }.onFailure { Log.w(TAG, "Deepgram voice: ${it.message}") }.getOrNull()
    }

    /** One mp3, played to the end. False if it could not be played. */
    private suspend fun play(mp3: ByteArray): Boolean {
        val file = withContext(Dispatchers.IO) { File.createTempFile("say", ".mp3", ctx.cacheDir).apply { writeBytes(mp3); deleteOnExit() } }
        try {
            return suspendCancellableCoroutine { cont ->
                val mp = MediaPlayer()
                player = mp
                try {
                    mp.setAudioAttributes(attrs)
                    mp.setDataSource(file.path)
                    mp.setOnCompletionListener { if (cont.isActive) cont.resume(true) }
                    mp.setOnErrorListener { _, _, _ -> if (cont.isActive) cont.resume(false); true }
                    mp.setOnPreparedListener { it.start(); follow(it) }
                    mp.prepareAsync()
                } catch (e: Exception) {
                    if (cont.isActive) cont.resume(false)
                }
                cont.invokeOnCancellation { scope.launch(Dispatchers.Main) { releasePlayer() } }
            }
        } finally {
            releasePlayer()
            withContext(Dispatchers.IO) { file.delete() }
        }
    }

    /** The level of what the player is saying, from its own output (needs the microphone permission, which the assistant has). */
    private fun follow(mp: MediaPlayer) {
        viz = runCatching {
            Visualizer(mp.audioSessionId).apply {
                captureSize = Visualizer.getCaptureSizeRange()[0].coerceAtLeast(128)
                setDataCaptureListener(object : Visualizer.OnDataCaptureListener {
                    override fun onWaveFormDataCapture(v: Visualizer?, wave: ByteArray?, rate: Int) {
                        if (wave == null || wave.isEmpty()) return
                        var sum = 0.0
                        for (b in wave) { val x = (b.toInt() and 0xff) - 128; sum += x * x }
                        val rms = kotlin.math.sqrt(sum / wave.size) / 128.0
                        _level.value = (kotlin.math.sqrt(rms) * 1.4).toFloat().coerceIn(0f, 1f)
                    }
                    override fun onFftDataCapture(v: Visualizer?, fft: ByteArray?, rate: Int) {}
                }, Visualizer.getMaxCaptureRate() / 2, true, false)
                enabled = true
            }
        }.getOrNull()
        if (viz == null) pulseWhile { player === mp }
    }

    private fun releasePlayer() {
        runCatching { viz?.enabled = false; viz?.release() }; viz = null
        runCatching { player?.stop() }
        runCatching { player?.release() }
        player = null
    }

    /** The phone's own voice for one sentence. False if it was stopped. */
    private suspend fun device(text: String): Boolean {
        val t = engine() ?: return true
        return suspendCancellableCoroutine { cont ->
            ttsDone = { ok -> ttsDone = null; if (cont.isActive) cont.resume(ok) }
            val id = "s" + System.nanoTime()
            if (t.speak(text, TextToSpeech.QUEUE_FLUSH, Bundle(), id) != TextToSpeech.SUCCESS) { ttsDone = null; cont.resume(true); return@suspendCancellableCoroutine }
            pulseWhile { ttsDone != null }
        }
    }

    /** The engine, made once and kept: its first start is slow. Null if the phone has none. */
    private suspend fun engine(): TextToSpeech? {
        tts?.let { if (ttsReady == true) return it }
        if (ttsReady == false) return null
        return suspendCancellableCoroutine { cont ->
            var made: TextToSpeech? = null
            made = TextToSpeech(ctx) { status -> ready(made, status, cont) }
        }
    }

    /** The engine has started (or failed to): choose the voice (D34) and hand it over. */
    private fun ready(e: TextToSpeech?, status: Int, cont: kotlinx.coroutines.CancellableContinuation<TextToSpeech?>) {
        if (status != TextToSpeech.SUCCESS || e == null) { ttsReady = false; if (cont.isActive) cont.resume(null); return }
        e.setAudioAttributes(attrs)
        val voices = runCatching { e.voices.orEmpty().map { v -> DeviceVoice(v.name, v.locale.toLanguageTag(), v.isNetworkConnectionRequired, !v.features.orEmpty().contains(TextToSpeech.Engine.KEY_FEATURE_NOT_INSTALLED)) } }.getOrDefault(emptyList())
        val pick = DeviceVoices.pick(voices, VoicePrefs.deviceVoice(ctx))
        val voice = pick?.let { p -> e.voices?.firstOrNull { it.name == p.name } }
        if (voice != null) e.setVoice(voice) else e.setLanguage(Locale.UK)
        e.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
            override fun onStart(id: String?) {}
            override fun onDone(id: String?) { scope.launch(Dispatchers.Main) { ttsDone?.invoke(true) } }
            @Deprecated("Still called by older engines") override fun onError(id: String?) { scope.launch(Dispatchers.Main) { ttsDone?.invoke(true) } }
            override fun onRangeStart(id: String?, start: Int, end: Int, frame: Int) { _level.value = 0.55f + Random.nextFloat() * 0.4f }
        })
        tts = e; ttsReady = true
        if (cont.isActive) cont.resume(e)
    }

    /** A voice the phone can't measure still moves: a soft wave while [still] holds. */
    private fun pulseWhile(still: () -> Boolean) = scope.launch {
        var t = 0.0
        while (still()) {
            t += 0.12
            val target = (0.35 + 0.25 * abs(sin(t * 2.3)) + 0.15 * abs(sin(t * 5.1))).toFloat()
            _level.value = _level.value * 0.6f + target * 0.4f
            delay(40)
        }
        _level.value = 0f
    }

    companion object { private const val TAG = "CrundiSpeaker" }
}

/** The phone's own voices, for the picker (D34). Calls back once, on the main thread. */
fun listDeviceVoices(ctx: Context, done: (List<DeviceVoice>) -> Unit) {
    var t: TextToSpeech? = null
    t = TextToSpeech(ctx.applicationContext) { status ->
        val e = t
        val list = if (status == TextToSpeech.SUCCESS && e != null) runCatching {
            e.voices.orEmpty()
                .filter { it.locale.language == "en" }
                .map { v -> DeviceVoice(v.name, v.locale.toLanguageTag(), v.isNetworkConnectionRequired, !v.features.orEmpty().contains(TextToSpeech.Engine.KEY_FEATURE_NOT_INSTALLED)) }
                .sortedWith(compareBy({ !it.locale.equals("en-GB", true) }, { it.name }))
        }.getOrDefault(emptyList()) else emptyList()
        e?.shutdown()
        done(list)
    }
}
