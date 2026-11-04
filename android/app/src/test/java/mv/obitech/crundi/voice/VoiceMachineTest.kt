package mv.obitech.crundi.voice

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The conversation rules of the voice assistant (VoiceMachine), with no Android. */
class VoiceMachineTest {
    private val m = VoiceMachine(silenceMs = 30_000, startMs = 8_000)

    private fun run(vararg events: VoiceEvent): Pair<VoiceState, List<VoiceEffect>> {
        var s = VoiceState()
        val fx = ArrayList<VoiceEffect>()
        for (e in events) { val (n, f) = m.on(s, e); s = n; fx += f }
        return s to fx
    }

    private fun listening() = arrayOf(VoiceEvent.Opened(false, 0), VoiceEvent.Ready(10))

    @Test fun anUtteranceIsAskedAndTheAnswerSpokenWithTheStreamPaused() {
        val (s, fx) = run(*listening(), VoiceEvent.Interim("how much", 20), VoiceEvent.Utterance("how much usage do I have left?", 30),
            VoiceEvent.Reply(true, "Plenty, Master.", "5h: 2%", "", 40))
        assertEquals(Phase.Speaking, s.phase)
        assertEquals("Plenty, Master.", s.reply)
        assertEquals("5h: 2%", s.detail)
        assertEquals(listOf(VoiceEffect.Ask("how much usage do I have left?"), VoiceEffect.PauseStream, VoiceEffect.Say("Plenty, Master.")), fx)
    }

    @Test fun afterSpeakingItListensAgain() {
        val (s, fx) = run(*listening(), VoiceEvent.Utterance("hi", 30), VoiceEvent.Reply(true, "Yes, Master.", "", "", 40), VoiceEvent.SpeechDone(50))
        assertEquals(Phase.Listening, s.phase)
        assertEquals(VoiceEffect.ResumeStream, fx.last())
    }

    @Test fun talkingOverCrundiStopsItAndResumesTheStream() {
        val (s, fx) = run(*listening(), VoiceEvent.Utterance("hi", 30), VoiceEvent.Reply(true, "A long answer.", "", "", 40), VoiceEvent.BargeIn(45))
        assertEquals(Phase.Listening, s.phase)
        assertEquals(listOf(VoiceEffect.StopSaying, VoiceEffect.ResumeStream), fx.takeLast(2))
        // A late "done" from the stopped speech changes nothing.
        assertEquals(Phase.Listening, m.on(s, VoiceEvent.SpeechDone(50)).first.phase)
    }

    @Test fun whatIsSaidWhileThinkingIsSentNext() {
        val (s, fx) = run(*listening(), VoiceEvent.Utterance("first", 30), VoiceEvent.Utterance("and also this", 35),
            VoiceEvent.Reply(true, "Done.", "", "", 40), VoiceEvent.SpeechDone(50))
        assertEquals(Phase.Thinking, s.phase)
        assertEquals("and also this", s.asked)
        assertEquals("", s.pending)
        assertEquals(VoiceEffect.Ask("and also this"), fx.last())
        assertEquals(2, fx.count { it is VoiceEffect.Ask })
    }

    @Test fun anErrorIsSaidAndShown() {
        val (s, fx) = run(*listening(), VoiceEvent.Utterance("hi", 30), VoiceEvent.Reply(false, "", "", "No Claude API key is set.", 40))
        assertEquals(Phase.Speaking, s.phase)
        assertEquals("No Claude API key is set.", s.notice)
        assertEquals(VoiceEffect.Say("No Claude API key is set."), fx.last())
    }

    @Test fun quietForTheTimeoutGoesBackToSleep() {
        val (s1, _) = run(*listening(), VoiceEvent.Tick(29_000))
        assertEquals(Phase.Listening, s1.phase)
        val (s2, fx) = run(*listening(), VoiceEvent.Tick(30_010))
        assertEquals(Phase.Ended, s2.phase)
        assertEquals(listOf(VoiceEffect.Finish), fx)
    }

    @Test fun speakingIsNeverCutOffByTheTimeout() {
        val (s, _) = run(*listening(), VoiceEvent.Utterance("hi", 30), VoiceEvent.Reply(true, "A long answer.", "", "", 40), VoiceEvent.Tick(90_000))
        assertEquals(Phase.Speaking, s.phase)
    }

    @Test fun aLostStreamMovesToThePhonesRecogniserOnce() {
        val (s, fx) = run(VoiceEvent.Opened(false, 0), VoiceEvent.StreamLost("no Deepgram key", 5), VoiceEvent.StreamLost("closed", 6))
        assertTrue(s.offline)
        assertEquals(Phase.Listening, s.phase)
        assertEquals(1, fx.count { it == VoiceEffect.UseFallback })
    }

