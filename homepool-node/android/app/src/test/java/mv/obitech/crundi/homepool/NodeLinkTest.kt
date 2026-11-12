package mv.obitech.crundi.homepool

import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okio.ByteString
import okio.ByteString.Companion.toByteString
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.net.InetAddress
import java.net.ServerSocket
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import kotlin.concurrent.thread

/**
 * The node against a stub server (MockWebServer speaking the server's side of
 * the protocol, as src/homepool.js does) and a local echo target.
 */
class NodeLinkTest {
    private lateinit var server: MockWebServer
    private val frames = LinkedBlockingQueue<Frame>()
    private val texts = LinkedBlockingQueue<JSONObject>()
    private val states = LinkedBlockingQueue<NodeLink.State>()
    @Volatile private var serverWs: WebSocket? = null
    private val opened = LinkedBlockingQueue<Boolean>()
    private var echo: ServerSocket? = null
    private var link: NodeLink? = null

    private val serverSide = object : WebSocketListener() {
        override fun onOpen(webSocket: WebSocket, response: Response) {
            serverWs = webSocket
            webSocket.send(JSONObject().put("type", "hello").put("node", JSONObject().put("id", "n1").put("name", "phone").put("network", "w1").put("networkName", "Home")).toString())
            opened.put(true)
        }
        override fun onMessage(webSocket: WebSocket, text: String) { texts.put(JSONObject(text)) }
        override fun onMessage(webSocket: WebSocket, bytes: ByteString) { frames.put(Wire.decode(bytes.toByteArray())!!) }
        override fun onClosing(webSocket: WebSocket, code: Int, reason: String) { webSocket.close(1000, null) }
    }

    private val events = object : NodeLink.Events {
        override fun onState(state: NodeLink.State, detail: String) { states.put(state) }
    }

    @Before fun setUp() {
        server = MockWebServer()
        server.start(InetAddress.getByName("127.0.0.1"), 0)
    }

    @After fun tearDown() {
        link?.close()
        echo?.close()
        runCatching { serverWs?.close(1001, null) }
        runCatching { server.shutdown() }
    }

    private fun startEcho(): Int {
        val ss = ServerSocket(0, 50, InetAddress.getByName("127.0.0.1"))
        echo = ss
        thread(isDaemon = true) {
            while (!ss.isClosed) {
                val s = try { ss.accept() } catch (e: Exception) { return@thread }
                thread(isDaemon = true) {
                    s.use { val i = it.getInputStream(); val o = it.getOutputStream(); val b = ByteArray(8192)
                        while (true) { val n = i.read(b); if (n < 0) break; o.write(b, 0, n); o.flush() } }
                }
            }
        }
        return ss.localPort
    }

    private fun base() = "http://127.0.0.1:${server.port}"

    private fun send(type: Int, id: Int, payload: ByteArray? = null) { serverWs!!.send(Wire.encode(type, id, payload).toByteString()) }

    private fun next(): Frame = frames.poll(10, TimeUnit.SECONDS) ?: throw AssertionError("no frame from the node")

    private fun waitState(want: NodeLink.State) {
        val end = System.currentTimeMillis() + 10_000
        while (System.currentTimeMillis() < end) { if (states.poll(1, TimeUnit.SECONDS) == want) return }
        throw AssertionError("never reached $want")
    }

    @Test fun signsInSendsInfoAndCarriesAStream() {
        server.enqueue(MockResponse().withWebSocketUpgrade(serverSide))
        val port = startEcho()
        link = NodeLink(base(), "hpn_n1.secret", events, info = { JSONObject().put("platform", "android").put("network", "Wi-Fi") }, allowPrivate = true).also { it.start() }

        val req = server.takeRequest(10, TimeUnit.SECONDS)!!
        assertEquals("/api/homepool/node?key=hpn_n1.secret", req.path)
        assertEquals("Bearer hpn_n1.secret", req.getHeader("Authorization"))
        opened.poll(10, TimeUnit.SECONDS)
        waitState(NodeLink.State.CONNECTED)

        val info = texts.poll(10, TimeUnit.SECONDS)!!
        assertEquals("info", info.getString("type"))
        assertEquals("android", info.getString("platform"))

        send(FrameType.OPEN, 7, JSONObject().put("host", "127.0.0.1").put("port", port).toString().toByteArray())
        val o = next()
        assertEquals(FrameType.OPENED, o.type); assertEquals(7, o.id)

        send(FrameType.DATA, 7, "hello there".toByteArray())
        val got = StringBuilder()
        while (got.length < "hello there".length) {
            val f = next(); assertEquals(FrameType.DATA, f.type); assertEquals(7, f.id); got.append(String(f.payload))
        }
        assertEquals("hello there", got.toString())

        // A big transfer comes back whole (backpressure must not lose or reorder bytes).
        val big = ByteArray(3 * 1024 * 1024) { (it * 31 + 7).toByte() }
        var off = 0
        while (off < big.size) { val n = minOf(60_000, big.size - off); send(FrameType.DATA, 7, big.copyOfRange(off, off + n)); off += n }
        val back = java.io.ByteArrayOutputStream()
        while (back.size() < big.size) {
            val f = next()
            if (f.type == FrameType.PAUSE || f.type == FrameType.RESUME) continue
            assertEquals(FrameType.DATA, f.type); back.write(f.payload)
        }
        assertArrayEquals(big, back.toByteArray())

        // The node's other end closing: the server closes, the node lets the socket go.
        send(FrameType.CLOSE, 7)
        // An unknown target port: ERROR, not OPENED.
        send(FrameType.OPEN, 8, JSONObject().put("host", "127.0.0.1").put("port", 1).toString().toByteArray())
        var e = next(); while (e.id != 8) e = next()
        assertEquals(FrameType.ERROR, e.type)
    }

