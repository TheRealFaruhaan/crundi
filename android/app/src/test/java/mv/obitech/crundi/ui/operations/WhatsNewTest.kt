package mv.obitech.crundi.ui.operations

import kotlinx.coroutines.test.runTest
import mv.obitech.crundi.api.ApiResponse
import mv.obitech.crundi.api.CrundiClient
import mv.obitech.crundi.api.FakeTransport
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * What's new on the phone, held to the web's openWhatsNew (src/webapp-html.js),
 * app/vendor/crundi-whatsnew.js and the routes in src/webapp.js.
 */
class WhatsNewTest {
    private val payload = """{"ok":true,"version":"1.21.2","steps":[
        {"id":"factory-secrets","version":"1.21.2","tag":"Factories","title":"Secrets a factory may use","body":"Typed once per run.",
         "keys":[{"keys":["Start run"],"text":"asks for each PIN"}],"art":"<div class=\"wna a-fs\"><style>.a-fs{background:var(--bg-card)}</style></div>","artTitle":"Start run"},
        {"id":"skills-from-link","version":"1.21.2","tag":"Skills","title":"Install skills from a GitHub link","body":"Paste a link.","keys":[],"art":"","artTitle":"Skills"}]}"""

    private fun server(json: String = payload, status: Int = 200) = FakeTransport()
        .on("GET /api/whatsnew", json, status)
        .on("POST /api/whatsnew/seen", """{"ok":true}""")

    private fun tour(t: FakeTransport) = CrundiClient(t).system.let { s -> WhatsNewTour({ s.whatsNew(it) }, { s.whatsNewSeen() }) }
    private fun seen(t: FakeTransport) = t.requests.count { it.method == "POST" && it.path == "/api/whatsnew/seen" }
    private fun asked(t: FakeTransport) = t.requests.filter { it.method == "GET" && it.path == "/api/whatsnew" }

    // ─── The payload ───

    @Test fun readsTheStepsWithTheirArt() = runTest {
        val d = CrundiClient(server()).system.whatsNew()
        assertEquals("1.21.2", d.version)
        assertEquals(2, d.steps.size)
        val s = d.steps[0]
        assertEquals("factory-secrets", s.id); assertEquals("Factories", s.tag); assertEquals("Start run", s.artTitle)
        assertTrue(s.art.startsWith("<div class=\"wna a-fs\">"))
        assertEquals(listOf("Start run"), s.keys[0].keys); assertEquals("asks for each PIN", s.keys[0].text)
    }

    @Test fun anOlderPayloadWithoutArtStillReads() = runTest {
        val d = CrundiClient(server("""{"ok":true,"version":"1.19.24","steps":[{"id":"a","tag":"T","title":"x","body":"y"}]}""")).system.whatsNew()
        assertEquals("", d.steps[0].art); assertEquals("", d.steps[0].artTitle); assertTrue(d.steps[0].keys.isEmpty())
    }

    // ─── When it shows ───

    @Test fun showsOnceAfterConnectingWhenThereAreSteps() = runTest {
        val t = server(); val w = tour(t)
        assertTrue(w.autoShow())
        assertNotNull(w.open); assertFalse(w.open!!.all); assertEquals(0, w.at); assertEquals(3, w.pages)
        assertNull(asked(t).single().query["all"])
        // Once per launch: closed, it is not offered again.
        w.close(WhatsNewClose.Skip)
        assertFalse(w.autoShow())
        assertNull(w.open); assertEquals(1, asked(t).size)
    }

    @Test fun nothingNewShowsNothing() = runTest {
        val w = tour(server("""{"ok":true,"version":"1.21.2","steps":[]}"""))
        assertFalse(w.autoShow()); assertNull(w.open); assertTrue(w.offered)
    }

    @Test fun anOlderServerOrAnErrorShowsNothingAndSaysNothing() = runTest {
        // No such route (an older server): FakeTransport answers 404.
        val old = FakeTransport(); val w = tour(old)
        assertFalse(w.autoShow()); assertNull(w.open)
        // The server's own failure, with 200 and ok:false.
        val bad = tour(server("""{"ok":false,"error":"boom","steps":[]}"""))
        assertFalse(bad.autoShow()); assertNull(bad.open)
        // A server that is down.
        val down = tour(FakeTransport().on("GET /api/whatsnew") { throw java.io.IOException("offline") })
        assertFalse(down.autoShow()); assertNull(down.open)
        // A failed ask is not "offered": the next window tries again.
        assertFalse(down.offered)
        assertEquals(0, seen(old))
    }

    @Test fun aCollaboratorGetsNoStepsSoSeesNothing() = runTest {
        val w = tour(server("""{"ok":true,"version":"1.21.2","steps":[]}"""))
        assertFalse(w.autoShow()); assertNull(w.open)
    }

