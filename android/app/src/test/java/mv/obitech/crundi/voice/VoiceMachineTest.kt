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

    @Test fun aTypedTurnsReplyIsShownNotSaid() {
        val (s, fx) = run(*listening(), VoiceEvent.Typed("how much usage?", 30), VoiceEvent.Reply(true, "Plenty, Master.", "5h: 2%", "", 40, silent = true))
        assertEquals(Phase.Listening, s.phase)
        assertEquals("Plenty, Master.", s.reply)
        assertEquals("5h: 2%", s.detail)
        assertTrue(fx.none { it is VoiceEffect.Say })
        assertEquals(VoiceEffect.ResumeStream, fx.last())
    }

    @Test fun aTypedTurnsErrorIsANoticeNotSaid() {
        val (s, fx) = run(*listening(), VoiceEvent.Typed("hi", 30), VoiceEvent.Reply(false, "", "", "Could not reach the server", 40, silent = true))
        assertEquals(Phase.Listening, s.phase)
        assertEquals("Could not reach the server", s.notice)
        assertTrue(fx.none { it is VoiceEffect.Say })
    }

    @Test fun aTypedGoodbyeStillCloses() {
        val (s, fx) = run(*listening(), VoiceEvent.Typed("bye", 30), VoiceEvent.Reply(true, "Goodbye, Sir.", "", "", 40, close = true, silent = true))
        assertEquals(Phase.Ended, s.phase)
        assertEquals(VoiceEffect.Dismiss, fx.last())
    }

    @Test fun theSpeakerOnAReplySaysItWhileListening() {
        val (s, fx) = run(*listening(), VoiceEvent.Typed("hi", 30), VoiceEvent.Reply(true, "Yes, Master.", "", "", 40, silent = true), VoiceEvent.SayAgain("Yes, Master.", 50))
        assertEquals(Phase.Speaking, s.phase)
        assertEquals(VoiceEffect.Say("Yes, Master."), fx.last())
        assertEquals(Phase.Listening, m.on(s, VoiceEvent.SpeechDone(60)).first.phase)
    }

    @Test fun theSpeakerWaitsWhileCruThinksOrTalks() {
        val (thinking, _) = run(*listening(), VoiceEvent.Typed("hi", 30))
        assertEquals(thinking to emptyList<VoiceEffect>(), m.on(thinking, VoiceEvent.SayAgain("Earlier answer.", 35)))
        val (talking, _) = run(*listening(), VoiceEvent.Utterance("hi", 30), VoiceEvent.Reply(true, "A long answer.", "", "", 40))
        assertEquals(talking to emptyList<VoiceEffect>(), m.on(talking, VoiceEvent.SayAgain("Earlier answer.", 45)))
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

    // 12 Nov: "show me some factory settings", said as a report began, waited for his next sentence.
    @Test fun wordsHeldByAPauseAreAskedOnceTheNewsIsSaid() {
        val (s1, _) = run(*listening(), VoiceEvent.Report("Sir, the API chat has finished.", "", 20))
        assertEquals(Phase.Speaking, s1.phase)
        val (s2, fx2) = m.on(s1, VoiceEvent.Utterance("show me some factory settings", 25, held = true))
        assertEquals(Phase.Speaking, s2.phase)
        assertEquals("show me some factory settings", s2.pending)
        assertTrue(fx2.none { it is VoiceEffect.StopSaying || it is VoiceEffect.Ask })
        val (s3, fx3) = m.on(s2, VoiceEvent.SpeechDone(40))
        assertEquals(Phase.Thinking, s3.phase)
        assertEquals("", s3.pending)
        assertTrue(fx3.contains(VoiceEffect.Ask("show me some factory settings")))
    }

    @Test fun heldWordsGoWithWhatHeSaysOverCru() {
        val (s1, _) = run(VoiceEvent.Opened(false, 0), VoiceEvent.Ready(10, live = true), VoiceEvent.Report("Sir, the API chat has finished.", "", 20))
        val (s2, _) = m.on(s1, VoiceEvent.Utterance("show me some factory settings", 25, held = true))
        val (s3, fx3) = m.on(s2, VoiceEvent.BargeIn(30))
        assertEquals(Phase.Thinking, s3.phase)
        assertTrue(fx3.contains(VoiceEffect.Ask("show me some factory settings")))
        assertEquals("", s3.pending)
    }

    // 12 Nov, the owner: "first he should be able to finish his speaking, then the response arrives".
    @Test fun newsWaitsUntilHeHasFinishedSpeaking() {
        val (s1, fx1) = run(*listening(), VoiceEvent.SpeechStarted(20), VoiceEvent.Report("Sir, the API chat has finished.", "", 25))
        assertEquals(Phase.Listening, s1.phase)
        assertEquals(1, s1.reports.size)
        assertTrue(fx1.none { it is VoiceEffect.Say })
        // His words come: they are asked; the news waits for the answer.
        val (s2, fx2) = m.on(s1, VoiceEvent.Utterance("show me some factory settings", 1_500))
        assertEquals(Phase.Thinking, s2.phase)
        assertTrue(fx2.contains(VoiceEffect.Ask("show me some factory settings")))
        assertEquals(1, s2.reports.size)
    }

    @Test fun anAnswerThatComesWhileHeSpeaksWaitsAndHisWordsGoFirst() {
        val (s1, _) = run(*listening(), VoiceEvent.Utterance("what is the weather", 30), VoiceEvent.Interim("and show me", 40))
        val (s2, fx2) = m.on(s1, VoiceEvent.Reply(true, "Sunny, Sir.", "", "", 45))
        assertEquals(Phase.Thinking, s2.phase)
        assertTrue(fx2.none { it is VoiceEffect.Say })
        val (s3, fx3) = m.on(s2, VoiceEvent.Utterance("and show me some factory settings", 1_200))
        assertTrue(fx3.contains(VoiceEffect.Ask("and show me some factory settings")))
        assertEquals("Sunny, Sir.", s3.reply)
        assertTrue(fx3.none { it is VoiceEffect.Say })
    }

    @Test fun aSpeechSignWithNoWordsHoldsOnlyForAMoment() {
        val (s1, _) = run(*listening(), VoiceEvent.Utterance("what is the weather", 30), VoiceEvent.SpeechStarted(40))
        val (s2, fx2) = m.on(s1, VoiceEvent.Reply(true, "Sunny, Sir.", "", "", 45))
        assertTrue(fx2.none { it is VoiceEffect.Say })
        val (s3, fx3) = m.on(s2, VoiceEvent.Tick(40 + m.talkMs + 10))
        assertEquals(Phase.Speaking, s3.phase)
        assertEquals(VoiceEffect.Say("Sunny, Sir."), fx3.last())
    }

    // Cru speaks more than once in a turn (say_now): each line said as it comes, then the answer.
    @Test fun linesOfTheTurnAreSaidThenTheAnswer() {
        val (s1, _) = run(*listening(), VoiceEvent.Utterance("show me how to add a card", 30))
        val (s2, fx2) = m.on(s1, VoiceEvent.Line("say1", "First, open the Kanban tab.", 40))
        assertEquals(Phase.Speaking, s2.phase)
        assertTrue(s2.midTurn)
        assertEquals(VoiceEffect.Say("First, open the Kanban tab."), fx2.last())
        // The next line waits for this one; the answer waits for both.
        val (s3, _) = m.on(s2, VoiceEvent.Line("say2", "Now click Add card.", 45))
        val (s4, fx4) = m.on(s3, VoiceEvent.Reply(true, "That's all, Sir.", "", "", 50))
        assertEquals(Phase.Speaking, s4.phase)
        assertTrue(fx4.none { it is VoiceEffect.Say })
        // The first is said: told to the server; the answer is here, so the second is only shown.
        val (s5, fx5) = m.on(s4, VoiceEvent.SpeechDone(60))
        assertTrue(fx5.contains(VoiceEffect.LineDone("say1", "said")))
        assertTrue(fx5.contains(VoiceEffect.LineDone("say2", "shown")))
        assertEquals(VoiceEffect.Say("That's all, Sir."), fx5.last())
        assertFalse(s5.midTurn)
        val (s6, _) = m.on(s5, VoiceEvent.SpeechDone(70))
        assertEquals(Phase.Listening, s6.phase)
    }

    @Test fun eachLineWaitsForTheOneBeforeWhenNoAnswerYet() {
        val (s1, _) = run(*listening(), VoiceEvent.Utterance("walk me through it", 30), VoiceEvent.Line("a", "Step one.", 40), VoiceEvent.Line("b", "Step two.", 41))
        val (s2, fx2) = m.on(s1, VoiceEvent.SpeechDone(50))
        assertTrue(fx2.contains(VoiceEffect.LineDone("a", "said")))
        assertEquals(VoiceEffect.Say("Step two."), fx2.last())
        val (s3, fx3) = m.on(s2, VoiceEvent.SpeechDone(60))
        assertTrue(fx3.contains(VoiceEffect.LineDone("b", "said")))
        assertEquals(Phase.Thinking, s3.phase)
    }

    @Test fun aTypedTurnsLinesAreShownNotSaid() {
        val (s1, _) = run(*listening(), VoiceEvent.Typed("check the server", 30))
        val (s2, fx2) = m.on(s1, VoiceEvent.Line("a", "Checking the server.", 40, silent = true))
        assertEquals(Phase.Thinking, s2.phase)
        assertEquals(listOf<VoiceEffect>(VoiceEffect.LineDone("a", "shown")), fx2)
    }

    @Test fun talkingOverALineKeepsTheTurnGoing() {
        val (s1, _) = run(VoiceEvent.Opened(false, 0), VoiceEvent.Ready(10, live = true), VoiceEvent.Utterance("fix the server", 30), VoiceEvent.Line("a", "Checking the disk now, Sir.", 40))
        val (s2, fx2) = m.on(s1, VoiceEvent.Utterance("and the logs too please", 45))
        assertEquals(Phase.Thinking, s2.phase)
        assertEquals("and the logs too please", s2.pending)
        assertTrue(fx2.contains(VoiceEffect.LineDone("a", "cut")))
        assertTrue(fx2.none { it is VoiceEffect.Ask })
        // The answer comes: what he said goes next.
        val (_, fx3) = m.on(s2, VoiceEvent.Reply(true, "Done, Sir.", "", "", 60))
        assertTrue(fx3.contains(VoiceEffect.Ask("and the logs too please")))
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

    @Test fun aNoiseAsLoudIsNot() {
        val x = d()
        repeat(8) { x.hear(0.2f, 0.5f, 100) }
        repeat(10) { assertFalse(x.hear(0.7f, 0.5f, 100) { false }) }
        assertTrue(x.hear(0.7f, 0.5f, 100) { true })
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

/** Talking over Cru (barge-in): the phone's gate, the pre-roll, and the "one moment" cue. */
class BargeInTest {
    private val m = VoiceMachine(silenceMs = 30_000)
    private val said = "Your build passed an hour ago, Sir, and the tests are green."

    private fun run(s0: VoiceState, vararg events: VoiceEvent): Pair<VoiceState, List<VoiceEffect>> {
        var s = s0
        val fx = ArrayList<VoiceEffect>()
        for (e in events) { val (n, f) = m.on(s, e); s = n; fx += f }
        return s to fx
    }

    /** Speaking [said]; [live]: a server that knows barge-in. */
    private fun speaking(live: Boolean = true): Pair<VoiceState, List<VoiceEffect>> =
        run(VoiceState(), VoiceEvent.Opened(false, 0), VoiceEvent.Ready(10, live = live), VoiceEvent.Utterance("did the build pass", 20), VoiceEvent.Reply(true, said, "", "", 30))

    @Test fun nothingGoesToTheServerWhileCruSpeaks() {
        val (s, fx) = speaking()
        assertEquals(Phase.Speaking, s.phase)
        // Told what Cru says (the server's second net), then paused: the gate listens on the phone.
        assertEquals(listOf(VoiceEffect.Saying(said), VoiceEffect.PauseStream, VoiceEffect.Say(said)), fx.takeLast(3))
        val (_, done) = m.on(s, VoiceEvent.SpeechDone(40))
        assertEquals(listOf(VoiceEffect.Said, VoiceEffect.ResumeStream), done)
    }

    @Test fun theGateHearingHimCutsCruOffAtOnce() {
        val (s0, _) = speaking()
        val (s, fx) = m.on(s0, VoiceEvent.VoiceHeard(35))
        assertEquals(Phase.Listening, s.phase)
        // Stopped here; the stream resumes (the controller sends the pre-roll first).
        assertEquals(listOf(VoiceEffect.StopSaying, VoiceEffect.Said, VoiceEffect.ResumeStream), fx)
        // What he says is then asked as soon as he finishes.
        val (s2, fx2) = m.on(s, VoiceEvent.Utterance("which build was that", 900))
        assertEquals(Phase.Thinking, s2.phase)
        assertTrue(VoiceEffect.Ask("which build was that") in fx2)
    }

    @Test fun heardJustAfterCruFinishedTheStreamResumesAtOnce() {
        val (s0, _) = speaking()
        val (s1, _) = m.on(s0, VoiceEvent.SpeechDone(40))
        assertEquals(listOf(VoiceEffect.ResumeStream), m.on(s1, VoiceEvent.VoiceHeard(200)).second)
    }

    @Test fun hisWordsStillCutInIfTheyArrive() {
        val (s0, _) = speaking()
        val (s, fx) = m.on(s0, VoiceEvent.Interim("which build", 35))
        assertEquals(Phase.Listening, s.phase)
        assertEquals(listOf(VoiceEffect.StopSaying, VoiceEffect.Said, VoiceEffect.ResumeStream), fx)
        val (s2, fx2) = m.on(s0, VoiceEvent.Utterance("open the kanban board", 35))
        assertEquals(Phase.Thinking, s2.phase)
        assertEquals(listOf(VoiceEffect.StopSaying, VoiceEffect.Said, VoiceEffect.ResumeStream, VoiceEffect.Ask("open the kanban board"), VoiceEffect.Cue), fx2)
    }

    @Test fun cruHeardBackIsNotAnInterruption() {
        val (s0, _) = speaking()
        for (heard in listOf("your build passed an hour ago", "and the tests are green", "um", "really")) {
            val (s, fx) = m.on(s0, VoiceEvent.Interim(heard, 35))
            assertEquals(heard, Phase.Speaking, s.phase)
            assertTrue(heard, fx.isEmpty())
        }
    }

    @Test fun anOlderServerPausesAndTheLevelAloneCutsIn() {
        val (s0, fx0) = speaking(live = false)
        assertEquals(listOf(VoiceEffect.PauseStream, VoiceEffect.Say(said)), fx0.takeLast(2))
        assertTrue(fx0.none { it is VoiceEffect.Saying })
        val (s, fx) = m.on(s0, VoiceEvent.VoiceHeard(35))
        assertEquals(Phase.Listening, s.phase)
        assertEquals(listOf(VoiceEffect.StopSaying, VoiceEffect.ResumeStream), fx)
    }

    @Test fun tappingCutsIn() {
        val (s0, _) = speaking()
        val (s, fx) = m.on(s0, VoiceEvent.BargeIn(35))
        assertEquals(Phase.Listening, s.phase)
        assertEquals(listOf(VoiceEffect.StopSaying, VoiceEffect.Said, VoiceEffect.ResumeStream), fx)
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

    private fun cueing(live: Boolean = true) = run(VoiceState(), VoiceEvent.Opened(false, 0), VoiceEvent.Ready(10, live = live), VoiceEvent.Utterance("what's running", 20), VoiceEvent.CueStarted("One moment, Sir.", 21))

    @Test fun theCueIsNeverSentToTheServer() {
        val (s1, fx1) = cueing()
        assertEquals(listOf(VoiceEffect.Saying("One moment, Sir."), VoiceEffect.PauseStream), fx1.takeLast(2))
        val (s3, fx3) = m.on(s1, VoiceEvent.CueDone(1_000))
        assertEquals("", s3.cue)
        assertEquals(listOf(VoiceEffect.Said, VoiceEffect.ResumeStream), fx3)
        // An older server: paused too.
        assertEquals(VoiceEffect.PauseStream, cueing(live = false).second.last())
        // The answer comes while it plays: nothing more for the cue.
        val (s4, _) = m.on(s1, VoiceEvent.Reply(true, "Two services, Sir.", "", "", 500))
        assertEquals(Phase.Speaking, s4.phase)
        assertTrue(m.on(s4, VoiceEvent.CueDone(600)).second.isEmpty())
    }

    @Test fun talkingOverTheCueStopsIt() {
        val (s0, _) = cueing()
        val (s, fx) = m.on(s0, VoiceEvent.VoiceHeard(300))
        assertEquals(listOf(VoiceEffect.StopCue, VoiceEffect.Said, VoiceEffect.ResumeStream), fx)
        assertEquals("", s.cue)
        assertEquals(Phase.Thinking, s.phase)
        val (_, old) = m.on(cueing(live = false).first, VoiceEvent.VoiceHeard(300))
        assertEquals(listOf(VoiceEffect.StopCue, VoiceEffect.ResumeStream), old)
    }

    @Test fun theOwnerSayingCueWordsHimselfIsAsked() {
        // Well after any cue: "let me check" is his, not an echo.
        val (s0, _) = run(VoiceState(), VoiceEvent.Opened(false, 0), VoiceEvent.Ready(10, live = true))
        val (s, fx) = m.on(s0, VoiceEvent.Utterance("Let me check the board.", 20_000))
        assertEquals(Phase.Thinking, s.phase)
        assertTrue(VoiceEffect.Ask("Let me check the board.") in fx)
        val (s1, _) = m.on(s0, VoiceEvent.Utterance("Right away, please.", 20_000))
        assertEquals(Phase.Thinking, s1.phase)
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

/** Taking back what was said: the ×, "scratch that", and a cancelled turn. */
class CancelTest {
    private val m = VoiceMachine(silenceMs = 30_000)

    private fun run(s0: VoiceState, vararg events: VoiceEvent): Pair<VoiceState, List<VoiceEffect>> {
        var s = s0
        val fx = ArrayList<VoiceEffect>()
        for (e in events) { val (n, f) = m.on(s, e); s = n; fx += f }
        return s to fx
    }

    private fun listening(cancellable: Boolean = true) = run(VoiceState(), VoiceEvent.Opened(false, 0, cancellable), VoiceEvent.Ready(10, live = true)).first
    private fun thinking() = run(listening(), VoiceEvent.Utterance("open the dhivo board", 20), VoiceEvent.CueStarted("One moment, Sir.", 21)).first

    @Test fun theCrossShowsFromTheFirstWordsUntilTheAnswer() {
        val l = listening()
        assertFalse(l.canCancel)
        val (heard, _) = m.on(l, VoiceEvent.Interim("open the", 15))
        assertTrue(heard.canCancel)
        assertTrue(thinking().canCancel)
        val (answered, _) = m.on(thinking(), VoiceEvent.Reply(true, "Done, Sir.", "", "", 30))
        assertFalse(answered.canCancel)
        // An older server: no ×.
        assertFalse(m.on(listening(cancellable = false), VoiceEvent.Interim("open the", 15)).first.canCancel)
    }

    @Test fun whileListeningItClearsTheWordsAndTellsTheStream() {
        val (s, fx) = run(listening(), VoiceEvent.Interim("open dhivo", 15), VoiceEvent.CancelTapped(16))
        assertEquals(Phase.Listening, s.phase)
        assertEquals("", s.heard)
        assertEquals(listOf(VoiceEffect.DiscardStream), fx)
    }

    @Test fun whileThinkingItCancelsTheTurnAndStopsTheCue() {
        val (s, fx) = m.on(thinking(), VoiceEvent.CancelTapped(25))
        assertEquals(Phase.Listening, s.phase)
        assertEquals("", s.heard)
        assertEquals("Cancelled.", s.notice)
        assertTrue(VoiceEffect.CancelTurn in fx && VoiceEffect.StopCue in fx && VoiceEffect.DiscardStream in fx)
        assertTrue(fx.none { it is VoiceEffect.Say || it == VoiceEffect.Cue })
        // What was already done comes back from the cancel.
        val (s2, _) = m.on(s, VoiceEvent.Cancelled(listOf("sent \"hi\" to the API chat"), 30))
        assertEquals("Cancelled. Already done: sent \"hi\" to the API chat.", s2.notice)
    }

    // The X on a faded bubble in the chat view (10 Nov): only that phrase goes, the answer in progress stays.
    @Test fun theQueuedBubblesXDropsItWithoutCancellingTheTurn() {
        val (queued, _) = m.on(thinking(), VoiceEvent.Utterance("and the logs", 30))
        assertEquals("and the logs", CruChat.queued(queued))
        val (s, fx) = m.on(queued, VoiceEvent.DropLive(queued = true, now = 31))
        assertEquals(Phase.Thinking, s.phase)
        assertEquals("", s.pending)
        assertEquals("", CruChat.queued(s))
        assertEquals("", CruChat.hearing(s))
        assertEquals(queued.turn, s.turn)
        assertTrue(fx.isEmpty())
        // The answer then comes and is said; nothing more is asked.
        val (r, rfx) = m.on(s, VoiceEvent.Reply(true, "Board open.", "", "", 40, turn = s.turn))
        assertEquals(Phase.Speaking, r.phase)
        assertTrue(rfx.none { it is VoiceEffect.Ask })
        // Words still coming in after the queue stay.
        val (more, _) = m.on(queued, VoiceEvent.Interim("please", 32))
        val (s2, _) = m.on(more, VoiceEvent.DropLive(queued = true, now = 33))
        assertEquals("please", CruChat.hearing(s2))
    }

    @Test fun theHeardBubblesXDropsTheWordsAndTellsTheStream() {
        val (heard, _) = run(listening(), VoiceEvent.Interim("open dhivo", 15))
        val (s, fx) = m.on(heard, VoiceEvent.DropLive(queued = false, now = 16))
        assertEquals("", CruChat.hearing(s))
        assertEquals(listOf(VoiceEffect.DiscardStream), fx)
        // While Cru thinks: the new words go, the turn and the queue stay.
        val (queued, _) = m.on(thinking(), VoiceEvent.Utterance("and the logs", 30))
        val (coming, _) = m.on(queued, VoiceEvent.Interim("please", 31))
        val (t, tfx) = m.on(coming, VoiceEvent.DropLive(queued = false, now = 32))
        assertEquals(Phase.Thinking, t.phase)
        assertEquals("and the logs", CruChat.queued(t))
        assertEquals("", CruChat.hearing(t))
        assertEquals(listOf(VoiceEffect.DiscardStream), tfx)
        // Nothing being heard: nothing happens.
        assertTrue(m.on(thinking(), VoiceEvent.DropLive(queued = false, now = 26)).second.isEmpty())
    }

    @Test fun theCancelledAnswerIsNeitherSaidNorShown() {
        val (s0, _) = m.on(thinking(), VoiceEvent.CancelTapped(25))
        val (s, fx) = m.on(s0, VoiceEvent.Reply(false, "", "", "", 40, turn = 1, cancelled = true))
        assertEquals(Phase.Listening, s.phase)
        assertEquals("", s.reply)
        assertEquals("Cancelled.", s.notice)
        assertTrue(fx.isEmpty())
        // Cancelled too late, the old answer still arrives: not said, even with a new question in flight.
        val (asked, _) = m.on(s0, VoiceEvent.Utterance("open crundi", 50))
        assertEquals(Phase.Thinking, asked.phase)
        val (s3, fx3) = m.on(asked, VoiceEvent.Reply(true, "Dhivo is open.", "", "", 60, turn = 1))
        assertEquals(Phase.Thinking, s3.phase)
        assertTrue(fx3.isEmpty())
        val (s4, fx4) = m.on(asked, VoiceEvent.Reply(true, "Crundi is open.", "", "", 70, turn = asked.turn))
        assertEquals(Phase.Speaking, s4.phase)
        assertTrue(VoiceEffect.Say("Crundi is open.") in fx4)
    }

    @Test fun theServersDiscardClearsTheWordsOrCancelsTheTurn() {
        val (s1, fx1) = run(listening(), VoiceEvent.Interim("open dhivo scratch", 15), VoiceEvent.Discard(16))
        assertEquals("", s1.heard)
        assertTrue(fx1.isEmpty())   // the server already dropped them
        val (s2, fx2) = m.on(thinking(), VoiceEvent.Discard(25))
        assertEquals(Phase.Listening, s2.phase)
        assertTrue(VoiceEffect.CancelTurn in fx2 && VoiceEffect.StopCue in fx2)
        assertTrue(VoiceEffect.DiscardStream !in fx2)
    }

    @Test fun scratchThatWhileThinkingCancelsAndIsNeverQueued() {
        val (s, fx) = run(thinking(), VoiceEvent.Utterance("and also the backlog", 22), VoiceEvent.Utterance("Never mind.", 24))
        assertEquals(Phase.Listening, s.phase)
        assertEquals("", s.pending)
        assertTrue(VoiceEffect.CancelTurn in fx)
        assertTrue(fx.none { it == VoiceEffect.Ask("Never mind.") })
        // Listening, it is simply not asked.
        val (l, lfx) = m.on(listening(), VoiceEvent.Utterance("Scratch that!", 30))
        assertEquals(Phase.Listening, l.phase)
        assertTrue(lfx.none { it is VoiceEffect.Ask })
    }

    @Test fun discardPhrases() {
        assertTrue(Speech.isDiscard("Scratch that."))
        assertTrue(Speech.isDiscard("open dhivo, never mind"))
        assertTrue(Speech.isDiscard("ignore that"))
        assertTrue(Speech.isDiscard("Nevermind!"))
        assertFalse(Speech.isDiscard("cancel"))                // answers Cru's questions
        assertFalse(Speech.isDiscard("scratch that itch on the board"))
        assertFalse(Speech.isDiscard("let me check"))
    }

    @Test fun typingKeepsTheSessionFromWindingDown() {
        // 25 s of nothing heard, then typing: the 30 s timeout starts again from there.
        val (s, fx) = run(listening(), VoiceEvent.Active(25_000), VoiceEvent.Tick(40_000))
        assertEquals(Phase.Listening, s.phase)
        assertTrue(fx.isEmpty())
        val (ended, _) = run(listening(), VoiceEvent.Active(25_000), VoiceEvent.Tick(56_000))
        assertEquals(Phase.Ended, ended.phase)
    }
}

/** Cru speaking up unprompted (news, wake-ups): follows how the owner's last message came in; muted, never. */
class VoiceOffTest {
    private val m = VoiceMachine(silenceMs = 30_000, startMs = 8_000)

    private fun run(vararg events: VoiceEvent) = from(VoiceState(), *events)

    private fun listening() = arrayOf(VoiceEvent.Opened(false, 0), VoiceEvent.Ready(10))


    private fun from(s0: VoiceState, vararg events: VoiceEvent): Pair<VoiceState, List<VoiceEffect>> {
        var s = s0
        val fx = ArrayList<VoiceEffect>()
        for (e in events) { val (n, f) = m.on(s, e); s = n; fx += f }
        return s to fx
    }

    @Test fun typedLastNewsIsShownNotSaid() {
        val (s, fx) = run(*listening(), VoiceEvent.Typed("check the build", 20), VoiceEvent.Reply(true, "On it.", "", "", 30, silent = true),
            VoiceEvent.Report("Sir, the build passed.", "all green", 40))
        assertTrue(s.lastTyped)
        assertEquals(Phase.Listening, s.phase)
        assertEquals("Sir, the build passed.", s.reply)
        assertEquals("all green", s.detail)
        assertTrue(fx.none { it is VoiceEffect.Say })
    }

    @Test fun spokenAgainNewsIsSaidAgain() {
        val (s, fx) = from(VoiceState(lastTyped = true), *listening(), VoiceEvent.Utterance("what's next", 20), VoiceEvent.Reply(true, "Nothing, Sir.", "", "", 30),
            VoiceEvent.SpeechDone(40), VoiceEvent.Report("Sir, a chat waits on you.", "", 50))
        assertFalse(s.lastTyped)
        assertEquals(Phase.Speaking, s.phase)
        assertEquals(VoiceEffect.Say("Sir, a chat waits on you."), fx.last())
    }

    @Test fun crusOwnWordsHeardBackLeaveTheInputModeAsItWas() {
        val (s, _) = run(*listening(), VoiceEvent.Typed("hi", 20), VoiceEvent.Reply(true, "Hello.", "", "", 30, silent = true),
            VoiceEvent.Report("Sir, the build passed.", "", 40))
        // Not said, so nothing to echo; a scratch-that is not a message either.
        val (n, _) = from(s, VoiceEvent.Utterance("scratch that", 50))
        assertTrue(n.lastTyped)
    }

    @Test fun yesOrNoTappedKeepsTheInputMode() {
        val (s, _) = run(*listening(), VoiceEvent.Typed("Yes", 20, tap = true))
        assertFalse(s.lastTyped)
    }

    @Test fun newsQueuedWhileTypedAllShowWithoutAWord() {
        val (s, fx) = run(*listening(), VoiceEvent.Typed("hi", 20), VoiceEvent.Report("First.", "", 25), VoiceEvent.Report("Second.", "", 26),
            VoiceEvent.Reply(true, "Hello.", "", "", 30, silent = true))
        assertEquals(Phase.Listening, s.phase)
        assertEquals("Second.", s.reply)
        assertTrue(s.reports.isEmpty())
        assertTrue(fx.none { it is VoiceEffect.Say })
    }

    @Test fun mutedNothingIsSaid() {
        val (s, fx) = from(VoiceState(voiceOff = true), *listening(), VoiceEvent.Utterance("how much usage?", 20), VoiceEvent.Reply(true, "Plenty, Sir.", "", "", 30),
            VoiceEvent.Report("Sir, the build passed.", "", 40), VoiceEvent.SayAgain("Plenty, Sir.", 50), VoiceEvent.Utterance("hey cru", 60))
        assertEquals(Phase.Listening, s.phase)
        assertTrue(fx.none { it is VoiceEffect.Say })
    }

    @Test fun mutingWhileCruTalksStopsIt() {
        val (s, fx) = run(*listening(), VoiceEvent.Utterance("hi", 20), VoiceEvent.Reply(true, "Hello, Sir.", "", "", 30), VoiceEvent.VoiceOff(true, 40))
        assertTrue(s.voiceOff)
        assertEquals(Phase.Listening, s.phase)
        assertEquals(VoiceEffect.StopSaying, fx.first { it == VoiceEffect.StopSaying })
        val (back, fx2) = from(s, VoiceEvent.VoiceOff(false, 50), VoiceEvent.Report("Sir, done.", "", 60))
        assertFalse(back.voiceOff)
        assertEquals(VoiceEffect.Say("Sir, done."), fx2.last())
    }

    // 13 Nov (the owner): Cru's news was read out while he was in keyboard mode ("chat").
    @Test fun keyboardModeShowsNewsWithoutSayingIt() {
        val (s, fx) = from(VoiceState(typing = true), *listening(), VoiceEvent.Report("Sir, the build is done.", "", 20))
        assertTrue(fx.none { it is VoiceEffect.Say })
        assertEquals("Sir, the build is done.", s.reply)
        // Out of keyboard mode, with his last message spoken: said, as before.
        val (_, fx2) = from(VoiceState(typing = false), *listening(), VoiceEvent.Report("Sir, done.", "", 20))
        assertEquals(VoiceEffect.Say("Sir, done."), fx2.last())
    }

    @Test fun kidModeAlwaysTalks() {
        val (_, fx) = from(VoiceState(voiceOff = true, lastTyped = true, kid = true), *listening(), VoiceEvent.Report("Well done!", "", 20))
        assertEquals(VoiceEffect.Say("Well done!"), fx.last())
    }
}
