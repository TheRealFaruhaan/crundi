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
        assertEquals(listOf(VoiceEffect.Ask("how much usage do I have left?"), VoiceEffect.Cue, VoiceEffect.PauseStream, VoiceEffect.Say("Plenty, Master.")), fx)
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

    @Test fun whatIsSaidWhileThinkingIsSentTheMomentTheAnswerComes() {
        val (s, fx) = run(*listening(), VoiceEvent.Utterance("first", 30), VoiceEvent.Utterance("and also this", 35),
            VoiceEvent.Reply(true, "Done.", "", "", 40))
        // Not after the answer has been said: he has moved on. The answer stays on screen.
        assertEquals(Phase.Thinking, s.phase)
        assertEquals("and also this", s.asked)
        assertEquals("Done.", s.reply)
        assertEquals("", s.pending)
        assertTrue(fx.none { it is VoiceEffect.Say })
        assertEquals(listOf(VoiceEffect.Ask("first"), VoiceEffect.Ask("and also this")), fx.filterIsInstance<VoiceEffect.Ask>())
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
        assertEquals(VoiceEffect.PauseStream, fx.first())
        assertTrue((fx.last() as VoiceEffect.Say).text in Speech.HERE)
        val pick = Speech.hereLine(kotlin.random.Random(7))
        val lines = (1..30).map { pick() }
        assertTrue(lines.zipWithNext().none { (a, b) -> a == b } && lines.toSet().size > 3)
        assertTrue(fx.none { it is VoiceEffect.Ask })
        assertTrue(Speech.isCalling("Cru") && Speech.isCalling("hey Kru!") && Speech.isCalling("crew crew"))
        assertFalse(Speech.isCalling("hey crew what's running") || Speech.isCalling("the crew") || Speech.isCalling("hey"))
    }

    @Test fun askedToCloseItClosesOnceTheGoodbyeIsSaid() {
        val (s1, fx1) = run(*listening(), VoiceEvent.Utterance("that's all, thanks", 30), VoiceEvent.Reply(true, "Goodbye, Sir.", "", "", 40, close = true))
        assertEquals(Phase.Speaking, s1.phase)
        assertTrue(fx1.none { it == VoiceEffect.Dismiss })
        val (s2, fx2) = m.on(s1, VoiceEvent.SpeechDone(60))
        assertEquals(Phase.Ended, s2.phase)
        assertEquals(listOf(VoiceEffect.Dismiss), fx2)
        // Nothing to say: closes at once.
        val (_, fx3) = run(*listening(), VoiceEvent.Utterance("close", 30), VoiceEvent.Reply(true, "", "", "", 40, close = true))
        assertEquals(VoiceEffect.Dismiss, fx3.last())
    }

    @Test fun newsIsSaidWhenListeningAndKeptWhileBusy() {
        val (s1, fx1) = run(*listening(), VoiceEvent.Report("Sir, the API chat has finished.", "tests pass", 20))
        assertEquals(Phase.Speaking, s1.phase)
        assertEquals(VoiceEffect.Say("Sir, the API chat has finished."), fx1.last())
        // Busy answering: kept, then said once the answer has been.
        val (s2, _) = run(*listening(), VoiceEvent.Utterance("hi", 30), VoiceEvent.Report("Sir, a chat waits on you.", "", 35))
        assertEquals(Phase.Thinking, s2.phase)
        assertEquals(1, s2.reports.size)
        val (s3, _) = m.on(s2, VoiceEvent.Reply(true, "Hello, Sir.", "", "", 40))
        val (s4, fx4) = m.on(s3, VoiceEvent.SpeechDone(50))
        assertEquals(VoiceEffect.Say("Sir, a chat waits on you."), fx4.last())
        assertTrue(s4.reports.isEmpty())
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
        assertTrue(VoiceEffect.Ask("and the done column?") in fx)
        assertFalse(Speech.isEcho("what is in the backlog of dhivo", said))
    }

    @Test fun theSameWordsLaterAreNotAnEcho() {
        val (s, _) = m.on(afterAnswer(), VoiceEvent.Utterance("all of them done", 9_000))
        assertEquals(Phase.Thinking, s.phase)
    }
}

/** Talking over Cru (barge-in) with a live stream, and the "one moment" cue. */
class BargeInTest {
    private val m = VoiceMachine(silenceMs = 30_000)
    private val said = "Your build passed an hour ago, Sir, and the tests are green."

