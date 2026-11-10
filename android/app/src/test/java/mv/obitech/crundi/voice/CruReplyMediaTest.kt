package mv.obitech.crundi.voice

import kotlinx.serialization.json.JsonArray
import mv.obitech.crundi.api.CrundiJson
import mv.obitech.crundi.api.ListenEvent
import mv.obitech.crundi.api.ScreenBlock
import mv.obitech.crundi.api.VoiceTurn
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** What Cru shows inside its replies (capability "voice-reply-media"): parsed, kept with the message, merged. */
class CruReplyMediaTest {
    private val items = CrundiJson.parseToJsonElement(
        """[
          {"kind":"card","title":"Services","blocks":[{"type":"table","headers":["Name","State"],"rows":[["api","up"]]},{"type":"progress","label":"Disk","value":62}]},
          {"kind":"image","name":"0123456789abcdef.png","filename":"shot.png","size":10,"w":640,"h":360,"caption":"The header"},
          {"kind":"file","id":"fedcba9876543210","filename":"report.csv","size":18},
          {"kind":"embed","name":"00112233445566aa.embed","title":"Chart","height":160},
          {"kind":"image","name":"../../etc/passwd"},
          {"kind":"card","blocks":[]}
        ]"""
    ) as JsonArray

    @Test fun cardsAndMediaAreReadApartAndForgedNamesDropped() {
        val cards = CruReplyItems.cards(items)
        assertEquals(1, cards.size)
        assertEquals("Services", cards[0].title)
        assertTrue(cards[0].blocks[0] is ScreenBlock.Table)
        val media = CruReplyItems.media(items)
        assertEquals(listOf("image", "file", "embed"), media.map { it.kind })
        assertEquals("/api/chat-media?session=cru-voice&name=0123456789abcdef.png", media[0].path)
        assertEquals("/api/chat-media/file?session=cru-voice&id=fedcba9876543210", media[1].path)
        assertEquals(640, media[0].width)
    }

    @Test fun historyTurnsKeepTheirMediaAndAMediaOnlyReplyCounts() {
        val m = CruChat.fromTurns(listOf(VoiceTurn(1, 1000, "show me", "", "", media = items), VoiceTurn(2, 2000, "", "", "")))
        assertEquals(1, m.size)
        assertEquals(6, m[0].media.size)
        assertTrue(m[0].answered)
    }

    @Test fun anAnswerFillsInItsMedia() {
        val asked = CruChat.asked(emptyList(), "a picture", 1000, "k1")
        val done = CruChat.answered(asked, "k1", "a picture", "", "", 2000, items)
        assertEquals(1, done.size)
        assertEquals(6, done[0].media.size)
        // Answered with media alone: not taken out as unanswered on a cancel.
        assertEquals(1, CruChat.drop(done, "k1").size)
        assertEquals(1, CruChat.add(emptyList(), "", "", "", 3000, items).size)
        assertEquals(0, CruChat.add(emptyList(), "", "", "", 3000, null).size)
    }

    @Test fun aReportOfMediaAloneIsStillAReport() {
        val ev = ListenEvent.parse("""{"type":"report","speak":"","show":"","media":[{"kind":"card","title":"x","blocks":[]}]}""")
        assertNotNull(ev)
        assertEquals(1, (ev as ListenEvent.Report).media!!.size)
        assertEquals(null, ListenEvent.parse("""{"type":"report","speak":"","show":""}"""))
    }
}
