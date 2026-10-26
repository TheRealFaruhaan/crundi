package mv.obitech.crundi.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import mv.obitech.crundi.api.model.Secret
import mv.obitech.crundi.api.model.SecretRequest
import mv.obitech.crundi.ui.chats.ChatActions
import mv.obitech.crundi.ui.chats.ChatScreen
import mv.obitech.crundi.ui.chats.ChatSecretsActions
import mv.obitech.crundi.ui.chats.ChatSecretsCard
import mv.obitech.crundi.ui.chats.ChatSecretsForm
import mv.obitech.crundi.ui.chats.ChatUi
import mv.obitech.crundi.ui.components.DialogCard
import mv.obitech.crundi.ui.operations.AddSecretForm
import mv.obitech.crundi.ui.operations.EditForm
import mv.obitech.crundi.ui.operations.EditSecretCard
import mv.obitech.crundi.ui.operations.Load
import mv.obitech.crundi.ui.operations.PinAsk
import mv.obitech.crundi.ui.operations.PinCard
import mv.obitech.crundi.ui.operations.SecretPromptDock
import mv.obitech.crundi.ui.operations.SecretPromptState
import mv.obitech.crundi.ui.operations.SecretsScreen
import mv.obitech.crundi.ui.operations.SecretsState
import mv.obitech.crundi.ui.operations.UpgradeForm
import mv.obitech.crundi.ui.operations.UpgradeLockCard
import mv.obitech.crundi.ui.theme.CrundiColors
import org.junit.Test

/**
 * Passphrases and the lock upgrade: the request card asking for a
 * passphrase, the "set a new passphrase" prompt after an old PIN, the edit
 * dialog, the list marking old locks, the add form, the pre-authorise dialog
 * with a passphrase field, and the PIN window asking for a passphrase.
 */
class SecretLockScreens : ScreenshotTest() {
    private val noChat = object : ChatActions {}
    private val pane = FakeApi.state.chats("crundi").first()

    @Composable
    private fun InDialog(content: @Composable () -> Unit) {
        Box(Modifier.fillMaxSize().background(CrundiColors.Scrim).padding(16.dp), contentAlignment = Alignment.Center) { content() }
    }

    private val passReq = SecretRequest("r3", "s4", "GITHUB_DEPLOY_TOKEN", "crundi", "Push the release tag", kind = "run", command = "git push origin v1.21.2", envName = "GH_TOKEN", lock = "passphrase")

    @Test fun requestPassphrase() = shoot("secrets-request-passphrase") {
        Box(Modifier.fillMaxSize()) {
            ChatScreen(ChatUi(FakeApi.chat(0), pane = pane), noChat)
            SecretPromptDock(listOf(passReq), SecretPromptState(armed = "r3", pin = "correct horse"), canSave = { true })
        }
    }

    @Test fun upgradeAfterOldPin() = shoot("secrets-upgrade-lock") {
        Box(Modifier.fillMaxSize()) {
            ChatScreen(ChatUi(FakeApi.chat(0), pane = pane), noChat)
            InDialog { UpgradeLockCard("STRIPE_API_KEY", "pin", UpgradeForm(a = "stripe-live-2026", b = "stripe-live-2026"), {}, {}, {}, canSave = true) }
        }
    }

    @Test fun upgradeRuleNotMet() = shoot("secrets-upgrade-lock-rule") {
        InDialog { UpgradeLockCard("STRIPE_API_KEY", "pin", UpgradeForm(a = "12345678"), {}, {}, {}) }
    }

    private val old = Secret("s9", "AWS_ACCESS_KEY", "S3 backups", "2026-05-01T10:00:00Z", lock = "pin", needsUpgrade = true)

    @Test fun edit() = shoot("secrets-edit") {
        InDialog { EditSecretCard(old, EditForm.of(old).copy(relock = true, a = "aws-backups-key", b = "aws-backups-key", current = "135790"), {}, {}, {}, canSave = true) }
    }

    @Test fun listOldLocks() = shoot("secrets-list-old-locks") {
        SecretsScreen(
            SecretsState(Load.Ready(listOf(
                Secret("s4", "GITHUB_DEPLOY_TOKEN", "Push to the release repo", "2026-10-26T05:00:00Z", lock = "passphrase"),
                old,
                Secret("s5", "OPENAI_KEY", "Captions", "2026-04-01T10:00:00Z", lock = "passphrase", needsUpgrade = true),
            ))),
            passphrase = true, canEdit = true,
        )
    }

    @Test fun addPassphrase() = shoot("secrets-add-passphrase") {
        InDialog { DialogCard("Add a secret", dismiss = null) { AddSecretForm({}, { _, _, _, _ -> }, passphrase = true) } }
    }

    @Test fun pinWindowPassphrase() = shoot("secrets-pin-passphrase") {
        InDialog { PinCard(PinAsk("Reveal secret", "Enter the passphrase for \"GITHUB_DEPLOY_TOKEN\".", "Reveal", lock = "passphrase") {}, "correct horse", {}, null, false, {}, {}) }
    }

    @Test fun preauthorisePassphrase() = shoot("chats-secrets-dialog-passphrase") {
        Box(Modifier.fillMaxSize()) {
            ChatScreen(ChatUi(FakeApi.chat(0), pane = pane, secretsOk = true), noChat)
            Box(Modifier.fillMaxSize().padding(16.dp), contentAlignment = Alignment.Center) {
                ChatSecretsCard(
                    ChatSecretsForm(
                        secrets = listOf(Secret("s4", "GITHUB_DEPLOY_TOKEN", lock = "passphrase"), Secret("s9", "AWS_ACCESS_KEY", needsUpgrade = true), Secret("s3", "OPENAI_KEY", lock = "passphrase")),
                        chosen = listOf("s4", "s9"), pins = mapOf("s4" to "correct horse", "s9" to "1357"),
                    ),
                    System.currentTimeMillis(), ChatSecretsActions(),
                )
            }
        }
    }
}
