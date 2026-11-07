package mv.obitech.crundi.api

import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import mv.obitech.crundi.screens.FakeApi
import mv.obitech.crundi.ui.nav.DeepLinks
import mv.obitech.crundi.ui.nav.Routes
import mv.obitech.crundi.ui.shell.ShellData
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Crundi's inbox: the API, the live event, the list model, and the badge in the shell. */
class InboxTest {
    private fun obj(s: String): JsonObject = CrundiJson.parseToJsonElement(s).jsonObject

    private val pageJson = """{"ok":true,"unread":2,"items":[
        {"id":"a3","at":3000,"kind":"photo","title":"Screenshot","text":"The login page","project":"crundi","chatId":"c1","chatTitle":"Fix login",
         "media":[{"url":"/api/inbox/media/a3/shot.png","name":"shot.png","mime":"image/png","size":20480}],"read":false,"link":null},
        {"id":"a2","at":2000,"kind":"question","title":"","text":"Which branch?\nmain or dev","project":null,"chatId":null,"read":false},
        {"id":"a1","at":1000,"kind":"something-new","title":"Old","read":true,"extra":42}
    ]}"""

    private fun item(id: String, at: Long, read: Boolean = false) = InboxItem(id, at, read = read)

    @Test fun olderServerWithoutTheCapabilityHidesIt() {
        assertFalse(InboxApi.supported(emptySet()))
        assertFalse(ShellData().hasInbox)
        assertFalse(ShellData(capabilities = setOf("voice-assistant", "chat-voice")).hasInbox)
        assertTrue(ShellData(capabilities = setOf("voice-assistant", "inbox")).hasInbox)
    }

    @Test fun listParsesAndAsksForThePage() = runBlocking {
        val t = FakeTransport().on("GET /api/inbox", pageJson)
        val page = CrundiClient(t).inbox.list(thread = "chat:c1", limit = 30, before = 5000)
        assertEquals(mapOf("thread" to "chat:c1", "limit" to "30", "before" to "5000"), t.requests.last().query)
        assertEquals(2, page.unread)
        assertEquals(listOf("a3", "a2", "a1"), page.items.map { it.id })
        val photo = page.items[0]
        assertEquals("Screenshot", photo.headline)
        assertEquals("crundi · Fix login", photo.where)
        assertTrue(photo.media.single().isImage)
        assertEquals(20480L, photo.media.single().size)
        assertEquals("", photo.link)
        // No title: the text's first line; nulls read as empty.
        val q = page.items[1]
        assertEquals("Which branch?", q.headline)
        assertEquals("", q.project)
        assertEquals("", q.chatId)
        assertEquals("help-circle", InboxItem.iconOf(q.kind))
        // A kind this app does not know still shows, with a plain icon.
        assertEquals("info", InboxItem.iconOf(page.items[2].kind))
        assertTrue(page.items[2].read)
    }

    @Test fun readAndDeleteSendWhatTheServerExpects() = runBlocking {
        val t = FakeTransport().on("POST /api/inbox/read", """{"ok":true}""").on("POST /api/inbox/delete", """{"ok":true}""")
        val api = CrundiClient(t).inbox
        api.markRead(listOf("a1", "a2"))
        assertEquals(obj("""{"ids":["a1","a2"]}"""), t.requests.last().json)
        api.markAllRead()
        assertEquals(obj("""{"all":true}"""), t.requests.last().json)
        api.delete(listOf("a3"))
        assertEquals("/api/inbox/delete", t.requests.last().path)
        assertEquals(obj("""{"ids":["a3"]}"""), t.requests.last().json)
        // A whole thread.
        api.markThreadRead("chat:c1")
        assertEquals("/api/inbox/read", t.requests.last().path)
        assertEquals(obj("""{"thread":"chat:c1"}"""), t.requests.last().json)
        api.deleteThread("system")
        assertEquals("/api/inbox/delete", t.requests.last().path)
        assertEquals(obj("""{"thread":"system"}"""), t.requests.last().json)
        // Nothing to send: no call at all.
        val n = t.requests.size
        api.markRead(emptyList()); api.delete(emptyList())
        assertEquals(n, t.requests.size)
    }

