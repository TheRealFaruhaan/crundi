package mv.obitech.crundi.api

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.Job
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import okhttp3.Call
import okhttp3.Callback
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.BufferedSink
import java.io.IOException
import java.io.OutputStream
import java.net.URLDecoder
import java.util.concurrent.TimeUnit
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

/**
 * The real [Transport]: OkHttp, with the phone's page login on every call.
 *
 * [clientTag] is what the server reads to decide whether this app is too old
 * (X-Crundi-Client, see Api.CLIENT_TAG); a refused client gets 426.
 */
class OkHttpTransport(
    override val baseUrl: String,
    private val credentials: Credentials,
    private val clientTag: String,
    private val userAgent: String,
    base: OkHttpClient = shared,
) : Transport {

    private val http = base.newBuilder()
        .connectTimeout(12, TimeUnit.SECONDS)
        .readTimeout(30, TimeUnit.SECONDS)
        .writeTimeout(60, TimeUnit.SECONDS)
        .followRedirects(false)
        .build()
    private val slowHttp = http.newBuilder().readTimeout(5, TimeUnit.MINUTES).build()
    /** The event stream and the socket stay open indefinitely; the socket pings to notice a dead network. */
    private val longHttp = http.newBuilder().readTimeout(0, TimeUnit.SECONDS).pingInterval(25, TimeUnit.SECONDS).build()
    /**
     * The event stream. The server pushes the whole state every 5 seconds, so
     * 20 seconds of silence means the connection is dead (a phone that slept,
     * a network that changed) even though nothing said so. Without a limit
     * the app went on showing hours-old state, connected dot and all.
     */
    private val sseHttp = http.newBuilder().readTimeout(20, TimeUnit.SECONDS).build()

    private fun url(path: String, query: Map<String, String?>): HttpUrl {
        val b = (baseUrl + path).toHttpUrl().newBuilder()
        for ((k, v) in query) if (v != null) b.addQueryParameter(k, v)
        return b.build()
    }

    private fun build(req: ApiRequest, token: String?, progress: Progress?): Request {
        val b = Request.Builder().url(url(req.path, req.query))
            .header("X-Crundi-Client", clientTag)
            .header("User-Agent", userAgent)
            .header("Accept", "application/json")
        if (token != null) b.header("Authorization", "Bearer $token")
        for ((k, v) in req.headers) b.header(k, v)
        val body: RequestBody? = when {
            req.upload != null -> StreamBody(req.upload, progress)
            req.json != null -> req.json.toString().toRequestBody(JSON)
            req.method == "POST" || req.method == "PUT" || req.method == "PATCH" -> "{}".toRequestBody(JSON)
            else -> null
        }
        return b.method(req.method, body).build()
    }

    /** Send with the current token; on a 401 renew once and send again. */
    private suspend fun authed(req: ApiRequest, progress: Progress?, client: OkHttpClient): Response {
        val first = withContext(Dispatchers.IO) { credentials.token() ?: credentials.renew(null) }
        val r = client.newCall(build(req, first, progress)).await()
        if (r.code != 401) return r
        r.close()
        val fresh = withContext(Dispatchers.IO) { credentials.renew(first) }
        return client.newCall(build(req, fresh, progress)).await()
    }

    override suspend fun exchange(request: ApiRequest, progress: Progress?): ApiResponse {
        val client = if (request.slow || request.upload != null) slowHttp else http
        authed(request, progress, client).use { r ->
            val type = r.header("Content-Type").orEmpty()
            val headers = r.headers.toMultimap().mapValues { it.value.firstOrNull().orEmpty() }
            return withContext(Dispatchers.IO) {
                if (type.startsWith("application/json") || type.startsWith("text/")) ApiResponse(r.code, r.body?.string().orEmpty(), headers)
                else ApiResponse(r.code, "", headers, r.body?.bytes())
            }
        }
    }

    override suspend fun download(request: ApiRequest, sink: OutputStream, progress: Progress?): DownloadInfo {
        val r = authed(request, null, slowHttp)
        r.use {
            if (!r.isSuccessful) {
                val text = withContext(Dispatchers.IO) { r.body?.string().orEmpty() }
                throw httpError(r.code, text)
            }
            val body = r.body ?: throw ApiException(r.code, "The server sent nothing")
            val total = body.contentLength()
            val info = DownloadInfo(filenameOf(r.header("Content-Disposition"), request), r.header("Content-Type").orEmpty(), total)
            withContext(Dispatchers.IO) {
                body.byteStream().use { ins ->
                    val buf = ByteArray(64 * 1024)
                    var done = 0L
                    while (true) {
                        currentCoroutineContext().ensureActive()
                        val n = ins.read(buf)
                        if (n < 0) break
                        sink.write(buf, 0, n)
                        done += n
                        progress?.invoke(done, total)
                    }
                }
            }
            return info
        }
    }

    override fun events(path: String): Flow<SseFrame> = flow {
        val token = credentials.token() ?: credentials.renew(null)
        var call = sseHttp.newCall(build(ApiRequest("GET", path, headers = mapOf("Accept" to "text/event-stream")), token, null))
        var resp = call.executeCancellable()
        if (resp.code == 401) {
            resp.close()
            val fresh = credentials.renew(token)
            call = sseHttp.newCall(build(ApiRequest("GET", path, headers = mapOf("Accept" to "text/event-stream")), fresh, null))
            resp = call.executeCancellable()
        }
        // A blocked read does not see coroutine cancellation: cancel the call
        // itself, so a restart (the app coming back to the front) is immediate.
        val live = call
        currentCoroutineContext()[Job]?.invokeOnCompletion { live.cancel() }
        resp.use { r ->
            if (!r.isSuccessful) throw httpError(r.code, runCatching { r.body?.string() }.getOrNull().orEmpty())
            val src = r.body!!.source()
            val parser = SseParser()
            while (true) {
                currentCoroutineContext().ensureActive()
                val line = src.readUtf8Line() ?: break
                parser.line(line)?.let { emit(it) }
            }
        }
    }.flowOn(Dispatchers.IO)

    override fun socket(listener: SocketListener): SocketConnection {
        val wsUrl = baseUrl.replaceFirst("https://", "wss://").replaceFirst("http://", "ws://") + "/ws"
        val token = credentials.token() ?: credentials.renew(null)
        lastSocketToken = token
        val req = Request.Builder().url(wsUrl.replaceFirst("wss://", "https://").replaceFirst("ws://", "http://"))
            .header("X-Crundi-Client", clientTag).header("User-Agent", userAgent)
            .apply { if (token != null) header("Authorization", "Bearer $token") }
            .build()
        val ws = longHttp.newWebSocket(req, object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) = listener.onOpen()
            override fun onMessage(webSocket: WebSocket, text: String) = listener.onMessage(text)
            override fun onClosing(webSocket: WebSocket, code: Int, reason: String) { webSocket.close(1000, null) }
            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) = listener.onClosed(0, reason)
            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) =
                listener.onClosed(response?.code ?: 0, t.message ?: "connection lost")
        })
        return object : SocketConnection {
            override fun send(text: String) = ws.send(text)
            override fun close() { ws.close(1000, null) }
        }
    }

    @Volatile private var lastSocketToken: String? = null

    override fun renewLogin() { credentials.renew(lastSocketToken) }

    override fun streamHeaders(): Map<String, String> = buildMap {
        put("X-Crundi-Client", clientTag)
        put("User-Agent", userAgent)
        credentials.token()?.let { put("Authorization", "Bearer $it") }
    }

    private class StreamBody(private val upload: Upload, private val progress: Progress?) : RequestBody() {
        override fun contentType() = upload.contentType.toMediaType()
        override fun contentLength() = upload.length
        override fun writeTo(sink: BufferedSink) {
            val out = sink.outputStream()
            upload.write(out) { done -> progress?.invoke(done, upload.sourceSize) }
            out.flush()
        }
    }

    companion object {
        private val JSON = "application/json; charset=utf-8".toMediaType()
        /** One connection pool for every server and window. */
        val shared: OkHttpClient by lazy { OkHttpClient() }

        internal fun httpError(status: Int, text: String): ApiException {
            val obj = runCatching { CrundiJson.parseToJsonElement(text) as? kotlinx.serialization.json.JsonObject }.getOrNull()
            return ApiException(status, obj?.errorText() ?: defaultMessage(status, text), obj)
        }

        internal fun defaultMessage(status: Int, text: String = ""): String = when (status) {
            0 -> "Could not reach the server. Check the connection and try again."
            401 -> "This phone's sign-in has ended. Pair it again."
            403 -> "The server would not do that for this phone."
            404 -> text.trim().takeIf { it.isNotEmpty() && it.length < 120 } ?: "Not found."
            426 -> "This app is too old for the server. Update it."
            in 500..599 -> "The server had a problem (${status}). Try again."
            else -> text.trim().takeIf { it.isNotEmpty() && it.length < 160 } ?: "The server answered $status."
        }

        /** The name from Content-Disposition (RFC 5987 first), else the last part of the path asked for. */
        internal fun filenameOf(disposition: String?, req: ApiRequest): String {
            val d = disposition.orEmpty()
            Regex("filename\\*=UTF-8''([^;]+)", RegexOption.IGNORE_CASE).find(d)?.let {
                return runCatching { URLDecoder.decode(it.groupValues[1], "UTF-8") }.getOrDefault(it.groupValues[1])
            }
            Regex("filename=\"([^\"]*)\"").find(d)?.let { return it.groupValues[1] }
            val q = req.query["file"] ?: req.query["path"] ?: req.path
            return q.substringAfterLast('/').substringAfterLast('\\').ifEmpty { "download" }
        }
    }
}

