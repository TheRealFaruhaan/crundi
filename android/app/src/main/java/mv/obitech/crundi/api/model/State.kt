package mv.obitech.crundi.api.model

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.longOrNull

/** A time the server sends either as epoch ms or as ISO text: epoch ms, or 0. */
fun timeMs(el: JsonElement?): Long {
    val p = el as? JsonPrimitive ?: return 0
    p.longOrNull?.let { return if (it in 1..999_999_999_999) it * 1000 else it }
    p.doubleOrNull?.let { return it.toLong() }
    val s = p.contentOrNull ?: return 0
    return runCatching { java.time.Instant.parse(s).toEpochMilli() }.getOrDefault(0)
}

/** One project (GET /api/projects, and state.projects). [alias] is its key everywhere. */
@Serializable
data class Project(
    val alias: String,
    val name: String = "",
    val path: String = "",
    /** "/api/project-icon?project=..&v=..", or "" for none. Fetch with the client's images. */
    val icon: String = "",
    /** Present for registered projects (may be ""), absent for auto-discovered ones. */
    val systemPrompt: String? = null,
) {
    val label get() = name.ifEmpty { alias }
    /** Registered (can be removed and given a prompt); auto-discovered ones cannot be removed. */
    val registered get() = systemPrompt != null
}

/** GET /api/projects/config: "single" mode makes new projects under [projectsDir] by alias alone. */
@Serializable data class ProjectsConfig(val mode: String = "multi", val projectsDir: String = "")

/** A service as the state push carries it (GET /api/services adds cpu and containers). */
@Serializable
data class Service(
    val key: String,
    val name: String = "",
    val alias: String = "",
    val command: String = "",
    val projectPath: String = "",
    /** running, stopped, crashed */
    val status: String = "stopped",
    val pid: Int? = null,
    /** Bytes. */
    val memory: Long? = null,
    val cpuPct: Double? = null,
    val containers: List<ServiceContainer>? = null,
    val uptime: String? = null,
    val tunnelPort: Int = 0,
    val tunnelEnabled: Boolean = false,
    /** connecting, active, error, stopped */
    val tunnelStatus: String? = null,
    val tunnelUrl: String? = null,
    val createdBy: String = "",
    val byCollaborator: Boolean = false,
    val mine: Boolean = false,
) {
    val running get() = status == "running"
}

@Serializable data class ServiceContainer(val name: String = "", val service: String = "", val rssBytes: Long? = null, val cpuPct: Double? = null)

/** An unfinished factory run, for badges (state.factoryRuns). */
@Serializable data class FactoryRunBrief(val id: String, val project: String = "", val status: String = "", val waitingOn: String = "", val factoryName: String = "", val managerChatId: String = "")

/** A collaborator's own view (state.collab), only for a collaborator's login. */
@Serializable data class CollabInfo(val name: String = "", val projects: List<String> = emptyList(), val expiresAt: Long = 0, val usage: JsonObject? = null)

/**
 * The whole workbench as the server pushes it (SSE `state`, GET /api/state):
 * projects, services, every chat/terminal/parked pane, scheduled projects,
 * unfinished factory runs.
 */
@Serializable
data class ServerState(
    val uptime: String = "",
    val projects: List<Project> = emptyList(),
    val services: List<Service> = emptyList(),
    val terminals: List<Pane> = emptyList(),
    /** Aliases of projects with at least one enabled schedule (the sidebar clock). */
    val scheduled: List<String> = emptyList(),
    val factoryRuns: List<FactoryRunBrief> = emptyList(),
    /** owner or collaborator */
    val role: String = "owner",
    val collab: CollabInfo? = null,
) {
    fun chats(project: String? = null) = terminals.filter { it.isChat && (project == null || it.project.equals(project, true)) }.sortedBy { it.order }
    fun terminalsOf(project: String? = null) = terminals.filter { it.isTerminal && (project == null || it.project.equals(project, true)) }.sortedBy { it.order }
    fun parked(project: String? = null) = terminals.filter { it.isParked && (project == null || it.project.equals(project, true)) }.sortedBy { it.order }
    fun servicesOf(project: String) = services.filter { it.alias.equals(project, true) }

    /** The project's dot, as the desktop's sidebar draws it: needs input, then working, waiting, running. */
    fun projectState(alias: String): AgentState? {
        val mine = terminals.filter { it.project.equals(alias, true) && !it.isParked }
        val states = mine.map { it.state }
        return when {
            AgentState.NeedsInput in states -> AgentState.NeedsInput
            AgentState.Working in states -> AgentState.Working
            AgentState.Waiting in states -> AgentState.Waiting
            mine.any { it.status == "running" } || servicesOf(alias).any { it.running } -> AgentState.Idle
            else -> null
        }
    }
}

