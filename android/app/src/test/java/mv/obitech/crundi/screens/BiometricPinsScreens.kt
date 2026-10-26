package mv.obitech.crundi.screens

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import mv.obitech.crundi.PinSettings
import mv.obitech.crundi.SavedPin
import mv.obitech.crundi.api.model.Secret
import mv.obitech.crundi.api.model.SecretRequest
import mv.obitech.crundi.ui.chats.ChatActions
import mv.obitech.crundi.ui.chats.ChatScreen
import mv.obitech.crundi.ui.chats.ChatSecretsActions
import mv.obitech.crundi.ui.chats.ChatSecretsCard
import mv.obitech.crundi.ui.chats.ChatSecretsForm
import mv.obitech.crundi.ui.chats.ChatUi
import mv.obitech.crundi.ui.operations.SecretPromptDock
import mv.obitech.crundi.ui.operations.SecretPromptState
import mv.obitech.crundi.ui.pair.AppSettingsScreen
import mv.obitech.crundi.ui.pair.SettingSection
import mv.obitech.crundi.ui.shell.Host
import org.junit.Test

/**
 * Secret PINs behind the fingerprint: the PIN window with a saved PIN (the
 * fingerprint in Unlock's place) and once something is typed (Unlock back),
 * the pre-authorise dialog's fingerprint, and the rows in App settings.
 */
class BiometricPinsScreens : ScreenshotTest() {
    private val noChat = object : ChatActions {}
    private val pane = FakeApi.state.chats("crundi").first()
    private val bubble = object : Host { override val isBubble = true }
    private val req = SecretRequest("r2", "s1", "PLAY_UPLOAD_KEY", "crundi", "Sign the bundle", kind = "run", command = "./gradlew --no-daemon bundlePlayRelease", envName = "CRUNDI_UPLOAD_STORE_PASSWORD", sessionId = "a1b2c3d4e5f60718")

    @Test fun pinWithFingerprint() = shoot("secrets-pin-fingerprint", host = bubble) {
        Box(Modifier.fillMaxSize()) {
            ChatScreen(ChatUi(FakeApi.chat(0), pane = pane, isBubble = true), noChat)
            SecretPromptDock(listOf(req), SecretPromptState(armed = "r2"), savedPin = { true }, canAllow = { true })
        }
    }

    @Test fun pinTyped() = shoot("secrets-pin-fingerprint-typing", host = bubble) {
        Box(Modifier.fillMaxSize()) {
            ChatScreen(ChatUi(FakeApi.chat(0), pane = pane, isBubble = true), noChat)
            SecretPromptDock(listOf(req), SecretPromptState(armed = "r2", pin = "48"), savedPin = { true }, canAllow = { true })
        }
    }

    @Test fun preauthoriseWithFingerprint() = shoot("chats-secrets-dialog-fingerprint") {
        Box(Modifier.fillMaxSize()) {
            ChatScreen(ChatUi(FakeApi.chat(0), pane = pane, secretsOk = true), noChat)
            Box(Modifier.fillMaxSize().padding(16.dp), contentAlignment = Alignment.Center) {
                ChatSecretsCard(
                    ChatSecretsForm(secrets = listOf(Secret("s1", "PLAY_UPLOAD_KEY"), Secret("s2", "GITHUB_TOKEN"), Secret("s3", "OPENAI_KEY")), chosen = listOf("s1", "s2"), pins = mapOf("s2" to "1234")),
                    System.currentTimeMillis(), ChatSecretsActions(), savedPin = { it == "s1" || it == "s2" },
                )
            }
        }
    }

    @Test fun settingsOn() = shoot("settings-secret-pins") {
        AppSettingsScreen("Crundi 1.1.1 (play), Android 16", listOf(SettingSection("Secret PINs", PinSettings.items(
            available = true, on = true, forgottenNow = false,
            saved = listOf(SavedPin("dev.faruhaan.com", "s1", "PLAY_UPLOAD_KEY"), SavedPin("dev.faruhaan.com", "s2", "GITHUB_TOKEN"), SavedPin("home.example.net", "s9", "HA_TOKEN")),
        ))), onBack = {})
    }

    @Test fun settingsOff() = shoot("settings-secret-pins-off") {
        AppSettingsScreen("Crundi 1.1.1 (play), Android 16", listOf(
            SettingSection("Secret PINs", PinSettings.items(available = true, on = false, saved = emptyList(), forgottenNow = false)),
            SettingSection("After the fingerprints changed", PinSettings.items(available = true, on = true, saved = emptyList(), forgottenNow = true)),
        ), onBack = {})
    }
}
