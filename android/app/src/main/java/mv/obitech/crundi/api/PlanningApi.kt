package mv.obitech.crundi.api

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.encodeToJsonElement
import mv.obitech.crundi.api.model.Cleanup
import mv.obitech.crundi.api.model.Factory
import mv.obitech.crundi.api.model.FactoryDetail
import mv.obitech.crundi.api.model.FactoryRun
import mv.obitech.crundi.api.model.FactorySummary
import mv.obitech.crundi.api.model.FactoryTemplate
import mv.obitech.crundi.api.model.FactoryTrigger
import mv.obitech.crundi.api.model.FactoryValidation
import mv.obitech.crundi.api.model.KanbanBoard
import mv.obitech.crundi.api.model.KanbanTask
import mv.obitech.crundi.api.model.KanbanTodo
import mv.obitech.crundi.api.model.MindMap
import mv.obitech.crundi.api.model.MindNode
import mv.obitech.crundi.api.model.NoteBlock
import mv.obitech.crundi.api.model.NotePage
import mv.obitech.crundi.api.model.NoteSave
import mv.obitech.crundi.api.model.NoteSummary
import mv.obitech.crundi.api.model.RunDiff
import mv.obitech.crundi.api.model.RunEvents
import mv.obitech.crundi.api.model.RunPrecheck
import mv.obitech.crundi.api.model.Schedule
import mv.obitech.crundi.api.model.Schedules

/** A project's kanban board. Refetch on the `kanban` event for the project. */
class KanbanApi internal constructor(private val rpc: Rpc) {
    suspend fun board(project: String, includeDeleted: Boolean = false): KanbanBoard =
        rpc.read(rpc.get("/api/kanban", mapOf("project" to project, "includeDeleted" to if (includeDeleted) "1" else null)), "board")

    private suspend fun act(project: String, action: String, vararg f: Pair<String, Any?>) = rpc.post("/api/kanban", mapOf("action" to action, "project" to project, *f))

    suspend fun addTask(project: String, title: String, description: String = "", status: String = "todo", todos: List<String> = emptyList()): KanbanTask =
        rpc.read(act(project, "addTask", "title" to title, "description" to description, "status" to status, "todos" to todos), "task")
    suspend fun updateTask(project: String, taskId: String, title: String? = null, description: String? = null, status: String? = null): KanbanTask =
        rpc.read(act(project, "updateTask", "taskId" to taskId, "title" to title, "description" to description, "status" to status), "task")
    /** Move to [status], at [index] within that column (end if null). */
    suspend fun moveTask(project: String, taskId: String, status: String, index: Int? = null): KanbanTask = rpc.read(act(project, "moveTask", "taskId" to taskId, "status" to status, "index" to index), "task")
    suspend fun deleteTask(project: String, taskId: String) { act(project, "deleteTask", "taskId" to taskId) }
    suspend fun restoreTask(project: String, taskId: String) { act(project, "restoreTask", "taskId" to taskId) }
    suspend fun addTodo(project: String, taskId: String, text: String): KanbanTodo = rpc.read(act(project, "addTodo", "taskId" to taskId, "text" to text), "todo")
    suspend fun updateTodo(project: String, taskId: String, todoId: String, text: String? = null, done: Boolean? = null): KanbanTodo =
        rpc.read(act(project, "updateTodo", "taskId" to taskId, "todoId" to todoId, "text" to text, "done" to done), "todo")
    suspend fun deleteTodo(project: String, taskId: String, todoId: String) { act(project, "deleteTodo", "taskId" to taskId, "todoId" to todoId) }
    suspend fun restoreTodo(project: String, taskId: String, todoId: String) { act(project, "restoreTodo", "taskId" to taskId, "todoId" to todoId) }
}

/**
 * A project's notes (block pages). Saves carry the version they were based
 * on; a conflict answers the page as it now is ([NoteSave.conflict]).
 * The `notes` event says which page changed and who changed it ([clientId]
 * lets a screen ignore its own saves).
 */
class NotesApi internal constructor(private val rpc: Rpc, val clientId: String) {
    suspend fun pages(project: String, includeDeleted: Boolean = false): List<NoteSummary> =
        rpc.read(rpc.get("/api/notes", mapOf("project" to project, "includeDeleted" to if (includeDeleted) "1" else null)), "pages", emptyList())
    suspend fun page(project: String, id: String): NotePage = rpc.read(rpc.get("/api/notes/page", mapOf("project" to project, "id" to id)), "page")

