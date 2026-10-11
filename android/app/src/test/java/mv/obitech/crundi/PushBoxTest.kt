package mv.obitech.crundi

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.Base64

/**
 * The message below was sealed by the real server (encryptFor in
 * src/web-push.js, which src/mobile-push.js uses) for the key pair below. If
 * this app can open it, it can open what a server sends.
 *
 * The key pair is a test vector made for this file and used nowhere else.
 */
class PushBoxTest {
    private val pkcs8 = "MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQguasbPHGq6dLW+rYO/o4r4Rpu8RiTst6qsnKWBGsWygihRANCAARbnhxbzK6oyNlsPt6WikOkwqyd4oR0kB4YjUp6UZW+RXuAYECW+FCDp2D1UYziBfXgHlWLyfQTmQdTRvT3ioWg"
    private val pub = "BFueHFvMrqjI2Ww-3paKQ6TCrJ3ihHSQHhiNSnpRlb5Fe4BgQJb4UIOnYPVRjOIF9eAeVYvJ9BOZB1NG9PeKhaA"
    private val auth = "1Er4li11gqIDVxmMQswMwQ"
    private val sealed = "_YH18xvxXYnyh9heIoFdigAAEABBBG-rhwJc6aOjf3owVB_1t-wYaPDKEmPfvtL4lrbn2sCBmLo1SZn5UOzc5mc45etJosMhYF9GuPtWKexdgYdvCI46XejHC8_PSIFx7bDwhJTdsLkF3_01Bi6G3auSjgyTuKVPBjM87hKZ6egRUSJpb8nIn06HUx4k8I4xQ19yWYyFVAeo5TDaAxp68fxdBoBB4XigepdoCH0LPcq48ytpiYgJyBswklwW9S9bjnoBP-iJ9oThtVSbhiyzpb7TKZpIJkJ4zan5jYs5MNqQgS60Jl63r5Moa-O0VTSqNYRkPeKK_OFZj5billfgjrwtYrcRDhbAI504mAt-28oqNIyhAVW4S5rIXoKmEIISw8-3RzMWqPE1UNFL66UGw9ZyXs_-AMb_LavTirCXCWGaUmqVaIuSewe2Yd2iE0DyydEMUqdX1eAza4olG6ohKmJJ_OB_oaL8sheGVZ3FOULVnq4h_GV-wzVyOU3inA"
    private val plain = "eyJ2IjoxLCJ0aXRsZSI6IkNydW5kaSIsImJvZHkiOiJEb25lLiBUaGUgY29va2llIGlzIG5vdyBzZXQgYmVmb3JlIHRoZSByZWRpcmVjdCDinJMiLCJraW5kIjoiZmluaXNoZWQiLCJ1cmwiOiIvY2hhdC9hYmMxMjMiLCJhdCI6MTc5MTc0MDAwMDAwMCwiY2hhdCI6eyJpZCI6ImFiYzEyMyIsInRpdGxlIjoiRml4IGxvZ2luIHJlZGlyZWN0IiwicHJvamVjdCI6ImNydW5kaSIsIm1hcmsiOnsiaW5pdGlhbHMiOiJGTCIsImNvbG9yIjoiI2I5MzE5NSIsImNvbG9yRGVlcCI6IiM4YzIxNzAifX19"

    private fun keys() = PushBox.Keys(Base64.getDecoder().decode(pkcs8), PushBox.unb64(pub), PushBox.unb64(auth))

    @Test fun opensWhatTheServerSealed() {
        val text = PushBox.open(sealed, keys())
        assertNotNull(text)
        assertEquals(String(Base64.getDecoder().decode(plain), Charsets.UTF_8), text)
        val j = JSONObject(text!!)
        assertEquals(1, j.getInt("v"))
        assertEquals("finished", j.getString("kind"))
        assertEquals("abc123", j.getJSONObject("chat").getString("id"))
        assertEquals("FL", j.getJSONObject("chat").getJSONObject("mark").getString("initials"))
        assertTrue(j.getString("body").endsWith("✓"))
    }

    @Test fun anotherPhonesKeysOpenNothing() {
        val other = PushBox.generate()
        assertNull(PushBox.open(sealed, other))
        assertNull(PushBox.open(sealed, PushBox.Keys(keys().privatePkcs8, keys().publicPoint, ByteArray(16))))
    }

    @Test fun aDamagedMessageOpensAsNothing() {
        val bytes = PushBox.unb64(sealed)
        bytes[bytes.size - 5] = (bytes[bytes.size - 5].toInt() xor 1).toByte()
        assertNull(PushBox.open(PushBox.b64(bytes), keys()))
        assertNull(PushBox.open("", keys()))
        assertNull(PushBox.open("not base64 !!", keys()))
        assertNull(PushBox.open(sealed.take(30), keys()))
    }

    @Test fun newKeysAreTheShapeTheServerAccepts() {
        val k = PushBox.generate()
        // An uncompressed P-256 point and a 16 byte secret: 87 and 22
        // characters of unpadded base64url, which is what the server checks.
        assertEquals(65, k.publicPoint.size); assertEquals(4, k.publicPoint[0].toInt())
        assertEquals(87, k.p256dh.length); assertEquals(22, k.authText.length)
        assertTrue(Regex("^[A-Za-z0-9_-]+$").matches(k.p256dh + k.authText))
    }
}
