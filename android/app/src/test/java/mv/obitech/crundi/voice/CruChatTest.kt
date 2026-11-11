package mv.obitech.crundi.voice

import mv.obitech.crundi.api.VoiceTurn
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The voice screen's chat view (CruChat), keyboard or voice (CruMode), following the reply (SpeechFollow), the mark (CruMark). */
class CruChatTest {
    private val turns = listOf(
        VoiceTurn(1, 1000, "usage?", "Plenty.", "5h 2%"),
        VoiceTurn(2, 2000, "[Report] build", "The build finished.", "", report = true),
        VoiceTurn(3, 3000, "", "", ""),
    )

    @Test fun turnsBecomeMessagesReportsAreCrusAlone() {
        val m = CruChat.fromTurns(turns)
        assertEquals(2, m.size)
        assertEquals("usage?", m[0].said)
        assertEquals("5h 2%", m[0].show)
        assertEquals("", m[1].said)
        assertEquals("The build finished.", m[1].speak)
        assertFalse(m[0].live)
        assertTrue(m[0].key != m[1].key)
    }

    @Test fun linesSaidDuringATurnAreMessagesOfTheirOwn() {
        val m = CruChat.fromTurns(listOf(VoiceTurn(7, 7000, "show me how", "That's all.", "", updates = listOf("Open the Kanban tab.", "Now click Add card."))))
        assertEquals(listOf("show me how", "", "", ""), m.map { it.said })
        assertEquals(listOf("", "Open the Kanban tab.", "Now click Add card.", "That's all."), m.map { it.speak })
        // Shown live here, then loaded: not twice.
        val live = listOf(CruMsg(7000, "show me how", "", "", live = true, key = "q1"), CruMsg(7001, "", "Open the Kanban tab.", "", live = true), CruMsg(7002, "", "That's all.", "", live = true))
        assertEquals(4, CruChat.merge(live, m, older = false).size)
    }

    @Test fun olderPagesGoFirstAFreshLoadKeepsWhatHappenedHere() {
        val loaded = CruChat.fromTurns(turns.take(1))
        val older = CruChat.fromTurns(listOf(VoiceTurn(0, 500, "hello", "Sir.", "")))
        assertEquals(listOf("hello", "usage?"), CruChat.merge(loaded, older, older = true).map { it.said })
        // Asked here before the history came: kept unless the server already has it.
        val here = CruChat.add(emptyList(), "usage?", "Plenty.", "5h 2%", 1001)
        val here2 = CruChat.add(here, "open kanban", "Done.", "", 4000)
        val fresh = CruChat.merge(here2, loaded, older = false)
        assertEquals(listOf("usage?", "open kanban"), fresh.map { it.said })
    }

    @Test fun anEmptyExchangeIsNotAdded() {
        assertTrue(CruChat.add(emptyList(), " ", "", "", 1).isEmpty())
        assertEquals(1, CruChat.add(emptyList(), "", "News, Sir.", "", 1).size)
    }

    @Test fun theWordsBeingHeardShowWhileListeningOrThinking() {
        assertEquals("open the", CruChat.hearing(VoiceState(phase = Phase.Listening, heard = "open the ")))
        assertEquals("open it", CruChat.hearing(VoiceState(phase = Phase.Thinking, heard = "open it")))
        assertEquals("", CruChat.hearing(VoiceState(phase = Phase.Speaking, heard = "open it")))
        assertEquals("", CruChat.hearing(VoiceState(phase = Phase.Listening, heard = "")))
    }

