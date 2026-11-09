package mv.obitech.crundi.api

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Cru's live screen and kid mode as the phone reads them (src/voice/screen.js, kid.js, kid-skills.js, kid-games.js). */
class CruLiveTest {
    private val screenJson = """{"ok":true,"screen":{"title":"Today","rev":7,"at":1,"kid":false,"blocks":[
        {"type":"heading","text":"Plan","id":"h"},
        {"type":"text","text":"Three things"},
        {"type":"list","items":["one","two"],"ordered":true},
        {"type":"checklist","items":[{"text":"tea","done":true},{"text":"mail","done":false}]},
        {"type":"table","rows":[["CPU","12%"],["RAM","3 GB"]],"headers":["What","Now"]},
        {"type":"progress","label":"Build","value":42.5},
        {"type":"image","url":"https://example.com/a.png","alt":"a chart"},
        {"type":"image","url":"http://example.com/plain.png","alt":"not https"},
        {"type":"chips","items":[{"text":"up","tone":"ok"},{"text":"odd","tone":"purple"}]},
        {"type":"sparkline","values":[1,2,3]}
    ]}}"""

    @Test fun aScreenIsReadBlockByBlockAndUnknownOnesAreLeftOut() = runBlocking {
        val s = CrundiClient(FakeTransport().on("GET /api/voice/screen", screenJson)).kid.screen()!!
        assertEquals("Today", s.title)
        assertEquals(7L, s.rev)
        assertFalse(s.framed)
        // The plain-http image and the unknown block type are dropped; the rest kept in order.
        assertEquals(8, s.blocks.size)
        assertEquals(ScreenBlock.Heading("Plan"), s.blocks[0])
        assertEquals(ScreenBlock.Items(listOf("one", "two"), ordered = true), s.blocks[2])
        assertEquals(ScreenBlock.Checklist(listOf(ScreenBlock.Check("tea", true), ScreenBlock.Check("mail", false))), s.blocks[3])
        assertEquals(ScreenBlock.Table(listOf("What", "Now"), listOf(listOf("CPU", "12%"), listOf("RAM", "3 GB"))), s.blocks[4])
        assertEquals(ScreenBlock.Progress("Build", 42.5), s.blocks[5])
        assertEquals(ScreenBlock.Image("https://example.com/a.png", "a chart"), s.blocks[6])
        // A tone this app does not know is shown as info.
        assertEquals(ScreenBlock.Chips(listOf(ScreenBlock.Chip("up", "ok"), ScreenBlock.Chip("odd", "info"))), s.blocks[7])
    }

    @Test fun nothingOnScreenOrAnOlderServerIsNull() = runBlocking {
        assertNull(CrundiClient(FakeTransport().on("GET /api/voice/screen", """{"ok":true,"screen":null}""")).kid.screen())
        assertNull(CrundiClient(FakeTransport()).kid.screen())
        assertNull(LiveScreen.parse(null))
    }

    @Test fun aGameAndAVideoGoInAFrame() {
        val game = LiveScreen.parse(CrundiJson.parseToJsonElement("""{"title":"Letters","blocks":[],"game":{"id":"letters","options":{}},"doc":"<!doctype html><html><head></head><body>x</body></html>","kid":true,"rev":2}"""))!!
        assertTrue(game.framed)
        assertEquals("letters", game.game)
        assertTrue(game.kid)
        val video = LiveScreen.parse(CrundiJson.parseToJsonElement("""{"title":"Song","blocks":[],"video":{"kind":"video","id":"abcdefghijk","title":"Song","embed":"https://www.youtube-nocookie.com/embed/abcdefghijk?rel=0"}}"""))!!
        assertEquals("https://www.youtube-nocookie.com/embed/abcdefghijk?rel=0", video.videoUrl)
        // Any other player address is never loaded.
        val other = LiveScreen.parse(CrundiJson.parseToJsonElement("""{"title":"x","video":{"embed":"https://www.youtube.com/embed/abcdefghijk"}}"""))!!
        assertNull(other.videoUrl)
        assertFalse(other.framed)
    }

