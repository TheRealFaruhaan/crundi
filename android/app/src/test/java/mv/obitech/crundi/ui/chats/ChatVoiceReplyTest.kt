package mv.obitech.crundi.ui.chats

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.sin

/** Talking over Claude's spoken reply, and a reply waiting behind the owner. */
class ChatVoiceReplyTest {
    private val t0 = 1_000_000L

    @Test fun aReplyPlaysAtOnceWhenTheOwnerIsQuiet() {
        val q = ReplyQueue()
        q.add("Done, Sir.")
        assertEquals("Done, Sir.", q.take(ownerBusy = false, hasWords = false, now = t0))
        // One at a time: nothing more until it has been said.
        q.add("And the tests pass, all forty of them.")
        assertNull(q.take(false, false, t0))
        q.done()
        assertEquals("And the tests pass, all forty of them.", q.take(false, false, t0))
        q.done()
        assertTrue(q.isEmpty)
    }

    @Test fun aReplyWaitsBehindAHold() {
        val q = ReplyQueue()
        q.add("Done, Sir.")
        assertNull(q.take(ownerBusy = true, hasWords = false, now = t0))
        assertNull(q.take(true, false, t0 + 10_000))
        // Let go and sent: it goes.
        q.ownerDone()
        assertEquals("Done, Sir.", q.take(false, false, t0 + 10_100))
    }

    @Test fun aReplyWaitsWhileWordsComeThenUntilTheyAreSentOrQuiet() {
        val q = ReplyQueue()
        q.words(t0)
        q.add("First reply here, a long enough sentence.")
        q.add("Second reply here, also long enough to stand.")
        assertNull(q.take(false, hasWords = true, now = t0 + 500))
        q.words(t0 + 1_000)
        assertNull(q.take(false, true, t0 + 2_400))           // new words 1.4 s ago
        // 1.5 s without new words: it goes, in order.
        assertEquals("First reply here, a long enough sentence.", q.take(false, true, t0 + 2_500))
        q.done()
        // His utterance sent (the words cleared) also lets it go at once.
        q.words(t0 + 2_600)
        assertNull(q.take(false, true, t0 + 2_700))
        q.ownerDone()
        assertEquals("Second reply here, also long enough to stand.", q.take(false, false, t0 + 2_800))
    }

    @Test fun talkedOverItPausesAndGoesOnFromTheSentenceItWasOn() {
        val q = ReplyQueue()
        q.add("The first sentence of the reply is this one. The second sentence of the reply follows it.")
        assertEquals("The first sentence of the reply is this one.", q.take(false, false, t0))
        q.pause(t0 + 400)
        assertTrue(q.paused)
        assertNull(q.playing)
        // Paused: not again until he has finished.
        assertNull(q.take(false, false, t0 + 1_000))
        q.words(t0 + 1_200)
        assertNull(q.take(false, true, t0 + 2_000))
        q.ownerDone()
        // From the start of the sentence it was on, then the rest.
        assertEquals("The first sentence of the reply is this one.", q.take(false, false, t0 + 2_100))
        assertFalse(q.paused)
        q.done()
        assertEquals("The second sentence of the reply follows it.", q.take(false, false, t0 + 2_200))
    }

    @Test fun aTapThatPausesWithNothingSaidGoesOnAfterTheQuiet() {
        val q = ReplyQueue()
        q.add("Something long enough to be its own sentence.")
        q.take(false, false, t0)
        q.pause(t0 + 100)
        assertNull(q.take(false, false, t0 + 1_500))
        assertNotNull(q.take(false, false, t0 + 1_600))
    }

    @Test fun cancelStopsItForGood() {
        val q = ReplyQueue()
        q.add("One sentence that is long enough. Another one also long enough.")
        q.take(false, false, t0)
        q.clear()
        assertTrue(q.isEmpty)
        assertNull(q.take(false, false, t0 + 60_000))
    }

    // ─── the gate ───

    private val chunk = ByteArray(3200)
    private fun barge() = ReplyBargeIn().also { b -> repeat(50) { b.idle(0.01f) }; b.begin() }
    private val cru = { i: Int -> if (i % 5 == 0) 0.15f else 0.55f + 0.1f * sin(i.toFloat()) }

    @Test fun claudesOwnVoiceNeverCutsIn() {
        val b = barge()
        for (i in 0 until 80) { val p = cru(i); assertNull("at $i", b.hear(chunk, p * 0.33f, p, 100) { true }) }
    }

    @Test fun theOwnerOverItCutsInWithHisFirstWords() {
        val b = barge()
        for (i in 0 until 20) { val p = cru(i); b.hear(chunk, p * 0.33f, p, 100) { true } }
        var first: List<ByteArray>? = null
        var at = -1
        for (i in 0 until 10) { val r = b.hear(chunk, 0.75f, 0.55f, 100) { true }; if (r != null && first == null) { first = r; at = i } }
        assertTrue("at $at", at in 0..5)
        // What he said before the gate opened goes first: from about 600 ms before his voice rose, at most 1.3 s.
        assertTrue(first!!.size in 3..13)
        // Once only, until the next reply.
        assertNull(b.hear(chunk, 0.75f, 0.55f, 100) { true })
    }

    @Test fun aClapIsNotHim() {
        val b = barge()
        for (i in 0 until 20) { val p = cru(i); b.hear(chunk, p * 0.33f, p, 100) { true } }
        for (i in 0 until 10) assertNull(b.hear(chunk, 0.75f, 0.55f, 100) { false })
    }
}