    @Test fun whatIsSentShowsAtOnceAndWhatIsQueuedShowsAsQueued() {
        // 10 Nov: a typed "Yes" never appeared while its turn ran.
        var list = CruChat.asked(emptyList(), "Yes", 1L, "q1")
        assertEquals(listOf("Yes"), list.map { it.said })
        assertEquals(list, CruChat.asked(list, "Yes", 2L, "q1"))                     // the same send once
        list = CruChat.answered(list, "q1", "Yes", "Done, Sir.", "", 3L)
        assertEquals("Done, Sir.", list.single().speak)                             // the answer fills it in
        assertEquals(2, CruChat.answered(list, "gone", "Hi", "Hello", "", 4L).size)  // not there: added
        assertTrue(CruChat.drop(CruChat.asked(emptyList(), "Oops", 1L, "q2"), "q2").isEmpty())  // cancelled: out
        // Said or typed while Cru thinks: queued, shown as such; what was asked is not repeated as heard.
        val thinking = VoiceState(phase = Phase.Thinking, asked = "Yes", heard = "Yes", pending = "")
        assertEquals("", CruChat.hearing(thinking))
        val queued = thinking.copy(pending = "and the logs", heard = "and the logs")
        assertEquals("and the logs", CruChat.queued(queued))
        assertEquals("", CruChat.hearing(queued))
        assertEquals("please", CruChat.hearing(queued.copy(heard = "and the logs please")))   // still coming in
        assertEquals("", CruChat.queued(queued.copy(phase = Phase.Listening)))
        // A fresh load: the sent message goes once the server's newest turns have it, answered.
        val got = CruChat.fromTurns(listOf(VoiceTurn(id = 1, at = 5, said = "Yes", speak = "Done, Sir.")))
        assertEquals(1, CruChat.merge(CruChat.asked(emptyList(), "Yes", 4L, "q3"), got, older = false).size)
    }

    @Test fun pagingBackStartsFromTheOldest() {
        assertEquals(0L, CruChat.before(emptyList()))
        assertEquals(1000L, CruChat.before(CruChat.fromTurns(turns)))
    }

    @Test fun keyboardWhenVoiceCannotWorkElseTheLastChoice() {
        // No microphone permission, no Deepgram key, or past its limit with no recogniser on the phone.
        assertTrue(CruMode.voiceUnavailable(micGranted = false, deepgramKeySet = true, deepgramLimitReached = false, deviceRecogniser = true))
        assertTrue(CruMode.voiceUnavailable(true, false, false, true))
        assertTrue(CruMode.voiceUnavailable(true, true, true, false))
        assertFalse(CruMode.voiceUnavailable(true, true, true, true))
        assertFalse(CruMode.voiceUnavailable(true, null, false, false))
        assertFalse(CruMode.voiceUnavailable(true, true, false, false))
        assertTrue(CruMode.startInKeyboard(saved = false, voiceUnavailable = true))
        assertTrue(CruMode.startInKeyboard(saved = true, voiceUnavailable = false))
        assertFalse(CruMode.startInKeyboard(saved = false, voiceUnavailable = false))
        assertFalse(CruMode.startInKeyboard(saved = null, voiceUnavailable = false))
    }

    @Test fun theReplyIsFollowedOnlyWhenTallerThanItsArea() {
        assertEquals(1500L, SpeechFollow.durationMs("Yes.", 1f))
        assertEquals(10_000L, SpeechFollow.durationMs("x".repeat(140), 1f))
        assertEquals(5_000L, SpeechFollow.durationMs("x".repeat(140), 2f))
        assertEquals(null, SpeechFollow.target(500, 1000, 0f, 300f, 600f, 0f))
        // Half way: the middle of the reply a third of the way down the view.
        assertEquals(100f + 500f - 200f, SpeechFollow.target(500, 1000, 100f, 1000f, 600f, 900f)!!, 0.01f)
        assertEquals(0f, SpeechFollow.target(0, 1000, 100f, 1000f, 600f, 900f)!!, 0.01f)
        assertEquals(900f, SpeechFollow.target(5000, 1000, 100f, 1000f, 600f, 900f)!!, 0.01f)
    }

    @Test fun theMarksWordParses() {
        val nodes = CruMark.wordNodes()
        assertTrue(nodes.first() is androidx.compose.ui.graphics.vector.PathNode.MoveTo)
        // One move, then nine curves and two lines.
        assertEquals(1, nodes.count { it is androidx.compose.ui.graphics.vector.PathNode.MoveTo })
        assertEquals(2, nodes.count { it is androidx.compose.ui.graphics.vector.PathNode.LineTo })
        assertEquals(9, nodes.count { it is androidx.compose.ui.graphics.vector.PathNode.CurveTo })
    }

