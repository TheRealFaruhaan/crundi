package mv.obitech.crundi.voice

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Files for Cru: what goes with a turn, and the owner's messages as chips. */
class CruAttachTest {
    @Test fun aTurnTakesTheReadyOnesAndTheTrayKeepsThoseUploading() {
        val list = listOf(
            CruAtt("a1", "cat.jpg", id = "att_1", kind = "image"),
            CruAtt("a2", "big.mov", up = true),
            CruAtt("a3", "bad.bin", bad = "Too large (50 MB at most)"),
            CruAtt("a4", "https://example.com", ref = "https://example.com"),
        )
        val t = CruAttach.take(list)
        assertEquals(listOf("att_1"), t.attachments)
        assertEquals(listOf("https://example.com"), t.refs)
        assertEquals("[The owner attached: \"cat.jpg\" (image), id att_1.]\n[The owner dropped in: https://example.com]", t.label)
        assertEquals(listOf("a2"), CruAttach.left(list).map { it.key })
    }

    @Test fun theServersLinesShowAsChips() {
        val said = "[The owner attached: \"cat.jpg\" (image, image/jpeg, 1.2 MB), id att_ab12. open_attachment to look at or read it; send_to_chat or start_chat with attachments to pass it on.]\n" +
            "[The owner dropped in: [Kanban Task ID: 42]]\nWhat is this?"
        assertEquals(
            listOf(SaidPart.Chip("cat.jpg", "image", "att_ab12"), SaidPart.Chip("[Kanban Task ID: 42]"), SaidPart.Words("What is this?")),
            CruAttach.parts(said),
        )
        assertEquals("What is this?", CruAttach.plain(said))
        // Files alone: the server's "(nothing said …)" is not shown.
        val alone = "[The owner attached: \"a.pdf\" (pdf, application/pdf, 3 KB), id att_x.]\n(nothing said: look at what was attached and respond to it)"
        assertEquals(listOf(SaidPart.Chip("a.pdf", "pdf", "att_x")), CruAttach.parts(alone))
        // The line shown here meanwhile reads the same way.
        assertEquals(listOf(SaidPart.Chip("cat.jpg", "image", "att_1")), CruAttach.parts("[The owner attached: \"cat.jpg\" (image), id att_1.]"))
        assertEquals(listOf(SaidPart.Words("plain words")), CruAttach.parts("plain words"))
    }

    @Test fun aLiveMessageWithFilesIsMatchedToTheServersCopy() {
        val live = CruChat.add(emptyList(), "[The owner attached: \"cat.jpg\" (image), id att_1.]\nhi", "Hello.", "", 10)
        val got = listOf(CruMsg(20, "[The owner attached: \"cat.jpg\" (image, image/jpeg, 1 KB), id att_1. open_attachment …]\nhi", "Hello.", ""))
        assertEquals(1, CruChat.merge(live, got, older = false).size)
    }

    @Test fun sizesAndPictures() {
        assertEquals("", CruAttach.tooLarge(50L * 1024 * 1024, 50L * 1024 * 1024))
        assertEquals("Too large (50 MB at most)", CruAttach.tooLarge(50L * 1024 * 1024 + 1, 50L * 1024 * 1024))
        assertTrue(CruAttach.previewable("image/JPEG"))
        assertEquals(false, CruAttach.previewable("video/mp4"))
        assertEquals("a b", CruAttach.refOf("  a\n\n b "))
    }
}
