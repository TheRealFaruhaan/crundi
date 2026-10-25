package mv.obitech.crundi.ui.chats

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import mv.obitech.crundi.api.model.Pane
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.DialogCard
import mv.obitech.crundi.ui.theme.CrundiType

/** The models a chat can switch to, as the desktop's picker. */
val MODELS = listOf("" to "Default", "opus" to "Opus", "sonnet" to "Sonnet", "haiku" to "Haiku")

/** The body of the rename dialog (chats and terminals). */
@Composable
fun RenameCard(what: String, value: String, onValue: (String) -> Unit, onSave: () -> Unit, onDismiss: () -> Unit, modifier: Modifier = Modifier) {
    DialogCard("Rename $what", modifier, confirm = "Rename", onConfirm = onSave, onDismiss = onDismiss) {
        CrundiTextField(value, { onValue(it.take(80)) }, placeholder = "Name", modifier = Modifier.fillMaxWidth())
        Text("${value.length}/80", style = CrundiType.Meta, modifier = Modifier.align(Alignment.End))
    }
}

/** The actions a chat offers from its header menu (the bubble's, or full screen). */
fun chatMenu(c: Pane?, inBubble: Boolean, rename: () -> Unit, bubble: () -> Unit, park: () -> Unit, close: () -> Unit, openInMain: (() -> Unit)? = null, stop: (() -> Unit)? = null, secrets: (() -> Unit)? = null, secretGrants: Int = 0): List<SheetAction> = buildList {
    // Asleep there is no Claude to rename or put away; it can still be closed for good.
    val asleep = ChatSleep.asleep(c)
    if (!asleep) add(SheetAction("Rename", "pencil", onClick = rename))
    if (inBubble) openInMain?.let { add(SheetAction("Open in Crundi", "maximize", onClick = it)) }
    else add(SheetAction("Open as bubble", "layers", detail = "Keep it floating over other apps", onClick = bubble))
    stop?.let { add(SheetAction("Stop Claude", "stop", detail = "Interrupt the turn it is on", onClick = it)) }
    // Where the header has no room for its lock (ChatSecrets.kt).
    secrets?.let { add(SheetAction("Secrets for this chat", "lock", detail = if (secretGrants > 0) "$secretGrants pre-authorised. Revoke, or add more" else "Let it run commands with a secret without asking for the PIN", onClick = it)) }
    if (!asleep) add(SheetAction("Put away", "archive", detail = "End its Claude now and keep it to resume later", onClick = park))
    if (c?.factory == null || c.factory.role.isEmpty()) add(SheetAction("Close chat", "x", danger = true, onClick = close))
}
