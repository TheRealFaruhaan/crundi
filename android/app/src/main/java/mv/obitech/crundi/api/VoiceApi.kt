package mv.obitech.crundi.api

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

/**
 * The owner's voice assistant on the server (src/voice/, capability
 * "voice-assistant"). The phone never holds a Claude or Deepgram key: it
 * sends what was said and gets back what to say and what to show.
 *
 *   listen()  the microphone, live, to Deepgram through the server
 *   turn()    one request to Claude: { speak, show }
 *   speak()   a reply in the chosen Deepgram voice (mp3)
 *
 * A server without the assistant answers 404: [unsupported] says so, and a
 * screen says "Update the server" rather than failing.
 */
class VoiceApi internal constructor(private val rpc: Rpc, private val transport: Transport) {

    suspend fun settings(): VoiceSettings = rpc.read(rpc.get("/api/voice/settings"), null)

    /** Change the model, the monthly limit or the Deepgram voice ("" = the phone's own). Null leaves one as it is. */
    suspend fun save(model: String? = null, monthlyLimitUsd: Double? = null, ttsVoice: String? = null, billingDay: Int? = null, deepgramBillingDay: Int? = null, pauseMs: Int? = null, deepgramLimitUsd: Double? = null, speechRate: Double? = null, screenCloseSeconds: Int? = null, brain: String? = null, cliIdleMinutes: Int? = null, volume: Int? = null, adminMode: Boolean? = null, ownerOnly: Boolean? = null, effort: String? = null): VoiceSettings =
        rpc.read(rpc.post("/api/voice/settings", mapOf("effort" to effort, "model" to model, "monthlyLimitUsd" to monthlyLimitUsd, "ttsVoice" to ttsVoice, "billingDay" to billingDay, "deepgramBillingDay" to deepgramBillingDay, "pauseMs" to pauseMs, "deepgramLimitUsd" to deepgramLimitUsd, "speechRate" to speechRate, "screenCloseSeconds" to screenCloseSeconds, "brain" to brain, "cliIdleMinutes" to cliIdleMinutes, "volume" to volume, "adminMode" to adminMode, "ownerOnly" to ownerOnly)), null)

    /**
     * Cru on its own (capability "voice-away"): whether Cru may act without
     * asking when nobody is talking to it, and which classes ([VoiceSettings.awayClasses] ids).
     */
    suspend fun saveAway(on: Boolean, allow: List<String>): VoiceSettings =
        rpc.read(rpc.post("/api/voice/settings", mapOf("away" to mapOf("on" to on, "allow" to allow))), null)

    /**
     * Stop everything Cru does on its own (capability "voice-away"): away off,
     * every ticker and trigger paused, its questions dropped, its jobs paused.
     */
    suspend fun stopAway(): AwayStopped {
        val o = rpc.post("/api/voice/away/stop", mapOf("everything" to true))
        fun n(k: String) = (o[k] as? JsonPrimitive)?.contentOrNull?.toDoubleOrNull()?.toInt() ?: 0
        return AwayStopped(n("wakeupsPaused"), n("questionsDropped"), n("jobsPaused"), runCatching { CrundiJson.decodeFromJsonElement(VoiceSettings.serializer(), o) }.getOrNull())
    }

    /** Answer one of Cru's questions (an approval with source "cru"): [choice] "yes" or "no". */
    suspend fun ask(id: String, choice: String) { rpc.post("/api/voice/ask", mapOf("id" to id, "choice" to choice)) }

    /**
     * Kid mode's settings (capability "voice-kid"); null leaves one as it is.
     * [kidVoice] "" is Cru's own voice. The server refuses any change while kid mode is on (423).
     */
    suspend fun saveKid(kidVoice: String? = null, kidChattiness: String? = null, kidPauseMs: Int? = null, kidReflex: Boolean? = null, kidGames: KidGamesSettings? = null): VoiceSettings =
        rpc.read(rpc.post("/api/voice/settings", mapOf(
            "kidVoice" to kidVoice, "kidChattiness" to kidChattiness, "kidPauseMs" to kidPauseMs, "kidReflex" to kidReflex,
            "kidGames" to kidGames?.let { mapOf("improve" to it.improve, "from" to it.from, "to" to it.to) },
        )), null)

    /**
     * Replace one of the server's keys: [key] is "anthropicKey", "deepgramKey"
     * or "anthropicAdminKey". Write only: no key ever comes back, only
     * whether each is set.
     */
    suspend fun saveKey(key: String, value: String): VoiceSettings {
        require(key in KEYS) { "Not a voice key: $key" }
        return rpc.read(rpc.post("/api/voice/settings", mapOf(key to value.trim())), null)
    }

    /**
     * The two brains compared: turns, speed, cost and cache use per brain and
     * model. Null from a server without the brain experiment (404).
     */
    suspend fun brainStats(): BrainStats? {
        val r = transport.exchange(ApiRequest("GET", "/api/voice/brain-stats"))
        if (r.status == 404) return null
        if (r.status !in 200..299) throw ApiException(r.status, OkHttpTransport.defaultMessage(r.status, r.text))
        return CrundiJson.decodeFromJsonElement(BrainStats.serializer(), CrundiJson.parseToJsonElement(r.text))
    }

