package mv.obitech.crundi.api

import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import mv.obitech.crundi.api.model.Usage
import mv.obitech.crundi.api.model.UsageWindow
import mv.obitech.crundi.screens.FakeApi
import mv.obitech.crundi.ui.components.DotState
import mv.obitech.crundi.ui.shell.Connection
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The live layer over the in-memory server. */
class LiveTest {
    @Test fun sseParser() {
        val p = SseParser()
        val lines = listOf(":" + " ".repeat(10), "event: notes", "data: {\"project\":\"crundi\",", "data: \"id\":\"n1\"}", "", "data: x", "")
        val frames = lines.mapNotNull { p.line(it) }
        assertEquals(listOf(SseFrame("notes", "{\"project\":\"crundi\",\n\"id\":\"n1\"}"), SseFrame("message", "x")), frames)
    }

    @Test fun eventsBecomeTypedFlows() = runBlocking {
        val t = FakeApi.transport()
        val c = CrundiClient(t)
        val got = launch {
            val e = c.events.changes("kanban").first()
            assertEquals("crundi", e.project)
        }
        val st = launch { assertEquals(4, c.events.state.first { it != null }!!.projects.size) }
        kotlinx.coroutines.delay(300)
        t.pushEvent("state", FixtureText.state)
        t.pushEvent("kanban", """{"project":"crundi"}""")
        withTimeout(5_000) { got.join(); st.join() }
        assertEquals(LiveStatus.Connected, c.events.status.value)
    }

    @Test fun terminalAndDockerOverTheSocket() = runBlocking {
        val t = FakeApi.transport()
        val c = CrundiClient(t)
        val term = c.terminal("c3d4e5f607182930")
        val out = launch { assertEquals(listOf("$ ls\r\n", "[Process exited with code 0]\r\n"), term.output.take(2).toList()) }
        kotlinx.coroutines.delay(200)
        assertEquals("""{"type":"subscribe","id":"c3d4e5f607182930"}""", t.socketSent.last())
        term.input("ls\r")
        assertEquals("""{"type":"input","id":"c3d4e5f607182930","data":"ls\r"}""", t.socketSent.last())
        t.pushSocket("""{"type":"output","id":"other","data":"nope"}""")
        t.pushSocket("""{"type":"output","id":"c3d4e5f607182930","data":"$ ls\r\n"}""")
        t.pushSocket("""{"type":"output","id":"c3d4e5f607182930","data":"[Process exited with code 0]\r\n"}""")
        withTimeout(5_000) { out.join() }
        assertEquals(0, TerminalStream.exitCode("[Process exited with code 0]\r\n"))
        kotlinx.coroutines.delay(100)
        assertEquals("""{"type":"unsubscribe","id":"c3d4e5f607182930"}""", t.socketSent.last())

        val logs = launch {
            val ev = c.dockerLogs("8f7e6d5c4b3a", 100).toList()
            assertEquals(2, ev.size)
            assertEquals("ready", (ev[0] as DockerLogEvent.Lines).frames.single().text)
            assertTrue(ev[1] is DockerLogEvent.Ended)
        }
        kotlinx.coroutines.delay(200)
        val follow = CrundiJson.parseToJsonElement(t.socketSent.last()).jsonObject
        assertEquals("docker-logs-follow", follow["type"]!!.jsonPrimitive.content)
        val ref = follow["ref"]!!.jsonPrimitive.content
        t.pushSocket("""{"type":"docker-logs","ref":"$ref","frames":[{"stream":"stdout","text":"ready"}]}""")
        t.pushSocket("""{"type":"docker-logs-end","ref":"$ref"}""")
        withTimeout(5_000) { logs.join() }
    }

    @Test fun shellData() {
        val now = java.time.Instant.parse("2026-10-21T11:00:00Z").toEpochMilli()
        val g = LiveShellSource.glance(Usage(fiveHour = UsageWindow(31, "2026-10-21T14:00:00Z"), week = UsageWindow(46, "2026-10-24T11:00:00Z")), now)!!
        assertEquals(0.31f, g.fiveHourUsed, 1e-4f)
        assertEquals(0.4f, g.fiveHourElapsed, 1e-4f)
        assertEquals(4f / 7f, g.weekElapsed, 1e-4f)
        assertEquals(DotState.NeedsInput, LiveShellSource.dotOf(FakeApi.state.projectState("crundi")))
        assertEquals(Connection.Offline, LiveShellSource.connectionOf(LiveStatus.Offline))
    }

    @Test fun shellSourceFromTheStream() = runBlocking {
        val t = FakeApi.transport()
        val src = LiveShellSource(CrundiClient(t), "dev.faruhaan.com", "faruhaan", null, "project.x")
        val job = launch { src.state.collect {} }
        kotlinx.coroutines.delay(300)
        t.pushEvent("state", FixtureText.state)
        t.pushEvent("approvals", FakeApi.text("approvals.json"))
        val d = withTimeout(5_000) { src.state.first { it.approvals == 3 && it.projects.isNotEmpty() } }
        assertEquals("crundi", d.currentProject)
        assertEquals(DotState.Running, d.projects.first { it.alias == "faruhaan.com" }.state)
        src.selectProject("ledger")
        assertEquals("ledger", withTimeout(5_000) { src.state.first { it.currentProject == "ledger" } }.currentProject)
        job.cancel()
    }

    @Test fun panelsCountAndUpdateBadge() = runBlocking {
        val t = FakeApi.transport()
        val src = LiveShellSource(CrundiClient(t), "dev.faruhaan.com", "faruhaan", null, "project.x")
        val job = launch { src.state.collect {} }
        val d = withTimeout(5_000) { src.state.first { it.panels > 0 && it.tabBadges["settings"] == -1 } }
        assertEquals(2, d.panels)
        // A panel moved out of the top bar: the count follows on the `widget` list event.
        t.on("GET /api/widgets", FakeApi.text("widgets.json").replace("\"slot\": \"chip\"", "\"slot\": \"tab\""))
        t.pushEvent("widget", """{"project":"crundi","id":"test-count","kind":"list"}""")
        assertEquals(0, withTimeout(5_000) { src.state.first { it.panels == 0 } }.panels)
        job.cancel()
    }
}
