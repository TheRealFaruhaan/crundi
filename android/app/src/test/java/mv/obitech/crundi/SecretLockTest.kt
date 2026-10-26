package mv.obitech.crundi

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import mv.obitech.crundi.api.ApiResponse
import mv.obitech.crundi.api.CrundiClient
import mv.obitech.crundi.api.CrundiJson
import mv.obitech.crundi.api.model.Secret
import mv.obitech.crundi.api.model.SecretRequest
import mv.obitech.crundi.api.model.Secrets
import mv.obitech.crundi.screens.FakeApi
import mv.obitech.crundi.screens.FakeServices
import mv.obitech.crundi.ui.chats.ChatSecretPreauth
import mv.obitech.crundi.ui.chats.ChatSecretsForm
import mv.obitech.crundi.ui.chats.ChatSecretsRoute
import mv.obitech.crundi.ui.operations.EditForm
import mv.obitech.crundi.ui.operations.SavedLockAfterRelock
import mv.obitech.crundi.ui.operations.SecretLocks
import mv.obitech.crundi.ui.operations.SecretPromptDock
import mv.obitech.crundi.ui.operations.SecretPromptRoute
import mv.obitech.crundi.ui.operations.SecretPromptState
import mv.obitech.crundi.ui.shell.AppServices
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.shell.ShellSource
import mv.obitech.crundi.ui.shell.StaticShellSource
import mv.obitech.crundi.ui.theme.CrundiTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/**
 * Passphrases where a PIN was asked, the upgrade of an old lock on use, the
 * edit of a secret, and a saved (fingerprint) lock the server refuses being
 * forgotten wherever it is used.
 */
@RunWith(AndroidJUnit4::class)
@Config(sdk = [35])
class SecretLockTest {
    @get:Rule val compose = createComposeRule()

    // ─── The rule for a new lock ───

    @Test fun aNewLockIsEightCharactersAndNotOnlyDigits() {
        assertFalse("digits only", SecretLocks.passphraseOk("12345678"))
        assertFalse("seven", SecretLocks.passphraseOk("abcdefg"))
        assertTrue(SecretLocks.passphraseOk("abcd1234"))
        assertTrue("any characters", SecretLocks.passphraseOk("ab cd éf"))
        assertNull("nothing typed is not an error yet", SecretLocks.newLockProblem("", ""))
        assertEquals("At least 8 characters, and not only digits.", SecretLocks.newLockProblem("12345678", ""))
        assertEquals("The two do not match.", SecretLocks.newLockProblem("abcd1234", "abcd12"))
        assertTrue(SecretLocks.newLockReady("abcd1234", "abcd1234"))
        assertFalse(SecretLocks.newLockReady("abcd1234", "abcd1235"))
    }

    @Test fun aPinFieldStaysSixDigitsAndAPassphraseFieldTakesText() {
        assertEquals("123456", SecretLocks.filter("pin", "12a34567"))
        assertEquals("correct horse 9", SecretLocks.filter("passphrase", "correct horse 9"))
        assertTrue(SecretLocks.lockOk("pin", "123456"))
        assertFalse(SecretLocks.lockOk("pin", "abc"))
        assertTrue(SecretLocks.lockOk("passphrase", "x"))
        assertFalse(SecretLocks.lockOk("passphrase", ""))
    }

    @Test fun theUpgradeIsOfferedOnlyWhenTheServerCanAndTheLockIsOld() {
        assertTrue(SecretLocks.offerUpgrade(setOf("secret-upgrade"), true))
        assertFalse(SecretLocks.offerUpgrade(emptySet(), true))
        assertFalse(SecretLocks.offerUpgrade(setOf("secret-upgrade"), false))
    }

    @Test fun anOlderServerSendsNoLockAndMeansAPin() {
        val s = CrundiJson.decodeFromString<Secrets>("""{"secrets":[{"id":"a","name":"A"}],"requests":[{"id":"r","secretId":"a"}]}""")
        assertEquals("pin", s.secrets[0].lock); assertFalse(s.secrets[0].needsUpgrade)
        assertEquals("pin", s.requests[0].lock)
        val n = CrundiJson.decodeFromString<Secrets>("""{"secrets":[{"id":"a","name":"A","lock":"passphrase","needsUpgrade":true}]}""")
        assertEquals("passphrase", n.secrets[0].lock); assertTrue(n.secrets[0].needsUpgrade)
    }

