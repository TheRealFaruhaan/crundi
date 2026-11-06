package mv.obitech.crundi.voice

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.util.Log
import androidx.core.content.ContextCompat
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
import mv.obitech.crundi.AppGraph
import mv.obitech.crundi.ServerCaps
import mv.obitech.crundi.Servers
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.DataSocket
import mv.obitech.crundi.api.ListenEvent
import mv.obitech.crundi.api.SocketListener
import mv.obitech.crundi.api.VoiceApi
import mv.obitech.crundi.api.VoiceReply

/** What the voice screen shows. */
data class VoiceUi(
    val state: VoiceState = VoiceState(),
    /** Why the assistant can't run at all (turned off, no server, a server without it, no key). */
    val blocked: String = "",
    /** The microphone permission is missing: the screen offers to ask. */
    val needsMic: Boolean = false,
    /** The server's host, for "Crundi on dev.example.com". */
    val server: String = "",
    /** The voice settings and this month's spend, for the screen's usage arcs. */
    val spend: mv.obitech.crundi.api.VoiceSettings? = null,
)

/**
 * One voice session: the side-key overlay (CrundiSession) or the Cru
 * screen (VoiceActivity). Runs [VoiceMachine]'s effects:
 *
 *   microphone ──PCM──▶ /api/voice/listen (Deepgram, through the server)
 *        └─ utterance ──▶ /api/voice/turn (Claude) ──▶ Speaker
 *
 * and calls /api/voice/warm the moment it opens, so Claude's cache is warm
 * by the time the owner has finished the sentence. Main thread only, except the
 * microphone callback.
 */
