package mv.obitech.crundi.homepool

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicLong

/**
 * The Kotlin node against a REAL Crundi server (src/homepool.js), not a stub.
 * Skipped unless given one:
 *
 *   HP_LIVE_SERVER  http://127.0.0.1:8889         (a dev Crundi)
 *   HP_LIVE_KEY     hpn_…                          a node alone in its own network
 *   HP_LIVE_SOCKS   socks5h://<key>:<secret>@…     a key for that network only
 *
 * It joins, gets the server's hello, and a request made through the server's
 * SOCKS port with that key comes out through this node (curl, to a public
 * address, so the node's private-address rule stays on).
 */
class LiveServerTest {
    @Test
    fun carriesTrafficForTheRealServer() {
        val server = System.getenv("HP_LIVE_SERVER") ?: ""
        val key = System.getenv("HP_LIVE_KEY") ?: ""
        val socks = System.getenv("HP_LIVE_SOCKS") ?: ""
        assumeTrue("no live server given", server.isNotEmpty() && key.isNotEmpty() && socks.isNotEmpty())

        val connected = CountDownLatch(1)
        val hello = CountDownLatch(1)
        val carried = AtomicLong(0)
        val link = NodeLink(server, key, object : NodeLink.Events {
            override fun onState(state: NodeLink.State, detail: String) { if (state == NodeLink.State.CONNECTED) connected.countDown() }
            override fun onHello(name: String?, network: String?, networkName: String?) { hello.countDown() }
            override fun onBytes(up: Long, down: Long) { carried.addAndGet(up + down) }
        })
        link.start()
        try {
            assertTrue("connects", connected.await(15, TimeUnit.SECONDS))
            assertTrue("gets hello", hello.await(10, TimeUnit.SECONDS))
            Thread.sleep(500)
            val p = ProcessBuilder("curl", "-s", "-m", "25", "--proxy", socks, "https://api.ipify.org").redirectErrorStream(true).start()
            val out = p.inputStream.bufferedReader().readText().trim()
            assertEquals("curl exits cleanly: $out", 0, p.waitFor())
            assertTrue("an address came back: $out", Regex("^[0-9a-f.:]+$").matches(out))
            assertTrue("the node carried it", carried.get() > 0)
        } finally {
            link.stop()
        }
    }
}
