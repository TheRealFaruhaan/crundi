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
import mv.obitech.crundi.voice.CallAudio
import mv.obitech.crundi.voice.CruAudio
import mv.obitech.crundi.voice.SpeechCheck
import mv.obitech.crundi.voice.SpeechDetector
import mv.obitech.crundi.voice.MicStream
import mv.obitech.crundi.voice.OwnerGate
import mv.obitech.crundi.voice.PreRoll
import mv.obitech.crundi.voice.Speaker

/**
 * Voice mode in a chat (capability "chat-voice"): the composer's mic.
 *
 *   tap   voice mode on: each thing said is sent as "voice input: …", and
 *         Claude's speak tool (the chat's voice-say events) is said aloud,
 *         until turned off, the chat is left or the app goes behind another
 *   hold  push to talk: listen while held, send what was said on release;
 *         slid onto Cancel and let go there, nothing is sent
 *
 * While it records, the wide Send is a live recording bar with Cancel at the
 * far right, and the mic is a send button: in voice mode a tap sends what has
 * been heard so far at once. Cancel drops the words; with none, it turns
 * voice mode off (as does holding the mic).
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
    /** A reply waiting behind the owner goes once he has been quiet this long. */
    const val QUIET_MS = 1_500L

    // ─── sharing the microphone with Cru ───

    const val ASK_SWITCH = "Cru is listening. Switch to this chat?"
    const val PAUSED_FOR_CRU = "Paused: Cru is listening"
    const val HEY_CRU_PAUSED = "Hey Cru paused while this chat listens"
    const val MOVED = "Voice mode moved to another chat."

    enum class TurnOn { Go, AskCru }

    /** The mic pressed with Cru's voice screen (or the side-key overlay) holding the microphone: ask first. "Hey Cru" alone just lets go. */
    fun turnOn(cruOpen: Boolean): TurnOn = if (cruOpen) TurnOn.AskCru else TurnOn.Go

    enum class Handover { None, Pause, Resume, Off }

    /**
     * Cru opened or closed, or the chat went behind another or came back.
     * Cru opened on purpose takes the microphone: voice mode pauses, and goes
     * on by itself once Cru has closed and the chat is in front again. Anything
     * else lit (a hold) ends; behind another app, voice mode ends.
     */
    fun handover(mode: ChatVoiceUi.Mode, cruOpen: Boolean, foreground: Boolean): Handover = when {
        mode == ChatVoiceUi.Mode.Paused -> if (!cruOpen && foreground) Handover.Resume else Handover.None
        mode == ChatVoiceUi.Mode.On && cruOpen -> Handover.Pause
        mode == ChatVoiceUi.Mode.Off -> Handover.None
        cruOpen || !foreground -> Handover.Off
        else -> Handover.None
    }

    /** Said in the chat when its voice mode made "Hey Cru" let go of the microphone. */
    fun heyCruNote(wakeListening: Boolean): String? = if (wakeListening) HEY_CRU_PAUSED else null

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

    enum class Release { Off, VoiceMode, PushToTalk, Cancel, SendNow }

    /**
     * What letting go of the mic does. Over Cancel: nothing is sent. In voice
     * mode a tap sends what has been heard so far and a hold turns it off;
     * otherwise a hold was push to talk (sent now) and a tap turns voice mode on.
     */
    fun release(wasOn: Boolean, heldMs: Long, overCancel: Boolean = false): Release = when {
        overCancel -> Release.Cancel
        wasOn && heldMs >= HOLD_MS -> Release.Off
        wasOn -> Release.SendNow
        heldMs >= HOLD_MS -> Release.PushToTalk
        else -> Release.VoiceMode
    }

    enum class CancelTap { Discard, Off }

    /** Cancel tapped in voice mode: words being heard are dropped (voice mode stays); with none, voice mode goes off. */
    fun cancelTap(hasWords: Boolean): CancelTap = if (hasWords) CancelTap.Discard else CancelTap.Off

    /** Slid this far from the mic toward Cancel (0..1), the release cancels. */
    const val CANCEL_AT = 0.8f

    /** How far the finger at [fingerX] is from the mic's centre [startX] toward Cancel's centre [cancelX], 0..1. */
    fun cancelProgress(fingerX: Float, startX: Float, cancelX: Float): Float =
        if (cancelX <= startX) 0f else ((fingerX - startX) / (cancelX - startX)).coerceIn(0f, 1f)

    fun overCancel(progress: Float): Boolean = progress >= CANCEL_AT

    /**
     * An utterance after words were already sent (send now) or dropped
     * (Cancel) from the interim: the part of it still new. A server that
     * honours { type: 'discard' } never repeats them; an older one may.
     */
    fun afterEarly(utterance: String, early: String): String {
        val u = utterance.trim()
        val e = early.trim()
        if (e.isEmpty() || !u.startsWith(e, ignoreCase = true)) return u
        return u.substring(e.length).trimStart(' ', ',', '.', ';', ':').trim()
    }

    /** The recording's length: "0:07", "1:05", "12:00". */
    fun elapsed(ms: Long): String { val s = (ms.coerceAtLeast(0) / 1000); return "${s / 60}:${(s % 60).toString().padStart(2, '0')}" }

    /** The recording bar's words: the end of them when they are long. */
    fun tail(words: String, max: Int = 48): String = words.trim().let { if (it.length <= max) it else "…" + it.takeLast(max - 1).trimStart() }

    /** Claude's voice is played now: in voice mode, or within [SAY_WINDOW_MS] of a push-to-talk message. */
    fun plays(voiceMode: Boolean, lastPushToTalkAt: Long, now: Long): Boolean =
        voiceMode || (lastPushToTalkAt > 0 && now - lastPushToTalkAt in 0..SAY_WINDOW_MS)

    /** The voice-says not seen before, in order. */
    fun fresh(says: List<VoiceSay>, seen: Set<String>): List<VoiceSay> = says.filter { it.id !in seen }
}