    private suspend fun act(project: String, action: String, vararg f: Pair<String, Any?>) = rpc.post("/api/notes", mapOf("action" to action, "project" to project, "clientId" to clientId, *f))

    suspend fun create(project: String, title: String = "", blocks: List<NoteBlock> = emptyList()): NotePage =
        rpc.read(act(project, "create", "title" to title, "blocks" to CrundiJson.encodeToJsonElement(blocks)), "page")

    suspend fun save(project: String, id: String, title: String, blocks: List<NoteBlock>, baseVersion: Int): NoteSave = try {
        val o = act(project, "save", "id" to id, "title" to title, "blocks" to CrundiJson.encodeToJsonElement(blocks), "baseVersion" to baseVersion)
        NoteSave(o.long("version")?.toInt() ?: baseVersion + 1, o.str("updatedAt") ?: "", o.str("title") ?: title)
    } catch (e: ApiException) {
        val page = e.body?.takeIf { it.bool("conflict") == true }?.obj("page") ?: throw e
        NoteSave(baseVersion, conflict = rpc.read(JsonObject(mapOf("p" to page)), "p"))
    }

    /** To the trash. */
    suspend fun delete(project: String, id: String) { act(project, "delete", "id" to id) }
    suspend fun restore(project: String, id: String) { act(project, "restore", "id" to id) }
    /** Delete for good (only from the trash). */
    suspend fun purge(project: String, id: String) { act(project, "purge", "id" to id) }
}

/** The mind map (all projects, flat). Refetch on the `mindmap` event. */
class MindMapApi internal constructor(private val rpc: Rpc) {
    suspend fun get(): MindMap = rpc.read(rpc.get("/api/mindmap"), "mindmap")

    private suspend fun act(action: String, vararg f: Pair<String, Any?>) = rpc.post("/api/mindmap", mapOf("action" to action, *f))

    suspend fun add(text: String, parentId: String? = null, notes: List<String>? = null): MindNode = rpc.read(act("addNode", "text" to text, "parentId" to parentId, "notes" to notes), "node")
    suspend fun update(id: String, text: String? = null, notes: List<String>? = null): MindNode = rpc.read(act("updateNode", "id" to id, "text" to text, "notes" to notes), "node")
    suspend fun addNote(id: String, text: String): MindNode = rpc.read(act("addNote", "id" to id, "text" to text), "node")
    suspend fun removeNote(id: String, index: Int): MindNode = rpc.read(act("removeNote", "id" to id, "index" to index), "node")
    /** Move under [parentId] (null: a root) at [index]. */
    suspend fun move(id: String, parentId: String?, index: Int? = null): MindNode = rpc.read(act("moveNode", "id" to id, "parentId" to (parentId ?: kotlinx.serialization.json.JsonNull), "index" to index), "node")
    suspend fun link(id: String, project: String, taskId: String, todoId: String? = null): MindNode = rpc.read(act("linkNode", "id" to id, "project" to project, "taskId" to taskId, "todoId" to todoId), "node")
    suspend fun unlink(id: String): MindNode = rpc.read(act("unlinkNode", "id" to id), "node")
    /** Give a root idea a project (null: every project). */
    suspend fun scope(id: String, project: String?): MindNode = rpc.read(act("scopeProject", "id" to id, "project" to (project ?: kotlinx.serialization.json.JsonNull)), "node")
    /** Deletes the idea and everything under it. Answers how many went. */
    suspend fun delete(id: String): Int = act("deleteNode", "id" to id).long("removed")?.toInt() ?: 0
}

/** Schedules (jobs at a time). Refetch on the `schedule` event. */
class SchedulesApi internal constructor(private val rpc: Rpc) {
    suspend fun list(project: String? = null): Schedules = rpc.read(rpc.get("/api/schedules", mapOf("project" to project)), null)
    suspend fun add(s: Schedule): Schedule = rpc.read(rpc.post("/api/schedules", mapOf("action" to "add", "schedule" to body(s))), "schedule")
    suspend fun update(s: Schedule): Schedule = rpc.read(rpc.post("/api/schedules", mapOf("action" to "update", "id" to s.id, "schedule" to body(s))), "schedule")
    suspend fun setEnabled(id: String, enabled: Boolean): Schedule = rpc.read(rpc.post("/api/schedules", mapOf("action" to "enable", "id" to id, "enabled" to enabled)), "schedule")
    suspend fun delete(id: String) { rpc.post("/api/schedules", mapOf("action" to "delete", "id" to id)) }
    /** A chat job set to "resume": make its chat now, so there is a session to resume. Save with [update] first. */
    suspend fun provisionChat(id: String): Schedule = rpc.read(rpc.post("/api/schedules", mapOf("action" to "provisionChat", "id" to id)), "schedule")

