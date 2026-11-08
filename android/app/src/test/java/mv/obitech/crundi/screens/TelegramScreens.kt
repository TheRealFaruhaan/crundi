package mv.obitech.crundi.screens

import mv.obitech.crundi.api.AuthConfig
import mv.obitech.crundi.api.OAuthConfigs
import mv.obitech.crundi.api.TelegramBot
import mv.obitech.crundi.api.model.NotifyChannel
import mv.obitech.crundi.api.model.ServerSettings
import mv.obitech.crundi.api.model.TelegramChannel
import mv.obitech.crundi.ui.operations.ConfigScreen
import mv.obitech.crundi.ui.operations.ConfigState
import mv.obitech.crundi.ui.operations.Load
import mv.obitech.crundi.ui.operations.NotificationsScreen
import mv.obitech.crundi.ui.operations.NotificationsState
import mv.obitech.crundi.ui.operations.SignInScreen
import mv.obitech.crundi.ui.operations.SignInState
import org.junit.Test
import org.robolectric.annotation.Config

/** Telegram is one sign-in method and one channel: Configuration without it, Sign-in > Telegram, Notifications > Telegram. */
class TelegramScreens : ScreenshotTest() {
    private val settings: ServerSettings = FakeApi.model("settings")

    @Test fun configuration() = shoot("settings-config-no-telegram") { ConfigScreen(ConfigState(Load.Ready(settings), telegramElsewhere = true)) }

    @Config(qualifiers = "w412dp-h1200dp-420dpi")
    @Test fun signInTelegram() = shoot("settings-signin-telegram") {
        SignInScreen(
            SignInState(Load.Ready(AuthConfig(
                telegram = true, telegramAvailable = true, passwordSet = true, oauth = OAuthConfigs(),
                telegramBot = TelegramBot(tokenSet = true, allowedUsername = "faruhaan", botUsername = "crundi_bot", status = "connected", chatId = "123456789"),
            ))),
            open = setOf("telegram"),
        )
    }

    @Test fun notificationsTelegram() = shoot("settings-notifications-telegram") {
        NotificationsScreen(NotificationsState(
            Load.Ready(listOf(
                NotifyChannel("inbox", "Crundi inbox", "Always on. Messages, pictures, files, questions and reports for you are kept here first, in the envelope at the bottom of the projects list.", enabled = true, alwaysOn = true),
                NotifyChannel("telegram", "Telegram", "Messages to your Telegram chat.", enabled = true, telegram = TelegramChannel("123456789", "connected", true, "crundi_bot")),
                NotifyChannel("mobile", "Phone app", "Notifications on paired phones", enabled = true),
            )),
            Load.Ready(settings.notifyPrefs),
        ))
    }
}