/** Reads `event:`/`data:` lines into frames; a blank line ends a frame. */
class SseParser {
    private var event = "message"
    private val data = StringBuilder()

    fun line(line: String): SseFrame? {
        if (line.isEmpty()) {
            if (data.isEmpty()) { event = "message"; return null }
            val f = SseFrame(event, data.toString())
            event = "message"; data.setLength(0)
            return f
        }
        if (line.startsWith(":")) return null
        val k = line.substringBefore(':')
        val v = line.substringAfter(':', "").removePrefix(" ")
        when (k) {
            "event" -> event = v
            "data" -> { if (data.isNotEmpty()) data.append('\n'); data.append(v) }
        }
        return null
    }
}

private suspend fun Call.await(): Response = suspendCancellableCoroutine { cont ->
    cont.invokeOnCancellation { runCatching { cancel() } }
    enqueue(object : Callback {
        override fun onResponse(call: Call, response: Response) { cont.resume(response) }
        override fun onFailure(call: Call, e: IOException) {
            if (!cont.isCancelled) cont.resumeWithException(ApiException(0, OkHttpTransport.defaultMessage(0)))
        }
    })
}

/** execute() on this thread, cancelled with the coroutine. */
private suspend fun Call.executeCancellable(): Response {
    val job = currentCoroutineContext()[Job]
    val h = job?.invokeOnCompletion { runCatching { cancel() } }
    try {
        return try { execute() } catch (e: IOException) { throw ApiException(0, OkHttpTransport.defaultMessage(0)) }
    } finally { h?.dispose() }
}
