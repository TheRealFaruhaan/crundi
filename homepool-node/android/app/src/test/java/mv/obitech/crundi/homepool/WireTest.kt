package mv.obitech.crundi.homepool

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.net.InetAddress

/** The pure parts of the node: the code, the frames, the private-target rule (as in app/homepool-node.js). */
class WireTest {

    // ── NodeConfig.parse ──

    @Test fun parsesACode() {
        val c = NodeConfig.parse("crundi-homepool://node?s=https%3A%2F%2Fdev.example.com%2F&k=hpn_n1a2b3.Secret_-x")!!
        assertEquals("https://dev.example.com", c.server)
        assertEquals("hpn_n1a2b3.Secret_-x", c.key)
        assertEquals("n1a2b3", c.nodeId)
    }

    @Test fun parsesWithSpacesAndAnyOrder() {
        val c = NodeConfig.parse("  crundi-homepool://node?k=hpn_nff.abc&s=http%3A%2F%2F127.0.0.1%3A8889  ")!!
        assertEquals("http://127.0.0.1:8889", c.server)
        assertEquals("nff", c.nodeId)
    }

    @Test fun roundTripsWhatItMakes() {
        val c = NodeConfig("https://x.example.com/sub", "hpn_n00.k+y/z")
        assertEquals(c, NodeConfig.parse(c.code()))
    }

    @Test fun refusesWhatIsNotACode() {
        for (bad in listOf(
            "", "hello", "https://dev.example.com", "homepool://node?s=https%3A%2F%2Fa&k=hpn_n1.x",
            "crundi-homepool://node?s=ftp%3A%2F%2Fa&k=hpn_n1.x", "crundi-homepool://node?s=https%3A%2F%2Fa&k=abc",
            "crundi-homepool://node?k=hpn_n1.x", "crundi-homepool://node?s=https%3A%2F%2Fa",
        )) assertNull(bad, NodeConfig.parse(bad))
    }

    @Test fun nodeIdIsEmptyForAnOddKey() {
        assertEquals("", NodeConfig("https://a", "hpn_zz").nodeId)
    }

    @Test fun neverPrintsTheKey() {
        assertFalse(NodeConfig("https://a", "hpn_n1.topsecret").toString().contains("topsecret"))
    }

    // ── frames ──

    @Test fun encodesTheHeader() {
        val b = Wire.encode(FrameType.DATA, 0x01020304, byteArrayOf(9, 8))
        assertArrayEquals(byteArrayOf(2, 1, 2, 3, 4, 9, 8), b)
    }

    @Test fun roundTripsFrames() {
        for (type in 1..7) for (id in listOf(0, 1, 255, 65536, Int.MAX_VALUE, -1)) {
            val p = "payload $type".toByteArray()
            val f = Wire.decode(Wire.encode(type, id, p))!!
            assertEquals(type, f.type)
            assertEquals(id, f.id)
            assertArrayEquals(p, f.payload)
        }
    }

    @Test fun idsAreUnsigned() {
        val f = Wire.decode(byteArrayOf(3, 0xFF.toByte(), 0xFF.toByte(), 0xFF.toByte(), 0xFE.toByte()))!!
        assertEquals(4294967294L, f.streamId)
        assertEquals(0, f.payload.size)
    }

    @Test fun emptyPayloadAndShortFrames() {
        assertEquals(5, Wire.encode(FrameType.CLOSE, 7).size)
        assertNull(Wire.decode(byteArrayOf(1, 0, 0, 0)))
        assertNull(Wire.decode(null))
    }

    @Test fun textPayloadIsUtf8() {
        val f = Wire.decode(Wire.encode(FrameType.ERROR, 5, "nope é"))!!
        assertEquals("nope é", String(f.payload, Charsets.UTF_8))
    }

    // ── private targets ──

    @Test fun refusesPrivateHosts() {
        for (h in listOf(
            "localhost", "LOCALHOST", "foo.localhost", "printer.local", "db.internal", "printer.local.",
            "127.0.0.1", "127.1.2.3", "10.0.0.1", "172.16.0.1", "172.31.255.255", "192.168.1.1", "169.254.169.254",
            "100.64.0.1", "100.127.255.255", "0.0.0.0", "224.0.0.1", "239.255.255.250", "255.255.255.255",
            "::1", "[::1]", "::", "fc00::1", "fd12:3456::1", "fe80::1", "[fe80::1%25wlan0]", "ff02::1",
            "::ffff:10.0.0.1", "::ffff:127.0.0.1", "", " ",
        )) assertTrue("should refuse '$h'", Targets.isPrivateHost(h))
    }

    @Test fun allowsPublicHosts() {
        for (h in listOf(
            "example.com", "www.instagram.com", "8.8.8.8", "1.1.1.1", "172.15.0.1", "172.32.0.1", "100.63.0.1",
            "100.128.0.1", "192.167.1.1", "169.253.1.1", "2606:4700:4700::1111", "[2001:4860:4860::8888]", "::ffff:8.8.8.8",
            "local.example.com", "internal.example.com",
        )) assertFalse("should allow '$h'", Targets.isPrivateHost(h))
    }

    @Test fun aNameIsNotLookedUpByTheHostCheck() {
        // A name passes the host check; its resolved addresses are checked separately.
        assertFalse(Targets.isPrivateHost("sneaky-rebind.example"))
        assertTrue(Targets.isPrivateAddress(InetAddress.getByAddress("sneaky-rebind.example", byteArrayOf(192.toByte(), 168.toByte(), 0, 1))))
        assertFalse(Targets.isPrivateAddress(InetAddress.getByAddress(byteArrayOf(93, 184.toByte(), 216.toByte(), 34))))
    }
}
