package mv.obitech.crundi.api

import kotlinx.serialization.json.jsonObject
import mv.obitech.crundi.api.model.AgentState
import mv.obitech.crundi.api.model.ChatMediaParser
import mv.obitech.crundi.screens.FakeApi
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The chat stream (ui-history, then ui-event), folded as the desktop does. */
class ChatStreamTest {
    @Test fun history() {
        val s = FakeApi.chat(events = 0)
        assertTrue(s.loaded)
        assertEquals("crundi", s.project)
        assertEquals(AgentState.Idle, s.state)
        assertEquals(listOf("compact", "clear", "review", "init"), s.slashCommands.map { it.name })
        assertEquals("Review a pull request", s.slashCommands[2].description)
        assertEquals(10, s.visible.size) // "gone" is not drawn
        assertEquals(812, s.messages[2].tokens)
        val q = s.messages.first { it.kind == "question" }
        assertEquals("Which HTTP library?", q.questions.single().question)
        assertEquals("OkHttp", q.questions.single().options[0].label)
        assertEquals("running", s.agents["toolu_01Ef"]?.status)
        assertEquals("Reading src/kanban-store.js", s.agents["toolu_01Ef"]?.step)
    }

    @Test fun media() {
        val s = FakeApi.chat(events = 0)
        val img = s.messages.first { it.id == "x2" }.media(s.id).single()
        assertEquals("image", img.kind)
        assertEquals("/api/chat-media?session=a1b2c3d4e5f60718&name=3fa9c0d1e2b4a5f6.png", img.path)
        assertEquals(1080 to 2400, img.width to img.height)
        val file = s.messages.first { it.id == "x4" }.media(s.id).single()
        assertEquals("/api/chat-media/file?session=a1b2c3d4e5f60718&id=9e8d7c6b5a4f3e2d", file.path)
        assertEquals("api.md", file.filename)
        // A tool that is not show_*, and a name that is not ours, show nothing.
        assertTrue(s.messages.first { it.id == "x1" }.media(s.id).isEmpty())
        val forged = s.messages.first { it.id == "x2" }.let { it.copy(result = it.result!!.replace("3fa9c0d1e2b4a5f6.png", "../../etc/passwd")) }
        assertTrue(ChatMediaParser.parse(s.id, forged).isEmpty())
    }

    @Test fun streaming() {
        val s = FakeApi.chat(events = 6)
        val a = s.messages.first { it.id == "a2" }
        assertEquals("Writing the tests now.", a.text)
        assertFalse(a.streaming)
        assertEquals(AgentState.Working, s.state)
        assertTrue(s.busy)
    }

    @Test fun waitingOnThePerson() {
        val s = FakeApi.chat(events = 14)
        assertEquals(AgentState.NeedsInput, s.state)
        assertEquals(listOf("p2", "q2"), s.waiting.map { it.id })
        assertTrue(s.waiting[1].questions.single().multiSelect)
        val agent = s.agents["toolu_01Ef"]!!
        assertEquals("completed", agent.status)
        assertEquals("done", agent.messages.single().status)
    }

    @Test fun everyEvent() {
        val s = FakeApi.chat()
        assertEquals("acceptEdits", s.permissionMode)
        assertEquals("proposed", s.plan?.status)
        assertEquals(listOf("compact", "clear", "review", "context"), s.slashCommands.map { it.name })
        assertEquals("allowed", s.messages.first { it.id == "p2" }.status)
        assertTrue(s.pendingInjections.isEmpty()) // queued, then taken
        assertEquals("Run the screenshot tests", s.suggestion?.text)
        assertEquals("BUILD SUCCESSFUL in 1m 12s", s.messages.first { it.id == "x5" }.result)
        assertEquals(0.6011, s.messages.last { it.kind == "result" }.costUsd, 1e-9)
        assertTrue(s.exited)
        assertEquals(0, s.exitCode)
        assertEquals(AgentState.Exited, s.state)
    }

    @Test fun reconnectReplacesEverything() {
        val s = FakeApi.chat()
        val again = ChatReducer.message(s, FakeApi.json("chat-history"))
        assertEquals(FakeApi.chat(events = 0), again)
        assertNull(again.exitCode)
    }

    @Test fun otherChatsAndUnknownEventsAreIgnored() {
        val s = FakeApi.chat(events = 0)
        val other = kotlinx.serialization.json.Json.parseToJsonElement("""{"type":"ui-event","id":"zz","event":{"type":"brand-new","x":1}}""").jsonObject
        assertEquals(s, ChatReducer.message(s, other))
    }
}
