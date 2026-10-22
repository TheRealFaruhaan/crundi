package mv.obitech.crundi.ui.workbench

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import mv.obitech.crundi.ui.workspace.git.absPath
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The text each target gets for each kind of dragged thing, as the web's formatDragRef and its notes rule. */
class PaneDropTest {
    @After fun reset() { PaneDrop.cancel(); PaneDrop.targets.clear() }

    @Test fun labelsMatchTheWeb() {
        assertEquals("[File Path: /a/b.js]", PaneDrop.formatRef("file", "/a/b.js"))
        assertEquals("[Folder Path: /a]", PaneDrop.formatRef("folder", "/a"))
        assertEquals("[Kanban Task ID: t1]", PaneDrop.formatRef("kanban-task", "t1"))
        assertEquals("[Kanban Subtask ID: d1]", PaneDrop.formatRef("kanban-subtask", "d1"))
        assertEquals("[Mindmap Idea ID: n1]", PaneDrop.formatRef("mindmap", "n1"))
        assertEquals("[Media Path: /m/x.png]", PaneDrop.formatRef("media", "/m/x.png"))
        assertEquals("plain", PaneDrop.formatRef("other", "plain"))
    }

    @Test fun notesTakePathsPlainAndTheRestLabelled() {
        PaneDrop.start("media", "/m/x.png", "x.png", Offset.Zero)
        assertEquals("/m/x.png", PaneDrop.textFor(DropKind.Notes))
        assertEquals("[Media Path: /m/x.png]", PaneDrop.textFor(DropKind.Terminal))
        PaneDrop.start("kanban-task", "t1", "Task", Offset.Zero)
        assertEquals("[Kanban Task ID: t1]", PaneDrop.textFor(DropKind.Notes))
        assertEquals("[Kanban Task ID: t1]", PaneDrop.textFor(DropKind.Chat))
    }

    @Test fun dropGoesToTheTargetUnderTheFinger() {
        PaneDrop.targets["term:a"] = PaneDrop.Target(DropKind.Terminal, Rect(0f, 0f, 100f, 100f))
        PaneDrop.targets["notes:b"] = PaneDrop.Target(DropKind.Notes, Rect(100f, 0f, 200f, 100f))
        PaneDrop.start("file", "/x", "x", Offset(150f, 50f))
        assertTrue(PaneDrop.hovered("notes:b"))
        assertTrue(PaneDrop.drop())
        assertNull(PaneDrop.text)
        PaneDrop.start("file", "/x", "x", Offset(300f, 50f))
        assertFalse(PaneDrop.drop())
    }

    @Test fun gitPathsBecomeAbsolute() {
        assertEquals("/home/p/src/a.kt", absPath("/home/p/", "src/a.kt"))
        assertEquals("C:\\p\\src\\a.kt", absPath("C:\\p", "src/a.kt"))
        assertEquals("/abs/a.kt", absPath("/home/p", "/abs/a.kt"))
    }
}
