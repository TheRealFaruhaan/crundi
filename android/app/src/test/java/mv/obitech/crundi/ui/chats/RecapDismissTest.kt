package mv.obitech.crundi.ui.chats

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.performSemanticsAction
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
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/**
 * Closing the recap in a real chat screen. Its X did nothing: the action
 * looked for the recap in the screen's own copy of the chat, which is never
 * filled (the live chat is drawn from the stream), found none and returned.
 * The pure functions all passed; only the screen showed it.
 */
@RunWith(AndroidJUnit4::class)
@Config(sdk = [35])
class RecapDismissTest {
    @get:Rule val compose = createComposeRule()

    private val chat = "c5d6e7f8a9b0c1d2"
    private val bubble = object : Host { override val isBubble = true }

    private fun open(t: FakeTransport) {
        val snapshot = FakeApi.text("chat-summaries.jsonl").lines().first { it.isNotBlank() }
        t.onSocketSend = { sent -> if (sent.contains("\"subscribe-ui\"")) listOf(snapshot) else null }
        t.on("POST /api/ui-sessions/$chat/dismiss-recap", """{"ok":true}""")
        val services = object : AppServices {
            override val shell: ShellSource = StaticShellSource(FakeServices.shellData.copy(capabilities = setOf("chat-recap", "chat-aside")))
            override val api: CrundiClient = CrundiClient(t)
        }
        compose.mainClock.autoAdvance = false
        compose.setContent { CrundiTheme { CompositionLocalProvider(LocalAppServices provides services, LocalHost provides bubble) { BubbleChat(chat) } } }
    }

    private fun shows(text: String) = compose.onAllNodes(hasText(text, substring = true)).fetchSemanticsNodes().isNotEmpty()
    private fun settle() { repeat(20) { compose.mainClock.advanceTimeByFrame(); compose.waitForIdle() } }
    private val closeRecap = SemanticsMatcher("clicks to close the recap") { it.config.getOrNull(SemanticsActions.OnClick)?.label == "Close the recap" }

    @Test fun theXClosesTheRecapAndTellsTheServer() {
        val t = FakeApi.transport()
        open(t)
        settle()
        assertTrue("the recap is shown", shows("Tidying the parser; the tests are next."))
        compose.onNode(closeRecap).performSemanticsAction(SemanticsActions.OnClick)
        settle()
        assertTrue("it is gone at once", !shows("Tidying the parser; the tests are next."))
        assertTrue("and the server is told", t.requests.any { it.method == "POST" && it.path.endsWith("/api/ui-sessions/$chat/dismiss-recap") })
    }
}
