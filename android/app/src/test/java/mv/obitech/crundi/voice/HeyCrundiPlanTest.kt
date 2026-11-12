package mv.obitech.crundi.voice

import mv.obitech.crundi.voice.HeyCrundiPlan.Command
import mv.obitech.crundi.voice.HeyCrundiPlan.Listener
import mv.obitech.crundi.voice.HeyCrundiPlan.Restart
import mv.obitech.crundi.voice.HeyCrundiPlan.Situation
import mv.obitech.crundi.voice.HeyCrundiPlan.Wake
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.Calendar
import java.util.TimeZone

/** The rules of "Hey Crundi" (D3–D7), with no Android. */
class HeyCrundiPlanTest {
    private val min = 60_000L

    @Test fun theChoicesAreUntilOffThenD6sDurationsWithThirtyTheDefault() {
        assertEquals(listOf(0, 15, 30, 45, 60, 120), HeyCrundiPlan.CHOICES)
        assertEquals(30, HeyCrundiPlan.DEFAULT_MINUTES)
        assertEquals(listOf("Until I turn it off", "15 min", "30 min", "45 min", "1 hour", "2 hours"), HeyCrundiPlan.CHOICES.map(HeyCrundiPlan::label))
        assertEquals(30, HeyCrundiPlan.choice(7))
        assertEquals(0, HeyCrundiPlan.choice(0))
        assertEquals(120, HeyCrundiPlan.choice(120))
    }

    @Test fun theVoiceToolAlwaysGetsATimeLimitAtMostTwoHours() {
        assertEquals(0, HeyCrundiPlan.voiceMinutes(0))
        assertEquals(0, HeyCrundiPlan.voiceMinutes(-5))
        assertEquals(15, HeyCrundiPlan.voiceMinutes(1))
        assertEquals(30, HeyCrundiPlan.voiceMinutes(30))
        assertEquals(60, HeyCrundiPlan.voiceMinutes(55))
        assertEquals(120, HeyCrundiPlan.voiceMinutes(90)) // a tie goes to the longer
        assertEquals(120, HeyCrundiPlan.voiceMinutes(600))
        assertEquals(120, HeyCrundiPlan.voiceMinutes(Int.MAX_VALUE))
    }

    @Test fun phoneActionsBecomeCommands() {
        assertEquals(Command.Start(60), HeyCrundiPlan.command("background-listening", 60))
        assertEquals(Command.Start(120), HeyCrundiPlan.command("background-listening", 1000))
        assertEquals(Command.Stop, HeyCrundiPlan.command("background-listening", 0))
        assertNull(HeyCrundiPlan.command("something-new", 30))
    }

    @Test fun cruOffOrderSilencesEveryListener() {
        // 6 Nov: Cru's "off" stopped the service, but the asleep voice screen and in-app listening went on.
        val base = HeyCrundiPlan.Situation(trained = true, appInFront = true, inAppOn = true)
        assertEquals(HeyCrundiPlan.Listener.InApp, HeyCrundiPlan.listener(base))
        assertEquals(HeyCrundiPlan.Listener.Screen, HeyCrundiPlan.listener(base.copy(screenWaiting = true)))
        assertEquals(HeyCrundiPlan.Listener.None, HeyCrundiPlan.listener(base.copy(heldOff = true)))
        assertEquals(HeyCrundiPlan.Listener.None, HeyCrundiPlan.listener(base.copy(heldOff = true, screenWaiting = true)))
        assertEquals(HeyCrundiPlan.Listener.None, HeyCrundiPlan.listener(base.copy(heldOff = true, backgroundOn = true)))
        assertEquals(HeyCrundiPlan.Command.Stop, HeyCrundiPlan.command("background-listening", 0))
    }

    @Test fun theDeadlineAndTimeLeft() {
        val now = 1_000_000L
        assertNull(HeyCrundiPlan.deadline(now, 0))
        val d = HeyCrundiPlan.deadline(now, 30)!!
        assertEquals(now + 30 * min, d)
        assertEquals("30 min left", HeyCrundiPlan.timeLeft(now, d))
        assertEquals("30 min left", HeyCrundiPlan.timeLeft(now + 1, d)) // rounded up
        assertEquals("1 min left", HeyCrundiPlan.timeLeft(d - min, d))
        assertEquals("Less than a minute left", HeyCrundiPlan.timeLeft(d - 59_000, d))
        assertEquals("Time is up", HeyCrundiPlan.timeLeft(d, d))
        assertEquals("2 h left", HeyCrundiPlan.timeLeft(now, now + 120 * min))
        assertEquals("1 h 5 min left", HeyCrundiPlan.timeLeft(now, now + 65 * min))
        assertEquals("Until you turn it off", HeyCrundiPlan.timeLeft(now, null))
        assertFalse(HeyCrundiPlan.expired(d - 1, d))
        assertTrue(HeyCrundiPlan.expired(d, d))
        assertFalse(HeyCrundiPlan.expired(Long.MAX_VALUE, null))
    }