    @Test fun theEditFormIsReadyOnlyWithWhatEachChangeNeeds() {
        val s = Secret("s1", "DEPLOY", "d", lock = "pin")
        assertFalse("nothing changed", EditForm.of(s).ready(s))
        assertTrue("a rename needs no lock", EditForm.of(s).copy(name = "DEPLOY 2").ready(s))
        assertFalse("a new value needs the current PIN", EditForm.of(s).copy(value = "v").ready(s))
        assertTrue(EditForm.of(s).copy(value = "v", current = "123456").ready(s))
        assertFalse("a new lock must meet the rule", EditForm.of(s).copy(relock = true, a = "12345678", b = "12345678", current = "123456").ready(s))
        assertTrue(EditForm.of(s).copy(relock = true, a = "abcd1234", b = "abcd1234", current = "123456").ready(s))
    }

    // ─── Saved locks ───

    private class Keeper(override val on: Boolean = true, var saved: MutableSet<String> = mutableSetOf("s1"), private val fill: String = "482913") : PinKeeper {
        val forgotten = mutableListOf<String>()
        val savedNow = mutableListOf<Pair<String, String>>()
        override fun saved(server: String, secretId: String) = secretId in saved
        override fun unlock(server: String, secretId: String, name: String, onPin: (String) -> Unit, onFail: (String?) -> Unit) = onPin(fill)
        override fun save(server: String, secretId: String, name: String, pin: String, done: (Boolean, String?) -> Unit) { savedNow += secretId to pin; saved += secretId; done(true, null) }
        override fun forget(server: String, secretId: String) { forgotten += secretId; saved -= secretId }
    }

    @Test fun aKeeperKeepsPassphrasesToo() {
        assertTrue(PinKeeper.storable("482913"))
        assertTrue(PinKeeper.storable("correct horse"))
        assertFalse(PinKeeper.storable(""))
    }

    @Test fun afterARelockTheOldSavedLockGoesAndTheNewOneIsSavedWhenAsked() {
        val k = Keeper()
        SavedLockAfterRelock.apply(k, "h", "s1", "A", "new-pass-1", save = false)
        assertEquals(listOf("s1"), k.forgotten); assertTrue(k.savedNow.isEmpty())
        SavedLockAfterRelock.apply(k, "h", "s1", "A", "new-pass-1", save = true)
        assertEquals(listOf("s1" to "new-pass-1"), k.savedNow)
        val off = Keeper(on = false)
        SavedLockAfterRelock.apply(off, "h", "s1", "A", "new-pass-1", save = true)
        assertTrue("the option off: forgotten, nothing saved", off.savedNow.isEmpty() && off.forgotten == listOf("s1"))
    }

    @Test fun preauthoriseRemembersWhatTheFingerprintFilled() {
        val f = ChatSecretsForm(secrets = listOf(Secret("s1", "A", lock = "passphrase", needsUpgrade = false), Secret("s2", "B", needsUpgrade = true)), chosen = listOf("s1", "s2"))
        val filled = ChatSecretPreauth.filledIn(f, "s1", "saved-pass")
        assertEquals(setOf("s1"), filled.filled)
        assertEquals("s1", ChatSecretPreauth.refusedSaved(filled, "s1"))
        assertNull("a typed one is not forgotten from the phone", ChatSecretPreauth.refusedSaved(filled, "s2"))
        val typed = ChatSecretPreauth.pin(filled, "s1", "typed now")
        assertTrue("typing over it makes it typed", typed.filled.isEmpty())
        assertEquals("typed now", typed.pins["s1"])
        assertEquals("12", ChatSecretPreauth.pin(f, "s2", "1a2").pins["s2"])
        assertEquals("Enter the PIN or passphrase of each secret chosen.", ChatSecretPreauth.problem(f.copy(pins = mapOf("s1" to "x")))?.first)
        assertNull(ChatSecretPreauth.problem(f.copy(pins = mapOf("s1" to "x", "s2" to "123456"))))
        assertEquals(listOf("s2"), ChatSecretPreauth.upgrades(f, mapOf("s1" to "x", "s2" to "123456")).map { it.secretId })
    }

    // ─── The request card ───

    private val passReq = SecretRequest("r1", "s1", "DEPLOY", "crundi", "", kind = "get", lock = "passphrase")

    @Test fun aPassphraseRequestTakesTextAndApproves() {
        compose.setContent {
            CrundiTheme {
                var st by remember { mutableStateOf(SecretPromptState(armed = "r1")) }
                SecretPromptDock(listOf(passReq), st, onPin = { st = st.copy(pin = it) })
            }
        }
        compose.onNode(hasSetTextAction()).performTextInput("correct horse")
        compose.onNode(hasText("Approve") and isEnabled()).assertExists()
    }

    private fun services(t: mv.obitech.crundi.api.FakeTransport, caps: Set<String>) = object : AppServices {
        override val shell: ShellSource = StaticShellSource(FakeServices.shellData.copy(capabilities = caps))
        override val api: CrundiClient = CrundiClient(t)
    }

