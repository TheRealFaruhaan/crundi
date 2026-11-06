package mv.obitech.crundi.api

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/** The phone's side of the voice routes (/api/voice/settings, turn, speak, listen), against a server in memory. */
class VoiceApiTest {
    private val settingsJson = """{"ok":true,"model":"claude-haiku-5-5","models":[{"id":"claude-haiku-5-5","label":"Claude Haiku 5.5"}],
        "anthropicKeySet":true,"deepgramKeySet":true,"ttsVoice":"aura-2-draco-en","monthlyLimitUsd":50,
        "spend":{"month":"2026-11","claudeUsd":0.0123,"deepgramUsd":0.002,"turns":7,"totalUsd":0.0143},"limitReached":false}"""

    @Test fun settingsAreRead() = runBlocking {
        val t = FakeTransport().on("GET /api/voice/settings", settingsJson)
        val s = CrundiClient(t).voice.settings()
        assertEquals("aura-2-draco-en", s.ttsVoice)
        assertEquals(7, s.spend.turns)
        assertEquals(50.0, s.monthlyLimitUsd, 0.0)
        assertTrue(s.anthropicKeySet)
    }

    @Test fun aServerWithoutTheAssistantSaysUpdateTheServer() = runBlocking {
        try { CrundiClient(FakeTransport()).voice.settings(); fail() } catch (e: ApiException) {
            assertTrue(VoiceApi.unsupported(e))
        }
        try { CrundiClient(FakeTransport()).voice.turn("hi", "side key", false); fail() } catch (e: ApiException) {
            assertEquals(VoiceApi.UNSUPPORTED, e.message)
        }
    }

    @Test fun aTurnSendsWhereAndLockedAndReadsTheReply() = runBlocking {
        val t = FakeTransport().on("POST /api/voice/turn", """{"ok":true,"speak":"Yes, Master.","show":"detail","tools":["get_usage"],"costUsd":0.0004}""")
        val r = CrundiClient(t).voice.turn("usage?", "side key, phone locked", true)
        assertTrue(r.ok)
        assertEquals("Yes, Master.", r.speak)
        assertEquals("detail", r.show)
        val body = t.requests.last().json.toString()
        assertTrue(body.contains("\"locked\":true") && body.contains("side key, phone locked"))
    }

    @Test fun aRefusedTurnIsAReplyNotAnException() = runBlocking {
        val t = FakeTransport().on("POST /api/voice/turn", """{"ok":false,"error":"This month's voice spend has reached the limit","limitReached":true}""")
        val r = CrundiClient(t).voice.turn("hi", "x", false)
        assertFalse(r.ok)
        assertTrue(r.limitReached)
        assertTrue(r.error.startsWith("This month"))
    }

    @Test fun speechComesBackAsBytes() = runBlocking {
        val mp3 = byteArrayOf(0x49, 0x44, 0x33, 1, 2, 3)
        val t = FakeTransport().on("POST /api/voice/speak") { ApiResponse(200, "", mapOf("Content-Type" to "audio/mpeg"), mp3) }
        assertArrayEquals(mp3, CrundiClient(t).voice.speak("Yes, Master.", "aura-2-draco-en"))
        assertTrue(t.requests.last().json.toString().contains("aura-2-draco-en"))
    }

    @Test fun theListenStreamsMessagesAreRead() {
        assertEquals(ListenEvent.Ready(), ListenEvent.parse("""{"type":"ready"}"""))
        assertEquals(ListenEvent.Ready(echoFilter = true), ListenEvent.parse("""{"type":"ready","echoFilter":true}"""))
        assertEquals(ListenEvent.Discard("open dhivo scratch that"), ListenEvent.parse("""{"type":"discard","text":"open dhivo scratch that"}"""))
        assertEquals(ListenEvent.Interim("add the"), ListenEvent.parse("""{"type":"interim","text":"add the"}"""))
        assertEquals(ListenEvent.Utterance("Add a kanban task."), ListenEvent.parse("""{"type":"utterance","text":"Add a kanban task."}"""))
        assertEquals(ListenEvent.Failed("No Deepgram key is set."), ListenEvent.parse("""{"type":"error","error":"No Deepgram key is set."}"""))
        assertEquals(null, ListenEvent.parse("""{"type":"utterance","text":""}"""))
        assertEquals(null, ListenEvent.parse("""{"type":"something-new"}"""))
        assertEquals(null, ListenEvent.parse("not json"))
    }

    @Test fun aTransportWithoutStreamsSaysSo() {
        try { CrundiClient(FakeTransport()).voice.listen(object : SocketListener { override fun onMessage(text: String) {}; override fun onClosed(code: Int, reason: String) {} }); fail() } catch (e: ApiException) {
            assertTrue(e.offline)
        }
    }

    // ─── Phase 6 and the notebook ───