/**
 * Claude's spoken replies, a sentence at a time, held behind the owner: one
 * that comes while he is talking (a hold, or words still coming) waits until
 * he has finished (his words sent, or [quietMs] with no new ones), and one
 * he talks over is paused, kept where it was (from the start of the sentence
 * it was on) and taken up again by the same rule. Pure: [ChatVoiceSession]
 * runs it.
 */
class ReplyQueue(private val quietMs: Long = ChatVoice.QUIET_MS) {
    private val waiting = ArrayDeque<String>()
    /** The sentence being said now. */
    var playing: String? = null
        private set
    /** Talked over: waiting for the owner to finish. */
    var paused = false
        private set
    private var pausedAt = 0L
    private var wordsAt = 0L
    /** The owner's words went (an utterance, a hold let go) since the pause or the last words. */
    private var doneSince = false

    val isEmpty get() = playing == null && waiting.isEmpty()
    val pending get() = waiting.toList()

    /** A voice-say: its sentences go after anything already there. */
    fun add(text: String) { waiting.addAll(mv.obitech.crundi.voice.Speech.sentences(text)) }

    /** New interim words from the owner. */
    fun words(now: Long) { wordsAt = now; doneSince = false }

    /** The owner's words were sent (or dropped): no need to wait for the quiet. */
    fun ownerDone() { doneSince = true }

    /** He talked over it (or tapped the mic): the sentence goes back to the front, to be said again from its start. */
    fun pause(now: Long) {
        playing?.let { waiting.addFirst(it) }
        playing = null
        if (waiting.isNotEmpty()) { paused = true; pausedAt = now; doneSince = false }
    }

    /** The sentence playing has been said. */
    fun done() { playing = null }

    /** Stopped for good (voice mode off, Cancel). */
    fun clear() { waiting.clear(); playing = null; paused = false }

    /**
     * Whether to wait now: the owner holding the mic ([ownerBusy]), words
     * still coming ([hasWords], new within [quietMs]), or a pause not yet
     * quiet for [quietMs].
     */
    fun waits(ownerBusy: Boolean, hasWords: Boolean, now: Long): Boolean = when {
        ownerBusy -> true
        doneSince -> false
        hasWords && now - wordsAt < quietMs -> true
        paused && now - maxOf(pausedAt, wordsAt) < quietMs -> true
        else -> false
    }

    /** The next sentence to say, now playing; null when there is none or it must wait. */
    fun take(ownerBusy: Boolean, hasWords: Boolean, now: Long): String? {
        if (playing != null || waiting.isEmpty() || waits(ownerBusy, hasWords, now)) return null
        paused = false; doneSince = false
        return waiting.removeFirst().also { playing = it }
    }
}

