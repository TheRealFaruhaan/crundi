package mv.obitech.crundi.api

import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import mv.obitech.crundi.api.model.HpKeyInput
import mv.obitech.crundi.ui.operations.homepoolStatusLine
import mv.obitech.crundi.ui.operations.logWords
import mv.obitech.crundi.ui.operations.nodeState
import mv.obitech.crundi.homepool.NodePhase
import mv.obitech.crundi.homepool.NodeSnapshot
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/** Settings > Homepool against a server in memory: the overview as a real dev server sent it (src/test/resources/api/homepool.json), and the actions. */
class HomepoolApiTest {
    private val overviewJson = javaClass.classLoader!!.getResource("api/homepool.json")!!.readText()

    @Test fun theRealOverviewIsRead() = runBlocking {
        val ov = CrundiClient(FakeTransport().on("GET /api/homepool", overviewJson)).homepool.overview()
        assertEquals(7580, ov.ports.socks)
        assertEquals(listOf("server", "pool", "home"), ov.networks.map { it.id })
        assertEquals("Home", ov.networkName("home"))
        assertEquals("Unassigned", ov.networkName(""))
        assertEquals(listOf("home"), ov.own.map { it.id })
        val pool = ov.pool!!
        assertTrue(pool.enabled && pool.pool!!.up)
        assertEquals(2, pool.pool!!.liveCount)

        val docker = ov.nodes.first { it.name == "test-docker" }
        assertTrue(docker.up)
        assertEquals("Ethernet", docker.info!!.network)
        assertEquals("203.0.113.17", docker.info!!.publicIp)
        assertEquals("up" to mv.obitech.crundi.ui.components.BadgeKind.Good, nodeState(docker))
        assertEquals("down", nodeState(ov.nodes.first { !it.up }).first)

        val block = ov.keys.first { it.name == "scraper-sim" }
        assertTrue(block.portBlock)
        assertEquals(7590, block.ports!!.base)
        assertNull(block.expiresAt)
        assertEquals("socks5://host.docker.internal:7590", block.use!!.block!!.any)
        assertEquals(3, block.use!!.block!!.compose.size)
        assertTrue(ov.keys.first { it.name == "curl-test" }.use!!.socks.startsWith("socks5h://"))

        assertEquals(1, ov.pending.size)
        assertEquals("insta-scraper", ov.pending[0].name)
        assertEquals("refused", ov.requests.last().status)

        val entries = ov.entries
        assertEquals("refused-login", entries[0].what)
        assertEquals("Refused a proxy login as kdeadbeef00 from 172.21.0.5", logWords(entries[0], ov))
        assertTrue(entries.any { it.what == "node-added" && logWords(it, ov).startsWith("Node test-docker added to Home") })
    }

    @Test fun aSparseOrNewerOverviewStillReads() = runBlocking {
        val ov = CrundiClient(FakeTransport().on("GET /api/homepool", """{"ok":true,"networks":[{"id":"server"},{"id":"x","name":"X","future":1}],"nodes":[{"id":"n1"}],"keys":[{"id":"k1","expiresAt":1800000000000,"newThing":{"a":1}}],"log":[{"at":1,"what":"something-new","node":"n1"}]}""")).homepool.overview()
        assertEquals("n1", ov.nodes[0].name)
        assertEquals(1800000000000L, ov.keys[0].expiresAt)
        assertEquals("Something new (node n1)", logWords(ov.entries[0], ov))
    }

    @Test fun actionsSendWhatTheServerReads() = runBlocking {
        val answer = """{"ok":true,"node":{"id":"n1","key":"hpn_n1.abc","code":"crundi-homepool://node?s=x&k=y","server":"https://x","docker":"docker run","qr":"<svg/>"},"overview":$overviewJson}"""
        val t = FakeTransport().on("POST /api/homepool", answer)
        val hp = CrundiClient(t).homepool
        val a = hp.selfNode("dev123", "Pixel 9", "home")
        assertEquals("hpn_n1.abc", a.node!!.key)
        assertNotNull(a.overview)
        val body = t.requests.last().json!!.jsonObject
        assertEquals("self-node", body["action"]!!.jsonPrimitive.content)
        assertEquals("dev123", body["deviceId"]!!.jsonPrimitive.content)

        hp.updateNode("n1", enabled = false)
        val upd = t.requests.last().json!!.jsonObject
        assertEquals(setOf("action", "id", "enabled"), upd.keys)

        hp.addKey(HpKeyInput(kind = "software", name = "scraper", networks = listOf("home"), portBlock = true, allowFrom = listOf("homepool")))
        val key = t.requests.last().json!!.jsonObject["key"] as JsonObject
        assertEquals("software", key["kind"]!!.jsonPrimitive.content)
        assertEquals(JsonNull, key["expiresAt"])
        assertEquals("homepool", (key["allowFrom"] as kotlinx.serialization.json.JsonArray)[0].jsonPrimitive.content)

        hp.decide("r1", true, HpKeyInput(kind = "project", networks = listOf("server")))
        val ch = t.requests.last().json!!.jsonObject["changes"] as JsonObject
        assertFalse("changes never carry the chat" , "chatId" in ch)
        hp.decide("r1", false)
        assertEquals("false", t.requests.last().json!!.jsonObject["approve"]!!.jsonPrimitive.content)

        hp.setPoolBridge(false)
        assertEquals("""{"poolBridge":{"enabled":false}}""", t.requests.last().json!!.jsonObject["settings"].toString())
    }

    @Test fun refusalsComeBackInTheServersWords() = runBlocking {
        val t = FakeTransport().on("POST /api/homepool", """{"ok":false,"error":"Give the key at least one network."}""", 400)
        try { CrundiClient(t).homepool.addKey(HpKeyInput(networks = emptyList())); fail() } catch (e: ApiException) { assertEquals("Give the key at least one network.", e.message) }
    }

    @Test fun anOlderServerIsNotSupported() {
        assertFalse(HomepoolApi.supported(setOf("handshake")))
        assertTrue(HomepoolApi.supported(setOf("handshake", "homepool")))
    }

    @Test fun theSettingsLine() {
        assertNull(homepoolStatusLine(NodeSnapshot.EMPTY))
        val s = NodeSnapshot.EMPTY.copy(configured = true, enabled = true, phase = NodePhase.CONNECTED, bytesToday = 120L * 1024 * 1024)
        assertEquals("Sharing this phone: Connected, 120 MB today", homepoolStatusLine(s))
        assertEquals("Sharing this phone: Paused (Wi-Fi only), 120 MB today", homepoolStatusLine(s.copy(phase = NodePhase.PAUSED, pauseReason = "Wi-Fi only")))
    }
}
