package mv.obitech.crundi.voice

import mv.obitech.crundi.api.FrameMessage
import mv.obitech.crundi.api.KidPoint
import mv.obitech.crundi.api.ScreenFilter
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The live screen's frame: what it may load, its bridge, and what it may ask for. */
class ScreenFilterTest {
    @Test fun aPageOrAGameLoadsNothingOverTheNetwork() {
        assertTrue(ScreenFilter.allows("data:image/png;base64,AAAA", video = false))
        assertTrue(ScreenFilter.allows("blob:null/1234", video = false))
        assertTrue(ScreenFilter.allows("about:blank", video = false))
        for (u in listOf("https://example.com/x.js", "http://example.com/", "https://www.youtube-nocookie.com/embed/abc", "https://dev.example.com/api/voice/settings", "file:///sdcard/a", "content://x/y", "javascript:alert(1)", "intent://x", "wss://x"))
            assertFalse(u, ScreenFilter.allows(u, video = false))
    }

    @Test fun aVideoReachesOnlyTheNocookiePlayerAndItsHosts() {
        for (u in listOf("https://www.youtube-nocookie.com/embed/abcdefghijk?rel=0", "https://i.ytimg.com/vi/x/hq.jpg", "https://rr3---sn-x.googlevideo.com/videoplayback?x", "https://yt3.ggpht.com/a.png", "data:image/gif;base64,R0"))
            assertTrue(u, ScreenFilter.allows(u, video = true))
        for (u in listOf("https://www.youtube.com/watch?v=x", "https://evil.com/youtube-nocookie.com", "https://youtube-nocookie.com.evil.com/", "http://www.youtube-nocookie.com/embed/x", "https://dev.example.com/", "https://notytimg.com/x"))
            assertFalse(u, ScreenFilter.allows(u, video = true))
        assertTrue(ScreenFilter.isNocookieEmbed("https://www.youtube-nocookie.com/embed/videoseries?list=PL123"))
        assertFalse(ScreenFilter.isNocookieEmbed("https://www.youtube-nocookie.com/watch?v=x"))
        assertFalse(ScreenFilter.isNocookieEmbed("https://www.youtube-nocookie.com/embed/x\"><script>"))
    }

    @Test fun theBridgeGoesRightAfterTheHead() {
        val doc = "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'\"></head><body>hi</body></html>"
        val out = ScreenFrames.withBridge(doc)
        assertTrue(out.startsWith("<!doctype html><html lang=\"en\"><head>" + ScreenFrames.BRIDGE_JS + "<meta charset"))
        assertTrue(out.endsWith("<body>hi</body></html>"))
        // A fragment without a head: the bridge first.
        assertTrue(ScreenFrames.withBridge("<p>x</p>").startsWith(ScreenFrames.BRIDGE_JS))
        // It never forwards what this side posts in.
        assertTrue(ScreenFrames.BRIDGE_JS.contains("d.cru==='volume'||d.type==='command'"))
    }

    @Test fun commandsAndVolumeArePostedSafely() {
        assertEquals("window.postMessage({type:'command',action:\"easier\"},'*')", ScreenFrames.commandJs("easier"))
        val evil = ScreenFrames.commandJs("x\"});alert(1);({\"")
        assertTrue(evil, evil.contains("\\\"});alert(1);({\\\""))
        assertEquals("window.postMessage({cru:'volume',value:0.55,muted:false},'*')", ScreenFrames.volumeJs(0.55f))
        assertEquals("window.postMessage({cru:'volume',value:1.00,muted:false},'*')", ScreenFrames.volumeJs(7f))
    }

    @Test fun whatThePageMayAskFor() {
        assertEquals(FrameMessage.Say("A"), ScreenFilter.message("""{"cru":"say","text":"A"}"""))
        assertEquals(FrameMessage.Tell("the red one"), ScreenFilter.message("""{"cru":"tell","text":"the red one"}"""))
        assertEquals(300, (ScreenFilter.message("""{"cru":"tell","text":"${"x".repeat(500)}"}""") as FrameMessage.Tell).text.length)
        assertNull(ScreenFilter.message("""{"cru":"say","text":"  "}"""))
        // What this side posts in, and anything else, is nothing.
        assertNull(ScreenFilter.message("""{"cru":"volume","value":1}"""))
        assertNull(ScreenFilter.message("""{"type":"command","action":"next"}"""))
        assertNull(ScreenFilter.message("""{"cru":"navigate","url":"https://x"}"""))
        assertNull(ScreenFilter.message("not json"))
        // Game events, as the web forwards them to /api/voice/kid/play.
        assertEquals(mapOf("event" to "miss", "item" to "B", "detail" to "B"), (ScreenFilter.message("""{"cru":"event","kind":"wrong","detail":"B"}""") as FrameMessage.Game).body)
        assertEquals(mapOf("event" to "level-up", "item" to "", "detail" to ""), (ScreenFilter.message("""{"cru":"event","kind":"level"}""") as FrameMessage.Game).body)
        val r = (ScreenFilter.message("""{"cru":"event","kind":"result","skill":"letters","item":"A","ok":true,"ms":900}""") as FrameMessage.Game).body
        assertEquals("result", r["event"]); assertEquals("letters", r["skill"]); assertEquals(true, r["ok"]); assertEquals(900.0, r["ms"])
        val g = (ScreenFilter.message("""{"type":"game","event":"correct","item":"cat","streak":3}""") as FrameMessage.Game).body
        assertEquals("correct", g["event"]); assertEquals("cat", g["item"]); assertEquals(3.0, g["streak"])
    }

    @Test fun theSparklineFitsItsBox() {
        assertEquals(emptyList<Any>(), sparkPoints(listOf(KidPoint("d", 1.0, 0.5)), 100f, 30f))
        val p = sparkPoints(listOf(KidPoint("a", 1.0, 0.0), KidPoint("b", 2.0, 1.0), KidPoint("c", 2.0, 2.0)), 104f, 34f)
        assertEquals(2f, p[0].x, 0.01f); assertEquals(32f, p[0].y, 0.01f)
        assertEquals(52f, p[1].x, 0.01f); assertEquals(2f, p[1].y, 0.01f)
        // Mastery past 1 is held at the top.
        assertEquals(102f, p[2].x, 0.01f); assertEquals(2f, p[2].y, 0.01f)
    }

    @Test fun theKidLineAndTheFoldsSummary() {
        val k = mv.obitech.crundi.api.KidStatus(on = true, since = 0, extraMinutes = 15, playing = "Letters", paused = true)
        assertEquals("0 min, +15 added · Letters · paused", kidLine(k, 1000))
        assertEquals("On, paused", kidSummary(k))
        assertEquals("No PIN yet", kidSummary(mv.obitech.crundi.api.KidStatus()))
        assertEquals("Off", kidSummary(mv.obitech.crundi.api.KidStatus(pinSet = true)))
    }
}
