package mv.obitech.crundi.ui.workspace

import mv.obitech.crundi.ui.workspace.git.DiffParser
import mv.obitech.crundi.ui.workspace.git.newFileDiff
import mv.obitech.crundi.ui.workspace.terminal.TermEmulator
import mv.obitech.crundi.ui.workspace.terminal.TermKeys
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.Instant

/** The workspace's pure parts: the terminal emulator, the diff reader, the small formatters. */
class WorkspaceLogicTest {
    private fun emu(cols: Int = 20, rows: Int = 5) = TermEmulator(cols, rows, scrollbackMax = 100)

    @Test fun plainTextAndNewlines() {
        val e = emu()
        e.write("hello\r\nworld")
        assertEquals("hello\nworld", e.screenText())
        assertEquals(5, e.cursorX); assertEquals(1, e.cursorY)
    }

    @Test fun longLinesWrapAndScrollIntoScrollback() {
        val e = emu(10, 3)
        e.write("0123456789abc")
        assertEquals("0123456789\nabc", e.screenText())
        assertTrue(e.line(0).wrapped)
        e.write("\r\nl3\r\nl4\r\nl5")
        assertEquals(2, e.scrollbackSize)
        assertEquals("0123456789", e.line(0).text())
        assertEquals("l3\nl4\nl5", e.screenText())
    }

    @Test fun cursorMovesAndErases() {
        val e = emu()
        e.write("abcdef\u001b[3D\u001b[K")
        assertEquals("abc", e.screenText())
        e.write("\u001b[2;5HX")
        assertEquals("abc\n    X", e.screenText())
        e.write("\u001b[2J")
        assertEquals("", e.screenText())
        e.write("\u001b[H12345\u001b[1;2H\u001b[2P")
        assertEquals("145", e.screenText())
        e.write("\u001b[1;2H\u001b[2@")
        assertEquals("1  45", e.screenText())
    }

    @Test fun coloursAndAttributes() {
        val e = emu()
        e.write("\u001b[1;31mR\u001b[0m\u001b[38;5;208mO\u001b[38;2;1;2;3mT\u001b[44mB")
        val l = e.line(e.scrollbackSize)
        assertEquals(2L, TermEmulator.fg(l.styles[0])) // red is palette 1, stored +1
        assertTrue(TermEmulator.flags(l.styles[0]) and TermEmulator.BOLD != 0L)
        assertEquals(209L, TermEmulator.fg(l.styles[1]))
        assertEquals(TermEmulator.RGB or 0x010203L, TermEmulator.fg(l.styles[2]))
        assertEquals(5L, TermEmulator.bg(l.styles[3]))
        assertEquals(0L, TermEmulator.flags(l.styles[1]))
    }

    @Test fun alternateScreenKeepsTheMainOne() {
        val e = emu()
        e.write("shell\u001b[?1049h\u001b[Hvim")
        assertTrue(e.altScreen)
        assertEquals("vim", e.screenText())
        e.write("\u001b[?1049l")
        assertFalse(e.altScreen)
        assertEquals("shell", e.screenText())
    }

    @Test fun scrollRegionScrollsOnlyInside() {
        val e = emu(10, 4)
        e.write("top\r\na\r\nb\r\nbottom\u001b[2;3r\u001b[3;1H\n")
        assertEquals("top\nb\n\nbottom", e.screenText())
        assertEquals(0, e.scrollbackSize)
    }

    @Test fun answersStatusAndIgnoresUnknownSequences() {
        val e = emu()
        val said = mutableListOf<String>()
        e.respond = { said += it }
        e.write("\u001b[3;4H\u001b[6n\u001b]0;my title\u0007\u001b[?2004h\u001b[>4;1m\u001b[5 qok")
        assertEquals(listOf("\u001b[3;4R"), said)
        assertEquals("my title", e.title)
        assertTrue(e.bracketedPaste)
        assertEquals("ok", e.line(e.scrollbackSize + 2).text().trim())
    }

    @Test fun wideCharactersTakeTwoCells() {
        val e = emu(6, 2)
        e.write("a中b")
        assertEquals(4, e.cursorX)
        assertEquals("a中b", e.screenText())
        e.write("😀") // an emoji, as a surrogate pair
        assertEquals("a中b😀", e.screenText())
    }