    @Test fun threadsParse() = runBlocking {
        val t = FakeTransport().on("GET /api/inbox/threads", """{"ok":true,"unread":5,"threads":[
            {"key":"chat:c1","kind":"chat","title":"Fix login","subtitle":"faruhaan.com","project":"faruhaan.com","chatId":"c1",
             "last":{"id":"a9","at":9000,"kind":"photo","title":"","text":"Here it is\nafter the fix"},"unread":3,"count":12},
            {"key":"system","kind":"system","title":"","subtitle":null,"last":{"id":"a2","at":2000,"kind":"system","title":"Backup done"},"unread":2,"count":40},
            {"key":"cru","kind":"cru","title":"Cru","last":null,"unread":0,"count":0,"future":true}
        ]}""")
        val r = CrundiClient(t).inbox.threads()
        assertEquals("/api/inbox/threads", t.requests.last().path)
        assertEquals(5, r.unread)
        assertEquals(listOf("chat:c1", "system", "cru"), r.threads.map { it.key })
        val chat = r.threads[0]
        assertEquals("Fix login", chat.name)
        assertEquals("c1", chat.chatId)
        assertEquals("Here it is", chat.last!!.line)
        assertEquals(9000L, chat.at)
        assertEquals(12, chat.count)
        // No title: Crundi's own; no last: at 0.
        assertEquals("Crundi", r.threads[1].name)
        assertEquals("Backup done", r.threads[1].last!!.line)
        assertEquals("", r.threads[1].subtitle)
        assertEquals(0L, r.threads[2].at)
        assertEquals("server", InboxThread.iconOf("system"))
        assertEquals("message-square", InboxThread.iconOf("chat"))
        // The badge is the total.
        assertEquals(5, CrundiClient(t).inbox.unread())
    }

    @Test fun oneItemWithItsThread() = runBlocking {
        val t = FakeTransport().on("GET /api/inbox/a9", """{"ok":true,"unread":3,"item":{"id":"a9","at":9000,"kind":"photo","thread":"chat:c1"}}""")
        val r = CrundiClient(t).inbox.item("a9")
        assertEquals("chat:c1", r.item!!.thread)
        assertEquals(3, r.unread)
    }

    @Test fun threadListModel() {
        val l = InboxThreadList().loaded(InboxThreads(listOf(
            InboxThread("system", "system", last = InboxLast("s", 100), unread = 2),
            InboxThread("chat:c1", "chat", "Fix login", last = InboxLast("x", 900), unread = 3),
            InboxThread("cru", "cru", last = InboxLast("y", 500)),
        ), unread = 5))
        assertTrue(l.loaded)
        // Newest activity first, whatever order the server sent.
        assertEquals(listOf("chat:c1", "cru", "system"), l.threads.map { it.key })
        val read = l.markRead("chat:c1")
        assertEquals(0, read.byKey("chat:c1")!!.unread)
        assertEquals(2, read.unread)
        assertEquals(2, read.markRead("chat:c1").unread)
        val del = l.delete("system")
        assertEquals(listOf("chat:c1", "cru"), del.threads.map { it.key })
        assertEquals(3, del.unread)
        assertEquals(0, l.markAllRead().unread)
        assertTrue(l.markAllRead().threads.all { it.unread == 0 })
    }

    @Test fun threadPagingAndLiveItems() {
        fun ti(id: String, at: Long, thread: String = "chat:c1") = InboxItem(id, at, thread = thread)
        val first = InboxList().first(InboxPage(listOf(ti("c", 30), ti("b", 20)), unread = 4), limit = 2)
        val more = first.older(InboxPage(listOf(ti("a", 10)), unread = 4), limit = 2)
        // Newest first in the model; the screen draws it bottom up (oldest at the top).
        assertEquals(listOf("c", "b", "a"), more.items.map { it.id })
        assertFalse(more.hasMore)
        // A new item of this thread comes in; another thread's is left out.
        val live = more.apply(InboxEvent(5, ti("d", 40)), "chat:c1").apply(InboxEvent(6, ti("z", 50, "system")), "chat:c1")
        assertEquals(listOf("d", "c", "b", "a"), live.items.map { it.id })
        // Which events concern a thread.
        assertTrue(InboxEvent(1, null, "chat:c1").touches("chat:c1"))
        assertFalse(InboxEvent(1, null, "system").touches("chat:c1"))
        assertTrue(InboxEvent(1, null, null).touches("chat:c1"))
        assertFalse(InboxEvent(1, ti("q", 1, "cru")).touches("chat:c1"))
    }

