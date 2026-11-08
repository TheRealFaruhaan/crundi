package mv.obitech.crundi.ui.operations

import mv.obitech.crundi.api.AuthConfig
import mv.obitech.crundi.api.TelegramBot
import mv.obitech.crundi.api.model.ServerSettings
import mv.obitech.crundi.api.model.TelegramChannel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Where Telegram is edited, and how its state is said. */
class TelegramRulesTest {
    @Test fun configurationKeepsTheTelegramFieldsOnlyOnAnOlderServer() {
        val older = ConfigState(Load.Ready(ServerSettings()))
        assertTrue(older.fields.any { it.key == "TELEGRAM_BOT_TOKEN" })
        assertTrue(older.fields.any { it.key == CHAT_ID })
        val newer = older.copy(telegramElsewhere = true)
        assertFalse(newer.fields.any { it.key in TELEGRAM_KEYS })
        assertEquals(listOf("PROJECTS_DIR", "WEB_PORT", "CLOUDFLARE_TUNNEL_TOKEN", "CLOUDFLARE_TUNNEL_URL", "DATA_DIR"), newer.fields.map { it.key })
    }

    @Test fun theSignInHeaderSaysWhereTheBotIs() {
        assertEquals("Not set up", SignInRules.state(AuthConfig(telegramBot = TelegramBot(tokenSet = false)), "telegram"))
        assertEquals("Restart needed", SignInRules.state(AuthConfig(telegram = true, telegramAvailable = true, telegramBot = TelegramBot(tokenSet = true, restartRequired = true)), "telegram"))
        assertEquals("On", SignInRules.state(AuthConfig(telegram = true, telegramAvailable = true, telegramBot = TelegramBot(tokenSet = true)), "telegram"))
        assertEquals("Off", SignInRules.state(AuthConfig(telegramAvailable = true, telegramBot = TelegramBot(tokenSet = true)), "telegram"))
        assertTrue(SignInRules.telegramStatus(TelegramBot(tokenSet = true, status = "connected", botUsername = "crundi_bot")).startsWith("Bot @crundi_bot is connected"))
        assertTrue(SignInRules.telegramStatus(TelegramBot(tokenSet = true, status = "offline")).startsWith("The bot is offline"))
        assertTrue(SignInRules.telegramStatus(TelegramBot()).startsWith("Not set up. Create a bot with @BotFather"))
    }

    @Test fun theChannelSaysHowTheBotIs() {
        assertEquals("Not set up", telegramBotState(TelegramChannel()).first)
        assertEquals("Saved, waiting for a restart", telegramBotState(TelegramChannel(tokenSet = true, restartRequired = true)).first)
        assertEquals("Connected as @crundi_bot", telegramBotState(TelegramChannel(botStatus = "connected", tokenSet = true, botUsername = "crundi_bot")).first)
        assertEquals("Offline", telegramBotState(TelegramChannel(botStatus = "offline", tokenSet = true)).first)
    }
}
