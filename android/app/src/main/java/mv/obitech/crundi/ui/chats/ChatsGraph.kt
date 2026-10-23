package mv.obitech.crundi.ui.chats

import androidx.compose.runtime.Composable
import androidx.navigation.NavGraphBuilder
import androidx.navigation.compose.composable
import mv.obitech.crundi.ui.nav.Routes
import mv.obitech.crundi.ui.shell.Area

/** Owned by the chats developer: Chats, one chat (also the bubble), the approvals list. Terminals: ui.workspace. */
fun NavGraphBuilder.chatsGraph() {
    // No Chats or Terminals tab since change 2: chats and terminals are Workbench panes.
    composable(Routes.CHAT) { e -> ChatRoute(e.arguments?.getString("chatId").orEmpty()) }
    composable(Routes.APPROVALS) { ApprovalsRoute() }
}

