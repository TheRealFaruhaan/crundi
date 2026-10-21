package mv.obitech.crundi.api.model

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.longOrNull
import mv.obitech.crundi.api.CrundiJson

/** What a chat is doing: the desktop's status dot. */
enum class AgentState(val wire: String) {
    Idle("idle"), Working("working"), NeedsInput("needs-input"), Waiting("waiting"), Exited("");

    companion object {
        fun of(s: String?): AgentState = when (s) {
            "working" -> Working
            "needs-input" -> NeedsInput
            "waiting" -> Waiting
            "idle" -> Idle
            else -> Exited
        }
    }
}

/** Who owns a chat or pane when it is not the owner's own. */
@Serializable data class Collaborator(val id: String = "", val name: String = "", val key: String = "")

/** A chat's place in a factory run. */
@Serializable data class FactoryRole(val runId: String = "", val role: String = "", val employeeId: String = "")

/**
 * One item of `state.terminals[]` (and /api/ui-sessions): a chat, a terminal,
 * or a parked pane. [kind] says which: "ui" (chat), "terminal", "parked".
 */
@Serializable
data class Pane(
    val id: String,
    val project: String = "",
    val title: String = "",
    val longTitle: String = "",
    val order: Int = 0,
    val kind: String = "ui",
    /** "running", "exited", or "parked". */
    val status: String = "",
    val agentState: String? = null,
    val model: String = "",
    val permissionMode: String = "",
    val skipPermissions: Boolean = false,
    val sessionId: String = "",
    val background: Boolean = false,
    /** "", "proposed", "executing", "rejected". */
    val planStatus: String = "",
    /** The permission requests and questions this chat is waiting on. */
    val pending: List<JsonObject> = emptyList(),
    val collaborator: Collaborator? = null,
    val factory: FactoryRole? = null,
    // Parked panes only
    val paneKind: String = "",
    val shellOnly: Boolean = false,
    val parkedAt: Long = 0,
    val reason: String = "",
    val hasSession: Boolean = false,
    val tokens: Long = 0,
    val heavy: Boolean = false,
) {
    val isChat get() = kind == "ui"
    val isTerminal get() = kind == "terminal"
    val isParked get() = kind == "parked"
    val state get() = AgentState.of(agentState)
    /** What to call it: the long title when there is one. */
    val label get() = longTitle.ifEmpty { title }
    val pendingEntries: List<ChatEntry> get() = pending.map { ChatEntry.from(it) }
}

/** POST /api/ui-sessions/create and /api/terminals/create answers. */
@Serializable data class Created(val id: String, val project: String = "", val title: String = "", val order: Int = 0, val kind: String = "", val forked: Boolean = false)

/** A conversation on disk that a chat can resume (GET /api/ui-sessions/resumable). */
@Serializable data class ResumableSession(val id: String, val title: String = "", val updatedAt: String = "", val sizeBytes: Long = 0, val tokens: Long = 0, val ageHours: Double = 0.0, val inUse: Boolean = false)

/** GET /api/ui-sessions/preflight: is the conversation a new chat would continue a heavy one? */
@Serializable data class ChatPreflight(val latest: ResumableSession? = null, val heavy: Boolean = false)

/** Options for a new chat (POST /api/ui-sessions/create). Nulls are left to the server's defaults. */
data class NewChat(
    val project: String,
    val title: String? = null,
    val model: String? = null,
    /** low, medium, high, xhigh, max */
    val effort: String? = null,
    /** default, acceptEdits, plan, auto, dontAsk ("" = the person's default) */
    val permissionMode: String? = null,
    val skipPermissions: Boolean? = null,
    /** continue, new, resume, compact */
    val sessionMode: String? = null,
    val resumeId: String? = null,
)

/** A slash command the chat offers (from ui-history / init). */
@Serializable data class SlashCommand(val name: String, val description: String = "", val argumentHint: String = "")

@Serializable data class ChatPlan(val text: String = "", val filePath: String = "", val at: Long = 0, val status: String = "")
@Serializable data class ChatSuggestion(val text: String = "", val uuid: String = "")
/** A message sent mid-turn, not yet taken by Claude. state: sent, queued (can be recalled), started. */
@Serializable data class PendingInjection(val text: String = "", val uuid: String = "", val state: String = "sent")