    @Test fun theShortBarsPulse() {
        assertEquals(1f, CruMark.pulse(0f), 0.001f)
        assertEquals(0.45f, CruMark.pulse(0.5f), 0.001f)
        assertEquals(1f, CruMark.pulse(1f), 0.001f)
        assertEquals(CruMark.pulse(0.9f), CruMark.pulse(-0.1f), 0.001f)
    }

    @Test fun unheardReportsCountAndTheirLink() {
        val ev = kotlinx.serialization.json.Json.parseToJsonElement("""{"count":3,"latestAt":5}""") as kotlinx.serialization.json.JsonObject
        assertEquals(3, CruUnread.countOf(ev))
        assertEquals(null, CruUnread.countOf(kotlinx.serialization.json.JsonObject(emptyMap())))
        assertTrue(CruUnread.isChatPath("/?cru=chat"))
        assertTrue(CruUnread.isChatPath(CruUnread.CHAT_PATH))
        assertTrue(CruUnread.isChatPath("/?x=1&cru=chat"))
        assertFalse(CruUnread.isChatPath("/inbox?cru=chat"))
        assertFalse(CruUnread.isChatPath("/chat/abc"))
        assertFalse(CruUnread.isChatPath(""))
        CruUnread.set(-2)
        assertEquals(0, CruUnread.count.value)
    }

    @Test fun thinkingShowsATypingBubbleOnlyWhileCruThinks() {
        assertTrue(CruChat.thinking(VoiceState(phase = Phase.Thinking)))
        assertFalse(CruChat.thinking(VoiceState(phase = Phase.Listening)))
        assertFalse(CruChat.thinking(VoiceState(phase = Phase.Speaking)))
    }

    @Test fun theSpeakingSignGoesByTheReplyBeingSaid() {
        val list = listOf(CruMsg(1, "hi", "Yes, Sir?", ""), CruMsg(2, "and?", "Done, Sir.", "detail"), CruMsg(3, "", "Yes, Sir?", ""))
        assertEquals(1, CruChat.speakingIndex(list, VoiceState(phase = Phase.Speaking, lastSaid = "Done, Sir.")))
        // The same words twice: the latest.
        assertEquals(2, CruChat.speakingIndex(list, VoiceState(phase = Phase.Speaking, lastSaid = "Yes, Sir? ")))
        // Not speaking, or saying something not in the list (an error, "Yes, Sir?" when called): none.
        assertEquals(-1, CruChat.speakingIndex(list, VoiceState(phase = Phase.Listening, lastSaid = "Done, Sir.")))
        assertEquals(-1, CruChat.speakingIndex(list, VoiceState(phase = Phase.Speaking, lastSaid = "I can't reach the server, Sir.")))
        assertEquals(-1, CruChat.speakingIndex(emptyList(), VoiceState(phase = Phase.Speaking, lastSaid = "Done, Sir.")))
    }

    @Test fun theSignsStayInRangeAndTheDotsTakeTurns() {
        for (k in 0 until 3) for (n in 0..100) {
            val t = n / 100f
            assertTrue(VoiceIndicators.dot(t, k) in 0f..1f)
            if (k < 2) assertTrue(VoiceIndicators.wave(t, k) in 0.25f..1f)
        }
        // The speaking head's waves light in turn: the inner, then the outer.
        assertTrue(VoiceIndicators.wave(0.3f, 0) > 0.95f && VoiceIndicators.wave(0.3f, 1) < 0.3f)
        assertTrue(VoiceIndicators.wave(0.6f, 1) > 0.95f && VoiceIndicators.wave(0.6f, 0) == 0.25f)
        // Each dot is highest at its own time, a third of a loop apart.
        assertTrue(VoiceIndicators.dot(0.2f, 0) > 0.9f && VoiceIndicators.dot(0.2f, 1) == 0f)
        assertTrue(VoiceIndicators.dot(0.2f + 1 / 3f, 1) > 0.9f && VoiceIndicators.dot(0.2f + 1 / 3f, 0) == 0f)
    }
}