    @Test fun refusesPrivateTargets() {
        server.enqueue(MockResponse().withWebSocketUpgrade(serverSide))
        link = NodeLink(base(), "hpn_n1.secret", events).also { it.start() }
        opened.poll(10, TimeUnit.SECONDS)
        waitState(NodeLink.State.CONNECTED)
        for ((i, h) in listOf("127.0.0.1", "localhost", "10.1.2.3", "192.168.0.10", "[::1]").withIndex()) {
            send(FrameType.OPEN, 100 + i, JSONObject().put("host", h).put("port", 80).toString().toByteArray())
            val f = next()
            assertEquals(FrameType.ERROR, f.type); assertEquals(100 + i, f.id)
            assertTrue(String(f.payload).contains("private"))
        }
        // A NAME that resolves to a private address is refused too.
        link?.close()
        frames.clear()
        server.enqueue(MockResponse().withWebSocketUpgrade(serverSide))
        link = NodeLink(base(), "hpn_n1.secret", events, resolve = { arrayOf(InetAddress.getByAddress(it, byteArrayOf(192.toByte(), 168.toByte(), 1, 1))) }).also { it.start() }
        opened.poll(10, TimeUnit.SECONDS)
        waitState(NodeLink.State.CONNECTED)
        send(FrameType.OPEN, 200, JSONObject().put("host", "rebind.example").put("port", 80).toString().toByteArray())
        val f = next()
        assertEquals(FrameType.ERROR, f.type)
        assertTrue(String(f.payload).contains("private"))
    }

    @Test fun pausedNodeRefusesNewStreams() {
        server.enqueue(MockResponse().withWebSocketUpgrade(serverSide))
        link = NodeLink(base(), "hpn_n1.secret", events).also { it.paused = true; it.start() }
        opened.poll(10, TimeUnit.SECONDS)
        waitState(NodeLink.State.CONNECTED)
        send(FrameType.OPEN, 1, JSONObject().put("host", "example.com").put("port", 443).toString().toByteArray())
        val f = next()
        assertEquals(FrameType.ERROR, f.type)
    }

    @Test fun aRefusedKeyIsFinal() {
        server.enqueue(MockResponse().setResponseCode(401))
        link = NodeLink(base(), "hpn_n1.bad", events).also { it.start() }
        waitState(NodeLink.State.REFUSED)
        Thread.sleep(1500)
        assertEquals(1, server.requestCount)   // not retried
    }

    @Test fun aFinalByeIsFinal() {
        server.enqueue(MockResponse().withWebSocketUpgrade(serverSide))
        link = NodeLink(base(), "hpn_n1.secret", events).also { it.start() }
        opened.poll(10, TimeUnit.SECONDS)
        waitState(NodeLink.State.CONNECTED)
        serverWs!!.send(JSONObject().put("type", "bye").put("reason", "removed by the owner").put("final", true).toString())
        serverWs!!.close(4001, null)
        waitState(NodeLink.State.REFUSED)
        Thread.sleep(1500)
        assertEquals(1, server.requestCount)
    }

    @Test fun reconnectsAfterADrop() {
        server.enqueue(MockResponse().withWebSocketUpgrade(serverSide))
        server.enqueue(MockResponse().withWebSocketUpgrade(serverSide))
        link = NodeLink(base(), "hpn_n1.secret", events).also { it.start() }
        opened.poll(10, TimeUnit.SECONDS)
        waitState(NodeLink.State.CONNECTED)
        serverWs!!.close(1001, "going away")
        waitState(NodeLink.State.CONNECTING)
        assertTrue(opened.poll(10, TimeUnit.SECONDS) == true)
        waitState(NodeLink.State.CONNECTED)
        assertEquals(2, server.requestCount)
    }
}
