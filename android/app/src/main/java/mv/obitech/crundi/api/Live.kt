package mv.obitech.crundi.api

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.delay
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.buffer
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.channelFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.onCompletion
import kotlinx.coroutines.flow.shareIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.decodeFromJsonElement
import mv.obitech.crundi.api.model.Approvals
import mv.obitech.crundi.api.model.SecretRequest
import mv.obitech.crundi.api.model.ServerState
import mv.obitech.crundi.api.model.Usage
import java.util.concurrent.CopyOnWriteArrayList
import kotlin.math.min

/** How the live connection is doing: the top bar's badge. */
enum class LiveStatus {
    /** Nothing is listening, so nothing is connected (the app is in the background). */
    Idle,
    Connecting,
    Connected,
    /** Dropped; trying again. */
    Reconnecting,
    /** Several tries in a row failed: the phone or the server is off the network. Still trying, more slowly. */
    Offline,
    /** The server refused this phone's sign-in: it has to be paired again. Not retried. */
    SignedOut,
}

/**
 * An event from the server's stream that says something changed and should
 * be fetched again: kanban {project}, notes {project, id, action, version,
 * by}, mindmap, media, schedule, factories {id}, factoryRun {id, project,
 * status}, backup, widget. [data] is what the server sent with it.
 */
data class ServerEvent(val name: String, val data: JsonObject) {
    val project: String? get() = data.str("project")
    val id: String? get() = data.str("id")
}

internal fun backoffMs(failures: Int): Long = min(30_000L, 1_000L shl min(failures, 5).coerceAtLeast(0))

/**
 * The server's event stream (GET /api/events), shared by every screen and
 * window of one server: connected while anything listens, reconnecting
 * with backoff when it drops, closed five seconds after the last listener
 * leaves. Each typed flow keeps the connection up while it is collected.
 */
class LiveEvents internal constructor(private val transport: Transport, scope: CoroutineScope, private val onSignedOut: () -> Unit) {
    private val _status = MutableStateFlow(LiveStatus.Idle)
    val status: StateFlow<LiveStatus> = _status

    private val _state = MutableStateFlow<ServerState?>(null)
    private val _usage = MutableStateFlow<Usage?>(null)
    private val _approvals = MutableStateFlow(Approvals())
    private val _secretRequests = MutableStateFlow<List<SecretRequest>>(emptyList())
    private val _changes = MutableSharedFlow<ServerEvent>(extraBufferCapacity = 64)
    private val kick = MutableStateFlow(0)

    private val _failure = MutableStateFlow<ApiException?>(null)
    /**
     * Why the stream last failed, when the server answered with an error
     * (a 500, a 403) or could not be reached; null once it is connected again,
     * or when it only dropped. Lets a screen word it with errorText.
     */
    val failure: StateFlow<ApiException?> = _failure

    /** The last state seen, without connecting (null before the first). */
    val lastState: ServerState? get() = _state.value

    private val hub: SharedFlow<Unit> = flow<Unit> {
        var failures = 0
        while (true) {
            _status.value = if (failures == 0) LiveStatus.Connecting else if (failures >= 3) LiveStatus.Offline else LiveStatus.Reconnecting
            // A kick (the app back in front, a retry) restarts even a stream
            // that looks connected: after a sleep it may be dead without
            // knowing, and a new connection brings the whole state at once.
            val k0 = kick.value
            var kicked = false
            try {
                coroutineScope {
                    val stream = launch {
                        transport.events().collect { f ->
                            if (_status.value != LiveStatus.Connected) { _status.value = LiveStatus.Connected; failures = 0; _failure.value = null; if (_usage.value == null) storedUsage() }
                            dispatch(f)
                        }
                    }
                    val watch = launch { kick.first { it != k0 }; kicked = true; stream.cancel() }
                    stream.join()
                    watch.cancel()
                }
                if (kicked) { failures = 0; continue }
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiException) {
                _failure.value = e
                if (e.signedOut) {
                    _status.value = LiveStatus.SignedOut
                    onSignedOut()
                    // Wait for a kick (pairing again builds a new client; a manual retry kicks).
                    val k = kick.value
                    kick.first { it != k }
                    failures = 0
                    continue
                }
            } catch (_: Exception) { _failure.value = null /* dropped, not refused: try again */ }
            failures++
            _status.value = if (failures >= 3) LiveStatus.Offline else LiveStatus.Reconnecting
            val k = kick.value
            withTimeoutOrNull(backoffMs(failures)) { kick.first { it != k } }
        }
    }.onCompletion { _status.value = LiveStatus.Idle }
        .shareIn(scope, SharingStarted.WhileSubscribed(5_000))