    /**
     * A webhook schedule's new key or secret ([which]: key or secret): calls
     * made with the old one stop working. Server 1.21.1.
     */
    suspend fun rotateHook(id: String, which: String): Schedule = rpc.read(rpc.post("/api/schedules", mapOf("action" to "rotateHook", "id" to id, "which" to which)), "schedule")

    /** What a save sends: webhook only when there is one (the server merges it with what it has, and makes the key and secret itself). */
    private fun body(s: Schedule): JsonObject {
        val o = CrundiJson.encodeToJsonElement(s.copy(webhook = s.webhook?.forSave())) as JsonObject
        return JsonObject(o.filterKeys { it in setOf("name", "project", "enabled", "tz", "action", "when", "conditions", "webhook") })
    }
}

/** Factories: the library, runs, triggers and cleanup. Owner only. */
class FactoriesApi internal constructor(private val rpc: Rpc) {
    suspend fun list(): List<FactorySummary> = rpc.read(rpc.get("/api/factories"), "factories", emptyList())
    suspend fun templates(): List<FactoryTemplate> = rpc.read(rpc.get("/api/factory-templates"), "templates", emptyList())
    suspend fun get(id: String): FactoryDetail = detail(rpc.get("/api/factories/get", mapOf("id" to id)))

    private suspend fun lib(action: String, vararg f: Pair<String, Any?>): FactoryDetail = detail(rpc.post("/api/factories", mapOf("action" to action, *f)))

    private fun detail(o: JsonObject) = FactoryDetail(rpc.read(o, "factory"), rpc.read(o, "validation", FactoryValidation()), o["factory"] as? JsonObject)
    suspend fun create(name: String): FactoryDetail = lib("create", "name" to name)
    suspend fun fromTemplate(templateId: String, name: String? = null): FactoryDetail = lib("fromTemplate", "templateId" to templateId, "name" to name)
    suspend fun duplicate(id: String, name: String? = null): FactoryDetail = lib("duplicate", "id" to id, "name" to name)
    /**
     * Save an edited definition over [raw] (the server's last copy, so fields
     * this app does not know survive: [FactoryJson]), as of [baseVersion].
     * A newer version on the server: ApiException whose body has conflict=true.
     */
    suspend fun save(factory: Factory, raw: JsonObject? = null, baseVersion: Int = factory.version): FactoryDetail =
        lib("save", "id" to factory.id, "definition" to FactoryJson.forSave(factory, raw), "baseVersion" to baseVersion)
    suspend fun delete(id: String) { rpc.post("/api/factories", mapOf("action" to "delete", "id" to id)) }
    /** The factory as a file to share: {format, formatVersion, factory}. */
    suspend fun export(id: String): JsonObject = rpc.read(rpc.get("/api/factories/export", mapOf("id" to id)), "data")
    suspend fun import(data: String): FactoryDetail = lib("import", "data" to data)
    /** The skills this machine has (GET /api/skills), for the editor's skill fields. */
    suspend fun skills(): List<mv.obitech.crundi.api.model.SkillBrief> = rpc.read(rpc.get("/api/skills"), "skills", emptyList())

    // ─── Runs ───
    suspend fun runs(project: String? = null): List<FactoryRun> = rpc.read(rpc.get("/api/factory-runs", mapOf("project" to project)), "runs", emptyList())
    /** A run with its recent history and the factory it runs ([FactoryRun.snapshot]): the live graph. */
    suspend fun run(id: String): FactoryRun = rpc.read(rpc.get("/api/factory-runs/get", mapOf("id" to id)), "run")
    /** The run's journal from [from]: poll again from [RunEvents.total] on each `factoryRun` event. */
    suspend fun events(id: String, from: Int = 0): RunEvents = rpc.read(rpc.get("/api/factory-runs/events", mapOf("id" to id, "from" to from.toString())), null)
    suspend fun diff(id: String): RunDiff = rpc.read(rpc.get("/api/factory-runs/diff", mapOf("id" to id), slow = true), null)
    /** Before starting: the branch it starts from, uncommitted work, a run already going. */
    suspend fun precheck(project: String): RunPrecheck = rpc.read(rpc.get("/api/factory-runs/precheck", mapOf("project" to project)), null)