class VoiceController(
    private val ctx: Context,
    /** Where the owner is, for Claude ("side key", "Cru"). */
    private val source: String,
    private val isLocked: () -> Boolean,
    /** The session is over (quiet for a while, or closed): the host hides itself. */
    private val onFinish: () -> Unit,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val machine = VoiceMachine(silenceMs = VoicePrefs.silenceSeconds(ctx) * 1000L)
    private val speaker = Speaker(ctx, scope)
    private val barge = BargeInDetector()

    private val _ui = MutableStateFlow(VoiceUi())
    val ui: StateFlow<VoiceUi> = _ui
    private val _mic = MutableStateFlow(0f)
    /** The owner\'s voice, 0..1, while listening. */
    val micLevel: StateFlow<Float> = _mic
    /** Crundi's voice, 0..1, while it speaks. */
    val voiceLevel: StateFlow<Float> = speaker.level

    private var api: VoiceApi? = null

    /** The app's live picture of the server (projects, their state, usage) for the field; null without a paired server. */
    val shell: StateFlow<mv.obitech.crundi.ui.shell.ShellData>? = Servers.current(ctx)?.let { AppGraph.forServer(ctx, it).shell.state }
    @Volatile private var socket: DataSocket? = null
    private var mic: MicStream? = null
    private var fallback: FallbackRecognizer? = null
    private var tick: Job? = null
    private var running = false
    /** This session paused the wake word; it lets it listen again when it ends. */
    private var holdsMic = false

    /** Audio goes to the server (not while Crundi speaks). Read on the recording thread. */
    @Volatile private var streaming = true
    @Volatile private var speakingNow = false
    /** The last half second of microphone while paused: The owner\'s first word when they talk over Crundi. */
    private val preroll = ArrayDeque<ByteArray>()
    private var sendPreroll = false

    fun open() {
        if (running) return
        _ui.value = VoiceUi()
        if (!VoicePrefs.enabled(ctx)) { block("The voice assistant is turned off. Turn it on in App settings, Voice assistant."); return }
        val server = Servers.current(ctx) ?: run { block("Pair this phone with a Crundi server first."); return }
        _ui.update { it.copy(server = server.host) }
        val caps = ServerCaps.get(ctx, server.id)
        // Known capabilities without the assistant: say so before trying. Unknown (never
        // handshaken since an update): try, and a 404 says the same.
        if (caps.isNotEmpty() && VoiceApi.CAPABILITY !in caps && "handshake" in caps) { block(VoiceApi.UNSUPPORTED); return }
        if (ContextCompat.checkSelfPermission(ctx, Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
            _ui.update { it.copy(needsMic = true) }
            return
        }
        val voice = AppGraph.forServer(ctx, server).api.voice
        api = voice
        speaker.api = voice
        speaker.streams = VoiceApi.TTS_STREAM in caps
        running = true
        streaming = true
        // One microphone owner: "Hey Cru" lets go of it before the session takes it.
        holdsMic = true
        HeyCrundi.sessionOpened()
        dispatch(VoiceEvent.Opened(isLocked(), now()))
        VoiceMicService.start(ctx)
        scope.launch { voice.warm() }
        scope.launch { checkServer(voice, server.host) }
        scope.launch(Dispatchers.IO) { openStream(voice) }
        startMic()
        tick = scope.launch { while (isActive) { delay(1000); dispatch(VoiceEvent.Tick(now())) } }
    }

    /** The big button: stop Crundi talking, or wake again once asleep. */
    fun tapMic() {
        when (_ui.value.state.phase) {
            Phase.Speaking -> bargeIn()
            Phase.Ended -> { stopAll(); open() }
            else -> {}
        }
    }

    /** Yes or No tapped on a question (or anything typed): sent as if said. */
    fun answer(text: String) = dispatch(VoiceEvent.Typed(text, now()))

    /** The phone has been unlocked since the session opened. */
    fun unlocked() = dispatch(VoiceEvent.Unlocked(now()))

    /** Close now (the X, the system hid the overlay). */
    fun close() {
        if (_ui.value.state.phase != Phase.Ended) dispatch(VoiceEvent.Close) else stopAll()
    }

    /** The host is going away for good. */
    fun release() {
        stopAll()
        speaker.release()
        fallback?.release(); fallback = null
        scope.cancel()
    }

    // ─── the machine ───

    private fun dispatch(e: VoiceEvent) {
        val (s, effects) = machine.on(_ui.value.state, e)
        _ui.update { it.copy(state = s) }
        for (fx in effects) run(fx)
    }

    private fun run(fx: VoiceEffect) {
        when (fx) {
            is VoiceEffect.Ask -> ask(fx.text)
            is VoiceEffect.Say -> {
                speakingNow = true
                barge.reset()
                speaker.say(fx.text) { speakingNow = false; dispatch(VoiceEvent.SpeechDone(now())) }
            }
            VoiceEffect.StopSaying -> { speakingNow = false; speaker.stop() }
            VoiceEffect.PauseStream -> {
                streaming = false
                synchronized(preroll) { preroll.clear() }
                socket?.send("""{"type":"pause"}""")
                fallback?.pause()
            }
            VoiceEffect.ResumeStream -> {
                val bargedIn = sendPreroll
                sendPreroll = false
                val resume = {
                    val s = socket
                    if (s != null) {
                        s.send("""{"type":"resume"}""")
                        // Talked over: The owner\'s first words are in the last moment held back.
                        if (bargedIn) synchronized(preroll) { for (b in preroll.takeLast(3)) s.send(b) }
                    }
                    synchronized(preroll) { preroll.clear() }
                    streaming = true
                    fallback?.start()
                }
                // After an answer, wait for the speaker and the room to go quiet,
                // or the end of Crundi's own voice is heard as the owner's words.
                if (bargedIn) resume() else scope.launch {
                    delay(RESUME_AFTER_SPEECH_MS)
                    val p = _ui.value.state.phase
                    if (running && (p == Phase.Listening || p == Phase.Thinking)) resume()
                }
            }
            VoiceEffect.UseFallback -> useFallback()
            VoiceEffect.Finish -> { stopAll(); onFinish() }
        }
    }

    private fun ask(text: String) {
        val voice = api ?: return
        val locked = _ui.value.state.locked
        val where = if (locked) "$source, phone locked" else source
        scope.launch {
            val r = try { voice.turn(text, where, locked) } catch (e: ApiException) {
                VoiceReply(ok = false, error = when {
                    e.offline -> "I can't reach the server, Sir."
                    VoiceApi.unsupported(e) -> VoiceApi.UNSUPPORTED
                    else -> e.message
                })
            } catch (e: Exception) { VoiceReply(ok = false, error = e.message ?: "Something went wrong") }
            dispatch(VoiceEvent.Reply(r.ok, r.speak, r.show, r.error, now(), r.pending.firstOrNull()?.question.orEmpty(), r.needsUnlock))
            // After the reply, so a note about it (untrained) isn't cleared by the reply's own.
            if (r.ok) phoneActions(r.phoneActions)
            // The spend moved: refresh the arcs.
            runCatching { voice.settings() }.getOrNull()?.let { s -> _ui.update { it.copy(spend = s) } }
        }
    }

    /**
     * What the server asked the phone to do (capability "voice-phone-actions"),
     * already confirmed by the owner (D32). The session is on screen, so Crundi is
     * in front and may start the microphone service (Android 14's rule); the
     * wake word itself waits until this session lets go of the microphone.
     */
    private fun phoneActions(actions: List<mv.obitech.crundi.api.PhoneAction>) {
        for (a in actions) when (val c = HeyCrundiPlan.command(a.type, a.minutes)) {
            is HeyCrundiPlan.Command.Start ->
                if (WakeProfiles.trained(ctx)) HeyCrundiService.start(ctx, c.minutes)
                else _ui.update { it.copy(state = it.state.copy(notice = "Hey Cru isn't trained yet: train it in Voice assistant settings first.")) }
            HeyCrundiPlan.Command.Stop -> HeyCrundiService.stop(ctx)
            null -> Log.i(TAG, "phone action not known here: ${a.type}")
        }
    }

    private fun bargeIn() {
        sendPreroll = true
        dispatch(VoiceEvent.BargeIn(now()))
    }

    // ─── listening ───

    private suspend fun checkServer(voice: VoiceApi, host: String) {
        try {
            val s = voice.settings()
            speaker.deepgramVoice = s.ttsVoice
            _ui.update { it.copy(spend = s) }
            if (!s.anthropicKeySet) block("There is no Claude API key on $host yet. Add one in its Voice assistant settings ($host/voice).")
        } catch (e: ApiException) {
            if (VoiceApi.unsupported(e)) block(VoiceApi.UNSUPPORTED)
            // Offline or signed out: the stream says so in its own way.
        }
    }

    private fun openStream(voice: VoiceApi) {
        try {
            val s = voice.listen(object : SocketListener {
                override fun onMessage(text: String) {
                    val ev = ListenEvent.parse(text) ?: return
                    scope.launch { heard(ev) }
                }
                override fun onClosed(code: Int, reason: String) {
                    scope.launch {
                        if (!running) return@launch
                        lost(when (code) { 404 -> "this server has no live speech"; 401 -> "signed out"; else -> reason.ifEmpty { "the stream closed" } })
                    }
                }
            })
            scope.launch { if (running) socket = s else s.close() }
        } catch (e: ApiException) {
            scope.launch { lost(e.message) }
        }
    }

    private fun heard(ev: ListenEvent) {
        if (!running) return
        when (ev) {
            ListenEvent.Ready -> dispatch(VoiceEvent.Ready(now()))
            ListenEvent.Speech -> {}
            is ListenEvent.Interim -> dispatch(VoiceEvent.Interim(ev.text, now()))
            is ListenEvent.Utterance -> dispatch(VoiceEvent.Utterance(ev.text, now()))
            is ListenEvent.Failed -> if (fallback != null) block("No speech recognition: ${ev.error}.") else lost(ev.error)
        }
    }

    private fun lost(why: String) {
        socket?.let { runCatching { it.close() } }
        socket = null
        dispatch(VoiceEvent.StreamLost(why.trimEnd('.'), now()))
    }

    private fun useFallback() {
        stopMic()
        val f = fallback ?: FallbackRecognizer(ctx, VOCABULARY, onLevel = { _mic.value = it }) { ev -> heard(ev) }.also { fallback = it }
        if (!f.available) { block("Live speech recognition is unavailable and this phone has none of its own."); return }
        if (_ui.value.state.phase != Phase.Speaking) f.start()
    }

    private fun startMic() {
        val m = MicStream { bytes, level ->
            _mic.value = level
            if (speakingNow) {
                val voice = if (speaker.measured) speaker.level.value else null
                if (barge.hear(level, voice, 100)) { speakingNow = false; scope.launch { if (_ui.value.state.phase == Phase.Speaking) bargeIn() } }
            } else barge.learn(level)
            val s = socket
            synchronized(preroll) {
                when {
                    // Live: anything held back goes first (said before the socket was up).
                    streaming && s != null -> { for (b in preroll) s.send(b); preroll.clear(); s.send(bytes) }
                    // The socket is still opening: keep it all (up to 10 s).
                    streaming -> { preroll.addLast(bytes); while (preroll.size > 100) preroll.removeFirst() }
                    // Crundi is speaking: keep the last half second, for a barge-in.
                    else -> { preroll.addLast(bytes); while (preroll.size > 5) preroll.removeFirst() }
                }
            }
        }
        if (!m.start()) { block("The microphone could not be opened. Is another app using it?"); return }
        mic = m
    }

    private fun stopMic() { mic?.stop(); mic = null; _mic.value = 0f }

    private fun stopAll() {
        running = false
        tick?.cancel(); tick = null
        speakingNow = false
        speaker.stop()
        stopMic()
        socket?.let { s -> runCatching { s.send("""{"type":"stop"}"""); s.close() } }
        socket = null
        fallback?.pause()
        synchronized(preroll) { preroll.clear() }
        VoiceMicService.stop(ctx)
        if (holdsMic) { holdsMic = false; HeyCrundi.sessionClosed() }
    }

    private fun block(why: String) {
        Log.i(TAG, why)
        stopAll()
        _ui.update { it.copy(blocked = why, state = it.state.copy(phase = Phase.Ended)) }
    }

    private fun now() = System.currentTimeMillis()

    companion object {
        private const val TAG = "CrundiVoice"
        /** Quiet after Crundi finishes before listening again: the speaker's tail and the room's echo. */
        private const val RESUME_AFTER_SPEECH_MS = 450L

        /** Words the phone's own recogniser is hinted with (the server sends Deepgram more). */
        val VOCABULARY = listOf(
            "Crundi", "kanban", "kanban task", "mindmap", "backlog", "to do", "in progress", "done", "subtask",
            "project", "chat", "terminal", "service", "schedule", "usage", "workbench", "panel", "notes",
            "park", "resume", "Claude", "Opus", "Sonnet", "Haiku",
        )
    }
}
