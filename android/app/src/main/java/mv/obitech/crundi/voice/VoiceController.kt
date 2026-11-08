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
    /** The server's capabilities (empty: not known), for what the screen offers. */
    val caps: Set<String> = emptySet(),
    /** Keyboard mode: no microphone, a text field; Cru's replies still said and shown. */
    val keyboard: Boolean = false,
    /** The chat view: the conversation as messages, in place of the field. */
    val chat: Boolean = false,
    /** The chat view's messages, oldest first, and whether older ones can be loaded. */
    val history: List<CruMsg> = emptyList(),
    val moreHistory: Boolean = false,
    val historyLoaded: Boolean = false,
    /** Why the history could not be loaded, or "". */
    val historyError: String = "",
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
    /** Keyboard mode (no microphone); [ownerChose]: he switched it himself, so the server's settings don't switch it back. */
    private var keyboard = false
    private var ownerChose = false
    /** Keyboard mode's line (capability "voice-keyboard"): Cru's reports and settings changes, no audio. */
    @Volatile private var kbSocket: DataSocket? = null
    /** Each stream opened is numbered: one closed on purpose (a mode switch) is not "lost". */
    @Volatile private var streamGen = 0
    @Volatile private var kbGen = 0

    init { _ui.update { it.copy(chat = VoicePrefs.chatView(ctx)) } }

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
        // The chat view and its messages, and the last settings, stay from one session to the next.
        val was = _ui.value
        _ui.value = VoiceUi(chat = was.chat, history = was.history, moreHistory = was.moreHistory, historyLoaded = was.historyLoaded, spend = was.spend)
        if (!VoicePrefs.enabled(ctx)) { block("The voice assistant is turned off. Turn it on in App settings, Voice assistant."); return }
        val server = Servers.current(ctx) ?: run { block("Pair this phone with a Crundi server first."); return }
        val caps = ServerCaps.get(ctx, server.id)
        _ui.update { it.copy(server = server.host, caps = caps) }
        // Known capabilities without the assistant: say so before trying. Unknown (never
        // handshaken since an update): try, and a 404 says the same.
        if (caps.isNotEmpty() && VoiceApi.CAPABILITY !in caps && "handshake" in caps) { block(VoiceApi.UNSUPPORTED); return }
        // Voice or keyboard: keyboard when voice can't work here, else the owner's last choice.
        if (!ownerChose) keyboard = CruMode.startInKeyboard(VoicePrefs.keyboard(ctx), voiceUnavailable(VoicePrefs.deepgramKeySet(ctx), VoicePrefs.deepgramLimitReached(ctx)))
        _ui.update { it.copy(keyboard = keyboard) }
        if (!keyboard && !micGranted()) {
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
        // A chat in voice mode may ask for the microphone ("Switch to this chat?"): this is how it closes the session.
        HeyCrundi.sessionCloser { close() }
        dispatch(VoiceEvent.Opened(isLocked(), now(), cancellable = VoiceApi.CANCEL in caps))
        scope.launch { voice.warm() }
        scope.launch { checkServer(voice, server.host) }
        if (keyboard) startKeyboard(voice) else startVoice(voice)
        tick = scope.launch { while (isActive) { delay(1000); dispatch(VoiceEvent.Tick(now())) } }
        if (_ui.value.chat && !_ui.value.historyLoaded) loadHistory(false)
        // Cru's unheard reports: seen once the chat view is up, else counted for the chat button's dot.
        if (VoiceApi.UNREAD in caps) {
            if (_ui.value.chat) markSeen()
            else scope.launch { runCatching { voice.unread() }.getOrNull()?.let { CruUnread.set(it.count) } }
        }
    }

    /** The chat view is open: Cru's unheard reports are read now. */
    private fun markSeen() {
        if (VoiceApi.UNREAD !in _ui.value.caps) return
        val v = voiceApi() ?: return
        CruUnread.set(0)
        scope.launch { v.seen() }
    }

    private fun micGranted() = ContextCompat.checkSelfPermission(ctx, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED

    private fun voiceUnavailable(deepgramKeySet: Boolean?, limitReached: Boolean) =
        CruMode.voiceUnavailable(micGranted(), deepgramKeySet, limitReached, recogniserAvailable())

    private fun recogniserAvailable() = runCatching {
        android.speech.SpeechRecognizer.isRecognitionAvailable(ctx) ||
            (android.os.Build.VERSION.SDK_INT >= 31 && android.speech.SpeechRecognizer.isOnDeviceRecognitionAvailable(ctx))
    }.getOrDefault(false)

    /** The microphone to the server (or the phone's recogniser, once the stream was lost). */
    private fun startVoice(voice: VoiceApi) {
        VoiceMicService.start(ctx)
        if (_ui.value.state.offline) { useFallback(); return }
        scope.launch(Dispatchers.IO) { openStream(voice) }
        startMic()
    }

    /** Keyboard mode: nothing to wait for, and the line for Cru's reports when the server has one. */
    private fun startKeyboard(voice: VoiceApi) {
        if (_ui.value.state.phase == Phase.Starting) dispatch(VoiceEvent.Ready(now(), false))
        if (VoiceApi.KEYBOARD in _ui.value.caps) scope.launch(Dispatchers.IO) { openKeyboardLine(voice) }
    }

    /**
     * The keyboard button: switch between voice and keyboard. Remembered on
     * this phone. Asleep, it wakes; voice without the microphone permission
     * asks for it.
     */
    fun setKeyboard(on: Boolean) {
        ownerChose = true
        VoicePrefs.setKeyboard(ctx, on)
        switchMode(on)
    }

    private fun switchMode(on: Boolean) {
        if (on == keyboard && running) return
        keyboard = on
        _ui.update { it.copy(keyboard = on, needsMic = false) }
        val v = api
        if (!running || v == null) {
            // Asleep, or stopped for the microphone: start again in the mode chosen.
            if (_ui.value.blocked.isEmpty()) { stopAll(); open() }
            return
        }
        if (on) {
            stopVoice()
            startKeyboard(v)
        } else {
            stopKeyboardLine()
            // No microphone permission: stopped, asking for it (allowed, open() starts again in voice).
            if (!micGranted()) { stopAll(); _ui.update { it.copy(needsMic = true) }; return }
            startVoice(v)
        }
    }

    /** Typed words (keyboard mode): asked as voice turns are. Asleep, it wakes first. */
    fun send(text: String) {
        val t = text.trim()
        if (t.isEmpty()) return
        if (!running || _ui.value.state.phase == Phase.Ended) {
            if (_ui.value.blocked.isNotEmpty()) return
            stopAll(); open()
        }
        typedNext = true
        dispatch(VoiceEvent.Typed(t, now()))
    }

    /** The owner is typing: that counts as activity, so the session does not wind down under him. */
    fun typing() { if (running) dispatch(VoiceEvent.Active(now())) }

    /** The chat view on or off, remembered on this phone. */
    fun setChat(on: Boolean) {
        VoicePrefs.setChatView(ctx, on)
        _ui.update { it.copy(chat = on) }
        if (on && !_ui.value.historyLoaded) loadHistory(false)
        if (on) markSeen()
    }

    /** "Earlier messages". */
    fun moreHistory() = loadHistory(true)

    private fun voiceApi(): VoiceApi? = api ?: Servers.current(ctx)?.let { AppGraph.forServer(ctx, it).api.voice }

    private var historyJob: Job? = null
    private fun loadHistory(older: Boolean) {
        if (VoiceApi.HISTORY !in _ui.value.caps) return
        val v = voiceApi() ?: return
        if (historyJob?.isActive == true) return
        val before = if (older) CruChat.before(_ui.value.history) else 0L
        historyJob = scope.launch {
            try {
                val r = v.history(40, before)
                if (r == null) { _ui.update { it.copy(historyLoaded = true, historyError = "Not supported by this server.") }; return@launch }
                _ui.update { it.copy(history = CruChat.merge(it.history, CruChat.fromTurns(r.turns), older), moreHistory = r.more, historyLoaded = true, historyError = "") }
            } catch (e: Exception) {
                _ui.update { it.copy(historyError = e.message ?: "Could not load the conversation") }
            }
        }
    }

    private fun chatAdd(said: String, speak: String, show: String) =
        _ui.update { it.copy(history = CruChat.add(it.history, said, speak, show, now())) }

    /** The Compact button: the conversation so far becomes a summary, the notebook kept. */
    fun compact() {
        val v = voiceApi() ?: return
        note("Compacting the conversation…")
        scope.launch {
            val msg = try {
                val r = v.compact()
                when {
                    r == null -> "Compacting is not supported by this server."
                    r.ok -> "Compacted: Cru carries on from a summary, its notebook kept."
                    else -> "Not compacted: " + r.error.ifEmpty { r.skipped }.ifEmpty { "the server could not" }
                }
            } catch (e: Exception) { "Not compacted: " + (e.message ?: "the server could not be reached") }
            note(msg)
        }
    }

    private fun note(text: String) = _ui.update { it.copy(state = it.state.copy(notice = text)) }

    /** The big button: stop Crundi talking, or wake again once asleep. */
    fun tapMic() {
        when (_ui.value.state.phase) {
            Phase.Speaking -> bargeIn()
            Phase.Ended -> { stopAll(); open() }
            else -> {}
        }
    }

    /** Yes or No tapped on a question (or anything typed): sent as if said. */
    fun answer(text: String) { typedNext = true; dispatch(VoiceEvent.Typed(text, now())) }

    /** The next question asked was typed or tapped, not said: the server's voice check does not apply to it. */
    private var typedNext = false

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
        val was = _ui.value.state.phase
        val (s, effects) = machine.on(_ui.value.state, e)
        _ui.update { it.copy(state = s) }
        // Cru has woken and listens: a soft rising tone.
        if (was == Phase.Starting && s.phase == Phase.Listening) speaker.tone(Tones.of(Tones.LISTEN))
        // The answer came (or the turn was taken back): no more thinking tones.
        if (was == Phase.Thinking && s.phase != Phase.Thinking) thinking?.cancel()
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
            // A soft "sent", then a calm tone while Cru thinks (no spoken "One moment, Sir").
            VoiceEffect.Cue -> thinkingTones()
            VoiceEffect.StopCue -> { thinking?.cancel(); speaker.stopTone() }
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
                    if (!keyboard) fallback?.start()
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
        // Typed (keyboard mode, Yes / No): said so, or once a voice is enrolled it would lose the owner's powers.
        val typed = keyboard || typedNext
        typedNext = false
        scope.launch {
            val r = try { voice.turn(text, where, locked, typed = typed) } catch (e: ApiException) {
                VoiceReply(ok = false, error = when {
                    e.offline -> "I can't reach the server, Sir."
                    VoiceApi.unsupported(e) -> VoiceApi.UNSUPPORTED
                    else -> e.message
                })
            } catch (e: Exception) { VoiceReply(ok = false, error = e.message ?: "Something went wrong") }
            // Cru may have just turned itself up or down: the reply is said at the new volume.
            r.volume?.let { speaker.volume = mv.obitech.crundi.api.volumeGain(it) }
            // The chat view: what was asked and the answer, when it is this turn's.
            val current = _ui.value.state.let { it.phase == Phase.Thinking && it.turn == turn }
            if (current && r.ok && !r.cancelled) chatAdd(text, r.speak, r.show)
            dispatch(VoiceEvent.Reply(r.ok, r.speak, r.show, r.error, now(), r.pending.firstOrNull()?.question.orEmpty(), r.needsUnlock, r.close, turn = turn, cancelled = r.cancelled, acted = r.acted.map { it.what }.filter { it.isNotBlank() }))
            // The voice was not the owner's: Cru answered, but changed nothing.
            if (r.ok && !r.cancelled && r.speaker?.unrecognised == true && current) note(NOT_RECOGNISED)
            // After the reply, so a note about it (untrained) isn't cleared by the reply's own.
            if (r.ok && !r.cancelled) phoneActions(r.phoneActions)
            // The spend moved: refresh the arcs.
            // (and the voice: Cru may just have changed its own speed or voice)
            refreshSettings(voice)
        }
    }

    /** The settings again (after a turn, or the server said one changed): the voice, its speed and volume, the spend. */
    private suspend fun refreshSettings(voice: VoiceApi) {
        runCatching { voice.settings() }.getOrNull()?.let { s ->
            applySettings(s)
        }
    }

    private fun applySettings(s: mv.obitech.crundi.api.VoiceSettings) {
        speaker.rate = s.speechRate.toFloat()
        speaker.deepgramVoice = s.ttsVoice
        speaker.volume = mv.obitech.crundi.api.volumeGain(s.volume)
        VoicePrefs.setDeepgram(ctx, s.deepgramKeySet, s.deepgramLimitReached)
        _ui.update { it.copy(spend = s) }
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

    // ─── the tones (in place of the spoken "one moment, Sir" cues) ───

    private var thinking: Job? = null

    /** "Sent" now; then, while this turn is still being thought about, the think tone after 1.4 s and every 4 s. */
    private fun thinkingTones() {
        speaker.tone(Tones.of(Tones.SENT))
        thinking?.cancel()
        val turn = _ui.value.state.turn
        thinking = scope.launch {
            delay(Tones.THINK_FIRST_MS)
            while (isActive && running && _ui.value.state.let { it.phase == Phase.Thinking && it.turn == turn }) {
                speaker.tone(Tones.of(Tones.THINK))
                delay(Tones.THINK_EVERY_MS)
            }
        }
    }

    // ─── listening ───

    private suspend fun checkServer(voice: VoiceApi, host: String) {
        try {
            val s = voice.settings()
            applySettings(s)
            // No Deepgram (or past its limit with no recogniser here): keyboard, unless he chose voice himself.
            if (running && !keyboard && !ownerChose && voiceUnavailable(s.deepgramKeySet, s.deepgramLimitReached)) switchMode(true)
            // Claude Code answers on the owner's subscription: no API key needed then.
            if (!s.anthropicKeySet && s.brain != "claude-code") block("There is no Claude API key on $host yet. Add one in Settings > Voice assistant, under Keys.")
        } catch (e: ApiException) {
            if (VoiceApi.unsupported(e)) block(VoiceApi.UNSUPPORTED)
            // Offline or signed out: the stream says so in its own way.
        }
    }

    private fun openStream(voice: VoiceApi) {
        val gen = streamGen
        try {
            val s = voice.listen(object : SocketListener {
                override fun onMessage(text: String) {
                    val ev = ListenEvent.parse(text) ?: return
                    scope.launch { if (gen == streamGen) heard(ev) }
                }
                override fun onClosed(code: Int, reason: String) {
                    scope.launch {
                        if (!running || gen != streamGen) return@launch
                        lost(when (code) { 404 -> "this server has no live speech"; 401 -> "signed out"; else -> reason.ifEmpty { "the stream closed" } })
                    }
                }
            })
            scope.launch { if (running && gen == streamGen) socket = s else runCatching { s.close() } }
        } catch (e: ApiException) {
            scope.launch { if (gen == streamGen) lost(e.message) }
        }
    }

    /** Keyboard mode's line: reports and settings changes only. Losing it loses only those. */
    private fun openKeyboardLine(voice: VoiceApi) {
        val gen = kbGen
        try {
            val s = voice.listenKeyboard(object : SocketListener {
                override fun onMessage(text: String) {
                    val ev = ListenEvent.parse(text) ?: return
                    if (ev is ListenEvent.Report || ev is ListenEvent.Settings) scope.launch { if (gen == kbGen && running) heard(ev) }
                }
                override fun onClosed(code: Int, reason: String) {
                    scope.launch { if (gen == kbGen) kbSocket = null }
                }
            })
            scope.launch {
                if (running && keyboard && gen == kbGen) kbSocket = s
                else runCatching { s.send("""{"type":"stop"}"""); s.close() }
            }
        } catch (e: ApiException) {
            Log.i(TAG, "keyboard line: ${e.message}")
        }
    }

    private fun stopKeyboardLine() {
        kbGen++
        kbSocket?.let { s -> runCatching { s.send("""{"type":"stop"}"""); s.close() } }
        kbSocket = null
    }

    private fun heard(ev: ListenEvent) {
        if (!running) return
        when (ev) {
            is ListenEvent.Ready -> dispatch(VoiceEvent.Ready(now(), ev.echoFilter && fallback == null))
            ListenEvent.Speech -> {}
            is ListenEvent.Interim -> dispatch(VoiceEvent.Interim(ev.text, now()))
            is ListenEvent.Utterance -> {
                dispatch(VoiceEvent.Utterance(ev.text, now()))
                if (ev.owner == false && _ui.value.state.phase == Phase.Thinking) note(NOT_RECOGNISED)
            }
            is ListenEvent.Failed -> if (fallback != null) block("No speech recognition: ${ev.error}.") else lost(ev.error)
            is ListenEvent.Report -> { chatAdd("", ev.speak, ev.show); dispatch(VoiceEvent.Report(ev.speak, ev.show, now())) }
            is ListenEvent.Discard -> dispatch(VoiceEvent.Discard(now()))
            // A setting changed on the server (Cru's volume, say): picked up at once.
            ListenEvent.Settings -> api?.let { v -> scope.launch { refreshSettings(v) } }
        }
    }

    private fun lost(why: String) {
        socket?.let { runCatching { it.close() } }
        socket = null
        dispatch(VoiceEvent.StreamLost(why.trimEnd('.'), now()))
    }

    private fun useFallback() {
        if (keyboard) return
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
        thinking?.cancel(); speaker.stopTone()
        stopVoice()
        stopKeyboardLine()
        if (holdsMic) { holdsMic = false; HeyCrundi.sessionCloser(null); HeyCrundi.sessionClosed() }
    }

    /** The microphone, its stream and the phone's recogniser stop (closed, or keyboard mode). */
    private fun stopVoice() {
        streamGen++
        stopMic()
        callAudio.stop(); speaker.route = CruAudio.Route.Media; speaker.sessionId = 0
        socket?.let { s -> runCatching { s.send("""{"type":"stop"}"""); s.close() } }
        socket = null
        fallback?.pause()
        synchronized(preroll) { preroll.clear() }
        held.clear()
        streaming = true
        VoiceMicService.stop(ctx)
    }

    private fun block(why: String) {
        Log.i(TAG, why)
        stopAll()
        _ui.update { it.copy(blocked = why, state = it.state.copy(phase = Phase.Ended)) }
    }

    private fun now() = System.currentTimeMillis()

    companion object {
        private const val TAG = "CrundiVoice"
        private const val NOT_RECOGNISED = "Not recognised: Cru answers but won't act."
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
