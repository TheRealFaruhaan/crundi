package mv.obitech.crundi.screens

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.github.takahirom.roborazzi.RobolectricDeviceQualifiers
import mv.obitech.crundi.api.model.ChatSecretGrant
import mv.obitech.crundi.api.model.Secret
import mv.obitech.crundi.api.model.SecretRequest
import mv.obitech.crundi.ui.chats.ChatActions
import mv.obitech.crundi.ui.chats.ChatScreen
import mv.obitech.crundi.ui.chats.ChatSecretsActions
import mv.obitech.crundi.ui.chats.ChatSecretsCard
import mv.obitech.crundi.ui.chats.ChatSecretsForm
import mv.obitech.crundi.ui.chats.ChatSheet
import mv.obitech.crundi.ui.chats.ChatUi
import mv.obitech.crundi.ui.operations.SecretPromptDock
import mv.obitech.crundi.ui.operations.SecretPromptState
import mv.obitech.crundi.ui.shell.Host
import org.junit.Test
import org.robolectric.annotation.Config

/**
 * Secrets pre-authorised for one chat ('chat-secret-preauth'): the dialog
 * (what is allowed now, and allowing more), the menu line on a phone, the
 * lock in the header where there is room, and "Allow for this chat" on the
 * PIN prompt.
 */
class ChatSecretsScreens : ScreenshotTest() {
    private val noChat = object : ChatActions {}
    private val pane = FakeApi.state.chats("crundi").first()
    private val now = System.currentTimeMillis()
    private fun inMin(m: Long) = java.time.Instant.ofEpochMilli(now + m * 60_000).toString()
    private val grants = listOf(
        ChatSecretGrant("s1", "PLAY_UPLOAD_KEY", expiresAt = inMin(42)),
        ChatSecretGrant("s2", "GITHUB_TOKEN", untilChatEnds = true),
    )
    private val secrets = listOf(
        Secret("s1", "PLAY_UPLOAD_KEY"), Secret("s2", "GITHUB_TOKEN"), Secret("s3", "CLOUDFLARE_API_TOKEN"), Secret("s4", "OPENAI_KEY"),
    )

    @Composable
    private fun Over(content: @Composable () -> Unit) {
        Box(Modifier.fillMaxSize()) {
            ChatScreen(ChatUi(FakeApi.chat(0), pane = pane, secretsOk = true, secretGrants = 2), noChat)
            Box(Modifier.fillMaxSize().padding(16.dp), contentAlignment = Alignment.Center) { content() }
        }
    }

    @Test fun dialog() = shoot("chats-secrets-dialog") {
        Over {
            ChatSecretsCard(
                ChatSecretsForm(grants = grants, secrets = secrets, chosen = listOf("s3"), pins = mapOf("s3" to "4829"), duration = "8h"),
                now, ChatSecretsActions(),
            )
        }
    }

    /** A bubble's short window and a long list: the list scrolls, the buttons stay on screen. */
    @Test fun dialogShortWindow() = shoot("chats-secrets-dialog-short") {
        val many = (1..9).map { Secret(id = "m$it", name = "Secret number $it with a long name") }
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
            Box(Modifier.height(560.dp)) {
                mv.obitech.crundi.ui.chats.SheetOverlay({}) {
                    ChatSecretsCard(ChatSecretsForm(secrets = many, chosen = listOf("m2", "m3"), pins = mapOf("m2" to "123456", "m3" to "654321"), duration = "1h"), now, ChatSecretsActions())
                }
            }
        }
    }

    @Test fun dialogWrongPin() = shoot("chats-secrets-dialog-wrong-pin") {
        Over {
            ChatSecretsCard(
                ChatSecretsForm(secrets = secrets, chosen = listOf("s3", "s4"), pins = mapOf("s4" to "120394"), bad = "s3", duration = "chat", error = "CLOUDFLARE_API_TOKEN: Incorrect PIN"),
                now, ChatSecretsActions(),
            )
        }
    }

    // A phone: no room for the lock in the header, so it is in the menu.
    @Test fun menuOnAPhone() = shoot("chats-secrets-menu") {
        ChatScreen(ChatUi(FakeApi.chat(0), pane = pane, secretsOk = true, secretGrants = 2, sheet = ChatSheet.Menu), noChat)
    }

    // A tablet: the lock in the header, yellow while anything is pre-authorised.
    @Config(qualifiers = RobolectricDeviceQualifiers.MediumTablet)
    @Test fun lockInTheHeader() = shoot("chats-secrets-lock-wide") {
        ChatScreen(ChatUi(FakeApi.chat(0), pane = pane, secretsOk = true, secretGrants = 2), noChat)
    }

    @Test fun allowOnThePinPrompt() = shoot("secrets-request-allow", host = object : Host { override val isBubble = true }) {
        Box(Modifier.fillMaxSize()) {
            ChatScreen(ChatUi(FakeApi.chat(0), pane = pane, isBubble = true), noChat)
            SecretPromptDock(
                listOf(SecretRequest("r2", "s1", "PLAY_UPLOAD_KEY", "crundi", "Sign the bundle", kind = "run", command = "./gradlew --no-daemon bundlePlayRelease", envName = "CRUNDI_UPLOAD_STORE_PASSWORD", sessionId = "a1b2c3d4e5f60718")),
                SecretPromptState(armed = "r2", pin = "482913", allow = true, allowFor = "1h"),
                canAllow = { true },
            )
        }
    }
}