/**
 * The owner talking over Claude's reply, told apart from Claude's own voice
 * on the phone (the voice screen's [OwnerGate]: louder than the echo the
 * playback makes, for 300 ms, and speech by [SpeechDetector]). While the reply plays nothing goes to
 * the server; the last moment is held ([PreRoll]) so his first words go
 * first once it is him. Thread-safe: fed on the recording thread.
 */
class ReplyBargeIn(private val gate: OwnerGate = OwnerGate(), private val held: PreRoll = PreRoll(PREROLL_MS)) {
    private var fired = false

    /** The reply started playing: count afresh. */
    @Synchronized fun begin() { gate.start(); held.clear(); fired = false }

    /** Nothing playing: how loud the room is. */
    @Synchronized fun idle(level: Float) = gate.learn(level)

    /**
     * One chunk ([ms] long) while it plays: [play] its level (null unknown),
     * [speech] whether it is someone talking. Non-null once it is him:
     * the chunks to send first, from just before his voice rose.
     */
    @Synchronized fun hear(chunk: ByteArray, level: Float, play: Float?, ms: Long, speech: () -> Boolean): List<ByteArray>? {
        held.push(chunk, ms)
        if (fired || !gate.hear(level, play, ms, speech)) return null
        fired = true
        return held.drain(minOf(PREROLL_MS, gate.loudMs + LEAD_MS))
    }

    companion object {
        const val PREROLL_MS = 1_300L
        /** From this long before his voice rose (a soft first syllable). */
        const val LEAD_MS = 600L
    }
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
    /** When this recording began (epoch ms): the bar's clock. */
    val since: Long = 0L,
    /** While held: how far the finger has slid toward Cancel, 0..1 ([ChatVoice.overCancel] past [ChatVoice.CANCEL_AT]). */
    val cancelProgress: Float = 0f,
    /** Sentences already heard in this recording (a hold), not sent yet. */
    val held: String = "",
    /** Asking "Cru is listening. Switch to this chat?" */
    val askSwitch: Boolean = false,
) {
    /** [Paused]: Cru took the microphone; voice mode goes on by itself once it is done. */
    enum class Mode { Off, Pressed, On, Holding, Finishing, Paused }
    /** Everything heard in this recording so far: the finished sentences and the one in progress. */
    val transcript get() = ChatVoice.joined(listOf(held, interim))
    /** The mic button is lit. */
    val lit get() = mode != Mode.Off
    val listening get() = mode == Mode.Pressed || mode == Mode.On || mode == Mode.Holding
    /** The recording bar takes Send's place, Cancel is at the far right, and the mic is a send button. */
    val recording get() = mode == Mode.Holding || mode == Mode.Finishing || mode == Mode.On
    val hasWords get() = interim.isNotBlank()
    val overCancel get() = ChatVoice.overCancel(cancelProgress)
}

/**
 * One chat's microphone and voice: the live stream /api/voice/listen (as the
 * voice screen uses it: raw PCM from the call-mode mic), and Claude's
 * voice-says through [Speaker] (the Deepgram voice, else the phone's own).
 * While Claude speaks nothing goes to the server, but the phone listens on:
 * when it is the owner talking over it ([ReplyBargeIn]) the reply pauses
 * and his words go; a reply that comes while he talks waits behind him
 * ([ReplyQueue]). In voice mode its voice is speakerphone call audio when it
 * can be ([CallAudio]), so the echo canceller takes most of it out.
 * One chat at a time has the mic. Main thread only, except the mic callback.
 */
