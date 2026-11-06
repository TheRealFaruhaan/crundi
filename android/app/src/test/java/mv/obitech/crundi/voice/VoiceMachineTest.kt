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

    @Test fun wordsFromTheRoomDoNotKeepTheMicrophoneOn() {
        // A television: words keep coming, nothing is ever sent.
        val noise = (1..29).map { VoiceEvent.Interim("and now the news", it * 1_000L) }.toTypedArray()
        val (s1, _) = run(*listening(), *noise, VoiceEvent.Tick(30_500))
        assertEquals(Phase.Listening, s1.phase) // mid-sentence, perhaps: the grace holds it
        val (s2, fx) = run(*listening(), *noise, VoiceEvent.Tick(32_100))
        assertEquals(Phase.Ended, s2.phase)
        assertEquals(listOf(VoiceEffect.Finish), fx)
    }

    @Test fun anAnswerStartsTheMicrophonesTimeAgain() {
        val (s, _) = run(*listening(), VoiceEvent.Utterance("hi", 20_000), VoiceEvent.Reply(true, "Yes, Master.", "", "", 21_000), VoiceEvent.SpeechDone(22_000), VoiceEvent.Tick(45_000))
        assertEquals(Phase.Listening, s.phase)
        assertEquals(Phase.Ended, m.on(s, VoiceEvent.Tick(52_100)).first.phase)
    }

    @Test fun heyCruWhileAwakeIsAnsweredAtOnceWithoutClaude() {
        val (s, fx) = run(*listening(), VoiceEvent.Utterance("Hey crew.", 30))
        assertEquals(Phase.Speaking, s.phase)
        assertEquals(listOf(VoiceEffect.PauseStream, VoiceEffect.Say(Speech.HERE)), fx)
        assertTrue(fx.none { it is VoiceEffect.Ask })
        assertTrue(Speech.isCalling("Cru") && Speech.isCalling("hey Kru!") && Speech.isCalling("crew crew"))
        assertFalse(Speech.isCalling("hey crew what's running") || Speech.isCalling("the crew") || Speech.isCalling("hey"))
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
    private fun d() = BargeInDetector(holdMs = 400, calibrateMs = 700).also { x -> repeat(50) { x.learn(0.01f) } }

    @Test fun crundisOwnVoiceThroughTheMicIsNotAnInterruption() {
        val x = d()
        // Crundi loud on the speaker, its echo at 0.4 of that in the microphone, for 3 s.
        repeat(30) { i -> val v = if (i % 3 == 0) 0.3f else 0.6f; assertFalse("at ${i * 100} ms", x.hear(v * 0.4f, v, 100)) }
    }

    @Test fun masterTalkingOverItIs() {
        val x = d()
        repeat(8) { x.hear(0.2f, 0.5f, 100) }          // learning the echo (0.4)
        assertFalse(x.hear(0.7f, 0.5f, 100))
        assertFalse(x.hear(0.7f, 0.5f, 100))
        assertFalse(x.hear(0.7f, 0.5f, 100))
        assertTrue(x.hear(0.7f, 0.5f, 100))
    }

    @Test fun neverInTheFirstMomentOfAnAnswer() {
        val x = d()
        repeat(7) { assertFalse(x.hear(0.9f, 0.5f, 100)) }
    }

    @Test fun withThePhonesVoiceOnlyALoudVoiceCounts() {
        val x = d()
        repeat(20) { assertFalse(x.hear(0.3f, null, 100)) }
        x.reset(); repeat(7) { x.hear(0.6f, null, 100) }
        assertFalse(x.hear(0.6f, null, 100)); assertFalse(x.hear(0.6f, null, 100)); assertFalse(x.hear(0.6f, null, 100))
        assertTrue(x.hear(0.6f, null, 100))
    }
}

class EchoTest {
    private val m = VoiceMachine()
    private val said = "Crundi has the most kanban tasks, Master, with thirty-one, all of them done."

    private fun afterAnswer(): VoiceState {
        var s = VoiceState()
        for (e in listOf(VoiceEvent.Opened(false, 0), VoiceEvent.Ready(1), VoiceEvent.Utterance("which project has the most tasks", 2),
            VoiceEvent.Reply(true, said, "", "", 3), VoiceEvent.SpeechDone(1_000))) s = m.on(s, e).first
        return s
    }

    @Test fun crundisOwnWordsHeardBackAreNotAsked() {
        val (s, fx) = m.on(afterAnswer(), VoiceEvent.Utterance("all of them done", 1_400))
        assertEquals(Phase.Listening, s.phase)
        assertTrue(fx.isEmpty())
        assertTrue(Speech.isEcho("with thirty-one, all of them", said))
    }

    @Test fun masterSpeakingRightAfterIsAsked() {
        val (s, fx) = m.on(afterAnswer(), VoiceEvent.Utterance("and the done column?", 1_400))
        assertEquals(Phase.Thinking, s.phase)
        assertEquals(VoiceEffect.Ask("and the done column?"), fx.last())
        assertFalse(Speech.isEcho("what is in the backlog of dhivo", said))
    }

    @Test fun theSameWordsLaterAreNotAnEcho() {
        val (s, _) = m.on(afterAnswer(), VoiceEvent.Utterance("all of them done", 9_000))
        assertEquals(Phase.Thinking, s.phase)
    }
}