    private fun run(s0: VoiceState, vararg events: VoiceEvent): Pair<VoiceState, List<VoiceEffect>> {
        var s = s0
        val fx = ArrayList<VoiceEffect>()
        for (e in events) { val (n, f) = m.on(s, e); s = n; fx += f }
        return s to fx
    }

    /** Speaking [said] on a server that drops Cru's own words (live). */
    private fun speaking(live: Boolean = true): Pair<VoiceState, List<VoiceEffect>> =
        run(VoiceState(), VoiceEvent.Opened(false, 0), VoiceEvent.Ready(10, live = live), VoiceEvent.Utterance("did the build pass", 20), VoiceEvent.Reply(true, said, "", "", 30))

    @Test fun aLiveStreamStaysOnWhileCruSpeaks() {
        val (s, fx) = speaking()
        assertEquals(Phase.Speaking, s.phase)
        assertTrue(s.live)
        assertEquals(listOf(VoiceEffect.Saying(said), VoiceEffect.Say(said)), fx.takeLast(2))
        assertTrue(fx.none { it == VoiceEffect.PauseStream })
        // Finished: the server is told, nothing to resume.
        val (_, done) = m.on(s, VoiceEvent.SpeechDone(40))
        assertEquals(listOf(VoiceEffect.Said), done)
    }

    @Test fun hisWordsCutCruOffAtOnce() {
        val (s0, _) = speaking()
        val (s, fx) = m.on(s0, VoiceEvent.Interim("which build", 35))
        assertEquals(Phase.Listening, s.phase)
        assertEquals("which build", s.heard)
        assertEquals(listOf(VoiceEffect.StopSaying, VoiceEffect.Said), fx)
        // And once he finishes, it is asked straight away.
        val (s2, fx2) = m.on(s, VoiceEvent.Utterance("which build was that", 40))
        assertEquals(Phase.Thinking, s2.phase)
        assertTrue(VoiceEffect.Ask("which build was that") in fx2)
    }

    @Test fun aFinishedSentenceOverCruIsAskedAtOnce() {
        val (s0, _) = speaking()
        val (s, fx) = m.on(s0, VoiceEvent.Utterance("open the kanban board", 35))
        assertEquals(Phase.Thinking, s.phase)
        assertEquals(listOf(VoiceEffect.StopSaying, VoiceEffect.Said, VoiceEffect.Ask("open the kanban board"), VoiceEffect.Cue), fx)
    }

    @Test fun cruHeardBackIsNotAnInterruption() {
        val (s0, _) = speaking()
        for (heard in listOf("your build passed an hour ago", "and the tests are green", "um", "really")) {
            val (s, fx) = m.on(s0, VoiceEvent.Interim(heard, 35))
            assertEquals(heard, Phase.Speaking, s.phase)
            assertTrue(heard, fx.isEmpty())
        }
        val (s, fx) = m.on(s0, VoiceEvent.Utterance("the tests are green", 35))
        assertEquals(Phase.Speaking, s.phase)
        assertTrue(fx.isEmpty())
    }

    @Test fun stopAloneIsEnough() {
        val (s0, _) = speaking()
        assertEquals(Phase.Listening, m.on(s0, VoiceEvent.Interim("stop", 35)).first.phase)
        assertEquals(Phase.Listening, m.on(s0, VoiceEvent.Interim("wait", 35)).first.phase)
    }

    @Test fun aVoiceOverCruLowersTheBarToOneWord() {
        val (s0, _) = speaking()
        val (s1, fx1) = m.on(s0, VoiceEvent.VoiceHeard(33))
        assertEquals(Phase.Speaking, s1.phase)          // a cough is not an interruption
        assertTrue(fx1.isEmpty())
        assertEquals(Phase.Listening, m.on(s1, VoiceEvent.Interim("really", 35)).first.phase)
        // Long after, one word is not enough again.
        assertEquals(Phase.Speaking, m.on(s1, VoiceEvent.Interim("really", 5_000)).first.phase)
    }

    @Test fun anOlderServerPausesAndTheLevelAloneCutsIn() {
        val (s0, fx0) = speaking(live = false)
        assertEquals(listOf(VoiceEffect.PauseStream, VoiceEffect.Say(said)), fx0.takeLast(2))
        assertEquals(Phase.Speaking, m.on(s0, VoiceEvent.Interim("which build", 35)).first.phase)
        val (s, fx) = m.on(s0, VoiceEvent.VoiceHeard(35))
        assertEquals(Phase.Listening, s.phase)
        assertEquals(listOf(VoiceEffect.StopSaying, VoiceEffect.ResumeStream), fx)
    }