    internal fun dispatch(f: SseFrame) {
        val data = runCatching { CrundiJson.parseToJsonElement(f.data) }.getOrNull()
        val obj = data as? JsonObject ?: JsonObject(emptyMap())
        when (f.event) {
            "state" -> runCatching { CrundiJson.decodeFromJsonElement<ServerState>(obj) }.getOrNull()?.let { _state.value = it }
            "usage" -> {
                val u = runCatching { CrundiJson.decodeFromJsonElement<Usage>(obj) }.getOrNull()
                if (u != null && u.ok) _usage.value = u
                // A live fetch the server could not make (the account's usage
                // is rate limited, often when two servers share it): the last
                // stored reading, as the web page shows, rather than nothing.
                else if (_usage.value == null) storedUsage()
            }
            "approvals" -> runCatching { CrundiJson.decodeFromJsonElement<Approvals>(obj) }.getOrNull()?.let { _approvals.value = it }
            "secret-requests" -> _secretRequests.value = runCatching { CrundiJson.decodeFromJsonElement<List<SecretRequest>>(obj["requests"] ?: JsonArray(emptyList())) }.getOrDefault(emptyList())
            else -> _changes.tryEmit(ServerEvent(f.event, obj))
        }
    }

    private val usageScope = scope
    private fun storedUsage() {
        usageScope.launch {
            runCatching {
                val r = transport.exchange(ApiRequest("GET", "/api/usage/latest"))
                if (r.status in 200..299) CrundiJson.decodeFromString<Usage>(r.text).takeIf { it.ok && (it.week != null || it.fiveHour != null) }?.let { if (_usage.value == null) _usage.value = it }
            }
        }
    }

    /** Keep the stream up while [f] is collected. */
    private fun <T> live(f: Flow<T>): Flow<T> = channelFlow {
        launch { hub.collect {} }
        f.collect { send(it) }
    }

    /** The whole workbench (projects, services, chats, terminals, runs). Null until the first push. */
    val state: Flow<ServerState?> get() = live(_state)
    /** Account usage; null until known (and always for a collaborator). */
    val usage: Flow<Usage?> get() = live(_usage)
    /** Everything waiting on the person: the top bar's count. */
    val approvals: Flow<Approvals> get() = live(_approvals)
    /** Claude asking to use a secret: show the PIN sheet. */
    val secretRequests: Flow<List<SecretRequest>> get() = live(_secretRequests)
    /** "Fetch again" events for the areas (kanban, notes, mindmap, media, schedule, factories, factoryRun...). */
    val changes: Flow<ServerEvent> get() = live(_changes)
    /** Events of one name only, e.g. changes("kanban"). */
    fun changes(vararg names: String): Flow<ServerEvent> = live(flow { _changes.collect { if (it.name in names) emit(it) } })

    /** Try again now (a retry button, the app coming back to the front). */
    fun reconnect() { kick.value++ }
}

/**
 * The server's WebSocket (/ws), shared by every screen of one server: chat
 * streams, terminals, docker logs and shells, server logs, presence. Open
 * while anything uses it; every subscription is sent again after a
 * reconnect (a chat then gets its whole history again, which resyncs it).
 */
class LiveSocket internal constructor(private val transport: Transport, private val scope: CoroutineScope, private val onSignedOut: () -> Unit) {
    private val _status = MutableStateFlow(LiveStatus.Idle)
    val status: StateFlow<LiveStatus> = _status

    private val lock = Any()
    private var conn: SocketConnection? = null
    private var connected = false
    private var replayed = false
    private var loop: Job? = null
    private var closer: Job? = null
    private var users = 0
    /** Subscription key -> (count, the message that subscribes). */
    private val subs = LinkedHashMap<String, Pair<Int, String>>()
    private val listeners = CopyOnWriteArrayList<(JsonObject) -> Unit>()
    private val kick = MutableStateFlow(0)

    /** Send now if connected (dropped otherwise: subscriptions are replayed on connect, input is not). */
    fun send(msg: JsonObject): Boolean = synchronized(lock) { if (connected) conn?.send(msg.toString()) == true else false }

