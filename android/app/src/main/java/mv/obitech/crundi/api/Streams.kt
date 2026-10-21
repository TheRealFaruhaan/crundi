package mv.obitech.crundi.api

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.flow.onCompletion
import kotlinx.coroutines.flow.transformWhile
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.intOrNull
import mv.obitech.crundi.api.model.DockerLogFrame
import mv.obitech.crundi.api.model.ServerLog
import java.util.concurrent.atomic.AtomicInteger

/**
 * A terminal's live I/O. [output] is the raw ANSI byte stream as text: the
 * scrollback first, then live output (collect it once per screen; draw it
 * with a terminal emulator). The process ending shows as the text
 * "[Process exited with code N]" in the output (see [exitCode]).
 */
class TerminalStream internal constructor(private val socket: LiveSocket, val id: String) {
    val output: Flow<String> = socket.messages("term:$id", msg("type" to "subscribe", "id" to id), msg("type" to "unsubscribe", "id" to id)) {
        it.type() == "output" && (it["id"] as? JsonPrimitive)?.contentOrNull == id
    }.mapNotNull { (it["data"] as? JsonPrimitive)?.contentOrNull }

    /** Keys typed (text, or escape sequences for the keyboard row: "\u001b[A" up, "\t", "\u0003" Ctrl-C). */
    fun input(data: String): Boolean = socket.send(msg("type" to "input", "id" to id, "data" to data))

    /** The screen's size in cells: send on layout and on every change. */
    fun resize(cols: Int, rows: Int): Boolean = socket.send(msg("type" to "resize", "id" to id, "cols" to cols, "rows" to rows))

    companion object {
        private val EXIT = Regex("\\[Process exited with code (-?\\d+)]")
        /** The exit code if this chunk of output says the process ended. */
        fun exitCode(chunk: String): Int? = EXIT.find(chunk)?.groupValues?.get(1)?.toIntOrNull()
    }
}

/** A line or the end of a container's live logs. */
sealed interface DockerLogEvent {
    data class Lines(val frames: List<DockerLogFrame>) : DockerLogEvent
    data object Ended : DockerLogEvent
    data class Failed(val error: String) : DockerLogEvent
}

/** What a shell inside a container sends back. */
sealed interface DockerShellEvent {
    data class Data(val text: String) : DockerShellEvent
    data class Exited(val code: Int) : DockerShellEvent
    data class Failed(val error: String) : DockerShellEvent
}

private val refs = AtomicInteger(0)
internal fun newRef(prefix: String) = "$prefix${System.currentTimeMillis().toString(36)}${refs.incrementAndGet()}"

/** Follow a container's logs live: the last [tail] lines first. Stops when the flow is cancelled. */
internal fun dockerLogs(socket: LiveSocket, id: String, tail: Int): Flow<DockerLogEvent> {
    val ref = newRef("l")
    return socket.messages("dlog:$ref", msg("type" to "docker-logs-follow", "ref" to ref, "id" to id, "tail" to tail), msg("type" to "docker-logs-stop", "ref" to ref)) {
        it.type().startsWith("docker-logs") && (it["ref"] as? JsonPrimitive)?.contentOrNull == ref
    }.mapNotNull { m ->
        when (m.type()) {
            "docker-logs" -> DockerLogEvent.Lines(runCatching { CrundiJson.decodeFromJsonElement<List<DockerLogFrame>>(m["frames"] ?: JsonArray(emptyList())) }.getOrDefault(emptyList()))
            "docker-logs-end" -> DockerLogEvent.Ended
            "docker-logs-error" -> DockerLogEvent.Failed((m["error"] as? JsonPrimitive)?.contentOrNull ?: "The logs stopped")
            else -> null
        }
    }.transformWhile { emit(it); it is DockerLogEvent.Lines }
}

/** A shell inside a container. Collect [events] to open it; cancelling closes it. */
class DockerShell internal constructor(private val socket: LiveSocket, val containerId: String, cols: Int, rows: Int) {
    val ref = newRef("s")
    val events: Flow<DockerShellEvent> = socket.messages("dsh:$ref", msg("type" to "docker-shell-open", "ref" to ref, "id" to containerId, "cols" to cols, "rows" to rows), msg("type" to "docker-shell-close", "ref" to ref)) {
        it.type().startsWith("docker-shell") && (it["ref"] as? JsonPrimitive)?.contentOrNull == ref
    }.mapNotNull { m ->
        when (m.type()) {
            "docker-shell-data" -> DockerShellEvent.Data((m["data"] as? JsonPrimitive)?.contentOrNull ?: "")
            "docker-shell-exit" -> DockerShellEvent.Exited((m["code"] as? JsonPrimitive)?.intOrNull ?: 0)
            "docker-shell-error" -> DockerShellEvent.Failed((m["error"] as? JsonPrimitive)?.contentOrNull ?: "The shell could not open")
            else -> null
        }
    }.transformWhile { emit(it); it is DockerShellEvent.Data }

    fun input(data: String) = socket.send(msg("type" to "docker-shell-input", "ref" to ref, "data" to data))
    fun resize(cols: Int, rows: Int) = socket.send(msg("type" to "docker-shell-resize", "ref" to ref, "cols" to cols, "rows" to rows))
}

/** Live server log lines (after [SystemApi.logs] for the backlog). */
internal fun serverLogs(socket: LiveSocket): Flow<ServerLog> =
    socket.messages("logs", msg("type" to "subscribe-logs"), null) { it.type() == "server-log" }
        .mapNotNull { runCatching { CrundiJson.decodeFromJsonElement<ServerLog>(JsonObject(it - "type")) }.getOrNull() }

