package mv.obitech.crundi

import mv.obitech.crundi.InboxNotify.Route
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Where a push that is also in the inbox goes: its own inbox bubble, the main window, or as before. */
class InboxNotifyTest {
    private fun chatPush(kind: String = "message", prompt: Boolean = false, thread: String? = "chat:c1", id: String? = "a9") = JSONObject()
        .put("kind", kind).put("body", "Here is the screenshot")
        .put("chat", JSONObject().put("id", "c1").put("title", "Fix login").put("project", "faruhaan.com"))
        .apply { if (thread != null) put("inboxThread", thread); if (id != null) put("inboxId", id) }
        .apply { if (prompt) put("prompt", JSONObject().put("requestId", "r1").put("tool", "Bash").put("summary", "npm run deploy")) }

    private fun systemPush(thread: String? = "system", id: String? = "s1") = JSONObject().put("kind", "update").put("title", "Crundi 1.22 is out").put("body", "Update from Settings")
        .apply { if (thread != null) put("inboxThread", thread); if (id != null) put("inboxId", id) }

    @Test fun aMessageWithBubblesOnGetsItsOwnInboxBubble() {
        assertEquals(Route.InboxBubble, InboxNotify.route(chatPush(), bubblesOn = true))
        assertEquals(Route.InboxBubble, InboxNotify.route(systemPush(), bubblesOn = true))
        // Only the item id on a chat's push: the chat's own thread.
        assertEquals(Route.InboxBubble, InboxNotify.route(chatPush(thread = null), bubblesOn = true))
        assertEquals("chat:c1", InboxNotify.thread(chatPush(thread = null)))
    }

    @Test fun aMessageWithBubblesOffOpensTheThreadInTheMainWindow() {
        assertEquals(Route.InboxMain, InboxNotify.route(chatPush(), bubblesOn = false))
        assertEquals(Route.InboxMain, InboxNotify.route(systemPush(), bubblesOn = false))
        assertEquals(Route.InboxMain, InboxNotify.route(systemPush(thread = null), bubblesOn = false))
        // An item with no thread cannot bubble as one: the main window.
        assertEquals(Route.InboxMain, InboxNotify.route(systemPush(thread = null), bubblesOn = true))
    }

    @Test fun requestsStayAsTheyWere() {
        for (on in listOf(true, false)) {
            assertEquals(Route.AsBefore, InboxNotify.route(chatPush(prompt = true), on))
            for (k in listOf("needsInput", "question", "permission", "secretRequest", "collabPermission")) assertEquals(k, Route.AsBefore, InboxNotify.route(chatPush(kind = k), on))
            assertTrue(InboxNotify.isRequest(chatPush(prompt = true)))
        }
        assertFalse(InboxNotify.isRequest(chatPush()))
    }

    @Test fun notAnInboxPushIsAsBefore() {
        for (on in listOf(true, false)) {
            assertEquals(Route.AsBefore, InboxNotify.route(chatPush(thread = null, id = null), on))
            assertEquals(Route.AsBefore, InboxNotify.route(systemPush(thread = null, id = null), on))
            // A bad thread key is ignored, not trusted.
            assertEquals(Route.AsBefore, InboxNotify.route(systemPush(thread = "../x y", id = null), on))
        }
        assertNull(InboxNotify.thread(systemPush(thread = null, id = "s1")))
    }

    @Test fun theThreadsOwnConversation() {
        val r = InboxRef("srv1", "chat:c1", InboxNotify.title("chat:c1", chatPush()), "faruhaan.com")
        assertEquals("Fix login", r.title)
        // Never the chat's own conversation key, so the two bubbles stay apart.
        assertEquals("inbox:srv1:chat:c1", r.key)
        assertFalse(r.key == ChatRef("srv1", "c1", "Fix login", "faruhaan.com", "F", "#000000", "#000000").key)
        assertEquals("c1", r.chatId)
        assertEquals("Crundi", InboxNotify.title("system", systemPush()))
        assertEquals("Cru", InboxNotify.title("cru", JSONObject()))
        assertEquals("ledger", InboxNotify.title("project:ledger", JSONObject()))
        assertEquals("Invoices", InboxNotify.title("project:ledger", JSONObject().put("inboxThreadTitle", "Invoices")))
        assertEquals("", InboxRef("srv1", "system", "Crundi").chatId)
    }
}
