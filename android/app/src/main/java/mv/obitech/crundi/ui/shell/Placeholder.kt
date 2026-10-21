package mv.obitech.crundi.ui.shell

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.Modifier
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import mv.obitech.crundi.ui.components.EmptyState

/**
 * FOUNDATION ONLY: what an area shows until its developer builds it. Every
 * use of this must be gone before the run is done (the tester greps for it).
 */
@Composable
fun AreaPlaceholder(area: Area, modifier: Modifier = Modifier) {
    EmptyState(icon = area.icon, title = area.label, body = "Not built yet. ${area.owner} builds this screen.", modifier = modifier)
}

/**
 * Tells the window a chat is in front of the person while this is composed
 * and the screen is resumed (see Host.chatOnScreen). Every screen that shows
 * a chat calls it once with the chat's id.
 */
@Composable
fun ChatOnScreenEffect(chatId: String, host: Host) {
    val owner = LocalLifecycleOwner.current
    DisposableEffect(chatId, owner, host) {
        val obs = LifecycleEventObserver { _, e ->
            when (e) {
                Lifecycle.Event.ON_RESUME -> host.chatOnScreen(chatId, true)
                Lifecycle.Event.ON_PAUSE -> host.chatOnScreen(chatId, false)
                else -> {}
            }
        }
        owner.lifecycle.addObserver(obs)
        if (owner.lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) host.chatOnScreen(chatId, true)
        onDispose {
            owner.lifecycle.removeObserver(obs)
            host.chatOnScreen(chatId, false)
        }
    }
}
