package mv.obitech.crundi.ui.planning

import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import mv.obitech.crundi.api.model.KanbanBoard
import mv.obitech.crundi.api.model.KanbanTask
import mv.obitech.crundi.api.model.NoteBlock
import mv.obitech.crundi.api.model.NotePage
import mv.obitech.crundi.api.model.ScheduleWhen
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

    @Test fun agoReadsLikeTheDesktop() {
        val now = 1_800_000_000_000
        assertEquals("just now", ago(now - 10_000, now))
        assertEquals("5m ago", ago(now - 5 * 60_000, now))
        assertEquals("3h ago", ago(now - 3 * 3600_000, now))
        assertEquals("yesterday", ago(now - 30 * 3600_000L, now))
    }
}
