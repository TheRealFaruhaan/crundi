package mv.obitech.crundi.api

import kotlinx.serialization.Serializable
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
    suspend fun save(model: String? = null, monthlyLimitUsd: Double? = null, ttsVoice: String? = null, billingDay: Int? = null, deepgramBillingDay: Int? = null, pauseMs: Int? = null, deepgramLimitUsd: Double? = null, speechRate: Double? = null, screenCloseSeconds: Int? = null, brain: String? = null, cliIdleMinutes: Int? = null): VoiceSettings =
        rpc.read(rpc.post("/api/voice/settings", mapOf("model" to model, "monthlyLimitUsd" to monthlyLimitUsd, "ttsVoice" to ttsVoice, "billingDay" to billingDay, "deepgramBillingDay" to deepgramBillingDay, "pauseMs" to pauseMs, "deepgramLimitUsd" to deepgramLimitUsd, "speechRate" to speechRate, "screenCloseSeconds" to screenCloseSeconds, "brain" to brain, "cliIdleMinutes" to cliIdleMinutes)), null)

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
    suspend fun turn(text: String, where: String, locked: Boolean): VoiceReply {
        // A failed turn comes back { ok:false, error } with 200: read it as a reply, not an exception.
        val r = transport.exchange(ApiRequest("POST", "/api/voice/turn", json = Uploads.toJson(mapOf("text" to text, "where" to where, "locked" to locked)), slow = true))
        if (r.status == 404) throw ApiException(404, UNSUPPORTED)
        val obj = runCatching { CrundiJson.parseToJsonElement(r.text) as? JsonObject }.getOrNull()
            ?: throw ApiException(r.status, OkHttpTransport.defaultMessage(r.status, r.text))
        if (r.status == 401) throw ApiException(401, OkHttpTransport.defaultMessage(401))
        return CrundiJson.decodeFromJsonElement(VoiceReply.serializer(), obj)
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

    companion object {
        const val CAPABILITY = "voice-assistant"
        const val TTS_STREAM = "voice-tts-stream"
        /** The notebook and its search (Phase 2). */
        const val MEMORY = "voice-memory"
        /** A turn's reply may carry [VoiceReply.phoneActions] (Phase 6). */
        const val PHONE_ACTIONS = "voice-phone-actions"
        /** POST /api/voice/cancel, { type: 'discard' } both ways on the stream. */
        const val CANCEL = "voice-cancel"
        /** GET /api/voice/cues and its clips. */
        const val CUES = "voice-cues"
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
data class VoiceModel(val id: String = "", val label: String = "")

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
)

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
    /** A finished sentence. */
    data class Utterance(val text: String) : ListenEvent
    data class Failed(val error: String) : ListenEvent
    /** Cru has news, unasked (a watched chat finished or waits on the owner): say it. */
    data class Report(val speak: String, val show: String) : ListenEvent
    /** The server dropped what the owner said ("scratch that"). */
    data class Discard(val text: String) : ListenEvent

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
                "utterance" -> if (t.isBlank()) null else Utterance(t)
                "error" -> Failed((o["error"] as? JsonPrimitive)?.contentOrNull ?: "The stream stopped")
                "report" -> (o["speak"] as? JsonPrimitive)?.contentOrNull.orEmpty().let { sp -> val sh = (o["show"] as? JsonPrimitive)?.contentOrNull.orEmpty(); if (sp.isBlank() && sh.isBlank()) null else Report(sp, sh) }
                else -> null
            }
        }
    }
}
