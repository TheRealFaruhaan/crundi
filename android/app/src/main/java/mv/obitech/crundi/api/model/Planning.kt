package mv.obitech.crundi.api.model

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject

// ─── Kanban ───

@Serializable data class KanbanTodo(val id: String, val text: String = "", val done: Boolean = false, val deleted: Boolean = false, val createdAt: String = "", val updatedAt: String = "")

@Serializable data class LinkedIdea(val id: String, val text: String = "", val parentId: String? = null)

@Serializable
data class KanbanTask(
    val id: String,
    val title: String = "",
    val description: String = "",
    /** backlog, todo, in_progress, done */
    val status: String = "todo",
    val todos: List<KanbanTodo> = emptyList(),
    val deleted: Boolean = false,
    val deletedAt: String? = null,
    val createdAt: String = "",
    val updatedAt: String = "",
    /** Mind-map ideas linked to this task (GET /api/kanban only). */
    val mindmapNodes: List<LinkedIdea> = emptyList(),
) {
    val todosDone get() = todos.count { it.done && !it.deleted }
    val todosTotal get() = todos.count { !it.deleted }
}

@Serializable data class KanbanEvent(val id: String = "", val ts: String = "", val action: String = "", val message: String = "", val taskId: String? = null, val todoId: String? = null)

/** GET /api/kanban: [statuses] are the four columns in order. */
@Serializable
data class KanbanBoard(
    val statuses: List<String> = listOf("backlog", "todo", "in_progress", "done"),
    val tasks: List<KanbanTask> = emptyList(),
    val deletedTasks: List<KanbanTask> = emptyList(),
    val history: List<KanbanEvent> = emptyList(),
) {
    fun column(status: String) = tasks.filter { it.status == status && !it.deleted }

    companion object {
        /** The desktop's column titles. */
        val TITLES = mapOf("backlog" to "Backlog", "todo" to "To do", "in_progress" to "In progress", "done" to "Done")
    }
}

// ─── Notes ───

/**
 * One block of a note page. Text kinds (p, h1, h2, h3, bullet, number, quote,
 * todo) carry [text] as sanitised inline HTML (b, i, code, a, s); todo adds
 * [checked]; code has plain [text] and [lang]; table has [rows] and [header];
 * divider has nothing else.
 */
@Serializable
data class NoteBlock(
    val id: String,
    val type: String = "p",
    val text: String = "",
    val checked: Boolean = false,
    val lang: String = "",
    val wrap: Boolean = false,
    val rows: List<List<String>> = emptyList(),
    val header: Boolean = false,
)

@Serializable data class NoteSummary(val id: String, val title: String = "", val createdAt: String = "", val updatedAt: String = "", val deleted: Boolean = false, val deletedAt: String? = null, val preview: String = "", val version: Int = 0)

@Serializable data class NotePage(val id: String, val title: String = "", val blocks: List<NoteBlock> = emptyList(), val version: Int = 0, val createdAt: String = "", val updatedAt: String = "", val deleted: Boolean = false, val deletedAt: String? = null)

/** A save's answer: the new version, or a conflict carrying the page as it now is. */
data class NoteSave(val version: Int, val updatedAt: String = "", val title: String = "", val conflict: NotePage? = null)

// ─── Mind map ───

@Serializable data class LinkedTask(val project: String = "", val taskId: String = "", val todoId: String? = null)

/** What a node's linked task or todo is now: [kind] task or todo; [missing] when it was deleted. */
@Serializable
data class LinkInfo(
    val kind: String = "",
    val project: String = "",
    val taskId: String = "",
    val todoId: String? = null,
    val title: String = "",
    val description: String = "",
    val status: String = "",
    val deleted: Boolean = false,
    val todosDone: Int = 0,
    val todosTotal: Int = 0,
    val taskTitle: String = "",
    val text: String = "",
    val done: Boolean = false,
    val missing: Boolean = false,
)

@Serializable
data class MindNode(
    val id: String,
    val text: String = "",
    val notes: List<String> = emptyList(),
    val parentId: String? = null,
    val project: String? = null,
    val linkedTask: LinkedTask? = null,
    val createdAt: String = "",
    val updatedAt: String = "",
    /** The project this idea belongs to, its own or inherited. */
    val effectiveProject: String? = null,
    val linkedTaskInfo: LinkInfo? = null,
)

