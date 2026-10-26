package mv.obitech.crundi.api

import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import mv.obitech.crundi.api.model.ChatState
import mv.obitech.crundi.screens.FakeApi
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/**
 * Claude Code's own summaries over the chat stream (chat-summaries.jsonl):
 * a recap in the snapshot and by event, a batch label patched onto tool
 * entries, an agent's progress line; and the two calls that go with them.
 */
class ChatSummariesStreamTest {
    private val id = "c5d6e7f8a9b0c1d2"

    /** The fixture's history, then its first [events] events. */
    private fun chat(events: Int = Int.MAX_VALUE - 1): ChatState {
        var s = ChatState(id)
        for (line in FakeApi.text("chat-summaries.jsonl").lines().filter { it.isNotBlank() }.take(events + 1)) s = ChatReducer.message(s, CrundiJson.parseToJsonElement(line).jsonObject)
        return s
    }

    @Test fun recapInTheSnapshot() {
        val s = chat(0)
        assertTrue(s.loaded)
        assertEquals("Tidying the parser; the tests are next.", s.recap?.text)
        assertEquals("resumed", s.recap?.why)
        assertEquals(1792400000000, s.recap?.at)
        assertEquals("Where this chat stands", s.recap?.caption)
    }

    @Test fun recapEventsClearAndSet() {
        assertNull(chat(5).recap)
        val r = chat(6).recap!!
        assertEquals("idle", r.why)
        assertEquals("While you were away", r.caption)
        assertTrue(r.text.startsWith("The parser is tidied"))
    }

    @Test fun anEmptyOrBrokenRecapIsNone() {
        val s = chat()
        fun ev(json: String) = ChatReducer.event(s, CrundiJson.parseToJsonElement(json).jsonObject)
        assertNull(ev("""{"type":"recap","recap":{"text":"  ","at":1,"why":"idle"}}""").recap)
        assertNull(ev("""{"type":"recap","recap":"what"}""").recap)
        assertNull(ev("""{"type":"recap"}""").recap)
    }

    @Test fun anOlderServerHasNoRecapAndNoGroups() {
        val s = FakeApi.chat()
        assertNull(s.recap)
        assertTrue(s.messages.all { it.group.isEmpty() && it.groupSummary.isEmpty() })
        assertTrue(s.agents.values.all { it.progress.isEmpty() && it.doing == it.step })
    }

    @Test fun groupFieldsArriveByPatch() {
        val before = chat(0)
        assertTrue(before.messages.all { it.group.isEmpty() })
        val s = chat(3)
        for (eid in listOf("t1", "t2", "t3")) {
            val e = s.messages.first { it.id == eid }
            assertEquals("t1", e.group)
            assertEquals("Read a.txt twice, listed files", e.groupSummary)
            // The rest of the entry is as it was.
            assertEquals("done", e.status)
            assertTrue(e.result!!.isNotEmpty())
        }
        assertEquals("", s.messages.first { it.id == "t4" }.group)
    }

    @Test fun agentProgressIsShownOverItsStep() {
        val a0 = chat(0).agents["toolu_ag1"]!!
        assertEquals("", a0.progress)
        assertEquals("Reading src/parse.js", a0.doing)
        val a = chat(4).agents["toolu_ag1"]!!
        assertEquals("Tracing who calls parse() across src/", a.progress)
        assertEquals("Reading src/lex.js", a.step)
        assertEquals(a.progress, a.doing)
    }

    @Test fun dismissRecapPosts() = runTest {
        val t = FakeTransport().on("POST /api/ui-sessions/$id/dismiss-recap", """{"ok":true}""")
        CrundiClient(t).chats.dismissRecap(id)
        assertEquals("POST", t.requests.single().method)
    }

    @Test fun asideSendsTheQuestionAndAnswers() = runTest {
        val t = FakeTransport().on("POST /api/ui-sessions/$id/aside", """{"ok":true,"answer":"It reads a.txt twice."}""")
        assertEquals("It reads a.txt twice.", CrundiClient(t).chats.aside(id, "What did it read?"))
        val r = t.requests.single()
        assertEquals("What did it read?", r.json!!.jsonObject["question"]!!.jsonPrimitive.content)
        assertTrue("an answer can take up to a minute", r.slow)
    }

    @Test fun asideRefusedSaysWhy() = runTest {
        val t = FakeTransport().on("POST /api/ui-sessions/$id/aside", """{"ok":false,"error":"Session is not running"}""")
        try { CrundiClient(t).chats.aside(id, "Where are we?"); fail("should have thrown") } catch (e: ApiException) {
            assertEquals("Session is not running", e.message)
        }
    }
}