/** One usage window: percent used and when it resets. */
@Serializable data class UsageWindow(val utilization: Int = 0, val resetsAt: String? = null) {
    val resetsAtMs get() = resetsAt?.let { runCatching { java.time.Instant.parse(it).toEpochMilli() }.getOrNull() } ?: 0L
}

/** The account's Claude usage (SSE `usage`, GET /api/usage). Owner only. */
@Serializable
data class Usage(
    val ok: Boolean = true,
    val error: String = "",
    val subscriptionType: String? = null,
    val rateLimitTier: String? = null,
    val fiveHour: UsageWindow? = null,
    val week: UsageWindow? = null,
    val weekOpus: UsageWindow? = null,
    val weekSonnet: UsageWindow? = null,
    val extraUsage: JsonObject? = null,
    val fetchedAt: String = "",
)

/** One point of GET /api/usage/history (percentages, null where not recorded). */
@Serializable data class UsagePoint(val at: Long = 0, val five: Int? = null, val week: Int? = null, val fiveOpt: Int? = null, val weekOpt: Int? = null, val fiveReset: String? = null, val weekReset: String? = null)

/**
 * Something waiting on the person (SSE `approvals`, GET /api/approvals):
 * [source] secret (a secret request), chat (a permission or question in a
 * chat), collab (a collaborator asking).
 */
@Serializable
data class ApprovalItem(
    val id: String,
    val source: String = "",
    val title: String = "",
    val detail: String = "",
    val project: String = "",
    /** chat and collab: the chat it came from. */
    val sessionId: String = "",
    val isQuestion: Boolean = false,
    /** secret: get or run; collab: service, tunnel, forward, merge, note, tool, command */
    val kind: String = "",
    val command: String = "",
    /** collab: the collaborator's name. */
    val name: String = "",
    val host: String = "",
    /** Epoch ms or ISO text, depending on source: read [createdAtMs]. */
    val createdAt: JsonElement? = null,
) {
    val createdAtMs get() = timeMs(createdAt)
}

@Serializable data class Approvals(val items: List<ApprovalItem> = emptyList(), val count: Int = 0)

/** A secret the server holds: names only, never values (GET /api/secrets). */
@Serializable data class Secret(val id: String, val name: String = "", val description: String = "", val createdAt: String = "", val updatedAt: String = "")

/** Claude asking to use a secret (SSE `secret-requests`): approve with the 6-digit PIN, or deny. */
@Serializable
data class SecretRequest(
    val id: String,
    val secretId: String = "",
    val secretName: String = "",
    val project: String = "",
    val reason: String = "",
    val createdAt: String = "",
    /** get (hand Claude the value) or run (run [command] with it in [envName]) */
    val kind: String = "get",
    val command: String = "",
    val envName: String = "",
    val sessionId: String = "",
)

@Serializable data class Secrets(val secrets: List<Secret> = emptyList(), val requests: List<SecretRequest> = emptyList())

/**
 * A secret pre-authorised for one chat (GET /api/secrets/chat, SSE
 * `chat-secrets`): its name and when it runs out, never a value or a PIN.
 * [expiresAt] is null when it lasts until the chat ends.
 */
@Serializable
data class ChatSecretGrant(
    val secretId: String,
    val name: String = "",
    val expiresAt: String? = null,
    val untilChatEnds: Boolean = false,
    val grantedAt: String = "",
) {
    companion object {
        /** The durations offered, as the server's PREAUTH_PRESETS (src/chat-secrets.js). */
        val PRESETS = listOf("15m" to "15 minutes", "1h" to "1 hour", "8h" to "8 hours", "chat" to "Until this chat ends")
    }
}