/** The whole mind map (every project the caller can see), as a flat list linked by [MindNode.parentId]. */
@Serializable data class MindMap(val nodes: List<MindNode> = emptyList()) {
    fun roots(project: String? = null) = nodes.filter { it.parentId == null && (project == null || it.effectiveProject == null || it.effectiveProject.equals(project, true)) }
    fun children(id: String) = nodes.filter { it.parentId == id }
}

// ─── Schedules ───

/** When a schedule fires. [mode]: once, recurring, or webhook (called, not timed: no days or time). days: 0 = Sunday .. 6 = Saturday. */
@Serializable data class ScheduleWhen(val mode: String = "recurring", val time: String = "09:00", val date: String = "", val days: List<Int> = emptyList())

/**
 * What a schedule does. [kind]: agent (a terminal Claude), chat (a chat),
 * command (a shell command), service (start/stop/toggle [serviceKey]).
 * The desktop sends every field whatever the kind.
 */
@Serializable
data class ScheduleAction(
    val kind: String = "chat",
    val agent: String = "claude",
    /** agent: normal or skip; chat: permission mode */
    val mode: String = "",
    val model: String = "",
    val effort: String = "",
    val prompt: String = "",
    /** agent: continue, new, resume; chat: new, resume */
    val session: String = "new",
    val sessionId: String = "",
    val cwd: String = "",
    val command: String = "",
    val serviceKey: String = "",
    /** start, stop, toggle */
    val op: String = "start",
)

/** A condition that must hold when a schedule fires: usage (metric 5h/week, op below/above/equal, value), terminals (op, value), kanban (taskId, todoId, status). */
@Serializable
data class ScheduleCondition(
    val type: String = "usage",
    val metric: String = "",
    val op: String = "",
    val value: Double = 0.0,
    val taskId: String = "",
    val todoId: String = "",
    val status: String = "",
)

/**
 * How a webhook (a schedule's "When called", a factory trigger's) is called
 * (server 1.21.1). [auth]: key (in the URL, X-Crundi-Key or Bearer) or
 * signature (HMAC-SHA256 of the raw body with [secret], in
 * [signatureHeader]; with [timestampHeader], of "time:body", as Grafana
 * does). [idempotency]: a template such as {{payload.groupKey}}; a call
 * whose key was already seen is not processed again. The server makes the
 * key and secret and never takes them from input, so they are null here
 * unless read from it. Every field is nullable so that what an older server
 * did not send is not sent back.
 */
@Serializable
data class HookSettings(
    val key: String? = null,
    val secret: String? = null,
    val auth: String? = null,
    val signatureHeader: String? = null,
    val timestampHeader: String? = null,
    val idempotency: String? = null,
) {
    val signed get() = auth == "signature"
    /** For a save: what may be set, without the credentials. */
    fun forSave() = copy(key = null, secret = null)
}

@Serializable
data class Schedule(
    val id: String = "",
    val name: String = "",
    val project: String = "",
    val enabled: Boolean = true,
    val tz: String = "",
    val action: ScheduleAction = ScheduleAction(),
    val `when`: ScheduleWhen = ScheduleWhen(),
    val conditions: List<ScheduleCondition> = emptyList(),
    /** For when.mode webhook (server 1.21.1; absent before, and on schedules not saved since). */
    val webhook: HookSettings? = null,
    val createdAt: String = "",
    val updatedAt: String = "",
    val lastRun: String? = null,
    val lastMet: Boolean = false,
) {
    val calledNotTimed get() = `when`.mode == "webhook"
}

@Serializable data class ScheduleServiceChoice(val key: String, val name: String = "", val alias: String = "", val status: String = "")

/** GET /api/schedules: the schedules and the services an action can pick. */
@Serializable data class Schedules(val schedules: List<Schedule> = emptyList(), val services: List<ScheduleServiceChoice> = emptyList())

/** A message queued to send to a chat later (GET /api/chat-schedule). trigger.type: time (with at), turn-end, limit-reset. */
@Serializable data class ChatTrigger(val type: String = "time", val at: String? = null)

@Serializable
data class ChatScheduleItem(
    val id: String,
    val project: String = "",
    val text: String = "",
    val trigger: ChatTrigger = ChatTrigger(),
    /** pending, sent, failed */
    val status: String = "pending",
    val createdAt: Long = 0,
    val firedAt: Long = 0,
    val error: String = "",
    val nextFireAt: Long? = null,
)

@Serializable data class ChatSchedule(val items: List<ChatScheduleItem> = emptyList(), val recent: List<ChatScheduleItem> = emptyList(), val limitWarmup: Boolean = false)

// ─── Factories ───

