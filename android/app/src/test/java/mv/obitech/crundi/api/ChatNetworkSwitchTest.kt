package mv.obitech.crundi.api

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import mv.obitech.crundi.api.model.AgentState
import mv.obitech.crundi.api.model.ChatState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * A long reply stopped mid-word ("plain answ") with the cursor still there
 * and the chat still "working" after the turn had ended, when the owner
 * switched the phone's network and back. The socket was dead without
 * knowing it: everything after went nowhere, the watchdog's resync was
 * queued on that same dead socket, a kick left an open connection alone,
 * and only OkHttp's ping noticed, up to a minute later. And when the old
 * connection's close came after a new one had opened, it marked the new
 * one "not connected", so no resync was ever sent again.
 */
class ChatNetworkSwitchTest {
    private val id = "a1b2c3d4e5f60718"
    private val full = "No desktop app connected: chats and Cru get a plain answer at once."

    private fun history(state: String, text: String, streaming: Boolean) =
        """{"type":"ui-history","id":"$id","session":{"id":"$id","project":"crundi","title":"Browser panel","status":"running","state":"$state","messages":[""" +
            """{"id":"u1","kind":"user","text":"go","seq":1},{"id":"a2","kind":"assistant-text","text":"$text","streaming":$streaming,"seq":2}],"agents":[]}}"""
    private fun delta(text: String) = """{"type":"ui-event","id":"$id","event":{"type":"delta","id":"a2","text":"$text"}}"""

    /** The server as it stands: mid-reply at first; once the turn ends, the whole reply and idle. */
    private class Server { @Volatile var ended = false }

    private fun setUp(server: Server): Pair<FakeTransport, CrundiClient> {
        val t = FakeTransport()
        t.eventsEnabled = false
        t.onSocketSend = { sent ->
            if (sent.contains("\"subscribe-ui\"")) listOf(if (server.ended) history("idle", full, false) else history("working", "", true)) else null
        }
        return t to CrundiClient(t)
    }

    /** Shows the reply up to "plain answ", then the network changes under it while the server finishes. */
    private suspend fun stuckMidWord(t: FakeTransport, server: Server, last: () -> ChatState) {
        withTimeout(5_000) { while (last().state != AgentState.Working) delay(10) }
        t.pushSocket(delta("No desktop app connected: chats and Cru get a plain answ"))
        withTimeout(5_000) { while (!last().messages.last().text.endsWith("answ")) delay(10) }
        t.killSocketSilently()
        // The rest of the reply and the turn's end: sent, and lost on the dead connection.
        t.pushSocket(delta("er at once."))
        t.pushSocket("""{"type":"ui-event","id":"$id","event":{"type":"patch","id":"a2","patch":{"text":"$full","streaming":false}}}""")
        t.pushSocket("""{"type":"ui-event","id":"$id","event":{"type":"state","state":"idle"}}""")
        server.ended = true
        delay(100)
        assertEquals(AgentState.Working, last().state)
        assertTrue(last().messages.last().streaming)
    }

    @Test fun aNetworkChangeMakesTheSocketAgainAndTheReplyEnds() = runBlocking {
        val server = Server()
        val (t, c) = setUp(server)
        var last = ChatState(id)
        val job = launch(Dispatchers.Default) { c.chat(id).collect { last = it } }
        stuckMidWord(t, server) { last }
        // What the app did before: a resync, queued on the dead socket. Nothing comes.
        c.resyncChat(id)
        delay(200)
        assertEquals(AgentState.Working, last.state)
        // The phone says its network changed: a new connection, the history again.
        c.networkChanged()
        withTimeout(5_000) { while (last.state != AgentState.Idle) delay(10) }
        assertEquals(2, t.sockets.size)
        val reply = last.messages.first { it.id == "a2" }
        assertEquals(full, reply.text)
        assertEquals(false, reply.streaming)
        job.cancel()
    }