    /**
     * One request. [where] is what the owner is looking at ("lock screen",
     * "side key"); [locked] tells Claude the phone is locked, so anything
     * that would change something waits for an unlock (spec §4).
     */
    suspend fun turn(text: String, where: String, locked: Boolean, typed: Boolean = false, attachments: List<String> = emptyList(), refs: List<String> = emptyList(), requestId: String = ""): VoiceReply {
        // A failed turn comes back { ok:false, error } with 200: read it as a reply, not an exception.
        // [typed]: keyboard mode, words typed rather than heard (an older server ignores it).
        // [attachments]: ids from [attach]; [refs]: shared text or links (capability "voice-attach").
        val body = mutableMapOf<String, Any?>("text" to text, "where" to where, "locked" to locked)
        if (typed) body["typed"] = true
        if (attachments.isNotEmpty()) body["attachments"] = attachments
        if (refs.isNotEmpty()) body["refs"] = refs
        // [requestId] (capability "voice-turn-id"): the same turn sent again (the HTTP client resends it
        // after a dropped connection) gets that turn's answer, never a second run or "still answering".
        if (requestId.isNotEmpty()) body["requestId"] = requestId
        val r = transport.exchange(ApiRequest("POST", "/api/voice/turn", json = Uploads.toJson(body), slow = true))
        if (r.status == 404) throw ApiException(404, UNSUPPORTED)
        val obj = runCatching { CrundiJson.parseToJsonElement(r.text) as? JsonObject }.getOrNull()
            ?: throw ApiException(r.status, OkHttpTransport.defaultMessage(r.status, r.text))
        if (r.status == 401) throw ApiException(401, OkHttpTransport.defaultMessage(401))
        return CrundiJson.decodeFromJsonElement(VoiceReply.serializer(), obj)
    }

    /**
     * A file for Cru (capability "voice-attach"), streamed as base64 JSON:
     * [open] is read once, never held whole. [preview]: a smaller JPEG of a
     * picture (base64), which is what Cru looks at. A server without it: 404,
     * said as [ATTACH_UNSUPPORTED].
     */
    suspend fun attach(name: String, type: String, size: Long, preview: String? = null, progress: Progress? = null, open: () -> java.io.InputStream): VoiceAttached {
        val fields = mapOf<String, Any?>("name" to name, "type" to type, "preview" to preview, "from" to "android")
        val o = try {
            rpc.call("POST", "/api/voice/attach", upload = Uploads.base64Json(fields, size, open), progress = progress, slow = true)
        } catch (e: ApiException) {
            throw if (e.status == 404) ApiException(404, ATTACH_UNSUPPORTED) else e
        }
        return CrundiJson.decodeFromJsonElement(VoiceAttached.serializer(), o)
    }

    /**
     * Cancel the turn in progress (capability "voice-cancel"): what it had
     * already done comes back in [CancelReply.acted]. Null from an older server.
     */
    suspend fun cancel(): CancelReply? {
        val r = transport.exchange(ApiRequest("POST", "/api/voice/cancel", json = Uploads.toJson(emptyMap<String, Any?>())))
        if (r.status == 404) return null
        if (r.status !in 200..299) throw ApiException(r.status, OkHttpTransport.defaultMessage(r.status, r.text))
        return CrundiJson.decodeFromJsonElement(CancelReply.serializer(), CrundiJson.parseToJsonElement(r.text))
    }

    /** A voice session just opened: warm the server's prompt cache while the owner speaks. Best effort. */
    suspend fun warm() { runCatching { rpc.post("/api/voice/warm") } }

    /**
     * The conversation so far becomes a summary (capability "voice-compact"),
     * the notebook kept. Null from an older server (404).
     */
    suspend fun compact(): CompactReply? {
        val r = transport.exchange(ApiRequest("POST", "/api/voice/compact", json = Uploads.toJson(emptyMap<String, Any?>()), slow = true))
        if (r.status == 404) return null
        val obj = runCatching { CrundiJson.parseToJsonElement(r.text) as? JsonObject }.getOrNull()
            ?: throw ApiException(r.status, OkHttpTransport.defaultMessage(r.status, r.text))
        return CrundiJson.decodeFromJsonElement(CompactReply.serializer(), obj)
    }

    /**
     * Cru's conversation as chat messages (capability "voice-history"): the
     * latest [limit] turns before [before] (ms; 0 the newest), oldest first.
     * Null from an older server (404).
     */
    suspend fun history(limit: Int = 40, before: Long = 0): VoiceHistory? {
        val q = mutableMapOf("limit" to limit.toString())
        if (before > 0) q["before"] = before.toString()
        val r = transport.exchange(ApiRequest("GET", "/api/voice/history", query = q))
        if (r.status == 404) return null
        if (r.status !in 200..299) throw ApiException(r.status, OkHttpTransport.defaultMessage(r.status, r.text))
        return CrundiJson.decodeFromJsonElement(VoiceHistory.serializer(), CrundiJson.parseToJsonElement(r.text))
    }

