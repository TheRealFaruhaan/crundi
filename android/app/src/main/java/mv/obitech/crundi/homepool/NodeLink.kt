package mv.obitech.crundi.homepool

import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import okio.ByteString.Companion.toByteString
import org.json.JSONObject
import java.io.IOException
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.Socket
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executors
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.ScheduledFuture
import java.util.concurrent.ThreadFactory
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong

/**
 * One homepool node's connection to the server: the websocket at
 * /api/homepool/node, and the TCP streams the server asks it to open.
 * The Kotlin twin of startNode() in app/homepool-node.js.
 *
 * Plain JVM (OkHttp, org.json, java.net): no Android classes, so it runs in
 * a unit test against a stub server. The service decides when it runs; this
 * class only keeps it connected (backing off 1 s .. 60 s) until [stop], or
 * until the server refuses the key (401/403, or a final "bye").
 *
 * Threads: OkHttp's reader thread delivers frames; each stream has one thread
 * that connects and then reads the target, and one that writes to it, so a
 * slow target never holds up the websocket.
 */
class NodeLink(
    server: String,
    private val key: String,
    private val events: Events,
    /** Extra fields for the "info" message (network, publicIp, paused, bytes…). */
    private val info: () -> JSONObject = { JSONObject() },
    /** Tests only: let the node open loopback and private addresses. */
    private val allowPrivate: Boolean = false,
    /** Resolve a target's name on this device. */
    private val resolve: (String) -> Array<InetAddress> = { InetAddress.getAllByName(it) },
    baseClient: OkHttpClient? = null,
) {
    enum class State { CONNECTING, CONNECTED, STOPPED, REFUSED }

    interface Events {
        fun onState(state: State, detail: String) {}
        fun onLog(line: String) {}
        /** The server's "hello": who this node is. */
        fun onHello(name: String?, network: String?, networkName: String?) {}
        /** The server's "stats" message, as sent. */
        fun onStats(msg: JSONObject) {}
        /** Bytes carried on a stream: [up] target -> server, [down] server -> target. */
        fun onBytes(up: Long, down: Long) {}
    }

    private val base = server.trimEnd('/')
    private val url = base.replaceFirst(Regex("^http", RegexOption.IGNORE_CASE), "ws") + NODE_PATH

    private val client: OkHttpClient = (baseClient ?: OkHttpClient()).newBuilder()
        .pingInterval(25, TimeUnit.SECONDS)
        .connectTimeout(20, TimeUnit.SECONDS)
        .readTimeout(0, TimeUnit.MILLISECONDS)
        .build()

    private val timers = Executors.newSingleThreadScheduledExecutor(named("homepool-timer"))
    private val workers = Executors.newCachedThreadPool(named("homepool-stream"))

    @Volatile private var stopped = true
    @Volatile var state: State = State.STOPPED
        private set
    @Volatile private var conn: Conn? = null
    private var retryMs = 1000L
    private var retryTask: ScheduledFuture<*>? = null
    private var infoTask: ScheduledFuture<*>? = null
    private val lock = Any()

    /** While paused (by the network rules) new streams are refused; the server sees paused:true in "info". */
    @Volatile var paused: Boolean = false

    /** Streams open now. */
    val streamCount: Int get() = conn?.streams?.size ?: 0

    fun start() {
        synchronized(lock) {
            if (!stopped) return
            stopped = false
            retryMs = 1000
        }
        connect()
    }

    fun stop() {
        synchronized(lock) {
            stopped = true
            retryTask?.cancel(false); retryTask = null
            infoTask?.cancel(false); infoTask = null
        }
        val c = synchronized(lock) { conn.also { conn = null } }
        c?.let { it.shutdown(); it.ws?.close(1000, "stopped") }
        if (state != State.REFUSED) setState(State.STOPPED, "")
    }

    /** Stop for good: [stop] and let the threads go. */
    fun close() {
        stop()
        timers.shutdownNow()
        workers.shutdownNow()
    }

    /** Reconnect at once (the network changed under us: the old socket is as good as dead). */
    fun kick() {
        if (stopped) return
        log("network changed, reconnecting")
        val c = synchronized(lock) {
            retryMs = 1000; retryTask?.cancel(false); retryTask = null
            conn.also { conn = null }
        }
        if (c != null) { c.shutdown(); c.ws?.cancel() }
        connect()
    }

    fun sendInfo() {
        val c = conn ?: return
        if (!c.open) return
        val m = try { info() } catch (e: Exception) { JSONObject() }
        m.put("type", "info")
        c.ws?.send(m.toString())
    }

    // ── connecting ──

    private fun connect() {
        if (stopped) return
        setState(State.CONNECTING, "")
        val req = try {
            Request.Builder().url("$url?key=${java.net.URLEncoder.encode(key, "UTF-8")}")
                .header("Authorization", "Bearer $key")
                .build()
        } catch (e: Exception) {
            log("bad server address: ${e.message}")
            stopped = true
            setState(State.REFUSED, "bad server address")
            return
        }
        val c = Conn()
        synchronized(lock) {
            if (stopped) return
            conn = c
            c.ws = client.newWebSocket(req, c)
        }
    }

    private fun later() {
        synchronized(lock) {
            if (stopped) return
            val wait = retryMs
            retryMs = (retryMs * 2).coerceAtMost(60_000)
            setState(State.CONNECTING, "retrying in ${wait / 1000}s")
            retryTask?.cancel(false)
            retryTask = runCatching { timers.schedule({ connect() }, wait, TimeUnit.MILLISECONDS) }.getOrNull()
        }
    }

    private fun refused(reason: String) {
        synchronized(lock) {
            stopped = true
            retryTask?.cancel(false); retryTask = null
            infoTask?.cancel(false); infoTask = null
        }
        setState(State.REFUSED, reason)
    }

    private fun setState(s: State, detail: String) {
        state = s
        try { events.onState(s, detail) } catch (e: Exception) { /* the caller's problem */ }
    }

    private fun log(line: String) {
        try { events.onLog(line) } catch (e: Exception) { /* ignore */ }
    }

    /** One websocket connection and the streams it carries. A stale one's callbacks are ignored. */
    private inner class Conn : WebSocketListener() {
        /** Set as soon as OkHttp hands it over (a callback may come before newWebSocket returns). */
        @Volatile var ws: WebSocket? = null
        @Volatile var open = false
        @Volatile private var dead = false
        val streams = ConcurrentHashMap<Int, Stream>()

        private val current: Boolean get() = conn === this && !dead

        fun send(type: Int, id: Int, payload: ByteArray? = null): Boolean =
            if (dead) false else ws?.send(Wire.encode(type, id, payload).toByteString()) ?: false

        fun queued(): Long = if (dead) 0 else ws?.queueSize() ?: 0

        fun shutdown() {
            dead = true
            open = false
            for (s in streams.values) s.destroy()
            streams.clear()
        }

        override fun onOpen(webSocket: WebSocket, response: Response) {
            ws = webSocket
            if (!current) { webSocket.cancel(); return }
            open = true
            synchronized(lock) {
                retryMs = 1000
                infoTask?.cancel(false)
                infoTask = runCatching { timers.scheduleWithFixedDelay({ if (current) sendInfo() }, 30, 30, TimeUnit.SECONDS) }.getOrNull()
            }
            log("connected to $base")
            sendInfo()
            setState(State.CONNECTED, "")
        }

        override fun onMessage(webSocket: WebSocket, text: String) {
            if (!current) return
            val m = try { JSONObject(text) } catch (e: Exception) { return }
            when (m.optString("type")) {
                "hello" -> {
                    val n = m.optJSONObject("node")
                    val name = n?.optString("name")?.takeIf { it.isNotEmpty() }
                    val net = n?.optString("network")?.takeIf { it.isNotEmpty() }
                    val netName = n?.optString("networkName")?.takeIf { it.isNotEmpty() }
                    log("node \"${name ?: "?"}\" on network \"${netName ?: net ?: "-"}\"")
                    try { events.onHello(name, net, netName) } catch (e: Exception) { /* ignore */ }
                }
                "stats" -> try { events.onStats(m) } catch (e: Exception) { /* ignore */ }
                "bye" -> {
                    val reason = m.optString("reason")
                    log("server ended the node: $reason")
                    if (m.optBoolean("final")) refused(reason.ifEmpty { "ended by the server" })
                }
            }
        }

        override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
            if (!current) return
            val f = Wire.decode(bytes.toByteArray()) ?: return
            if (f.type == FrameType.OPEN) { open(f); return }
            val s = streams[f.id] ?: return
            when (f.type) {
                FrameType.DATA -> { events.onBytes(0, f.payload.size.toLong()); s.write(f.payload) }
                FrameType.CLOSE -> { streams.remove(f.id); s.finish() }
                FrameType.PAUSE -> s.remotePause(true)
                FrameType.RESUME -> s.remotePause(false)
            }
        }

        private fun open(f: Frame) {
            var host = ""
            var port = 0
            try {
                val j = JSONObject(String(f.payload, Charsets.UTF_8))
                host = j.optString("host")
                port = j.optInt("port")
            } catch (e: Exception) { /* bad frame */ }
            if (host.isEmpty() || port !in 1..65535) { send(FrameType.ERROR, f.id, "bad target".toByteArray()); return }
            if (paused) { send(FrameType.ERROR, f.id, "the node is paused".toByteArray()); return }
            // The server decides what to open, but a node must never become a way into its own home network.
            if (!allowPrivate && Targets.isPrivateHost(host)) { send(FrameType.ERROR, f.id, PRIVATE.toByteArray()); return }
            val s = Stream(this, f.id, host, port)
            streams[f.id] = s
            try { workers.execute { s.run() } } catch (e: Exception) { streams.remove(f.id); send(FrameType.ERROR, f.id, "node is stopping".toByteArray()) }
        }

        override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
            webSocket.close(1000, null)
        }

        override fun onClosed(webSocket: WebSocket, code: Int, reason: String) = gone("disconnected")

        override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
            if (!current) return
            val code = response?.code ?: 0
            if (!open && (code == 401 || code == 403)) {
                shutdown()
                log("the server refused this node key")
                refused("the server refused this node key")
                return
            }
            if (!open && code != 0) log("server answered $code") else log("connection error: ${t.message ?: t.javaClass.simpleName}")
            gone("disconnected")
        }

        private fun gone(why: String) {
            if (!current) return
            val wasOpen = open
            shutdown()
            synchronized(lock) { infoTask?.cancel(false); infoTask = null }
            if (stopped) return
            if (wasOpen) log(why)
            later()
        }
    }

    /** One TCP connection the server asked for. */
    private inner class Stream(private val c: Conn, val id: Int, private val host: String, private val port: Int) {
        @Volatile private var socket: Socket? = null
        @Volatile private var closed = false
        private val pauseLock = Object()
        private var remotePaused = false
        private val out = LinkedBlockingQueue<ByteArray>()
        private val queuedOut = AtomicLong()
        @Volatile private var sentPause = false

        /** Connect, say OPENED, then read the target until it ends. On a worker thread. */
        fun run() {
            val sock: Socket
            try {
                val addrs = resolve(host)
                val usable = if (allowPrivate) addrs.toList() else addrs.filterNot { Targets.isPrivateAddress(it) }
                if (usable.isEmpty()) { fail(if (addrs.isEmpty()) "ENOTFOUND" else PRIVATE); return }
                var last: IOException? = null
                var got: Socket? = null
                for (a in usable) {
                    if (closed) return
                    val s = Socket()
                    try {
                        s.tcpNoDelay = true
                        s.connect(InetSocketAddress(a, port), CONNECT_TIMEOUT_MS)
                        got = s
                        break
                    } catch (e: IOException) { last = e; runCatching { s.close() } }
                }
                sock = got ?: run { fail(reasonOf(last)); return }
            } catch (e: Exception) {
                fail(if (e is java.net.UnknownHostException) "ENOTFOUND" else reasonOf(e))
                return
            }
            socket = sock
            if (closed) { runCatching { sock.close() }; return }
            c.send(FrameType.OPENED, id)
            try { workers.execute { writeLoop(sock) } } catch (e: Exception) { destroy(); return }
            readLoop(sock)
        }

        private fun readLoop(sock: Socket) {
            val buf = ByteArray(32 * 1024)
            try {
                val inp = sock.getInputStream()
                while (!closed) {
                    // Backpressure: the server's side is full, or the websocket's queue is.
                    synchronized(pauseLock) { while (remotePaused && !closed) pauseLock.wait(500) }
                    if (c.queued() > WS_HIGH_WATER) {
                        while (!closed && c.queued() > WS_HIGH_WATER / 2) Thread.sleep(20)
                    }
                    if (closed) break
                    val n = inp.read(buf)
                    if (n < 0) break
                    if (n == 0) continue
                    events.onBytes(n.toLong(), 0)
                    if (!c.send(FrameType.DATA, id, buf.copyOf(n))) break
                }
                end(null)
            } catch (e: Exception) {
                end(if (closed) null else reasonOf(e))
            }
        }

        private fun writeLoop(sock: Socket) {
            try {
                val o = sock.getOutputStream()
                while (true) {
                    val b = out.take()
                    if (b === END) {
                        // The server closed its side: what is queued goes out, then the socket closes.
                        runCatching { o.flush(); sock.shutdownOutput() }
                        Thread.sleep(LINGER_MS)
                        destroy()
                        return
                    }
                    if (b === KILL || closed) return
                    o.write(b)
                    val left = queuedOut.addAndGet(-b.size.toLong())
                    if (sentPause && left < OUT_LOW) { sentPause = false; c.send(FrameType.RESUME, id) }
                }
            } catch (e: Exception) {
                if (!closed) end(reasonOf(e))
            }
        }

        /** DATA from the server, to the target. Never blocks the websocket's thread. */
        fun write(b: ByteArray) {
            if (closed) return
            out.put(b)
            val q = queuedOut.addAndGet(b.size.toLong())
            if (q > OUT_HIGH && !sentPause) { sentPause = true; c.send(FrameType.PAUSE, id) }
        }

        fun remotePause(on: Boolean) = synchronized(pauseLock) { remotePaused = on; pauseLock.notifyAll() }

        /** CLOSE from the server: finish writing, then close. */
        fun finish() { out.put(END); if (socket == null) destroy() }

        /** The target ended or failed: tell the server (once) and let the socket go. */
        private fun end(error: String?) {
            if (closed) return
            if (c.streams.remove(id, this)) {
                if (error != null) c.send(FrameType.ERROR, id, error.toByteArray()) else c.send(FrameType.CLOSE, id)
            }
            destroy()
        }

        private fun fail(reason: String) {
            if (c.streams.remove(id, this)) c.send(FrameType.ERROR, id, reason.toByteArray())
            destroy()
        }

        fun destroy() {
            if (closed) return
            closed = true
            runCatching { socket?.close() }
            out.clear()
            out.offer(KILL)
            remotePause(false)
        }
    }

    private fun reasonOf(e: Throwable?): String {
        val m = e?.message.orEmpty()
        return when {
            e is java.net.SocketTimeoutException -> "ETIMEDOUT"
            e is java.net.ConnectException && m.contains("refused", true) -> "ECONNREFUSED"
            e is java.net.NoRouteToHostException -> "EHOSTUNREACH"
            m.contains("reset", true) -> "ECONNRESET"
            m.isNotEmpty() -> m.take(120)
            else -> e?.javaClass?.simpleName ?: "error"
        }
    }

    companion object {
        private const val PRIVATE = "private address refused by the node"
        private const val CONNECT_TIMEOUT_MS = 15_000
        private const val LINGER_MS = 5_000L
        /** Queued for a target above this: PAUSE the server's side of that stream; RESUME below [OUT_LOW]. */
        private const val OUT_HIGH = 512L * 1024
        private const val OUT_LOW = 128L * 1024
        private val END = ByteArray(0)
        private val KILL = ByteArray(0)

        private fun named(prefix: String): ThreadFactory {
            val n = AtomicInteger()
            return ThreadFactory { r -> Thread(r, "$prefix-${n.incrementAndGet()}").apply { isDaemon = true } }
        }
    }
}
