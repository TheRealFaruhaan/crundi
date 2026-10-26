package mv.obitech.crundi.api

import kotlinx.serialization.json.JsonObject
import mv.obitech.crundi.api.model.Attachment
import mv.obitech.crundi.api.model.ChatPreflight
import mv.obitech.crundi.api.model.ChatSchedule
import mv.obitech.crundi.api.model.ChatScheduleItem
import mv.obitech.crundi.api.model.Created
import mv.obitech.crundi.api.model.NewChat
import mv.obitech.crundi.api.model.Pane
import mv.obitech.crundi.api.model.ResumableSession
import mv.obitech.crundi.api.model.SendResult
import java.io.InputStream

/**
 * Chats (Claude in UI mode). The list of chats comes from the live state
 * ([CrundiClient.state], `terminals` of kind "ui"); a chat's conversation
 * streams over the socket ([CrundiClient.chat]). These are the actions.
 */
class ChatsApi internal constructor(private val rpc: Rpc) {
    private fun at(id: String, action: String) = "/api/ui-sessions/${seg(id)}/$action"

    /** Every chat with its project mark (GET /api/ui-sessions). The live state has the same list without marks. */
    suspend fun list(): List<Pane> = rpc.read(rpc.get("/api/ui-sessions"), "sessions", emptyList())

    /** Start a chat. Answers its id; open it with [CrundiClient.chat]. */
    suspend fun create(o: NewChat): Created = rpc.read(rpc.post("/api/ui-sessions/create", mapOf(
        "project" to o.project, "title" to o.title, "model" to o.model, "effort" to o.effort,
        "permissionMode" to o.permissionMode, "skipPermissions" to o.skipPermissions,
        "sessionMode" to o.sessionMode, "resumeId" to o.resumeId,
    )), null)

    /** Is the conversation a new chat would continue heavy (big or old)? Offer "start fresh" if so. */
    suspend fun preflight(project: String): ChatPreflight = rpc.read(rpc.get("/api/ui-sessions/preflight", mapOf("project" to project)), null)

    /** Conversations on disk the person can resume (newest first, at most 30). */
    suspend fun resumable(project: String): List<ResumableSession> = rpc.read(rpc.get("/api/ui-sessions/resumable", mapOf("project" to project)), "sessions", emptyList())

    /** The whole conversation now (the socket's ui-history gives the same; this is for a one-off read). */
    suspend fun history(id: String): JsonObject = rpc.read(rpc.get(at(id, "history")), "session")

    /**
     * Send a message. Attachments are uploaded first ([attach]) and their
     * paths put into [text], as the desktop does. Mid-turn it is "injected":
     * it shows when Claude takes it, and can be recalled while queued.
     */
    suspend fun send(id: String, text: String): SendResult = rpc.read(rpc.post(at(id, "send"), mapOf("text" to text)), null)

    /** Take back a message sent mid-turn that Claude has not taken yet. True if it was. */
    suspend fun recall(id: String, uuid: String): Boolean = rpc.post(at(id, "cancel-queued"), mapOf("uuid" to uuid)).bool("cancelled") == true

    /** Stop: interrupt the turn (the desktop asks for a second tap). */
    suspend fun stop(id: String) { rpc.post(at(id, "interrupt")) }

    /** Close the chat (ends its Claude). A factory's Manager refuses (ApiException 409). */
    suspend fun close(id: String) { rpc.post(at(id, "close")) }

    /** Rename. Answers the title the server kept. */
    suspend fun rename(id: String, title: String): String = rpc.post(at(id, "rename"), mapOf("title" to title)).str("title") ?: title

    /** Allow a permission request. [always]: true (write the CLI's rule), "session" (this chat only), or null. */
    suspend fun allow(id: String, requestId: String, always: Any? = null, updatedInput: JsonObject? = null) {
        rpc.post(at(id, "respond"), mapOf("requestId" to requestId, "behavior" to "allow", "always" to always, "updatedInput" to updatedInput))
    }

    suspend fun deny(id: String, requestId: String, message: String? = null) {
        rpc.post(at(id, "respond"), mapOf("requestId" to requestId, "behavior" to "deny", "message" to message))
    }

    /**
     * Answer a question (AskUserQuestion). [answers] maps each question's
     * text to the chosen label (several labels joined with ", " for a
     * multi-select, or the person's own words); [notes] optional per question.
     */
    suspend fun answer(id: String, entry: mv.obitech.crundi.api.model.ChatEntry, answers: Map<String, String>, notes: Map<String, String> = emptyMap()) {
        val input = entry.input.toMutableMap()
        input["answers"] = Uploads.toJson(answers)
        if (notes.isNotEmpty()) input["annotations"] = Uploads.toJson(notes.mapValues { mapOf("notes" to it.value) })
        rpc.post(at(id, "respond"), mapOf("requestId" to entry.requestId, "behavior" to "allow", "updatedInput" to JsonObject(input)))
    }

    /** Dismiss a question without answering. */
    suspend fun dismissQuestion(id: String, requestId: String) = deny(id, requestId, "The user dismissed the question.")