/** What a send answered: [injected] means it went in mid-turn and shows later (see [PendingInjection]). */
@Serializable data class SendResult(val injected: Boolean = false, val uuid: String = "")

/** One option of a question (AskUserQuestion). */
@Serializable data class QuestionOption(val label: String = "", val description: String = "")
@Serializable data class Question(val question: String = "", val header: String = "", val options: List<QuestionOption> = emptyList(), val multiSelect: Boolean = false)

/** A picture, video, audio clip, file or frame Claude showed with show_image/... under a tool call. */
data class ChatMedia(
    /** image, video, audio, file, embed */
    val kind: String,
    /** Server path to fetch it with the client (images, video, audio, file downloads, embed HTML). */
    val path: String? = null,
    val posterPath: String? = null,
    /** An https address to open outside the app (embeds of other sites). */
    val openUrl: String? = null,
    val caption: String = "",
    val filename: String = "",
    val size: Long = 0,
    val width: Int = 0,
    val height: Int = 0,
    val durationSec: Double = 0.0,
    val title: String = "",
    val provider: String = "",
)

/**
 * One entry of a chat's conversation. Kinds: user, assistant-text,
 * thinking, tool, permission, question, result, notice, error, gone (draw
 * nothing), goal-verdict, factory-result (a card with buttons), and any
 * other card kind Crundi adds. Fields not used by a kind are left at their
 * defaults; [raw] has everything the server sent.
 */
data class ChatEntry(
    val id: String,
    val kind: String,
    val seq: Long = 0,
    val text: String = "",
    /** Thinking: tokens so far when the reasoning text is withheld. */
    val tokens: Long = 0,
    val streaming: Boolean = false,
    /** user: who typed it when it was not the owner. */
    val by: String = "",
    // tool
    val toolUseId: String = "",
    val name: String = "",
    val input: JsonObject = JsonObject(emptyMap()),
    /** tool: running, done, unanswered. permission/question: pending, allowed, denied, unanswered. */
    val status: String = "",
    val result: String? = null,
    val isError: Boolean = false,
    // permission / question
    val requestId: String = "",
    val toolName: String = "",
    val displayName: String = "",
    val description: String = "",
    val title: String = "",
    val decisionReason: String = "",
    val requiresUserInteraction: Boolean = false,
    val suppressAlwaysAllow: Boolean = false,
    /** Has "Always allow" suggestions the CLI can write as a rule. */
    val hasSuggestions: Boolean = false,
    /** A collaborator's or employee's request, answered elsewhere. */
    val escalated: Boolean = false,
    val answeredInput: JsonObject? = null,
    // result
    val subtype: String = "",
    val costUsd: Double = 0.0,
    val durationMs: Long = 0,
    val numTurns: Int = 0,
    val raw: JsonObject = JsonObject(emptyMap()),
) {
    val isPending get() = (kind == "permission" || kind == "question") && status == "pending"

    /** The questions of a question entry (AskUserQuestion's input). */
    val questions: List<Question>
        get() = (input["questions"] as? JsonArray)?.mapNotNull { runCatching { CrundiJson.decodeFromJsonElement(Question.serializer(), it) }.getOrNull() }.orEmpty()

    /** What a tool call is about in one line: its command, file, address or pattern. */
    val summary: String
        get() {
            for (k in listOf("command", "file_path", "path", "url", "pattern", "query", "description", "prompt", "host")) {
                val v = (input[k] as? JsonPrimitive)?.contentOrNull
                if (!v.isNullOrBlank()) return v
            }
            return ""
        }

    /** What Claude showed under this call (show_image, show_video, show_audio, show_file, show_embed). */
    fun media(chatId: String): List<ChatMedia> = ChatMediaParser.parse(chatId, this)

    /** The same entry with a patch from the server applied. */
    fun patched(patch: JsonObject): ChatEntry = from(JsonObject(raw + patch))

    companion object {
        fun from(o: JsonObject): ChatEntry {
            fun s(k: String) = (o[k] as? JsonPrimitive)?.contentOrNull ?: ""
            fun b(k: String) = (o[k] as? JsonPrimitive)?.booleanOrNull ?: false
            fun l(k: String) = (o[k] as? JsonPrimitive)?.let { it.longOrNull ?: it.doubleOrNull?.toLong() } ?: 0L
            fun d(k: String) = (o[k] as? JsonPrimitive)?.doubleOrNull ?: 0.0
            val result = o["result"]
            return ChatEntry(
                id = s("id"), kind = s("kind"), seq = l("seq"), text = s("text"), tokens = l("tokens"), streaming = b("streaming"), by = s("by"),
                toolUseId = s("toolUseId"), name = s("name"), input = o["input"] as? JsonObject ?: JsonObject(emptyMap()),
                status = s("status"),
                result = when (result) { null -> null; is JsonPrimitive -> result.contentOrNull; else -> result.toString() },
                isError = b("isError"),
                requestId = s("requestId"), toolName = s("toolName"), displayName = s("displayName"), description = s("description"), title = s("title"),
                decisionReason = s("decisionReason"), requiresUserInteraction = b("requiresUserInteraction"), suppressAlwaysAllow = b("suppressAlwaysAllow"),
                hasSuggestions = (o["suggestions"] as? JsonArray)?.isNotEmpty() == true,
                escalated = b("escalated"), answeredInput = o["answeredInput"] as? JsonObject,
                subtype = s("subtype"), costUsd = d("costUsd"), durationMs = l("durationMs"), numTurns = l("numTurns").toInt(),
                raw = o,
            )
        }
    }
}