    /**
     * Cru's tickers and triggers (capability "voice-wakeups"). Null from an
     * older server (404).
     */
    suspend fun wakeups(): VoiceWakeups? {
        val r = transport.exchange(ApiRequest("GET", "/api/voice/wakeups"))
        if (r.status == 404) return null
        if (r.status !in 200..299) throw ApiException(r.status, OkHttpTransport.defaultMessage(r.status, r.text))
        return CrundiJson.decodeFromJsonElement(VoiceWakeups.serializer(), CrundiJson.parseToJsonElement(r.text))
    }

    /** Switch one on or off, or delete it: [action] is "enable", "disable" or "delete". The list as it is now. */
    suspend fun wakeup(id: String, action: String): VoiceWakeups {
        require(action in setOf("enable", "disable", "delete")) { "Not a wake-up action: $action" }
        return rpc.read(rpc.post("/api/voice/wakeups", mapOf("id" to id, "action" to action)), null)
    }

    // ─── Cru's unread reports (capability "voice-unread") ───

    /** How many of Cru's reports nobody heard. Null from an older server (404). */
    suspend fun unread(): VoiceUnread? {
        val r = transport.exchange(ApiRequest("GET", "/api/voice/unread"))
        if (r.status == 404) return null
        if (r.status !in 200..299) throw ApiException(r.status, OkHttpTransport.defaultMessage(r.status, r.text))
        return CrundiJson.decodeFromJsonElement(VoiceUnread.serializer(), CrundiJson.parseToJsonElement(r.text))
    }

    /** The owner has seen them (the chat view opened). Best effort; nothing on an older server. */
    suspend fun seen() { runCatching { transport.exchange(ApiRequest("POST", "/api/voice/seen", json = Uploads.toJson(emptyMap<String, Any?>()))) } }

    // ─── the owner's voice (capability "voice-speaker") ───

    /** Whether Cru knows the owner's voice, and the phrases to enrol with. Null from an older server (404). */
    suspend fun speaker(): SpeakerStatus? {
        val r = transport.exchange(ApiRequest("GET", "/api/voice/speaker"))
        if (r.status == 404) return null
        if (r.status !in 200..299) throw ApiException(r.status, OkHttpTransport.defaultMessage(r.status, r.text))
        return CrundiJson.decodeFromJsonElement(SpeakerStatus.serializer(), CrundiJson.parseToJsonElement(r.text))
    }

    /**
     * One enrolment phrase: [pcm] 16-bit little-endian mono at [rate].
     * [restart]: the first phrase of a fresh enrolment. A phrase the server
     * does not accept comes back with [EnrolReply.problem]; a failure with [EnrolReply.error].
     */
    suspend fun enrolPhrase(pcm: ByteArray, rate: Int, restart: Boolean): EnrolReply {
        val up = Upload("audio/L16;rate=$rate", pcm.size.toLong(), { out, count -> out.write(pcm); count(pcm.size.toLong()) }, pcm.size.toLong())
        val r = transport.exchange(ApiRequest("POST", "/api/voice/speaker/enrol", query = if (restart) mapOf("restart" to "1") else emptyMap(), upload = up, slow = true))
        if (r.status == 404) return EnrolReply(ok = false, error = SPEAKER_UNSUPPORTED)
        val obj = runCatching { CrundiJson.parseToJsonElement(r.text) as? JsonObject }.getOrNull()
            ?: return EnrolReply(ok = false, error = OkHttpTransport.defaultMessage(r.status, r.text))
        return CrundiJson.decodeFromJsonElement(EnrolReply.serializer(), obj)
    }

    /**
     * One short reply (capability "voice-short-words"): [word] is its index in
     * [SpeakerStatus.wordPrompts], said [SpeakerStatus.wordRepeats] times in
     * [pcm] (16-bit little-endian mono at [rate]). [restart]: the first of a fresh round.
     */
    suspend fun enrolWord(pcm: ByteArray, rate: Int, word: Int, restart: Boolean): EnrolReply {
        val up = Upload("audio/L16;rate=$rate", pcm.size.toLong(), { out, count -> out.write(pcm); count(pcm.size.toLong()) }, pcm.size.toLong())
        val q = if (restart) mapOf("word" to word.toString(), "restart" to "1") else mapOf("word" to word.toString())
        val r = transport.exchange(ApiRequest("POST", "/api/voice/speaker/words", query = q, upload = up, slow = true))
        if (r.status == 404) return EnrolReply(ok = false, error = SPEAKER_UNSUPPORTED)
        val obj = runCatching { CrundiJson.parseToJsonElement(r.text) as? JsonObject }.getOrNull()
            ?: return EnrolReply(ok = false, error = OkHttpTransport.defaultMessage(r.status, r.text))
        return CrundiJson.decodeFromJsonElement(EnrolReply.serializer(), obj)
    }

