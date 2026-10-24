package mv.obitech.crundi.ui.chats

import kotlinx.coroutines.test.runTest
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.model.AgentState
import mv.obitech.crundi.api.model.Pane
import mv.obitech.crundi.api.model.SendResult
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * A chat put to sleep, seen from its bubble or chat screen (ChatSleep): how
 * the screen knows, the order of waking and sending, the id it comes back
 * under, and what the person is told when it does not wake.
 */
class ChatSleepTest {
    private val id = "32ea1d4e407af03b"
    private val parkedChat = Pane(id, project = "crundi", kind = "parked", paneKind = "ui", status = "parked", reason = "idle", hasSession = true)
    private val liveChat = Pane(id, project = "crundi", kind = "ui", status = "running", agentState = "idle")

    // The state broadcast's rows are what the workbench goes by too (PaneView.ofPane).
    @Test fun asleepFromTheLiveStateRow() {
        assertTrue(ChatSleep.asleep(parkedChat))
        assertFalse(ChatSleep.asleep(liveChat))
        assertFalse(ChatSleep.asleep(null))
        assertFalse("a parked terminal is not a sleeping chat", ChatSleep.asleep(parkedChat.copy(paneKind = "terminal")))
        assertTrue(ChatSleep.asleep(parkedChat.copy(paneKind = "")))
        assertTrue(ChatUi(mv.obitech.crundi.api.model.ChatState(id), pane = parkedChat).asleep)
        assertFalse(ChatUi(mv.obitech.crundi.api.model.ChatState(id), pane = liveChat).asleep)
    }

    @Test fun upMeansListedAsARunningChat() {
        assertTrue(ChatSleep.up(listOf(liveChat), id))
        assertFalse(ChatSleep.up(listOf(parkedChat), id))
        assertFalse(ChatSleep.up(listOf(liveChat.copy(status = "exited")), id))
        assertFalse(ChatSleep.up(listOf(liveChat), "another"))
        assertFalse(ChatSleep.up(null, id))
    }

    @Test fun footerAndHeaderSayAsleepNotIdle() {
        assertEquals("asleep", stateLine(AgentState.Idle, exited = false, asleep = true))
        assertEquals("asleep", stateLine(AgentState.Idle, exited = true, asleep = true))
        assertEquals("waking…", stateLine(AgentState.Idle, exited = false, asleep = true, waking = true))
        assertEquals("idle", stateLine(AgentState.Idle, exited = false))
    }

    @Test fun theMenuOffersNoPutAwayOrRenameWhileAsleep() {
        val asleep = chatMenu(parkedChat, inBubble = true, rename = {}, bubble = {}, park = {}, close = {}, openInMain = {}).map { it.label }
        assertFalse("Put away" in asleep)
        assertFalse("Rename" in asleep)
        assertTrue("Close chat" in asleep)
        val awake = chatMenu(liveChat, inBubble = true, rename = {}, bubble = {}, park = {}, close = {}, openInMain = {}).map { it.label }
        assertTrue("Put away" in awake && "Rename" in awake)
    }

    @Test fun noSessionIsRecognisedAndNeverShownRaw() {
        assertTrue(ChatSleep.noSession(ApiException(200, "No session \"$id\"")))
        assertTrue(ChatSleep.noSession(ApiException(200, "Session is not running")))
        assertFalse(ChatSleep.noSession(ApiException(0, "No session")))
        assertFalse(ChatSleep.noSession(ApiException(200, "Message is empty")))
        assertFalse(ChatSleep.noSession(IllegalStateException("No session")))
        assertEquals("This chat is asleep. Resume it first.", ChatSleep.idleError(parkedChat))
        assertEquals(ChatSleep.GONE, ChatSleep.idleError(null))
        assertFalse(ChatSleep.GONE.contains("No session"))
    }

    @Test fun wakeErrorsArePlain() {
        assertEquals(
            "This chat is asleep and could not be woken: project \"crundi\" no longer exists.",
            ChatSleep.wakeError(ApiException(200, "Project \"crundi\" no longer exists")),
        )
        // An older server, with no /api/panes routes.
        val old = ChatSleep.wakeError(ApiException(404, "Not found"))
        assertTrue(old.startsWith(ChatSleep.WAKE_FAILED))
        assertTrue(old.contains("this server cannot wake chats from the app"))
        assertTrue(ChatSleep.wakeError(ApiException(0, "timeout")).startsWith(ChatSleep.WAKE_FAILED + "the server cannot be reached"))
    }

    /** A server that writes down what it was asked, in order. */
    private class FakeServer(
        val resumeTo: String = "32ea1d4e407af03b",
        val resumeFails: Exception? = null,
        val upAfterNotParked: Boolean = false,
        val up: Boolean = true,
        val sendFails: List<Exception> = emptyList(),
    ) : ChatSleep.Server {
        val calls = mutableListOf<String>()
        var sends = 0
        override suspend fun resume(id: String, mode: String): String {
            calls += "resume $id $mode"
            resumeFails?.let { throw it }
            return resumeTo
        }
        override suspend fun awaitUp(id: String, timeoutMs: Long): Boolean {
            calls += "up $id"
            return if (resumeFails != null) upAfterNotParked else up
        }
        override suspend fun send(id: String, text: String): SendResult {
            calls += "send $id $text"
            sendFails.getOrNull(sends++)?.let { throw it }
            return SendResult()
        }
    }