class ChatVoiceSession(private val ctx: Context, private val voice: VoiceApi, private val sendOut: (String) -> Unit) {
    /** A voice message goes to the chat: with Cru's soft "sent" tone (no thinking tones in a chat). */
    private val send: (String) -> Unit = { msg -> speaker.tone(mv.obitech.crundi.voice.Tones.of(mv.obitech.crundi.voice.Tones.SENT)); sendOut(msg) }
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val _ui = MutableStateFlow(ChatVoiceUi())
    val ui: StateFlow<ChatVoiceUi> = _ui
    private val speaker = Speaker(ctx, scope).also { it.api = voice }
    private val _levels = MutableStateFlow(List(BARS) { 0f })
    /** The mic's last levels, 0..1, oldest first: the recording bar's bars. */
    val levels: StateFlow<List<Float>> = _levels
    /** Words already sent (send now) or dropped (Cancel) from the interim: not sent again if the utterance repeats them. */
    private var early = ""

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
    /** The screen shows what [said] holds (ChatVoiceUi.held). */
    private fun syncHeld() { val h = ChatVoice.joined(said); if (_ui.value.held != h) _ui.update { it.copy(held = h) } }
    private var final: CompletableDeferred<Unit>? = null
    private var pressWasOn = false
    private var pressing = false
    private var pressPaused = false
    private val replies = ReplyQueue()
    private val barge = ReplyBargeIn()
    /** Is it someone talking, not a noise ([SpeechDetector])? Loaded on the recording thread, at the first chunk. */
    private val speech by lazy { SpeechCheck(SpeechDetector.load(ctx)) }
    /** The reply is playing in voice mode: the mic is kept here, nothing streamed. Read on the recording thread. */
    @Volatile private var pausedForReply = false
    private var pump: Job? = null
    private var callAudio: CallAudio? = null
    private var noteJob: Job? = null
    private var holdsMic = false

    /** When a push-to-talk message last went (epoch ms), for [ChatVoice.plays]. */
    var lastPushToTalkAt = 0L
        private set

    /** The voice and speed from the server's voice settings; [streams]: capability "voice-tts-stream". */
    fun configure(s: VoiceSettings?, streams: Boolean) {
        speaker.deepgramVoice = if (s?.deepgramLimitReached == true) "" else s?.ttsVoice.orEmpty()
        speaker.rate = (s?.speechRate ?: 1.0).toFloat()
        speaker.volume = mv.obitech.crundi.api.volumeGain(s?.volume)
        speaker.streams = streams
    }

    // ─── the button ───

    /** The finger went down on the mic. */
    fun press() {
        pressing = true
        val m = _ui.value.mode
        // Paused for Cru: a tap ends voice mode here.
        if (m == ChatVoiceUi.Mode.Paused) { pressing = false; stop(); return }
        pressWasOn = m == ChatVoiceUi.Mode.On
        // A tap during the reply pauses it (taken up again once he has finished).
        if (m == ChatVoiceUi.Mode.On) { pressPaused = replies.playing != null; if (pressPaused) pauseReply(); return }
        pressPaused = false
        if (m != ChatVoiceUi.Mode.Off) return
        // Talking over Claude: it pauses, and goes on once he has finished.
        if (replies.playing != null) pauseReply()
        if (ChatVoice.turnOn(HeyCrundi.sessionOpen) == ChatVoice.TurnOn.AskCru) { pressing = false; _ui.update { it.copy(askSwitch = true) }; return }
        _ui.update { it.copy(mode = ChatVoiceUi.Mode.Pressed, interim = "", note = null) }
        start()
    }

    /** Still held after [ChatVoice.HOLD_MS]: push to talk. */
    fun hold() {
        if (pressing && _ui.value.mode == ChatVoiceUi.Mode.Pressed) _ui.update { it.copy(mode = ChatVoiceUi.Mode.Holding, since = System.currentTimeMillis()) }
    }

    /** While held, the finger slid [progress] of the way to Cancel. */
    fun drag(progress: Float) {
        if (pressing && _ui.value.lit) _ui.update { it.copy(cancelProgress = progress.coerceIn(0f, 1f)) }
    }

    /**
     * The finger came up (or slid off) after [heldMs], [overCancel] or not.
     * False only for a tap in voice mode with nothing heard to send (the
     * screen may then send what is typed).
     */
    fun release(heldMs: Long, overCancel: Boolean = false): Boolean {
        if (!pressing) return true
        pressing = false
        _ui.update { it.copy(cancelProgress = 0f) }
        val m = _ui.value.mode
        if (m == ChatVoiceUi.Mode.Off || m == ChatVoiceUi.Mode.Finishing) return true
        when (ChatVoice.release(pressWasOn, heldMs, overCancel)) {
            ChatVoice.Release.Off -> stop()
            ChatVoice.Release.VoiceMode -> {
                _ui.update { it.copy(mode = ChatVoiceUi.Mode.On, since = System.currentTimeMillis()) }
                startCallAudio()
                // Said while the press was being decided: it goes now.
                ChatVoice.message(ChatVoice.joined(said))?.let(send)
                said.clear(); syncHeld()
            }
            ChatVoice.Release.PushToTalk -> finishPushToTalk()
            // Tapped to pause the reply, with nothing to send: that was all.
            ChatVoice.Release.SendNow -> return sendNow() || pressPaused
            // Let go over Cancel: held, nothing goes and the mic stops; in voice mode the words go and it listens on.
            ChatVoice.Release.Cancel -> if (pressWasOn) { replies.clear(); stopSpeaking(); discardWords() } else { socket?.send(DISCARD); teardown() }
        }
        return true
    }

