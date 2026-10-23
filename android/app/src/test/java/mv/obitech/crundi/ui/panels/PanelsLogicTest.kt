package mv.obitech.crundi.ui.panels

import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray
import kotlinx.serialization.json.putJsonObject
import mv.obitech.crundi.api.model.Widget
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PanelsLogicTest {
    private fun w(id: String, slot: String = "cell", state: String = "open", chat: String = "c1", at: Long = 0) =
        Widget(id, slot = slot, state = state, chat = chat, updatedAt = JsonPrimitive(at))

    @Test fun placement() {
        assertEquals("none", placeOf(w("a", state = "closed")))
        assertEquals("cell", placeOf(w("a", slot = "dock", chat = "")))
        assertEquals("dock", placeOf(w("a", slot = "dock"), setOf("c1")))
        assertEquals("none", placeOf(w("a", slot = "dock"), setOf("c2")))
        assertEquals("none", placeOf(w("a", slot = "tab")))
        assertEquals("chip", placeOf(w("a", slot = "chip")))
    }

    @Test fun tabOrderIsOpenThenNewest() {
        val s = sortForTab(listOf(w("old", at = 1), w("closed", state = "closed", at = 9), w("new", at = 5)))
        assertEquals(listOf("new", "old", "closed"), s.map { it.id })
    }

    @Test fun words() {
        assertEquals("closed", statusWord(w("a", state = "closed")))
        assertEquals("workbench", statusWord(w("a")))
        assertEquals("top", statusWord(w("a", slot = "chip")))
        assertEquals("already open", pickerWord(w("a")))
        assertEquals("Panels tab", pickerWord(w("a", slot = "tab")))
    }

    @Test fun dockIsOfferedOnlyWithAChat() {
        assertTrue(movableSlots(w("a")).any { it.first == "dock" })
        assertFalse(movableSlots(w("a", chat = "")).any { it.first == "dock" })
    }

    @Test fun chipValueFollowsTheDottedPath() {
        val v = buildJsonObject { putJsonObject("tests") { put("passed", 412) }; putJsonArray("runs") { add(JsonPrimitive("first")) }; put("long", "x".repeat(90)) }
        assertEquals("412", chipText(v, "tests.passed"))
        assertEquals("first", chipText(v, "runs.0"))
        assertEquals("", chipText(v, "tests.missing"))
        assertEquals(60, chipText(v, "long").length)
        assertEquals("{\"passed\":412}", chipText(v, "tests"))
    }

    @Test fun callsAreLimitedTo40In10Seconds() {
        val l = CallLimiter()
        repeat(40) { assertTrue(l.allow(1000L + it)) }
        assertFalse(l.allow(2000))
        assertTrue(l.allow(11_001))
    }

    @Test fun onlyWebLinksOpen() {
        assertTrue(safeLink("https://dev.faruhaan.com/x"))
        assertTrue(safeLink("HTTP://a.b"))
        assertFalse(safeLink("javascript:alert(1)"))
        assertFalse(safeLink("intent://scan#Intent;end"))
        assertFalse(safeLink("data:text/html,hi"))
    }

    @Test fun eventKeysMatchTheServer() {
        assertEquals("crundi", panelKey("crundi"))
        assertEquals("my_app", panelKey("My App"))
        assertEquals("_global", panelKey(""))
    }

    @Test fun resultMessages() {
        assertEquals("""{"t":"result","seq":3,"ok":false,"error":"Too many requests; slow down."}""", result(JsonPrimitive(3), false, error = "Too many requests; slow down.").toString())
        assertEquals("""{"t":"result","seq":1,"ok":true,"value":true}""", result(JsonPrimitive(1), true, JsonPrimitive(true)).toString())
    }

    @Test fun shotsParse() {
        val r = buildJsonObject { put("at", 77); putJsonArray("shots") { add(buildJsonObject { put("frame", "cell"); put("state", "live") }); add(buildJsonObject { put("frame", "dock"); put("state", "empty") }) } }
        val (shots, at) = parseShots(r)
        assertEquals(77L, at)
        assertEquals(listOf("cell--live", "dock--empty"), shots.map { it.name })
        assertEquals("dock · empty", shots[1].label)
    }
}
