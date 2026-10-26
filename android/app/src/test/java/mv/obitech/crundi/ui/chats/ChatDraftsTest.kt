package mv.obitech.crundi.ui.chats

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The chat's unsent text and half-made answers (ChatDrafts), kept as the web
 * keeps them in claude-chat.js: per chat, per project with the claim rule,
 * per question card, and pruned after 30 days.
 */
class ChatDraftsTest {
    private class MapStore : ChatDrafts.Store {
        val m = LinkedHashMap<String, String>()
        override fun get(key: String) = m[key]
        override fun put(key: String, value: String) { m[key] = value }
        override fun remove(key: String) { m.remove(key) }
        override fun keys(): Collection<String> = m.keys.toList()
    }

    private val store = MapStore()
    private val claims = HashMap<String, String>()
    private var clock = 1_000_000_000_000L
    private val drafts = ChatDrafts(store, "dev.example", claims) { clock }

    @Test fun savedAsTypedAndRestoredWhenTheChatOpens() {
        val a = drafts.Composer("chat1")
        assertEquals("", a.load())
        a.save("half a thought")
        assertEquals("""{"v":"half a thought","t":$clock}""", store.m["crundi_chat_draft_chat1"])
        // Opened again (main window, pane or bubble: the same store).
        assertEquals("half a thought", drafts.Composer("chat1").load())
    }

    @Test fun emptyingTheBoxClearsItSoSendingDoes() {
        val a = drafts.Composer("chat1")
        a.attach("crundi", "")
        a.save("send me")
        assertTrue("crundi_chat_draft_p_crundi" in store.m)
        a.save("")   // sent
        assertFalse("crundi_chat_draft_chat1" in store.m)
        assertFalse("crundi_chat_draft_p_crundi" in store.m)
        a.save("send me")   // the send failed and the text came back
        assertEquals("send me", drafts.Composer("chat1").load())
    }

    @Test fun aBareTextDraftFromBeforeTimestampsStillReads() {
        store.m["crundi_chat_draft_old"] = "plain"
        assertEquals("plain", drafts.Composer("old").load())
    }

    @Test fun aNewChatAdoptsTheProjectsDraft() {
        val a = drafts.Composer("chat1")
        a.attach("crundi", "")
        a.save("typed before a restart")
        a.release()
        // Relaunched under a fresh id, with nothing of its own.
        val b = drafts.Composer("chat2")
        assertEquals("", b.load())
        assertEquals("typed before a restart", b.attach("crundi", ""))
        assertEquals("chat2", claims["dev.example|crundi_chat_draft_p_crundi"])
        assertEquals("typed before a restart", drafts.Composer("chat2").load())   // re-anchored under its own id
    }

    @Test fun aSecondChatCanNeitherStealNorClearTheFirstOnes() {
        val a = drafts.Composer("chat1")
        a.attach("crundi", "")
        a.save("first chat's text")
        val b = drafts.Composer("chat2")
        assertNull("claimed by an open chat: not adopted", b.attach("crundi", ""))
        b.save("second chat's text")
        assertTrue(store.m["crundi_chat_draft_p_crundi"]!!.contains("first chat's text"))
        b.save("")
        assertTrue(store.m["crundi_chat_draft_p_crundi"]!!.contains("first chat's text"))
        // Even with text of its own when it opens, it does not take the claim over.
        val c = drafts.Composer("chat3")
        c.save("third")
        c.attach("crundi", "third")
        assertEquals("chat1", claims["dev.example|crundi_chat_draft_p_crundi"])
        assertTrue(store.m["crundi_chat_draft_p_crundi"]!!.contains("first chat's text"))
        // Once the first one closes, the next to open may have it.
        a.release()
        assertEquals("first chat's text", drafts.Composer("chat4").attach("crundi", ""))
    }

    @Test fun claimsArePerServer() {
        val a = drafts.Composer("chat1")
        a.attach("crundi", "")
        a.save("here")
        val elsewhere = ChatDrafts(MapStore().apply { m["crundi_chat_draft_p_crundi"] = "there" }, "other.example", claims) { clock }
        assertEquals("there", elsewhere.Composer("chat9").attach("crundi", ""))
    }

    @Test fun forgetDropsTheOldIdAfterAMove() {
        val a = drafts.Composer("old")
        a.attach("crundi", "")
        a.save("carried")
        a.forget()
        assertFalse("crundi_chat_draft_old" in store.m)
        assertTrue("the project copy stays for the new id", "crundi_chat_draft_p_crundi" in store.m)
        assertTrue(claims.isEmpty())
    }

    @Test fun answersAreKeptRestoredAndCleared() {
        val picks = listOf(ChatDrafts.Pick(listOf("A", "B"), "", "why", "B"), ChatDrafts.Pick(other = "my own"))
        drafts.saveAnswer("chat1", "e7", picks)
        assertTrue(store.m["crundi_chat_ans_chat1_e7"]!!.startsWith("""{"v":[{"chosen":["A","B"],"other":"","note":"why","noteOpt":"B"}"""))
        assertEquals(picks, drafts.loadAnswer("chat1", "e7"))
        // Nothing picked: removed.
        drafts.saveAnswer("chat1", "e7", listOf(ChatDrafts.Pick(), ChatDrafts.Pick()))
        assertNull(drafts.loadAnswer("chat1", "e7"))
        drafts.saveAnswer("chat1", "e7", picks)
        drafts.clearAnswer("chat1", "e7")   // submitted
        assertNull(drafts.loadAnswer("chat1", "e7"))
    }

    @Test fun anOldBareArrayAnswerStillReads() {
        store.m["crundi_chat_ans_chat1_e1"] = """[{"chosen":["Yes"],"other":"","note":"","noteOpt":""}]"""
        assertEquals(listOf(ChatDrafts.Pick(listOf("Yes"))), drafts.loadAnswer("chat1", "e1"))
    }

    @Test fun questionPicksRoundTrip() {
        val q = QuestionPicks()
        q.chosen[0] = listOf("A"); q.note[0] = "n"; q.noteOpt[0] = "A"; q.other[1] = "own"
        val saved = q.saved(2)
        val back = QuestionPicks().apply { restore(saved, 2) }
        assertEquals(saved, back.saved(2))
        assertEquals("own", back.answer(1))
        // A card with fewer questions than were saved takes only its own.
        assertEquals(1, QuestionPicks().apply { restore(saved, 1) }.saved(1).size)
    }

    @Test fun oldDraftsAndAnswersExpireAfterThirtyDays() {
        drafts.Composer("stale").save("old")
        drafts.saveAnswer("stale", "e1", listOf(ChatDrafts.Pick(listOf("A"))))
        store.m["crundi_chat_draft_bare"] = "untimestamped"
        store.m["something_else"] = """{"v":"x","t":1}"""
        clock += ChatDrafts.TTL_MS - 1
        drafts.Composer("fresh").save("new")
        clock += 2
        drafts.Composer("x").load()   // opening a chat prunes, as the web's mount does
        assertFalse("crundi_chat_draft_stale" in store.m)
        assertFalse("crundi_chat_ans_stale_e1" in store.m)
        assertTrue("crundi_chat_draft_fresh" in store.m)
        assertTrue("untimestamped ones are left alone", "crundi_chat_draft_bare" in store.m)
        assertTrue("not a draft", "something_else" in store.m)
    }
}
