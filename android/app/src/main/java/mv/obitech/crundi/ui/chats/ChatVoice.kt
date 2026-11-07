package mv.obitech.crundi.ui.chats

import android.content.Context
import android.util.Log
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.DataSocket
import mv.obitech.crundi.api.ListenEvent
import mv.obitech.crundi.api.SocketListener
import mv.obitech.crundi.api.VoiceApi
import mv.obitech.crundi.api.VoiceSettings
import mv.obitech.crundi.api.model.VoiceSay
import mv.obitech.crundi.voice.HeyCrundi
import mv.obitech.crundi.voice.MicStream
import mv.obitech.crundi.voice.Speaker

/**
 * Voice mode in a chat (capability "chat-voice"): the composer's mic.
 *
 *   tap   voice mode on: each thing said is sent as "voice input: …", and
 *         Claude's speak tool (the chat's voice-say events) is said aloud,
 *         until tapped off, the chat is left or the app goes behind another
 *   hold  push to talk: listen while held, send what was said on release
 *
 * The pure rules are here; [ChatVoiceSession] runs them.
 */
object ChatVoice {
    /** What a spoken message starts with, so Claude knows to answer with its speak tool too. */
    const val PREFIX = "voice input: "
    /** Held this long, the press is push to talk; shorter, a tap. */
    const val HOLD_MS = 350L
    /** After a push-to-talk message, Claude's voice-say is still played for this long. */
    const val SAY_WINDOW_MS = 10 * 60_000L
    /** Push to talk let go: how long to wait for the final words. */
    const val STOP_WAIT_MS = 2_500L
    const val LABEL = "Voice input (tap: voice mode, hold: push to talk)"

    /**
     * The mic is shown: a server that has chat voice, the voice assistant set
     * up there (a Deepgram key; [deepgramKeySet] null until known), and the
     * owner's own chat (never a collaborator's).
     */
    fun shown(capabilities: Set<String>, collaborator: Boolean, deepgramKeySet: Boolean?): Boolean =
        supported(capabilities, collaborator) && deepgramKeySet == true

    /** Worth asking the server's voice settings at all. */
    fun supported(capabilities: Set<String>, collaborator: Boolean): Boolean = VoiceApi.CHAT_VOICE in capabilities && !collaborator

    /** The message for what was said, or null when nothing was. */
    fun message(said: String): String? = said.trim().takeIf { it.isNotEmpty() }?.let { PREFIX + it }

    /** Several finished sentences heard while held, as one. */
    fun joined(parts: List<String>): String = parts.map { it.trim() }.filter { it.isNotEmpty() }.joinToString(" ")

    enum class Release { Off, VoiceMode, PushToTalk }

    /** What letting go of the mic does: off if voice mode was on, else push to talk when held long enough, else voice mode on. */
    fun release(wasOn: Boolean, heldMs: Long): Release = when {
        wasOn -> Release.Off
        heldMs >= HOLD_MS -> Release.PushToTalk
        else -> Release.VoiceMode
    }

    /** Claude's voice is played now: in voice mode, or within [SAY_WINDOW_MS] of a push-to-talk message. */
    fun plays(voiceMode: Boolean, lastPushToTalkAt: Long, now: Long): Boolean =
        voiceMode || (lastPushToTalkAt > 0 && now - lastPushToTalkAt in 0..SAY_WINDOW_MS)

    /** The voice-says not seen before, in order. */
    fun fresh(says: List<VoiceSay>, seen: Set<String>): List<VoiceSay> = says.filter { it.id !in seen }
}

/** What the composer shows of voice mode. */
data class ChatVoiceUi(
    val mode: Mode = Mode.Off,
    /** Words so far, still changing. */
    val interim: String = "",
    /** Claude's voice is being played. */
    val speaking: Boolean = false,
    /** Waiting for the server's speech stream. */
    val connecting: Boolean = false,
    /** A short note (no microphone, the stream stopped). */
    val note: String? = null,
) {
    enum class Mode { Off, Pressed, On, Holding, Finishing }
    /** The mic button is lit. */
    val lit get() = mode != Mode.Off
    val listening get() = mode == Mode.Pressed || mode == Mode.On || mode == Mode.Holding
}

/**
 * One chat's microphone and voice: the live stream /api/voice/listen (as the
 * voice screen uses it: raw PCM from the call-mode mic), and Claude's
 * voice-says through [Speaker] (the Deepgram voice, else the phone's own).
 * While Claude speaks, listening pauses, so its voice is not taken as words.
 * One chat at a time has the mic. Main thread only, except the mic callback.
 */
