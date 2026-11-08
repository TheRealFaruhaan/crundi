package mv.obitech.crundi.voice

import mv.obitech.crundi.api.EnrolReply
import mv.obitech.crundi.api.SpeakerStatus
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Enrolling the owner's voice (Enrolment): phrase by phrase, retries, finishing. */
class EnrolmentTest {
    private val st = SpeakerStatus(ok = true, available = true, minPhrases = 2, prompts = listOf("one", "two", "three"))

    @Test fun anAcceptedPhraseMovesOnARefusedOneIsReadAgain() {
        var u = Enrolment.start(SpeakerUi(status = st, msg = "old"))
        assertTrue(u.active)
        assertEquals("", u.msg)
        assertTrue(Enrolment.restart(u))
        u = Enrolment.after(u, EnrolReply(ok = true, accepted = false, seconds = 0.2, problem = "Nothing was heard."))
        assertEquals(0, u.pos)
        assertTrue(u.bad)
        assertEquals("Nothing was heard.", u.msg)
        // A refused first phrase still means the next one starts afresh.
        assertTrue(Enrolment.restart(u))
        u = Enrolment.after(u, EnrolReply(ok = true, accepted = true, seconds = 2.4, phrases = 1))
        assertEquals(1, u.pos)
        assertFalse(Enrolment.restart(u))
        assertFalse(Enrolment.canFinish(u))
        u = Enrolment.after(u, EnrolReply(ok = true, accepted = true, seconds = 2.0, consistency = 0.91, phrases = 2))
        assertTrue(Enrolment.canFinish(u))
        assertEquals(2, Enrolment.kept(u))
        assertEquals(3, u.results.size)
    }

    @Test fun aFailedCheckSaysWhyAndKeepsThePhrase() {
        val u = Enrolment.after(Enrolment.start(SpeakerUi(status = st)).copy(busy = true), EnrolReply(ok = false, error = "The voice check is not available"))
        assertEquals(0, u.pos)
        assertFalse(u.busy)
        assertTrue(u.results.isEmpty())
        assertEquals("The voice check is not available", u.msg)
    }
}
