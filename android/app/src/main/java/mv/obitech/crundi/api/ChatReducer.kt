package mv.obitech.crundi.api

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.intOrNull
import mv.obitech.crundi.api.model.AgentState
import mv.obitech.crundi.api.model.ChatAgent
import mv.obitech.crundi.api.model.ChatEntry
import mv.obitech.crundi.api.model.ChatPlan
import mv.obitech.crundi.api.model.ChatRecap
import mv.obitech.crundi.api.model.ChatState
import mv.obitech.crundi.api.model.ChatSuggestion
import mv.obitech.crundi.api.model.FactoryRole
import mv.obitech.crundi.api.model.PendingInjection
import mv.obitech.crundi.api.model.slashCommandsOf

/**
 * Turns a chat's socket stream into [ChatState], as the desktop's chat does:
 * `ui-history` replaces everything (on subscribe and after every
 * reconnect), each `ui-event` changes one thing. Pure, so it is tested
 * against the server's real messages.
 */
object ChatReducer {
    private const val MAX_MESSAGES = 2000

    /** A whole conversation (ui-history's `session`, or a `history` event). */
    fun history(id: String, session: JsonObject): ChatState {
        fun s(k: String) = (session[k] as? JsonPrimitive)?.contentOrNull ?: ""
        val agents = LinkedHashMap<String, ChatAgent>()
        (session["agents"] as? JsonArray)?.forEach { el -> (el as? JsonObject)?.let { ChatAgent.from(it) }?.let { agents[it.toolUseId] = it } }
        return ChatState(
            id = id, loaded = true, project = s("project"), title = s("title"), status = s("status"),
            state = if (s("status") == "exited") AgentState.Exited else AgentState.of(s("state").ifEmpty { "idle" }),
            sessionId = s("sessionId"), model = s("model"), permissionMode = s("permissionMode"),
            skipPermissions = (session["skipPermissions"] as? JsonPrimitive)?.booleanOrNull ?: false,
            slashCommands = slashCommandsOf(session["slashCommands"]),
            totalCostUsd = (session["totalCostUsd"] as? JsonPrimitive)?.doubleOrNull ?: 0.0,
            cwd = s("cwd"),
            messages = (session["messages"] as? JsonArray)?.mapNotNull { (it as? JsonObject)?.let(ChatEntry::from) }.orEmpty(),
            agents = agents,
            goal = session["goal"] as? JsonObject,
            plan = plan(session["plan"]),
            suggestion = suggestion(session["suggestion"]),
            recap = recap(session["recap"]),
            pendingInjections = (session["pendingInjections"] as? JsonArray)?.mapNotNull { runCatching { CrundiJson.decodeFromJsonElement<PendingInjection>(it) }.getOrNull() }.orEmpty(),
            factory = (session["factory"] as? JsonObject)?.let { runCatching { CrundiJson.decodeFromJsonElement<FactoryRole>(it) }.getOrNull() },
        )
    }

    /** A socket message for this chat: ui-history or ui-event. Anything else leaves the state as it is. */
    fun message(s: ChatState, m: JsonObject): ChatState = when (m.type()) {
        "ui-history" -> (m["session"] as? JsonObject)?.let { history(s.id, it) } ?: s
        "ui-event" -> (m["event"] as? JsonObject)?.let { event(s, it) } ?: s
        else -> s
    }

