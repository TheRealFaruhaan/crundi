package com.crundi.app

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The same cases the server is held to, so the two cannot drift apart. */
class VersionsTest {
    private val order = listOf("1.19.8", "1.19.9-dev.1", "1.19.9-dev.2", "1.19.9-dev.10", "1.19.9", "1.19.10-dev.1", "1.20.0")

    @Test fun orderMatchesTheServers() {
        for (i in order.indices) for (j in order.indices) {
            val want = Integer.signum(i.compareTo(j))
            assertEquals("${order[i]} vs ${order[j]}", want, Integer.signum(Versions.compare(order[i], order[j])))
        }
    }

    @Test fun notAVersionSortsLowest() {
        assertTrue(Versions.compare("nonsense", "0.0.1") < 0)
        assertTrue(Versions.compare("1.0.0", null) > 0)
        assertEquals(0, Versions.compare("x", "y"))
    }

    @Test fun belowAMinimum() {
        assertFalse(Versions.below("1.0.0", ""))
        assertFalse(Versions.below("anything", null))
        assertTrue(Versions.below("1.0.0-dev.3", "1.0.0"))
        assertFalse(Versions.below("1.0.0", "1.0.0"))
        assertTrue(Versions.below("", "1.0.0"))
        assertFalse(Versions.below("1.20.0", "1.20.0-dev.1"))
    }

    private fun handshake(server: String, minAndroid: String, accepted: Boolean? = null) = JSONObject(
        """{"ok":true,"server":{"version":"$server","minClient":{"web":"","desktop":"","android":"$minAndroid","ios":""},"capabilities":["handshake","pairing"]}""" +
            (if (accepted == null) "}" else ""","client":{"accepted":$accepted}}""")
    )

    @Test fun bothSidesAgree() {
        val v = Gate.judge(handshake("1.20.0", ""), "1.0.0", "1.20.0-dev.1")
        assertTrue(v.ok)
        assertTrue("pairing" in v.capabilities)
    }

    @Test fun theServerNeedsANewerApp() {
        val v = Gate.judge(handshake("1.21.0", "1.1.0"), "1.0.0", "1.20.0-dev.1")
        assertTrue(v.appTooOld); assertFalse(v.serverTooOld); assertEquals("1.1.0", v.minApp)
    }

    @Test fun theAppNeedsANewerServer() {
        val v = Gate.judge(handshake("1.19.27", ""), "1.0.0", "1.20.0-dev.1")
        assertTrue(v.serverTooOld); assertFalse(v.appTooOld); assertEquals("1.19.27", v.serverVersion)
    }

    @Test fun eachNeedsTheOtherNewer() {
        val v = Gate.judge(handshake("1.19.27", "2.0.0"), "1.0.0", "1.20.0-dev.1")
        assertTrue(v.appTooOld && v.serverTooOld)
    }

    @Test fun theServersOwnRefusalCounts() {
        assertTrue(Gate.judge(handshake("1.20.0", "", accepted = false), "1.0.0", "1.20.0-dev.1").appTooOld)
        assertTrue(Gate.judge(handshake("1.20.0", "", accepted = true), "1.0.0", "1.20.0-dev.1").ok)
    }

    @Test fun anAnswerThatIsNotAHandshakeIsAServerTooOld() {
        // A server from before the handshake existed answers something else.
        assertTrue(Gate.judge(JSONObject("{}"), "1.0.0", "1.20.0-dev.1").serverTooOld)
    }

    @Test fun pairingLinks() {
        val code = "0123456789abcdef01234567"
        val a = PairingLink.parse("https://dev.faruhaan.com/pair#$code")
        assertNotNull(a); assertEquals("https://dev.faruhaan.com", a!!.baseUrl); assertEquals(code, a.code); assertFalse(a.insecure)
        val b = PairingLink.parse("  http://192.168.1.20:8888/pair#$code\n")
        assertEquals("http://192.168.1.20:8888", b!!.baseUrl); assertTrue(b.insecure)
        for (bad in listOf("", "nonsense", "https://x.example/other#$code", "https://x.example/pair#short", "https://x.example/pair?c=$code", "javascript:alert(1)//pair#$code", "crundi://x/pair#$code", "https://x.example:99999/pair#$code", "https://x.example/pair#${code.uppercase()}")) {
            assertNull(bad, PairingLink.parse(bad))
        }
    }
}
