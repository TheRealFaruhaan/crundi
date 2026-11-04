package mv.obitech.crundi.voice

import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.speech.RecognitionListener
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import android.util.Log
import mv.obitech.crundi.api.ListenEvent

/**
 * The phone's own speech recognition, used only when the live stream to the
 * server can't be had (offline, a server without the assistant's stream, no
 * Deepgram key): D37. Phase 0 showed it mishears Crundi's words, so it is a
 * fallback, not the way in.
 *
 * ONE recogniser for the whole session, started again after each result:
 * destroying and making a new one caused frequent error 11 (server
 * disconnected) in the Voice Lab. It turns the recogniser's results into the
 * same [ListenEvent]s the stream sends. It holds the microphone itself, so
 * MicStream is off while it is used. Main thread only.
 */
class FallbackRecognizer(
    private val ctx: Context,
    private val vocabulary: List<String>,
    /** The microphone's loudness while it listens, 0..1, for the screen. */
    private val onLevel: (Float) -> Unit,
    private val emit: (ListenEvent) -> Unit,
) {
    private val main = Handler(Looper.getMainLooper())
    private var sr: SpeechRecognizer? = null
    private var active = false
    private var listening = false

    val available: Boolean get() = SpeechRecognizer.isRecognitionAvailable(ctx) ||
        (Build.VERSION.SDK_INT >= 31 && SpeechRecognizer.isOnDeviceRecognitionAvailable(ctx))

    fun start() {
        if (active) return
        active = true
        if (sr == null) sr = make() ?: run { emit(ListenEvent.Failed("this phone has no speech recognition")); active = false; return }
        listen()
    }

    /** Stop listening for now (Crundi is speaking); [start] carries on with the same recogniser. */
    fun pause() {
        active = false
        if (listening) runCatching { sr?.cancel() }
        listening = false
    }

    fun release() {
        active = false
        main.removeCallbacksAndMessages(null)
        runCatching { sr?.destroy() }
        sr = null
    }

    private fun make(): SpeechRecognizer? = runCatching {
        // The phone's default (network) recogniser did slightly better than
        // the on-device one in Phase 0; on-device is the one that works offline.
        val r = if (SpeechRecognizer.isRecognitionAvailable(ctx)) SpeechRecognizer.createSpeechRecognizer(ctx)
        else if (Build.VERSION.SDK_INT >= 31) SpeechRecognizer.createOnDeviceSpeechRecognizer(ctx) else null
        r?.setRecognitionListener(listener)
        r
    }.onFailure { Log.w(TAG, "no recogniser: ${it.message}") }.getOrNull()

    private fun listen() {
        val r = sr ?: return
        if (!active || listening) return
        val i = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH)
            .putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
            .putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, true)
            .putExtra(RecognizerIntent.EXTRA_MAX_RESULTS, 3)
        if (Build.VERSION.SDK_INT >= 33) i.putStringArrayListExtra(RecognizerIntent.EXTRA_BIASING_STRINGS, ArrayList(vocabulary.take(100)))
        listening = true
        runCatching { r.startListening(i) }.onFailure { listening = false; again(600) }
    }

    private fun again(ms: Long) { main.postDelayed({ listen() }, ms) }

    private val listener = object : RecognitionListener {
        override fun onReadyForSpeech(params: Bundle?) { emit(ListenEvent.Ready) }
        override fun onBeginningOfSpeech() { emit(ListenEvent.Speech) }
        override fun onRmsChanged(rmsdB: Float) { onLevel(((rmsdB + 2f) / 12f).coerceIn(0f, 1f)) }
        override fun onBufferReceived(buffer: ByteArray?) {}
        override fun onEndOfSpeech() {}
        override fun onPartialResults(partial: Bundle?) {
            partial?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull()?.takeIf { it.isNotBlank() }?.let { emit(ListenEvent.Interim(it)) }
        }
        override fun onResults(results: Bundle?) {
            listening = false
            results?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull()?.takeIf { it.isNotBlank() }?.let { emit(ListenEvent.Utterance(it)) }
            again(150)
        }
        override fun onError(error: Int) {
            listening = false
            // No match and speech timeout are just silence; 11 (server
            // disconnected) clears up on a second try. Anything else: wait a bit.
            when (error) {
                SpeechRecognizer.ERROR_NO_MATCH, SpeechRecognizer.ERROR_SPEECH_TIMEOUT -> again(150)
                SpeechRecognizer.ERROR_RECOGNIZER_BUSY -> again(500)
                SpeechRecognizer.ERROR_INSUFFICIENT_PERMISSIONS -> { active = false; emit(ListenEvent.Failed("the microphone is not allowed")) }
                else -> again(800)
            }
        }
        override fun onEvent(eventType: Int, params: Bundle?) {}
    }

    companion object { private const val TAG = "CrundiFallbackSR" }
}