    @Test fun theVoiceStreamCarriesTheScreenKidModeAndGameCommands() {
        val e = ListenEvent.parse("""{"type":"screen","screen":{"title":"Hi","blocks":[{"type":"text","text":"x"}],"rev":3}}""") as ListenEvent.Screen
        assertEquals("Hi", e.screen?.title)
        assertEquals(ListenEvent.Screen(null), ListenEvent.parse("""{"type":"screen","screen":null}"""))
        val k = ListenEvent.parse("""{"type":"kid","kid":{"on":true,"since":1000,"pinSet":true,"by":"tray","paused":true,"extraMinutes":15,"playing":"Letters","asks":[]}}""") as ListenEvent.Kid
        assertTrue(k.status.on); assertTrue(k.status.paused); assertEquals(15, k.status.extraMinutes); assertEquals("Letters", k.status.playing)
        assertEquals(ListenEvent.GameCommand("easier"), ListenEvent.parse("""{"type":"game-command","command":{"action":"easier"}}"""))
        assertNull(ListenEvent.parse("""{"type":"game-command","command":{}}"""))
        assertNull(ListenEvent.parse("""{"type":"kid","kid":{}}"""))
    }

    @Test fun kidStatusWithItsQuestionsAndAnOlderServer() = runBlocking {
        val t = FakeTransport().on("GET /api/voice/kid", """{"ok":true,"on":true,"since":1700000000000,"pinSet":true,"by":"settings","paused":false,"extraMinutes":0,"playing":"",
            "asks":[{"id":"kid-1","kind":"video","title":"Sara wants to watch: Blippi","choices":[{"id":"once","label":"Allow once"},{"id":"always","label":"Always allow"},{"id":"no","label":"No"}],"createdAt":5}],
            "fails":0,"lockouts":0,"lockedUntil":0}""")
        val k = CrundiClient(t).kid.status()!!
        assertTrue(k.on)
        assertEquals(listOf("once", "always", "no"), k.asks.single().choices.map { it.id })
        assertEquals(3L, k.minutes(1700000000000 + 3 * 60_000 + 5))
        assertNull(CrundiClient(FakeTransport()).kid.status())
        // Off: the instructions and the parent's name come too.
        val off = CrundiClient(FakeTransport().on("GET /api/voice/kid", """{"ok":true,"on":false,"since":0,"pinSet":false,"paused":false,"extraMinutes":0,"playing":"","asks":[],"instructions":"## About the kid","parentName":"Daddy"}""")).kid.status()!!
        assertFalse(off.on); assertEquals("Daddy", off.parentName); assertEquals("## About the kid", off.instructions)
    }

    @Test fun progressPerAreaWithItsSeriesAndPlayTime() = runBlocking {
        val t = FakeTransport().on("GET /api/voice/kid/progress", """{"ok":true,"range":"week","areas":[
            {"id":"language","label":"Language and literacy","about":"Letters.","level":2.5,"trend":"up","series":[{"date":"2026-11-01","level":2,"mastery":0.3,"plays":4},{"date":"2026-11-02","level":2.5,"mastery":0.5,"plays":6}],"learning":["Letter recognition: D-F"],"mastered":["Letter recognition: A-C"],"skills":[{"id":"letters","label":"Letter recognition","level":3,"mastery":0.5,"last":null}]},
            {"id":"motor","label":"Fine motor","about":"Tracing.","level":0,"trend":"none","series":[],"learning":[],"mastered":[],"skills":[]}
        ],"playTime":[{"date":"2026-11-01","minutes":12.5},{"date":"2026-11-02","minutes":20}],"notes":["Language and literacy improved a lot this week."],"taxonomy":[]}""")
        val p = CrundiClient(t).kid.progress("week")
        assertEquals("week", t.requests.last().query["range"])
        assertEquals(listOf("language"), p.areas.filter { it.played }.map { it.id })
        val a = p.areas.first()
        assertEquals("up", a.trend)
        assertEquals(0.5, a.series.last().mastery, 1e-9)
        assertEquals(listOf("Letter recognition: A-C"), a.mastered)
        assertEquals(33L, p.totalMinutes)
        assertEquals(1, p.notes.size)
    }

