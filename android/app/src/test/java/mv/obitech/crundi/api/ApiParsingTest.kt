package mv.obitech.crundi.api

import kotlinx.coroutines.test.runTest
import mv.obitech.crundi.api.model.AgentState
import mv.obitech.crundi.api.model.KanbanBoard
import mv.obitech.crundi.screens.FakeApi
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Every area's calls, answered with the server's real response shapes
 * (src/test/resources/api, written from src/webapp.js and the stores it
 * calls), read through the real client.
 */
class ApiParsingTest {
    private val client = CrundiClient(FakeApi.transport())

    @Test fun state() {
        val s = FakeApi.state
        assertEquals(4, s.projects.size)
        assertEquals("/api/project-icon?project=crundi&v=lx3k2a-1f4", s.projects[0].icon)
        assertTrue(s.projects[0].registered)
        assertFalse(s.projects[1].registered)
        val chats = s.chats("crundi")
        assertEquals(listOf("Android API", "Fix login redirect"), chats.map { it.title })
        assertEquals(AgentState.NeedsInput, chats[0].state)
        assertEquals("Typed Kotlin client for the Crundi server API", chats[0].label)
        val pending = chats[0].pendingEntries.single()
        assertEquals("permission", pending.kind)
        assertEquals("req-4b1c", pending.requestId)
        assertTrue(pending.hasSuggestions)
        assertEquals("cd android && ./gradlew --no-daemon testPlayDebugUnitTest", pending.summary)
        assertEquals("manager", chats[1].factory?.role)
        assertEquals(1, s.terminalsOf("faruhaan.com").size)
        assertTrue(s.parked("ledger").single().heavy)
        assertEquals(AgentState.NeedsInput, s.projectState("crundi"))
        assertEquals(AgentState.Idle, s.projectState("faruhaan.com"))
        assertNull(s.projectState("dhivo-socials"))
        assertEquals("active", s.services.first().tunnelStatus)
        assertNull(s.services[1].pid)
        assertEquals("Android team", s.factoryRuns.single().factoryName)
    }

    @Test fun liveCounters() = runTest {
        val u = client.usage.now()
        assertEquals(31, u.fiveHour?.utilization)
        assertTrue(u.week!!.resetsAtMs > 0)
        assertNull(u.weekSonnet)
        val a = client.approvals.list()
        assertEquals(3, a.count)
        assertEquals(listOf("secret", "chat", "collab"), a.items.map { it.source })
        // createdAt is ISO for a secret and epoch ms for the others.
        assertEquals(java.time.Instant.parse("2026-10-21T11:02:44.120Z").toEpochMilli(), a.items[0].createdAtMs)
        assertEquals(1792322590000, a.items[1].createdAtMs)
        val sec = client.secrets.list()
        assertEquals("GITHUB_TOKEN", sec.secrets[0].name)
        assertEquals("GH_TOKEN", sec.requests.single().envName)
    }

    @Test fun chatsAndTerminals() = runTest {
        val r = client.chats.resumable("crundi")
        assertEquals(2, r.size)
        assertTrue(r[0].inUse)
        val sch = client.chats.scheduled("crundi")
        assertNull(sch.items[0].nextFireAt) // limit-reset comes as null
        assertEquals("time", sch.items[1].trigger.type)
        assertEquals("sent", sch.recent.single().status)
    }

    @Test fun workspace() = runTest {
        val f = client.files.list("crundi")
        assertEquals(3, f.entries.count { it.isDir })
        assertEquals("/home/crundi/projects/crundi", f.root)
        assertEquals("src/webapp.js", client.files.search("crundi", "webapp").entries[0].rel)
        assertTrue(client.files.read("crundi", "package.json").content.contains("\"crundi\""))
        val g = client.git.status("crundi")
        assertEquals(2, g.ahead)
        assertEquals(1, g.staged.size)
        assertNull(g.files[1].add)
        assertEquals("untracked", g.files[1].status)
        assertTrue(client.git.diff("crundi", "AppGraph.kt", false).diff.startsWith("diff --git"))
        val m = client.media.list("crundi")
        assertEquals("/api/media/raw/4f3e2d1c0b9a8776", m[0].rawPath)
        assertEquals("task", m[0].link?.type)
        assertNull(m[1].link)
    }

