package mv.obitech.crundi.ui.operations

import mv.obitech.crundi.api.AuthConfig
import mv.obitech.crundi.api.OAuthConfigs
import mv.obitech.crundi.api.OAuthView
import mv.obitech.crundi.api.model.NotifyChannel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** What the Sign-in page shows and allows; the Configuration page's Telegram rule; the inbox channel. */
class SignInRulesTest {
    private val google = OAUTH_PROVIDERS.first { it.key == "google" }
    private val github = OAUTH_PROVIDERS.first { it.key == "github" }

    @Test fun eachMethodSaysOnOffOrNotSetUp() {
        val c = AuthConfig(telegram = true, telegramAvailable = true, passwordSet = false,
            oauth = OAuthConfigs(google = OAuthView(configured = true, verified = false)))
        assertEquals("On", SignInRules.state(c, "telegram"))
        assertEquals("Not set up", SignInRules.state(c, "password"))
        assertEquals("Off", SignInRules.state(c, "google"))
        assertEquals("Not set up", SignInRules.state(c, "github"))
        assertEquals("Off", SignInRules.state(c.copy(passwordSet = true), "password"))
        assertEquals("Not set up", SignInRules.state(AuthConfig(), "telegram"))
    }

    @Test fun googleAndGitHubTurnOnOnlyAfterAPassedTest() {
        val untested = AuthConfig(telegram = true, oauth = OAuthConfigs(google = OAuthView(configured = true, verified = false)))
        assertFalse(SignInRules.canSwitch(untested, "google"))
        val tested = untested.copy(oauth = OAuthConfigs(google = OAuthView(configured = true, verified = true)))
        assertTrue(SignInRules.canSwitch(tested, "google"))
        assertFalse("not set up", SignInRules.canSwitch(tested, "github"))
        assertFalse("an older server has no OAuth", SignInRules.canSwitch(AuthConfig(telegram = true), "google"))
    }

    @Test fun aMethodThatIsOnCanAlwaysBeAskedToTurnOff() {
        // The server refuses the last one and says why; the page shows its words, so the switch stays movable.
        val onlyTelegram = AuthConfig(telegram = true, telegramAvailable = true)
        assertTrue(SignInRules.canSwitch(onlyTelegram, "telegram"))
        assertEquals(1, onlyTelegram.onCount)
        assertFalse(SignInRules.canSwitch(onlyTelegram, "password"))
        assertTrue(SignInRules.canSwitch(onlyTelegram.copy(passwordSet = true), "password"))
    }

    @Test fun whatAProviderSaysAboutItself() {
        assertTrue(SignInRules.oauthStatus(google, OAuthView()).startsWith("Not set up. Create an OAuth app with Google"))
        assertTrue(SignInRules.oauthStatus(google, OAuthView(configured = true)).startsWith("Saved, not tested"))
        assertEquals("Tested: signed in as me@example.com.", SignInRules.oauthStatus(google, OAuthView(configured = true, verified = true, verifiedAs = "me@example.com")))
        assertTrue(SignInRules.oauthStatus(github, OAuthView(configured = true, verified = true, allowed = "me", idPinned = true)).contains("Pinned to that account's id"))
        assertTrue(SignInRules.isLocal("http://localhost:3000/api/auth/oauth/google/callback"))
        assertFalse(SignInRules.isLocal("https://dev.example.com/api/auth/oauth/google/callback"))
    }

    @Test fun telegramIsBothOrNeither() {
        assertNull(ConfigRules.telegramError(emptyMap()))
        assertNull(ConfigRules.telegramError(mapOf("TELEGRAM_BOT_TOKEN" to "1:a", "ALLOWED_USERNAME" to "me")))
        assertTrue(ConfigRules.telegramError(mapOf("TELEGRAM_BOT_TOKEN" to "1:a", "ALLOWED_USERNAME" to " ")) != null)
        assertTrue(ConfigRules.telegramError(mapOf("ALLOWED_USERNAME" to "me")) != null)
    }

    @Test fun theInboxIsSaidToBeAlwaysOnOnlyWhenTheServerHasIt() {
        assertTrue(notifyChannelsDetail(listOf(NotifyChannel("inbox", alwaysOn = true), NotifyChannel("telegram"))).contains("Crundi inbox first, and it is always on"))
        assertFalse(notifyChannelsDetail(listOf(NotifyChannel("telegram"))).contains("inbox"))
    }
}
