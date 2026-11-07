package mv.obitech.crundi.ui.operations

import mv.obitech.crundi.api.BackupRunning
import mv.obitech.crundi.api.BackupSchedule
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import java.time.ZoneId
import java.time.ZonedDateTime

/** The Backup page's words and checks. */
class BackupRulesTest {
    @Test fun whatIsHappening() {
        assertEquals("Uploading… 50%", BackupRules.phase(BackupRunning("uploading", done = 512.0, total = 1024.0)))
        assertEquals("Gathering files…", BackupRules.phase(BackupRunning("collecting")))
        assertEquals("Working…", BackupRules.phase(BackupRunning("something-new")))
        assertEquals(0.5f, BackupRules.progress(BackupRunning("uploading", done = 512.0, total = 1024.0))!!, 0.001f)
        assertNull("unpacking cannot be measured", BackupRules.progress(BackupRunning("unpacking", done = 1.0, total = 2.0)))
    }

    @Test fun whenInWords() {
        val z = ZoneId.of("Indian/Maldives")
        val now = ZonedDateTime.of(2026, 11, 6, 12, 0, 0, 0, z).toInstant().toEpochMilli()
        val at = { d: Int, h: Int -> ZonedDateTime.of(2026, 11, d, h, 0, 0, 0, z).toInstant().toEpochMilli() }
        assertEquals("today at 03:00", BackupRules.whenText(at(6, 3), now, z))
        assertEquals("tomorrow at 03:00", BackupRules.whenText(at(7, 3), now, z))
        assertEquals("yesterday at 15:00", BackupRules.whenText(at(5, 15), now, z))
        assertEquals("Mon 9 Nov at 03:00", BackupRules.whenText(at(9, 3), now, z))
    }

    @Test fun theScheduleInAFewWords() {
        assertEquals("Off", BackupRules.scheduleSummary(BackupSchedule(enabled = false)))
        assertEquals("Daily at 03:00", BackupRules.scheduleSummary(BackupSchedule(enabled = true)))
        assertEquals("Fridays at 04:30", BackupRules.scheduleSummary(BackupSchedule(true, "weekly", "04:30", 5)))
        assertEquals("Every 6 h", BackupRules.scheduleSummary(BackupSchedule(true, "hours", everyHours = 6)))
    }

    @Test fun theFormIsCheckedBeforeItIsSent() {
        assertNull(BackupRules.formError("", "", "03:00", "12", "14"))
        assertEquals("The two passphrases differ", BackupRules.formError("abcdefgh", "abcdefgx", "03:00", "12", "14"))
        assertEquals("Time must be HH:MM", BackupRules.formError("", "", "3am", "12", "14"))
        assertEquals("Every 1 to 168 hours", BackupRules.formError("", "", "03:00", "0", "14"))
        assertEquals("Keep 0 to 1000 backups", BackupRules.formError("", "", "03:00", "12", ""))
    }
}
