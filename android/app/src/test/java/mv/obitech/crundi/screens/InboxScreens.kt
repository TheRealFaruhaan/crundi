package mv.obitech.crundi.screens

import mv.obitech.crundi.api.InboxItem
import mv.obitech.crundi.api.InboxLast
import mv.obitech.crundi.api.InboxList
import mv.obitech.crundi.api.InboxMedia
import mv.obitech.crundi.api.InboxPage
import mv.obitech.crundi.api.InboxThread
import mv.obitech.crundi.api.InboxThreadList
import mv.obitech.crundi.api.InboxThreads
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.inbox.InboxThreadScreen
import mv.obitech.crundi.ui.inbox.InboxThreadUi
import mv.obitech.crundi.ui.inbox.InboxThreadsScreen
import mv.obitech.crundi.ui.inbox.InboxThreadsUi
import mv.obitech.crundi.ui.shell.Area
import mv.obitech.crundi.ui.shell.ShellCallbacks
import mv.obitech.crundi.ui.shell.ShellScaffold
import org.junit.Test
import androidx.compose.foundation.layout.padding
import androidx.compose.ui.unit.dp

/** Crundi's inbox: the envelope in the drawer, the threads, a chat's thread, Crundi's own. */
class InboxScreens : ScreenshotTest() {
    private val now = System.currentTimeMillis()
    private val min = 60_000L

    private val threads = InboxThreadList().loaded(InboxThreads(listOf(
        InboxThread("chat:c1", "chat", "Ship the inbox", "crundi", "crundi", "c1", InboxLast("i7", now - 2 * min, "permission", "Allow Bash: npm run deploy?"), unread = 2, count = 9),
        InboxThread("chat:c2", "chat", "Fix login", "faruhaan.com", "faruhaan.com", "c2", InboxLast("i6", now - 14 * min, "photo", "Login page after the fix"), unread = 1, count = 4),
        InboxThread("cru", "cru", "Cru", "", last = InboxLast("i5", now - 48 * min, "report", "What happened today"), count = 31),
        InboxThread("system", "system", "Crundi", "Updates, backups, limits", last = InboxLast("i4", now - 3 * 60 * min, "schedule", "Nightly backup"), count = 112),
        InboxThread("project:ledger", "project", "ledger", "", "ledger", last = InboxLast("i2", now - 26 * 60 * min, "file", "Invoice for October"), count = 3),
        InboxThread("chat:c4", "chat", "Kitchen lights automation", "home-automation", "home-automation", "c4", InboxLast("i1", now - 3 * 24 * 60 * min, "question", "", "Which room should the motion sensor control?"), count = 6),
    ), unread = 3))

    @Test fun drawer() = shoot("inbox-drawer") {
        ShellScaffold(
            data = FakeServices.shellData.copy(capabilities = setOf("inbox"), inboxUnread = 3),
            current = Area.Workbench,
            showChrome = true,
            callbacks = ShellCallbacks(),
            drawerOpen = true,
            onDrawer = {},
        ) { EmptyState("terminal", "Workbench") }
    }

    @Test fun topBarDot() = shoot("inbox-topbar-dot") {
        ShellScaffold(
            data = FakeServices.shellData.copy(capabilities = setOf("inbox"), inboxUnread = 3),
            current = Area.Workbench,
            showChrome = true,
            callbacks = ShellCallbacks(),
            drawerOpen = false,
            onDrawer = {},
        ) { EmptyState("terminal", "Workbench") }
    }

    @Test fun topBarFlash() = shoot("inbox-topbar-flash") {
        androidx.compose.foundation.layout.Box(androidx.compose.ui.Modifier.padding(16.dp)) { mv.obitech.crundi.ui.shell.NewMessageFlash {} }
    }

    @Test fun threadList() = shoot("inbox-threads") { InboxThreadsScreen(InboxThreadsUi(threads)) }

    @Test fun empty() = shoot("inbox-empty") { InboxThreadsScreen(InboxThreadsUi(InboxThreadList().loaded(InboxThreads()))) }

    @Test fun chatThread() = shoot("inbox-thread-chat") {
        val items = listOf(
            InboxItem("i7", now - 2 * min, "permission", "Allow Bash: npm run deploy?", "Claude wants to run `npm run deploy` in **crundi**. Answer it in the chat.", "crundi", "c1", "Ship the inbox", thread = "chat:c1"),
            InboxItem("i6", now - 9 * min, "photo", "The drawer with the envelope", "", "crundi", "c1", "Ship the inbox", listOf(InboxMedia("/api/inbox/media/i6/drawer.png", "drawer.png", "image/png", 106_710)), thread = "chat:c1"),
            InboxItem("i5", now - 31 * min, "file", "Release notes", "", "crundi", "c1", "Ship the inbox", listOf(InboxMedia("/api/inbox/media/i5/notes.md", "release-notes-1.22.md", "text/markdown", 4_812)), read = true, thread = "chat:c1"),
            InboxItem("i3", now - 48 * min, "message", "Build finished", "All 787 tests pass.", "crundi", "c1", "Ship the inbox", read = true, thread = "chat:c1"),
        )
        InboxThreadScreen(InboxThreadUi("chat:c1", threads.byKey("chat:c1"), InboxList().first(InboxPage(items, 2), limit = 30)))
    }

    @Test fun systemThread() = shoot("inbox-thread-system") {
        val items = listOf(
            InboxItem("s5", now - 3 * 60 * min, "schedule", "Nightly backup", "Backed up 14 projects (2.1 GB) to the NAS.", thread = "system", read = true),
            InboxItem("s4", now - 7 * 60 * min, "system", "Usage limit reset", "The 5-hour limit has reset. Paused chats can carry on.", thread = "system", read = true),
            InboxItem("s3", now - 26 * 60 * min, "system", "Crundi 1.21.6 is out", "Update from Settings > Server when nothing is running.", thread = "system", read = true),
            InboxItem("s2", now - 2 * 24 * 60 * min, "alert", "Disk almost full", "/home is at 91%. Media and old logs can be cleared in Settings > Machine.", thread = "system", read = true),
        )
        InboxThreadScreen(InboxThreadUi("system", threads.byKey("system"), InboxList().first(InboxPage(items, 0), limit = 30)))
    }
}
