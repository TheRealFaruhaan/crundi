package mv.obitech.crundi.ui.planning

import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import mv.obitech.crundi.api.model.KanbanBoard
import mv.obitech.crundi.api.model.KanbanTask
import mv.obitech.crundi.api.model.NoteBlock
import mv.obitech.crundi.api.model.NotePage
import mv.obitech.crundi.api.model.FactoryTrigger
import mv.obitech.crundi.api.model.HookSettings
import mv.obitech.crundi.api.model.Schedule
import mv.obitech.crundi.api.model.ScheduleAction
import mv.obitech.crundi.api.model.ScheduleWhen
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PlanningLogicTest {
    @Test fun noteTextRoundTrips() {
        val html = "Production is <strong>main</strong>'s <em>top</em> <code>-dev.N</code> <a href=\"https://x.y/a?b=1&amp;c=2\">link</a> <s>old</s> <u>u</u> a &lt; b"
        val src = NoteText.htmlToSource(html)
        assertEquals("Production is **main**'s *top* `-dev.N` [link](https://x.y/a?b=1&c=2) ~~old~~ __u__ a < b", src)
        assertEquals(NoteText.parseHtml(html), NoteText.parseSource(src))
        assertEquals(NoteText.plain(html), NoteText.plain(NoteText.sourceToHtml(src)))
    }

    @Test fun lonelyMarksStayText() {
        assertEquals("2 * 3 = 6", NoteText.plain(NoteText.sourceToHtml("2 * 3 = 6")))
        assertEquals("a<br>b", NoteText.sourceToHtml("a\nb"))
        assertEquals("a\nb", NoteText.htmlToSource("a<br>b"))
    }

    private fun page(vararg b: NoteBlock) = NotePage("p", "T", b.toList(), version = 3)

    @Test fun enterSplitsAndKeepsListKind() {
        val ed = NoteEditor(page(NoteBlock("a", "bullet", "one two")))
        ed.focus("a", 3)
        ed.edit("a", TextFieldValue("one\n two", TextRange(4)))
        assertEquals(listOf("one", " two"), ed.blocks.map { it.source })
        assertEquals(listOf("bullet", "bullet"), ed.blocks.map { it.type })
        assertEquals(ed.blocks[1].id, ed.focused)
        assertTrue(ed.dirty)
    }

    @Test fun enterOnEmptyListItemEndsTheList() {
        val ed = NoteEditor(page(NoteBlock("a", "todo", "")))
        ed.focus("a")
        ed.edit("a", TextFieldValue("\n", TextRange(1)))
        assertEquals(1, ed.blocks.size)
        assertEquals("p", ed.blocks[0].type)
    }

    @Test fun markdownAtTheStartTurnsTheKind() {
        val ed = NoteEditor(page(NoteBlock("a", "p", "")))
        ed.focus("a")
        ed.edit("a", TextFieldValue("## ", TextRange(3)))
        assertEquals("h2", ed.blocks[0].type)
        ed.edit("a", TextFieldValue("x", TextRange(1)))
        assertEquals("x", ed.blocks[0].source)
    }

    @Test fun backspaceMergesIntoTheBlockAbove() {
        val ed = NoteEditor(page(NoteBlock("a", "p", "ab"), NoteBlock("b", "p", "cd")))
        assertTrue(ed.backspaceAtStart("b"))
        assertEquals(listOf("abcd"), ed.blocks.map { it.source })
        assertEquals(TextRange(2), ed.value.selection)
    }

    @Test fun savedBlocksAreHtml() {
        val ed = NoteEditor(page(NoteBlock("a", "p", "<b>x</b>"), NoteBlock("t", "table", rows = listOf(listOf("<em>h</em>")), header = true)))
        val out = ed.toBlocks()
        assertEquals("<strong>x</strong>", out[0].text)
        assertEquals("<em>h</em>", out[1].rows[0][0])
    }

    @Test fun kanbanMovePutsTheCardWhereItWasDropped() {
        val b = KanbanBoard(tasks = listOf(KanbanTask("a", status = "todo"), KanbanTask("b", status = "todo"), KanbanTask("c", status = "done")))
        assertEquals(listOf("c", "a"), b.moved("c", "todo", 0).column("todo").map { it.id }.take(2))
        assertEquals(listOf("b", "a"), b.moved("a", "todo", 1).column("todo").map { it.id })
        assertEquals(listOf("c", "a"), b.moved("a", "done", 5).column("done").map { it.id })
    }

    @Test fun mindLayoutCentresParentsOnTheirChildren() {
        val kids = mapOf("r" to listOf("a", "b"), "a" to emptyList(), "b" to emptyList())
        val (p, total) = MindLayout.layout(listOf("r"), { kids[it].orEmpty() }) { 40f }
        assertEquals((p["a"]!!.y + p["b"]!!.y) / 2, p["r"]!!.y, 0.01f)
        assertEquals(MindLayout.PAD + MindLayout.COL, p["a"]!!.x, 0.01f)
        assertTrue(p["b"]!!.y - p["a"]!!.y >= 40f + MindLayout.GAP - 0.01f)
        assertTrue(total > p["b"]!!.y)
    }

    @Test fun mindLayoutPushesATallParentDown() {
        val kids = mapOf("r" to listOf("a"))
        val (p, _) = MindLayout.layout(listOf("x", "r"), { kids[it].orEmpty() }) { if (it == "r") 120f else 40f }
        assertTrue(p["r"]!!.y - 60f >= p["x"]!!.y + 20f + MindLayout.GAP - 0.01f)
    }

    @Test fun scheduleSummariesSpeakLikeTheDesktop() {
        assertEquals("Daily 09:00", whenSummary(ScheduleWhen("recurring", "09:00", days = (0..6).toList())))
        assertEquals("Mo,We,Fr 08:30", whenSummary(ScheduleWhen("recurring", "08:30", days = listOf(5, 1, 3))))
        assertEquals("2026-10-25 23:00", whenSummary(ScheduleWhen("once", "23:00", "2026-10-25")))
        assertEquals("no days · 09:00", whenSummary(ScheduleWhen("recurring", "09:00")))
    }

    // ─── Webhooks (server 1.21.1) ───

    @Test fun aCalledScheduleSaysWhenCalled() {
        assertEquals("When called", whenSummary(ScheduleWhen("webhook", "09:00")))
        assertEquals("When called", whenSummary(ScheduleWhen("webhook", "")))
    }

    @Test fun webhooksAreKnownByNameVersionOrData() {
        // 1.21.1 has them but does not list the capability.
        assertTrue(Webhooks.supported("1.21.1", emptySet()))
        assertTrue(Webhooks.supported("1.21.1-dev.12", emptySet()))
        assertTrue(Webhooks.supported("1.22.0", emptySet()))
        assertFalse(Webhooks.supported("1.21.1-dev.11", emptySet()))
        assertFalse(Webhooks.supported("1.21.0", setOf("handshake", "pairing")))
        assertFalse(Webhooks.supported("", emptySet()))
        assertTrue(Webhooks.supported("", setOf("webhooks")))
        assertTrue(Webhooks.inSchedules(listOf(Schedule("a", `when` = ScheduleWhen("webhook")))))
        assertTrue(Webhooks.inSchedules(listOf(Schedule("a", webhook = HookSettings(auth = "key")))))
        assertFalse(Webhooks.inSchedules(listOf(Schedule("a"))))
        assertTrue(Webhooks.inTriggers(listOf(FactoryTrigger("t", webhook = HookSettings("k", auth = "key")))))
        assertFalse(Webhooks.inTriggers(listOf(FactoryTrigger("t", kind = "webhook", webhook = HookSettings("k")))))
    }

    private val hook = Schedule("a1b2c3d4e5f60718", "Alerts", "crundi", action = ScheduleAction("chat", prompt = "Look into {{payload.message}}", session = "resume", sessionId = "8441d374"),
        `when` = ScheduleWhen("webhook", "07:15", "", listOf(2)), webhook = HookSettings("KEY", "SECRET", "signature", "x-sig", "", "{{payload.groupKey}}"))

    @Test fun aCalledScheduleSavesAsCalledWithItsSettings() {
        val f = ScheduleForm(hook)
        assertEquals(null, f.problem(emptyList()))
        val s = f.toSchedule("Indian/Maldives")
        // Not turned into a timed one; the stored time and days go back as they were.
        assertEquals(ScheduleWhen("webhook", "07:15", "", listOf(2)), s.`when`)
        assertEquals(hook.webhook, s.webhook)
        assertEquals("resume", s.action.session)
        assertEquals("8441d374", s.action.sessionId)
    }

    @Test fun editingTheWebhookKeepsTheRest() {
        val f = ScheduleForm(hook)
        f.hook = f.hook.copy(auth = "key", idempotency = "{{payload.id}}")
        val wh = f.toSchedule("").webhook!!
        assertEquals("key", wh.auth)
        assertEquals("{{payload.id}}", wh.idempotency)
        assertEquals("x-sig", wh.signatureHeader)
        assertEquals("KEY", wh.key)
    }

    @Test fun aTimedScheduleFromAnOlderServerGetsNoWebhook() {
        val s = Schedule("s1", "Morning", "crundi", action = ScheduleAction("command", command = "make"), `when` = ScheduleWhen("recurring", "08:30", days = listOf(1, 3)))
        val out = ScheduleForm(s).toSchedule("")
        assertNull(out.webhook)
        assertEquals(ScheduleWhen("recurring", "08:30", "", listOf(1, 3)), out.`when`)
    }

    @Test fun aTimedScheduleKeepsWebhookSettingsItHad() {
        // Switched from When called back to Recurring on the web: its settings stay for next time.
        val s = hook.copy(`when` = ScheduleWhen("recurring", "08:30", days = listOf(1)))
        assertEquals(hook.webhook, ScheduleForm(s).toSchedule("").webhook)
    }

    @Test fun aWhenModeThisAppDoesNotKnowGoesBackAsItCame() {
        val w = ScheduleWhen("cron", "08:30", "2026-11-01", listOf(1, 2))
        assertEquals(w, ScheduleForm(hook.copy(`when` = w)).toSchedule("").`when`)
    }

    @Test fun whenCalledTakesAChatOrAServiceAndNoTime() {
        val f = ScheduleForm(ScheduleForm.blank("crundi")).apply { name = "Hook"; prompt = "Look"; time = "" }
        assertEquals("agent", f.kind)
        f.pickWhen("webhook")
        assertEquals("chat", f.kind)
        assertEquals(null, f.problem(emptyList()))
        f.kind = "command"
        assertEquals("A webhook can run a background chat or a service action", f.problem(emptyList()))
        f.kind = "service"
        assertEquals("Pick a service", f.problem(emptyList()))
        // A new one sends the default settings; the server makes the key and secret.
        f.kind = "chat"
        assertEquals(HookSettings(auth = "key", signatureHeader = "x-crundi-signature", timestampHeader = "", idempotency = ""), f.toSchedule("").webhook)
        f.pickWhen("recurring")
        assertEquals("Set a time", f.problem(emptyList()))
    }

    @Test fun webhookUrlsCarryTheKeyOnlyWhenNotSigned() {
        val key = HookSettings("K1", "S1", "key")
        assertEquals("https://dev.faruhaan.com/api/schedule-hook/a1?key=K1", Webhooks.scheduleUrl("dev.faruhaan.com", "a1", key))
        assertEquals("https://dev.faruhaan.com/api/schedule-hook/a1", Webhooks.scheduleUrl("dev.faruhaan.com", "a1", key.copy(auth = "signature")))
        assertEquals("https://dev.faruhaan.com/api/factory-hook/trigger/trg_1?key=K1", Webhooks.triggerUrl("dev.faruhaan.com", "trg_1", key))
        assertFalse(Webhooks.maskedUrl("https://h/api/schedule-hook/a1?key=K1").contains("K1"))
        assertEquals("https://h/x", Webhooks.maskedUrl("https://h/x"))
    }

    @Test fun aTriggerEditKeepsItsKeyAndSecret() {
        val sent = HookSettings("K", "S", "key", "x-crundi-signature", "", "")
        val e = TriggerHooks.edited(sent, Webhooks.filled(sent).copy(auth = "signature", idempotency = "{{payload.id}}"))
        assertEquals(HookSettings("K", "S", "signature", "x-crundi-signature", "", "{{payload.id}}"), e)
        // An older server's trigger (only a key): the editor fills the rest only if it is shown and changed.
        assertEquals(HookSettings("k", auth = "key", signatureHeader = "x-crundi-signature", timestampHeader = "", idempotency = ""), Webhooks.filled(HookSettings("k")).copy(secret = null))
    }

    @Test fun agoReadsLikeTheDesktop() {
        val now = 1_800_000_000_000
        assertEquals("just now", ago(now - 10_000, now))
        assertEquals("5m ago", ago(now - 5 * 60_000, now))
        assertEquals("3h ago", ago(now - 3 * 3600_000, now))
        assertEquals("yesterday", ago(now - 30 * 3600_000L, now))
    }
}
