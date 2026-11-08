package mv.obitech.crundi.api

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Telegram as one sign-in method and one channel: the bot under Sign-in, the chat under Notifications; older servers. */
class TelegramSettingsTest {
    @Test fun theBotComesWithTheSignInConfig() = runBlocking {
        val t = FakeTransport().on("GET /api/auth/config", """{"ok":true,"telegram":true,"telegramAvailable":true,"passwordSet":false,
            "telegramBot":{"tokenSet":true,"allowedUsername":"faruhaan","botUsername":"crundi_bot","status":"connected","restartRequired":false,"chatId":"123456"}}""")
        val b = CrundiClient(t).auth.config()!!.telegramBot!!
        assertTrue(b.tokenSet)
        assertEquals("faruhaan", b.allowedUsername)
        assertEquals("connected", b.status)
        assertEquals("123456", b.chatId)
    }

    @Test fun anOlderServerHasNoBotBlock() = runBlocking {
        val c = CrundiClient(FakeTransport().on("GET /api/auth/config", """{"ok":true,"telegram":true,"telegramAvailable":true}""")).auth.config()!!
        assertNull("the bot is still set in Configuration there", c.telegramBot)
        val ch = CrundiClient(FakeTransport().on("GET /api/notify/channels", """{"ok":true,"channels":[{"id":"telegram","label":"Telegram","enabled":true}]}""")).system.channels()
        assertNull(ch[0].telegram)
    }

    @Test fun savingAndRemovingTheBot() = runBlocking {
        val t = FakeTransport().on("POST /api/auth/config", """{"ok":true,"restartRequired":true}""")
        val a = CrundiClient(t).auth
        val r = a.saveTelegram("", "@faruhaan")
        assertTrue(r.restartRequired)
        val body = t.requests.last().json.toString()
        assertTrue(body.contains("\"action\":\"telegram-save\"") && body.contains("\"botToken\":\"\"") && body.contains("\"allowedUsername\":\"faruhaan\""))
        a.clearTelegram()
        assertTrue(t.requests.last().json.toString().contains("\"action\":\"telegram-clear\""))
    }

    @Test fun theChannelCarriesTheBotAndTheChat() = runBlocking {
        val t = FakeTransport()
            .on("GET /api/notify/channels", """{"ok":true,"channels":[{"id":"inbox","label":"Crundi inbox","alwaysOn":true,"enabled":true},
                {"id":"telegram","label":"Telegram","available":false,"enabled":true,"telegram":{"chatId":"","botStatus":"offline","tokenSet":true,"botUsername":"crundi_bot","restartRequired":false}}]}""")
            .on("POST /api/notify/telegram/chat", """{"ok":true}""")
        val sys = CrundiClient(t).system
        val tg = sys.channels()[1].telegram!!
        assertEquals("offline", tg.botStatus)
        assertTrue(tg.tokenSet)
        sys.setTelegramChat(" 987654 ")
        assertEquals("""{"chatId":"987654"}""", t.requests.last().json.toString())
    }
}