    @Test fun untilIsTheClockTime() {
        val cal = Calendar.getInstance(TimeZone.getTimeZone("UTC"))
        assertEquals("until 14:30", HeyCrundiPlan.until(Date.utc(14, 30), cal))
        assertEquals("until 09:05", HeyCrundiPlan.until(Date.utc(9, 5), cal))
    }

    private object Date {
        fun utc(h: Int, m: Int) = (h * 60L + m) * 60_000L
    }

    @Test fun theNotificationRefreshesOnTheMinute() {
        assertNull(HeyCrundiPlan.nextRefresh(0, null))
        assertEquals(60_000L, HeyCrundiPlan.nextRefresh(0, 30 * min))
        assertEquals(10_000L, HeyCrundiPlan.nextRefresh(0, 29 * min + 10_000))
        assertEquals(0L, HeyCrundiPlan.nextRefresh(10, 5))
    }

    @Test fun oneMicrophoneOwnerAtATime() {
        // Untrained, nothing ever listens: "Hey Cru" is set up from the owner's voice first.
        assertEquals(Listener.None, HeyCrundiPlan.listener(Situation(backgroundOn = true, inAppOn = true, appInFront = true, screenWaiting = true)))
        assertEquals(Listener.None, HeyCrundiPlan.listener(Situation(trained = true, backgroundOn = true, training = true)))
        val none = Situation(trained = true)
        assertEquals(Listener.None, HeyCrundiPlan.listener(none))
        assertEquals(Listener.None, HeyCrundiPlan.listener(none.copy(inAppOn = true))) // app not in front
        assertEquals(Listener.InApp, HeyCrundiPlan.listener(none.copy(inAppOn = true, appInFront = true)))
        assertEquals(Listener.None, HeyCrundiPlan.listener(none.copy(appInFront = true))) // toggle off
        assertEquals(Listener.Background, HeyCrundiPlan.listener(none.copy(backgroundOn = true)))
        assertEquals(Listener.Background, HeyCrundiPlan.listener(none.copy(backgroundOn = true, inAppOn = true, appInFront = true)))
        // A voice session has the microphone: nobody else listens.
        assertEquals(Listener.None, HeyCrundiPlan.listener(none.copy(backgroundOn = true, sessionOpen = true)))
        assertEquals(Listener.None, HeyCrundiPlan.listener(none.copy(inAppOn = true, appInFront = true, sessionOpen = true)))
        assertEquals(Listener.None, HeyCrundiPlan.listener(none.copy(backgroundOn = true, handingOver = true)))
        assertEquals(Listener.None, HeyCrundiPlan.listener(none.copy(backgroundOn = true, micAllowed = false)))
        // The voice screen asleep listens with both settings off, and is woken in place.
        assertEquals(Listener.Screen, HeyCrundiPlan.listener(none.copy(screenWaiting = true, appInFront = true)))
        assertEquals(Listener.None, HeyCrundiPlan.listener(none.copy(screenWaiting = true, sessionOpen = true)))
        assertEquals(HeyCrundiPlan.Wake.WakeScreen, HeyCrundiPlan.onHeard(none.copy(screenWaiting = true, appInFront = true, backgroundOn = true)))
        assertEquals(Listener.None, HeyCrundiPlan.listener(none.copy(backgroundOn = true, engineReady = false)))
    }

    @Test fun heardInFrontOpensTheScreenOtherwiseAsksAndroid() {
        assertEquals(Wake.OpenScreen, HeyCrundiPlan.onHeard(Situation(appInFront = true, inAppOn = true)))
        assertEquals(Wake.FromBackground, HeyCrundiPlan.onHeard(Situation(backgroundOn = true)))
    }

    @Test fun aRestartAfterAKillTellsMasterOnlyIfItWasStillInItsTime() {
        assertEquals(Restart.NotifyStopped, HeyCrundiPlan.afterRestart(true, 10, 20 * min))
        assertEquals(Restart.NotifyStopped, HeyCrundiPlan.afterRestart(true, 10, null)) // until turned off
        assertEquals(Restart.Clear, HeyCrundiPlan.afterRestart(true, 30 * min, 20 * min))
        assertEquals(Restart.Clear, HeyCrundiPlan.afterRestart(false, 10, null))
    }

    @Test fun resumingKeepsWhatIsLeftOrStartsTheChoiceAgain() {
        assertEquals(0, HeyCrundiPlan.resumeMinutes(0, null, 30))
        assertEquals(20, HeyCrundiPlan.resumeMinutes(0, 20 * min, 30))
        assertEquals(45, HeyCrundiPlan.resumeMinutes(30 * min, 20 * min, 45))
        assertEquals(30, HeyCrundiPlan.resumeMinutes(30 * min, 20 * min, 0)) // ran out: never unlimited by itself
    }
}