    // ─── Seen ───

    @Test fun skipMarksItSeen() = runTest {
        val t = server(); val w = tour(t)
        w.autoShow(); w.next()
        w.close(WhatsNewClose.Skip)
        assertNull(w.open); assertEquals(1, seen(t))
    }

    @Test fun doneMarksItSeen() = runTest {
        val t = server(); val w = tour(t)
        w.autoShow(); w.go(99)
        assertEquals(2, w.at)
        assertEquals(0, seen(t))
        w.close(WhatsNewClose.Done)
        assertEquals(1, seen(t))
        // Closing what is already closed sends nothing more.
        w.close(WhatsNewClose.Done)
        assertEquals(1, seen(t))
    }

    @Test fun pagingIsKeptInRange() = runTest {
        val w = tour(server())
        w.autoShow()
        w.back(); assertEquals(0, w.at)
        w.next(); w.next(); w.next(); assertEquals(2, w.at)
        w.go(1); assertEquals(1, w.at)
    }

    @Test fun showItAgainAsksForAllAndMarksNothing() = runTest {
        val t = server(); val w = tour(t)
        assertTrue(w.showAgain())
        assertEquals("1", asked(t).single().query["all"])
        assertTrue(w.open!!.all)
        w.close(WhatsNewClose.Done)
        assertEquals(0, seen(t))
        // And it can be shown again, as often as asked.
        assertTrue(w.showAgain()); w.close(WhatsNewClose.Skip)
        assertEquals(0, seen(t))
    }

    @Test fun showItAgainWithNothingToShowSaysSo() = runTest {
        assertFalse(tour(server("""{"ok":true,"version":"1.21.2","steps":[]}""")).showAgain())
        assertFalse(tour(FakeTransport()).showAgain())
    }

    @Test fun showItAgainWhileOpenFromStartStartsAgain() = runTest {
        val t = server(); val w = tour(t)
        w.autoShow(); w.next()
        w.showAgain()
        assertEquals(0, w.at); assertTrue(w.open!!.all)
    }

    @Test fun swipesMoveOnePage() {
        assertEquals(1, swipeTarget(0, 3, -80f, 55f))
        assertEquals(0, swipeTarget(1, 3, 80f, 55f))
        assertEquals(1, swipeTarget(1, 3, 20f, 55f))
        assertEquals(2, swipeTarget(2, 3, -80f, 55f))
        assertEquals(0, swipeTarget(0, 3, 80f, 55f))
    }

    // ─── The animation's page ───

    @Test fun thePageSetsTheThemeVariablesAndHasNoScript() {
        val page = WhatsNewArt.page("<div class=\"wna a-x\"><style>.a-x{color:var(--accent)}</style></div>", 0.5f)
        for (v in listOf("--bg-primary", "--bg-secondary", "--bg-card", "--border", "--text-primary", "--text-secondary", "--text-muted", "--accent", "--accent-hover", "--accent-dim", "--green", "--yellow", "--yellow-dim", "--mono"))
            assertTrue("$v is set", Regex(Regex.escape(v) + ":[^;}]+").containsMatchIn(page))
        assertTrue(page.contains("--bg-primary:#0a0a0f"))
        assertTrue(page.contains("--accent:#6366f1"))
        assertTrue(page.contains("--accent-dim:rgba(99, 102, 241, 0.15)"))
        assertFalse(page.contains("<script", ignoreCase = true))
        // The 640 x 400 stage, scaled to the box.
        assertTrue(page.contains("width:640px;height:400px"))
        assertTrue(page.contains("scale(0.5000)"))
        assertTrue(page.contains("<div class=\"wna a-x\">"))
        // Nothing may load: no scripts, no fetches.
        assertTrue(page.contains("default-src 'none'"))
    }

    @Test fun aScriptOrHandlerInTheArtIsTakenOut() {
        val page = WhatsNewArt.page("<div class=\"wna a-x\" onclick=\"alert(1)\"><SCRIPT>alert(2)</SCRIPT><img src=x onerror='alert(3)'><style>.a-x{}</style></div>", 1f)
        assertFalse(page.contains("<script", ignoreCase = true))
        assertFalse(page.contains("alert"))
        assertTrue(page.contains("<style>.a-x{}</style>"))
    }

    @Test fun theShippedAnimationsAreWrappedWhole() {
        // Every animation in the repo's editions goes through the wrapper unchanged.
        val root = generateSequence(java.io.File("").absoluteFile) { it.parentFile }.map { java.io.File(it, "app/vendor/whatsnew") }.firstOrNull { it.isDirectory } ?: return
        val files = root.walk().filter { it.extension == "html" }.toList()
        assertTrue(files.isNotEmpty())
        for (f in files) {
            val art = f.readText()
            assertEquals(f.path, art, WhatsNewArt.clean(art))
        }
    }
}