    /** The short replies said become part of the owner's voice (at least [SpeakerStatus.minWords]). */
    suspend fun finishWords(): SpeakerStatus = rpc.read(rpc.post("/api/voice/speaker/words/finish"), null)

    /** The phrases read become the owner's voice profile (at least [SpeakerStatus.minPhrases]). */
    suspend fun finishEnrol(): SpeakerStatus = rpc.read(rpc.post("/api/voice/speaker/finish"), null)

    /** Forget the owner's voice: Cru acts on any voice again. */
    suspend fun forgetSpeaker(): SpeakerStatus = rpc.read(rpc.delete("/api/voice/speaker"), null)

    /** Start a fresh conversation. */
    suspend fun reset() { rpc.post("/api/voice/reset") }

    /** What Anthropic reports the whole account has cost this billing month (capability "voice-account-usage"). */
    suspend fun accountUsage(): AccountUsage {
        val r = transport.exchange(ApiRequest("GET", "/api/voice/account-usage"))
        if (r.status == 404) throw ApiException(404, UNSUPPORTED)
        return CrundiJson.decodeFromJsonElement(AccountUsage.serializer(), CrundiJson.parseToJsonElement(r.text))
    }

    suspend fun voices(): VoiceList = rpc.read(rpc.get("/api/voice/voices"), null)

    // ─── memory (capability "voice-memory", D39) ───

    /** The notebook's files with their text, how many things are remembered, and the meaning search's state. */
    suspend fun memory(): VoiceMemory = rpc.read(rpc.get("/api/voice/memory"), null)

    /** A notebook file written as [text]; '' deletes it. */
    suspend fun saveMemory(path: String, text: String) { rpc.post("/api/voice/memory", mapOf("path" to path, "text" to text)) }

    /** What Crundi remembers about [q]: turns, compactions, notes. */
    suspend fun searchMemory(q: String): List<MemoryHit> =
        rpc.read<MemorySearch>(rpc.get("/api/voice/memory/search", mapOf("q" to q)), null).results

    /**
     * The "one moment, Sir" cues for the voice in use (capability "voice-cues").
     * Null from an older server (404): play none, or say [Cues.FALLBACK] with the phone's voice.
     */
    suspend fun cues(): CueList? {
        val r = transport.exchange(ApiRequest("GET", "/api/voice/cues"))
        if (r.status == 404) return null
        if (r.status !in 200..299) throw ApiException(r.status, OkHttpTransport.defaultMessage(r.status, r.text))
        return CrundiJson.decodeFromJsonElement(CueList.serializer(), CrundiJson.parseToJsonElement(r.text))
    }

    /** One cue's mp3, from [CueItem.url] (a path with its query). */
    suspend fun cueClip(url: String): ByteArray {
        val path = url.substringBefore('?')
        val query = url.substringAfter('?', "").split('&').filter { it.contains('=') }.associate { kv ->
            java.net.URLDecoder.decode(kv.substringBefore('='), "UTF-8") to java.net.URLDecoder.decode(kv.substringAfter('='), "UTF-8")
        }
        val r = transport.exchange(ApiRequest("GET", path, query = query, headers = mapOf("Accept" to "audio/mpeg")))
        if (r.status in 200..299 && r.bytes != null && r.bytes.isNotEmpty()) return r.bytes
        throw ApiException(r.status, OkHttpTransport.defaultMessage(r.status, r.text))
    }

    /** [text] spoken in [voice] (or the chosen one): mp3 bytes. */
    suspend fun speak(text: String, voice: String = ""): ByteArray {
        val r = transport.exchange(ApiRequest("POST", "/api/voice/speak", json = Uploads.toJson(mapOf("text" to text, "voice" to voice.ifEmpty { null })), headers = mapOf("Accept" to "audio/mpeg")))
        if (r.status in 200..299 && r.bytes != null && r.bytes.isNotEmpty()) return r.bytes
        val err = runCatching { (CrundiJson.parseToJsonElement(r.text) as? JsonObject)?.errorText() }.getOrNull()
        throw ApiException(r.status, if (r.status == 404) UNSUPPORTED else err ?: OkHttpTransport.defaultMessage(r.status, r.text))
    }

    /**
     * [text] spoken as it is made (capability "voice-tts-stream"): raw 16-bit
     * mono PCM at [STREAM_RATE] written to [sink] as it arrives, so playback
     * can start after the first bytes. Only call it on a server that lists
     * the capability: an older one sends a whole mp3 instead.
     */
    suspend fun speakStream(text: String, voice: String, sink: java.io.OutputStream) {
        transport.download(
            ApiRequest("POST", "/api/voice/speak", query = mapOf("stream" to "1"), json = Uploads.toJson(mapOf("text" to text, "voice" to voice.ifEmpty { null })), headers = mapOf("Accept" to "audio/L16"), slow = true),
            sink,
        )
    }

    /**
     * The live microphone stream. The phone sends 16-bit mono PCM at
     * [sampleRate] as binary frames, and { type: pause | resume | stop };
     * the server sends [ListenEvent]s. Blocking (it may sign in first).
     */
    fun listen(listener: SocketListener, sampleRate: Int = 16000): DataSocket =
        transport.openSocket("/api/voice/listen", mapOf("encoding" to "linear16", "sample_rate" to sampleRate.toString()), listener)

