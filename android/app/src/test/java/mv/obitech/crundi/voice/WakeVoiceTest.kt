package mv.obitech.crundi.voice

import mv.obitech.crundi.api.CrundiJson
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.sin

/** "Hey Cru" for the owner only ([WakeVoice]): the scores and the profile, with made-up embeddings (no model). */
class WakeVoiceTest {
    /** An embedding pointing one way ([seed]: whose voice), with a little wobble per take. */
    private fun emb(seed: Int, wobble: Double) = FloatArray(WakeVoice.DIM) { i -> (sin(i * (0.91 + seed * 0.37)) + 0.15 * sin(i * wobble)).toFloat() }
    private val voice = WakeVoice.enrol((1..5).map { emb(1, it.toDouble()) })

    @Test fun theTakesBecomeTheVoice() {
        assertTrue(voice.valid())
        assertEquals(WakeVoice.MODEL_ID, voice.model)
        assertEquals(5, voice.clips.size)
        assertFalse(voice.copy(model = "other").valid())
        assertFalse(voice.copy(clips = voice.clips.take(WakeVoice.K - 1)).valid())
    }

    @Test fun theOwnerPassesAndAnotherVoiceDoesNot() {
        assertTrue(WakeVoice.score(voice, emb(1, 9.0)) >= WakeVoice.THRESHOLD)
        assertEquals(true, WakeVoice.verdict(voice, emb(1, 9.0), 0.8))
        assertEquals(false, WakeVoice.verdict(voice, emb(2, 9.0), 0.8))
    }

    @Test fun noVerdictWhenThereIsNothingToGoOn() {
        assertNull("too short", WakeVoice.verdict(voice, emb(1, 9.0), 0.1))
        assertNull("no voice in the profile", WakeVoice.verdict(null, emb(1, 9.0), 0.8))
        assertNull("no embedding (the model failed)", WakeVoice.verdict(voice, null, 0.8))
    }

    @Test fun theVoiceIsKeptWithTheProfileAndAnOlderProfileStillLoads() {
        val p = WakeProfile(limit = 1.0, phraseFrames = 80, templates = List(3) { listOf(listOf(0.0)) }, voice = voice)
        val back = CrundiJson.decodeFromString(WakeProfile.serializer(), CrundiJson.encodeToString(WakeProfile.serializer(), p))
        assertTrue(back.hasVoice() && back.voice == voice)
        val older = CrundiJson.decodeFromString(WakeProfile.serializer(), CrundiJson.encodeToString(WakeProfile.serializer(), p.copy(voice = null)).replace(",\"voice\":null", ""))
        assertFalse(older.hasVoice())
        assertTrue(older.valid())
    }
}
