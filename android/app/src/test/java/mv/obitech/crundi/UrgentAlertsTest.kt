package mv.obitech.crundi

import mv.obitech.crundi.UrgentAlerts.Outcome
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * How an alert from alert_user is shown: which channel, whether it rings
 * until seen, and what the notification log says. The payloads are the shapes
 * src/mobile-push.js makes (scripts/test-alert-user.mjs checks that side).
 */
class UrgentAlertsTest {
    private val quiet = Notifier.SERVER

    private fun payload(urgent: Boolean? = null, downgraded: Boolean? = null) = JSONObject().apply {
        put("v", 1); put("kind", "alert"); put("title", "Outage"); put("body", "The site is down"); put("url", "/")
        if (urgent != null) put("urgent", urgent)
        if (downgraded != null) put("downgraded", downgraded)
    }

    @Test fun urgentRingsOnTheUrgentChannel() {
        val d = UrgentAlerts.decide(payload(urgent = true), serverOn = true, dndOptIn = false, dndGranted = false, quietChannel = quiet)
        assertEquals(UrgentAlerts.CHANNEL, d.channel)
        assertTrue(d.insistent)
        assertEquals(Outcome.RANG, d.outcome)
    }

    @Test fun normalIsShownQuietly() {
        val d = UrgentAlerts.decide(payload(), serverOn = true, dndOptIn = true, dndGranted = true, quietChannel = quiet)
        assertEquals(quiet, d.channel)
        assertFalse(d.insistent)
        assertEquals(Outcome.NORMAL, d.outcome)
    }

    @Test fun urgentFalseIsNormal() {
        val d = UrgentAlerts.decide(payload(urgent = false), serverOn = true, dndOptIn = false, dndGranted = false, quietChannel = quiet)
        assertFalse(d.rings)
        assertEquals(Outcome.NORMAL, d.outcome)
    }

    @Test fun serverSwitchOffMakesItQuiet() {
        val d = UrgentAlerts.decide(payload(urgent = true), serverOn = false, dndOptIn = true, dndGranted = true, quietChannel = quiet)
        assertEquals(quiet, d.channel)
        assertFalse(d.insistent)
        assertEquals(Outcome.QUIET_SERVER_OFF, d.outcome)
    }

    @Test fun downgradedNeverRings() {
        val d = UrgentAlerts.decide(payload(downgraded = true), serverOn = true, dndOptIn = true, dndGranted = true, quietChannel = quiet)
        assertEquals(quiet, d.channel)
        assertFalse(d.insistent)
        assertEquals(Outcome.DOWNGRADED, d.outcome)
        // Even if a payload said both, the server's word that it is over the limit wins.
        val both = UrgentAlerts.decide(payload(urgent = true, downgraded = true), serverOn = true, dndOptIn = false, dndGranted = false, quietChannel = quiet)
        assertFalse(both.rings)
        assertEquals(Outcome.DOWNGRADED, both.outcome)
    }

    @Test fun throughDndOnlyWhenOptedInAndGranted() {
        val both = UrgentAlerts.decide(payload(urgent = true), serverOn = true, dndOptIn = true, dndGranted = true, quietChannel = quiet)
        assertEquals(UrgentAlerts.CHANNEL_DND, both.channel)
        assertTrue(both.insistent)
        assertEquals(Outcome.RANG_THROUGH_DND, both.outcome)

        val notGranted = UrgentAlerts.decide(payload(urgent = true), serverOn = true, dndOptIn = true, dndGranted = false, quietChannel = quiet)
        assertEquals(UrgentAlerts.CHANNEL, notGranted.channel)
        assertEquals(Outcome.RANG, notGranted.outcome)

        val notOptedIn = UrgentAlerts.decide(payload(urgent = true), serverOn = true, dndOptIn = false, dndGranted = true, quietChannel = quiet)
        assertEquals(UrgentAlerts.CHANNEL, notOptedIn.channel)
        assertEquals(Outcome.RANG, notOptedIn.outcome)
    }

    @Test fun levelForTheLog() {
        assertEquals("urgent", UrgentAlerts.levelOf(urgent = true, downgraded = false))
        assertEquals("urgent", UrgentAlerts.levelOf(urgent = false, downgraded = true))
        assertEquals("normal", UrgentAlerts.levelOf(urgent = false, downgraded = false))
    }

    @Test fun logLinesSayTheOutcomeAndNothingElse() {
        assertEquals("rang", Outcome.RANG.log)
        assertTrue(Outcome.QUIET_SERVER_OFF.log.contains("urgent alerts are off for this server"))
        assertTrue(Outcome.DOWNGRADED.log.contains("downgraded by the server"))
    }