    @Test fun aSavedLockTheServerRefusesIsForgottenFromTheRequestCard() {
        val t = FakeApi.transport()
        t.on("GET /api/secrets", """{"ok":true,"secrets":[],"requests":[{"id":"r1","secretId":"s1","secretName":"DEPLOY","kind":"get"}]}""")
        t.on("POST /api/secrets") { ApiResponse(200, """{"ok":false,"error":"Incorrect PIN"}""") }
        val k = Keeper()
        compose.setContent { CrundiTheme { CompositionLocalProvider(LocalAppServices provides services(t, emptySet()), LocalPinKeeper provides k) { SecretPromptRoute() } } }
        compose.waitUntil(5000) { compose.onAllNodes(hasTestTag("pin-fingerprint")).fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag("pin-fingerprint").performClick()
        compose.waitUntil(5000) { k.forgotten.isNotEmpty() }
        assertEquals(listOf("s1"), k.forgotten)
    }

    @Test fun anOldLockApprovedOffersTheUpgradeAndSendsTheLockJustTyped() {
        val t = FakeApi.transport()
        t.on("GET /api/secrets", """{"ok":true,"secrets":[],"requests":[{"id":"r1","secretId":"s1","secretName":"DEPLOY","kind":"get","lock":"pin","needsUpgrade":true}]}""")
        t.on("POST /api/secrets") { ApiResponse(200, """{"ok":true}""") }
        val k = Keeper(saved = mutableSetOf())
        compose.setContent { CrundiTheme { CompositionLocalProvider(LocalAppServices provides services(t, setOf("secret-upgrade", "secret-passphrase", "secret-edit")), LocalPinKeeper provides k) { SecretPromptRoute() } } }
        compose.waitUntil(5000) { compose.onAllNodes(hasSetTextAction()).fetchSemanticsNodes().isNotEmpty() }
        compose.onNode(hasSetTextAction()).performTextInput("482913")
        compose.onNodeWithText("Approve").performClick()
        compose.waitUntil(5000) { compose.onAllNodesWithText("Update this secret’s lock").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText(SecretLocks.UPGRADE_PIN).assertExists()
        compose.onNode(hasText("Update") and isEnabled()).assertDoesNotExist()
        val fields = compose.onAllNodes(hasSetTextAction())
        val n = fields.fetchSemanticsNodes().size
        fields[n - 2].performTextInput("new-pass-1")
        fields[n - 1].performTextInput("new-pass-1")
        compose.onNode(hasText("Update") and isEnabled()).performClick()
        compose.waitUntil(5000) { t.requests.any { it.method == "POST" && it.json?.jsonObject?.get("action")?.jsonPrimitive?.content == "edit" } }
        val body = t.requests.last { it.method == "POST" && it.json?.jsonObject?.get("action")?.jsonPrimitive?.content == "edit" }.json!!.jsonObject
        assertEquals("s1", body["id"]!!.jsonPrimitive.content)
        assertEquals("482913", body["pin"]!!.jsonPrimitive.content)
        assertEquals("new-pass-1", body["newPin"]!!.jsonPrimitive.content)
        compose.waitUntil(5000) { k.forgotten.isNotEmpty() }
        assertEquals("the old saved one is forgotten", listOf("s1"), k.forgotten)
    }

    // ─── The pre-authorise dialog ───

    @Test fun aSavedLockTheServerRefusesIsForgottenFromThePreauthoriseDialog() {
        val t = FakeApi.transport()
        t.on("GET /api/secrets", """{"ok":true,"secrets":[{"id":"s1","name":"DEPLOY","lock":"passphrase"}],"requests":[]}""")
        t.on("POST /api/secrets/chat") { ApiResponse(200, """{"ok":false,"error":"Incorrect passphrase","secretId":"s1"}""") }
        val k = Keeper(fill = "old-pass-1")
        compose.setContent {
            CrundiTheme { CompositionLocalProvider(LocalAppServices provides services(t, setOf("chat-secret-preauth")), LocalPinKeeper provides k) { ChatSecretsRoute(CrundiClient(t), "c1", emptyList()) {} } }
        }
        compose.waitUntil(5000) { compose.onAllNodesWithText("DEPLOY").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText("DEPLOY").performClick()
        compose.waitUntil(5000) { compose.onAllNodes(hasTestTag("pin-fingerprint")).fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag("pin-fingerprint").performClick()
        compose.onNodeWithText("Pre-authorise").performClick()
        compose.waitUntil(5000) { k.forgotten.isNotEmpty() }
        assertEquals(listOf("s1"), k.forgotten)
    }
}