@Serializable data class FactorySummary(val id: String, val name: String = "", val description: String = "", val version: Int = 0, val createdAt: String = "", val updatedAt: String = "", val nodeCount: Int = 0, val employeeCount: Int = 0, val runnable: Boolean = false, val errorCount: Int = 0, val warningCount: Int = 0, val crTemplate: List<CrField> = emptyList(), val secrets: List<String> = emptyList())

/** A skill this machine has (GET /api/skills): offered for pinning in the factory editor. */
@Serializable data class SkillBrief(val name: String, val description: String = "", val scope: String = "")

/** One section of a change request form. */
@Serializable data class CrField(val id: String, val label: String = "", val hint: String = "", val required: Boolean = false, val list: Boolean = false)

@Serializable data class FactoryTemplate(val id: String, val name: String = "", val description: String = "", val goodFor: String = "", val team: List<String> = emptyList(), val steps: Int = 0)

@Serializable data class Position(val x: Double = 0.0, val y: Double = 0.0)

@Serializable data class Option(val id: String, val label: String = "")

@Serializable data class FactoryDecision(val kind: String = "yesno", val question: String = "", val options: List<Option> = emptyList(), val answeredBy: String? = null, val review: Boolean = false)
@Serializable data class FactoryAsk(val prompt: String = "", val replies: List<Option> = emptyList())
@Serializable data class FactoryWait(val kind: String = "time", val minutes: Double = 0.0, val at: String = "", val command: String = "", val every: Double = 0.0, val timeout: Double = 0.0, val what: String = "")
/** A Join's pick-any decision ([forkId]): whose parallel branches meet here. */
@Serializable data class FactoryJoin(val forkId: String = "")

/**
 * A step of a factory's flow. [type]: start, manager, employee, decision,
 * join, ask, wait, end. Out handles ([FactoryEdge.sourceHandle]): out for
 * start/employee/join; out and finish for manager; yes/no or option ids (and
 * none) for decision; reply ids for ask; done/timeout or out for wait.
 */
@Serializable
data class FactoryNode(
    val id: String,
    val type: String = "employee",
    val label: String = "",
    val position: Position = Position(),
    val employeeId: String? = null,
    val decision: FactoryDecision? = null,
    val ask: FactoryAsk? = null,
    val wait: FactoryWait? = null,
    val join: FactoryJoin? = null,
)

/** [kind] flow or note (a note edge feeds a node's "notein"). [maxRounds] 0 = no limit. */
@Serializable data class FactoryEdge(val id: String, val kind: String = "flow", val source: String = "", val sourceHandle: String = "out", val target: String = "", val requireNote: Boolean = false, val noteLabel: String = "", val maxRounds: Int = 0)

@Serializable
data class FactoryMember(
    val id: String,
    val name: String = "",
    val persona: String = "",
    val model: String = "",
    val effort: String = "",
    val memory: String = "keep",
    val tools: FactoryTools = FactoryTools(),
    val skills: List<String> = emptyList(),
    /** Permission presets for this person only (the factory's apply to everyone). */
    val presets: List<String> = emptyList(),
)

/** Claude Code tool rules for one person: always allowed, never allowed. */
@Serializable data class FactoryTools(val allow: List<String> = emptyList(), val deny: List<String> = emptyList())

/**
 * A factory's settings, every field src/factory-model.js keeps (normalizeFactory).
 * Fields a newer server adds survive a save all the same: the editor saves
 * over the server's own JSON (FactoryJson.forSave).
 */
@Serializable
data class FactorySettings(
    val managerMode: String = "normal",
    val mergeMode: String = "ask",
    val setupCommand: String = "",
    val autoRecover: Boolean = false,
    val teamMode: String = "ask",
    val presets: List<String> = listOf("git-read", "tests"),
    val rules: List<String> = emptyList(),
    val managerRules: Boolean = true,
    val crTemplate: List<CrField> = emptyList(),
    val limits: FactoryLimits = FactoryLimits(),
    /** The Manager's check-in, minutes (5 to 1440); 0: the Manager chooses. */
    val tickMinutes: Int = 0,
    val gatekeeper: FactoryGatekeeper = FactoryGatekeeper(),
    val caps: FactoryCaps = FactoryCaps(),
    /** Secrets a run may use without a PIN at each use (ids only; 'factory-secrets'). Their PINs are asked at the start. */
    val secrets: List<String> = emptyList(),
)

