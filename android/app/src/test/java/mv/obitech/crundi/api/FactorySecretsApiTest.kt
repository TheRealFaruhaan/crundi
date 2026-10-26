package mv.obitech.crundi.api

import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import mv.obitech.crundi.api.model.Factory
import mv.obitech.crundi.api.model.FactorySummary
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** A factory's pre-authorised secrets over the wire (capability 'factory-secrets'), and what an older server is sent. */
class FactorySecretsApiTest {
    private val t = FakeTransport()
        .on("POST /api/factory-runs") { ApiResponse(200, """{"ok":true,"run":{"id":"run_a","status":"running"}}""") }
        .on("POST /api/secrets") { ApiResponse(200, """{"ok":true,"name":"Deploy token"}""") }
    private val client = CrundiClient(t)
    private fun sent(): JsonObject = t.requests.last().json!!.jsonObject

    @Test fun startWithoutPinsSendsNoField() = runTest {
        client.factories.start("p", "f", "Title", emptyMap())
        assertFalse("secretPins" in sent())
        client.factories.start("p", "f", "Title", emptyMap(), secretPins = emptyMap())
        assertFalse("secretPins" in sent())
    }

    @Test fun startCarriesThePinsByIdOnly() = runTest {
        client.factories.start("p", "f", "Title", emptyMap(), secretPins = mapOf("abc123" to "482913"))
        val pins = sent()["secretPins"]!!.jsonObject
        assertEquals("482913", pins["abc123"]!!.jsonPrimitive.content)
        assertEquals(1, pins.size)
    }

    @Test fun checkingAPinNeverAsksForTheValue() = runTest {
        client.secrets.check("abc123", "482913")
        assertEquals("check", sent()["action"]!!.jsonPrimitive.content)
        assertEquals("abc123", sent()["id"]!!.jsonPrimitive.content)
    }

    @Test fun olderServersHaveNoList() {
        val old = CrundiJson.decodeFromString(Factory.serializer(), """{"id":"f","name":"x","settings":{"managerMode":"normal"}}""")
        assertTrue(old.settings.secrets.isEmpty())
        val sum = CrundiJson.decodeFromString(FactorySummary.serializer(), """{"id":"f","name":"x"}""")
        assertTrue(sum.secrets.isEmpty())
        val now = CrundiJson.decodeFromString(FactorySummary.serializer(), """{"id":"f","name":"x","secrets":["a","b"]}""")
        assertEquals(listOf("a", "b"), now.secrets)
    }
}
