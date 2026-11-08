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

    // ─── volume, admin mode, compact, history, keyboard, tickers and triggers ───

    @Test fun volumeAndAdminModeAreRead() = runBlocking {
        val t = FakeTransport().on("GET /api/voice/settings", settingsJson.replace("\"limitReached\":false}", "\"limitReached\":false,\"volume\":55,\"adminMode\":true,\"adminCapable\":true}"))
        val s = CrundiClient(t).voice.settings()
        assertEquals(55, s.volume)
        assertTrue(s.adminMode)
        assertEquals(true, s.adminCapable)
        assertEquals(0.55f, volumeGain(s.volume), 0.001f)
    }

    @Test fun anOlderServerHasFullVolumeAndNoAdminMode() = runBlocking {
        val s = CrundiClient(FakeTransport().on("GET /api/voice/settings", settingsJson)).voice.settings()
        assertEquals(100, s.volume)
        assertFalse(s.adminMode)
        assertEquals(null, s.adminCapable)
        assertEquals(1f, volumeGain(null), 0f)
        assertEquals(1f, volumeGain(5), 0f)
    }

    @Test fun volumeAndAdminModeAreSaved() = runBlocking {
        val t = FakeTransport().on("POST /api/voice/settings", settingsJson)
        CrundiClient(t).voice.save(volume = 40, adminMode = false)
        val body = t.requests.last().json.toString()
        assertTrue(body, body.contains("\"volume\":40") && body.contains("\"adminMode\":false"))
        assertFalse(body.contains("model"))
    }

    @Test fun adminModeRefusedSaysWhy() = runBlocking {
        val t = FakeTransport().on("POST /api/voice/settings", """{"ok":false,"error":"Admin mode needs Sonnet or above. Choose another model first."}""", 400)
        try { CrundiClient(t).voice.save(adminMode = true); fail() } catch (e: ApiException) {
            assertTrue(e.message!!.startsWith("Admin mode needs Sonnet"))
        }
    }

    @Test fun aTypedTurnSaysSoAndItsVolumeIsRead() = runBlocking {
        val t = FakeTransport().on("POST /api/voice/turn", """{"ok":true,"speak":"Louder, Sir.","volume":85}""")
        val r = CrundiClient(t).voice.turn("speak up", "Cru", false, typed = true)
        assertEquals(85, r.volume)
        assertTrue(t.requests.last().json.toString().contains("\"typed\":true"))
        CrundiClient(t).voice.turn("speak up", "Cru", false)
        assertFalse(t.requests.last().json.toString().contains("typed"))
        assertEquals(null, CrundiClient(FakeTransport().on("POST /api/voice/turn", """{"ok":true,"speak":"x"}""")).voice.turn("a", "b", false).volume)
    }

    @Test fun compactAnswersOrSaysNotSupported() = runBlocking {
        assertTrue(CrundiClient(FakeTransport().on("POST /api/voice/compact", """{"ok":true}""")).voice.compact()!!.ok)
        val no = CrundiClient(FakeTransport().on("POST /api/voice/compact", """{"ok":false,"error":"Nothing to compact yet"}""")).voice.compact()!!
        assertFalse(no.ok)
        assertEquals("Nothing to compact yet", no.error)
        assertEquals(null, CrundiClient(FakeTransport()).voice.compact())
    }

    @Test fun historyIsReadAndPagedBack() = runBlocking {
        val t = FakeTransport().on("GET /api/voice/history", """{"ok":true,"turns":[{"id":3,"at":1000,"said":"usage?","speak":"Plenty.","show":"5h 2%","report":false},{"id":4,"at":2000,"said":"","speak":"The build finished.","show":"","report":true}],"more":true}""")
        val h = CrundiClient(t).voice.history(40, 0)!!
        assertEquals(2, h.turns.size)
        assertTrue(h.more)
        assertTrue(h.turns[1].report)
        assertEquals("40", t.requests.last().query["limit"])
        assertEquals(null, t.requests.last().query["before"])
        CrundiClient(t).voice.history(40, 1000)
        assertEquals("1000", t.requests.last().query["before"])
        assertEquals(null, CrundiClient(FakeTransport()).voice.history())
    }

    @Test fun tickersAndTriggersAreReadAndChanged() = runBlocking {
        val list = """{"ok":true,"tickers":[{"id":"t1","type":"ticker","name":"Morning briefing","purpose":"Start the day","enabled":true,"enabledAt":5,"createdBy":"cru","createdAt":1,"lastFiredAt":0,"fires":0,"window":null,"summary":"07:30 on weekdays","every":null,"at":"07:30","days":[1,2,3,4,5]}],
            "triggers":[{"id":"g1","type":"trigger","name":"Hot CPU","purpose":"","enabled":false,"createdBy":"owner","createdAt":2,"lastFiredAt":1700000000000,"fires":3,"window":{"from":"08:00"},"summary":"CPU above 90%","condition":{"kind":"cpu","above":90},"once":false,"cooldown":30}],
            "kinds":["cpu","service-down"],"limits":{"perHour":12,"firedLastHour":1,"minEvery":15,"defaultCooldown":30}}"""
        val t = FakeTransport().on("GET /api/voice/wakeups", list).on("POST /api/voice/wakeups", list)
        val w = CrundiClient(t).voice.wakeups()!!
        assertEquals("Morning briefing", w.tickers[0].name)
        assertTrue(w.tickers[0].enabled)
        assertEquals("owner", w.triggers[0].createdBy)
        assertEquals(3, w.triggers[0].fires)
        assertEquals(1, w.limits.firedLastHour)
        CrundiClient(t).voice.wakeup("g1", "enable")
        val body = t.requests.last().json.toString()
        assertTrue(body.contains("\"id\":\"g1\"") && body.contains("\"action\":\"enable\""))
        assertEquals(null, CrundiClient(FakeTransport()).voice.wakeups())
    }

    @Test fun anUnknownWakeupSaysSo() = runBlocking {
        val t = FakeTransport().on("POST /api/voice/wakeups", """{"ok":false,"error":"No ticker or trigger with that id"}""", 404)
        try { CrundiClient(t).voice.wakeup("zz", "delete"); fail() } catch (e: ApiException) { assertEquals("No ticker or trigger with that id", e.message) }
    }

    @Test fun theStreamSaysWhenSettingsChange() {
        assertEquals(ListenEvent.Settings, ListenEvent.parse("""{"type":"settings"}"""))
        assertEquals(ListenEvent.Ready(), ListenEvent.parse("""{"type":"ready","keyboard":true}"""))
    }

    // ─── the owner's voice ───

    @Test fun utterancesSayWhetherTheVoiceWasTheOwners() {
        assertEquals(ListenEvent.Utterance("open it", false), ListenEvent.parse("""{"type":"utterance","text":"open it","owner":false}"""))
        assertEquals(ListenEvent.Utterance("open it", true), ListenEvent.parse("""{"type":"utterance","text":"open it","owner":true}"""))
        assertEquals(ListenEvent.Utterance("open it", null), ListenEvent.parse("""{"type":"utterance","text":"open it","owner":null}"""))
    }

    @Test fun speakerStatusEnrolFinishAndForget() = runBlocking {
        val status = """{"ok":true,"available":true,"model":"ready","enrolled":true,"phrases":6,"seconds":14.2,"enrolledAt":"2026-11-09T10:00:00.000Z","threshold":0.6,"minSeconds":1,"minPhrases":5,"enrolling":null,"prompts":["Hey Cru, what's on the board?"],"ownerOnly":true}"""
        val t = FakeTransport().on("GET /api/voice/speaker", status).on("DELETE /api/voice/speaker", status.replace("\"enrolled\":true", "\"enrolled\":false"))
            .on("POST /api/voice/speaker/enrol", """{"ok":true,"accepted":true,"seconds":2.1,"consistency":null,"phrases":1}""")
            .on("POST /api/voice/speaker/finish", status)
        val c = CrundiClient(t).voice
        val s = c.speaker()!!
        assertTrue(s.enrolled)
        assertEquals(6, s.phrases)
        assertEquals(1, s.prompts.size)
        val r = c.enrolPhrase(ByteArray(3200), 16000, restart = true)
        assertTrue(r.accepted)
        assertEquals(null, r.consistency)
        val req = t.requests.last()
        assertEquals("1", req.query["restart"])
        assertEquals("audio/L16;rate=16000", req.upload!!.contentType)
        assertEquals(3200L, req.upload!!.length)
        assertTrue(c.finishEnrol().enrolled)
        assertFalse(c.forgetSpeaker().enrolled)
        assertEquals(null, CrundiClient(FakeTransport()).voice.speaker())
        assertEquals(VoiceApi.SPEAKER_UNSUPPORTED, CrundiClient(FakeTransport()).voice.enrolPhrase(ByteArray(2), 16000, false).error)
    }

    @Test fun aTurnSaysWhenTheVoiceWasNotRecognised() = runBlocking {
        val r = CrundiClient(FakeTransport().on("POST /api/voice/turn", """{"ok":true,"speak":"I can't do that.","speaker":{"owner":false,"unrecognised":true}}""")).voice.turn("delete it", "Cru", false)
        assertEquals(false, r.speaker?.owner)
        assertTrue(r.speaker!!.unrecognised)
    }

    @Test fun ownerOnlyIsSaved() = runBlocking {
        val t = FakeTransport().on("POST /api/voice/settings", settingsJson)
        CrundiClient(t).voice.save(ownerOnly = false)
        assertTrue(t.requests.last().json.toString().contains("\"ownerOnly\":false"))
    }
}