    @Test fun aResyncWithNoAnswerMakesTheSocketAgain() = runBlocking {
        val server = Server()
        val (t, c) = setUp(server)
        var last = ChatState(id)
        val job = launch(Dispatchers.Default) { c.chat(id).collect { last = it } }
        stuckMidWord(t, server) { last }
        // No network callback (or it came too early): the resync's own check finds the socket dead.
        assertTrue(c.socket.resync("ui:$id", answerMs = 300) { it.type() == "ui-history" })
        withTimeout(5_000) { while (last.state != AgentState.Idle) delay(10) }
        assertEquals(2, t.sockets.size)
        assertEquals(full, last.messages.first { it.id == "a2" }.text)
        job.cancel()
    }

    @Test fun anAnsweredResyncKeepsTheSocket() = runBlocking {
        val (t, c) = setUp(Server())
        var last = ChatState(id)
        val job = launch(Dispatchers.Default) { c.chat(id).collect { last = it } }
        withTimeout(5_000) { while (!last.loaded) delay(10) }
        assertTrue(c.socket.resync("ui:$id", answerMs = 300) { it.type() == "ui-history" })
        delay(600)
        assertEquals(1, t.sockets.size)
        // The app back in front only retries a connection that is waiting: an open one stays.
        c.reconnect()
        delay(200)
        assertEquals(1, t.sockets.size)
        job.cancel()
    }

    @Test fun aLateCloseOfTheOldConnectionLeavesTheNewOneAlone() = runBlocking {
        val server = Server()
        val (t, c) = setUp(server)
        var last = ChatState(id)
        val job = launch(Dispatchers.Default) { c.chat(id).collect { last = it } }
        withTimeout(5_000) { while (!last.loaded) delay(10) }
        c.networkChanged()
        withTimeout(5_000) { while (t.sockets.size < 2) delay(10) }
        delay(100)
        // The old connection's close lands now, a minute late on a dead network.
        t.sockets[0].onClosed(0, "connection lost")
        delay(100)
        // The new one is still the live one: what is sent goes out, a resync is answered.
        val before = t.socketSent.count { it.contains("\"subscribe-ui\"") }
        server.ended = true
        assertTrue(c.resyncChat(id))
        withTimeout(5_000) { while (last.state != AgentState.Idle) delay(10) }
        assertEquals(before + 1, t.socketSent.count { it.contains("\"subscribe-ui\"") })
        assertEquals(2, t.sockets.size)
        job.cancel()
    }

    @Test fun networkChangeIsTheNetworkChanging() {
        var n = 0
        val w = NetworkChange { n++ }
        w.seen("wifi")          // registering reports the one in use
        assertEquals(0, n)
        w.seen("wifi")          // the same one again (its details changed)
        assertEquals(0, n)
        w.lost("wifi"); w.seen("mobile")
        assertEquals(1, n)
        w.seen("wifi")          // straight back, no loss reported between
        assertEquals(2, n)
        w.lost("mobile")        // not the one in use: nothing
        w.lost("wifi"); w.seen("wifi")   // gone and back: its connections are dead too
        assertEquals(3, n)
    }

    /** The watchdog: a reply streaming in that stops for a while is caught up, even when the server's list says nothing. */
    @Test fun watchdogCatchesAStalledReply() {
        val w = ChatWatchdog()
        assertEquals(false, w.check(AgentState.Working, AgentState.Working, now = ChatWatchdog.STALL_MS - 1, lastMessageAt = 0, streaming = true))
        assertEquals(true, w.check(AgentState.Working, AgentState.Working, now = ChatWatchdog.STALL_MS, lastMessageAt = 0, streaming = true))
        // Nothing came of it: again, but not straight away.
        assertEquals(false, w.check(AgentState.Working, AgentState.Working, now = 2 * ChatWatchdog.STALL_MS, lastMessageAt = 0, streaming = true))
        assertEquals(true, w.check(AgentState.Working, AgentState.Working, now = ChatWatchdog.STALL_MS + ChatWatchdog.RETRY_MS, lastMessageAt = 0, streaming = true))
        // Not streaming (a long tool run is quiet): only the long quiet sign.
        val w2 = ChatWatchdog()
        assertEquals(false, w2.check(AgentState.Working, AgentState.Working, now = ChatWatchdog.STALL_MS * 2, lastMessageAt = 0, streaming = false))
    }
}
