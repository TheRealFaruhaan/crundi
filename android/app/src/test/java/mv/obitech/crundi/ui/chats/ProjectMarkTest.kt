package mv.obitech.crundi.ui.chats

import org.junit.Assert.assertEquals
import org.junit.Test

/** The values src/project-mark.js gives (node, 2026-10-22): the two copies must agree. */
class ProjectMarkTest {
    private fun check(name: String, initials: String, hue: Int, color: String, deep: String) {
        val m = projectMark(name)
        assertEquals("initials of '$name'", initials, m.initials)
        assertEquals("hue of '$name'", hue, m.hue)
        assertEquals("colour of '$name'", color, m.color.hex())
        assertEquals("deep colour of '$name'", deep, m.colorDeep.hex())
    }

    @Test fun projects() {
        check("crundi", "C", 316, "#b93195", "#8c2170")
        check("faruhaan.com", "FC", 270, "#7531b9", "#57218c")
        check("dhivo-socials", "DS", 202, "#3187b9", "#21658c")
        check("ledger", "L", 260, "#5f31b9", "#45218c")
        check("my-cool_project", "MC", 229, "#314ab9", "#21358c")
        check("", "?", 61, "#979929", "#72741b")
        check("Ünïcode Name", "ÜN", 196, "#3195b9", "#21708c")
        check("yellow", "Y", 249, "#4631b9", "#31218c")
    }

    @Test fun chats() {
        assertEquals("FL", chatMark("crundi", "Fix login redirect").initials)
        assertEquals("C", chatMark("crundi", "Chat").initials)
        assertEquals("C", chatMark("crundi", "  ").initials)
        assertEquals("#b93195", chatMark("crundi", "Fix login redirect").color.hex())
    }
}
