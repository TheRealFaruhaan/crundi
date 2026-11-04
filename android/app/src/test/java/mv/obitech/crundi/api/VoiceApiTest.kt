package mv.obitech.crundi.api

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/** The phone's side of /api/voice/*, against a server in memory. */
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
        assertEquals(ListenEvent.Ready, ListenEvent.parse("""{"type":"ready"}"""))
        assertEquals(ListenEvent.Interim("add the"), ListenEvent.parse("""{"type":"interim","text":"add the"}"""))
        assertEquals(ListenEvent.Utterance("Add a kanban task."), ListenEvent.parse("""{"type":"utterance","text":"Add a kanban task."}"""))
        assertEquals(ListenEvent.Failed("No Deepgram key is set."), ListenEvent.parse("""{"type":"error","error":"No Deepgram key is set."}"""))
        assertEquals(null, ListenEvent.parse("""{"type":"utterance","text":""}"""))
        assertEquals(null, ListenEvent.parse("""{"type":"something-new"}"""))
        assertEquals(null, ListenEvent.parse("not json"))
    }

    @Test fun aTransportWithoutStreamsSaysSo() {
        try { CrundiClient(FakeTransport()).voice.listen(object : SocketListener { override fun onMessage(text: String) {} }); fail() } catch (e: ApiException) {
            assertTrue(e.offline)
        }
    }
}
