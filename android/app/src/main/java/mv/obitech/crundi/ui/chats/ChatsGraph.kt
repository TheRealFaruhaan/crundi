package mv.obitech.crundi.ui.chats

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.navigation.NavGraphBuilder
import androidx.navigation.compose.composable
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.components.TopBar
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.nav.Routes
import mv.obitech.crundi.ui.shell.Area
import mv.obitech.crundi.ui.shell.AreaPlaceholder
import mv.obitech.crundi.ui.shell.ChatOnScreenEffect
import mv.obitech.crundi.ui.shell.LocalHost
import mv.obitech.crundi.ui.theme.CrundiColors

/** Owned by the chats developer: Chats, Terminals, one chat, the approvals list. */
fun NavGraphBuilder.chatsGraph() {
    composable(Area.Chats.key) { ChatsScreen() }
    composable(Area.Terminals.key) { TerminalsScreen() }
    composable(Routes.CHAT) { e -> ChatRoute(e.arguments?.getString("chatId").orEmpty()) }
    composable(Routes.APPROVALS) { ApprovalsScreen() }
}

@Composable fun ChatsScreen() = AreaPlaceholder(Area.Chats)
@Composable fun TerminalsScreen() = AreaPlaceholder(Area.Terminals)

@Composable
fun ApprovalsScreen() {
    val nav = LocalNavigator.current
    Column(Modifier.fillMaxSize().background(CrundiColors.BgPrimary)) {
        TopBar("Waiting on you", onBack = { nav.back() })
        EmptyState("bell", "Waiting on you", body = "Not built yet. ui.chats builds this screen.")
    }
}

/**
 * One chat. The same composable is the main window's chat screen and the
 * whole of a bubble (ChatActivity, `LocalHost.current.isBubble`): in a bubble
 * the back arrow closes the bubble and a menu item opens the chat in the app.
 */
@Composable
fun ChatRoute(chatId: String) {
    val host = LocalHost.current
    val nav = LocalNavigator.current
    ChatOnScreenEffect(chatId, host)
    Column(Modifier.fillMaxSize().background(CrundiColors.BgPrimary)) {
        TopBar(
            "Chat",
            subtitle = chatId,
            onBack = { if (host.isBubble) host.close() else nav.back() },
            actions = {
                if (host.isBubble) CrundiIconButton("maximize", { host.openInMain("/chat/$chatId") }, contentDescription = "Open in Crundi")
                else CrundiIconButton("layers", { host.openBubble(chatId) }, contentDescription = "Open as bubble")
            },
        )
        EmptyState("message-square", "Chat", body = "Not built yet. ui.chats builds this screen.")
    }
}
