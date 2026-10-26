package mv.obitech.crundi.ui

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import mv.obitech.crundi.api.CrundiClient
import mv.obitech.crundi.api.FakeTransport
import mv.obitech.crundi.api.model.SecretRequest
import mv.obitech.crundi.screens.FakeApi
import mv.obitech.crundi.ui.chats.BubbleChat
import mv.obitech.crundi.ui.operations.SecretPrompts
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
 * Claude asking for a secret from a chat open in a bubble. The bubble
 * (ChatActivity) had no secret prompt at all: the main window draws it from
 * its shell, which a bubble does not have, so the request sat unanswerable
 * there and timed out or was denied elsewhere. Both ways it arrives: already
 * waiting when the bubble opens (from the request's own notification), and
 * coming in while the bubble is open.
 */
@RunWith(AndroidJUnit4::class)
@Config(sdk = [35])
class BubbleSecretPromptTest {
    @get:Rule val compose = createComposeRule()

    private val chat = "a1b2c3d4e5f60718"
    private val bubble = object : Host { override val isBubble = true }

    private fun reqJson(id: String, session: String, name: String) =
        """{"id":"$id","secretId":"s-$id","secretName":"$name","project":"crundi","reason":"","kind":"run","command":"./deploy.sh","envName":"TOKEN","sessionId":"$session"}"""

    private fun open(t: FakeTransport) {
        val services = object : AppServices {
            override val shell: ShellSource = StaticShellSource(mv.obitech.crundi.screens.FakeServices.shellData)
            override val api: CrundiClient = CrundiClient(t)
        }
        compose.mainClock.autoAdvance = false
        compose.setContent { CrundiTheme { CompositionLocalProvider(LocalAppServices provides services, LocalHost provides bubble) { BubbleChat(chat) } } }
    }

    private fun shows(text: String) = compose.onAllNodes(hasText(text, substring = true)).fetchSemanticsNodes().isNotEmpty()
    private fun settle() { repeat(20) { compose.mainClock.advanceTimeByFrame(); compose.waitForIdle() } }

    @Test fun aRequestAlreadyWaitingShowsWhenTheBubbleOpens() {
        val t = FakeApi.transport()
        t.on("GET /api/secrets", """{"ok":true,"secrets":[],"requests":[${reqJson("r1", chat, "DEPLOY_TOKEN")},${reqJson("r2", "another-chat", "OTHER_TOKEN")}]}""")
        open(t)
        settle()
        assertTrue("the waiting request is shown", shows("DEPLOY_TOKEN"))
        assertTrue("with Approve", shows("Approve"))
        assertTrue("another chat's request is not", !shows("OTHER_TOKEN"))
    }

    @Test fun aRequestArrivingWhileOpenShows() {
        val t = FakeApi.transport()
        t.on("GET /api/secrets", """{"ok":true,"secrets":[],"requests":[]}""")
        open(t)
        settle()
        assertTrue("nothing yet", !shows("LIVE_TOKEN"))
        t.pushEvent("secret-requests", """{"requests":[${reqJson("r3", chat, "LIVE_TOKEN")}]}""")
        settle()
        assertTrue("the request that came in is shown", shows("LIVE_TOKEN"))
    }

    @Test fun whichRequestsABubbleShows() {
        val mine = SecretRequest("a", sessionId = chat, project = "crundi")
        val other = SecretRequest("b", sessionId = "zzz", project = "crundi")
        val noChatSameProject = SecretRequest("c", project = "crundi")
        val noChatOtherProject = SecretRequest("d", project = "ledger")
        assertEquals(listOf("a", "c"), SecretPrompts.forChat(listOf(mine, other, noChatSameProject, noChatOtherProject), chat, "crundi").map { it.id })
    }

    @Test fun theLiveListAndTheFetchedOneTogether() {
        val a = SecretRequest("a"); val b = SecretRequest("b")
        assertEquals(listOf("a"), SecretPrompts.merge(listOf(a), null).map { it.id })
        assertEquals(listOf("b"), SecretPrompts.merge(emptyList(), listOf(b)).map { it.id })
        assertEquals(listOf("a", "b"), SecretPrompts.merge(listOf(a), listOf(a, b)).map { it.id })
    }
}
