package mv.obitech.crundi.ui.workbench

import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject
import mv.obitech.crundi.api.ApiRequest
import mv.obitech.crundi.api.ApiResponse
import mv.obitech.crundi.api.CrundiClient
import mv.obitech.crundi.api.FakeTransport
import mv.obitech.crundi.api.model.ResumableSession
import mv.obitech.crundi.screens.FakeApi
import mv.obitech.crundi.ui.shell.Host
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The session chooser formats and behaves as the web's (openSessionChooser, renderChatResume, launchTerminal). */
class SessionChooserTest {
    private val now = java.time.Instant.parse("2026-10-21T12:00:00Z").toEpochMilli()

    @Test fun size() {
        assertEquals("142.0k tokens", sessionSize(ResumableSession("a", tokens = 142_000)))
        assertEquals("38.5k tokens", sessionSize(ResumableSession("a", tokens = 38_456)))
        // No token count: the file's size, never under 1 KB.
        assertEquals("891 KB", sessionSize(ResumableSession("a", sizeBytes = 912_334)))
        assertEquals("1 KB", sessionSize(ResumableSession("a", sizeBytes = 100)))
    }

    @Test fun age() {
        assertEquals("just now", relTime("2026-10-21T11:59:30Z", now))
        assertEquals("5m ago", relTime("2026-10-21T11:55:00Z", now))
        assertEquals("3h ago", relTime("2026-10-21T08:30:00Z", now))
        assertEquals("2d ago", relTime("2026-10-19T10:00:00Z", now))
        assertEquals("29d ago", relTime("2026-09-22T11:00:00Z", now))
        // After 30 days, a date (in the phone's own format).
        assertFalse(relTime("2026-08-01T00:00:00Z", now).endsWith("ago"))
        assertEquals("", relTime("", now))
        // From the future (a clock ahead): just now, as the web.
        assertEquals("just now", relTime("2026-10-21T12:05:00Z", now))
    }

    @Test fun heavyFromHundredThousand() {
        assertFalse(ResumableSession("a", tokens = 99_999).heavy)
        assertTrue(ResumableSession("a", tokens = 100_000).heavy)
    }

    @Test fun meta() {
        val s = ResumableSession("0c9b8a7d-6e5f-4a3b", updatedAt = "2026-10-19T16:20:44.000Z", tokens = 38_000)
        assertEquals("0c9b8a7d · 1d ago · 38.0k tokens", sessionMeta(s, now))
    }

    @Test fun chatTitles() {
        assertEquals("Chat (new)", chatLaunch("p", LaunchMode.Chat, "new", null).title)
        assertEquals("Chat (compacted)", chatLaunch("p", LaunchMode.ChatSkip, "compact", "x").title)
        // Resumed from the chooser: the web leaves the title (skip perms keeps its own).
        assertNull(chatLaunch("p", LaunchMode.Chat, "resume", "x").title)
        assertEquals("Chat (skip perms)", chatLaunch("p", LaunchMode.ChatSkip, "resume", "x").title)
        assertEquals("Chat (resumed)", chatLaunch("p", LaunchMode.Chat, null, "x").title)
        val c = chatLaunch("p", LaunchMode.ChatSkip, "compact", "x")
        assertEquals(true, c.skipPermissions)
        assertEquals("compact", c.sessionMode)
        assertEquals("x", c.resumeId)
        assertNull(chatLaunch("p", LaunchMode.Chat, "new", "").resumeId)
        assertNull(chatLaunch("p", LaunchMode.Chat, "new", null).skipPermissions)
    }

    @Test fun labels() {
        assertEquals("UI Mode", LaunchMode.Chat.chooserLabel)
        assertEquals("Terminal — Skip Permissions", LaunchMode.TerminalSkip.chooserLabel)
    }

    // ─── The controller, over the fake server ───

    private fun TestScope.workbench(t: FakeTransport): WorkbenchController {
        t.on("POST /api/ui-sessions/create", """{"ok":true,"id":"newchat1"}""")
        t.on("POST /api/terminals/create", """{"ok":true,"id":"newterm1"}""")
        val store = WorkbenchStore.memory().also { it.setPending("crundi", listOf(PendingCell("n1"))) }
        return WorkbenchController("crundi", store, CrundiClient(t), this, Host.None)
    }

