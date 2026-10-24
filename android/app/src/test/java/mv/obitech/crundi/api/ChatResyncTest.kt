package mv.obitech.crundi.api

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.withTimeoutOrNull
import mv.obitech.crundi.api.model.AgentState
import mv.obitech.crundi.api.model.ChatState
import mv.obitech.crundi.screens.FakeApi
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The bubble that stopped mid-sentence and stayed "working…" (card
 * 0983dd82). A bubble folded away is frozen by Android; when it opens again
 * everything the server sent meanwhile arrives at once, faster than the chat
 * screen can fold it. The socket's flow kept 64 messages and dropped the
 * rest without a word, so the end of the reply and the turn's "idle" were
 * lost, and nothing ever asked for them again.
 */
class ChatResyncTest {
    private val id = "a1b2c3d4e5f60718"

    private fun delta(n: Int) = """{"type":"ui-event","id":"$id","event":{"type":"delta","id":"a2","text":" w$n"}}"""
    private val idle = """{"type":"ui-event","id":"$id","event":{"type":"state","state":"idle"}}"""
    private val working = """{"type":"ui-event","id":"$id","event":{"type":"state","state":"working"}}"""

    /** Reproduces it: a burst after a fold, read by a slow screen, ends where the server ended. */
    @Test fun aBurstAfterAFoldIsNotDropped() = runBlocking {
        val t = FakeApi.transport()
        val c = CrundiClient(t)
        var last = ChatState(id)
        // A screen slower than the burst, as the main thread is with a long chat.
        val job = launch(Dispatchers.Default) { c.chat(id).collect { last = it; Thread.sleep(2) } }
        withTimeout(5_000) { while (!last.loaded) delay(10) }
        t.pushSocket(working)
        t.pushSocket("""{"type":"ui-event","id":"$id","event":{"type":"entry","entry":{"id":"a2","kind":"assistant-text","text":"","streaming":true,"seq":21}}}""")
        // Folded: the server goes on; unfolded: all of it lands in one go.
        for (n in 1..400) t.pushSocket(delta(n))
        t.pushSocket(idle)
        withTimeout(10_000) { while (last.state != AgentState.Idle) delay(20) }
        assertTrue(last.messages.first { it.id == "a2" }.text.endsWith(" w400"))
        job.cancel()
    }

    /** On resume the chat asks for its history again (the same resync a reconnect does), and it replaces what was there. */
    @Test fun resyncAsksForTheHistoryAgain() = runBlocking {
        val t = FakeApi.transport()
        val c = CrundiClient(t)
        var last = ChatState(id)
        val job = launch(Dispatchers.Default) { c.chat(id).collect { last = it } }
        withTimeout(5_000) { while (!last.loaded) delay(10) }
        t.pushSocket(working)
        withTimeout(5_000) { while (last.state != AgentState.Working) delay(10) }
        val before = t.socketSent.count { it.contains("subscribe-ui") }
        // The fixture history ends idle: the turn ended while the bubble was away.
        assertTrue(c.resyncChat(id))
        withTimeout(5_000) { while (last.state != AgentState.Idle) delay(10) }
        assertEquals(before + 1, t.socketSent.count { it.contains("\"subscribe-ui\"") })
        // Replaced, not added to: the history once, no duplicates.
        assertEquals(FakeApi.chat(events = 0).messages.map { it.id }, last.messages.map { it.id })
        // A chat nobody holds asks for nothing.
        assertEquals(false, c.resyncChat("not-held"))
        job.cancel()
    }

    /** The watchdog: when the screen says working and the server says otherwise, or nothing has come for long. */
    @Test fun watchdog() {
        val w = ChatWatchdog()
        // Server agrees: nothing to do.
        assertEquals(false, w.check(AgentState.Working, AgentState.Working, now = 0, lastMessageAt = 0))
        // Server says idle: not at once (its push may just be ahead of the socket)...
        assertEquals(false, w.check(AgentState.Working, AgentState.Idle, now = 1_000, lastMessageAt = 0))
        assertEquals(false, w.check(AgentState.Working, AgentState.Idle, now = 1_000 + ChatWatchdog.DISAGREE_MS - 1, lastMessageAt = 0))
        // ...but once it has said so for a while.
        assertEquals(true, w.check(AgentState.Working, AgentState.Idle, now = 1_000 + ChatWatchdog.DISAGREE_MS, lastMessageAt = 0))
        // Then not again straight away.
        assertEquals(false, w.check(AgentState.Working, AgentState.Idle, now = 2_000 + ChatWatchdog.DISAGREE_MS, lastMessageAt = 0))
        // Not working on screen: nothing to catch up on.
        val w2 = ChatWatchdog()
        assertEquals(false, w2.check(AgentState.Idle, AgentState.Working, now = 0, lastMessageAt = 0))
        assertEquals(false, w2.check(AgentState.Idle, AgentState.Working, now = 60_000, lastMessageAt = 0))
        // A turn gone quiet with no word from the server (a factory employee is not in the list).
        val w3 = ChatWatchdog()
        assertEquals(false, w3.check(AgentState.Working, null, now = ChatWatchdog.QUIET_MS - 1, lastMessageAt = 0))
        assertEquals(true, w3.check(AgentState.Working, null, now = ChatWatchdog.QUIET_MS, lastMessageAt = 0))
        assertEquals(false, w3.check(AgentState.Working, null, now = ChatWatchdog.QUIET_MS + 1_000, lastMessageAt = 0))
        assertEquals(true, w3.check(AgentState.Working, null, now = 2 * ChatWatchdog.QUIET_MS, lastMessageAt = 0))
    }
}