    /**
     * Keyboard mode's line (capability "voice-keyboard"): no audio and no
     * Deepgram, only Cru's reports and settings changes come down it. Send
     * { type: stop } and close it when leaving keyboard mode. Blocking.
     */
    fun listenKeyboard(listener: SocketListener): DataSocket =
        transport.openSocket("/api/voice/listen", mapOf("keyboard" to "1"), listener)

    companion object {
        const val CAPABILITY = "voice-assistant"
        const val TTS_STREAM = "voice-tts-stream"
        /** The notebook and its search (Phase 2). */
        const val MEMORY = "voice-memory"
        /** A turn's reply may carry [VoiceReply.phoneActions] (Phase 6). */
        const val PHONE_ACTIONS = "voice-phone-actions"
        /** POST /api/voice/cancel, { type: 'discard' } both ways on the stream. */
        const val CANCEL = "voice-cancel"
        /** Voice mode in a chat: the composer's mic, and the chat's voice-say events (Claude's speak tool). */
        const val CHAT_VOICE = "chat-voice"
        /** GET /api/voice/cues and its clips. */
        const val CUES = "voice-cues"
        /** settings.volume, a turn's reply volume, { type: settings } on the stream. */
        const val VOLUME = "voice-volume"
        /** POST /api/voice/compact: the voice screen's Compact button. */
        const val COMPACT = "voice-compact"
        /** GET /api/voice/history: the voice screen's chat view. */
        const val HISTORY = "voice-history"
        /** /api/voice/listen?keyboard=1: reports and settings without a microphone. */
        const val KEYBOARD = "voice-keyboard"
        /** GET/POST /api/voice/wakeups and the 'voice-wakeups' event. */
        const val WAKEUPS = "voice-wakeups"
        /** The owner's voice: GET/DELETE /api/voice/speaker, enrol, finish; ownerOnly; utterances' owner. */
        const val SPEAKER = "voice-speaker"
        /** Short replies: POST /api/voice/speaker/words?word=N, /words/finish; status shortWords, wordPrompts. */
        const val SHORT_WORDS = "voice-short-words"
        /** GET /api/voice/unread, POST /api/voice/seen, the 'voice-unread' event, cruChat pushes. */
        const val UNREAD = "voice-unread"
        /** Cru on its own: settings.away/awayClasses, POST /api/voice/away/stop, POST /api/voice/ask, approvals with source "cru", built-in tickers. */
        const val AWAY = "voice-away"
        /** Files for Cru: [attach], and a turn's attachments and refs. */
        const val ATTACH = "voice-attach"
        const val ATTACH_UNSUPPORTED = "This server does not take files for Cru yet: update it"
        /** The largest file the server takes. */
        const val ATTACH_MAX = 50L * 1024 * 1024
        const val SPEAKER_UNSUPPORTED = "Not supported by this server: update it to teach Cru your voice."
        /** The rate of [speakStream]'s PCM (the server's STREAM_RATE). */
        const val STREAM_RATE = 24000
        /** The keys [saveKey] may set. */
        val KEYS = setOf("anthropicKey", "deepgramKey", "anthropicAdminKey")
        const val UNSUPPORTED = "This server does not have the voice assistant yet. Update the server to use it."
        fun unsupported(e: Throwable) = e is ApiException && e.status == 404
    }
}

@Serializable
data class AccountUsage(
    val ok: Boolean = false,
    val totalUsd: Double = 0.0,
    /** The billing month's start (ISO). */
    val since: String = "",
    /** No Admin API key on the server yet. */
    val needsKey: Boolean = false,
    val error: String = "",
)

@Serializable
data class CueItem(val id: String = "", val text: String = "", val url: String? = null)

/** GET /api/voice/cues: [device] means say the texts with the phone's own voice. */
@Serializable
data class CueList(val ok: Boolean = false, val voice: String = "", val rate: String = "", val device: Boolean = false, val cues: List<CueItem> = emptyList())

@Serializable
data class VoiceModel(
    val id: String = "", val label: String = "",
    /** The thinking efforts it takes (capability "voice-effort"); empty from an older server. */
    val efforts: List<String> = emptyList(),
    /** Its own default effort on the API, for the note. */
    val apiDefaultEffort: String = "",
)

@Serializable
data class VoiceSpend(
    val month: String = "",
    val claudeUsd: Double = 0.0,
    val deepgramUsd: Double = 0.0,
    val totalUsd: Double = 0.0,
    val turns: Int = 0,
)