/** How hard a run may use the person's Claude usage (normalizeLimits). */
@Serializable data class FactoryLimits(val pauseAt5h: Int = 100, val pauseAtWeek: Int = 100, val maxParallel: Int = 0, val oneAtATimeAbove: Int = 0)
@Serializable data class FactoryGatekeeper(val enabled: Boolean = false, val model: String = "haiku")
@Serializable data class FactoryCaps(val maxHandoffs: Int = 200, val maxMinutes: Int = 480, val tokenBudget: Long = 0)

/** A factory's definition (GET /api/factories/get). */
@Serializable
data class Factory(
    val id: String = "",
    val name: String = "",
    val description: String = "",
    val version: Int = 0,
    val createdAt: String = "",
    val updatedAt: String = "",
    val settings: FactorySettings = FactorySettings(),
    val roster: List<FactoryMember> = emptyList(),
    val nodes: List<FactoryNode> = emptyList(),
    val edges: List<FactoryEdge> = emptyList(),
)

/** A problem validateFactory found, and where: a step ([nodeId], or several: [nodeIds]), a connection, a person. */
@Serializable data class FactoryIssue(val code: String = "", val message: String = "", val fix: String = "", val nodeId: String? = null, val edgeId: String? = null, val employeeId: String? = null, val nodeIds: List<String> = emptyList(), val handle: String? = null, val skill: String? = null)
@Serializable data class FactoryValidation(val ok: Boolean = true, val errors: List<FactoryIssue> = emptyList(), val warnings: List<FactoryIssue> = emptyList())

/**
 * A factory and whether it can run. [raw] is the definition exactly as the
 * server sent it: saving goes over it, so nothing the app does not know is lost.
 */
data class FactoryDetail(val factory: Factory, val validation: FactoryValidation, val raw: JsonObject? = null)

// ─── Factory runs ───

@Serializable data class RunWait(val kind: String = "", val what: String = "", val startedAt: JsonElement? = null, val deadline: JsonElement? = null, val timeoutAt: JsonElement? = null, val tries: Int = 0, val last: String = "")

/** Where the run is: one token per active branch of the flow. status: moving, work, answer, ask, wait, join, delegated, finished. */
@Serializable data class RunToken(
    val id: String, val nodeId: String = "", val label: String = "", val status: String = "", val employeeId: String = "", val employee: String = "", val blocked: Boolean = false, val wait: RunWait? = null,
    /** A job the Manager handed out (factory_delegate): the task, on a token that belongs to no step. */
    val job: String? = null,
)

/** What a person is doing this second, from their chat (servers from 1.21: the tool in flight, thinking, writing). state: working, idle, ... */
@Serializable data class RunActivity(val state: String = "", val text: String = "", val at: JsonElement? = null)

@Serializable data class RunEmployee(
    val chatId: String = "", val busy: String? = null, val queued: Int = 0, val retired: Boolean = false, val name: String = "",
    /** Held by the Manager (not handed new work for now). */
    val held: Boolean = false,
    /** Missing from older servers: the live view then says "Starting" or "Deciding". */
    val activity: RunActivity? = null,
)

/** Someone with work waiting for a free seat (the run's limit on people at once). */
@Serializable data class RunSeat(val tokenId: String = "", val employeeId: String = "", val employee: String = "")

/** A review finding. status: open, claimed, closed, dismissed. */
@Serializable data class RunFinding(val id: String, val text: String = "", val where: String = "", val blocking: Boolean = false, val status: String = "open", val by: String = "", val reopened: Int = 0)

/** An Ask step waiting on the person: answer with one of [replies]. */
@Serializable data class RunAsk(val tokenId: String, val nodeId: String = "", val prompt: String = "", val replies: List<Option> = emptyList())

@Serializable data class RunPermission(val id: String = "", val employee: String = "", val toolName: String = "", val input: JsonElement? = null, val at: JsonElement? = null, val sent: Boolean = false)

@Serializable data class RunCrSection(val id: String = "", val label: String = "", val text: String = "")
@Serializable data class RunCr(val title: String = "", val sections: List<RunCrSection> = emptyList(), val file: String = "")

@Serializable data class RunUsage(val costUsd: Double = 0.0, val turns: Int = 0, val chats: Int = 0, val activeMs: Long = 0)

@Serializable data class RunOutcome(val summary: String = "", val how: String = "", val commit: String = "")

@Serializable data class RunHistoryItem(val seq: Int = 0, val type: String = "", val nodeId: String = "", val by: String = "", val summary: String = "", val answers: JsonElement? = null, val reply: String = "", val note: String = "", val outcome: String = "", val detail: String = "")

