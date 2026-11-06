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
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

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
    /** The owner asked Cru to close: by default the same as [onFinish]. */
    private val onDismiss: () -> Unit = onFinish,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val machine = VoiceMachine(silenceMs = VoicePrefs.silenceSeconds(ctx) * 1000L)
    private val speaker = Speaker(ctx, scope)
    private val barge = BargeInDetector()
    /** Is it the owner over Cru? Decided here while nothing is sent ([OwnerGate]). */
    private val gate = OwnerGate()
    private val ring = VoiceRing()
    @Volatile private var gateFired = false
    /** Cru's voice as speakerphone call audio, so the echo canceller takes it out ([CallAudio]). */
    private val callAudio = CallAudio(ctx)

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
    /** The microphone said before the socket was up: sent first once it is. */
    private val preroll = ArrayDeque<ByteArray>()
    /** The microphone held back while paused (Cru talking): the owner's first words when he talks over it. */
    private val held = PreRoll(PREROLL_MS)
    private var sendPreroll = false
    /** Of the pre-roll, how much to send: from just before the owner's voice rose (not Cru's before it). */
    private var prerollTake = PREROLL_MS

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
        dispatch(VoiceEvent.Opened(isLocked(), now(), cancellable = VoiceApi.CANCEL in caps))
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
            // A live stream (barge-in): the server is told what Crundi says, and drops it.
            is VoiceEffect.Saying -> socket?.send(buildJsonObject { put("type", "saying"); put("text", fx.text) }.toString())
            VoiceEffect.Said -> { sendPreroll = false; socket?.send("""{"type":"said"}""") }
            VoiceEffect.Cue -> playCue()
            VoiceEffect.StopCue -> speaker.stopCue()
            VoiceEffect.StopSaying -> { speakingNow = false; speaker.stop() }
            VoiceEffect.PauseStream -> {
                if (streaming) { gate.start(); gateFired = false }
                streaming = false
                synchronized(preroll) { preroll.clear() }
                held.clear()
                socket?.send("""{"type":"pause"}""")
                fallback?.pause()
            }
            VoiceEffect.ResumeStream -> {
                val bargedIn = sendPreroll
                sendPreroll = false
                val take = prerollTake
                prerollTake = PREROLL_MS
                val resume = {
                    val s = socket
                    if (s != null) {
                        s.send("""{"type":"resume"}""")
                        // Talked over: The owner\'s first words are in the last moment held back.
                        if (bargedIn) for (b in held.drain(take)) s.send(b)
                    }
                    held.clear()
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
            VoiceEffect.Dismiss -> { stopAll(); onDismiss() }
            VoiceEffect.DiscardStream -> socket?.send("""{"type":"discard"}""")
            VoiceEffect.CancelTurn -> api?.let { v ->
                scope.launch {
                    val r = runCatching { v.cancel() }.onFailure { Log.i(TAG, "cancel: ${it.message}") }.getOrNull()
                    if (r != null) dispatch(VoiceEvent.Cancelled(r.acted.map { it.what }.filter { it.isNotBlank() }, now()))
                }
            }
        }
    }

    /** The × by the owner's words: take back what he said (or cancel the turn it started). */
    fun cancelWords() = dispatch(VoiceEvent.CancelTapped(now()))

    private fun ask(text: String) {
        val voice = api ?: return
        val locked = _ui.value.state.locked
        // The machine numbered this turn: its answer is matched to it.
        val turn = _ui.value.state.turn
        val where = if (locked) "$source, phone locked" else source
        scope.launch {
            val r = try { voice.turn(text, where, locked) } catch (e: ApiException) {
                VoiceReply(ok = false, error = when {
                    e.offline -> "I can't reach the server, Sir."
                    VoiceApi.unsupported(e) -> VoiceApi.UNSUPPORTED
                    else -> e.message
                })
            } catch (e: Exception) { VoiceReply(ok = false, error = e.message ?: "Something went wrong") }
            dispatch(VoiceEvent.Reply(r.ok, r.speak, r.show, r.error, now(), r.pending.firstOrNull()?.question.orEmpty(), r.needsUnlock, r.close, turn = turn, cancelled = r.cancelled, acted = r.acted.map { it.what }.filter { it.isNotBlank() }))
            // After the reply, so a note about it (untrained) isn't cleared by the reply's own.
            if (r.ok && !r.cancelled) phoneActions(r.phoneActions)
            // The spend moved: refresh the arcs.
            // (and the voice: Cru may just have changed its own speed or voice)
            runCatching { voice.settings() }.getOrNull()?.let { s ->
                val old = _ui.value.spend
                speaker.rate = s.speechRate.toFloat(); speaker.deepgramVoice = s.ttsVoice; _ui.update { it.copy(spend = s) }
                // Another voice or speed (or Deepgram's limit reached): the cues follow it.
                if (old == null || old.ttsVoice != s.ttsVoice || old.speechRate != s.speechRate || old.deepgramLimitReached != s.deepgramLimitReached) scope.launch { loadCues(voice, s) }
            }
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
        // Tapped: on a live server the pre-roll is Cru's own voice, never sent.
        sendPreroll = !_ui.value.state.live
        prerollTake = OLD_PREROLL_MS
        dispatch(VoiceEvent.BargeIn(now()))
    }

    /** The gate heard the owner over Cru (or the cue): stop it here, and resume the stream from his first words. */
    private fun ownerHeard() {
        if (!running || streaming) return
        sendPreroll = true
        prerollTake = minOf(PREROLL_MS, gate.loudMs + LEAD_MS)
        dispatch(VoiceEvent.VoiceHeard(now()))
    }

    /** An older server: the level detector heard a voice over Crundi's, and that alone cuts in. */
    private fun voiceHeard() {
        if (_ui.value.state.phase != Phase.Speaking) return
        sendPreroll = !_ui.value.state.live
        prerollTake = OLD_PREROLL_MS
        dispatch(VoiceEvent.VoiceHeard(now()))
    }

    // ─── the "one moment, Sir" cue ───

    /** The cues for the voice in use: (phrase, clip on the phone) or (phrase, null) for the phone's own voice. */
    @Volatile private var cues: List<Pair<String, java.io.File?>> = emptyList()
    private var lastCue = -1
    private var cueRetry: Job? = null

    /**
     * The server's cues (capability "voice-cues"), their clips fetched once and
     * kept in the app's cache, so one plays with no network wait. An older
     * server has none: the phone's voice says ours, a Deepgram voice plays none
     * (it would not match).
     */
    private suspend fun loadCues(voice: VoiceApi, s: mv.obitech.crundi.api.VoiceSettings) {
        val deviceVoice = s.ttsVoice.isEmpty() || s.deepgramLimitReached
        val list = try { voice.cues() } catch (e: Exception) { Log.i(TAG, "cues: ${e.message}"); cues = if (deviceVoice) Speech.CUES.map { it to null } else emptyList(); return }
        if (list == null) { cues = if (deviceVoice) Speech.CUES.map { it to null } else emptyList(); return }
        if (list.device) { cues = list.cues.map { it.text to null }; return }
        val safe = { x: String -> x.filter { it.isLetterOrDigit() || it == '-' || it == '.' }.ifEmpty { "_" } }
        val dir = java.io.File(ctx.cacheDir, "cues/${safe(list.voice)}/${safe(list.rate)}")
        val out = ArrayList<Pair<String, java.io.File?>>()
        for (c in list.cues) {
            val url = c.url ?: continue
            val f = java.io.File(dir, "${safe(c.id)}.mp3")
            if (!f.exists()) runCatching {
                val bytes = voice.cueClip(url)
                kotlinx.coroutines.withContext(Dispatchers.IO) {
                    dir.mkdirs()
                    val tmp = java.io.File(dir, "${safe(c.id)}.tmp")
                    tmp.writeBytes(bytes)
                    tmp.renameTo(f)
                }
            }.onFailure { Log.i(TAG, "cue clip: ${it.message}") }
            if (f.exists()) out += c.text to f
        }
        cues = out
        // Being made for the first time: ask again in a while.
        if (list.cues.any { it.url == null } && cueRetry?.isActive != true) cueRetry = scope.launch { delay(20_000); if (running) loadCues(voice, s) }
    }

    private fun playCue() {
        if (speaker.cueing) return
        val list = cues
        val i = Speech.pickCue(list.size, lastCue)
        if (i < 0) return
        lastCue = i
        val (text, file) = list[i]
        if (speaker.cue(file, text) { dispatch(VoiceEvent.CueDone(now())) }) dispatch(VoiceEvent.CueStarted(text, now()))
    }

    // ─── listening ───

    private suspend fun checkServer(voice: VoiceApi, host: String) {
        try {
            val s = voice.settings()
            speaker.deepgramVoice = s.ttsVoice
            speaker.rate = s.speechRate.toFloat()
            _ui.update { it.copy(spend = s) }
            scope.launch { loadCues(voice, s) }
            // Claude Code answers on the owner's subscription: no API key needed then.
            if (!s.anthropicKeySet && s.brain != "claude-code") block("There is no Claude API key on $host yet. Add one in Settings > Voice assistant, under Keys.")
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
            is ListenEvent.Ready -> dispatch(VoiceEvent.Ready(now(), ev.echoFilter && fallback == null))
            ListenEvent.Speech -> {}
            is ListenEvent.Interim -> dispatch(VoiceEvent.Interim(ev.text, now()))
            is ListenEvent.Utterance -> dispatch(VoiceEvent.Utterance(ev.text, now()))
            is ListenEvent.Failed -> if (fallback != null) block("No speech recognition: ${ev.error}.") else lost(ev.error)
            is ListenEvent.Report -> dispatch(VoiceEvent.Report(ev.speak, ev.show, now()))
            is ListenEvent.Discard -> dispatch(VoiceEvent.Discard(now()))
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
            if (!streaming && _ui.value.state.let { it.live || it.cue.isNotEmpty() }) {
                // Cru is talking (or a cue plays): nothing goes out; the phone decides if it is him.
                ring.add(bytes)
                if (!gateFired) {
                    val play = speaker.playLevel
                    if (gate.hear(level, play, 100) { CruWake.voicedShare(ring.snapshot()) }) { gateFired = true; scope.launch { ownerHeard() } }
                }
            } else if (speakingNow) {
                val voice = if (speaker.measured) speaker.level.value else null
                if (barge.hear(level, voice, 100)) {
                    // A live stream keeps listening for his words; a paused one cuts in on this alone.
                    if (_ui.value.state.live) barge.rearm() else speakingNow = false
                    scope.launch { voiceHeard() }
                }
            } else { barge.learn(level); if (!speakingNow && !speaker.cueing) gate.learn(level) }
            val s = socket
            synchronized(preroll) {
                when {
                    // Live: anything held back goes first (said before the socket was up).
                    streaming && s != null -> { for (b in preroll) s.send(b); preroll.clear(); s.send(bytes) }
                    // The socket is still opening: keep it all (up to 10 s).
                    streaming -> { preroll.addLast(bytes); while (preroll.size > 100) preroll.removeFirst() }
                    // Crundi is speaking: keep the last 1.3 s, for when the owner cuts in.
                    else -> held.push(bytes, 100)
                }
            }
        }
        if (!m.start()) { block("The microphone could not be opened. Is another app using it?"); return }
        mic = m
        // Cru's voice joins the microphone's session, as call audio on the speaker when it can.
        speaker.sessionId = m.sessionId
        speaker.route = callAudio.start()
    }

    private fun stopMic() { mic?.stop(); mic = null; _mic.value = 0f }

    private fun stopAll() {
        running = false
        tick?.cancel(); tick = null
        speakingNow = false
        speaker.stop()
        stopMic()
        callAudio.stop(); speaker.route = CruAudio.Route.Media; speaker.sessionId = 0
        socket?.let { s -> runCatching { s.send("""{"type":"stop"}"""); s.close() } }
        socket = null
        fallback?.pause()
        synchronized(preroll) { preroll.clear() }
        held.clear()
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
        /** 100 ms chunks kept while Cru talks: 1.3 s. */
        private const val PREROLL_MS = 1_300L
        /** An older server (no barge-in): the last moment only, as before. */
        private const val OLD_PREROLL_MS = 300L
        /** Sent from this long before the owner's voice rose over Cru's (a soft first syllable). */
        private const val LEAD_MS = 600L

        /** Words the phone's own recogniser is hinted with (the server sends Deepgram more). */
        val VOCABULARY = listOf(
            "Crundi", "kanban", "kanban task", "mindmap", "backlog", "to do", "in progress", "done", "subtask",
            "project", "chat", "terminal", "service", "schedule", "usage", "workbench", "panel", "notes",
            "park", "resume", "Claude", "Opus", "Sonnet", "Haiku",
        )
    }
}