@Serializable
data class VoiceSettings(
    val model: String = "",
    val models: List<VoiceModel> = emptyList(),
    /** Cru's thinking effort (capability "voice-effort"): low, medium, high, xhigh or max; "" from an older server. */
    val effort: String = "",
    val efforts: List<String> = emptyList(),
    val anthropicKeySet: Boolean = false,
    val deepgramKeySet: Boolean = false,
    /** A Deepgram voice, or "" for the phone's own (D34). */
    val ttsVoice: String = "",
    /** Claude's monthly limit. */
    val monthlyLimitUsd: Double = 50.0,
    /** Deepgram's own (D46); past it the phone's voice and recogniser take over. */
    val deepgramLimitUsd: Double = 200.0,
    val deepgramLimitReached: Boolean = false,
    /** The day of the month the API account is billed; the spend restarts then. */
    val billingDay: Int = 1,
    /** This billing month's first day and the next one ("2026-11-20"); empty from an older server. */
    val periodStart: String = "",
    val nextReset: String = "",
    /** Deepgram bills on its own day: its part of the spend restarts then. */
    val deepgramBillingDay: Int = 1,
    val deepgramNextReset: String = "",
    /** How long the owner may pause before what they said is sent, ms (D44). */
    val pauseMs: Int = 2000,
    /** How fast Cru talks: 1 normal, 0.7 to 1.5 (Deepgram's speed; the phone's voice follows). An older server has none: 1. */
    val speechRate: Double = 1.0,
    /** After this many seconds of silence the voice screen closes (every client; "Hey Cru" opens it again). An older server has none: 0, never. */
    val screenCloseSeconds: Int = 0,
    /** Which brain answers: "api" or "claude-code" (an experiment). An older server has none: "", and the switch is not shown. */
    val brain: String = "",
    /** Claude Code (the claude-code brain) stops after this many idle minutes, 1 to 240. An older server has none: 0, and the row is not shown. */
    val cliIdleMinutes: Int = 0,
    /** The Admin API key (the whole account's usage) is set. Null from an older server, which does not say. */
    val anthropicAdminKeySet: Boolean? = null,
    val spend: VoiceSpend = VoiceSpend(),
    val limitReached: Boolean = false,
    /** How loud Cru talks, 10 to 100 (%). An older server has none: 100. */
    val volume: Int = 100,
    /** Admin mode: Cru may run any command on the server. */
    val adminMode: Boolean = false,
    /** The model can hold admin mode (Sonnet or above). Null from an older server, which has no admin mode: the switch is not shown. */
    val adminCapable: Boolean? = null,
    /** Cru on its own (capability "voice-away"). Null from an older server: the section is not shown. */
    val away: VoiceAway? = null,
    /** What may be ticked under [away] (chats, kanban, notes, services, jobs). */
    val awayClasses: List<AwayClass> = emptyList(),
    // ─── Kid mode (capability "voice-kid"); an older server has none of these ───
    /** Kid Cru's voice (a Flux voice), or "" for Cru's own. Null from an older server. */
    val kidVoice: String? = null,
    /** quiet, normal or chatty. */
    val kidChattiness: String = "normal",
    /** How long kid Cru waits before answering, ms. */
    val kidPauseMs: Int = 3200,
    /** Instant cheers in games. */
    val kidReflex: Boolean = true,
    /** Improve games overnight. */
    val kidGames: KidGamesSettings? = null,
)

/** Cru on its own: switched [on], and the [AwayClass] ids it may do without asking. */
@Serializable
data class VoiceAway(val on: Boolean = false, val allow: List<String> = emptyList())

@Serializable
data class AwayClass(val id: String = "", val label: String = "")

/** POST /api/voice/away/stop: how much was paused or dropped, and the settings as they are now (null if not readable). */
data class AwayStopped(val wakeupsPaused: Int = 0, val questionsDropped: Int = 0, val jobsPaused: Int = 0, val settings: VoiceSettings? = null)

/** "Improve games overnight": on a night after kid mode was used, between [from] and [to] (HH:MM). */
@Serializable
data class KidGamesSettings(val improve: Boolean = true, val from: String = "01:00", val to: String = "05:00")

/** [VoiceSettings.volume] as a player's gain, 0.1 to 1 (anything out of range: 1, as the web). */
fun volumeGain(volume: Int?): Float = if (volume != null && volume in 10..100) volume / 100f else 1f

/** One brain and model's record (src/voice/agent.js stats()). */
@Serializable
data class BrainStat(
    val brain: String = "",
    val model: String = "",
    val turns: Int = 0,
    val failed: Int = 0,
    val medianMs: Double = 0.0,
    val p90Ms: Double = 0.0,
    val costUsd: Double = 0.0,
    val costPerTurnUsd: Double = 0.0,
    /** Of the input, the share read from the prompt cache, 0 to 1. */
    val cacheReadShare: Double = 0.0,
    val toolsPerTurn: Double = 0.0,
)

@Serializable
data class BrainStats(val ok: Boolean = false, val brain: String = "", val stats: List<BrainStat> = emptyList())

