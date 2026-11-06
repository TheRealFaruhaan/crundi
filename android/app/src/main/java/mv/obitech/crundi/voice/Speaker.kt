package mv.obitech.crundi.voice

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioTrack
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
import kotlinx.coroutines.isActive
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
 * [cue] plays a short "one moment" clip (a file made by the server in the
 * Deepgram voice, kept on the phone) or says the phrase in the phone's own
 * voice. An answer takes over from it: a cue within [CUE_FINISH_MS] of its
 * end is let finish, any other is stopped.
 *
 * Main thread only.
 */
class Speaker(private val ctx: Context, private val scope: CoroutineScope) {
    /** The server to fetch Deepgram speech from; null = the phone's voice only. */
    var api: VoiceApi? = null
    /** A Deepgram voice name, or "" for the phone's own. */
    var deepgramVoice: String = ""
    /** How fast Cru talks (the server's speechRate): Deepgram applies it; the phone's own voice is set to it here. */
    var rate: Float = 1f
    /** The server streams speech as it is made (capability "voice-tts-stream"). */
    var streams: Boolean = false

    private val _level = MutableStateFlow(0f)
    val level: StateFlow<Float> = _level

    private var job: Job? = null
    private var player: MediaPlayer? = null
    private var viz: Visualizer? = null
    /**
     * How Cru is played ([CruAudio]): as call audio on the speaker, so the
     * call-mode microphone's echo canceller takes it out, or as assistant
     * audio (headphones, older phones). [sessionId]: the microphone's audio
     * session, which the playback joins (0: its own).
     */
    var route: CruAudio.Route = CruAudio.Route.Media
    var sessionId: Int = 0
    private val attrs: AudioAttributes get() = AudioAttributes.Builder()
        .setUsage(if (route == CruAudio.Route.Call) AudioAttributes.USAGE_VOICE_COMMUNICATION else AudioAttributes.USAGE_ASSISTANT)
        .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build()

    private var tts: TextToSpeech? = null
    private var ttsReady: Boolean? = null
    private var ttsDone: ((Boolean) -> Unit)? = null

    val speaking: Boolean get() = job?.isActive == true

    /** [level] is measured from the actual playback (Deepgram's voice), not drawn. */
    val measured: Boolean get() = viz != null || track != null || cueViz != null
    private var cueViz: Visualizer? = null

    /**
     * Cru's level now, for the owner gate: 0 when nothing plays, null when it
     * can't be known (the phone's own voice, or a clip that can't be measured).
     */
    val playLevel: Float? get() = when {
        cueOnDevice || ttsId != null -> null
        player != null && viz == null -> null
        cuePlayer != null && cueViz == null -> null
        else -> _level.value
    }

    @Volatile private var track: AudioTrack? = null
    /** (first frame, level) of each 20 ms block written to [track], oldest first. */
    private val levels = java.util.ArrayDeque<Pair<Long, Float>>()
    private fun lvl(sum: Double, n: Int) = (kotlin.math.sqrt(kotlin.math.sqrt(sum / n) / 32768.0) * 1.4).toFloat().coerceIn(0f, 1f)

    private var cuePlayer: MediaPlayer? = null
    private var cueOnDevice = false
    private var cueDone: (() -> Unit)? = null
    /** The device voice's utterance now being said for a reply (its "done" is not the cue's). */
    private var ttsId: String? = null

    /** A cue is playing. */
    val cueing: Boolean get() = cueDone != null

    /**
     * Play a cue: [file] (an mp3 on the phone), or [text] in the phone's own
     * voice when [file] is null. [onDone] runs once, when it ends or is stopped.
     * False when it could not be played.
     */
    fun cue(file: File?, text: String, onDone: () -> Unit): Boolean {
        stopCue()
        if (file != null) {
            val mp = runCatching { MediaPlayer().apply { setAudioAttributes(attrs); joinSession(this); setDataSource(file.path); prepare() } }.getOrNull() ?: return false
            cuePlayer = mp
            cueDone = onDone
            // Its level, for the gate: how loud Cru is now.
            cueViz = runCatching { levelOf(mp.audioSessionId) }.getOrNull()
            mp.setOnCompletionListener { if (cuePlayer === mp) stopCue() }
            mp.setOnErrorListener { _, _, _ -> if (cuePlayer === mp) stopCue(); true }
            runCatching { mp.start() }.onFailure { stopCue(); return false }
            return true
        }
        cueOnDevice = true
        cueDone = onDone
        scope.launch {
            val t = engine()
            if (t == null || !cueOnDevice) { if (cueOnDevice) stopCue(); return@launch }
            t.setSpeechRate(rate)
            t.setAudioAttributes(attrs)
            if (t.speak(text, TextToSpeech.QUEUE_FLUSH, ttsParams(), CUE_ID) != TextToSpeech.SUCCESS) stopCue()
        }
        return true
    }