    /**
     * Use the socket: [listener] sees every message while held. [key] and
     * [subscribe] name a subscription to (re)send on every connect; it is
     * sent once however many hold it, and [unsubscribe] is sent when the last
     * lets go. Call the returned function to let go.
     */
    fun hold(key: String?, subscribe: JsonObject?, unsubscribe: JsonObject?, listener: (JsonObject) -> Unit): () -> Unit {
        listeners.add(listener)
        synchronized(lock) {
            users++
            closer?.cancel(); closer = null
            if (key != null && subscribe != null) {
                val had = subs[key]
                subs[key] = Pair((had?.first ?: 0) + 1, subscribe.toString())
                if (connected && (had == null || key.startsWith("ui:"))) conn?.send(subscribe.toString())
            }
            if (loop == null || loop?.isActive != true) loop = scope.launch(Dispatchers.IO) { run() }
        }
        var released = false
        return {
            synchronized(lock) {
                if (!released) {
                    released = true
                    listeners.remove(listener)
                    if (key != null) {
                        val had = subs[key]
                        if (had != null) {
                            if (had.first <= 1) { subs.remove(key); if (unsubscribe != null && connected) conn?.send(unsubscribe.toString()) }
                            else subs[key] = Pair(had.first - 1, had.second)
                        }
                    }
                    users--
                    if (users <= 0) {
                        closer = scope.launch { delay(10_000); synchronized(lock) { if (users <= 0) { loop?.cancel(); loop = null; conn?.close(); conn = null; connected = false; _status.value = LiveStatus.Idle } } }
                    }
                }
            }
        }
    }

    private suspend fun run() {
        var failures = 0
        while (true) {
            _status.value = if (failures == 0) LiveStatus.Connecting else if (failures >= 3) LiveStatus.Offline else LiveStatus.Reconnecting
            val closed = kotlinx.coroutines.CompletableDeferred<Int>()
            synchronized(lock) { replayed = false; conn = null; connected = false }
            val c = try {
                transport.socket(object : SocketListener {
                    override fun onOpen() {
                        synchronized(lock) {
                            connected = true
                            if (conn != null && !replayed) { replayed = true; for ((_, v) in subs) conn?.send(v.second) }
                        }
                        _status.value = LiveStatus.Connected
                    }
                    override fun onMessage(text: String) {
                        val o = runCatching { CrundiJson.parseToJsonElement(text) as? JsonObject }.getOrNull() ?: return
                        for (l in listeners) runCatching { l(o) }
                    }
                    override fun onClosed(code: Int, reason: String) { synchronized(lock) { connected = false; conn = null }; closed.complete(code) }
                })
            } catch (e: ApiException) {
                if (e.signedOut) { _status.value = LiveStatus.SignedOut; onSignedOut(); waitKick(); failures = 0; continue }
                null
            } catch (e: CancellationException) { throw e } catch (_: Exception) { null }
            if (c != null) {
                // onOpen can run before newWebSocket returns: replay the subscriptions here then.
                synchronized(lock) {
                    if (!closed.isCompleted) conn = c
                    if (connected && conn != null && !replayed) { replayed = true; for ((_, v) in subs) c.send(v.second) }
                }
                val code = try { closed.await() } catch (e: CancellationException) { c.close(); throw e }
                if (_status.value == LiveStatus.Connected) failures = 0
                if (code == 401) {
                    try { transport.renewLogin() } catch (e: ApiException) {
                        if (e.signedOut) { _status.value = LiveStatus.SignedOut; onSignedOut(); waitKick(); failures = 0; continue }
                    }
                }
            }
            failures++
            _status.value = if (failures >= 3) LiveStatus.Offline else LiveStatus.Reconnecting
            val k = kick.value
            withTimeoutOrNull(backoffMs(failures)) { kick.first { it != k } }
        }
    }

    private suspend fun waitKick() { val k = kick.value; kick.first { it != k } }

    fun reconnect() { kick.value++ }

    /**
     * Ask again for what [key] subscribes to, as a reconnect does: for a chat
     * the server sends its whole history again, which replaces what the
     * screen has (a bubble opened again, the app back in front, a stream that
     * went quiet). Not connected: try now rather than after the backoff, and
     * the connect replays it. False when nothing holds [key].
     */
    fun resync(key: String): Boolean {
        val sent = synchronized(lock) {
            val sub = subs[key] ?: return false
            connected && conn?.send(sub.second) == true
        }
        if (!sent) reconnect()
        return true
    }

    /**
     * Messages of one type (and id), while collected. Every one of them,
     * however slowly they are read: the channel has no limit. It kept 64
     * and dropped the rest without a word, so a bubble opened again after
     * being folded away, which gets all it missed in one burst, lost the end
     * of a reply and the turn's end, and showed "working…" for ever.
     */
    fun messages(key: String?, subscribe: JsonObject?, unsubscribe: JsonObject?, accept: (JsonObject) -> Boolean): Flow<JsonObject> = callbackFlow {
        val release = hold(key, subscribe, unsubscribe) { if (accept(it)) trySend(it) }
        awaitClose { release() }
    }.buffer(Channel.UNLIMITED)
}

internal fun msg(vararg f: Pair<String, Any?>): JsonObject = Uploads.toJson(mapOf(*f).filterValues { it != null }) as JsonObject
internal fun JsonObject.type(): String = (this["type"] as? JsonPrimitive)?.content ?: ""