@Serializable
data class VoiceReply(
    val ok: Boolean = false,
    /** Said aloud: short (D28). */
    val speak: String = "",
    /** Shown on screen, never spoken. */
    val show: String = "",
    val tools: List<String> = emptyList(),
    val costUsd: Double = 0.0,
    val error: String = "",
    val limitReached: Boolean = false,
    /** Actions waiting for the owner's yes (Phase 4): the screen shows Yes / No. */
    val pending: List<PendingAction> = emptyList(),
    /** Something needed the phone unlocked; nothing was changed. */
    val needsUnlock: Boolean = false,
    /** For the phone to do, already confirmed by the owner (D32), e.g. background listening. */
    val phoneActions: List<PhoneAction> = emptyList(),
    /** The owner said they were done: close the voice screen once [speak] has been said. */
    val close: Boolean = false,
    /** The turn was cancelled (POST /api/voice/cancel): neither said nor shown; [acted] is what it had already done. */
    val cancelled: Boolean = false,
    val acted: List<Acted> = emptyList(),
    /** The volume to say it at, 10 to 100 (capability "voice-volume"; Cru may just have changed it). Null from an older server. */
    val volume: Int? = null,
    /** The voice check on what was said (capability "voice-speaker"); null for typed words or an older server. */
    val speaker: SpeakerCheck? = null,
    /** What the reply showed (capability "voice-reply-media"): pictures, players, files, frames, cards. */
    val media: kotlinx.serialization.json.JsonArray? = null,
)

/** GET /api/voice/unread: Cru's reports nobody heard; [line] the latest one's words. */
@Serializable
data class VoiceUnread(val ok: Boolean = false, val count: Int = 0, val latestAt: Long = 0, val line: String = "")

@Serializable
data class SpeakerCheck(val owner: Boolean? = null, val unrecognised: Boolean = false)

/** GET /api/voice/speaker. */
@Serializable
data class SpeakerStatus(
    val ok: Boolean = false,
    /** The voice check can run on this server. */
    val available: Boolean = false,
    val enrolled: Boolean = false,
    val phrases: Int = 0,
    val seconds: Double = 0.0,
    /** When the voice was enrolled (ISO), or null. */
    val enrolledAt: String? = null,
    val minPhrases: Int = 5,
    val minSeconds: Double = 0.0,
    val prompts: List<String> = emptyList(),
    /** "Only act on my voice". */
    val ownerOnly: Boolean = true,
    /** Short replies taught (capability "voice-short-words"); null from an older server. */
    val shortWords: ShortWords? = null,
    /** The short replies to say, each [wordRepeats] times; at least [minWords] of them. */
    val wordPrompts: List<String> = emptyList(),
    val wordRepeats: Int = 3,
    val minWords: Int = 5,
)

/** Whether the owner's short replies ("yes", "no"…) are taught, so a bare one is recognised. */
@Serializable
data class ShortWords(val trained: Boolean = false, val words: Int = 0, val clips: Int = 0, val at: String? = null)

/** POST /api/voice/speaker/enrol: the phrase [accepted], or the [problem] with it; [error] when it could not be checked. */
@Serializable
data class EnrolReply(
    val ok: Boolean = false,
    val accepted: Boolean = false,
    val seconds: Double = 0.0,
    /** How like the earlier phrases, 0..1; null for the first. */
    val consistency: Double? = null,
    val phrases: Int = 0,
    /** A short reply: how many times it was heard, and how like the enrolled voice (0..1). */
    val sayings: Int = 0,
    val likeYou: Double? = null,
    val problem: String = "",
    val error: String = "",
)

@Serializable
data class CompactReply(val ok: Boolean = false, val error: String = "", val skipped: String = "")

/** POST /api/voice/attach: the file as the server keeps it ([kind]: image, video, audio, pdf, text, file...). */
@Serializable
data class VoiceAttached(val ok: Boolean = false, val id: String = "", val name: String = "", val kind: String = "", val size: Long = 0)

/** One turn of Cru's conversation: what the owner [said] ("" for Cru speaking up on its own, [report]) and the answer. */
@Serializable
data class VoiceTurn(val id: Long = 0, val at: Long = 0, val said: String = "", val speak: String = "", val show: String = "", val report: Boolean = false, val media: kotlinx.serialization.json.JsonArray? = null)

/** GET /api/voice/history: [turns] oldest first; [more] older ones to page back to. */
@Serializable
data class VoiceHistory(val ok: Boolean = false, val turns: List<VoiceTurn> = emptyList(), val more: Boolean = false)

/** A ticker (on a schedule) or a trigger (on a condition): a reason for Cru to wake up on its own. */
@Serializable
data class WakeItem(
    val id: String = "",
    val type: String = "",
    val name: String = "",
    val purpose: String = "",
    val enabled: Boolean = false,
    val enabledAt: Long = 0,
    /** "cru" or "owner". */
    val createdBy: String = "",
    val createdAt: Long = 0,
    /** When it last woke Cru, ms; 0 never. */
    val lastFiredAt: Long = 0,
    val fires: Int = 0,
    val summary: String = "",
    /** Built in (the heartbeat, capability "voice-away"): can be switched off, never deleted. */
    val builtin: Boolean = false,
)

@Serializable
data class WakeLimits(val perHour: Int = 12, val firedLastHour: Int = 0, val minEvery: Int = 0, val defaultCooldown: Int = 0)

