package mv.obitech.crundi.api

import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

/** Pause's choices over the wire (capability 'factory-pause-options'), and the plain pause an older server is sent. */
class FactoryPauseApiTest {
    private val t = FakeTransport().on("POST /api/factory-runs") { ApiResponse(200, """{"ok":true,"run":{"id":"run_a","status":"paused"}}""") }
    private val client = CrundiClient(t)
    private fun sent(): JsonObject = t.requests.last().json!!.jsonObject

    @Test fun aPlainPauseSendsNoChoices() = runTest {
        client.factories.pause("run_a")
        assertEquals("pause", sent()["action"]!!.jsonPrimitive.content)
        assertFalse("mode" in sent())
        assertFalse("resumeOnReset" in sent())
    }

    @Test fun runLimitsSendOnlyWhatChanged() = runTest {
        client.factories.setRunLimits("run_a", mapOf("e4" to 5), maxHandoffs = 300)
        assertEquals("runLimits", sent()["action"]!!.jsonPrimitive.content)
        assertEquals("5", sent()["edges"]!!.jsonObject["e4"]!!.jsonPrimitive.content)
        assertEquals("300", sent()["maxHandoffs"]!!.jsonPrimitive.content)
        assertFalse("maxMinutes" in sent())
        client.factories.setRunLimits("run_a", maxMinutes = 90)
        assertFalse("edges" in sent())
        assertEquals("90", sent()["maxMinutes"]!!.jsonPrimitive.content)
    }

    @Test fun runLimitsOnAnOlderServer() = runTest {
        val old = CrundiClient(FakeTransport().on("POST /api/factory-runs") { ApiResponse(400, """{"ok":false,"error":"Unknown factory run action: runLimits"}""") })
        try {
            old.factories.setRunLimits("run_a", mapOf("e4" to 5))
            org.junit.Assert.fail("an older server")
        } catch (e: ApiException) { assertEquals("Not supported by this server", e.message) }
    }

    @Test fun stopNowAndCarryOnAfterTheReset() = runTest {
        client.factories.pause("run_a", now = true, resumeOnReset = true)
        assertEquals("now", sent()["mode"]!!.jsonPrimitive.content)
        assertEquals("true", sent()["resumeOnReset"]!!.jsonPrimitive.content)
        client.factories.pause("run_a", now = false, resumeOnReset = true)
        assertFalse("mode" in sent())
        assertEquals("true", sent()["resumeOnReset"]!!.jsonPrimitive.content)
    }
}
