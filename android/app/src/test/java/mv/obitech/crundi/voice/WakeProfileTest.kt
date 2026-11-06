package mv.obitech.crundi.voice

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.double
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.exp
import kotlin.math.roundToInt
import kotlin.math.sin

/**
 * "Hey Cru" training on the phone computes what the PC's enroll.js does:
 * the numbers in wake-fixture.json come from there (scripts/wake-fixture.mjs).
 */
class WakeProfileTest {
    private val fx: JsonObject by lazy {
        val text = javaClass.classLoader!!.getResource("wake-fixture.json")!!.readText()
        Json.parseToJsonElement(text) as JsonObject
    }

    /** The same made-up utterance as wake-fixture.mjs's signal(). */
    private fun signal(seed: Int, pitch: Double = 140.0, stretch: Double = 1.0): FloatArray {
        var x = seed
        fun rnd(): Double { x = x * 1664525 + 1013904223; return (x.toLong() and 0xFFFFFFFFL) / 4294967296.0 - 0.5 }
        val n = (16000 * 1.6 * stretch).roundToInt()
        return FloatArray(n) { i ->
            val t = i / 16000.0; val u = t / stretch
            val a = (u - 0.45) / 0.12; val b = (u - 0.95) / 0.16
            val env = exp(-(a * a)) + 0.9 * exp(-(b * b))
            val f0 = pitch * (1 + 0.08 * sin(2 * PI * 1.5 * u))
            var v = 0.0
            for (h in 1..8) v += sin(2 * PI * f0 * h * t) / h * (if (u < 0.7) (if (h == 2 || h == 5) 1.6 else 1.0) else (if (h == 3 || h == 7) 1.6 else 1.0))
            (0.12 * env * v + 0.003 * rnd()).toFloat()
        }
    }

    private fun near(want: Double, got: Double, tol: Double, what: String) =
        assertTrue("$what: want $want, got $got", abs(want - got) <= tol)

    @Test fun featuresMatchThePc() {
        val a = signal(1)
        val f = CruWake.mfcc(a)
        assertEquals(fx["frames"]!!.jsonPrimitive.int, f.size)
        val first = fx["mfccFirst"]!!.jsonArray.map { it.jsonPrimitive.double }
        val mid = fx["mfccMiddle"]!!.jsonArray.map { it.jsonPrimitive.double }
        first.forEachIndexed { j, v -> near(v, f[10][j], 1e-3, "frame 10, coefficient $j") }
        mid.forEachIndexed { j, v -> near(v, f[f.size / 2][j], 1e-3, "middle frame, coefficient $j") }
        assertEquals(fx["trimmed"]!!.jsonPrimitive.int, CruWake.trim(a)?.size ?: -1)
    }

    @Test fun distancesMatchThePc() {
        val fa = CruWake.mfcc(signal(1))
        near(fx["distanceAB"]!!.jsonPrimitive.double, CruWake.distance(fa, CruWake.mfcc(signal(2, 150.0, 1.1))), 1e-3, "A to B")
        near(fx["distanceAC"]!!.jsonPrimitive.double, CruWake.distance(fa, CruWake.mfcc(signal(3, 220.0, 0.8))), 1e-3, "A to C")
        assertEquals(0.0, CruWake.distance(fa, fa), 1e-9)
    }

    @Test fun trainingTunesAndKeepsTheVoice() {
        // Five takes of one "voice", a spotter that hears them up to threshold 0.25.
        val takes = (1..5).map { signal(10 + it, 140.0 + it, 0.95 + it * 0.02) }
        val r = CruWake.train(takes, "2026-11-06T06:00:00Z") { line, _ -> line.substringAfter('#').substringBefore(' ').toDouble() <= 0.25 }
        assertTrue(r is CruWake.Trained.Ok)
        val p = (r as CruWake.Trained.Ok).profile
        assertEquals(0.15, p.keywordsThreshold, 1e-9) // two steps looser than the strictest that heard all five
        assertTrue(p.valid() && p.templates.size == 15 && p.takes == 5 && p.keywordLine().contains("#0.15"))
        assertTrue("the owner's own take wakes", p.verify(signal(30, 143.0, 1.0)).ok)
        assertFalse("a very different voice does not", p.verify(signal(31, 260.0, 0.7)).ok)
    }

    @Test fun onlyEmptyTakesAreAskedAgainNeverOnesTheModelMissed() {
        val takes = (1..5).map { signal(10 + it, 140.0 + it, 0.95 + it * 0.02) }
        val silent = takes.toMutableList().also { it[2] = FloatArray(16000) }
        val r1 = CruWake.train(silent, "") { _, _ -> true }
        assertTrue(r1 is CruWake.Trained.Redo && (r1 as CruWake.Trained.Redo).which == listOf(2))
        // A voice the model never hears still trains: the owner's phrase wakes for it.
        val r2 = CruWake.train(takes, "") { _, _ -> false }
        assertTrue(r2 is CruWake.Trained.Ok)
        val p = (r2 as CruWake.Trained.Ok).profile
        assertEquals(0, p.modelHeard)
        assertTrue("the owner's take, on its own, wakes", p.matchPhrase(signal(30, 143.0, 1.0)).ok)
        assertFalse("a very different voice does not", p.matchPhrase(signal(31, 260.0, 0.7)).ok)
    }

    @Test fun burstsOfSpeechAreFoundWithAPauseEitherSide() {
        val seg = CruWake.Segmenter()
        val quiet = FloatArray(1600) { ((it * 7919) % 13 - 6) * 0.0002f }
        val word = signal(5)
        val found = ArrayList<FloatArray>()
        fun feed(a: FloatArray) { var i = 0; while (i < a.size) { seg.accept(a.copyOfRange(i, minOf(a.size, i + 1600)))?.let { found += it }; i += 1600 } }
        repeat(10) { feed(quiet) }
        feed(word)
        repeat(6) { feed(quiet) }
        assertEquals(1, found.size)
        assertTrue(CruWake.checkSample(found[0]) is CruWake.Sample.Ok)
        // Talk that runs on (over 2.5 s) is not a call.
        feed(FloatArray(16000 * 4) { (sin(it * 0.07) * 0.2).toFloat() })
        repeat(6) { feed(quiet) }
        assertEquals(1, found.size)
    }
}