    @Test fun eventParsing() {
        val withItem = InboxEvent.parse(obj("""{"type":"inbox","unread":4,"item":{"id":"x","at":9,"kind":"alert","title":"Disk full","read":false}}"""))!!
        assertEquals(4, withItem.unread)
        assertEquals("x", withItem.item!!.id)
        assertEquals("alert", withItem.item!!.kind)
        val bare = InboxEvent.parse(obj("""{"unread":0}"""))!!
        assertEquals(0, bare.unread)
        assertNull(bare.item)
        assertNull(bare.thread)
        assertEquals("chat:c1", InboxEvent.parse(obj("""{"type":"inbox","unread":2,"thread":"chat:c1"}"""))!!.thread)
        assertEquals("cru", InboxEvent.parse(obj("""{"type":"inbox","unread":2,"item":{"id":"i","thread":"cru"}}"""))!!.thread)
        assertNull(InboxEvent.parse(obj("""{"type":"approvals","unread":2}""")))
        assertNull(InboxEvent.parse(obj("""{"type":"inbox"}""")))
        // A broken item still gives the count.
        assertEquals(1, InboxEvent.parse(obj("""{"type":"inbox","unread":1,"item":{"kind":"alert"}}"""))!!.let { assertNull(it.item); it.unread })
    }

    @Test fun theStreamCarriesInboxEvents() = runBlocking {
        val t = FakeApi.transport()
        val c = CrundiClient(t)
        val got = launch { assertEquals(7, InboxEvent.parse(c.events.changes(InboxEvent.NAME).first().data)!!.unread) }
        withTimeout(10_000) {
            while (got.isActive) { t.pushEvent("inbox", """{"type":"inbox","unread":7}"""); kotlinx.coroutines.delay(100) }
        }
    }

    @Test fun listModelPagesAndMarks() {
        val first = InboxList().first(InboxPage(listOf(item("c", 30), item("b", 20)), unread = 2), limit = 2)
        assertTrue(first.loaded)
        assertTrue(first.hasMore)
        assertEquals(20L, first.oldest)
        // An older page: added below, duplicates dropped; a short page is the last.
        val more = first.older(InboxPage(listOf(item("b", 20), item("a", 10, read = true)), unread = 2), limit = 2)
        assertEquals(listOf("c", "b", "a"), more.items.map { it.id })
        assertTrue(more.hasMore)
        val end = more.older(InboxPage(emptyList(), unread = 2), limit = 2)
        assertFalse(end.hasMore)

        // Reading: once, and the count follows.
        val read = end.markRead(listOf("c"))
        assertTrue(read.byId("c")!!.read)
        assertEquals(1, read.unread)
        assertEquals(1, read.markRead(listOf("c", "a")).unread)
        assertEquals(0, read.markAllRead().unread)
        assertTrue(read.markAllRead().items.all { it.read })

        // Deleting an unread one takes it off the count; a read one does not.
        val del = end.delete(listOf("b"))
        assertEquals(listOf("c", "a"), del.items.map { it.id })
        assertEquals(1, del.unread)
        assertEquals(1, del.delete(listOf("a")).unread)

        // Refreshing keeps the older pages already loaded.
        val again = end.first(InboxPage(listOf(item("d", 40), item("c", 30, read = true)), unread = 1), limit = 2)
        assertEquals(listOf("d", "c", "b", "a"), again.items.map { it.id })
        assertTrue(again.byId("c")!!.read)
        assertFalse(again.hasMore)
        // An empty inbox.
        val none = InboxList().first(InboxPage(emptyList(), 0))
        assertTrue(none.items.isEmpty()); assertFalse(none.hasMore)
    }

