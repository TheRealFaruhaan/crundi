package mv.obitech.crundi.api

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.longOrNull

/**
 * Cru's live screen (capability "voice-screen", src/voice/screen.js): one
 * panel on the voice screen that Cru fills and changes as the conversation
 * goes. Read by hand, field by field, so a block this app does not know (a
 * newer server's) is left out rather than failing the whole panel.
 *
 * [doc] is html (Cru's own page, or a built-in game) already wrapped by the
 * server for a sandboxed frame: its CSP allows no network at all. [video] is
 * a youtube-nocookie embed, only ever built by the server from an approved id.
 */
data class LiveScreen(
    val title: String = "",
    val blocks: List<ScreenBlock> = emptyList(),
    val doc: String = "",
    /** The built-in game on screen (its id), or "". */
    val game: String = "",
    val video: ScreenVideo? = null,
    /** Shown while kid mode was on. */
    val kid: Boolean = false,
    /** Cru's UI takes the whole content area (capability "voice-screen-full"); the top bar's "Normal view" goes back. */
    val full: Boolean = false,
    val rev: Long = 0,
) {
    /** The video's player address, only when it is a youtube-nocookie embed (anything else is not shown). */
    val videoUrl: String? get() = video?.embed?.takeIf { ScreenFilter.isNocookieEmbed(it) }

    /** Something to draw in a frame (a page, a game, a video) rather than blocks alone. */
    val framed: Boolean get() = videoUrl != null || doc.isNotEmpty()

    companion object {
        /** `screen` of GET /api/voice/screen or of a { type: 'screen' } message; null for none (or nothing readable). */
        fun parse(el: JsonElement?): LiveScreen? {
            val o = el as? JsonObject ?: return null
            val v = o["video"] as? JsonObject
            val s = LiveScreen(
                title = o.ls("title"),
                blocks = (o["blocks"] as? JsonArray).orEmpty().mapNotNull { ScreenBlock.parse(it) },
                doc = o.ls("doc"),
                game = (o["game"] as? JsonObject)?.ls("id").orEmpty(),
                video = v?.let { ScreenVideo(it.ls("kind"), it.ls("id"), it.ls("title"), it.ls("embed")) },
                kid = (o["kid"] as? JsonPrimitive)?.booleanOrNull == true,
                full = (o["full"] as? JsonPrimitive)?.booleanOrNull == true,
                rev = (o["rev"] as? JsonPrimitive)?.longOrNull ?: 0,
            )
            return s
        }
    }
}

data class ScreenVideo(val kind: String = "", val id: String = "", val title: String = "", val embed: String = "")

/** One of Cru's blocks, drawn natively in Crundi's style. */
sealed interface ScreenBlock {
    data class Heading(val text: String) : ScreenBlock
    data class Text(val text: String) : ScreenBlock
    data class Items(val items: List<String>, val ordered: Boolean) : ScreenBlock
    data class Checklist(val items: List<Check>) : ScreenBlock
    data class Table(val headers: List<String>, val rows: List<List<String>>) : ScreenBlock
    /** [value] 0..100. */
    data class Progress(val label: String, val value: Double) : ScreenBlock
    /** [url] https: or data:image. */
    data class Image(val url: String, val alt: String) : ScreenBlock
    data class Chips(val items: List<Chip>) : ScreenBlock

    data class Check(val text: String, val done: Boolean)
    /** [tone] ok, warn, bad, info or muted. */
    data class Chip(val text: String, val tone: String)

    companion object {
        val TONES = setOf("ok", "warn", "bad", "info", "muted")

        /** A block, or null for a type this app does not draw (or one with nothing to draw). */
        fun parse(el: JsonElement): ScreenBlock? {
            val o = el as? JsonObject ?: return null
            fun texts(key: String) = (o[key] as? JsonArray).orEmpty().map { x -> (x as? JsonObject)?.ls("text") ?: x.lt() }
            return when (o.ls("type")) {
                "heading" -> Heading(o.ls("text"))
                "text" -> Text(o.ls("text"))
                "list" -> Items(texts("items"), (o["ordered"] as? JsonPrimitive)?.booleanOrNull == true)
                "checklist" -> Checklist((o["items"] as? JsonArray).orEmpty().map { x ->
                    val c = x as? JsonObject
                    if (c != null) Check(c.ls("text"), (c["done"] as? JsonPrimitive)?.booleanOrNull == true) else Check(x.lt(), false)
                })
                "table" -> Table(
                    (o["headers"] as? JsonArray).orEmpty().map { it.lt() },
                    (o["rows"] as? JsonArray).orEmpty().map { r -> (r as? JsonArray)?.map { it.lt() } ?: listOf(r.lt()) },
                )
                "progress" -> Progress(o.ls("label"), ((o["value"] as? JsonPrimitive)?.doubleOrNull ?: 0.0).coerceIn(0.0, 100.0))
                "image" -> o.ls("url").takeIf { it.startsWith("https://", true) || it.startsWith("data:image/", true) }?.let { Image(it, o.ls("alt")) }
                "chips" -> Chips((o["items"] as? JsonArray).orEmpty().map { x ->
                    val c = x as? JsonObject
                    if (c != null) Chip(c.ls("text"), c.ls("tone").takeIf { it in TONES } ?: "info") else Chip(x.lt(), "info")
                })
                else -> null
            }
        }
    }
}

