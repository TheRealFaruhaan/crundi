package mv.obitech.crundi.api

import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import mv.obitech.crundi.api.model.ChatSecretGrant
import mv.obitech.crundi.ui.chats.ChatSecretPreauth
import mv.obitech.crundi.ui.chats.ChatSecretsForm
import mv.obitech.crundi.ui.chats.chatMenu
import mv.obitech.crundi.ui.components.ErrorWords
import mv.obitech.crundi.ui.components.errorOrUnsupported
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Secrets pre-authorised for one chat (capability 'chat-secret-preauth'): the wire, and the dialog's logic. */
class ChatSecretsApiTest {
    private val grants = """{"ok":true,"grants":[{"secretId":"s1","name":"DEPLOY","expiresAt":"2026-10-26T12:00:00.000Z","untilChatEnds":false,"grantedAt":"2026-10-26T11:00:00.000Z"}]}"""
    private val t = FakeTransport()
        .on("POST /api/secrets/chat") { ApiResponse(200, grants) }
        .on("GET /api/secrets/chat") { ApiResponse(200, grants) }
        .on("POST /api/secrets") { ApiResponse(200, """{"ok":true,"preauthorised":true}""") }
    private val client = CrundiClient(t)
    private fun sent(): JsonObject = t.requests.last().json!!.jsonObject

    @Test fun grantSendsEachPinOnceWithTheDuration() = runTest {
        val g = client.secrets.grantChat("chat1", linkedMapOf("s1" to "482913", "s2" to "111222"), "8h")
        val b = sent()
        assertEquals("grant", b["action"]!!.jsonPrimitive.content)
        assertEquals("chat1", b["sessionId"]!!.jsonPrimitive.content)
        assertEquals("8h", b["duration"]!!.jsonPrimitive.content)
        val list = b["secrets"]!!.jsonArray.map { it.jsonObject }
        assertEquals(listOf("s1", "s2"), list.map { it["id"]!!.jsonPrimitive.content })
        assertEquals("482913", list[0]["pin"]!!.jsonPrimitive.content)
        assertEquals("DEPLOY", g.single().name)
    }

    @Test fun listAndRevoke() = runTest {
        assertEquals("s1", client.secrets.chatGrants("chat1").single().secretId)
        assertEquals("chat1", t.requests.last().query["sessionId"])
        client.secrets.revokeChat("chat1", "s1")
        assertEquals("revoke", sent()["action"]!!.jsonPrimitive.content)
        assertEquals("s1", sent()["secretId"]!!.jsonPrimitive.content)
        client.secrets.revokeAllChat("chat1")
        assertEquals("revokeAll", sent()["action"]!!.jsonPrimitive.content)
    }

    @Test fun approveOnceSendsNoAllowField() = runTest {
        client.secrets.approve("r1", "482913")
        assertFalse("allowForChat" in sent())
    }

    @Test fun approveAndAllowForThisChat() = runTest {
        assertTrue(client.secrets.approve("r1", "482913", allowForChat = "chat"))
        assertEquals("chat", sent()["allowForChat"]!!.jsonObject["duration"]!!.jsonPrimitive.content)
    }

    @Test fun anOlderServerSaysItNeedsAnUpdate() = runTest {
        val old = CrundiClient(FakeTransport())
        val e = runCatching { old.secrets.grantChat("c", mapOf("s" to "123456"), "1h") }.exceptionOrNull()!!
        assertEquals(ErrorWords.NOT_SUPPORTED, errorOrUnsupported(e))
    }

    @Test fun offeredOnlyWhenTheServerListsIt() {
        assertTrue(ChatSecretPreauth.supported(setOf("handshake", "chat-secret-preauth")))
        assertFalse(ChatSecretPreauth.supported(setOf("handshake", "factory-secrets")))
        assertFalse(ChatSecretPreauth.supported(emptySet()))
    }

    @Test fun describesWhenEachRunsOut() {
        val utc = java.time.ZoneOffset.UTC
        val now = java.time.Instant.parse("2026-10-26T11:18:00Z").toEpochMilli()
        assertEquals("until 12:00 (42 min left)", ChatSecretPreauth.describe(ChatSecretGrant("s", expiresAt = "2026-10-26T12:00:00.000Z"), now, utc))
        assertEquals("until 19:18 (8 h left)", ChatSecretPreauth.describe(ChatSecretGrant("s", expiresAt = "2026-10-26T19:18:00.000Z"), now, utc))
        assertEquals("until 12:23 (1 h 5 min left)", ChatSecretPreauth.describe(ChatSecretGrant("s", expiresAt = "2026-10-26T12:23:00.000Z"), now, utc))
        assertEquals("until this chat ends", ChatSecretPreauth.describe(ChatSecretGrant("s", untilChatEnds = true), now, utc))
    }

    @Test fun runOutGrantsAreNotShown() {
        val now = java.time.Instant.parse("2026-10-26T12:30:00Z").toEpochMilli()
        val list = listOf(ChatSecretGrant("old", expiresAt = "2026-10-26T12:00:00.000Z"), ChatSecretGrant("chat", untilChatEnds = true), ChatSecretGrant("new", expiresAt = "2026-10-26T13:00:00.000Z"))
        assertEquals(listOf("chat", "new"), ChatSecretPreauth.active(list, now).map { it.secretId })
    }

    @Test fun theDialogsForm() {
        var f = ChatSecretsForm()
        assertEquals("Choose at least one secret.", ChatSecretPreauth.problem(f)!!.first)
        f = ChatSecretPreauth.toggle(f, "s1")
        assertEquals("s1", ChatSecretPreauth.problem(f)!!.second)
        f = ChatSecretPreauth.pin(f, "s1", "48a29x13")
        assertEquals("482913", f.pins["s1"])
        assertNull(ChatSecretPreauth.problem(f))
        f = ChatSecretPreauth.toggle(f, "s1")
        assertTrue(f.chosen.isEmpty() && f.pins.isEmpty())
    }

    @Test fun theEventIsReadForThisChatOnly() {
        val obj = CrundiJson.parseToJsonElement("""{"sessionId":"c1","grants":[{"secretId":"s1","name":"A","expiresAt":null,"untilChatEnds":true}]}""").jsonObject
        assertEquals("A", ChatSecretPreauth.fromEvent(obj, "c1")!!.single().name)
        assertNull(ChatSecretPreauth.fromEvent(obj, "c2"))
    }

    @Test fun theMenuHasItWhenAsked() {
        val with = chatMenu(null, false, {}, {}, {}, {}, secrets = {}, secretGrants = 2)
        assertTrue(with.any { it.label == "Secrets for this chat" && it.detail!!.startsWith("2 pre-authorised") })
        assertFalse(chatMenu(null, false, {}, {}, {}, {}).any { it.label == "Secrets for this chat" })
    }
}