    @Test fun theTwoChannelsAreDistinctFromTheOthers() {
        assertFalse(UrgentAlerts.CHANNEL == UrgentAlerts.CHANNEL_DND)
        for (c in listOf(Notifier.CHATS, Notifier.SERVER, Notifier.REQUESTS)) {
            assertFalse(c == UrgentAlerts.CHANNEL)
            assertFalse(c == UrgentAlerts.CHANNEL_DND)
        }
    }

    @Test fun otherNotificationsStaySilentWhileAnAlertRings() {
        val insistent = android.app.Notification.FLAG_INSISTENT
        assertTrue(UrgentAlerts.isUrgentChannel(UrgentAlerts.CHANNEL) && UrgentAlerts.isUrgentChannel(UrgentAlerts.CHANNEL_DND))
        assertFalse(UrgentAlerts.isUrgentChannel("server") || UrgentAlerts.isUrgentChannel(null))
        assertTrue(UrgentAlerts.ringing(listOf(UrgentAlerts.Showing("chats", 0, 1000), UrgentAlerts.Showing(UrgentAlerts.CHANNEL, insistent, 5_000))))
        assertTrue(UrgentAlerts.ringing(listOf(UrgentAlerts.Showing(UrgentAlerts.CHANNEL_DND, insistent, 0))))
        // Not ringing: a quiet alert, nothing urgent showing, or one left in the shade too long.
        assertFalse(UrgentAlerts.ringing(listOf(UrgentAlerts.Showing("server", insistent, 1000))))
        assertFalse(UrgentAlerts.ringing(listOf(UrgentAlerts.Showing(UrgentAlerts.CHANNEL, 0, 1000))))
        assertFalse(UrgentAlerts.ringing(emptyList()))
        assertFalse(UrgentAlerts.ringing(listOf(UrgentAlerts.Showing(UrgentAlerts.CHANNEL, insistent, UrgentAlerts.HUSH_MS))))
    }

    @Test fun fullScreenOnlyWhenItRingsAndIsAllowed() {
        val ring = UrgentAlerts.decide(urgent = true, downgraded = false, serverOn = true, dndOptIn = false, dndGranted = false, quietChannel = "server", fullScreenReady = true)
        assertTrue(ring.rings && ring.fullScreen)
        assertFalse(UrgentAlerts.decide(true, false, true, false, false, "server").fullScreen)
        // Never for an alert that does not ring.
        assertFalse(UrgentAlerts.decide(urgent = false, downgraded = false, serverOn = true, dndOptIn = false, dndGranted = false, quietChannel = "server", fullScreenReady = true).fullScreen)
        assertFalse(UrgentAlerts.decide(urgent = true, downgraded = true, serverOn = true, dndOptIn = false, dndGranted = false, quietChannel = "server", fullScreenReady = true).fullScreen)
        assertFalse(UrgentAlerts.decide(urgent = true, downgraded = false, serverOn = false, dndOptIn = false, dndGranted = false, quietChannel = "server", fullScreenReady = true).fullScreen)
        // With Do Not Disturb too: the bypassing channel, and full screen.
        val both = UrgentAlerts.decide(urgent = true, downgraded = false, serverOn = true, dndOptIn = true, dndGranted = true, quietChannel = "server", fullScreenReady = true)
        assertEquals(UrgentAlerts.CHANNEL_DND, both.channel); assertTrue(both.fullScreen)
    }

    @Test fun theSliderCountsOnlyNearTheEnd() {
        assertEquals(1, AlertSlide.outcome(1f)); assertEquals(1, AlertSlide.outcome(AlertSlide.COMMIT))
        assertEquals(-1, AlertSlide.outcome(-1f)); assertEquals(-1, AlertSlide.outcome(-AlertSlide.COMMIT))
        assertEquals(0, AlertSlide.outcome(0f)); assertEquals(0, AlertSlide.outcome(0.5f)); assertEquals(0, AlertSlide.outcome(-0.69f))
    }

    @Test fun setupVisitsOnlyThePagesStillNeeded() {
        assertEquals(listOf(UrgentSetup.FULL, UrgentSetup.DND), UrgentSetup.steps(full = true, fullGranted = false, dnd = true, dndGranted = false))
        assertEquals(listOf(UrgentSetup.DND), UrgentSetup.steps(full = true, fullGranted = true, dnd = true, dndGranted = false))
        assertEquals(emptyList<String>(), UrgentSetup.steps(full = false, fullGranted = false, dnd = false, dndGranted = false))
        assertEquals(listOf(UrgentSetup.FULL), UrgentSetup.steps(full = true, fullGranted = false, dnd = false, dndGranted = false))
    }

    @Test fun eachAlertIsANewNotification() {
        // An update of an alert still in the shade buzzes but never opens full screen.
        assertTrue(UrgentAlerts.alertId("s1", "c1", 1000L) != UrgentAlerts.alertId("s1", "c1", 2000L))
        assertEquals(UrgentAlerts.alertId("s1", "c1", 1000L), UrgentAlerts.alertId("s1", "c1", 1000L))
    }
}