    @Test fun aReplyCarriesPhoneActions() = runBlocking {
        val t = FakeTransport().on("POST /api/voice/turn", """{"ok":true,"speak":"Listening for an hour, Master.","phoneActions":[{"type":"background-listening","minutes":60},{"type":"something-new","x":1}]}""")
        val r = CrundiClient(t).voice.turn("keep listening for an hour", "Crundi Voice", false)
        assertEquals(listOf(PhoneAction("background-listening", 60), PhoneAction("something-new", 0)), r.phoneActions)
        // An older server sends none.
        val old = CrundiClient(FakeTransport().on("POST /api/voice/turn", """{"ok":true,"speak":"Yes."}""")).voice.turn("hi", "x", false)
        assertTrue(old.phoneActions.isEmpty())
    }

    @Test fun theNotebookIsListedSavedAndSearched() = runBlocking {
        val t = FakeTransport()
            .on("GET /api/voice/memory", """{"ok":true,"files":[{"path":"/memories/master.md","text":"Likes tea.","size":10}],"entries":42,"meaning":"ready"}""")
            .on("POST /api/voice/memory", """{"ok":true}""")
            .on("GET /api/voice/memory/search", """{"ok":true,"results":[{"kind":"turn","at":"2026-11-05T09:30:00.000Z","project":null,"text":"Master likes tea"},{"kind":"note","at":"2026-11-04T10:00:00.000Z","project":"crundi","text":"x"}],"meaning":true}""")
        val api = CrundiClient(t).voice
        val m = api.memory()
        assertEquals(42, m.entries)
        assertEquals("ready", m.meaning)
        assertEquals("/memories/master.md", m.files.single().path)
        assertEquals("Likes tea.", m.files.single().text)
        api.saveMemory("/memories/master.md", "")
        val body = t.requests.last().json.toString()
        assertTrue(body, body.contains("\"path\":\"/memories/master.md\"") && body.contains("\"text\":\"\""))
        val hits = api.searchMemory("tea")
        assertEquals("tea", t.requests.last().query["q"])
        assertEquals(2, hits.size)
        assertEquals(null, hits[0].project)
        assertEquals("crundi", hits[1].project)
    }

    @Test fun aServerWithoutMemorySaysSo() = runBlocking {
        try { CrundiClient(FakeTransport()).voice.memory(); fail() } catch (e: ApiException) { assertTrue(VoiceApi.unsupported(e)) }
    }

    @Test fun anOlderServerHasNoBrainNoIdleTimeAndNoAdminKeyFlag() = runBlocking {
        val s = CrundiClient(FakeTransport().on("GET /api/voice/settings", settingsJson)).voice.settings()
        assertEquals("", s.brain)
        assertEquals(0, s.cliIdleMinutes)
        assertEquals(0, s.screenCloseSeconds)
        assertEquals(null, s.anthropicAdminKeySet)
    }

    @Test fun theClaudeCodeIdleTimeIsSavedAndAKeyIsSentAlone() = runBlocking {
        val t = FakeTransport().on("POST /api/voice/settings", settingsJson.replace("\"ok\":true,", "\"ok\":true,\"brain\":\"claude-code\",\"cliIdleMinutes\":15,\"anthropicAdminKeySet\":true,"))
        val s = CrundiClient(t).voice.save(cliIdleMinutes = 15)
        assertEquals("claude-code", s.brain)
        assertEquals(15, s.cliIdleMinutes)
        assertEquals(true, s.anthropicAdminKeySet)
        assertTrue(t.requests.last().json.toString().contains("\"cliIdleMinutes\":15"))
        CrundiClient(t).voice.saveKey("anthropicAdminKey", "  sk-ant-admin-x  ")
        assertEquals("{\"anthropicAdminKey\":\"sk-ant-admin-x\"}", t.requests.last().json.toString())
        try { CrundiClient(t).voice.saveKey("ttsVoice", "x"); fail() } catch (_: IllegalArgumentException) {}
    }

    @Test fun theBrainsComparedAreRead() = runBlocking {
        val t = FakeTransport().on("GET /api/voice/brain-stats", """{"ok":true,"brain":"api","stats":[{"brain":"claude-code","model":"claude-haiku-4-5","turns":3,"failed":1,"medianMs":5120,"p90Ms":9800,"costUsd":0.04,"costPerTurnUsd":0.01333,"cacheReadShare":0.91,"toolsPerTurn":1.3}]}""")
        val st = CrundiClient(t).voice.brainStats()!!
        assertEquals("api", st.brain)
        assertEquals(1, st.stats.size)
        assertEquals(5120.0, st.stats[0].medianMs, 0.0)
        assertEquals(0.91, st.stats[0].cacheReadShare, 0.0)
    }

    @Test fun aServerWithoutBrainStatsHasNone() = runBlocking {
        assertEquals(null, CrundiClient(FakeTransport()).voice.brainStats())
    }
}