private fun JsonObject.ls(key: String): String = (this[key] as? JsonPrimitive)?.takeIf { it !is JsonNull }?.contentOrNull.orEmpty()
private fun JsonElement.lt(): String = (this as? JsonPrimitive)?.takeIf { it !is JsonNull }?.contentOrNull.orEmpty()

/**
 * What the live screen's frames may load. Cru's page and the games carry a
 * CSP of no network; this is the second wall, in the WebView itself:
 *
 *   a page or a game  data: and blob: only, nothing over the network
 *   a video           the same, plus youtube-nocookie and the hosts its
 *                     player streams and draws from (ytimg, googlevideo)
 *
 * Pure (no Android), so it is unit-tested.
 */
object ScreenFilter {
    private val NOCOOKIE = Regex("^https://www\\.youtube-nocookie\\.com/embed/[A-Za-z0-9_-]+([?/].*)?$")
    /** What a youtube-nocookie player needs: its own host, its pictures and its streams. */
    private val VIDEO_HOSTS = listOf("youtube-nocookie.com", "ytimg.com", "googlevideo.com", "ggpht.com")

    fun isNocookieEmbed(url: String): Boolean = NOCOOKIE.matches(url)

    /** May a frame showing a video ([video]) or a page/game load [url]? */
    fun allows(url: String, video: Boolean): Boolean {
        val u = url.trim()
        val scheme = u.substringBefore(':', "").lowercase()
        if (scheme == "data" || scheme == "blob") return true
        if (u.startsWith("about:blank", true)) return true
        if (!video || scheme != "https") return false
        val host = runCatching { java.net.URI(u).host }.getOrNull()?.lowercase() ?: return false
        return VIDEO_HOSTS.any { host == it || host.endsWith(".$it") }
    }

    /**
     * A message the frame posted (cru.say, cru.tell, a game's event), as JSON
     * text, read into what the voice screen does with it; null for anything
     * else (including the volume and commands this side posts into it).
     */
    fun message(json: String): FrameMessage? {
        val o = runCatching { CrundiJson.parseToJsonElement(json) as? JsonObject }.getOrNull() ?: return null
        val cru = o.ls("cru")
        val clip = { s: String -> s.take(300) }
        return when {
            cru == "say" -> clip(o.ls("text")).takeIf { it.isNotBlank() }?.let { FrameMessage.Say(it) }
            cru == "tell" -> clip(o.ls("text")).takeIf { it.isNotBlank() }?.let { FrameMessage.Tell(it) }
            o.ls("type") == "game" -> o.ls("event").takeIf { it.isNotEmpty() }?.let {
                FrameMessage.Game(buildMap<String, Any?> {
                    put("event", it)
                    o["item"]?.let { v -> put("item", v.lt()) }
                    o["detail"]?.let { v -> put("detail", v.lt()) }
                    (o["streak"] as? JsonPrimitive)?.doubleOrNull?.let { v -> put("streak", v) }
                })
            }
            cru == "event" -> {
                val kind = o.ls("kind")
                if (kind.isEmpty()) null
                else if (kind == "result") FrameMessage.Game(buildMap<String, Any?> {
                    put("event", "result"); put("skill", o.ls("skill")); put("item", o.ls("item"))
                    put("ok", (o["ok"] as? JsonPrimitive)?.booleanOrNull == true)
                    (o["ms"] as? JsonPrimitive)?.doubleOrNull?.let { put("ms", it) }
                })
                else FrameMessage.Game(mapOf("event" to (if (kind == "wrong") "miss" else if (kind == "level") "level-up" else kind), "item" to o.ls("detail"), "detail" to o.ls("detail")))
            }
            else -> null
        }
    }
}

