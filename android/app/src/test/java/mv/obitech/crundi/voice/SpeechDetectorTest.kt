package mv.obitech.crundi.voice

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.exp
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * Barge-in's speech detector: the same as Silero VAD itself, and the same as
 * the web client (scripts/test-voice-barge.mjs has these cases too): speech
 * cuts in, noise and music never do.
 */
class SpeechDetectorTest {
    /** The repository's root, from the module's folder (android/app) where unit tests run. */
    private val root = generateSequence(File("").absoluteFile) { it.parentFile }.first { File(it, "app/vendor/silero-vad-16k.bin").isFile }
    private val weights = SpeechDetector.parse(File(root, "app/vendor/silero-vad-16k.bin").readBytes())!!
    private val voice: FloatArray = File(root, "test/fixtures/vad-speech.s16").readBytes().let { b ->
        FloatArray(b.size / 2) { ((b[2 * it + 1].toInt() shl 8) or (b[2 * it].toInt() and 0xff)).toShort() / 32768f }
    }

    private var seed = 11L
    private fun rnd(): Float { seed = (seed * 16807) % 2147483647; return (seed / 2147483647.0 * 2 - 1).toFloat() }
    private fun clip(n: Int, f: (Int, Double) -> Float) = FloatArray(n) { f(it, it / 16000.0) }

    /** How far into [pcm] (ms) it is taken for someone talking, after a second of a quiet room; -1 never. */
    private fun talks(pcm: FloatArray): Int {
        val d = SpeechDetector(weights)
        d.push(clip(16000) { _, _ -> rnd() * 0.004f })
        var i = 0
        while (i < pcm.size) { val to = min(pcm.size, i + 1600); d.push(pcm, i, to); if (d.talking) return to / 16; i = to }
        return -1
    }

    @Test fun theWeightsFileReads() {
        assertEquals(4, weights.enc.size)
        assertNull(SpeechDetector.parse(ByteArray(16)))
    }

    @Test fun frameForFrameTheSameAsSileroItself() {
        val ref = File(root, "test/fixtures/vad-speech.probs.json").readText().trim().removePrefix("[").removeSuffix("]").split(",").map { it.trim().toFloat() }
        val d = SpeechDetector(weights)
        var worst = 0f
        var n = 0
        var i = 0
        while (i + 512 <= voice.size) { d.push(voice, i, i + 512); worst = maxOf(worst, abs(d.prob - ref[n])); n++; i += 512 }
        assertEquals(ref.size, n)
        assertTrue("worst $worst", worst < 0.01f)
    }

    @Test fun aSpokenSentenceCutsIn() {
        val at = talks(voice)
        assertTrue("at $at", at in 1..800)
    }

    @Test fun andStillDoesOverAFan() {
        var pk = 0f
        val pink = FloatArray(80000) { pk = pk * 0.97f + rnd() * 0.15f; pk }
        fun rms(a: FloatArray) = sqrt(a.fold(0.0) { t, x -> t + x * x } / a.size)
        val gain = (rms(voice.filter { abs(it) > 0.01f }.toFloatArray()) / rms(pink) / 10.0.pow(0.5)).toFloat()
        val at = talks(FloatArray(voice.size) { voice[it] + pink[it % pink.size] * gain })
        assertTrue("at $at", at in 1..1000)
    }

    @Test fun noiseAndMusicNeverCutIn() {
        var pk = 0f
        val sounds = mapOf(
            "white noise" to clip(80000) { _, _ -> rnd() * 0.3f },
            "a fan" to clip(80000) { _, _ -> pk = pk * 0.97f + rnd() * 0.15f; pk },
            "a loud hum starting" to clip(80000) { _, t -> (0.4 * (sin(2 * PI * 120 * t) + 0.4 * sin(2 * PI * 240 * t))).toFloat() },
            "a hum fading in" to clip(80000) { i, t -> (min(1.0, i / 4800.0) * 0.4 * (sin(2 * PI * 120 * t) + 0.4 * sin(2 * PI * 240 * t))).toFloat() },
            "music" to clip(80000) { i, t -> val n = listOf(220.0, 277.0, 330.0, 440.0)[(t * 2).toInt() % 4]; (min(1.0, i / 4800.0) * 0.2 * (sin(2 * PI * n * t) + sin(2 * PI * n * 1.5 * t) + 0.5 * sin(2 * PI * n * 2 * t))).toFloat() },
            "clicks" to clip(80000) { i, _ -> if (i % 8000 < 300) rnd() * 0.8f * exp(-(i % 8000) / 60f) else 0f },
            "a door slam" to clip(32000) { i, _ -> if (i > 8000) rnd() * 0.9f * exp(-(i - 8000) / 1600f) else 0f },
        )
        for ((name, pcm) in sounds) assertEquals(name, -1, talks(pcm))
        // Tones that start while it listens are where it is closest: over ten rooms, never once; the sentence always.
        for (r in 1..10) {
            seed = r * 7919L
            assertEquals("hum, room $r", -1, talks(sounds.getValue("a hum fading in")))
            seed = r * 7919L
            assertEquals("music, room $r", -1, talks(sounds.getValue("music")))
            seed = r * 7919L
            assertTrue("voice, room $r", talks(voice) > 0)
        }
    }

    @Test fun pcmBytesAsTheMicGivesThem() {
        val bytes = ByteArray(voice.size * 2)
        for (i in voice.indices) { val v = (voice[i] * 32768).toInt().coerceIn(-32768, 32767); bytes[2 * i] = v.toByte(); bytes[2 * i + 1] = (v shr 8).toByte() }
        val a = SpeechDetector(weights); val b = SpeechDetector(weights)
        a.push(bytes); b.push(voice)
        assertEquals(b.prob, a.prob, 0.001f)
        assertTrue(a.frames > 0)
    }

    @Test fun theCheckTrustsTheDetectorOnlyOnceWarm() {
        val d = SpeechDetector(weights)
        val check = SpeechCheck(d)
        assertFalse(check.notTalking())   // not warm: a level rule stands as before
        val quiet = ByteArray(3200)
        repeat(12) { check.push(quiet) }
        assertTrue(d.warm)
        assertFalse(check.talking())
        assertTrue(check.notTalking())
        val none = SpeechCheck(null)
        assertFalse(none.notTalking())
        assertNotNull(none.talking())
    }

    @Test fun resetStartsAfresh() {
        val d = SpeechDetector(weights)
        d.push(voice); d.reset()
        assertEquals(0f, d.prob)
        assertFalse(d.speech || d.talking)
    }
}
