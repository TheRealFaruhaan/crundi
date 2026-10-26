package mv.obitech.crundi

import android.content.Context
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import mv.obitech.crundi.api.ApiResponse
import mv.obitech.crundi.api.CrundiClient
import mv.obitech.crundi.api.model.SecretRequest
import mv.obitech.crundi.screens.FakeApi
import mv.obitech.crundi.screens.FakeServices
import mv.obitech.crundi.ui.operations.SecretPromptDock
import mv.obitech.crundi.ui.operations.SecretPromptRoute
import mv.obitech.crundi.ui.operations.SecretPromptState
import mv.obitech.crundi.ui.pair.SettingItem
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

/** Secret PINs behind the fingerprint: when the button shows, the switch on typing, storage, settings. */
@RunWith(AndroidJUnit4::class)
@Config(sdk = [35])
class BiometricPinsTest {
    @get:Rule val compose = createComposeRule()

    // ─── The rule ───

    @Test fun theFingerprintStandsInForSubmitOnlyWhileNothingIsTyped() {
        assertTrue(PinButtons.fingerprint(saved = true, typed = ""))
        assertFalse(PinButtons.fingerprint(saved = true, typed = "4"))
        assertFalse(PinButtons.fingerprint(saved = false, typed = ""))
    }

    @Test fun savingIsOfferedForATypedPinNotYetSaved() {
        assertTrue(PinButtons.offerSave(optionOn = true, saved = false, typed = true))
        assertFalse(PinButtons.offerSave(optionOn = false, saved = false, typed = true))
        assertFalse(PinButtons.offerSave(optionOn = true, saved = true, typed = true))
        assertFalse(PinButtons.offerSave(optionOn = true, saved = false, typed = false))
    }

    // ─── Storage ───

    private fun store() = SavedPins(ApplicationProvider.getApplicationContext<Context>().getSharedPreferences("test-pins", Context.MODE_PRIVATE).also { it.edit().clear().commit() })

    @Test fun perServerAndPerSecret() {
        val s = store()
        val sealed = Sealed(byteArrayOf(1, 2, 3), byteArrayOf(9, 8, 7, 6))
        s.put("a.example.com", "s1", "DEPLOY", sealed)
        assertFalse("nothing is kept while the option is off", s.has("a.example.com", "s1"))
        s.enabled = true
        s.put("a.example.com", "s1", "DEPLOY", sealed)
        s.put("b.example.com", "s1", "DEPLOY", sealed)
        s.put("a.example.com", "s2", "OTHER", sealed)
        assertTrue(s.has("a.example.com", "s1") && s.has("b.example.com", "s1"))
        assertFalse(s.has("c.example.com", "s1"))
        assertEquals(listOf(9, 8, 7, 6), s.get("a.example.com", "s1")!!.data.map { it.toInt() })
        assertEquals(3, s.list().size)
        s.forget("a.example.com", "s1")
        assertFalse(s.has("a.example.com", "s1"))
        s.forgetServer("b.example.com")
        assertEquals(listOf("s2"), s.list().map { it.secretId })
    }

    @Test fun turningItOffForgetsThemAll() {
        val s = store()
        s.enabled = true
        s.put("h", "s1", "A", Sealed(byteArrayOf(1), byteArrayOf(2)))
        s.enabled = false
        assertTrue(s.list().isEmpty())
        s.enabled = true
        assertTrue(s.list().isEmpty())
    }

    @Test fun onlySealedDataIsWritten() {
        val prefs = ApplicationProvider.getApplicationContext<Context>().getSharedPreferences("test-pins2", Context.MODE_PRIVATE)
        val s = SavedPins(prefs); s.enabled = true
        s.put("h", "s1", "A", Sealed(byteArrayOf(1, 2), byteArrayOf(3, 4)))
        val stored = prefs.all.values.joinToString(" ")
        assertFalse("482913" in stored)
        assertEquals("AQI=:AwQ=", prefs.getString("pin|h|s1", null))
        assertNull(Sealed.decode("garbage"))
        assertNull(Sealed.decode(":"))
    }

    @Test fun keptUnderTheServersHost() {
        assertEquals("dev.faruhaan.com", SavedPins.serverKey("https://dev.faruhaan.com"))
        assertEquals("10.0.0.2", SavedPins.serverKey("http://10.0.0.2:8889"))
    }

    // ─── Settings ───

