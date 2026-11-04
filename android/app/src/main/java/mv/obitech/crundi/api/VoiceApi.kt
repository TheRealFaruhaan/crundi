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
    suspend fun save(model: String? = null, monthlyLimitUsd: Double? = null, ttsVoice: String? = null): VoiceSettings =
        rpc.read(rpc.post("/api/voice/settings", mapOf("model" to model, "monthlyLimitUsd" to monthlyLimitUsd, "ttsVoice" to ttsVoice)), null)

    /**
     * One request. [where] is what Master is looking at ("lock screen",
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

    /** A voice session just opened: warm the server's prompt cache while Master speaks. Best effort. */
    suspend fun warm() { runCatching { rpc.post("/api/voice/warm") } }

    /** Start a fresh conversation. */
    suspend fun reset() { rpc.post("/api/voice/reset") }

    suspend fun voices(): VoiceList = rpc.read(rpc.get("/api/voice/voices"), null)

    /** [text] spoken in [voice] (or the chosen one): mp3 bytes. */
    suspend fun speak(text: String, voice: String = ""): ByteArray {
        val r = transport.exchange(ApiRequest("POST", "/api/voice/speak", json = Uploads.toJson(mapOf("text" to text, "voice" to voice.ifEmpty { null })), headers = mapOf("Accept" to "audio/mpeg")))
        if (r.status in 200..299 && r.bytes != null && r.bytes.isNotEmpty()) return r.bytes
        val err = runCatching { (CrundiJson.parseToJsonElement(r.text) as? JsonObject)?.errorText() }.getOrNull()
        throw ApiException(r.status, if (r.status == 404) UNSUPPORTED else err ?: OkHttpTransport.defaultMessage(r.status, r.text))
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
        const val UNSUPPORTED = "This server does not have the voice assistant yet. Update the server to use it."
        fun unsupported(e: Throwable) = e is ApiException && e.status == 404
    }
}

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
    val monthlyLimitUsd: Double = 50.0,
    val spend: VoiceSpend = VoiceSpend(),
    val limitReached: Boolean = false,
)

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
)

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
    data object Ready : ListenEvent
    /** Master started speaking. */
    data object Speech : ListenEvent
    /** Words so far; may still change. */
    data class Interim(val text: String) : ListenEvent
    /** A finished sentence. */
    data class Utterance(val text: String) : ListenEvent
    data class Failed(val error: String) : ListenEvent

    companion object {
        /** One message off the socket, or null for one this app does not know. */
        fun parse(text: String): ListenEvent? {
            val o = runCatching { CrundiJson.parseToJsonElement(text) as? JsonObject }.getOrNull() ?: return null
            val t = (o["text"] as? JsonPrimitive)?.contentOrNull.orEmpty()
            return when ((o["type"] as? JsonPrimitive)?.contentOrNull) {
                "ready" -> Ready
                "speech" -> Speech
                "interim" -> Interim(t)
                "utterance" -> if (t.isBlank()) null else Utterance(t)
                "error" -> Failed((o["error"] as? JsonPrimitive)?.contentOrNull ?: "The stream stopped")
                else -> null
            }
        }
    }
}