    @Test fun resumesThenWaitsThenSends() = runTest {
        val s = FakeServer()
        val r = ChatSleep.wake(s, id, "carry on", retryMs = 0)
        assertEquals(listOf("resume $id resume", "up $id", "send $id carry on"), s.calls)
        assertTrue(r is ChatSleep.Outcome.Sent)
        assertEquals(id, r.id)
    }

    @Test fun aNewIdIsFollowedForTheSendAndAfter() = runTest {
        val s = FakeServer(resumeTo = "aaaaaaaaaaaaaaaa")
        val r = ChatSleep.wake(s, id, "carry on", retryMs = 0)
        assertEquals(listOf("resume $id resume", "up aaaaaaaaaaaaaaaa", "send aaaaaaaaaaaaaaaa carry on"), s.calls)
        assertEquals("aaaaaaaaaaaaaaaa", r.id)
    }

    @Test fun anEmptyIdFromTheServerKeepsTheOldOne() = runTest {
        val r = ChatSleep.wake(FakeServer(resumeTo = ""), id, null)
        assertEquals(ChatSleep.Outcome.Woken(id), r)
    }

    @Test fun nothingIsSentWhenWakingFails() = runTest {
        val s = FakeServer(resumeFails = ApiException(200, "Project \"crundi\" no longer exists"))
        val r = ChatSleep.wake(s, id, "carry on")
        assertEquals(listOf("resume $id resume"), s.calls)
        assertEquals(ChatSleep.Outcome.Failed(id, "This chat is asleep and could not be woken: project \"crundi\" no longer exists."), r)
    }

    @Test fun nothingIsSentWhenItDoesNotComeUp() = runTest {
        val s = FakeServer(up = false)
        val r = ChatSleep.wake(s, id, "carry on")
        assertEquals(listOf("resume $id resume", "up $id"), s.calls)
        assertEquals(ChatSleep.Outcome.Failed(id, ChatSleep.NOT_UP), r)
    }

    @Test fun aClaudeStillStartingIsTriedAgain() = runTest {
        val s = FakeServer(sendFails = listOf(ApiException(200, "Session is not running"), ApiException(200, "No session \"$id\"")))
        val r = ChatSleep.wake(s, id, "carry on", retryMs = 0)
        assertEquals(3, s.sends)
        assertTrue(r is ChatSleep.Outcome.Sent)
    }

    @Test fun aClaudeThatNeverTakesItFailsPlainly() = runTest {
        val s = FakeServer(sendFails = List(4) { ApiException(200, "No session \"$id\"") })
        val r = ChatSleep.wake(s, id, "carry on", retryMs = 0)
        assertEquals(4, s.sends)
        assertEquals(ChatSleep.Outcome.Failed(id, ChatSleep.NOT_UP), r)
        assertFalse((r as ChatSleep.Outcome.Failed).message.contains("No session"))
    }

    @Test fun anotherSendFailureIsWordedAsOne() = runTest {
        val s = FakeServer(sendFails = listOf(ApiException(200, "Token limit reached")))
        val r = ChatSleep.wake(s, id, "carry on", retryMs = 0)
        assertEquals(1, s.sends)
        assertEquals(ChatSleep.Outcome.Failed(id, "The message did not go: Token limit reached"), r)
    }

    @Test fun wokenElsewhereMeanwhileStillSends() = runTest {
        val s = FakeServer(resumeFails = ApiException(200, "This pane is not parked"), upAfterNotParked = true)
        val r = ChatSleep.wake(s, id, "carry on", retryMs = 0)
        assertTrue(r is ChatSleep.Outcome.Sent)
        assertEquals("send $id carry on", s.calls.last())
    }

    // A send met "No session" with nothing saying the chat was asleep: closed, not sleeping.
    @Test fun aChatThatIsGoneSaysSo() = runTest {
        val s = FakeServer(resumeFails = ApiException(200, "This pane is not parked"))
        assertEquals(ChatSleep.Outcome.Failed(id, ChatSleep.GONE), ChatSleep.wake(s, id, "carry on", known = false))
        // An older server has no /api/panes at all: the same plain words, no raw error.
        val old = FakeServer(resumeFails = ApiException(404, "Not found"))
        assertEquals(ChatSleep.Outcome.Failed(id, ChatSleep.GONE), ChatSleep.wake(old, id, "carry on", known = false))
    }

    @Test fun resumeButtonsPassTheirMode() = runTest {
        val s = FakeServer()
        assertEquals(ChatSleep.Outcome.Woken(id), ChatSleep.wake(s, id, null, mode = "new"))
        assertEquals(listOf("resume $id new", "up $id"), s.calls)
    }
}