    /** A tap without the gesture (accessibility, or the microphone just allowed): voice mode on, or off. */
    fun toggle() { if (_ui.value.mode == ChatVoiceUi.Mode.On) stop() else { press(); release(0) } }

    /**
     * In voice mode, what has been heard so far goes now, without waiting for
     * the pause, and it listens on for the next. False when nothing was heard.
     */
    fun sendNow(): Boolean {
        if (_ui.value.mode != ChatVoiceUi.Mode.On) return false
        val words = ChatVoice.joined(said + _ui.value.interim)
        val msg = ChatVoice.message(words) ?: return false
        dropHeard(words)
        send(msg)
        replies.ownerDone(); pumpSoon()
        return true
    }

    /** Cancel tapped (voice mode): the words being heard are dropped; with none, voice mode goes off. */
    fun cancel() {
        if (!_ui.value.lit) return
        when (ChatVoice.cancelTap(_ui.value.hasWords || said.isNotEmpty())) {
            ChatVoice.CancelTap.Discard -> { replies.clear(); stopSpeaking(); discardWords(); replies.ownerDone() }
            ChatVoice.CancelTap.Off -> stop()
        }
    }

    private fun discardWords() = dropHeard(ChatVoice.joined(said + _ui.value.interim))

    /** The server forgets what it has heard (so its utterance does not bring [words] back), and so does the screen. */
    private fun dropHeard(words: String) {
        early = words
        socket?.send(DISCARD)
        said.clear(); syncHeld()
        _ui.update { it.copy(interim = "", since = System.currentTimeMillis()) }
    }

    /** "Switch to this chat?" answered: yes closes Cru's session, and once it has let go of the microphone voice mode goes on here. */
    fun switchAnswer(yes: Boolean) {
        _ui.update { it.copy(askSwitch = false) }
        if (!yes) return
        HeyCrundi.closeSession()
        scope.launch {
            withTimeoutOrNull(2_000) { while (HeyCrundi.sessionOpen) delay(50) }
            // Its microphone a moment to close.
            delay(300)
            if (HeyCrundi.sessionOpen) note("Cru did not let go of the microphone.") else toggle()
        }
    }

    /** The chat is in front ([on]) or behind another (Cru's screen, another app). */
    fun foreground(on: Boolean) { foreground = on; handover() }

    @Volatile private var foreground = true
    private var cruWatch: Job? = null

    private fun handover() {
        when (ChatVoice.handover(_ui.value.mode, HeyCrundi.sessionOpen, foreground)) {
            ChatVoice.Handover.Pause -> pauseForCru()
            ChatVoice.Handover.Resume -> resumeAfterCru()
            ChatVoice.Handover.Off -> stop()
            ChatVoice.Handover.None -> {}
        }
    }

    /** While lit: Cru opening or closing is noticed here. */
    private fun watchCru() {
        if (cruWatch?.isActive == true) return
        cruWatch = scope.launch { while (isActive && _ui.value.lit) { delay(500); handover() } }
    }

    /** Cru took the microphone: the stream closes, the reply pauses, and voice mode waits for Cru to be done. */
    private fun pauseForCru() {
        if (replies.playing != null) { speaker.stop(); replies.pause(System.currentTimeMillis()) }
        pump?.cancel(); pump = null
        teardown()
        _ui.update { it.copy(mode = ChatVoiceUi.Mode.Paused, speaking = false) }
        watchCru()
    }

    private fun resumeAfterCru() {
        _ui.update { it.copy(mode = ChatVoiceUi.Mode.On, since = System.currentTimeMillis(), interim = "") }
        start()
        if (_ui.value.mode == ChatVoiceUi.Mode.On) { startCallAudio(); pumpSoon() }
    }

