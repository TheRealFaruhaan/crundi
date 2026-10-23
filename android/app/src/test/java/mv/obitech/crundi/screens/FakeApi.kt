package mv.obitech.crundi.screens

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonObject
import mv.obitech.crundi.api.ApiResponse
import mv.obitech.crundi.api.ChatReducer
import mv.obitech.crundi.api.CrundiJson
import mv.obitech.crundi.api.FakeTransport
import mv.obitech.crundi.api.model.ChatState
import mv.obitech.crundi.api.model.ServerState

/**
 * Realistic server answers for tests and screenshots, read from
 * src/test/resources/api (one JSON file per answer). The shapes are the server's own (see
 * shared/api.md); edit the JSON to change what the screens show.
 *
 * Screens draw from models, so a screenshot test usually wants the
 * decoded objects: [state], [chat], or `FakeApi.model<KanbanBoard>("kanban", "board")`.
 */
object FakeApi {
    /** A fixture's text. */
    fun text(name: String): String = javaClass.classLoader!!.getResource("api/$name")!!.readText()
    fun json(name: String): JsonObject = CrundiJson.parseToJsonElement(text("$name.json")).jsonObject

    /** A fixture (or one field of it) as a model. */
    inline fun <reified T> model(name: String, key: String? = null): T = CrundiJson.decodeFromJsonElement(if (key == null) json(name) else json(name)[key]!!)

    /** The workbench as the server pushes it. */
    val state: ServerState get() = model("state")

    /** The chat "Android API" after its history and every event of chat-events.jsonl up to [events] (all by default). */
    fun chat(events: Int = Int.MAX_VALUE): ChatState {
        var s = ChatReducer.message(ChatState("a1b2c3d4e5f60718"), json("chat-history"))
        for (line in text("chat-events.jsonl").lines().filter { it.isNotBlank() }.take(events)) s = ChatReducer.message(s, CrundiJson.parseToJsonElement(line).jsonObject)
        return s
    }

    /** Every route the fixtures cover. */
    val routes = mapOf(
        "GET /api/state" to "state", "GET /api/projects" to "projects", "GET /api/usage" to "usage", "GET /api/approvals" to "approvals",
        "GET /api/secrets" to "secrets", "GET /api/ui-sessions/resumable" to "resumable", "GET /api/chat-schedule" to "chat-schedule",
        "GET /api/files/list" to "files-list", "GET /api/files/search" to "files-search", "GET /api/files/read" to "files-read",
        "GET /api/git/info" to "git-info", "GET /api/git/diff" to "git-diff", "GET /api/git/log" to "git-log", "GET /api/git/show" to "git-show", "GET /api/media/list" to "media-list",
        "GET /api/kanban" to "kanban", "GET /api/notes" to "notes", "GET /api/notes/page" to "notes-page", "GET /api/mindmap" to "mindmap",
        "GET /api/schedules" to "schedules", "GET /api/factories" to "factories", "GET /api/factory-templates" to "factory-templates",
        "GET /api/factories/get" to "factory-get", "GET /api/factory-runs" to "factory-runs", "GET /api/factory-runs/get" to "factory-run-get",
        "GET /api/factory-triggers" to "factory-triggers", "GET /api/services" to "services", "GET /api/forwards" to "forwards",
        "GET /api/docker/status" to "docker-status", "GET /api/docker/containers" to "docker-containers", "GET /api/docker/containers/detail" to "docker-detail",
        "GET /api/docker/stacks" to "docker-stacks", "GET /api/docker/images" to "docker-images", "GET /api/docker/volumes" to "docker-volumes",
        "GET /api/docker/networks" to "docker-networks", "GET /api/docker/system" to "docker-system", "GET /api/docker/system/df" to "docker-df",
        "GET /api/settings" to "settings", "GET /api/stats" to "stats", "GET /api/update/status" to "update", "GET /api/auth/sessions" to "sessions",
        "GET /api/server-logs" to "server-logs", "GET /api/widgets" to "widgets", "GET /api/widgets/doc" to "widget-doc",
        "GET /api/widgets/data" to "widget-data", "GET /api/widgets/versions" to "widget-versions", "GET /api/browsers" to "browsers",
        "GET /api/browsers/crundi%3Adefault/screenshot" to "browser-screenshot", "GET /api/collaborators" to "collaborators", "GET /api/tunnel" to "tunnel", "GET /api/notify/channels" to "notify-channels", "GET /api/usage/history" to "usage-history",
    )

    /** A server in memory answering every route above (and {"ok":true} to any POST). */
    fun transport(): FakeTransport {
        val t = FakeTransport()
        // The server sends the whole workbench when the event stream connects.
        t.initialEvents.add(mv.obitech.crundi.api.SseFrame("state", text("state.json")))
        // A chat opened on the socket gets its history, as the server answers subscribe-ui:
        // the fixture conversation, under the id that was asked for.
        val history = text("chat-history.json")
        t.onSocketSend = { sent ->
            val m = runCatching { CrundiJson.parseToJsonElement(sent).jsonObject }.getOrNull()
            val id = (m?.get("id") as? kotlinx.serialization.json.JsonPrimitive)?.content
            if (m?.get("type")?.let { (it as? kotlinx.serialization.json.JsonPrimitive)?.content } == "subscribe-ui" && id != null)
                listOf(history.replace("a1b2c3d4e5f60718", id))
            else null
        }
        for ((route, file) in routes) t.on(route, text("$file.json"))
        t.on("GET /api/services/crundi%3Adocs/logs", text("service-logs.json"))
        t.on("POST /api/widgets/call") { ApiResponse(200, """{"ok":true,"key":"filter","value":"failed"}""") }
        t.on("POST /api/widgets/user") { ApiResponse(200, json("widgets")["widgets"]!!.let { w -> """{"ok":true,"widget":${(w as kotlinx.serialization.json.JsonArray)[0]}}""" }) }
        for (post in listOf("/api/kanban", "/api/notes", "/api/mindmap", "/api/schedules", "/api/git/stage", "/api/git/unstage", "/api/git/commit", "/api/widgets/grant", "/api/widgets/watch", "/api/widgets/fault"))
            t.on("POST $post") { ApiResponse(200, """{"ok":true}""") }
        return t
    }
}