/** A subagent or background task bubble under a chat. */
data class ChatAgent(
    val toolUseId: String,
    /** agent or task */
    val kind: String = "agent",
    val description: String = "",
    val subagentType: String = "",
    val prompt: String = "",
    /** running, completed, failed, stopped, ... */
    val status: String = "",
    /** The step it is on now ("Reading README.md"). */
    val step: String = "",
    val lastTool: String = "",
    val summary: String = "",
    val endedAt: Long = 0,
    val dismissed: Boolean = false,
    val count: Int = 0,
    /** Its own transcript, filled by agent-entry events (and on history). */
    val messages: List<ChatEntry> = emptyList(),
    val raw: JsonObject = JsonObject(emptyMap()),
) {
    val running get() = status == "running"

    companion object {
        fun from(o: JsonObject, keep: List<ChatEntry> = emptyList()): ChatAgent {
            fun s(k: String) = (o[k] as? JsonPrimitive)?.contentOrNull ?: ""
            val msgs = (o["messages"] as? JsonArray)?.mapNotNull { (it as? JsonObject)?.let(ChatEntry::from) } ?: keep
            return ChatAgent(
                toolUseId = s("toolUseId"), kind = s("kind").ifEmpty { "agent" }, description = s("description"), subagentType = s("subagentType"),
                prompt = s("prompt"), status = s("status"), step = s("step"), lastTool = s("lastTool"), summary = s("summary"),
                endedAt = (o["endedAt"] as? JsonPrimitive)?.let { it.longOrNull ?: it.doubleOrNull?.toLong() } ?: 0L,
                dismissed = (o["dismissed"] as? JsonPrimitive)?.booleanOrNull ?: false,
                count = (o["count"] as? JsonPrimitive)?.longOrNull?.toInt() ?: msgs.size,
                messages = msgs, raw = o,
            )
        }
    }
}

/**
 * Everything a chat screen shows, kept up to date by [ChatReducer] from the
 * socket's ui-history and ui-event messages.
 */