@Serializable
data class VoiceWakeups(
    val ok: Boolean = false,
    val tickers: List<WakeItem> = emptyList(),
    val triggers: List<WakeItem> = emptyList(),
    val kinds: List<String> = emptyList(),
    val limits: WakeLimits = WakeLimits(),
)

/** Something a cancelled turn had already done, which can't be undone. */
@Serializable
data class Acted(val tool: String = "", val what: String = "")

@Serializable
data class CancelReply(val ok: Boolean = false, val cancelled: Boolean = false, val acted: List<Acted> = emptyList())

/** { type: "background-listening", minutes } (0 = off). Unknown types are ignored. */
@Serializable
data class PhoneAction(val type: String = "", val minutes: Int = 0)

@Serializable
data class MemoryFile(val path: String = "", val text: String = "", val size: Long = 0)

@Serializable
data class VoiceMemory(
    val ok: Boolean = false,
    val files: List<MemoryFile> = emptyList(),
    /** Things remembered in the searchable store. */
    val entries: Int = 0,
    /** The meaning search's model: "ready", "not loaded yet", "unavailable: …". */
    val meaning: String = "",
)

@Serializable
data class MemoryHit(val kind: String = "", val at: String = "", val project: String? = null, val text: String = "")

@Serializable
data class MemorySearch(val ok: Boolean = false, val results: List<MemoryHit> = emptyList(), val meaning: Boolean = false)

@Serializable
data class PendingAction(val id: String = "", val question: String = "")

@Serializable
data class DeepgramVoice(
    val model: String = "",
    val name: String = "",
    val accent: String = "",
    val gender: String = "",
    val tags: List<String> = emptyList(),
    val sample: String = "",
    val aura2: Boolean = false,
)

@Serializable
data class VoiceList(val voices: List<DeepgramVoice> = emptyList(), val fromDeepgram: Boolean = false)

/** What /api/voice/listen sends (src/voice/stream.js). */
sealed interface ListenEvent {
    /** [echoFilter]: the server drops Cru's own words while told what it is saying (capability "voice-barge-in"), so the stream may stay on while Cru speaks. */
    data class Ready(val echoFilter: Boolean = false) : ListenEvent
    /** The owner started speaking. */
    data object Speech : ListenEvent
    /** Words so far; may still change. */
    data class Interim(val text: String) : ListenEvent
    /** A finished sentence; [owner]: the voice check (capability "voice-speaker"), null when not checked. */
    data class Utterance(val text: String, val owner: Boolean? = null) : ListenEvent
    data class Failed(val error: String) : ListenEvent
    /** Cru has news, unasked (a watched chat finished or waits on the owner): say it. */
    data class Report(val speak: String, val show: String, val media: JsonArray? = null) : ListenEvent
    /** The server dropped what the owner said ("scratch that"). */
    data class Discard(val text: String) : ListenEvent
    /** A setting changed on the server (Cru's volume, say): fetch them again. */
    data object Settings : ListenEvent
    /** Cru changed its live screen (capability "voice-screen"); null: it is gone. */
    data class Screen(val screen: LiveScreen?) : ListenEvent
    /** Kid mode changed (capability "voice-kid"). */
    data class Kid(val status: KidStatus) : ListenEvent
    /** Kid Cru tells the game on screen to do something: next, easier, harder, hint, pause, resume, end. */
    data class GameCommand(val action: String) : ListenEvent

    companion object {
        /** One message off the socket, or null for one this app does not know. */
        fun parse(text: String): ListenEvent? {
            val o = runCatching { CrundiJson.parseToJsonElement(text) as? JsonObject }.getOrNull() ?: return null
            val t = (o["text"] as? JsonPrimitive)?.contentOrNull.orEmpty()
            return when ((o["type"] as? JsonPrimitive)?.contentOrNull) {
                "ready" -> Ready((o["echoFilter"] as? JsonPrimitive)?.contentOrNull == "true")
                "speech" -> Speech
                "interim" -> Interim(t)
                "discard" -> Discard(t)
                "settings" -> Settings
                "utterance" -> if (t.isBlank()) null else Utterance(t, (o["owner"] as? JsonPrimitive)?.contentOrNull?.toBooleanStrictOrNull())
                "error" -> Failed((o["error"] as? JsonPrimitive)?.contentOrNull ?: "The stream stopped")
                "screen" -> Screen(LiveScreen.parse(o["screen"]))
                "kid" -> KidStatus.of(o["kid"])?.let { Kid(it) }
                "game-command" -> ((o["command"] as? JsonObject)?.get("action") as? JsonPrimitive)?.contentOrNull?.takeIf { it.isNotBlank() }?.let { GameCommand(it) }
                "report" -> (o["speak"] as? JsonPrimitive)?.contentOrNull.orEmpty().let { sp -> val sh = (o["show"] as? JsonPrimitive)?.contentOrNull.orEmpty(); val md = (o["media"] as? JsonArray)?.takeIf { it.isNotEmpty() }; if (sp.isBlank() && sh.isBlank() && md == null) null else Report(sp, sh, md) }
                else -> null
            }
        }
    }
}
