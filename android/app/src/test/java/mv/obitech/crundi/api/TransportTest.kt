package mv.obitech.crundi.api

import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okhttp3.mockwebserver.Dispatcher
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okhttp3.mockwebserver.RecordedRequest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Before
import org.junit.Test
import java.io.ByteArrayOutputStream
import java.util.Base64
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit

/** The real OkHttp transport against a local server speaking the server's wire format. */
class TransportTest {
    private val server = MockWebServer()
    private val renewals = CopyOnWriteArrayList<String?>()
    private var token = "tok-1"
    private val creds = object : Credentials {
        override fun token() = token
        override fun renew(stale: String?): String { renewals.add(stale); token = "tok-2"; return token }
    }

    @Before fun start() = server.start()
    @After fun stop() = server.shutdown()

    private fun transport() = OkHttpTransport(server.url("/").toString().trimEnd('/'), creds, "android-play/1.1.0-dev.1", "Crundi test")

    @Test fun signsEveryCallAndNamesTheApp() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"projects":[{"alias":"crundi","name":"crundi","path":"/p","icon":""}]}""").setHeader("Content-Type", "application/json"))
        val list = CrundiClient(transport()).projects.list()
        assertEquals("crundi", list.single().alias)
        val r = server.takeRequest()
        assertEquals("Bearer tok-1", r.getHeader("Authorization"))
        assertEquals("android-play/1.1.0-dev.1", r.getHeader("X-Crundi-Client"))
        assertEquals("/api/projects", r.path)
    }

    @Test fun aServiceKeyWithASpaceGoesOutAsPercent20() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"ok":true}""").setHeader("Content-Type", "application/json"))
        CrundiClient(transport()).services.restart("dhivo-socials:Social Desk")
        // What the server's decodeURIComponent turns back into the key ("+" would stay a plus).
        assertEquals("/api/services/dhivo-socials%3ASocial%20Desk/restart", server.takeRequest().path)
    }

    @Test fun renewsOnceOn401() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(401).setBody("""{"error":"Unauthorized"}""").setHeader("Content-Type", "application/json"))
        server.enqueue(MockResponse().setBody("""{"ok":true,"branch":"dev","files":[],"ahead":0,"behind":0}""").setHeader("Content-Type", "application/json"))
        val g = CrundiClient(transport()).git.status("crundi")
        assertEquals("dev", g.branch)
        assertEquals(listOf<String?>("tok-1"), renewals)
        server.takeRequest()
        val second = server.takeRequest()
        assertEquals("Bearer tok-2", second.getHeader("Authorization"))
        assertEquals("/api/git/info?project=crundi", second.path)
    }

    @Test fun okFalseWithStatus200IsAnError() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"ok":false,"error":"Task not found"}""").setHeader("Content-Type", "application/json"))
        try { CrundiClient(transport()).kanban.deleteTask("crundi", "t1"); fail() } catch (e: ApiException) {
            assertEquals("Task not found", e.message)
            assertEquals(200, e.status)
        }
        val body = server.takeRequest().body.readUtf8()
        assertEquals("""{"action":"deleteTask","project":"crundi","taskId":"t1"}""", body)
    }

    @Test fun plainTextErrorsAndUnreachable() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(404).setBody("File not found"))
        try { CrundiClient(transport()).files.download("crundi", "x.txt", ByteArrayOutputStream()); fail() } catch (e: ApiException) {
            assertEquals(404, e.status); assertEquals("File not found", e.message)
        }
        val dead = OkHttpTransport("http://127.0.0.1:1", creds, "t", "t")
        try { CrundiClient(dead).projects.list(); fail() } catch (e: ApiException) { assertTrue(e.offline) }
    }

    @Test fun uploadsStreamBase64Json() = runBlocking {
        server.enqueue(MockResponse().setBody("""{"ok":true,"path":"/p/crundi_attachments/1-shot.png","name":"1-shot.png","size":5}""").setHeader("Content-Type", "application/json"))
        val bytes = byteArrayOf(1, 2, 3, 4, 5)
        var seen = 0L
        val a = CrundiClient(transport()).chats.attach("crundi", "shot.png", bytes.size.toLong(), { d, _ -> seen = d }) { bytes.inputStream() }
        assertEquals("/p/crundi_attachments/1-shot.png", a.path)
        assertEquals(5L, seen)
        val r = server.takeRequest()
        val o = CrundiJson.parseToJsonElement(r.body.readUtf8()).jsonObject
        assertEquals("crundi", o["project"]!!.jsonPrimitive.content)
        assertEquals("shot.png", o["name"]!!.jsonPrimitive.content)
        assertTrue(Base64.getDecoder().decode(o["data"]!!.jsonPrimitive.content).contentEquals(bytes))
        assertEquals(r.bodySize, r.getHeader("Content-Length")!!.toLong())
    }

    @Test fun downloadsKeepTheServersName() = runBlocking {
        server.enqueue(MockResponse().setBody("hello").setHeader("Content-Type", "application/octet-stream")
            .setHeader("Content-Disposition", "attachment; filename=\"r_sum_.pdf\"; filename*=UTF-8''r%C3%A9sum%C3%A9.pdf"))
        val out = ByteArrayOutputStream()
        val info = CrundiClient(transport()).files.download("crundi", "docs/résumé.pdf", out)
        assertEquals("résumé.pdf", info.filename)
        assertEquals("hello", out.toString())
    }

    @Test fun eventStream() = runBlocking {
        val padding = (":" + " ".repeat(2048) + "\n").repeat(24)
        val body = padding +
            "event: usage\ndata: {\"ok\":true,\"fiveHour\":{\"utilization\":31,\"resetsAt\":null},\"week\":null}\n\n" +
            "event: kanban\ndata: {\"project\":\"crundi\"}\n\n"
        server.enqueue(MockResponse().setBody(body).setHeader("Content-Type", "text/event-stream"))
        val frames = withTimeout(10_000) { transport().events().take(2).toList() }
        assertEquals(listOf("usage", "kanban"), frames.map { it.event })
        assertEquals("{\"project\":\"crundi\"}", frames[1].data)
        assertEquals("Bearer tok-1", server.takeRequest().getHeader("Authorization"))
    }

    @Test fun liveEventsReconnectAndType() = runBlocking {
        server.enqueue(MockResponse().setResponseCode(500))
        server.enqueue(MockResponse().setBody("event: state\ndata: ${FixtureText.state}\n\nevent: approvals\ndata: {\"items\":[],\"count\":0}\n\n").setHeader("Content-Type", "text/event-stream"))
        val client = CrundiClient(transport())
        val st = withTimeout(15_000) { client.events.state.first { it != null } }
        assertEquals(4, st!!.projects.size)
    }

    @Test fun socketSubscribesAndStreamsAChat() = runBlocking {
        val received = LinkedBlockingQueue<String>()
        server.dispatcher = object : Dispatcher() {
            override fun dispatch(request: RecordedRequest): MockResponse =
                if (request.path == "/ws") MockResponse().withWebSocketUpgrade(object : WebSocketListener() {
                    override fun onMessage(webSocket: WebSocket, text: String) {
                        received.add(text)
                        if (text.contains("subscribe-ui")) webSocket.send(FixtureText.chatHistory)
                    }
                    override fun onOpen(webSocket: WebSocket, response: Response) {}
                }) else MockResponse().setResponseCode(404)
        }
        val client = CrundiClient(transport())
        val s = withTimeout(10_000) { client.chat("a1b2c3d4e5f60718").first { it.loaded } }
        assertEquals("Android API", s.title)
        assertEquals("""{"type":"subscribe-ui","id":"a1b2c3d4e5f60718"}""", received.poll(5, TimeUnit.SECONDS))
        val ws = server.takeRequest()
        assertEquals("Bearer tok-1", ws.getHeader("Authorization"))
        assertEquals("android-play/1.1.0-dev.1", ws.getHeader("X-Crundi-Client"))
    }
}

/** Fixture text for transport tests (the same files FakeApi serves). */
object FixtureText {
    private fun read(n: String) = javaClass.classLoader!!.getResource("api/$n")!!.readText()
    val state get() = CrundiJson.parseToJsonElement(read("state.json")).toString()
    val chatHistory get() = CrundiJson.parseToJsonElement(read("chat-history.json")).toString()
}