    /** Answer a question whose chat has since restarted (its card shows "unanswered"). */
    suspend fun answerClosed(id: String, entryId: String, answers: Map<String, String>, text: String? = null) {
        rpc.post(at(id, "answer-closed"), mapOf("entryId" to entryId, "answers" to answers, "text" to text))
    }

    /** default, acceptEdits, plan, auto, dontAsk, bypassPermissions. Answers the mode now in force. */
    suspend fun setPermissionMode(id: String, mode: String): String = rpc.post(at(id, "permission-mode"), mapOf("mode" to mode)).str("permissionMode") ?: mode

    suspend fun setModel(id: String, model: String): String = rpc.post(at(id, "model"), mapOf("model" to model)).str("model") ?: model

    /** Put away finished agent bubbles: these, or all with [all]. */
    suspend fun dismissAgents(id: String, toolUseIds: List<String> = emptyList(), all: Boolean = false) {
        rpc.post(at(id, "dismiss-agents"), if (all) mapOf("all" to true) else mapOf("toolUseIds" to toolUseIds))
    }

    suspend fun dismissSuggestion(id: String) { rpc.post(at(id, "dismiss-suggestion")) }

    /** Put the chats of a project in this order. */
    suspend fun reorder(project: String, order: List<String>) { rpc.post("/api/ui-sessions/reorder", mapOf("project" to project, "order" to order)) }

    /**
     * Upload a file for the composer (POST /api/attachments/upload). Streams
     * the file as base64; [progress] gets bytes of the file sent. Put the
     * answer's [Attachment.path] into the message text.
     */
    suspend fun attach(project: String, name: String, size: Long, progress: Progress? = null, open: () -> InputStream): Attachment =
        rpc.read(rpc.call("POST", "/api/attachments/upload", upload = Uploads.base64Json(mapOf("project" to project, "name" to name), size, open), progress = progress), null)

    // ─── Messages scheduled for later (the composer's clock) ───

    suspend fun scheduled(project: String): ChatSchedule = rpc.read(rpc.get("/api/chat-schedule", mapOf("project" to project)), null)

    /** [trigger]: "time" (with [at], ISO), "turn-end", or "limit-reset". */
    suspend fun schedule(project: String, text: String, trigger: String, at: String? = null): ChatScheduleItem =
        rpc.read(rpc.post("/api/chat-schedule", mapOf("project" to project, "text" to text, "trigger" to mapOf("type" to trigger, "at" to at).filterValues { it != null })), "item")

    suspend fun unschedule(itemId: String) { rpc.delete("/api/chat-schedule/${seg(itemId)}") }
}

/** Terminals (Claude or a shell in a PTY). Output and input go over the socket ([CrundiClient.terminal]). */
class TerminalsApi internal constructor(private val rpc: Rpc) {
    private fun at(id: String, action: String) = "/api/terminals/${seg(id)}/$action"

    /**
     * Open a terminal. [shell] true for a plain shell, otherwise Claude
     * ([sessionMode] continue/new/resume, [model], [skipPermissions]).
     */
    suspend fun create(project: String, shell: Boolean = false, cols: Int = 80, rows: Int = 30, title: String? = null, command: String? = null, sessionMode: String? = null, resumeId: String? = null, model: String? = null, skipPermissions: Boolean? = null): Created =
        rpc.read(rpc.post("/api/terminals/create", mapOf(
            "project" to project, "shell" to shell, "cols" to cols, "rows" to rows, "title" to title, "command" to command,
            "sessionMode" to sessionMode, "resumeId" to resumeId, "model" to model, "skipPermissions" to skipPermissions,
        )), null)

    suspend fun close(id: String) { rpc.post(at(id, "close")) }
    suspend fun resize(id: String, cols: Int, rows: Int) { rpc.post(at(id, "resize"), mapOf("cols" to cols, "rows" to rows)) }
    suspend fun rename(id: String, title: String): String = rpc.post(at(id, "rename"), mapOf("title" to title)).str("title") ?: title
    suspend fun reorder(project: String, order: List<String>) { rpc.post("/api/terminals/reorder", mapOf("project" to project, "order" to order)) }

    // ─── Parked panes (a chat or terminal put away, kept to resume) ───
    private fun pane(id: String, action: String) = "/api/panes/${seg(id)}/$action"

    /** What a parked pane last showed: [PanePreview.screen] for a terminal, messages for a chat. */
    suspend fun preview(id: String): PanePreview {
        val o = rpc.get(pane(id, "preview"))
        return PanePreview(o.str("kind") ?: "", o.str("screen") ?: "", (o["messages"] as? kotlinx.serialization.json.JsonArray)?.mapNotNull { (it as? JsonObject)?.let(mv.obitech.crundi.api.model.ChatEntry::from) }.orEmpty())
    }
    /** Bring a parked pane back. [mode]: resume, compact, new. Answers the new id. */
    suspend fun resume(id: String, mode: String = "resume"): String = rpc.post(pane(id, "resume"), mapOf("mode" to mode)).str("id") ?: id
    /** Put a chat or terminal away to resume later. */
    suspend fun park(id: String) { rpc.post(pane(id, "park")) }
    suspend fun discard(id: String) { rpc.post(pane(id, "discard")) }
}

data class PanePreview(val kind: String, val screen: String, val messages: List<mv.obitech.crundi.api.model.ChatEntry>)