/** What Cru's page or game asked for. */
sealed interface FrameMessage {
    /** Say [text] now (a letter, a picture's name), unless Cru is talking. */
    data class Say(val text: String) : FrameMessage
    /** Something tapped that Cru should hear: POST /api/voice/screen/event. */
    data class Tell(val text: String) : FrameMessage
    /** A game event for POST /api/voice/kid/play, as the web sends it. */
    data class Game(val body: Map<String, Any?>) : FrameMessage
}

// ─── Kid mode (capability "voice-kid", src/voice/kid.js) ───

@Serializable
data class KidChoice(val id: String = "", val label: String = "")

/** Kid Cru asking the parent: [choices] are the answers it offers (Allow once / Always allow / No …). */
@Serializable
data class KidAsk(val id: String = "", val kind: String = "", val title: String = "", val choices: List<KidChoice> = emptyList(), val createdAt: Long = 0)

/** GET /api/voice/kid, SSE 'voice-kid', { type: 'kid', kid } on the voice stream. */
@Serializable
data class KidStatus(
    val on: Boolean = false,
    /** When it was switched on (ms), 0 when off. */
    val since: Long = 0,
    val pinSet: Boolean = false,
    /** Where it was started: settings, tray, screen. */
    val by: String = "",
    val paused: Boolean = false,
    val extraMinutes: Int = 0,
    /** What is on the kid screen (its title), or "". */
    val playing: String = "",
    val asks: List<KidAsk> = emptyList(),
    /** The parent's instructions (kid-mode.md); only while kid mode is off. */
    val instructions: String = "",
    /** What the child calls the parent; only while kid mode is off. */
    val parentName: String = "",
) {
    /** Minutes since it was switched on, at [now]. */
    fun minutes(now: Long): Long = if (on && since > 0) ((now - since) / 60_000L).coerceAtLeast(0) else 0

    companion object {
        /** A status from an event's data (or a message's `kid`); null when it isn't one. */
        fun of(el: JsonElement?): KidStatus? {
            val o = el as? JsonObject ?: return null
            if (o["on"] == null) return null
            return runCatching { CrundiJson.decodeFromJsonElement(serializer(), o) }.getOrNull()
        }
    }
}

/** The PC lock's auto lock: after [minutes] idle (or when the computer itself locks), as [mode] (full or visible). */
@Serializable
data class PcAutoLock(val on: Boolean = false, val minutes: Int = 10, val mode: String = "full")

/**
 * The PC lock (capability "pc-lock"): the PC app's screen locked behind the
 * owner's PIN (the same PIN as kid mode). GET /api/lock, SSE 'pc-lock'.
 */
@Serializable
data class PcLock(
    val on: Boolean = false,
    /** visible (see-through overlay) or full (opaque). */
    val mode: String = "full",
    /** When it was locked (ms), 0 when unlocked. */
    val since: Long = 0,
    /** Where it was locked: settings, tray, shortcut, phone. */
    val by: String = "",
    val pinSet: Boolean = false,
    /** Locking by itself when the PC is idle; null from a server without it. */
    val auto: PcAutoLock? = null,
    /** The idle times offered, in minutes. */
    val autoMinutes: List<Int> = listOf(1, 2, 5, 10, 15, 30, 60),
) {
    companion object {
        /** A lock state from an event's data or a reply; null when it isn't one. */
        fun of(el: JsonElement?): PcLock? {
            val o = el as? JsonObject ?: return null
            if (o["on"] == null) return null
            return runCatching { CrundiJson.decodeFromJsonElement(serializer(), o) }.getOrNull()
        }
    }
}

@Serializable
data class KidPoint(val date: String = "", val level: Double = 0.0, val mastery: Double = 0.0, val plays: Int = 0)

@Serializable
data class KidArea(
    val id: String = "",
    val label: String = "",
    val about: String = "",
    /** 1 to 5; 0 nothing measured. */
    val level: Double = 0.0,
    /** up, down, steady or none. */
    val trend: String = "none",
    val series: List<KidPoint> = emptyList(),
    val learning: List<String> = emptyList(),
    val mastered: List<String> = emptyList(),
) {
    /** Played at all in this range (the web lists only these). */
    val played: Boolean get() = level > 0 || series.isNotEmpty()
}

@Serializable
data class PlayDay(val date: String = "", val minutes: Double = 0.0)

/** GET /api/voice/kid/progress?range=week|month|all&area=. */
@Serializable
data class KidProgress(
    val range: String = "month",
    val areas: List<KidArea> = emptyList(),
    val playTime: List<PlayDay> = emptyList(),
    val notes: List<String> = emptyList(),
) {
    val totalMinutes: Long get() = Math.round(playTime.sumOf { it.minutes })
}