    @Test fun noAnswerFromTheServerWhileStartingAlsoFallsBack() {
        val (s, fx) = run(VoiceEvent.Opened(false, 0), VoiceEvent.Tick(8_500))
        assertTrue(s.offline)
        assertEquals(listOf(VoiceEffect.UseFallback), fx)
    }

    @Test fun lockedIsKeptUntilUnlocked() {
        val (s, _) = run(VoiceEvent.Opened(true, 0), VoiceEvent.Ready(1))
        assertTrue(s.locked)
        assertFalse(m.on(s, VoiceEvent.Unlocked(2)).first.locked)
    }

    @Test fun closingStopsSpeechAndNothingHappensAfter() {
        val (s, fx) = run(*listening(), VoiceEvent.Close, VoiceEvent.Utterance("hello?", 40))
        assertEquals(Phase.Ended, s.phase)
        assertEquals(listOf(VoiceEffect.StopSaying, VoiceEffect.Finish), fx)
    }

    @Test fun blankUtterancesAreIgnored() {
        val (s, fx) = run(*listening(), VoiceEvent.Utterance("   ", 30))
        assertEquals(Phase.Listening, s.phase)
        assertTrue(fx.isEmpty())
    }
}

class SpeechTest {
    @Test fun repliesAreSaidASentenceAtATime() {
        assertEquals(
            listOf("The board is empty, Master.", "Every one of its thirty-one tasks is done."),
            Speech.sentences("The board is empty, Master. Every one of its thirty-one tasks is done."),
        )
    }

    @Test fun shortPiecesAreJoined() {
        assertEquals(listOf("Yes. On it. The board is empty, Master."), Speech.sentences("Yes. On it. The board is empty, Master."))
        assertEquals(listOf("Roger."), Speech.sentences("Roger."))
        assertTrue(Speech.sentences("   ").isEmpty())
    }
}

class DeviceVoicesTest {
    private val gbLocal = DeviceVoice("en-gb-x-gbd-local", "en-GB")
    private val rjsNet = DeviceVoice("en-gb-x-rjs-network", "en-GB", needsNetwork = true)
    private val rjsLocal = DeviceVoice("en-gb-x-rjs-local", "en-GB")
    private val us = DeviceVoice("en-us-x-iol-local", "en-US")

    @Test fun d34Order() {
        assertEquals(rjsNet, DeviceVoices.pick(listOf(us, gbLocal, rjsLocal, rjsNet), null))
        assertEquals(rjsLocal, DeviceVoices.pick(listOf(us, gbLocal, rjsLocal, rjsNet), null, online = false))
        assertEquals(gbLocal, DeviceVoices.pick(listOf(us, gbLocal), null))
        assertNull(DeviceVoices.pick(listOf(us), null))
    }

    @Test fun theChosenVoiceWinsWhileThePhoneHasIt() {
        assertEquals(us, DeviceVoices.pick(listOf(us, rjsNet), "en-us-x-iol-local"))
        assertEquals(rjsNet, DeviceVoices.pick(listOf(rjsNet), "gone-voice"))
        assertEquals(rjsLocal, DeviceVoices.pick(listOf(rjsLocal, rjsNet.copy(installed = false)), null))
    }
}

class BargeInDetectorTest {
    @Test fun talkingOverCrundiIsHeardAfterAMoment() {
        val d = BargeInDetector(holdMs = 280)
        repeat(50) { d.learn(0.01f) }
        assertFalse(d.hear(0.3f, 100))
        assertFalse(d.hear(0.3f, 100))
        assertTrue(d.hear(0.3f, 100))
    }

    @Test fun aQuietRoomOrABlipIsNot() {
        val d = BargeInDetector(holdMs = 280)
        repeat(50) { d.learn(0.01f) }
        repeat(10) { assertFalse(d.hear(0.015f, 100)) }
        assertFalse(d.hear(0.3f, 100)); assertFalse(d.hear(0.01f, 100)); assertFalse(d.hear(0.3f, 100))
    }

    @Test fun aNoisyRoomRaisesTheBar() {
        val d = BargeInDetector(holdMs = 200)
        repeat(200) { d.learn(0.1f) }
        repeat(5) { assertFalse(d.hear(0.2f, 100)) }
        d.reset()
        assertFalse(d.hear(0.5f, 100)); assertTrue(d.hear(0.5f, 100))
    }
}