    @Test fun splitSequencesAcrossChunks() {
        val e = emu()
        e.write("\u001b["); e.write("31"); e.write("mX")
        assertEquals("X", e.screenText())
        assertEquals(2L, TermEmulator.fg(e.line(e.scrollbackSize).styles[0]))
    }

    @Test fun resizeKeepsTheCursorInView() {
        val e = emu(10, 5)
        e.write("1\r\n2\r\n3\r\n4\r\n5")
        e.resize(8, 3)
        assertEquals(8, e.cols); assertEquals(3, e.rows)
        assertEquals("3\n4\n5", e.screenText())
        assertEquals(2, e.cursorY)
        e.resize(8, 5)
        assertEquals("1\n2\n3\n4\n5", e.screenText())
        val v = e.version
        e.resize(8, 5)
        assertEquals(v, e.version)
    }

    @Test fun keyRowSequences() {
        assertEquals("\u001b[A", TermKeys.arrow('A', app = false))
        assertEquals("\u001bOA", TermKeys.arrow('A', app = true))
        assertEquals("\u0003", TermKeys.ctrl('c'))
        assertEquals("\u001a", TermKeys.ctrl('Z'))
        assertEquals("\u001b", TermKeys.ctrl('['))
    }

    @Test fun diffParsesChunksWithLineNumbers() {
        val diff = """
            diff --git a/src/a.kt b/src/a.kt
            index 1111111..2222222 100644
            --- a/src/a.kt
            +++ b/src/a.kt
            @@ -1,3 +1,4 @@ fun main()
             one
            -two
            +TWO
            +three
             four
            @@ -10,2 +11,2 @@
             ten
            -eleven
            +ELEVEN
        """.trimIndent()
        val f = DiffParser.parse(diff).single()
        assertEquals("src/a.kt", f.name)
        assertEquals(2, f.hunks.size)
        assertEquals(3, f.added); assertEquals(2, f.removed)
        val l = f.hunks[0].lines
        assertEquals(DiffParser.Line(DiffParser.Kind.Removed, "two", 2, null), l[1])
        assertEquals(DiffParser.Line(DiffParser.Kind.Added, "three", null, 3), l[3])
        assertEquals(DiffParser.Line(DiffParser.Kind.Context, "four", 3, 4), l[4])
        assertEquals(12, f.hunks[1].lines[2].newNo)
        val patch = DiffParser.patchFor(f, f.hunks[1])
        assertTrue(patch.startsWith("diff --git a/src/a.kt b/src/a.kt\nindex 1111111..2222222 100644\n--- a/src/a.kt\n+++ b/src/a.kt\n@@ -10,2 +11,2 @@\n"))
        assertFalse(patch.contains("TWO"))
    }

    @Test fun diffOfSeveralFilesAndANewOne() {
        val two = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\ndiff --git a/y b/y\nnew file mode 100644\n--- /dev/null\n+++ b/y\n@@ -0,0 +1 @@\n+y\n"
        val files = DiffParser.parse(two)
        assertEquals(listOf("x", "y"), files.map { it.name })
        val n = DiffParser.parse(newFileDiff("notes.txt", "a\nb\n")).single()
        assertEquals(2, n.added); assertEquals("notes.txt", n.name)
    }

    @Test fun formatters() {
        assertEquals("", formatSize(0))
        assertEquals("412 B", formatSize(412))
        assertEquals("8.0 KB", formatSize(8192))
        assertEquals("2.3 MB", formatSize(2_400_000))
        val now = Instant.parse("2026-10-21T12:00:00Z")
        assertEquals("just now", relativeTime("2026-10-21T11:59:30Z", now))
        assertEquals("5 min ago", relativeTime("2026-10-21T11:55:00Z", now))
        assertEquals("3 h ago", relativeTime("2026-10-21T09:00:00+00:00", now))
        assertEquals("yesterday", relativeTime("2026-10-20T10:00:00Z", now))
        assertNotEquals("", relativeTime("2026-01-01T00:00:00Z", now))
    }

    @Test fun routesCarryTheirAreaKey() {
        assertTrue(WorkspaceRoutes.file("/home/a b/c.kt").startsWith("files/view?path=%2Fhome%2Fa+b%2Fc.kt"))
        assertTrue(WorkspaceRoutes.diff("src/a.kt", true).startsWith("git/"))
        assertTrue(WorkspaceRoutes.terminal("abc").startsWith("terminals/"))
        assertTrue(WorkspaceRoutes.mediaItem("abc").startsWith("media/"))
    }
}
