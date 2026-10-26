package mv.obitech.crundi.api

import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

/** [seg] encodes as the web's encodeURIComponent, so a key with a space reaches the server as itself. */
class PathEncodeTest {

    @Test fun encodesLikeEncodeURIComponent() {
        // Expected values are what encodeURIComponent gives in a browser (and in node).
        assertEquals("dhivo-socials%3ASocial%20Desk", seg("dhivo-socials:Social Desk"))
        assertEquals("a%20b", seg("a b"))
        assertEquals("a%2Bb", seg("a+b"))
        assertEquals("a%2Fb", seg("a/b"))
        assertEquals("caf%C3%A9%20%C3%B1%20%E6%97%A5%E6%9C%AC", seg("café ñ 日本"))
        assertEquals("%3F%23%26%3D%25", seg("?#&=%"))
        // Left alone by encodeURIComponent: letters, digits and - _ . ! ~ * ' ( )
        assertEquals("AZaz09-_.!~*'()", seg("AZaz09-_.!~*'()"))
        assertEquals("", seg(""))
    }

    @Test fun serviceActionsUseTheEncodedKey() = runTest {
        val t = FakeTransport()
        val key = "dhivo-socials:Social Desk"
        val path = "/api/services/dhivo-socials%3ASocial%20Desk"
        for (a in listOf("start", "stop", "restart", "delete")) t.on("POST $path/$a", """{"ok":true}""")
        t.on("GET $path/logs", """{"ok":true,"logs":["up"]}""")
        val services = ServicesApi(Rpc(t))
        services.start(key)
        services.stop(key)
        services.restart(key)
        services.delete(key)
        assertEquals(listOf("up"), services.logs(key))
        assertEquals(listOf("start", "stop", "restart", "delete", "logs").map { "$path/$it" }, t.requests.map { it.path })
        for (r in t.requests) assertFalse(r.path, r.path.contains('+'))
    }

    @Test fun forwardsAndBrowsersUseTheEncodedValue() = runTest {
        val t = FakeTransport()
        t.on("DELETE /api/forwards/my%20app", """{"ok":true}""")
        t.on("POST /api/browsers/crundi%3Amy%20tab/close", """{"ok":true}""")
        ForwardsApi(Rpc(t)).remove("my app")
        BrowsersApi(Rpc(t)).close("crundi:my tab")
        assertEquals(listOf("/api/forwards/my%20app", "/api/browsers/crundi%3Amy%20tab/close"), t.requests.map { it.path })
    }
}
