package mv.obitech.crundi.ui.chats

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import mv.obitech.crundi.api.ChatReducer
import mv.obitech.crundi.api.CrundiJson
import mv.obitech.crundi.api.VoiceApi
import mv.obitech.crundi.api.model.ChatState
import mv.obitech.crundi.api.model.VoiceSay
import mv.obitech.crundi.voice.HeyCrundiPlan
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Voice mode in a chat: the composer's mic, and Claude's voice-say events. */
class ChatVoiceTest {
    private val caps = setOf("handshake", VoiceApi.CAPABILITY, VoiceApi.CHAT_VOICE)

    @Test fun shownOnlyWithTheCapabilityAndAKey() {
        assertTrue(ChatVoice.shown(caps, collaborator = false, deepgramKeySet = true))
        // An older server: no "chat-voice", no mic (and its settings are not even asked).
        assertFalse(ChatVoice.shown(setOf("handshake", VoiceApi.CAPABILITY), false, true))
        assertFalse(ChatVoice.supported(setOf("handshake", VoiceApi.CAPABILITY), false))
        // The voice assistant not set up, or not known yet.
        assertFalse(ChatVoice.shown(caps, false, false))
        assertFalse(ChatVoice.shown(caps, false, null))
        // A collaborator's chat.
        assertFalse(ChatVoice.shown(caps, collaborator = true, deepgramKeySet = true))
        assertFalse(ChatVoice.supported(caps, collaborator = true))
    }

    @Test fun tapOrHold() {
        assertEquals(ChatVoice.Release.VoiceMode, ChatVoice.release(wasOn = false, heldMs = 0))
        assertEquals(ChatVoice.Release.VoiceMode, ChatVoice.release(false, ChatVoice.HOLD_MS - 1))
        assertEquals(ChatVoice.Release.PushToTalk, ChatVoice.release(false, ChatVoice.HOLD_MS))
        assertEquals(ChatVoice.Release.PushToTalk, ChatVoice.release(false, 4_000))
        // Voice mode on: the mic is a send button, a tap sends what was heard now; a hold turns it off.
        assertEquals(ChatVoice.Release.SendNow, ChatVoice.release(wasOn = true, heldMs = 50))
        assertEquals(ChatVoice.Release.Off, ChatVoice.release(true, 2_000))
        assertEquals(350L, ChatVoice.HOLD_MS)
    }

    @Test fun releaseOnTheMicSendsOverCancelCancels() {
        // Held and let go on the mic: push to talk, sent.
        assertEquals(ChatVoice.Release.PushToTalk, ChatVoice.release(wasOn = false, heldMs = 1_200, overCancel = false))
        // Held, slid onto Cancel and let go there: nothing is sent.
        assertEquals(ChatVoice.Release.Cancel, ChatVoice.release(wasOn = false, heldMs = 1_200, overCancel = true))
        assertEquals(ChatVoice.Release.Cancel, ChatVoice.release(wasOn = true, heldMs = 900, overCancel = true))
        // Tapped in voice mode: sent now.
        assertEquals(ChatVoice.Release.SendNow, ChatVoice.release(wasOn = true, heldMs = 120, overCancel = false))
    }

    @Test fun slidingToCancel() {
        // The mic's centre at 100, Cancel's at 400.
        assertEquals(0f, ChatVoice.cancelProgress(100f, 100f, 400f), 0.001f)
        assertEquals(0f, ChatVoice.cancelProgress(40f, 100f, 400f), 0.001f)   // back past the mic
        assertEquals(0.5f, ChatVoice.cancelProgress(250f, 100f, 400f), 0.001f)
        assertEquals(1f, ChatVoice.cancelProgress(460f, 100f, 400f), 0.001f)  // past Cancel
        assertFalse(ChatVoice.overCancel(ChatVoice.cancelProgress(330f, 100f, 400f)))
        assertTrue(ChatVoice.overCancel(ChatVoice.cancelProgress(340f, 100f, 400f)))
        // No Cancel laid out: never over it.
        assertEquals(0f, ChatVoice.cancelProgress(500f, 100f, 0f), 0.001f)
    }

    @Test fun cancelTapInVoiceMode() {
        assertEquals(ChatVoice.CancelTap.Discard, ChatVoice.cancelTap(hasWords = true))
        // Nothing to drop (or a second tap): voice mode off.
        assertEquals(ChatVoice.CancelTap.Off, ChatVoice.cancelTap(hasWords = false))
    }

    @Test fun wordsSentEarlyAreNotSentTwice() {
        assertEquals("", ChatVoice.afterEarly("Open the kanban.", "open the kanban"))
        assertEquals("and the mind map", ChatVoice.afterEarly("Open the kanban, and the mind map", "open the kanban"))
        // A server that dropped them sends new words: all of them go.
        assertEquals("Something else", ChatVoice.afterEarly(" Something else ", "open the kanban"))
        assertEquals("Anything", ChatVoice.afterEarly("Anything", ""))
    }

