package mv.obitech.crundi.ui.chats

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasScrollToIndexAction
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToIndex
import androidx.test.ext.junit.runners.AndroidJUnit4
import mv.obitech.crundi.api.CrundiClient
import mv.obitech.crundi.api.FakeTransport
import mv.obitech.crundi.screens.FakeApi
import mv.obitech.crundi.screens.FakeServices
import mv.obitech.crundi.ui.shell.AppServices
import mv.obitech.crundi.ui.shell.Host
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.shell.LocalHost
import mv.obitech.crundi.ui.shell.ShellSource
import mv.obitech.crundi.ui.shell.StaticShellSource
import mv.obitech.crundi.ui.theme.CrundiTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/**
 * The arrow back to the newest message (ChatJump.kt) in a real chat screen:
 * hidden at the bottom, shown well up the log, gone again once tapped, and
 * marked when something comes in below.
 */
@RunWith(AndroidJUnit4::class)
@Config(sdk = [35])
class ChatJumpTest {
    @get:Rule val compose = createComposeRule()

    private val chat = "d1e2f3a4b5c6d7e8"
    private val bubble = object : Host { override val isBubble = true }

    private fun history(n: Int): String {
        val msgs = (1..n).joinToString(",") { i ->
            val kind = if (i % 2 == 1) "user" else "assistant-text"
            """{"id":"m$i","kind":"$kind","text":"Message number $i, long enough to take a line or two on a phone screen so the log scrolls.","seq":$i}"""
        }
        return """{"type":"ui-history","id":"$chat","session":{"id":"$chat","project":"crundi","title":"Long chat","status":"running","state":"idle","messages":[$msgs],"agents":[]}}"""
    }

    private fun open(t: FakeTransport) {
        t.onSocketSend = { sent -> if (sent.contains("\"subscribe-ui\"")) listOf(history(60)) else null }
        val services = object : AppServices {
            override val shell: ShellSource = StaticShellSource(FakeServices.shellData)
            override val api: CrundiClient = CrundiClient(t)
        }
        compose.mainClock.autoAdvance = false
        compose.setContent { CrundiTheme { CompositionLocalProvider(LocalAppServices provides services, LocalHost provides bubble) { BubbleChat(chat) } } }
    }

    private fun settle() { repeat(30) { compose.mainClock.advanceTimeByFrame(); compose.waitForIdle() } }
    private fun jumpShown() = compose.onAllNodes(hasContentDescription("Go to the newest message")).fetchSemanticsNodes().isNotEmpty()

    @Test fun shownUpTheLogAndGoneOnceTapped() {
        val t = FakeApi.transport()
        open(t)
        settle()
        assertTrue("not at the bottom", !jumpShown())
        compose.onAllNodes(hasScrollToIndexAction()).onFirst().performScrollToIndex(0)
        settle()
        assertTrue("well up the log, it shows", jumpShown())
        compose.onNode(hasContentDescription("Go to the newest message")).performClick()
        repeat(120) { compose.mainClock.advanceTimeByFrame(); compose.waitForIdle() }
        assertTrue("back at the bottom, it goes", !jumpShown())
    }

    @Test fun howFarAndWhen() {
        // 240dp at a density of 2 is 480px; a third of a 3000px log is more.
        assertEquals(480, ChatJump.threshold(900, 2f))
        assertEquals(1000, ChatJump.threshold(3000, 2f))
    }

    private fun delta(id: String, text: String) =
        """{"type":"ui-event","id":"$chat","event":{"type":"delta","id":"$id","text":"$text"}}"""
    private fun entry(i: Int) =
        """{"type":"ui-event","id":"$chat","event":{"type":"entry","entry":{"id":"m$i","kind":"assistant-text","text":"Message number $i arrives.","seq":$i}}}"""
    private fun shown(text: String) = compose.onAllNodes(androidx.compose.ui.test.hasText(text, substring = true)).fetchSemanticsNodes().isNotEmpty()

    @Test fun scrolledUpStaysPutWhileTheReplyStreams() {
        val t = FakeApi.transport()
        open(t)
        settle()
        compose.onAllNodes(hasScrollToIndexAction()).onFirst().performScrollToIndex(30)
        settle()
        assertTrue("reading up the log", shown("Message number 31,"))
        repeat(10) { t.pushSocket(delta("m60", " and more of the reply streaming in, line after line after line")); settle() }
        t.pushSocket(entry(61)); settle()
        assertTrue("not dragged back down", shown("Message number 31,"))
        assertTrue("the arrow is there", jumpShown())
        compose.onNode(hasContentDescription("Go to the newest message")).performClick()
        repeat(120) { compose.mainClock.advanceTimeByFrame(); compose.waitForIdle() }
        assertTrue("at the newest", shown("Message number 61 arrives."))
        // Following again: what comes next stays in view.
        t.pushSocket(entry(62)); settle()
        assertTrue("follows once back", shown("Message number 62 arrives."))
        assertTrue(!jumpShown())
    }

    @Test fun atTheBottomTheStreamStaysInView() {
        val t = FakeApi.transport()
        open(t)
        settle()
        repeat(6) { t.pushSocket(delta("m60", " streaming more words onto the last reply so it grows taller")); settle() }
        t.pushSocket(entry(61)); settle()
        assertTrue("the newest is in view", shown("Message number 61 arrives."))
        assertTrue(!jumpShown())
    }

    @Test fun followRules() {
        val f = ChatFollow()
        assertTrue(f.shouldFollow)
        // The reader scrolls up: following stops.
        f.moved(1 to 0, atEnd = false)
        assertTrue(!f.following)
        // Content growing does not move the position, and nothing restarts it but the reader.
        f.moved(1 to 0, atEnd = false)
        assertTrue(!f.following)
        // Back at the bottom by scrolling: following again.
        f.moved(5 to 0, atEnd = true)
        assertTrue(f.following)
        // A finger on the log: never follow under it; dragging up turns it off at once.
        f.dragging = true
        assertTrue(!f.shouldFollow)
        f.moved(4 to 0, atEnd = false)
        f.released(atEnd = false)
        assertTrue(!f.following)
        // The arrow, or a send.
        f.jumped()
        assertTrue(f.shouldFollow)
        // Let go at the bottom: following.
        f.moved(2 to 0, atEnd = false)
        f.dragging = true
        f.released(atEnd = true)
        assertTrue(f.following)
    }

    @Test fun ownScrollsAreNotTheReaders() = kotlinx.coroutines.runBlocking {
        val f = ChatFollow()
        // Mid-scroll positions of the log's own scroll never stop following.
        f.auto({ 9 to 0 }) { f.moved(3 to 0, atEnd = false) }
        assertTrue(f.following)
        // Hearing late of where it ended does not either.
        f.moved(9 to 0, atEnd = false)
        assertTrue(f.following)
        // A finger counts even mid-scroll.
        f.dragging = true
        f.auto({ 9 to 0 }) { f.moved(8 to 0, atEnd = false) }
        assertTrue(!f.following)
    }
}
