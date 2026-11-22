package mv.obitech.crundi.api

import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import mv.obitech.crundi.api.model.FactoryRun
import mv.obitech.crundi.api.model.FactorySettings
import mv.obitech.crundi.api.model.Pane
import mv.obitech.crundi.api.model.Worker
import mv.obitech.crundi.api.model.workerUpdatesWords
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/** Terminals on a PC, a factory's PC-offline setting, runs' PCs away, and how a worker updates. All optional: an older server leaves them out. */
class PcTerminalsTest {
    @Test fun aTerminalOnAPcIsRead() {
        val p = CrundiJson.decodeFromString<Pane>(
            """{"id":"t1","project":"crundi","kind":"terminal","where":"worker:w1","pc":{"where":"worker:w1","name":"Studio PC","online":false},"notice":"The PC did not have that conversation; starting fresh"}""",
        )
        assertEquals("on Studio PC · PC offline", p.runsOn!!.badge)
        assertEquals("The PC did not have that conversation; starting fresh", p.notice)
        val old = CrundiJson.decodeFromString<Pane>("""{"id":"t2","kind":"terminal"}""")
        assertNull(old.runsOn)
        assertEquals("", old.notice)
    }

    @Test fun whereIsSentForATerminalOnlyWhenSet() = runBlocking {
        val t = FakeTransport().on("POST /api/terminals/create", """{"ok":true,"id":"t9"}""")
        val c = CrundiClient(t)
        c.terminals.create("crundi", where = "worker:w1")
        assertEquals("worker:w1", t.requests.last().json!!.jsonObject["where"]!!.jsonPrimitive.content)
        c.terminals.create("crundi", shell = true)
        assertFalse((t.requests.last().json as JsonObject).containsKey("where"))
        c.terminals.create("crundi", where = "")
        assertFalse((t.requests.last().json as JsonObject).containsKey("where"))
    }

    @Test fun pcOfflineMinutesIsOptional() {
        assertEquals(45, CrundiJson.decodeFromString<FactorySettings>("""{"workerOfflineMinutes":45}""").workerOfflineMinutes)
        assertNull(CrundiJson.decodeFromString<FactorySettings>("""{"tickMinutes":10}""").workerOfflineMinutes)
    }

    @Test fun pcsAwayAreRead() {
        val r = CrundiJson.decodeFromString<FactoryRun>(
            """{"id":"r1","pcsAway":[{"workerId":"w1","worker":"Studio PC","since":"2026-11-19T10:00:00.000Z","minutes":42,"overdue":true,"employees":["Ada","Linus"]}]}""",
        )
        val a = r.pcsAway.single()
        assertEquals("w1", a.workerId)
        assertTrue(a.overdue)
        assertEquals(listOf("Ada", "Linus"), a.employees)
        assertEquals(1795082400000L, a.sinceMs)
        assertTrue(CrundiJson.decodeFromString<FactoryRun>("""{"id":"r2"}""").pcsAway.isEmpty())
    }

    @Test fun moveToServer() = runBlocking {
        val t = FakeTransport().on("POST /api/factory-runs", """{"ok":true}""")
        CrundiClient(t).factories.moveToServer("r1", "w1")
        val sent = t.requests.last().json!!.jsonObject
        assertEquals("moveToServer", sent["action"]!!.jsonPrimitive.content)
        assertEquals("r1", sent["id"]!!.jsonPrimitive.content)
        assertEquals("w1", sent["worker"]!!.jsonPrimitive.content)
        try {
            CrundiClient(FakeTransport().on("POST /api/factory-runs", """{"ok":false,"error":"Unknown factory run action"}""", 400)).factories.moveToServer("r1", "w1")
            fail("an older server")
        } catch (e: ApiException) { assertEquals("Not supported by this server", e.message) }
    }

    @Test fun workerUpdates() {
        assertEquals("self", CrundiJson.decodeFromString<Worker>("""{"id":"w1","updates":"self"}""").updates)
        assertEquals("", CrundiJson.decodeFromString<Worker>("""{"id":"w1"}""").updates)
        assertEquals("updates itself", workerUpdatesWords("self"))
        assertEquals("updates with the PC app", workerUpdatesWords("app"))
        assertEquals("", workerUpdatesWords(""))
    }
}
