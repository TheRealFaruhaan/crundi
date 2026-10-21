package mv.obitech.crundi.ui

import mv.obitech.crundi.ui.components.Markdown
import mv.obitech.crundi.ui.icons.IconPaths
import mv.obitech.crundi.ui.nav.DeepLinks
import mv.obitech.crundi.ui.nav.Routes
import mv.obitech.crundi.ui.shell.Area
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class FoundationTest {
    /** The icons are the desktop's: every ICON_PATHS name is here, with the same shapes. Run tools/gen-icons.mjs if this fails. */
    @Test
    fun iconsMatchTheDesktop() {
        val html = File("../../src/webapp-html.js").takeIf { it.exists() } ?: return  // only inside the repository
        val text = html.readText()
        val start = text.indexOf("const ICON_PATHS = {")
        val body = text.substring(start, text.indexOf("\n    };", start))
        val desk = Regex("^\\s*'?([A-Za-z0-9_-]+)'?:\\s*'", RegexOption.MULTILINE).findAll(body).map { it.groupValues[1] }.toSet()
        assertTrue("ICON_PATHS not read", desk.size > 40)
        assertEquals("IconPaths.kt is out of date: run node android/tools/gen-icons.mjs", desk, IconPaths.desktop)
        for (n in desk) assertTrue("no shapes for $n", IconPaths.all[n].orEmpty().isNotEmpty())
    }

    @Test
    fun everyAreaHasAnIcon() {
        for (a in Area.entries) assertTrue("${a.key} icon ${a.icon}", IconPaths.all.containsKey(a.icon))
        assertEquals(Area.entries.size, Area.entries.map { it.key }.toSet().size)
    }

    @Test
    fun deepLinks() {
        assertEquals(Routes.chat("abc_1-2"), DeepLinks.route("/chat/abc_1-2"))
        assertEquals("services", DeepLinks.route("/?tab=services"))
        assertNull(DeepLinks.route("/"))
        assertNull(DeepLinks.route("//evil.example/chat/x"))
        assertNull(DeepLinks.route("https://evil.example/chat/x"))
        assertNull(DeepLinks.route("/chat/../x"))
        assertEquals(Area.Chats, Routes.areaOf(Routes.chat("x")))
        assertTrue(Routes.isAreaRoot("files"))
        assertTrue(!Routes.isAreaRoot("files/view"))
    }

    @Test
    fun markdown() {
        val b = Markdown.parse("# Title\n\nSome **bold** text\n\n- a\n- b\n\n```js\nlet x = 1\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |")
        assertEquals(Markdown.Heading(1, "Title"), b[0])
        assertEquals(Markdown.Paragraph("Some **bold** text"), b[1])
        assertEquals(Markdown.ListBlock(false, listOf(0 to "a", 0 to "b")), b[2])
        assertEquals(Markdown.Fence("js", "let x = 1"), b[3])
        assertEquals(Markdown.Table(listOf(listOf("a", "b"), listOf("1", "2"))), b[4])
        assertEquals("Some bold text", Markdown.inline("Some **bold** text").text)
    }
}
