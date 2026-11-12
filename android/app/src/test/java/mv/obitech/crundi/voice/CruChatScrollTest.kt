package mv.obitech.crundi.voice

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.hasScrollToIndexAction
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToIndex
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.unit.dp
import androidx.test.ext.junit.runners.AndroidJUnit4
import mv.obitech.crundi.api.VoiceApi
import mv.obitech.crundi.ui.theme.CrundiTheme
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/**
 * Cru's chat view keeps the newest message in view: the keyboard opening
 * moves the conversation up with it, and a typed send goes to the new
 * message wherever the owner had scrolled to.
 */
@RunWith(AndroidJUnit4::class)
@Config(sdk = [35])
class CruChatScrollTest {
    @get:Rule val compose = createComposeRule()

    private val room = mutableStateOf(460)
    private val ui = mutableStateOf(
        VoiceUi(
            state = VoiceState(phase = Phase.Listening), caps = setOf(VoiceApi.HISTORY), chat = true, keyboard = true, historyLoaded = true,
            history = (1..40).map { i -> CruMsg(at = 1000L * i, said = "Question $i, asked of Cru;", speak = "Answer $i, long enough to wrap across a line or two.", show = "", key = "k$i") },
        ),
    )

    private fun open() {
        compose.mainClock.autoAdvance = false
        compose.setContent {
            CrundiTheme {
                Box(Modifier.height(room.value.dp)) {
                    VoiceScreen(
                        ui.value, mic = { 0f }, voice = { 0f }, onMic = {}, onClose = {},
                        more = VoiceMore(onSend = { t ->
                            val u = ui.value
                            ui.value = u.copy(history = u.history + CruMsg(at = 99_000, said = t, speak = "", show = "", live = true, key = "sent"))
                        }),
                    )
                }
            }
        }
    }

    private fun settle() { repeat(40) { compose.mainClock.advanceTimeByFrame(); compose.waitForIdle() } }

    /** Wholly on screen inside the chat list, not only in the tree. */
    private fun onScreen(text: String): Boolean {
        val log = compose.onAllNodes(hasScrollToIndexAction()).fetchSemanticsNodes().first().boundsInRoot
        return compose.onAllNodes(hasText(text, substring = true)).fetchSemanticsNodes()
            .any { val b = it.boundsInRoot; b.bottom > log.top && b.bottom <= log.bottom + 1f }
    }

    @Test fun theKeyboardKeepsTheNewestInView() {
        open()
        settle()
        assertTrue("at the newest", onScreen("Answer 40,"))
        room.value = 300
        settle()
        assertTrue("still the newest with the keyboard open", onScreen("Answer 40,"))
    }

    @Test fun readingUpTheKeyboardMovesTheChatUpWithIt() {
        open()
        settle()
        compose.onAllNodes(hasScrollToIndexAction()).onFirst().performScrollToIndex(15)
        settle()
        val bottom = (1..40).last { onScreen("Answer $it,") }
        assertTrue(bottom < 40)
        room.value = 320
        settle()
        assertTrue("the message at the bottom stays in view", onScreen("Answer $bottom,"))
        assertTrue("not taken to the end", !onScreen("Answer 40,"))
    }

    @Test fun aSendGoesToTheNewMessage() {
        open()
        settle()
        compose.onAllNodes(hasScrollToIndexAction()).onFirst().performScrollToIndex(5)
        settle()
        assertTrue(!onScreen("Answer 40,"))
        compose.onNode(hasSetTextAction()).performTextInput("what is next")
        settle()
        compose.onNode(hasContentDescription("Send")).performClick()
        repeat(120) { compose.mainClock.advanceTimeByFrame(); compose.waitForIdle() }
        assertTrue("the sent message is in view", onScreen("what is next"))
    }
}