    @Test fun planning() = runTest {
        val b = client.kanban.board("crundi")
        assertEquals(KanbanBoard.TITLES.keys.toList(), b.statuses)
        assertEquals(1, b.column("in_progress").size)
        assertEquals(1 to 2, b.tasks[0].todosDone to b.tasks[0].todosTotal)
        assertEquals("Login flow", b.tasks[0].mindmapNodes.single().text)
        assertEquals(14, client.notes.pages("crundi").single().version)
        val page = client.notes.page("crundi", "n1a2b3c4d5e6f708")
        assertEquals(listOf("h1", "p", "todo", "todo", "code", "divider", "table"), page.blocks.map { it.type })
        assertTrue(page.blocks.last().header)
        assertEquals("bash", page.blocks[4].lang)
        val mm = client.mindmap.get()
        assertEquals(listOf("mm1", "mm3"), mm.roots().map { it.id })
        assertEquals("mm2", mm.children("mm1").single().id)
        assertEquals(1, mm.nodes[0].linkedTaskInfo?.todosDone)
        val s = client.schedules.list("crundi")
        assertEquals(listOf(0, 1, 2, 3, 4), s.schedules[0].`when`.days)
        assertEquals("5h", s.schedules[0].conditions.single().metric)
        assertEquals("service", s.schedules[1].action.kind)
        assertEquals("crundi:docs", s.services.single().key)
    }

    @Test fun factories() = runTest {
        assertTrue(client.factories.list().single().runnable)
        assertEquals("Feature team", client.factories.templates().single().name)
        val d = client.factories.get("fac_android")
        assertEquals("many", d.factory.nodes.first { it.type == "decision" }.decision?.kind)
        assertEquals(3, d.factory.edges.last().maxRounds)
        assertEquals("no-tests", d.validation.warnings.single().code)
        val runs = client.factories.runs("crundi")
        val r = runs.single()
        assertTrue(r.active)
        assertEquals("command", r.tokens[1].wait?.kind)
        assertEquals("a1b2c3d4e5f60718", r.employees["emp_api"]?.chatId)
        assertTrue(r.findings.single().blocking)
        assertEquals("Ship it", r.asks.single().replies[0].label)
        val full = client.factories.run(r.id)
        assertEquals(5, full.snapshot?.nodes?.size)
        assertEquals("handoff", full.history[1].type)
        assertEquals("skip", client.factories.triggers("crundi").single().ifBusy)
    }

    @Test fun operations() = runTest {
        val sv = client.services.list()
        assertEquals(2, sv[1].containers?.size)
        assertEquals(listOf("Serving docs on http://localhost:4811", "[err] (node:48121) ExperimentalWarning: fetch", "--- process exited with code 0 ---"), client.services.logs("crundi:docs"))
        val fw = client.forwards.list()
        assertEquals("dev.faruhaan.com", fw.domain)
        assertFalse(fw.forwards[1].watch)
        assertTrue(client.docker.status().reachable)
        val cs = client.docker.containers(stats = true)
        assertEquals(5432, cs[0].ports[0].public)
        assertNull(cs[1].stats)
        val det = client.docker.container("8f7e6d5c4b3a")
        assertTrue(det.overview.env[1].hidden)
        assertEquals("ledger", det.overview.labels["com.docker.compose.project"])
        assertEquals(2, client.docker.stacks()[0].live)
        assertTrue(client.docker.images()[1].dangling)
        assertEquals("ledger_pg", client.docker.volumes()[0].name)
        assertTrue(client.docker.networks()[0].builtin)
        assertEquals(4, client.docker.engine().cpus)
        assertEquals(120000000L, client.docker.disk().images?.reclaimable)
        val st = client.system.settings()
        assertEquals("always", st.notifyPrefs["needsInput"])
        assertEquals(120, st.autoParkMinutes)
        val stats = client.system.stats()
        assertNull(stats.system!!.cpu.cores[2])
        assertEquals(0.42, stats.system!!.load!!.one, 0.001)
        assertTrue(client.system.update().update.available)
        assertEquals("Pixel 7", client.system.sessions()[0].device?.name)
        assertEquals("warn", client.system.logs()[1].level)
        assertFalse(client.system.channels()[0].available)
        assertEquals(2, client.usage.history().size)
    }
}