@Serializable
data class KidGame(
    val id: String = "",
    val title: String = "",
    val skills: List<String> = emptyList(),
    val ageRange: String = "",
    val version: Int = 1,
    /** builtin or workshop. */
    val createdBy: String = "",
    val enabled: Boolean = true,
    val isNew: Boolean = false,
    /** A version to roll back to. */
    val hasPrev: Boolean = false,
    val changedByWorkshop: Boolean = false,
)

@Serializable
data class KidWorkshopRun(
    val at: JsonElement? = null,
    val created: List<JsonElement> = emptyList(),
    val updated: List<JsonElement> = emptyList(),
    val failed: List<JsonElement> = emptyList(),
    val summary: String = "",
) {
    val atMs: Long get() = mv.obitech.crundi.api.model.timeMs(at)
}

@Serializable
data class KidWorkshop(val runs: List<KidWorkshopRun> = emptyList(), val running: Boolean = false)

/** GET/POST /api/voice/kid/games. */
@Serializable
data class KidGames(val games: List<KidGame> = emptyList(), val workshop: KidWorkshop? = null)

/**
 * Kid mode, the parent's side (capability "voice-kid"), and the live
 * screen's calls (capability "voice-screen"). Every call is only made on a
 * server that lists its capability; an older one answering 404 anyway comes
 * back as null or [UNSUPPORTED], never a crash.
 */
class KidApi internal constructor(private val rpc: Rpc, private val transport: Transport) {

    // ─── the live screen ───

    /** What is on Cru's live screen (null: nothing, or an older server). */
    suspend fun screen(): LiveScreen? {
        val r = transport.exchange(ApiRequest("GET", "/api/voice/screen"))
        if (r.status !in 200..299) return null
        val o = runCatching { CrundiJson.parseToJsonElement(r.text) as? JsonObject }.getOrNull() ?: return null
        return LiveScreen.parse(o["screen"])
    }

    /** The X: the panel goes for every screen. */
    suspend fun closeScreen() { runCatching { transport.exchange(ApiRequest("POST", "/api/voice/screen/close", json = Uploads.toJson(emptyMap<String, Any?>()))) } }

    /** "Normal view": out of Cru's full UI for every screen, the content kept as the panel (only ever shown when the server sent full). */
    suspend fun normalScreen() { runCatching { transport.exchange(ApiRequest("POST", "/api/voice/screen/normal", json = Uploads.toJson(emptyMap<String, Any?>()))) } }

    /** Something tapped on Cru's page (cru.tell): answered as a turn. */
    suspend fun screenEvent(text: String): VoiceReply {
        val r = transport.exchange(ApiRequest("POST", "/api/voice/screen/event", json = Uploads.toJson(mapOf("text" to text)), slow = true))
        val o = runCatching { CrundiJson.parseToJsonElement(r.text) as? JsonObject }.getOrNull() ?: return VoiceReply(ok = false, error = OkHttpTransport.defaultMessage(r.status, r.text))
        return runCatching { CrundiJson.decodeFromJsonElement(VoiceReply.serializer(), o) }.getOrElse { VoiceReply(ok = false) }
    }

    /** A game's event: the play log and levels; [KidReflex] the instant line to say, if any. */
    suspend fun play(body: Map<String, Any?>): KidReflex? {
        val r = transport.exchange(ApiRequest("POST", "/api/voice/kid/play", json = Uploads.toJson(body)))
        if (r.status !in 200..299) return null
        val o = runCatching { CrundiJson.parseToJsonElement(r.text) as? JsonObject }.getOrNull() ?: return null
        val x = o["reflex"] as? JsonObject ?: return null
        return KidReflex(x.ls("text"), (x["expressivity"] as? JsonPrimitive)?.doubleOrNull ?: 1.0).takeIf { it.text.isNotBlank() }
    }

    // ─── the PC lock (capability "pc-lock") ───

    /** The PC lock now; null from a server without it (404). */
    suspend fun lock(): PcLock? {
        val r = transport.exchange(ApiRequest("GET", "/api/lock"))
        if (r.status == 404) return null
        if (r.status !in 200..299) throw ApiException(r.status, OkHttpTransport.defaultMessage(r.status, r.text))
        return PcLock.of(CrundiJson.parseToJsonElement(r.text)) ?: PcLock()
    }

    /** Lock the PC: [mode] visible or full. Needs the PIN set. */
    suspend fun lockStart(mode: String): PcLock {
        require(mode == "visible" || mode == "full") { "Not a lock mode: $mode" }
        return PcLock.of(rpc.post("/api/lock/start", mapOf("mode" to mode, "by" to "phone"))) ?: PcLock(on = true, mode = mode, pinSet = true)
    }