    /** One event of the stream. */
    fun event(s: ChatState, ev: JsonObject): ChatState {
        fun str(k: String) = (ev[k] as? JsonPrimitive)?.contentOrNull
        return when (str("type")) {
            "history" -> (ev["session"] as? JsonObject)?.let { history(s.id, it) } ?: s
            "entry" -> (ev["entry"] as? JsonObject)?.let { add(s, ChatEntry.from(it)) } ?: s
            "patch" -> {
                val id = str("id") ?: return s
                val patch = ev["patch"] as? JsonObject ?: return s
                s.copy(messages = s.messages.map { if (it.id == id) it.patched(patch) else it })
            }
            "delta" -> {
                val id = str("id") ?: return s
                val add = str("text") ?: return s
                s.copy(messages = s.messages.map { if (it.id == id) it.copy(text = it.text + add, raw = JsonObject(it.raw + ("text" to JsonPrimitive(it.text + add)))) else it })
            }
            "state" -> s.copy(state = AgentState.of(str("state")), status = if (s.status == "exited") "running" else s.status)
            "exit" -> s.copy(status = "exited", state = AgentState.Exited, exitCode = (ev["code"] as? JsonPrimitive)?.intOrNull ?: 0)
            "meta" -> s.copy(
                permissionMode = str("permissionMode") ?: s.permissionMode,
                model = str("model") ?: s.model,
                plan = if (ev.containsKey("plan")) plan(ev["plan"]) else s.plan,
            )
            "init" -> s.copy(
                sessionId = str("sessionId")?.takeIf { it.isNotEmpty() } ?: s.sessionId,
                model = str("model")?.takeIf { it.isNotEmpty() } ?: s.model,
                slashCommands = if (ev.containsKey("slashCommands")) slashCommandsOf(ev["slashCommands"]) else s.slashCommands,
                permissionMode = str("permissionMode") ?: s.permissionMode,
                status = "running",
            )
            "agent" -> {
                val a = ev["agent"] as? JsonObject ?: return s
                val id = (a["toolUseId"] as? JsonPrimitive)?.contentOrNull ?: return s
                val old = s.agents[id]
                s.copy(agents = LinkedHashMap(s.agents).apply { put(id, ChatAgent.from(a, old?.messages.orEmpty())) })
            }
            "agent-entry" -> {
                val id = str("toolUseId") ?: return s
                val e = (ev["entry"] as? JsonObject)?.let(ChatEntry::from) ?: return s
                val a = s.agents[id] ?: ChatAgent(toolUseId = id, status = "running")
                s.copy(agents = LinkedHashMap(s.agents).apply { put(id, a.copy(messages = (a.messages + e).takeLast(150), count = a.count + 1)) })
            }
            "agent-patch" -> {
                val id = str("toolUseId") ?: return s
                val eid = str("id") ?: return s
                val patch = ev["patch"] as? JsonObject ?: return s
                val a = s.agents[id] ?: return s
                s.copy(agents = LinkedHashMap(s.agents).apply { put(id, a.copy(messages = a.messages.map { if (it.id == eid) it.patched(patch) else it })) })
            }
            "injected" -> {
                // Claude took a message sent mid-turn: it leaves the drawer (oldest with that text).
                val text = str("text") ?: return s
                val i = s.pendingInjections.indexOfFirst { it.text == text }
                if (i < 0) s else s.copy(pendingInjections = s.pendingInjections.toMutableList().apply { removeAt(i) })
            }
            "queued-state" -> {
                val uuid = str("uuid") ?: ""
                val state = str("state") ?: ""
                val text = str("text") ?: ""
                val list = s.pendingInjections.toMutableList()
                var i = list.indexOfFirst { it.uuid == uuid && uuid.isNotEmpty() }
                if (i < 0) i = list.indexOfFirst { it.uuid.isEmpty() && it.text == text }
                if (state in setOf("cancelled", "discarded", "refused")) { if (i >= 0) list.removeAt(i) }
                else if (i >= 0) list[i] = list[i].copy(uuid = uuid, state = state)
                else list.add(PendingInjection(text, uuid, state))
                s.copy(pendingInjections = list)
            }
            "goal" -> s.copy(goal = ev["goal"] as? JsonObject)
            "suggestion" -> s.copy(suggestion = suggestion(ev["suggestion"]))
            // Set when the chat has sat idle or was resumed; null once closed or a message went.
            "recap" -> s.copy(recap = recap(ev["recap"]))
            else -> s
        }
    }

    /** A message this phone just sent mid-turn: show it in the drawer until Claude takes it. */
    fun sentMidTurn(s: ChatState, text: String, uuid: String): ChatState =
        if (s.pendingInjections.any { it.uuid == uuid && uuid.isNotEmpty() }) s
        else s.copy(pendingInjections = s.pendingInjections + PendingInjection(text, uuid, "sent"))

    private fun add(s: ChatState, e: ChatEntry): ChatState {
        val i = s.messages.indexOfFirst { it.id == e.id }
        val list = if (i >= 0) s.messages.toMutableList().apply { set(i, e) } else (s.messages + e)
        return s.copy(messages = if (list.size > MAX_MESSAGES) list.takeLast(MAX_MESSAGES) else list)
    }

    private fun plan(el: kotlinx.serialization.json.JsonElement?): ChatPlan? =
        if (el == null || el is JsonNull) null else runCatching { CrundiJson.decodeFromJsonElement<ChatPlan>(el) }.getOrNull()

    private fun recap(el: kotlinx.serialization.json.JsonElement?): ChatRecap? =
        if (el == null || el is JsonNull) null else runCatching { CrundiJson.decodeFromJsonElement<ChatRecap>(el) }.getOrNull()?.takeIf { it.text.isNotBlank() }

    private fun suggestion(el: kotlinx.serialization.json.JsonElement?): ChatSuggestion? =
        if (el == null || el is JsonNull) null else runCatching { CrundiJson.decodeFromJsonElement<ChatSuggestion>(el) }.getOrNull()?.takeIf { it.text.isNotEmpty() }
}
