package mv.obitech.crundi.api

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/** Settings > Sign-in against a server in memory: /api/auth/config and the OAuth test. */
class AuthApiTest {
    private val configJson = """{"ok":true,"telegram":true,"password":false,"google":false,"github":false,"anyConfigured":true,
        "passwordFromEnv":false,"telegramAvailable":true,"passwordSet":true,
        "oauth":{"google":{"configured":true,"clientId":"123.apps.googleusercontent.com","secretSet":true,"allowed":"me@example.com","verified":true,"verifiedAs":"me@example.com"},
                 "github":{"configured":false,"clientId":"","secretSet":false,"allowed":"","idPinned":false,"verified":false,"verifiedAs":""}},
        "callbackUrls":{"google":"https://dev.example.com/api/auth/oauth/google/callback","github":"https://dev.example.com/api/auth/oauth/github/callback"}}"""

    @Test fun theConfigIsRead() = runBlocking {
        val c = CrundiClient(FakeTransport().on("GET /api/auth/config", configJson)).auth.config()!!
        assertTrue(c.telegram && c.passwordSet && !c.password)
        assertEquals(1, c.onCount)
        assertEquals("me@example.com", c.oauth!!.google.allowed)
        assertTrue(c.oauth!!.google.verified && c.oauth!!.google.secretSet)
        assertFalse(c.oauth!!.github.configured)
        assertEquals("https://dev.example.com/api/auth/oauth/github/callback", c.callbackUrls["github"])
    }

    @Test fun anOlderServerHasNoConfigOrNoOAuth() = runBlocking {
        assertNull("no /api/auth/config: not supported", CrundiClient(FakeTransport()).auth.config())
        val c = CrundiClient(FakeTransport().on("GET /api/auth/config", """{"ok":true,"telegram":true,"password":true,"telegramAvailable":true,"passwordSet":true}""")).auth.config()!!
        assertNull("no Google or GitHub there", c.oauth)
        assertTrue(c.callbackUrls.isEmpty())
    }

    @Test fun turningOffTheLastMethodShowsTheServersWords() = runBlocking {
        val t = FakeTransport().on("POST /api/auth/config", """{"ok":false,"error":"That is the only way to sign in. Turn on another method first."}""", 400)
        try { CrundiClient(t).auth.setEnabled("telegram", false); fail() } catch (e: ApiException) {
            assertEquals("That is the only way to sign in. Turn on another method first.", e.message)
        }
        val body = t.requests.last().json.toString()
        assertTrue(body.contains("\"action\":\"disable\"") && body.contains("\"method\":\"telegram\""))
        try { CrundiClient(t).auth.setEnabled("facebook", true); fail() } catch (_: IllegalArgumentException) {}
    }

    @Test fun savingAProviderAndSettingAPassword() = runBlocking {
        val t = FakeTransport().on("POST /api/auth/config", """{"ok":true,"totpSecret":"JBSWY3DPEHPK3PXP","uri":"otpauth://totp/Crundi:crundi?secret=JBSWY3DPEHPK3PXP"}""")
        val a = CrundiClient(t).auth
        a.saveOAuth("github", " Iv1.abc ", "", "@me")
        val body = t.requests.last().json.toString()
        assertTrue(body.contains("\"action\":\"oauth-save\"") && body.contains("\"clientId\":\"Iv1.abc\"") && body.contains("\"clientSecret\":\"\"") && body.contains("\"allowed\":\"@me\""))
        val pw = a.setPassword("a long enough password")
        assertEquals("JBSWY3DPEHPK3PXP", pw.totpSecret)
        assertTrue(pw.uri.startsWith("otpauth://"))
        a.clearOAuth("google")
        assertTrue(t.requests.last().json.toString().contains("\"action\":\"oauth-clear\""))
    }

    @Test fun aTestStartsAndIsPolled() = runBlocking {
        val t = FakeTransport()
            .on("POST /api/auth/oauth/start", """{"ok":true,"url":"https://accounts.google.com/o/oauth2/v2/auth?x=1","mode":"poll","pollId":"p1","confirmCode":"4821"}""")
            .on("POST /api/auth/oauth/poll", """{"ok":true,"status":"done","account":"me@example.com"}""")
        val a = CrundiClient(t).auth
        val s = a.startTest("google")
        assertEquals("p1", s.pollId)
        assertTrue(s.url.startsWith("https://accounts.google.com/"))
        val start = t.requests.last().json.toString()
        assertTrue(start.contains("\"purpose\":\"test\"") && start.contains("\"provider\":\"google\""))
        val p = a.poll("p1")
        assertEquals("done", p.status)
        assertEquals("me@example.com", p.account)
    }
}