class ChatVoiceSession(ctx: Context, private val voice: VoiceApi, private val send: (String) -> Unit) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val _ui = MutableStateFlow(ChatVoiceUi())
    val ui: StateFlow<ChatVoiceUi> = _ui
    private val speaker = Speaker(ctx, scope).also { it.api = voice }

    private var mic: MicStream? = null
    @Volatile private var socket: DataSocket? = null
    /** Audio goes to the server (not while Claude speaks, nor once let go). */
    @Volatile private var streaming = false
    /** The mic before the socket is up: sent first once it is. */
    private val preroll = ArrayDeque<ByteArray>()
    /** Bumped on every start and stop: an older stream's callbacks are ignored. */
    private var gen = 0
    /** Sentences heard before the press was decided, or while held. */
    private val said = ArrayList<String>()
    private var final: CompletableDeferred<Unit>? = null
    private var pressWasOn = false
    private var pressing = false
    private val queue = ArrayDeque<String>()
    private var watch: Job? = null
    private var noteJob: Job? = null
    private var holdsMic = false

    /** When a push-to-talk message last went (epoch ms), for [ChatVoice.plays]. */
    var lastPushToTalkAt = 0L
        private set

    /** The voice and speed from the server's voice settings; [streams]: capability "voice-tts-stream". */
    fun configure(s: VoiceSettings?, streams: Boolean) {
        speaker.deepgramVoice = if (s?.deepgramLimitReached == true) "" else s?.ttsVoice.orEmpty()
        speaker.rate = (s?.speechRate ?: 1.0).toFloat()
        speaker.streams = streams
    }

    // ─── the button ───

    /** The finger went down on the mic. */
    fun press() {
        pressing = true
        val m = _ui.value.mode
        pressWasOn = m == ChatVoiceUi.Mode.On
        if (m != ChatVoiceUi.Mode.Off) return
        // Talking over Claude: it stops.
        stopSpeaking()
        if (HeyCrundi.sessionOpen) { note("Cru's voice assistant has the microphone. Close it first."); pressing = false; return }
        _ui.update { it.copy(mode = ChatVoiceUi.Mode.Pressed, interim = "", note = null) }
        start()
    }

    /** Still held after [ChatVoice.HOLD_MS]: push to talk. */
    fun hold() {
        if (pressing && _ui.value.mode == ChatVoiceUi.Mode.Pressed) _ui.update { it.copy(mode = ChatVoiceUi.Mode.Holding) }
    }

    /** The finger came up (or slid off) after [heldMs]. */
    fun release(heldMs: Long) {
        if (!pressing) return
        pressing = false
        val m = _ui.value.mode
        if (m == ChatVoiceUi.Mode.Off || m == ChatVoiceUi.Mode.Finishing) return
        when (ChatVoice.release(pressWasOn, heldMs)) {
            ChatVoice.Release.Off -> stop()
            ChatVoice.Release.VoiceMode -> {
                _ui.update { it.copy(mode = ChatVoiceUi.Mode.On) }
                // Said while the press was being decided: it goes now.
                ChatVoice.message(ChatVoice.joined(said))?.let(send)
                said.clear()
            }
            ChatVoice.Release.PushToTalk -> finishPushToTalk()
        }
    }

    /** A tap without the gesture (accessibility, or the microphone just allowed). */
    fun toggle() { press(); release(0) }

    /** Voice mode off: listening and Claude's voice stop. [why] is shown for a moment. */
    fun stop(why: String? = null) {
        teardown()
        stopSpeaking()
        if (why != null) note(why)
    }

    /** The screen is gone for good. */
    fun close() {
        stop()
        speaker.release()
        scope.cancel()
    }

    fun note(text: String) {
        noteJob?.cancel()
        _ui.update { it.copy(note = text) }
        noteJob = scope.launch { delay(5_000); _ui.update { it.copy(note = null) } }
    }

    // ─── Claude's voice ───

    /** Say [text] after anything already queued. */
    fun say(text: String) {
        queue.addLast(text)
        if (!_ui.value.speaking) next()
    }

    private fun next() {
        val t = queue.removeFirstOrNull()
        if (t == null) {
            _ui.update { it.copy(speaking = false) }
            // The speaker's tail and the room's echo, then listening again.
            val g = gen
            scope.launch {
                delay(RESUME_AFTER_SPEECH_MS)
                if (g == gen && _ui.value.mode == ChatVoiceUi.Mode.On && !_ui.value.speaking) {
                    socket?.send("""{"type":"resume"}""")
                    streaming = true
                }
            }
            return
        }
        if (!_ui.value.speaking) {
            _ui.update { it.copy(speaking = true) }
            if (streaming) { streaming = false; socket?.send("""{"type":"pause"}""") }
        }
        speaker.say(t) { next() }
    }

    private fun stopSpeaking() {
        queue.clear()
        speaker.stop()
        if (_ui.value.speaking) {
            _ui.update { it.copy(speaking = false) }
            if (_ui.value.listening) { socket?.send("""{"type":"resume"}"""); streaming = true }
        }
    }

    // ─── listening ───

    private fun start() {
        current?.takeIf { it !== this }?.stop()
        current = this
        val g = ++gen
        said.clear()
        synchronized(preroll) { preroll.clear() }
        streaming = true
        HeyCrundi.chatVoice(true); holdsMic = true
        val m = MicStream { bytes, _ ->
            if (!streaming) return@MicStream
            val s = socket
            synchronized(preroll) {
                if (s != null) { for (b in preroll) s.send(b); preroll.clear(); s.send(bytes) }
                else { preroll.addLast(bytes); while (preroll.size > 100) preroll.removeFirst() }
            }
        }
        if (!m.start()) { stop("The microphone could not be opened. Is another app using it?"); return }
        mic = m
        _ui.update { it.copy(connecting = true) }
        scope.launch(Dispatchers.IO) {
            try {
                val s = voice.listen(object : SocketListener {
                    override fun onMessage(text: String) {
                        val ev = ListenEvent.parse(text) ?: return
                        scope.launch { if (g == gen) heard(ev) }
                    }
                    override fun onClosed(code: Int, reason: String) {
                        scope.launch {
                            if (g != gen) return@launch
                            if (_ui.value.mode == ChatVoiceUi.Mode.Finishing) { final?.complete(Unit); return@launch }
                            if (_ui.value.mode != ChatVoiceUi.Mode.Off) stop("Voice input stopped: " + when (code) { 404 -> "this server has no live speech"; 401 -> "signed out"; else -> reason.ifEmpty { "the stream closed" } }.trimEnd('.') + ".")
                        }
                    }
                })
                scope.launch { if (g == gen) socket = s else runCatching { s.close() } }
            } catch (e: ApiException) {
                scope.launch { if (g == gen) stop("Voice input could not start: " + e.message.trimEnd('.') + ".") }
            } catch (e: Exception) {
                Log.w(TAG, "listen: ${e.message}")
                scope.launch { if (g == gen) stop("Voice input could not start.") }
            }
        }
        // Cru's own voice screen (or the side-key overlay) opened meanwhile: it has the microphone.
        watch = scope.launch {
            while (isActive) { delay(1_000); if (HeyCrundi.sessionOpen) stop("Cru's voice assistant took the microphone.") }
        }
    }

    private fun heard(ev: ListenEvent) {
        val mode = _ui.value.mode
        if (mode == ChatVoiceUi.Mode.Off) return
        when (ev) {
            is ListenEvent.Ready -> _ui.update { it.copy(connecting = false) }
            is ListenEvent.Interim -> if (mode != ChatVoiceUi.Mode.Finishing || ev.text.isNotBlank()) _ui.update { it.copy(interim = ev.text, connecting = false) }
            is ListenEvent.Utterance -> {
                _ui.update { it.copy(interim = "") }
                when (mode) {
                    ChatVoiceUi.Mode.On -> ChatVoice.message(ev.text)?.let(send)
                    ChatVoiceUi.Mode.Finishing -> { said += ev.text; final?.complete(Unit) }
                    else -> said += ev.text
                }
            }
            // "Scratch that": what was heard so far goes nowhere.
            is ListenEvent.Discard -> { said.clear(); _ui.update { it.copy(interim = "") } }
            is ListenEvent.Failed -> if (mode == ChatVoiceUi.Mode.Finishing) final?.complete(Unit) else stop("Voice input stopped: " + ev.error.trimEnd('.') + ".")
            // The voice assistant's own news is not this chat's.
            is ListenEvent.Report, ListenEvent.Speech -> {}
        }
    }

    /** Let go after a hold: the stream is told to stop, its last words come back (up to [ChatVoice.STOP_WAIT_MS]), and all of it goes as one message. */
    private fun finishPushToTalk() {
        _ui.update { it.copy(mode = ChatVoiceUi.Mode.Finishing) }
        mic?.stop(); mic = null
        val g = gen
        val done = CompletableDeferred<Unit>().also { final = it }
        scope.launch {
            withTimeoutOrNull(ChatVoice.STOP_WAIT_MS) {
                // A short hold may end before the stream is up: what was said is still waiting to go.
                while (socket == null) delay(50)
                val s = socket ?: return@withTimeoutOrNull
                synchronized(preroll) { for (b in preroll) s.send(b); preroll.clear() }
                streaming = false
                s.send("""{"type":"stop"}""")
                done.await()
            }
            if (g != gen) return@launch
            val msg = ChatVoice.message(ChatVoice.joined(said))
            teardown()
            if (msg != null) { lastPushToTalkAt = System.currentTimeMillis(); send(msg) }
        }
    }

    private fun teardown() {
        gen++
        streaming = false
        watch?.cancel(); watch = null
        mic?.stop(); mic = null
        socket?.let { s -> runCatching { s.close() } }
        socket = null
        synchronized(preroll) { preroll.clear() }
        said.clear()
        final = null
        if (holdsMic) { holdsMic = false; HeyCrundi.chatVoice(false) }
        if (current === this) current = null
        _ui.update { it.copy(mode = ChatVoiceUi.Mode.Off, interim = "", connecting = false) }
    }

    companion object {
        private const val TAG = "CrundiChatVoice"
        private const val RESUME_AFTER_SPEECH_MS = 450L
        /** The chat whose voice mode has the mic (one at a time, across panes and bubbles). */
        private var current: ChatVoiceSession? = null
    }
}