    /** The auto lock saved; the whole lock comes back. Turning it on needs the PIN set. */
    suspend fun lockAuto(auto: PcAutoLock): PcLock {
        require(auto.mode == "visible" || auto.mode == "full") { "Not a lock mode: ${auto.mode}" }
        return PcLock.of(rpc.post("/api/lock/auto", mapOf("on" to auto.on, "minutes" to auto.minutes, "mode" to auto.mode))) ?: PcLock(auto = auto)
    }

    /** Unlocked from this phone (the owner is signed in: no PIN). */
    suspend fun lockEnd(): PcLock = PcLock.of(rpc.post("/api/lock/end")) ?: PcLock(pinSet = true)

    // ─── kid mode ───

    /** Kid mode now; null from a server without it (404). */
    suspend fun status(): KidStatus? {
        val r = transport.exchange(ApiRequest("GET", "/api/voice/kid"))
        if (r.status == 404) return null
        if (r.status !in 200..299) throw ApiException(r.status, OkHttpTransport.defaultMessage(r.status, r.text))
        return KidStatus.of(CrundiJson.parseToJsonElement(r.text)) ?: KidStatus()
    }

    /** Set or change the PIN (4 to 8 digits); kid mode must be off. */
    suspend fun setPin(pin: String): KidStatus = KidStatus.of(rpc.post("/api/voice/kid/pin", mapOf("pin" to pin))) ?: KidStatus(pinSet = true)
    suspend fun start(): KidStatus = KidStatus.of(rpc.post("/api/voice/kid/start", mapOf("by" to "settings"))) ?: KidStatus(on = true)
    /** Ended from this phone (the parent is signed in: no PIN). */
    suspend fun end() { rpc.post("/api/voice/kid/end") }
    suspend fun pause(paused: Boolean): KidStatus? = KidStatus.of(rpc.post("/api/voice/kid/pause", mapOf("paused" to paused)))
    /** [minutes] 1 to 240 more. */
    suspend fun addTime(minutes: Int): KidStatus? = KidStatus.of(rpc.post("/api/voice/kid/time", mapOf("minutes" to minutes)))
    suspend fun parentName(name: String) { rpc.post("/api/voice/kid/parent-name", mapOf("name" to name)) }
    /** The answer to kid Cru's question: one of its [KidAsk.choices] ids. */
    suspend fun answer(id: String, choice: String) { rpc.post("/api/voice/kid/ask", mapOf("id" to id, "choice" to choice)) }

    suspend fun progress(range: String = "month", area: String = ""): KidProgress =
        rpc.read(rpc.get("/api/voice/kid/progress", if (area.isEmpty()) mapOf("range" to range) else mapOf("range" to range, "area" to area)), null)
    suspend fun resetProgress() { rpc.post("/api/voice/kid/progress/reset") }

    suspend fun games(): KidGames = rpc.read(rpc.get("/api/voice/kid/games"), null)
    /** [action]: enable, disable, rollback or seen. */
    suspend fun game(id: String, action: String): KidGames {
        require(action in setOf("enable", "disable", "rollback", "seen")) { "Not a game action: $action" }
        return rpc.read(rpc.post("/api/voice/kid/games", mapOf("id" to id, "action" to action)), null)
    }

    /** What kid Cru remembers about the child (friend.md). */
    suspend fun friend(): String = (rpc.get("/api/voice/kid/friend")["text"] as? JsonPrimitive)?.contentOrNull.orEmpty()
    suspend fun forgetFriend() { rpc.delete("/api/voice/kid/friend") }

    companion object {
        const val SCREEN = "voice-screen"
        const val KID = "voice-kid"
        const val LOCK = "pc-lock"
        /** Kid Cru's voices (Deepgram Flux), as the web offers them; "" the same as Cru's. */
        val VOICES = listOf("flux-gemma-en" to "Gemma (lively)", "flux-heather-en" to "Heather", "flux-brooke-en" to "Brooke", "flux-meghan-en" to "Meghan", "flux-cole-en" to "Cole", "" to "Same as Cru")
        val CHATTINESS = listOf("quiet" to "Quiet", "normal" to "Normal", "chatty" to "Chatty")
        val WAITS = listOf(2500, 3200, 4000, 5000)
        val PIN = Regex("^[0-9]{4,8}$")
    }
}

/** The instant line a game event earned (kid mode). */
data class KidReflex(val text: String, val expressivity: Double = 1.0)
