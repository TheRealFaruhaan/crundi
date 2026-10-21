package mv.obitech.crundi.ui.chats

import androidx.compose.runtime.Composable
import androidx.navigation.NavGraphBuilder
import androidx.navigation.compose.composable
import mv.obitech.crundi.ui.nav.Routes
import mv.obitech.crundi.ui.shell.Area
import mv.obitech.crundi.ui.shell.AreaPlaceholder

/** Owned by the chats developer: Chats, one chat (also the bubble), the approvals list. Terminals: ui.workspace. */
fun NavGraphBuilder.chatsGraph() {
    composable(Area.Chats.key) { ChatsRoute() }
    composable(Area.Terminals.key) { TerminalsScreen() }
    composable(Routes.CHAT) { e -> ChatRoute(e.arguments?.getString("chatId").orEmpty()) }
    composable(Routes.APPROVALS) { ApprovalsRoute() }
}

@Composable fun ChatsScreen() = AreaPlaceholder(Area.Chats)
/** Terminals are built by ui.workspace (the workspace dev's persona); this only hands over. */
@Composable fun TerminalsScreen() = mv.obitech.crundi.ui.workspace.terminal.TerminalsRoute()

