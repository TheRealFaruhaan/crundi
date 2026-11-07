package mv.obitech.crundi.ui.chats

import mv.obitech.crundi.ui.chats.ChatVoice.Handover
import mv.obitech.crundi.ui.chats.ChatVoiceUi.Mode
import mv.obitech.crundi.voice.HeyCrundiPlan
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Sharing the microphone between a chat's voice mode and Cru. */
class ChatVoiceHandoverTest {
    @Test fun turnedOnWhileCruListensItAsks() {
        assertEquals(ChatVoice.TurnOn.AskCru, ChatVoice.turnOn(cruOpen = true))
        // "Hey Cru" alone is no session: the chat just takes the microphone.
        assertEquals(ChatVoice.TurnOn.Go, ChatVoice.turnOn(cruOpen = false))
        assertEquals("Cru is listening. Switch to this chat?", ChatVoice.ASK_SWITCH)
    }

    @Test fun heyCruPausesWithANote() {
        assertEquals("Hey Cru paused while this chat listens", ChatVoice.heyCruNote(wakeListening = true))
        assertNull(ChatVoice.heyCruNote(false))
        // While the chat listens the wake word is off entirely, even in the app or the asleep voice screen.
        val s = HeyCrundiPlan.Situation(trained = true, inAppOn = true, appInFront = true, backgroundOn = true, screenWaiting = true)
        assertEquals(HeyCrundiPlan.Listener.None, HeyCrundiPlan.listener(s.copy(chatVoice = true)))
        // And back by itself when it ends.
        assertEquals(HeyCrundiPlan.Listener.Screen, HeyCrundiPlan.listener(s))
    }

    @Test fun cruOpenedOnPurposePausesVoiceModeAndItGoesOnAfter() {
        // In front or behind Cru's own screen: paused, not off.
        assertEquals(Handover.Pause, ChatVoice.handover(Mode.On, cruOpen = true, foreground = true))
        assertEquals(Handover.Pause, ChatVoice.handover(Mode.On, cruOpen = true, foreground = false))
        // Waiting while Cru is open, or the chat is not in front.
        assertEquals(Handover.None, ChatVoice.handover(Mode.Paused, cruOpen = true, foreground = true))
        assertEquals(Handover.None, ChatVoice.handover(Mode.Paused, cruOpen = false, foreground = false))
        // Cru done and the chat in front: on again by itself.
        assertEquals(Handover.Resume, ChatVoice.handover(Mode.Paused, cruOpen = false, foreground = true))
        assertEquals("Paused: Cru is listening", ChatVoice.PAUSED_FOR_CRU)
    }

    @Test fun otherwiseItEnds() {
        // Behind another app (not Cru): voice mode ends.
        assertEquals(Handover.Off, ChatVoice.handover(Mode.On, cruOpen = false, foreground = false))
        // A hold when Cru takes the microphone just ends.
        assertEquals(Handover.Off, ChatVoice.handover(Mode.Holding, cruOpen = true, foreground = true))
        assertEquals(Handover.None, ChatVoice.handover(Mode.On, cruOpen = false, foreground = true))
        assertEquals(Handover.None, ChatVoice.handover(Mode.Off, cruOpen = true, foreground = false))
    }

    @Test fun pausedLooksLitButNotRecording() {
        val p = ChatVoiceUi(mode = Mode.Paused)
        assertTrue(p.lit)
        assertFalse(p.recording)
        assertFalse(p.listening)
        assertEquals("Voice mode moved to another chat.", ChatVoice.MOVED)
    }
}
