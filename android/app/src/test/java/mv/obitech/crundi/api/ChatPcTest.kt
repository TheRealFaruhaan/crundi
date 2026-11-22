package mv.obitech.crundi.api

import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import mv.obitech.crundi.api.model.ChatState
import mv.obitech.crundi.api.model.NewChat
import mv.obitech.crundi.api.model.Pane
import mv.obitech.crundi.ui.components.ErrorWords
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/** Whole chats on a worker PC: `where` and `pc` on a chat, "Runs on", and "Bring changes back" (src/claude-ui.js, src/workers.js). */
class ChatPcTest {
    private fun obj(s: String) = CrundiJson.parseToJsonElement(s).jsonObject

    @Test fun aSessionOnAPcIsRead() {
        val p = CrundiJson.decodeFromString<Pane>(
            """{"id":"u1","project":"crundi","kind":"ui","where":"worker:w1","pc":{"where":"worker:w1","name":"Studio PC","online":false,"branch":"crundi/u1","worktree":"/srv/wt/u1"}}""",
        )
        val pc = p.runsOn!!
        assertEquals("Studio PC", pc.name)
        assertFalse(pc.online)
        assertEquals("crundi/u1", pc.branch)
        assertEquals("on Studio PC · PC offline", pc.badge)
        assertEquals("on Studio PC", pc.copy(online = true).badge)

        val c = ChatReducer.history("u1", obj("""{"project":"crundi","status":"running","where":"worker:w1","pc":{"where":"worker:w1","name":"Studio PC","online":true}}"""))
        assertEquals("worker:w1", c.where)
        assertTrue(c.runsOn!!.online)
        // The PC went away: the meta event says so.
        val off = ChatReducer.event(c, obj("""{"type":"meta","pc":{"where":"worker:w1","name":"Studio PC","online":false,"branch":"b1"}}"""))
        assertFalse(off.runsOn!!.online)
        assertEquals("b1", off.runsOn!!.branch)
        // A meta event about something else leaves it as it was.
        assertEquals(off.pc, ChatReducer.event(off, obj("""{"type":"meta","model":"opus"}""")).pc)
    }

    @Test fun aSessionWithoutPcShowsNothing() {
        // An older server, or a chat that runs here.
        val p = CrundiJson.decodeFromString<Pane>("""{"id":"u2","project":"crundi","kind":"ui"}""")
        assertEquals("", p.where)
        assertNull(p.pc)
        assertNull(p.runsOn)
        // where "" with a stray pc is still not on a PC.
        assertNull(CrundiJson.decodeFromString<Pane>("""{"id":"u3","where":"","pc":{"name":"x"}}""").runsOn)
        val c = ChatReducer.history("u2", obj("""{"project":"crundi","status":"running"}"""))
        assertNull(c.runsOn)
        assertNull(ChatState("u2").runsOn)
    }

    @Test fun placesAndWhereAreSentAsTheWebSendsThem() = runBlocking {
        val t = FakeTransport()
            .on("POST /api/workers", """{"ok":true,"places":[{"id":"w1","name":"Studio PC","where":"worker:w1"}]}""")
            .on("POST /api/ui-sessions/create", """{"ok":true,"id":"u9"}""")
        val c = CrundiClient(t)
        val places = c.workers.chatPlaces()
        assertEquals(listOf("worker:w1"), places.map { it.where })
        assertEquals("chat-places", t.requests.last().json!!.jsonObject["action"]!!.jsonPrimitive.content)

        c.chats.create(NewChat(project = "crundi", where = "worker:w1"))
        assertEquals("worker:w1", t.requests.last().json!!.jsonObject["where"]!!.jsonPrimitive.content)
        c.chats.create(NewChat(project = "crundi"))
        assertFalse((t.requests.last().json as JsonObject).containsKey("where"))
    }

    @Test fun anOlderServerOffersNoPlaces() = runBlocking {
        val unknown = CrundiClient(FakeTransport().on("POST /api/workers", """{"ok":false,"error":"Unknown action."}""", 400))
        assertTrue(unknown.workers.chatPlaces().isEmpty())
        val none = CrundiClient(FakeTransport().on("POST /api/workers", """{"error":"Not found"}""", 404))
        assertTrue(none.workers.chatPlaces().isEmpty())
    }

    @Test fun bringingChangesBack() = runBlocking {
        val t = FakeTransport().on("POST /api/workers", """{"ok":true,"branch":"crundi/u1","worktree":"/srv/wt/u1","head":"abc"}""")
        val r = CrundiClient(t).workers.pullChat("u1")
        val sent = t.requests.last().json!!.jsonObject
        assertEquals("chat-pull", sent["action"]!!.jsonPrimitive.content)
        assertEquals("u1", sent["chat"]!!.jsonPrimitive.content)
        assertEquals("Changes brought back on branch crundi/u1 (/srv/wt/u1)", r.words)

        try {
            CrundiClient(FakeTransport().on("POST /api/workers", """{"ok":false,"error":"Unknown action."}""", 400)).workers.pullChat("u1")
            fail("an older server")
        } catch (e: ApiException) { assertEquals(ErrorWords.NOT_SUPPORTED, e.message) }
        try {
            CrundiClient(FakeTransport().on("POST /api/workers", """{"ok":false,"error":"That chat does not run on a PC."}""")).workers.pullChat("u1")
            fail("not a PC chat")
        } catch (e: ApiException) { assertEquals("That chat does not run on a PC.", e.message) }
    }
}
