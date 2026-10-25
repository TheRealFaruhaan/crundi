package mv.obitech.crundi.ui.chats

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import mv.obitech.crundi.ui.operations.SecretPromptRoute

/**
 * What a bubble (ChatActivity) shows: the chat, and over it Claude asking
 * for a secret from that chat, answered with the PIN right there.
 *
 * The main window draws the secret prompt from its shell (CrundiApp), which a
 * bubble does not have, so a request from a chat open in a bubble was never
 * shown in it, whether it was already waiting when the bubble opened (from
 * the request's own notification) or came while it was open. [promptFor] is
 * the chat's id now (a chat woken from sleep can come back under another).
 */
@Composable
fun BubbleChat(chatId: String, promptFor: String = chatId) {
    Box(Modifier.fillMaxSize()) {
        ChatRoute(chatId)
        SecretPromptRoute(promptFor)
    }
}
