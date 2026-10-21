package mv.obitech.crundi.api

import kotlinx.coroutines.flow.Flow
import kotlinx.serialization.json.JsonElement
import java.io.OutputStream

/** One call to the server, before it is sent. [path] starts with "/api/". */
class ApiRequest(
    val method: String,
    val path: String,
    val query: Map<String, String?> = emptyMap(),
    /** A JSON body (sent as application/json). */
    val json: JsonElement? = null,
    /** A raw body (uploads). Mutually exclusive with [json]. */
    val upload: Upload? = null,
    val headers: Map<String, String> = emptyMap(),
    /** Longer than the default 30 s read timeout (git push, a big search). */
    val slow: Boolean = false,
)

/**
 * A body written straight to the connection, so a large file is never held
 * in memory. The server takes every upload as JSON with the file in base64
 * (`{"project":..,"name":..,"data":"<base64>"}`); [Uploads.base64Json] builds
 * that from a stream.
 */
class Upload(
    val contentType: String,
    /** Bytes, or -1 if unknown. */
    val length: Long,
    /** Writes the body; [count] is told how many bytes of the SOURCE file have gone, for progress. */
    val write: (out: OutputStream, count: (Long) -> Unit) -> Unit,
    /** Size of the source file, for progress (-1 if unknown). */
    val sourceSize: Long = -1,
)

/** What came back. [text] is the body for JSON answers; [bytes] for binary ones. */
class ApiResponse(
    val status: Int,
    val text: String = "",
    val headers: Map<String, String> = emptyMap(),
    val bytes: ByteArray? = null,
)

/** A file being downloaded: what the server says it is. */
data class DownloadInfo(val filename: String, val contentType: String, val size: Long)

/** One frame of the server's event stream (GET /api/events). */
data class SseFrame(val event: String, val data: String)

/** Progress of an upload or download, in bytes; [total] is -1 when unknown. */
typealias Progress = (done: Long, total: Long) -> Unit

/**
 * How the client reaches the server. [OkHttpTransport] is the real one;
 * FakeTransport (in src/test) answers from canned JSON for tests and screenshots.
 *
 * Every call here is authenticated as the phone's page login and renews it
 * once on a 401 before giving up (see [Credentials]).
 */
interface Transport {
    /** The server's address, "https://dev.example.com" (no trailing slash). */
    val baseUrl: String

    /** A call whose answer is read whole (JSON, or small bytes with [ApiRequest.headers] Accept). Never throws for an HTTP status: the caller reads it. Throws [ApiException] (status 0) when the server cannot be reached. */
    suspend fun exchange(request: ApiRequest, progress: Progress? = null): ApiResponse

    /** Stream a response body into [sink]. Throws [ApiException] for any non-2xx status. */
    suspend fun download(request: ApiRequest, sink: OutputStream, progress: Progress? = null): DownloadInfo

    /** One connection to the event stream; completes when it drops, throws [ApiException] if refused. */
    fun events(path: String = "/api/events"): Flow<SseFrame>

    /** Open the WebSocket (/ws). Blocking (it may fetch a token first); the listener is called from a background thread. */
    fun socket(listener: SocketListener): SocketConnection

    /** The socket was refused with 401: get a fresh login before opening it again. Blocking. Throws [ApiException]. */
    fun renewLogin() {}
}

interface SocketListener {
    fun onOpen() {}
    fun onMessage(text: String)
    /** The socket is gone; [code] is the HTTP status if the upgrade was refused (401, 426), else 0. */
    fun onClosed(code: Int, reason: String)
}

interface SocketConnection {
    fun send(text: String): Boolean
    fun close()
}

/**
 * The phone's login for the screens: the page login PageLogin keeps (one
 * renewal at a time for the whole app). Blocking; called off the main thread.
 */
interface Credentials {
    /** A token with time left, without going to the network if possible. Null when there is none yet. */
    fun token(): String?

    /**
     * A token was refused (or there was none): get a good one. [stale] is the
     * one that was refused. Throws [ApiException]: status 0 when the server
     * cannot be reached, 401 when this phone's sign-in has ended.
     */
    fun renew(stale: String?): String
}