    @Test fun listModelFollowsLiveEvents() {
        val l = InboxList().first(InboxPage(listOf(item("b", 20), item("a", 10)), unread = 2))
        val added = l.apply(InboxEvent(3, item("c", 30)))
        assertEquals(listOf("c", "b", "a"), added.items.map { it.id })
        assertEquals(3, added.unread)
        val changed = added.apply(InboxEvent(2, item("b", 20, read = true)))
        assertEquals(3, changed.items.size)
        assertTrue(changed.byId("b")!!.read)
        assertEquals(0, changed.apply(InboxEvent(0, null)).unread)
    }

    @Test fun shellBadgeFromTheServer() = runBlocking {
        val t = FakeApi.transport().on("GET /api/inbox/threads", """{"ok":true,"threads":[],"unread":3}""")
        val src = LiveShellSource(CrundiClient(t), "dev.faruhaan.com", "faruhaan", null, "project.x", about = { "1.22.0" to setOf("inbox") })
        val job = launch { src.state.collect {} }
        try {
            withTimeout(20_000) {
                while (src.state.value.inboxUnread != 3) { t.pushEvent("state", FixtureText.state); kotlinx.coroutines.delay(100) }
                // Live: the count from the event.
                while (src.state.value.inboxUnread != 5) { t.pushEvent("inbox", """{"type":"inbox","unread":5}"""); kotlinx.coroutines.delay(100) }
            }
            assertTrue(src.state.value.hasInbox)
        } finally { job.cancel() }
    }

    @Test fun shellAsksNothingOfAnOlderServer() = runBlocking {
        val t = FakeApi.transport()
        val src = LiveShellSource(CrundiClient(t), "dev.faruhaan.com", "faruhaan", null, "project.x", about = { "1.20.0" to setOf("voice-assistant") })
        val job = launch { src.state.collect {} }
        try {
            withTimeout(20_000) { while (src.state.value.projects.isEmpty()) { t.pushEvent("state", FixtureText.state); kotlinx.coroutines.delay(100) } }
            t.pushEvent("inbox", """{"type":"inbox","unread":5}""")
            kotlinx.coroutines.delay(300)
            assertFalse(src.state.value.hasInbox)
            assertEquals(0, src.state.value.inboxUnread)
            assertTrue(t.requests.none { it.path.startsWith("/api/inbox") })
        } finally { job.cancel() }
    }

    @Test fun notificationsOpenTheThreadAtTheItem() {
        val n = mv.obitech.crundi.Notifier
        // Thread and item: that thread, at that item.
        val both = n.inboxPath(org.json.JSONObject().put("inboxId", "a3").put("inboxThread", "chat:c1"))!!
        assertEquals("/inbox/a3?thread=chat%3Ac1", both)
        assertEquals("inbox/thread?key=chat%3Ac1&open=a3", DeepLinks.route(both))
        // Only the item: the thread screen asks the server for its thread.
        assertEquals("/inbox/a3", n.inboxPath(org.json.JSONObject().put("inboxId", "a3")))
        assertEquals("inbox/thread?open=a3", DeepLinks.route("/inbox/a3"))
        // Only the thread.
        assertEquals("inbox/thread?key=system", DeepLinks.route(n.inboxPath(org.json.JSONObject().put("inboxThread", "system"))))
        // The inbox itself; a bad thread key is ignored.
        assertEquals(Routes.inbox(), DeepLinks.route("/inbox"))
        assertEquals("inbox/thread?open=a3", DeepLinks.route("/inbox/a3?thread=..%2F..%2Fx%20y"))
        // Nothing of the inbox: the notification's own url, as before; chats still open chats.
        assertNull(n.inboxPath(org.json.JSONObject().put("url", "/chat/x")))
        assertNull(n.inboxPath(org.json.JSONObject().put("inboxId", "../etc")))
        assertEquals(Routes.chat("abc"), DeepLinks.route("/chat/abc"))
        assertEquals("inbox/thread", Routes.inboxThread(null, null))
    }
}