    @Test fun theBarsClockAndWords() {
        assertEquals("0:00", ChatVoice.elapsed(0))
        assertEquals("0:07", ChatVoice.elapsed(7_400))
        assertEquals("1:05", ChatVoice.elapsed(65_000))
        assertEquals("12:00", ChatVoice.elapsed(720_000))
        assertEquals("0:00", ChatVoice.elapsed(-50))
        assertEquals("short words", ChatVoice.tail("  short words "))
        val long = ChatVoice.tail("x".repeat(30) + " and the end of what was said")
        assertTrue(long.startsWith("…") && long.endsWith("what was said") && long.length <= 48)
    }

    @Test fun recordingState() {
        assertFalse(ChatVoiceUi().recording)
        assertFalse(ChatVoiceUi(mode = ChatVoiceUi.Mode.Pressed).recording)   // not yet decided: as before
        assertTrue(ChatVoiceUi(mode = ChatVoiceUi.Mode.Holding).recording)
        assertTrue(ChatVoiceUi(mode = ChatVoiceUi.Mode.On).recording)
        assertTrue(ChatVoiceUi(mode = ChatVoiceUi.Mode.Holding, cancelProgress = 0.9f).overCancel)
    }

    @Test fun thePrefix() {
        assertEquals("voice input: open the kanban", ChatVoice.message("  open the kanban "))
        assertNull(ChatVoice.message(""))
        assertNull(ChatVoice.message("   "))
        // Held: the sentences heard go as one message; nothing heard, nothing sent.
        assertEquals("voice input: first this. then that", ChatVoice.message(ChatVoice.joined(listOf("first this.", " ", "then that "))))
        assertNull(ChatVoice.message(ChatVoice.joined(emptyList())))
    }

    @Test fun whenAVoiceSayPlays() {
        val now = 1_000_000_000L
        assertTrue(ChatVoice.plays(voiceMode = true, lastPushToTalkAt = 0, now = now))
        assertFalse(ChatVoice.plays(false, 0, now))
        assertTrue(ChatVoice.plays(false, now - 60_000, now))
        assertTrue(ChatVoice.plays(false, now - ChatVoice.SAY_WINDOW_MS, now))
        assertFalse(ChatVoice.plays(false, now - ChatVoice.SAY_WINDOW_MS - 1, now))
    }

    @Test fun onlyNewSaysAreTaken() {
        val says = listOf(VoiceSay("a", "one", 1), VoiceSay("b", "two", 2), VoiceSay("c", "three", 3))
        assertEquals(listOf("b", "c"), ChatVoice.fresh(says, setOf("a")).map { it.id })
        assertEquals(emptyList<VoiceSay>(), ChatVoice.fresh(says, setOf("a", "b", "c")))
    }

    private fun ev(json: String): JsonObject = CrundiJson.parseToJsonElement(json).jsonObject

    @Test fun voiceSayEventsAreParsed() {
        var s = ChatState("c1", loaded = true)
        s = ChatReducer.message(s, ev("""{"type":"ui-event","id":"c1","event":{"type":"voice-say","id":"v1","text":" Done, Sir. ","at":1762500000000}}"""))
        assertEquals(listOf(VoiceSay("v1", "Done, Sir.", 1762500000000L)), s.voiceSays)
        // The same one again (a repeat) is not added twice; one without text or id is dropped.
        s = ChatReducer.message(s, ev("""{"type":"ui-event","id":"c1","event":{"type":"voice-say","id":"v1","text":"Done, Sir.","at":1762500000000}}"""))
        s = ChatReducer.message(s, ev("""{"type":"ui-event","id":"c1","event":{"type":"voice-say","id":"v2","text":"  "}}"""))
        s = ChatReducer.message(s, ev("""{"type":"ui-event","id":"c1","event":{"type":"voice-say","text":"no id"}}"""))
        assertEquals(1, s.voiceSays.size)
        // Only the newest few are kept.
        repeat(30) { i -> s = ChatReducer.message(s, ev("""{"type":"ui-event","id":"c1","event":{"type":"voice-say","id":"n$i","text":"line $i","at":${i + 1}}}""")) }
        assertEquals(20, s.voiceSays.size)
        assertEquals("n29", s.voiceSays.last().id)
        // A history replaces the state: no voice-says come back with it.
        s = ChatReducer.message(s, ev("""{"type":"ui-history","id":"c1","session":{"project":"crundi","messages":[]}}"""))
        assertTrue(s.voiceSays.isEmpty())
    }

    @Test fun anOlderAppIgnoresUnknownEvents() {
        val s = ChatState("c1", loaded = true)
        assertEquals(s, ChatReducer.message(s, ev("""{"type":"ui-event","id":"c1","event":{"type":"voice-something-new","id":"x"}}""")))
    }

    @Test fun chatVoiceTakesTheMicFromTheWakeWord() {
        val s = HeyCrundiPlan.Situation(trained = true, inAppOn = true, appInFront = true)
        assertEquals(HeyCrundiPlan.Listener.InApp, HeyCrundiPlan.listener(s))
        assertEquals(HeyCrundiPlan.Listener.None, HeyCrundiPlan.listener(s.copy(chatVoice = true)))
    }
}