    @Test fun tappingCutsInOnALiveStreamToo() {
        val (s0, _) = speaking()
        val (s, fx) = m.on(s0, VoiceEvent.BargeIn(35))
        assertEquals(Phase.Listening, s.phase)
        assertEquals(listOf(VoiceEffect.StopSaying, VoiceEffect.Said), fx)
    }

    @Test fun theFallbackRecogniserIsNeverLive() {
        val (s, _) = run(VoiceState(), VoiceEvent.Opened(false, 0), VoiceEvent.StreamLost("closed", 5), VoiceEvent.Ready(6, live = true))
        assertFalse(s.live)
    }

    @Test fun aCuePlaysWhenSomethingIsAskedNotForHereOrNews() {
        val (_, fx) = run(VoiceState(), VoiceEvent.Opened(false, 0), VoiceEvent.Ready(10, live = true), VoiceEvent.Utterance("what's running", 20))
        assertEquals(listOf(VoiceEffect.Ask("what's running"), VoiceEffect.Cue), fx)
        val (_, here) = run(VoiceState(), VoiceEvent.Opened(false, 0), VoiceEvent.Ready(10), VoiceEvent.Utterance("hey cru", 20))
        assertTrue(here.none { it == VoiceEffect.Cue })
        val (_, news) = run(VoiceState(), VoiceEvent.Opened(false, 0), VoiceEvent.Ready(10), VoiceEvent.Report("Sir, a chat finished.", "", 20))
        assertTrue(news.none { it == VoiceEffect.Cue })
    }

    @Test fun theCueIsToldToTheServerAndItsEchoIsNotAsked() {
        val (s0, _) = run(VoiceState(), VoiceEvent.Opened(false, 0), VoiceEvent.Ready(10, live = true), VoiceEvent.Utterance("what's running", 20))
        val (s1, fx1) = m.on(s0, VoiceEvent.CueStarted("One moment, Sir.", 21))
        assertEquals(listOf(VoiceEffect.Saying("One moment, Sir.")), fx1)
        // Its echo is not kept as something he said meanwhile.
        val (s2, _) = m.on(s1, VoiceEvent.Utterance("one moment sir", 900))
        assertEquals("", s2.pending)
        val (s3, fx3) = m.on(s2, VoiceEvent.CueDone(1_000))
        assertEquals("", s3.cue)
        assertEquals(listOf(VoiceEffect.Said), fx3)
        // The answer: no Said for the cue after it starts.
        val (s4, _) = m.on(s1, VoiceEvent.Reply(true, "Two services, Sir.", "", "", 500))
        assertEquals(Phase.Speaking, s4.phase)
        assertTrue(m.on(s4, VoiceEvent.CueDone(600)).second.isEmpty())
    }

    @Test fun talkingOverTheCueStopsIt() {
        val (s0, _) = run(VoiceState(), VoiceEvent.Opened(false, 0), VoiceEvent.Ready(10, live = true), VoiceEvent.Utterance("what's running", 20), VoiceEvent.CueStarted("One moment, Sir.", 21))
        val (s, fx) = m.on(s0, VoiceEvent.Interim("and the docker ones", 300))
        assertEquals(listOf(VoiceEffect.StopCue, VoiceEffect.Said), fx)
        assertEquals("", s.cue)
        assertEquals("and the docker ones", s.heard)
    }

    @Test fun talksOverRule() {
        assertFalse(Speech.talksOver("your build passed", said))
        assertFalse(Speech.talksOver("uh", said))
        assertFalse(Speech.talksOver("really", said))
        assertTrue(Speech.talksOver("really", said, min = 1))
        assertFalse(Speech.talksOver("build sir", said, min = 1))
        assertTrue(Speech.talksOver("which build was it", said))
        assertTrue(Speech.talksOver("stop", said))
    }

    @Test fun cuesAreRandomButNeverTheSameTwiceRunning() {
        val r = kotlin.random.Random(3)
        var last = -1
        val picks = (1..200).map { Speech.pickCue(8, last, r).also { last = it } }
        assertTrue(picks.zipWithNext().none { (a, b) -> a == b })
        assertTrue(picks.all { it in 0..7 } && picks.toSet().size == 8)
        assertEquals(0, Speech.pickCue(1, 0, r))
        assertEquals(-1, Speech.pickCue(0, -1, r))
        assertTrue(Speech.CUES.size in 6..8)
    }
}
