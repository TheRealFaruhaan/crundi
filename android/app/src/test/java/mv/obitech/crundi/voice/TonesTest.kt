package mv.obitech.crundi.voice

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.abs

/** Cru's tones (Tones): the web's notes and envelope, as PCM. */
class TonesTest {
    private val n = Tones.Note(880.0, 0.0, 0.08, 0.13)

    @Test fun theEnvelopeRisesToItsPeakThenFallsAway() {
        assertEquals(Tones.FLOOR, Tones.gain(n, 0.0), 1e-9)
        assertEquals(0.13, Tones.gain(n, 0.012), 1e-9)
        assertEquals(Tones.FLOOR, Tones.gain(n, 0.08), 1e-9)
        // Exponential: half way up is the geometric mean of the ends.
        assertEquals(kotlin.math.sqrt(Tones.FLOOR * 0.13), Tones.gain(n, 0.006), 1e-9)
        assertTrue(Tones.gain(n, 0.03) > Tones.gain(n, 0.06))
        assertEquals(0.0, Tones.gain(n, 0.2), 0.0)
        assertEquals(0.0, Tones.gain(n, -0.01), 0.0)
        // The think tone's slow rise.
        assertEquals(0.07, Tones.gain(Tones.THINK[0], 0.18), 1e-9)
    }

    @Test fun eachToneLastsItsLongestNotePlusTheTail() {
        assertEquals(((0.09 + 0.16 + Tones.TAIL) * 24_000).toInt(), Tones.length(Tones.LISTEN))
        assertEquals(((0.07 + 0.14 + Tones.TAIL) * 24_000).toInt(), Tones.length(Tones.SENT))
        assertEquals(((0.9 + Tones.TAIL) * 24_000).toInt(), Tones.pcm(Tones.THINK).size)
    }

    @Test fun theSoundIsSoftAndEndsInSilence() {
        for (notes in listOf(Tones.LISTEN, Tones.SENT, Tones.THINK)) {
            val pcm = Tones.pcm(notes)
            val peak = pcm.maxOf { abs(it.toInt()) }
            assertTrue(peak > 1000)
            // Never louder than the notes' peaks summed.
            assertTrue(peak <= (notes.sumOf { it.peak } * Short.MAX_VALUE).toInt() + 1)
            // The tail: each note held at the floor gain (0.0001 of full scale).
            assertTrue(pcm.takeLast(200).all { abs(it.toInt()) <= 4 * notes.size })
        }
        assertTrue(Tones.of(Tones.SENT) === Tones.of(Tones.SENT))
    }
}
