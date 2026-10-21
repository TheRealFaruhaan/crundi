package mv.obitech.crundi.ui.operations

import mv.obitech.crundi.api.model.CollabInvite
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** The Info tab's helpers, held to the web's collabTimeLeft, fmtTok and parseTok. */
class InfoLogicTest {
    private val now = 1_792_322_400_000L

    @Test fun timeLeftLikeTheWeb() {
        assertEquals("expired", CollabFmt.timeLeft(now - 1, now))
        assertEquals("3 days left", CollabFmt.timeLeft(now + 3 * 86_400_000L + 5_000, now))
        assertEquals("1 day left", CollabFmt.timeLeft(now + 86_400_000L, now))
        assertEquals("5 hours left", CollabFmt.timeLeft(now + 5 * 3_600_000L + 60_000, now))
        assertEquals("1 hour left", CollabFmt.timeLeft(now + 3_600_000L, now))
        // Never rounds four minutes up to an hour.
        assertEquals("4 minutes left", CollabFmt.timeLeft(now + 4 * 60_000L, now))
        assertEquals("1 minute left", CollabFmt.timeLeft(now + 5_000, now))
    }

    @Test fun tokensLikeTheWeb() {
        assertEquals("2M", CollabFmt.tokens(2_000_000))
        assertEquals("2.4M", CollabFmt.tokens(2_412_000))
        assertEquals("412k", CollabFmt.tokens(412_000))
        assertEquals("900", CollabFmt.tokens(900))
    }

    @Test fun parseTokensLikeTheWeb() {
        assertEquals(2_000_000L, CollabFmt.parseTokens("2M"))
        assertEquals(500_000L, CollabFmt.parseTokens(" 500k "))
        assertEquals(1_500_000L, CollabFmt.parseTokens("1,500,000"))
        assertEquals(1_500_000L, CollabFmt.parseTokens("1.5m"))
        assertNull(CollabFmt.parseTokens("lots"))
        assertNull(CollabFmt.parseTokens(""))
    }

    @Test fun peopleGroupByTelegramElseName() {
        val list = listOf(
            CollabInvite("1", "Aisha", "crundi", telegram = "aisha_m"),
            CollabInvite("2", "aisha m", "ledger", telegram = "aisha_m"),
            CollabInvite("3", "Sam", "crundi"),
            CollabInvite("4", "sam", "ledger"),
        )
        val people = CollabFmt.people(list)
        assertEquals(listOf(listOf("1", "2"), listOf("3", "4")), people.map { p -> p.map { it.id } })
    }
}