    /** Voice mode off: listening and Claude's voice stop. [why] is shown for a moment. */
    fun stop(why: String? = null) {
        cruWatch?.cancel(); cruWatch = null
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

    /** Say [text] after anything already queued, and behind the owner if he is talking. */
    fun say(text: String) {
        replies.add(text)
        pumpSoon()
    }

    /** The owner is holding the mic (or it is being decided, or his hold is being sent). */
    private fun ownerBusy() = _ui.value.mode.let { it == ChatVoiceUi.Mode.Pressed || it == ChatVoiceUi.Mode.Holding || it == ChatVoiceUi.Mode.Finishing }

    /** Play the next sentence now, or keep looking until the owner has finished. */
    private fun pumpSoon() {
        if (playNext()) return
        if (replies.isEmpty || pump?.isActive == true) return
        pump = scope.launch { while (isActive && !replies.isEmpty && replies.playing == null) { delay(200); if (playNext()) break } }
    }

    /** True when something is playing (or nothing is left to play). */
    private fun playNext(): Boolean {
        if (replies.playing != null) return true
        val t = replies.take(ownerBusy(), _ui.value.hasWords, System.currentTimeMillis())
        if (t == null) { if (replies.isEmpty) replyEnded(); return replies.isEmpty }
        if (!_ui.value.speaking) _ui.update { it.copy(speaking = true) }
        // Nothing goes to the server while it plays; the phone listens for the owner over it.
        if (_ui.value.mode == ChatVoiceUi.Mode.On && !pausedForReply) {
            barge.begin()
            pausedForReply = true
            streaming = false
            socket?.send("""{"type":"pause"}""")
        }
        speaker.say(t) { replies.done(); if (!playNext()) pumpSoon() }
        return true
    }

    /** All said: listening again after the speaker's tail and the room's echo. */
    private fun replyEnded() {
        if (!_ui.value.speaking && !pausedForReply) return
        _ui.update { it.copy(speaking = false) }
        if (!pausedForReply) return
        val g = gen
        scope.launch {
            delay(RESUME_AFTER_SPEECH_MS)
            if (g == gen && pausedForReply && replies.playing == null) resumeStream(emptyList())
        }
    }

    /** The owner talked over the reply (or tapped the mic): it pauses where it was, and his words go. */
    private fun pauseReply(first: List<ByteArray> = emptyList()) {
        speaker.stop()
        replies.pause(System.currentTimeMillis())
        _ui.update { it.copy(speaking = false) }
        if (pausedForReply) resumeStream(first)
        pumpSoon()
    }

    private fun resumeStream(first: List<ByteArray>) {
        pausedForReply = false
        if (!_ui.value.listening) return
        val s = socket
        if (s != null) { s.send("""{"type":"resume"}"""); for (b in first) s.send(b) }
        streaming = true
    }

    /** Claude's voice stops for good (voice mode off, Cancel). */
    private fun stopSpeaking() {
        pump?.cancel(); pump = null
        replies.clear()
        speaker.stop()
        if (_ui.value.speaking) _ui.update { it.copy(speaking = false) }
        if (pausedForReply) resumeStream(emptyList())
    }

    /** Voice mode: Claude's voice as speakerphone call audio, in the mic's session, so the echo canceller can take it out. */
    private fun startCallAudio() {
        val m = mic ?: return
        if (callAudio != null) return
        val c = CallAudio(ctx)
        callAudio = c
        speaker.sessionId = m.sessionId
        c.onRoute = { r -> speaker.route = r; speaker.device = c.speaker }
        speaker.route = c.start()
        speaker.device = c.speaker
    }

    // ─── listening ───

    private fun start() {
        current?.takeIf { it !== this }?.stop(ChatVoice.MOVED)
        current = this
        // Cru opened on purpose takes the microphone: voice mode pauses (a hold just ends).
        HeyCrundi.onSessionOpening { if (_ui.value.mode == ChatVoiceUi.Mode.On) pauseForCru() else if (_ui.value.lit && _ui.value.mode != ChatVoiceUi.Mode.Paused) stop() }
        val g = ++gen
        said.clear(); syncHeld()
        synchronized(preroll) { preroll.clear() }
        streaming = true
        val wake = HeyCrundi.wakeListening
        HeyCrundi.chatVoice(true); holdsMic = true
        ChatVoice.heyCruNote(wake)?.let { note(it) }
        val m = MicStream { bytes, level ->
            _levels.value = _levels.value.drop(1) + level
            speech.push(bytes)
            if (pausedForReply) {
                // Claude is talking: nothing goes out; the phone decides if it is him over it.
                barge.hear(bytes, level, speaker.playLevel, 100) { speech.talking() }?.let { first ->
                    scope.launch { if (pausedForReply) pauseReply(first) }
                }
                return@MicStream
            }
            if (!_ui.value.speaking) barge.idle(level)
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
        watchCru()
    }

    private fun heard(ev: ListenEvent) {
        val mode = _ui.value.mode
        if (mode == ChatVoiceUi.Mode.Off) return
        when (ev) {
            is ListenEvent.Ready -> {
                // The microphone is live: Cru's "listening" tone, once per stream.
                if (_ui.value.connecting) speaker.tone(mv.obitech.crundi.voice.Tones.of(mv.obitech.crundi.voice.Tones.LISTEN))
                _ui.update { it.copy(connecting = false) }
            }
            is ListenEvent.Interim -> if (early.isNotEmpty() && ChatVoice.afterEarly(ev.text, early).isEmpty()) {} else if (mode != ChatVoiceUi.Mode.Finishing || ev.text.isNotBlank()) {
                if (ev.text.isNotBlank() && ev.text != _ui.value.interim) replies.words(System.currentTimeMillis())
                _ui.update { it.copy(interim = ev.text, connecting = false) }
            }
            is ListenEvent.Utterance -> {
                _ui.update { it.copy(interim = "") }
                when (mode) {
                    ChatVoiceUi.Mode.On -> { val t = ChatVoice.afterEarly(ev.text, early); early = ""; ChatVoice.message(t)?.let(send); replies.ownerDone(); pumpSoon() }
                    ChatVoiceUi.Mode.Finishing -> { said += ev.text; syncHeld(); final?.complete(Unit) }
                    else -> { said += ev.text; syncHeld() }
                }
            }
            // Cru's settings changed: nothing for a chat's voice mode to do.
            ListenEvent.Settings -> {}
            // "Scratch that": what was heard so far goes nowhere.
            is ListenEvent.Discard -> { said.clear(); syncHeld(); _ui.update { it.copy(interim = "") }; replies.ownerDone(); pumpSoon() }
            is ListenEvent.Failed -> if (mode == ChatVoiceUi.Mode.Finishing) final?.complete(Unit) else stop("Voice input stopped: " + ev.error.trimEnd('.') + ".")
            // The voice assistant's own news is not this chat's.
            is ListenEvent.Report, is ListenEvent.Update, ListenEvent.Speech, is ListenEvent.Screen, is ListenEvent.Kid, is ListenEvent.GameCommand -> {}
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
            // Let go and sent: a reply waiting behind him goes now.
            replies.ownerDone(); pumpSoon()
        }
    }

    private fun teardown() {
        gen++
        streaming = false
        pausedForReply = false
        callAudio?.let { it.onRoute = null; it.stop() }; callAudio = null
        speaker.route = CruAudio.Route.Media; speaker.device = null; speaker.sessionId = 0
        mic?.stop(); mic = null
        socket?.let { s -> runCatching { s.close() } }
        socket = null
        synchronized(preroll) { preroll.clear() }
        said.clear(); syncHeld()
        early = ""
        _levels.value = List(BARS) { 0f }
        final = null
        if (holdsMic) { holdsMic = false; HeyCrundi.chatVoice(false) }
        if (current === this) { current = null; HeyCrundi.onSessionOpening(null) }
        _ui.update { it.copy(mode = ChatVoiceUi.Mode.Off, interim = "", connecting = false, cancelProgress = 0f) }
    }

    companion object {
        private const val TAG = "CrundiChatVoice"
        private const val RESUME_AFTER_SPEECH_MS = 450L
        private const val DISCARD = """{"type":"discard"}"""
        /** Levels kept for the recording bar (100 ms each). */
        const val BARS = 28
        /** The chat whose voice mode has the mic (one at a time, across panes and bubbles). */
        private var current: ChatVoiceSession? = null
    }
}