/**
 * A factory run (GET /api/factory-runs, /get). status: starting, running,
 * waiting, paused, finishing, crashed, stopped, finished. waitingOn: usage,
 * paused, cap, ask, manager, or "".
 */
@Serializable
data class FactoryRun(
    val id: String,
    val project: String = "",
    val factoryId: String = "",
    val factoryName: String = "",
    val task: String = "",
    val cr: RunCr? = null,
    val status: String = "",
    val waitingOn: String = "",
    val createdAt: JsonElement? = null,
    val updatedAt: JsonElement? = null,
    val handoffs: Int = 0,
    val branch: String = "",
    val parentBranch: String = "",
    val worktree: String = "",
    val managerChatId: String = "",
    val tokens: List<RunToken> = emptyList(),
    val employees: Map<String, RunEmployee> = emptyMap(),
    val usage: RunUsage = RunUsage(),
    val findings: List<RunFinding> = emptyList(),
    val asks: List<RunAsk> = emptyList(),
    val managerMode: String = "",
    val teamMode: String = "",
    val permissions: List<RunPermission> = emptyList(),
    val outcome: RunOutcome? = null,
    val crash: JsonObject? = null,
    val capHit: JsonObject? = null,
    val paused: JsonObject? = null,
    val error: String = "",
    val timeLeftMs: Long? = null,
    val waitingForSeat: List<RunSeat> = emptyList(),
    // Only from /get:
    val history: List<RunHistoryItem> = emptyList(),
    /** The factory as it was when the run started: the graph to draw. */
    val snapshot: Factory? = null,
) {
    val createdAtMs get() = timeMs(createdAt)
    val updatedAtMs get() = timeMs(updatedAt)
    val active get() = status in setOf("starting", "running", "waiting", "paused", "finishing", "crashed")
    /** Ended with its work to decide on (merge, keep, discard). */
    val awaitingFinish get() = status == "finished" && outcome?.how.isNullOrEmpty()
}

/** GET /api/factory-runs/precheck: where a run would start from. */
@Serializable data class RunPrecheck(val top: String = "", val branch: String = "", val commit: String = "", val dirty: Boolean = false, val active: FactoryRunBrief? = null)

/** GET /api/factory-runs/events: the run's journal from [from]; poll again from [total]. */
@Serializable data class RunEvents(val from: Int = 0, val total: Int = 0, val events: List<JsonObject> = emptyList())

@Serializable data class RunDiff(val diff: String = "", val truncated: Boolean = false)

@Serializable data class TriggerSchedule(val at: String = "09:00", val days: List<Int> = emptyList())
@Serializable data class TriggerKanban(val status: String = "")
@Serializable data class TriggerCr(val title: String = "", val sections: Map<String, String> = emptyMap())
@Serializable data class TriggerHistory(val at: JsonElement? = null, val outcome: String = "", val source: String = "", val runId: String? = null, val title: String? = null, val note: String? = null)

/** What starts a factory by itself. [kind]: schedule, kanban (a card enters [kanban] status), webhook. */
@Serializable
data class FactoryTrigger(
    val id: String = "",
    val name: String = "",
    val enabled: Boolean = true,
    val project: String = "",
    val factoryId: String = "",
    val kind: String = "schedule",
    val schedule: TriggerSchedule? = null,
    val kanban: TriggerKanban? = null,
    /** Servers before 1.21.1 send only the key. */
    val webhook: HookSettings? = null,
    val cr: TriggerCr = TriggerCr(),
    val mergeMode: String = "",
    /** queue, skip, or manager (1.21.1: tell the running Manager, marked mid-run) */
    val ifBusy: String = "queue",
    /** A column that must hold a card for it to start a run ("" = always). Null: left as it is (servers before 'trigger-needs-cards' do not know it). */
    val needsCards: String? = null,
    val createdAt: JsonElement? = null,
    val lastFired: JsonElement? = null,
    val history: List<TriggerHistory> = emptyList(),
    /** Idempotency keys it remembers (30 days). */
    val seenCount: Int = 0,
)

@Serializable data class CleanupItem(val id: String, val kind: String = "", val path: String = "", val project: String = "", val runId: String = "", val factoryName: String = "", val status: String = "", val branch: String = "", val repo: String = "", val bytes: Long = 0, val ageMs: Long = 0, val uncommitted: JsonElement? = null, val why: String = "")
@Serializable data class Cleanup(val items: List<CleanupItem> = emptyList(), val autoDeleteDays: Int = 0)
