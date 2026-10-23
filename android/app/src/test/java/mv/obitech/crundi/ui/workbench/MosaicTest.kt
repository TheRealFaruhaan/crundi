package mv.obitech.crundi.ui.workbench

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The workbench layout behaves as the web's mosaic does. */
class MosaicTest {
    @Test fun defaults() {
        assertNull((Mosaic.default(emptyList()) as Mosaic.Leaf).key)
        assertEquals("a", (Mosaic.default(listOf("a")) as Mosaic.Leaf).key)
        val row = Mosaic.default(listOf("a", "b", "c")) as Mosaic.Split
        assertEquals(Mosaic.Dir.Row, row.dir)
        assertEquals(listOf("a", "b", "c"), row.keys())
        assertEquals(3, row.columns().size)
    }

    @Test fun syncAddsAndRemoves() {
        var t = Mosaic.default(listOf("a", "b"))
        t = t.sync(listOf("b", "c"))
        assertEquals(listOf("b", "c"), t.keys())
        t = t.sync(listOf("c"))
        assertTrue(t is Mosaic.Leaf && t.key == "c")
        t = null.sync(listOf("x"))
        assertEquals(listOf("x"), t.keys())
    }

    @Test fun newPaneFillsAnEmptySlotFirst() {
        var t = Mosaic.default(listOf("a"))
        t = t.splitLeaf(t.leafOf("a")!!.id, Mosaic.Dir.Col)
        assertEquals(2, t.leaves().size)
        t = t.addKey("b")
        assertEquals(listOf("a", "b"), t.keys())
        assertEquals(1, t.columns().size)  // a column split top and bottom, still one column
    }

    @Test fun splitAndCollapse() {
        var t = Mosaic.default(listOf("a", "b"))
        t = t.splitLeaf(t.leafOf("b")!!.id, Mosaic.Dir.Col)
        assertEquals(2, t.columns().size)
        assertEquals(3, t.leaves().size)
        val empty = t.leaves().first { it.key == null }
        t = t.collapse(empty.id)
        assertEquals(Mosaic.default(listOf("a", "b")).keys(), t.keys())
        assertTrue(t.columns().all { it is Mosaic.Leaf })
    }

    @Test fun assignSwapsOrMoves() {
        var t = Mosaic.default(listOf("a", "b"))
        t = t.assign(t.leafOf("a")!!.id, "b")
        assertEquals(listOf("b", "a"), t.keys())
        t = t.splitLeaf(t.leafOf("a")!!.id, Mosaic.Dir.Row)
        val empty = t.leaves().first { it.key == null }
        t = t.assign(empty.id, "b")
        assertEquals(listOf("a", "b"), t.keys())
        assertEquals(2, t.leaves().size)
    }

    @Test fun moveColumns() {
        val t = Mosaic.default(listOf("a", "b", "c")).moveColumn("c", -1)
        assertEquals(listOf("a", "c", "b"), t.keys())
        assertEquals(1, t.columnOf("c"))
        assertEquals(t.keys(), t.moveColumn("a", -1).keys())
    }

    @Test fun jsonRoundTrip() {
        var t = Mosaic.default(listOf("t:1", "w:2"))
        t = t.splitLeaf(t.leafOf("w:2")!!.id, Mosaic.Dir.Col)
        val s = t.resize((t as Mosaic.Split).id, listOf(30f, 70f))
        val back = Mosaic.fromJson(s.toJson())!!
        assertEquals(s.keys(), back.keys())
        assertEquals(listOf(30f, 70f), (back as Mosaic.Split).sizes)
        assertEquals(s.leaves().size, back.leaves().size)
        assertNull(Mosaic.fromJson("not json"))
    }

    @Test fun storeKeepsLayoutCellsAndChoice() {
        val st = WorkbenchStore.memory()
        st.setTree("p", Mosaic.default(listOf("w:x")))
        st.setCells("p", listOf(WbCell("x", PaneKind.Notes, mapOf("pageId" to "n1"))))
        st.setPending("p", listOf(PendingCell("q")))
        st.layout = WbLayout.Stack
        assertEquals(listOf("w:x"), st.tree("p")!!.keys())
        assertEquals("n1", st.cells("p").single().data["pageId"])
        assertEquals("p:q", st.pending("p").single().key)
        assertEquals(WbLayout.Stack, st.layout)
    }
}
