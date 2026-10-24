package mv.obitech.crundi.api

import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import mv.obitech.crundi.api.model.FactoryTrigger
import mv.obitech.crundi.api.model.HookSettings
import mv.obitech.crundi.api.model.Schedule
import mv.obitech.crundi.api.model.ScheduleAction
import mv.obitech.crundi.api.model.ScheduleWhen
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Schedule webhooks and signed factory triggers (server 1.21.1) over the wire, and what an older server is sent. */
class WebhookApiTest {
    private val t = FakeTransport()
    private val client = CrundiClient(t)
    private fun sent(): JsonObject = t.requests.last().json!!.jsonObject

    // As src/schedule-store.js stores one, and GET /api/schedules sends it.
    private val hookJson = """{"ok":true,"services":[],"schedules":[{"id":"a1b2c3d4e5f60718","project":"crundi","name":"Grafana alerts","enabled":true,"tz":"",
        "action":{"kind":"chat","prompt":"Look into {{payload.message}}","session":"resume","sessionId":"8441d374"},
        "when":{"mode":"webhook","time":"09:00","date":"","days":[]},"conditions":[],
        "webhook":{"key":"KEY123","secret":"SECRET456","auth":"signature","signatureHeader":"x-grafana-alerting-signature","timestampHeader":"","idempotency":"{{payload.groupKey}}"},
        "createdAt":"","updatedAt":"","lastRun":null,"lastMet":false}]}"""

    @Test fun readsAWebhookSchedule() = runTest {
        t.on("GET /api/schedules", hookJson)
        val s = client.schedules.list().schedules.single()
        assertTrue(s.calledNotTimed)
        assertEquals("signature", s.webhook?.auth)
        assertEquals("SECRET456", s.webhook?.secret)
        assertEquals("{{payload.groupKey}}", s.webhook?.idempotency)
    }

    @Test fun aSaveSendsTheWebhookSettingsButNeverTheCredentials() = runTest {
        t.on("GET /api/schedules", hookJson).on("POST /api/schedules") { ApiResponse(200, """{"ok":true,"schedule":{"id":"a1b2c3d4e5f60718"}}""") }
        val s = client.schedules.list().schedules.single()
        client.schedules.update(s)
        val wh = sent()["schedule"]!!.jsonObject["webhook"]!!.jsonObject
        assertEquals("signature", wh["auth"]!!.jsonPrimitive.content)
        assertEquals("x-grafana-alerting-signature", wh["signatureHeader"]!!.jsonPrimitive.content)
        assertEquals("{{payload.groupKey}}", wh["idempotency"]!!.jsonPrimitive.content)
        assertFalse("key" in wh)
        assertFalse("secret" in wh)
        assertEquals("webhook", sent()["schedule"]!!.jsonObject["when"]!!.jsonObject["mode"]!!.jsonPrimitive.content)
    }

    @Test fun aScheduleWithoutWebhookSendsNone() = runTest {
        t.on("POST /api/schedules") { ApiResponse(200, """{"ok":true,"schedule":{"id":"x"}}""") }
        client.schedules.update(Schedule("x", "Nightly", "crundi", action = ScheduleAction("command", command = "make"), `when` = ScheduleWhen("recurring", "02:00", days = listOf(1))))
        assertFalse("webhook" in sent()["schedule"]!!.jsonObject)
    }

    @Test fun rotatesAScheduleSecret() = runTest {
        t.on("POST /api/schedules") { ApiResponse(200, """{"ok":true,"schedule":{"id":"a1","when":{"mode":"webhook"},"webhook":{"key":"K","secret":"NEW","auth":"signature"}}}""") }
        val s = client.schedules.rotateHook("a1", "secret")
        assertEquals("rotateHook", sent()["action"]!!.jsonPrimitive.content)
        assertEquals("secret", sent()["which"]!!.jsonPrimitive.content)
        assertEquals("NEW", s.webhook?.secret)
    }

    @Test fun anOlderServersTriggerGoesBackWithoutTheNewFields() = runTest {
        // Before 1.21.1 a trigger's webhook is only its key.
        t.on("GET /api/factory-triggers", """{"ok":true,"triggers":[{"id":"trg_2c3d4e5f6071","kind":"webhook","webhook":{"key":"k7"},"ifBusy":"queue"}]}""")
            .on("POST /api/factory-triggers") { ApiResponse(200, """{"ok":true,"trigger":{"id":"trg_2c3d4e5f6071"}}""") }
        val tr = client.factories.triggers().single()
        assertNull(tr.webhook?.auth)
        client.factories.saveTrigger(tr)
        val wh = sent()["trigger"]!!.jsonObject["webhook"]!!.jsonObject
        assertTrue(wh.isEmpty())
    }

    @Test fun aSignedTriggerRoundTrips() = runTest {
        t.on("POST /api/factory-triggers") { ApiResponse(200, """{"ok":true,"trigger":{"id":"trg_1"}}""") }
        client.factories.saveTrigger(FactoryTrigger("trg_1", kind = "webhook", ifBusy = "manager",
            webhook = HookSettings("K", "S", "signature", "x-sig", "x-ts", "{{payload.id}}")))
        val tr = sent()["trigger"]!!.jsonObject
        val wh = tr["webhook"]!!.jsonObject
        assertEquals(setOf("auth", "signatureHeader", "timestampHeader", "idempotency"), wh.keys)
        assertEquals("x-ts", wh["timestampHeader"]!!.jsonPrimitive.content)
        assertEquals("manager", tr["ifBusy"]!!.jsonPrimitive.content)
    }

    @Test fun rotatesATriggerKeyTheOldWayAndItsSecretTheNew() = runTest {
        t.on("POST /api/factory-triggers") { ApiResponse(200, """{"ok":true,"trigger":{"id":"trg_1"}}""") }
        client.factories.rotateTrigger("trg_1")
        assertFalse("which" in sent())   // as before 1.21.1
        client.factories.rotateTrigger("trg_1", "secret")
        assertEquals("secret", sent()["which"]!!.jsonPrimitive.content)
    }
}
