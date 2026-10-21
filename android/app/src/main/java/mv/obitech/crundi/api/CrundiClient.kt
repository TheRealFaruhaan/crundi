package mv.obitech.crundi.api

import android.content.Context
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.runningFold
import kotlinx.coroutines.launch
import kotlinx.coroutines.delay
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import mv.obitech.crundi.Api
import mv.obitech.crundi.Server
import mv.obitech.crundi.api.model.ChatState
import mv.obitech.crundi.api.model.ServerLog
import java.util.UUID

/**
 * The one client for one Crundi server: every HTTP call, the live event
 * stream, the WebSocket (chats, terminals, docker, logs), and the server's
 * pictures. One per paired server, kept for the life of the process
 * (AppGraph), shared by the main window and every bubble.
 *
 * Screens never use OkHttp or URLs directly: they call the area APIs here
 * (suspend functions that throw [ApiException] with words fit to show) and
 * collect the live flows.
 */
class CrundiClient(val transport: Transport, scope: CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)) {
    private val _signedOut = MutableStateFlow(false)
    /** The server has refused this phone's sign-in: offer to pair again (Host.pairAgain). */
    val signedOut: StateFlow<Boolean> = _signedOut

    private val rpc = Rpc(transport) { _signedOut.value = true }

    /** The live event stream: [LiveEvents.state], usage, approvals, secret requests, change events. */
    val events = LiveEvents(transport, scope) { _signedOut.value = true }
    /** The WebSocket shared by chats, terminals, docker and logs. */
    val socket = LiveSocket(transport, scope) { _signedOut.value = true }
    val images = ServerImages(transport)

    val projects = ProjectsApi(rpc)
    val chats = ChatsApi(rpc)
    val terminals = TerminalsApi(rpc)
    val files = FilesApi(rpc)
    val git = GitApi(rpc)
    val media = MediaApi(rpc)
    val kanban = KanbanApi(rpc)
    /** [NotesApi.clientId] names this app in `notes` events, so a screen can skip its own saves. */
    val notes = NotesApi(rpc, "android-" + UUID.randomUUID().toString().take(8))
    val mindmap = MindMapApi(rpc)
    val schedules = SchedulesApi(rpc)
    val factories = FactoriesApi(rpc)
    val services = ServicesApi(rpc)
    val forwards = ForwardsApi(rpc)
    val docker = DockerApi(rpc)
    val secrets = SecretsApi(rpc)
    val approvals = ApprovalsApi(rpc)
    val usage = UsageApi(rpc)
    val system = SystemApi(rpc)
    /** Panels (widgets): list, frame documents, data, calls from frames. See api.md "Panels". */
    val widgets = WidgetsApi(rpc)
    /** The server's own browsers (Browsers tab list). */
    val browsers = BrowsersApi(rpc)
    /** Outside collaborators (Info tab). */
    val collaborators = CollaboratorsApi(rpc)

    /**
     * A chat, live: its whole conversation, then every change, as
     * [ChatState]. Starts with an unloaded state ([ChatState.loaded] false)
     * until the history arrives; after a reconnect the history comes again
     * and replaces it.
     */
    fun chat(id: String): Flow<ChatState> =
        socket.messages("ui:$id", msg("type" to "subscribe-ui", "id" to id), msg("type" to "unsubscribe-ui", "id" to id)) {
            (it.type() == "ui-history" || it.type() == "ui-event") && (it["id"] as? JsonPrimitive)?.contentOrNull == id
        }.runningFold(ChatState(id)) { s, m -> ChatReducer.message(s, m) }

    /** A terminal's output and input. */
    fun terminal(id: String) = TerminalStream(socket, id)

    /** A container's live logs (the last [tail] lines first). */
    fun dockerLogs(containerId: String, tail: Int = 500): Flow<DockerLogEvent> = dockerLogs(socket, containerId, tail)

    /** A shell in a container: collect [DockerShell.events] to open it. */
    fun dockerShell(containerId: String, cols: Int = 80, rows: Int = 24) = DockerShell(socket, containerId, cols, rows)

    /** Live server log lines. */
    fun serverLogs(): Flow<ServerLog> = serverLogs(socket)

    /**
     * The app is (or is no longer) in front of the person, looking at
     * [project]. The server uses it to decide whether to notify the phone.
     * Sent again every minute while active, as the desktop does.
     */
    fun setPresence(active: Boolean, project: String?) = presence.set(active, project)
    private val presence = Presence(socket, scope)

    /** The worst of the two live connections: what the top bar's badge shows. */
    val status: Flow<LiveStatus> = combine(events.status, socket.status) { a, b ->
        when {
            a == LiveStatus.SignedOut || b == LiveStatus.SignedOut -> LiveStatus.SignedOut
            a == LiveStatus.Idle -> b
            b == LiveStatus.Idle -> a
            else -> maxOf(a, b, compareBy { it.ordinal })
        }
    }

    /** Try both connections again now. */
    fun reconnect() { _signedOut.value = false; events.reconnect(); socket.reconnect() }

    companion object {
        /** The client for a paired server, signed in with the phone's page login. */
        fun forServer(app: Context, server: Server): CrundiClient = CrundiClient(
            OkHttpTransport(server.baseUrl.trimEnd('/'), PageLoginCredentials(app.applicationContext, server.id), Api.CLIENT_TAG, "Crundi ${Api.UA_SUFFIX}"),
        )
    }
}

/** Presence over the socket: held while active, refreshed every minute. */
private class Presence(private val socket: LiveSocket, private val scope: CoroutineScope) {
    private var release: (() -> Unit)? = null
    private var timer: kotlinx.coroutines.Job? = null

    @Synchronized
    fun set(active: Boolean, project: String?) {
        timer?.cancel(); timer = null
        if (active) {
            val m = msg("type" to "presence", "active" to true, "engaged" to true, "project" to project)
            release?.invoke()
            release = socket.hold("presence", m, null) {}
            timer = scope.launchEvery(60_000) { socket.send(m) }
        } else {
            socket.send(msg("type" to "presence", "active" to false, "engaged" to false))
            release?.invoke(); release = null
        }
    }
}

private fun CoroutineScope.launchEvery(ms: Long, block: () -> Unit) = launch {
    while (true) { delay(ms); block() }
}
