package mv.obitech.crundi.voice

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.PI
import kotlin.math.sin

/** The phone deciding, from the microphone alone, whether the owner is talking over Cru. */
class OwnerGateTest {
    private val speech = { true }
    private val clap = { false }

    private fun gate() = OwnerGate().also { g -> repeat(50) { g.learn(0.01f) }; g.start() }

    /** The first chunk (100 ms each) at which the gate said "him", or -1. */
    private fun feed(g: OwnerGate, chunks: Int, mic: (Int, Float?) -> Float, play: (Int) -> Float?, voiced: () -> Boolean = speech): Int {
        var hit = -1
        for (i in 0 until chunks) { val p = play(i); if (g.hear(mic(i, p), p, 100, voiced) && hit < 0) hit = i }
        return hit
    }

    private val cru = { i: Int -> if (i % 5 == 0) 0.15f else 0.55f + 0.1f * sin(i.toFloat()) }

    @Test fun speakersCrusOwnVoiceNeverCounts() {
        val g = gate()
        assertEquals(-1, feed(g, 60, { _, p -> p!! * 0.33f }, cru))
    }

    @Test fun speakersTheOwnerOverItIsHeardWithinHalfASecond() {
        val g = gate()
        feed(g, 20, { _, p -> p!! * 0.33f }, cru)
        val hit = feed(g, 10, { _, _ -> 0.75f }, { 0.55f })
        assertTrue("at $hit", hit in 0..5)
    }

    @Test fun aClapIsNotHim() {
        val g = gate()
        feed(g, 20, { _, p -> p!! * 0.33f }, cru)
        assertEquals(-1, feed(g, 10, { _, _ -> 0.75f }, { 0.55f }, clap))
    }

    @Test fun headphonesSilenceStaysSilentAndAQuietVoiceCutsIn() {
        val g = gate()
        assertEquals(-1, feed(g, 40, { _, p -> p!! * 0.02f }, { 0.6f }))
        val hit = feed(g, 10, { _, _ -> 0.2f }, { 0.6f })
        assertTrue("at $hit", hit in 0..4)
    }

    @Test fun thePhonesOwnVoiceIsLearntFromTheMic() {
        val g = gate()
        assertEquals(-1, feed(g, 5, { _, _ -> 0.4f }, { null }))
        assertEquals(-1, feed(g, 60, { i, _ -> if (i % 4 == 0) 0.42f else 0.3f }, { null }))
        assertTrue(feed(g, 10, { _, _ -> 0.85f }, { null }) >= 0)
    }

    @Test fun nothingPlayingTheRoomsNoiseIsTheBar() {
        val g = gate()
        assertEquals(-1, feed(g, 10, { _, _ -> 0.02f }, { 0f }))
        assertTrue(feed(g, 6, { _, _ -> 0.2f }, { 0f }) >= 0)
    }

    @Test fun voicing() {
        val voiced = FloatArray(6400) { val t = it / 16000.0; (0.5 * sin(2 * PI * 140 * t) + 0.25 * sin(2 * PI * 280 * t)).toFloat() }
        val r = kotlin.random.Random(5)
        val hiss = FloatArray(6400) { r.nextFloat() * 2 - 1 }
        assertTrue(CruWake.voicedShare(voiced) > 0.8)
        assertTrue(CruWake.voicedShare(hiss) < 0.3)
    }

    @Test fun thePreRollKeepsTheLastSecondAndABit() {
        val p = PreRoll(1_300)
        for (i in 0 until 30) p.push(byteArrayOf(i.toByte()), 100)
        val got = p.drain()
        assertEquals(13, got.size)
        assertEquals(17, got.first()[0].toInt())
        assertEquals(29, got.last()[0].toInt())
        assertTrue(p.drain().isEmpty())
        for (i in 0 until 30) p.push(byteArrayOf(i.toByte()), 100)
        // Only from just before he spoke: Cru's voice before that is never sent.
        assertEquals((21..29).toList(), p.drain(900).map { it[0].toInt() })
    }

    @Test fun theVoiceRingKeepsTheLatestSamples() {
        val v = VoiceRing(4)
        v.add(byteArrayOf(0, 0x40, 0, 0x20, 0, 0x10, 0, -0x80, 0, 0x08))
        val s = v.snapshot()
        assertEquals(listOf(0.25f, 0.125f, -1f, 0.0625f), s.toList())
    }

    @Test fun callVolumeSitsAboveTheMediaShareNeverLower() {
        assertEquals(6, CruAudio.callVolume(3, 10, 0.45f))    // 45% media: 60% call (the call curve is quieter)
        assertEquals(10, CruAudio.callVolume(3, 10, 0.95f))   // never above the top
        assertEquals(8, CruAudio.callVolume(8, 10, 0.2f))     // never lowered
        assertEquals(1, CruAudio.callVolume(0, 10, 0f))       // muted media: the call at its lowest, still audible
    }

    @Test fun callAudioOnlyOnTheSpeakerOfANewEnoughPhone() {
        val speaker = listOf(CruAudio.BUILTIN_SPEAKER, 1 /* earpiece */)
        assertEquals(CruAudio.Route.Call, CruAudio.route(34, speaker))
        assertEquals(CruAudio.Route.Media, CruAudio.route(30, speaker))                 // Android 11: can't pick the speaker
        assertEquals(CruAudio.Route.Media, CruAudio.route(34, speaker + 8))             // Bluetooth headphones
        assertEquals(CruAudio.Route.Media, CruAudio.route(34, speaker + 4))             // wired headphones
        assertEquals(CruAudio.Route.Media, CruAudio.route(34, listOf(1)))               // no speaker: never the earpiece
        assertEquals(CruAudio.USAGE_VOICE_COMMUNICATION, CruAudio.usage(CruAudio.Route.Call))
        assertEquals(CruAudio.USAGE_ASSISTANT, CruAudio.usage(CruAudio.Route.Media))
        assertEquals(android.media.AudioAttributes.USAGE_VOICE_COMMUNICATION, CruAudio.USAGE_VOICE_COMMUNICATION)
        assertEquals(android.media.AudioAttributes.USAGE_ASSISTANT, CruAudio.USAGE_ASSISTANT)
        assertEquals(android.media.AudioDeviceInfo.TYPE_BUILTIN_SPEAKER, CruAudio.BUILTIN_SPEAKER)
        assertTrue(android.media.AudioDeviceInfo.TYPE_BLUETOOTH_A2DP in CruAudio.HEADSETS && android.media.AudioDeviceInfo.TYPE_WIRED_HEADPHONES in CruAudio.HEADSETS)
    }

    @Test fun callVolumeIsNeverQuieterThanTheMediaWas() {
        assertEquals(5, CruAudio.callVolume(2, 7, 0.6f))   // 60% media, lifted 15%: 75% of 7
        assertEquals(6, CruAudio.callVolume(6, 7, 0.2f))
        assertFalse(CruAudio.callVolume(0, 7, 0f) == 0)
    }
}
