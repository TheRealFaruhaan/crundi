package mv.obitech.crundi.api

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import java.io.OutputStream
import java.util.concurrent.CopyOnWriteArrayList

/**
 * A server in memory, for tests and screenshots: answers each call from
 * [routes] ("GET /api/kanban" -> JSON text, or a function of the request),
 * records what was asked ([requests]), and lets a test push event-stream
 * frames ([pushEvent]) and socket messages ([pushSocket]).
 */
class FakeTransport(override val baseUrl: String = "https://dev.example.com") : Transport {
    val routes = LinkedHashMap<String, (ApiRequest) -> ApiResponse>()
    val requests = CopyOnWriteArrayList<ApiRequest>()
    /** What the app sent on the socket. */
    val socketSent = CopyOnWriteArrayList<String>()
    /** The body of the last upload, as it would have gone to the server. */
    @Volatile var lastUpload: ByteArray? = null
    private val sse = MutableSharedFlow<SseFrame>(extraBufferCapacity = 64)
    private var listener: SocketListener? = null

    /** Answer "METHOD /path" with this JSON (status 200 unless given). */
    fun on(route: String, json: String, status: Int = 200): FakeTransport { routes[route] = { ApiResponse(status, json) }; return this }
    fun on(route: String, answer: (ApiRequest) -> ApiResponse): FakeTransport { routes[route] = answer; return this }

    override suspend fun exchange(request: ApiRequest, progress: Progress?): ApiResponse {
        requests.add(request)
        request.upload?.let { u -> lastUpload = u.toBytes(); progress?.invoke(u.sourceSize, u.sourceSize) }
        val answer = routes["${request.method} ${request.path}"] ?: run {
            System.getProperty("fake.missing")?.let { java.io.File(it).appendText("${request.method} ${request.path} ${request.query}\n") }
            return ApiResponse(404, """{"ok":false,"error":"Not found"}""")
        }
        return answer(request)
    }

    override suspend fun download(request: ApiRequest, sink: OutputStream, progress: Progress?): DownloadInfo {
        val r = exchange(request, null)
        if (r.status !in 200..299) throw OkHttpTransport.httpError(r.status, r.text)
        val bytes = r.bytes ?: r.text.toByteArray()
        sink.write(bytes)
        progress?.invoke(bytes.size.toLong(), bytes.size.toLong())
        return DownloadInfo(OkHttpTransport.filenameOf(r.headers["Content-Disposition"], request), r.headers["Content-Type"].orEmpty(), bytes.size.toLong())
    }

    /** Frames every new event-stream connection gets first, as the server sends its "state" on connect. */
    val initialEvents = CopyOnWriteArrayList<SseFrame>()

    override fun events(path: String): Flow<SseFrame> = if (!eventsEnabled) emptyFlow() else flow {
        for (f in initialEvents) emit(f)
        emitAll(sse)
    }
    /** Off: the event stream connects and ends at once (screens that need no live data). */
    var eventsEnabled = true

    fun pushEvent(event: String, data: String) { sse.tryEmit(SseFrame(event, data)) }

    override fun socket(listener: SocketListener): SocketConnection {
        this.listener = listener
        listener.onOpen()
        return object : SocketConnection {
            override fun send(text: String): Boolean {
                socketSent.add(text)
                // A server that answers: e.g. subscribe-ui gets the chat's history.
                onSocketSend?.invoke(text)?.forEach { listener.onMessage(it) }
                return true
            }
            override fun close() { listener.onClosed(0, "closed") }
        }
    }

    /** What the fake server answers to a message the app sends on the socket (null: nothing). */
    var onSocketSend: ((String) -> List<String>?)? = null

    /** A message from the server on the socket. */
    fun pushSocket(json: String) { listener?.onMessage(json) }
}