    @Test fun gamesAndTheWorkshopsLastRun() = runBlocking {
        val t = FakeTransport().on("GET /api/voice/kid/games", """{"ok":true,"games":[
            {"id":"letters","title":"Letters","skills":["letters"],"ageRange":"3-5","version":2,"createdBy":"builtin","enabled":true,"hasPrev":true,"options":"","uses":[]},
            {"id":"count-ducks","title":"Count the ducks","skills":["counting"],"createdBy":"workshop","enabled":false,"isNew":true,"changedByWorkshop":true,"version":1,"hasPrev":false}
        ],"workshop":{"runs":[{"at":"2026-11-08T02:00:00.000Z","created":["count-ducks"],"updated":[],"failed":["x"],"summary":"One new game."}],"running":false}}""")
            .on("POST /api/voice/kid/games", """{"ok":true,"games":[]}""")
        val g = CrundiClient(t).kid.games()
        assertEquals(2, g.games.size)
        assertTrue(g.games[0].hasPrev)
        assertFalse(g.games[1].enabled)
        assertTrue(g.games[1].isNew)
        val run = g.workshop!!.runs.single()
        assertEquals(1, run.created.size); assertEquals(1, run.failed.size)
        assertTrue(run.atMs > 0)
        CrundiClient(t).kid.game("letters", "rollback")
        // An older server's games list (no workshop) reads too.
        assertNull(CrundiClient(FakeTransport().on("GET /api/voice/kid/games", """{"ok":true,"games":[],"workshop":null}""")).kid.games().workshop)
    }

    @Test fun kidQuestionsInTheApprovalsCarryTheirChoices() = runBlocking {
        val t = FakeTransport().on("GET /api/approvals", """{"items":[
            {"id":"s1","source":"secret","kind":"get","title":"Claude wants a secret","createdAt":1},
            {"id":"kid-a-1","source":"kid","kind":"time","title":"Sara asks for more time","detail":"","choices":[{"id":"add-15","label":"15 more minutes"},{"id":"add-30","label":"30 more minutes"},{"id":"no","label":"No"}],"createdAt":2}
        ],"count":2}""").on("POST /api/voice/kid/ask", """{"ok":true}""")
        val a = CrundiClient(t).approvals.list()
        assertEquals(emptyList<String>(), a.items[0].choices.map { it.id })
        assertEquals(listOf("15 more minutes", "30 more minutes", "No"), a.items[1].choices.map { it.label })
        CrundiClient(t).kid.answer("kid-a-1", "add-15")
        val body = t.requests.last().json.toString()
        assertTrue(body, body.contains("\"choice\":\"add-15\"") && body.contains("\"id\":\"kid-a-1\""))
    }

    @Test fun anOlderServersSettingsHaveNoKidMode() = runBlocking {
        val s = CrundiClient(FakeTransport().on("GET /api/voice/settings", """{"ok":true,"model":"m"}""")).voice.settings()
        assertNull(s.kidVoice); assertNull(s.kidGames)
        val k = CrundiClient(FakeTransport().on("GET /api/voice/settings", """{"ok":true,"kidVoice":"flux-gemma-en","kidChattiness":"chatty","kidPauseMs":4000,"kidReflex":false,"kidGames":{"improve":false,"from":"02:00","to":"04:00"}}""")).voice.settings()
        assertEquals("flux-gemma-en", k.kidVoice); assertEquals("chatty", k.kidChattiness); assertEquals(4000, k.kidPauseMs); assertFalse(k.kidReflex)
        assertEquals(KidGamesSettings(false, "02:00", "04:00"), k.kidGames)
    }

    @Test fun aGamesReflexLineComesBackFromItsEvent() = runBlocking {
        val t = FakeTransport().on("POST /api/voice/kid/play", """{"ok":true,"reflex":{"text":"Well done!","expressivity":1.4,"cached":true}}""")
        assertEquals(KidReflex("Well done!", 1.4), CrundiClient(t).kid.play(mapOf("event" to "correct")))
        assertNull(CrundiClient(FakeTransport().on("POST /api/voice/kid/play", """{"ok":true}""")).kid.play(mapOf("event" to "miss")))
        assertNull(CrundiClient(FakeTransport()).kid.play(mapOf("event" to "miss")))
    }
}
