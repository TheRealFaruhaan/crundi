package mv.obitech.crundi.screens

import mv.obitech.crundi.api.AuthConfig
import mv.obitech.crundi.api.OAuthConfigs
import mv.obitech.crundi.api.OAuthView
import mv.obitech.crundi.ui.operations.Load
import mv.obitech.crundi.ui.operations.OAuthTest
import mv.obitech.crundi.ui.operations.SignInScreen
import mv.obitech.crundi.ui.operations.SignInState
import org.junit.Test
import org.robolectric.annotation.Config

/** Settings > Sign-in: every method folded with its state, Google opened mid-test, and an older server. */
class SignInScreens : ScreenshotTest() {
    private val config = AuthConfig(
        telegram = true, password = false, google = false, github = false,
        telegramAvailable = true, passwordSet = true,
        oauth = OAuthConfigs(
            google = OAuthView(configured = true, clientId = "1048572-abc.apps.googleusercontent.com", secretSet = true, allowed = "faruhan@outlook.com", verified = false),
            github = OAuthView(),
        ),
        callbackUrls = mapOf("google" to "https://dev.faruhaan.com/api/auth/oauth/google/callback", "github" to "https://dev.faruhaan.com/api/auth/oauth/github/callback"),
    )

    @Test fun folded() = shoot("settings-signin") { SignInScreen(SignInState(Load.Ready(config))) }

    @Config(qualifiers = "w412dp-h1500dp-420dpi")
    @Test fun googleOpen() = shoot("settings-signin-google") {
        SignInScreen(
            SignInState(Load.Ready(config), tests = mapOf("google" to OAuthTest(waiting = true, text = "Waiting for the sign-in in the browser. Sign in with Google there, then come back."))),
            open = setOf("google"),
        )
    }

    @Test fun olderServer() = shoot("settings-signin-older-server") { SignInScreen(SignInState(Load.Ready(null))) }
}