data class ChatState(
    val id: String,
    /** False until the first ui-history arrives. */
    val loaded: Boolean = false,
    val project: String = "",
    val title: String = "",
    /** running or exited */
    val status: String = "",
    val state: AgentState = AgentState.Idle,
    val sessionId: String = "",
    val model: String = "",
    val permissionMode: String = "",
    val skipPermissions: Boolean = false,
    val slashCommands: List<SlashCommand> = emptyList(),
    val totalCostUsd: Double = 0.0,
    val cwd: String = "",
    val messages: List<ChatEntry> = emptyList(),
    /** Subagent and task bubbles by tool use id, in the order they appeared. */
    val agents: Map<String, ChatAgent> = emptyMap(),
    val goal: JsonObject? = null,
    val plan: ChatPlan? = null,
    val suggestion: ChatSuggestion? = null,
    val pendingInjections: List<PendingInjection> = emptyList(),
    val factory: FactoryRole? = null,
    /** Set when the Claude process ended (exit code). */
    val exitCode: Int? = null,
) {
    val busy get() = state == AgentState.Working || pendingInjections.isNotEmpty()
    val exited get() = status == "exited" || exitCode != null
    /** Entries worth drawing (not "gone"). */
    val visible get() = messages.filter { it.kind != "gone" }
    /** The permission requests and questions waiting on the person, oldest first. */
    val waiting get() = messages.filter { it.isPending }
}

internal fun slashCommandsOf(el: JsonElement?): List<SlashCommand> = (el as? JsonArray)?.mapNotNull {
    when (it) {
        is JsonPrimitive -> it.contentOrNull?.let { n -> SlashCommand(n.removePrefix("/")) }
        is JsonObject -> (it["name"] as? JsonPrimitive)?.contentOrNull?.let { n ->
            SlashCommand(n.removePrefix("/"), (it["description"] as? JsonPrimitive)?.contentOrNull ?: "", (it["argumentHint"] as? JsonPrimitive)?.contentOrNull ?: "")
        }
        else -> null
    }
}.orEmpty()

/** Reads the result of a show_* tool call into [ChatMedia], as the desktop's toolMedia does. */
object ChatMediaParser {
    private val NAME = Regex("^[0-9a-f]{16}([.]poster)?[.](png|jpg|gif|webp|mp4|webm|mov|mp3|wav|ogg|m4a|flac|weba)$")
    private val EMBED = Regex("^[0-9a-f]{16}[.]embed$")
    private val ID = Regex("^[0-9a-f]{16}$")
    private val TOOL = Regex("show_(image|video|audio|file|embed)$")

    fun parse(chatId: String, e: ChatEntry): List<ChatMedia> {
        if (e.kind != "tool" || e.status != "done" || e.isError || !TOOL.containsMatchIn(e.name)) return emptyList()
        val r = runCatching { CrundiJson.parseToJsonElement(e.result ?: "").jsonObject }.getOrNull() ?: return emptyList()
        val items = (r["items"] as? JsonArray) ?: (r["images"] as? JsonArray)?.let { arr -> JsonArray(arr.map { JsonObject((it as JsonObject) + ("kind" to JsonPrimitive("image"))) }) } ?: return emptyList()
        val sid = java.net.URLEncoder.encode(chatId, "UTF-8")
        fun media(name: String) = "/api/chat-media?session=$sid&name=${java.net.URLEncoder.encode(name, "UTF-8")}"
        return items.mapNotNull { el ->
            val i = el as? JsonObject ?: return@mapNotNull null
            fun s(k: String) = (i[k] as? JsonPrimitive)?.contentOrNull ?: ""
            fun n(k: String) = (i[k] as? JsonPrimitive)?.doubleOrNull ?: 0.0
            val base = ChatMedia(kind = s("kind"), caption = s("caption").take(200), filename = s("filename").take(200), size = n("size").toLong(),
                width = n("w").toInt(), height = n("h").toInt(), durationSec = n("duration"))
            when (s("kind")) {
                "embed" -> {
                    val open = s("url").takeIf { it.startsWith("https://") }
                    if (s("name").isNotEmpty()) {
                        if (!EMBED.matches(s("name"))) null else base.copy(path = media(s("name")), openUrl = open, title = s("title"), provider = s("provider"), height = n("height").toInt())
                    } else base.copy(openUrl = open ?: s("src").takeIf { it.startsWith("https://") }, title = s("title"), provider = s("provider"), height = n("height").toInt())
                }
                "file" -> if (!ID.matches(s("id"))) null else base.copy(path = "/api/chat-media/file?session=$sid&id=${s("id")}")
                "image", "video", "audio" -> if (!NAME.matches(s("name"))) null else base.copy(
                    path = media(s("name")),
                    posterPath = s("poster").takeIf { NAME.matches(it) }?.let { media(it) },
                )
                else -> null
            }
        }
    }
}