    /**
     * Start a run with a change request: [sections] by the factory's
     * crTemplate field id; [attachments] are uploaded paths ([ChatsApi.attach]).
     */
    suspend fun start(project: String, factoryId: String, title: String, sections: Map<String, String>, attachments: List<Pair<String, String>> = emptyList(), mergeMode: String? = null, ignoreMissingSkills: Boolean = false): FactoryRun =
        rpc.read(rpc.post("/api/factory-runs", mapOf("action" to "start", "project" to project, "factoryId" to factoryId, "mergeMode" to mergeMode,
            "ignoreMissingSkills" to ignoreMissingSkills.takeIf { it },
            "cr" to mapOf("title" to title, "sections" to sections, "attachments" to attachments.map { mapOf("path" to it.first, "name" to it.second) }))), "run")

    private suspend fun runAct(action: String, id: String, vararg f: Pair<String, Any?>): FactoryRun = rpc.read(rpc.post("/api/factory-runs", mapOf("action" to action, "id" to id, *f)), "run")
    suspend fun stop(id: String) = runAct("stop", id)
    /**
     * Pause. [now]: stop the turns in progress too (they carry on at resume);
     * [resumeOnReset]: carry on by itself once the usage window resets. Both
     * need the server's 'factory-pause-options'; an older one ignores them.
     */
    suspend fun pause(id: String, now: Boolean = false, resumeOnReset: Boolean = false) =
        runAct("pause", id, "mode" to (if (now) "now" else null), "resumeOnReset" to resumeOnReset.takeIf { it })
    suspend fun resume(id: String) = runAct("resume", id)
    suspend fun recover(id: String, note: String? = null) = runAct("recover", id, "note" to note)
    /** Answer an Ask step. */
    suspend fun answerAsk(id: String, tokenId: String, replyId: String, note: String? = null) = runAct("ask", id, "tokenId" to tokenId, "replyId" to replyId, "note" to note)
    /** End a Wait step now: outcome done or timeout. */
    suspend fun endWait(id: String, tokenId: String, outcome: String = "done", note: String? = null) = runAct("endWait", id, "tokenId" to tokenId, "outcome" to outcome, "note" to note)
    suspend fun setManagerMode(id: String, mode: String) = runAct("managerMode", id, "mode" to mode)
    suspend fun setTeamMode(id: String, mode: String) = runAct("teamMode", id, "mode" to mode)
    /** Decide a finished run: merge, changes (with [note]), keep, discard, undo. */
    suspend fun finish(id: String, choice: String, note: String? = null) = runAct("finish", id, "choice" to choice, "note" to note)

    // ─── Triggers ───
    suspend fun triggers(project: String? = null): List<FactoryTrigger> = rpc.read(rpc.get("/api/factory-triggers", mapOf("project" to project)), "triggers", emptyList())
    suspend fun saveTrigger(t: FactoryTrigger): FactoryTrigger = rpc.read(rpc.post("/api/factory-triggers", mapOf("action" to "save", "trigger" to CrundiJson.encodeToJsonElement(t.copy(webhook = t.webhook?.forSave())))), "trigger")
    suspend fun deleteTrigger(id: String) { rpc.post("/api/factory-triggers", mapOf("action" to "delete", "id" to id)) }
    /** A webhook trigger's new key, or ([which] secret, server 1.21.1) its new secret. */
    suspend fun rotateTrigger(id: String, which: String = "key"): FactoryTrigger = rpc.read(rpc.post("/api/factory-triggers", mapOf("action" to "rotate", "id" to id, "which" to which.takeIf { it != "key" })), "trigger")
    /** Fire it now. Answers the server's word: run, queued or skipped. */
    suspend fun runTrigger(id: String): JsonObject = rpc.post("/api/factory-triggers", mapOf("action" to "run", "id" to id))

    // ─── Cleanup of finished runs' worktrees ───
    suspend fun cleanup(): Cleanup { val o = rpc.get("/api/factory-cleanup"); return Cleanup(rpc.read(o, "items", emptyList()), o.obj("settings")?.long("autoDeleteDays")?.toInt() ?: 0) }
    suspend fun setAutoDelete(days: Int) { rpc.post("/api/factory-cleanup", mapOf("action" to "settings", "autoDeleteDays" to days)) }
    suspend fun removeLeftovers(ids: List<String>) { rpc.post("/api/factory-cleanup", mapOf("action" to "remove", "ids" to ids)) }
}