    /** How long the cue still has to play, ms; 0 when none, or the phone's voice (unknown). */
    fun cueLeftMs(): Long = cuePlayer?.let { mp -> runCatching { (mp.duration - mp.currentPosition).toLong().coerceAtLeast(0L) }.getOrDefault(0L) } ?: 0L

    fun stopCue() {
        val done = cueDone ?: return
        cueDone = null
        runCatching { cueViz?.enabled = false; cueViz?.release() }; cueViz = null
        cuePlayer?.let { mp -> runCatching { mp.stop() }; runCatching { mp.release() } }
        cuePlayer = null
        if (viz == null && track == null) _level.value = 0f
        if (cueOnDevice) { cueOnDevice = false; if (ttsId == null) tts?.stop() }
        done()
    }

    /** Say [text]; [onDone] runs once when it has all been said (not after [stop]). */
    fun say(text: String, onDone: () -> Unit) {
        stopSpeech()
        val parts = Speech.sentences(text)
        if (parts.isEmpty()) { stopCue(); onDone(); return }
        job = scope.launch {
            // A cue nearly done is let finish; otherwise the answer takes over now.
            val left = cueLeftMs()
            if (left in 1..CUE_FINISH_MS) delay(left)
            stopCue()
            var cloud = deepgramVoice.isNotEmpty() && api != null
            // Streamed: the whole answer in one go, playing from its first bytes.
            if (cloud && streams) {
                when (stream(text)) {
                    true -> { if (!isActive) return@launch; _level.value = 0f; job = null; onDone(); return@launch }
                    false -> {}            // nothing played: try the other ways
                    null -> return@launch  // stopped
                }
            }
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

    /** Stop at once (The owner talked over it, or the session closed), the cue too. */
    fun stop() {
        stopSpeech()
        stopCue()
    }

    private fun stopSpeech() {
        job?.cancel(); job = null
        releaseTrack()
        releasePlayer()
        tts?.stop()
        ttsDone?.invoke(false); ttsDone = null
        _level.value = 0f
    }

    fun release() {
        stop()
        tts?.shutdown(); tts = null; ttsReady = null
    }

    /**
     * Plays [text] streamed from the server: PCM into an AudioTrack as it
     * arrives. True when it was played to the end, false when nothing could
     * be played (try the other ways), null when it was stopped.
     */
    private suspend fun stream(text: String): Boolean? {
        val v = api ?: return false
        val rate = VoiceApi.STREAM_RATE
        val t = runCatching {
            AudioTrack.Builder()
                .setAudioAttributes(attrs)
                .apply { if (sessionId > 0) setSessionId(sessionId) }
                .setAudioFormat(AudioFormat.Builder().setSampleRate(rate).setEncoding(AudioFormat.ENCODING_PCM_16BIT).setChannelMask(AudioFormat.CHANNEL_OUT_MONO).build())
                .setTransferMode(AudioTrack.MODE_STREAM)
                .setBufferSizeInBytes(maxOf(AudioTrack.getMinBufferSize(rate, AudioFormat.CHANNEL_OUT_MONO, AudioFormat.ENCODING_PCM_16BIT), rate / 5 * 2))
                .build()
        }.getOrNull() ?: return false
        track = t
        var frames = 0L
        synchronized(levels) { levels.clear() }
        // The level of what is being heard now: the block at the play head.
        val ticker = scope.launch {
            while (track === t) {
                val head = runCatching { t.playbackHeadPosition.toLong() }.getOrDefault(0L)
                synchronized(levels) {
                    while (levels.size > 1 && levels.elementAt(1).first <= head) levels.removeFirst()
                    _level.value = levels.firstOrNull()?.takeIf { head >= it.first }?.second ?: 0f
                }
                delay(16)
            }
        }
        var odd: Int = -1
        val sink = object : java.io.OutputStream() {
            override fun write(b: Int) = write(byteArrayOf(b.toByte()), 0, 1)
            override fun write(b: ByteArray, off: Int, len: Int) {
                if (track !== t || len <= 0) return
                // Keep whole samples: a chunk may end half way through one.
                var start = off; var n = len
                if (odd >= 0) { t.write(byteArrayOf(odd.toByte(), b[off]), 0, 2); start++; n--; odd = -1; frames++ }
                if (n % 2 == 1) { odd = b[start + n - 1].toInt() and 0xff; n-- }
                if (n <= 0) return
                // The level of each 20 ms of what is written, keyed by its frame:
                // the screen reads the one being played now (see the ticker below).
                var i = start
                var sum = 0.0; var cnt = 0; var blockStart = frames
                while (i + 1 < start + n) {
                    val x = ((b[i + 1].toInt() shl 8) or (b[i].toInt() and 0xff)).toShort().toInt()
                    sum += x.toDouble() * x; cnt++; i += 2
                    if (cnt == BLOCK) { val e = blockStart to lvl(sum, cnt); synchronized(levels) { levels.addLast(e) }; blockStart += cnt; sum = 0.0; cnt = 0 }
                }
                if (cnt > 0) { val e = blockStart to lvl(sum, cnt); synchronized(levels) { levels.addLast(e) } }
                if (t.playState != AudioTrack.PLAYSTATE_PLAYING) t.play()
                t.write(b, start, n)
                frames += n / 2
            }
        }
        return try {
            withContext(Dispatchers.IO) {
                v.speakStream(text, deepgramVoice, sink)
                // Let what is buffered play out.
                while (track === t && t.playbackHeadPosition < frames) { delay(30) }
            }
            if (track !== t) null else true
        } catch (e: kotlinx.coroutines.CancellationException) {
            null
        } catch (e: Exception) {
            Log.w(TAG, "streamed voice: ${e.message}")
            if (frames > 0) true else false
        } finally {
            ticker.cancel()
            if (track === t) releaseTrack()
        }
    }

    private fun releaseTrack() {
        val t = track ?: return
        track = null
        runCatching { t.pause(); t.flush() }
        runCatching { t.release() }
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
                    joinSession(mp)
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
        viz = runCatching { levelOf(mp.audioSessionId) }.getOrNull()
        if (viz == null) pulseWhile { player === mp }
    }

    /** The playback shares the microphone's session (the echo canceller's reference on some phones). */
    private fun joinSession(mp: MediaPlayer) { if (sessionId > 0) runCatching { mp.audioSessionId = sessionId } }

    /** The device voice's parameters: the microphone's session too. */
    private fun ttsParams() = Bundle().apply { if (sessionId > 0) putInt(TextToSpeech.Engine.KEY_PARAM_SESSION_ID, sessionId) }

    /** [_level] follows what is played on [session]. */
    private fun levelOf(session: Int): Visualizer =
            Visualizer(session).apply {
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
            val id = "s" + System.nanoTime()
            ttsId = id
            ttsDone = { ok -> ttsDone = null; ttsId = null; if (cont.isActive) cont.resume(ok) }
            t.setSpeechRate(rate)
            t.setAudioAttributes(attrs)
            if (t.speak(text, TextToSpeech.QUEUE_FLUSH, ttsParams(), id) != TextToSpeech.SUCCESS) { ttsDone = null; ttsId = null; cont.resume(true); return@suspendCancellableCoroutine }
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
            override fun onDone(id: String?) { scope.launch(Dispatchers.Main) { ended(id) } }
            @Deprecated("Still called by older engines") override fun onError(id: String?) { scope.launch(Dispatchers.Main) { ended(id) } }
            override fun onRangeStart(id: String?, start: Int, end: Int, frame: Int) { _level.value = 0.55f + Random.nextFloat() * 0.4f }
        })
        tts = e; ttsReady = true
        if (cont.isActive) cont.resume(e)
    }

    /** An utterance ended: the cue's, or a reply sentence's. */
    private fun ended(id: String?) {
        if (id == CUE_ID) { if (cueOnDevice) stopCue(); return }
        ttsDone?.invoke(true)
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

    companion object {
        private const val TAG = "CrundiSpeaker"
        /** 20 ms at the streamed rate. */
        private const val BLOCK = VoiceApi.STREAM_RATE / 50
        private const val CUE_ID = "cru-cue"
        /** A cue this close to its end is let finish before the answer. */
        const val CUE_FINISH_MS = 300L
    }
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