    private fun FakeTransport.created(path: String): List<ApiRequest> = requests.filter { it.method == "POST" && it.path == path }
    private fun ApiRequest.field(k: String) = (json!!.jsonObject[k] as? JsonPrimitive)?.content
    private val key = "p:n1"

    @Test fun nothingEarlierStartsNew() = runTest {
        val t = FakeApi.transport().on("GET /api/ui-sessions/resumable", """{"ok":true,"sessions":[]}""")
        val wb = workbench(t)
        wb.pick(key, LaunchMode.Chat)
        advanceUntilIdle()
        assertNull(wb.chooser)
        val r = t.created("/api/ui-sessions/create").single()
        assertEquals("new", r.field("sessionMode"))
        assertEquals("Chat (new)", r.field("title"))
    }

    @Test fun heavyInUiModeAsksThenCompacts() = runTest {
        val t = FakeApi.transport()
        val wb = workbench(t)
        wb.pick(key, LaunchMode.ChatSkip)
        advanceUntilIdle()
        val c = wb.chooser!!
        assertEquals(LaunchMode.ChatSkip, c.mode)
        assertEquals(2, c.list.size)
        // A second tap while the dialog is up does nothing.
        wb.pick(key, LaunchMode.Terminal)
        advanceUntilIdle()
        assertEquals(LaunchMode.ChatSkip, wb.chooser!!.mode)
        val heavy = c.list.first { it.heavy }
        wb.choosePick(heavy)
        assertEquals(heavy, wb.chooser!!.heavy)
        assertTrue(t.created("/api/ui-sessions/create").isEmpty())
        wb.chooserBack()
        assertNull(wb.chooser!!.heavy)
        wb.choosePick(heavy)
        wb.chooseResume(heavy, "compact")
        advanceUntilIdle()
        assertNull(wb.chooser)
        val r = t.created("/api/ui-sessions/create").single()
        assertEquals("compact", r.field("sessionMode"))
        assertEquals(heavy.id, r.field("resumeId"))
        assertEquals("true", r.field("skipPermissions"))
    }

    @Test fun terminalResumesHeavyDirectly() = runTest {
        val t = FakeApi.transport()
        val wb = workbench(t)
        wb.pick(key, LaunchMode.TerminalSkip)
        advanceUntilIdle()
        val heavy = wb.chooser!!.list.first { it.heavy }
        wb.choosePick(heavy)
        advanceUntilIdle()
        assertNull(wb.chooser)
        val r = t.created("/api/terminals/create").single()
        assertEquals("resume", r.field("sessionMode"))
        assertEquals(heavy.id, r.field("resumeId"))
        assertEquals("true", r.field("skipPermissions"))
    }

    @Test fun newFromTheChooser() = runTest {
        val t = FakeApi.transport()
        val wb = workbench(t)
        wb.pick(key, LaunchMode.Terminal)
        advanceUntilIdle()
        wb.chooseNew()
        advanceUntilIdle()
        val r = t.created("/api/terminals/create").single()
        assertEquals("new", r.field("sessionMode"))
        assertEquals("", r.field("resumeId"))
        assertNull(r.field("skipPermissions"))
    }

    @Test fun failureStillOffersNew() = runTest {
        val t = FakeApi.transport().on("GET /api/ui-sessions/resumable") { ApiResponse(500, """{"ok":false,"error":"disk unreadable"}""") }
        val wb = workbench(t)
        wb.pick(key, LaunchMode.Chat)
        advanceUntilIdle()
        val c = wb.chooser!!
        assertTrue(c.list.isEmpty())
        assertNotNull(c.failed)
        assertTrue(t.created("/api/ui-sessions/create").isEmpty())
    }

    @Test fun cancelLaunchesNothing() = runTest {
        val t = FakeApi.transport()
        val wb = workbench(t)
        wb.pick(key, LaunchMode.Chat)
        advanceUntilIdle()
        wb.cancelChooser()
        advanceUntilIdle()
        assertNull(wb.chooser)
        assertTrue(t.created("/api/ui-sessions/create").isEmpty())
        assertFalse(wb.launching[key] == true)
    }

    @Test fun shellAsksNothing() = runTest {
        val t = FakeApi.transport()
        val wb = workbench(t)
        wb.pick(key, LaunchMode.Shell)
        advanceUntilIdle()
        assertNull(wb.chooser)
        assertTrue(t.requests.none { it.path == "/api/ui-sessions/resumable" })
        assertEquals("true", t.created("/api/terminals/create").single().field("shell"))
    }
}
