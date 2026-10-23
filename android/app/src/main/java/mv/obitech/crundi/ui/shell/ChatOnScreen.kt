package mv.obitech.crundi.ui.shell

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.delay

/**
 * Tells the window a chat is in front of the person while this is composed
 * and the screen is resumed (see Host.chatOnScreen). Every screen that shows
 * a chat calls it once with the chat's id.
 *
 * Said again every three seconds while it stays in front, as the page did:
 * the window trusts it for a few seconds only (OnScreen), so that a chat left
 * behind without a word stops counting. Said once, it went stale after seven
 * seconds, and a reply to the chat being read brought up its bubble.
 */
@Composable
fun ChatOnScreenEffect(chatId: String, host: Host) {
    val owner = LocalLifecycleOwner.current
    LaunchedEffect(chatId, owner, host) {
        owner.repeatOnLifecycle(Lifecycle.State.RESUMED) {
            try {
                while (true) {
                    host.chatOnScreen(chatId, true)
                    delay(3_000)
                }
            } finally {
                host.chatOnScreen(chatId, false)
            }
        }
    }
    DisposableEffect(chatId, owner, host) {
        onDispose { host.chatOnScreen(chatId, false) }
    }
}