    @Test fun settingsRows() {
        val off = PinSettings.items(available = true, on = false, saved = emptyList(), forgottenNow = false)
        assertEquals("Turn on", (off.single() as SettingItem.Line).action)
        val none = PinSettings.items(available = false, on = false, saved = emptyList(), forgottenNow = false)
        assertNull((none.single() as SettingItem.Line).action)
        val saved = listOf(SavedPin("h", "s1", "A"), SavedPin("h", "s2", "B"))
        val on = PinSettings.items(available = true, on = true, saved = saved, forgottenNow = true)
        assertEquals("Turn off", (on[0] as SettingItem.Line).action)
        assertTrue(on.any { it is SettingItem.Line && it.title == "Saved PINs forgotten" })
        assertEquals(2, on.count { it is SettingItem.Line && it.action == "Forget" })
        assertTrue(on.last() is SettingItem.Button)
    }

    // ─── The PIN window ───

    private val req = SecretRequest("r1", "s1", "PLAY_UPLOAD_KEY", "crundi", "", kind = "run", command = "./sign.sh", envName = "KEY", sessionId = "c1")

    @Test fun typingSwapsTheFingerprintForUnlockAndBack() {
        var fingered = 0
        compose.setContent {
            CrundiTheme {
                var st by remember { mutableStateOf(SecretPromptState(armed = "r1")) }
                SecretPromptDock(listOf(req), st, onPin = { st = st.copy(pin = it) }, savedPin = { true }, onFingerprint = { fingered++ })
            }
        }
        compose.onNodeWithTag("pin-fingerprint").assertExists()
        compose.onNodeWithText("Unlock").assertDoesNotExist()
        compose.onNode(hasSetTextAction()).performTextInput("4")
        compose.onNodeWithTag("pin-fingerprint").assertDoesNotExist()
        compose.onNodeWithText("Unlock").assertExists()
        compose.onNode(hasSetTextAction()).performTextClearance()
        compose.onNodeWithTag("pin-fingerprint").assertExists().performClick()
        assertEquals(1, fingered)
    }

    @Test fun noSavedPinMeansTheOrdinaryUnlock() {
        compose.setContent { CrundiTheme { SecretPromptDock(listOf(req), SecretPromptState(armed = "r1")) } }
        compose.onNodeWithTag("pin-fingerprint").assertDoesNotExist()
        compose.onNodeWithText("Unlock").assertExists()
    }

    @Test fun theFingerprintFillsThePinAndSubmitsIt() {
        val t = FakeApi.transport()
        t.on("GET /api/secrets", """{"ok":true,"secrets":[],"requests":[{"id":"r1","secretId":"s1","secretName":"PLAY_UPLOAD_KEY","project":"crundi","kind":"run","command":"./sign.sh","envName":"KEY","sessionId":"c1"}]}""")
        t.on("POST /api/secrets") { ApiResponse(200, """{"ok":true}""") }
        val services = object : AppServices {
            override val shell: ShellSource = StaticShellSource(FakeServices.shellData)
            override val api: CrundiClient = CrundiClient(t)
        }
        val keeper = object : PinKeeper {
            override val on = true
            override fun saved(server: String, secretId: String) = secretId == "s1"
            override fun unlock(server: String, secretId: String, name: String, onPin: (String) -> Unit, onFail: (String?) -> Unit) = onPin("482913")
            override fun save(server: String, secretId: String, name: String, pin: String, done: (Boolean, String?) -> Unit) = done(true, null)
            override fun forget(server: String, secretId: String) {}
        }
        compose.setContent { CrundiTheme { CompositionLocalProvider(LocalAppServices provides services, LocalPinKeeper provides keeper) { SecretPromptRoute() } } }
        compose.waitUntil(5000) { compose.onAllNodes(hasText("Approve")).fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText("Approve").performClick()
        compose.onNodeWithTag("pin-fingerprint").performClick()
        compose.waitUntil(5000) { t.requests.any { it.method == "POST" && it.path == "/api/secrets" } }
        val body = t.requests.last { it.method == "POST" && it.path == "/api/secrets" }.json!!.jsonObject
        assertEquals("approve", body["action"]!!.jsonPrimitive.content)
        assertEquals("482913", body["pin"]!!.jsonPrimitive.content)
        assertEquals("r1", body["reqId"]!!.jsonPrimitive.content)
    }
}
