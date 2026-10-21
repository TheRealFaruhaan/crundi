package mv.obitech.crundi.ui.workspace

import androidx.navigation.NavGraphBuilder
import androidx.navigation.NavType
import androidx.navigation.navArgument
import androidx.navigation.compose.composable
import mv.obitech.crundi.ui.shell.Area
import mv.obitech.crundi.ui.workspace.files.FileViewRoute
import mv.obitech.crundi.ui.workspace.files.FilesRoute
import mv.obitech.crundi.ui.workspace.git.CommitRoute
import mv.obitech.crundi.ui.workspace.git.DiffRoute
import mv.obitech.crundi.ui.workspace.git.GitRoute
import mv.obitech.crundi.ui.workspace.git.HistoryRoute
import mv.obitech.crundi.ui.workspace.media.MediaRoute
import mv.obitech.crundi.ui.workspace.media.MediaViewRoute
import mv.obitech.crundi.ui.workspace.terminal.TerminalRoute

/**
 * Owned by the workspace developer: Git, Files, Media, and the terminal
 * screens (the Terminals tab's front page is registered by chatsGraph and
 * shows [mv.obitech.crundi.ui.workspace.terminal.TerminalsRoute]). Detail
 * routes are in [WorkspaceRoutes], each prefixed with its area key.
 */
fun NavGraphBuilder.workspaceGraph() {
    composable(Area.Git.key) { GitRoute() }
    composable(WorkspaceRoutes.DIFF, arguments = listOf(str("file"), str("staged"))) { e -> DiffRoute(e.arguments?.getString("file").orEmpty(), e.arguments?.getString("staged") == "true") }
    composable(WorkspaceRoutes.HISTORY) { HistoryRoute() }
    composable(WorkspaceRoutes.COMMIT) { e -> CommitRoute(e.arguments?.getString("hash").orEmpty()) }

    composable(Area.Files.key) { FilesRoute() }
    composable(WorkspaceRoutes.FILE, arguments = listOf(str("path"))) { e -> FileViewRoute(e.arguments?.getString("path").orEmpty()) }

    composable(Area.Media.key) { MediaRoute() }
    composable(WorkspaceRoutes.MEDIA_ITEM) { e -> MediaViewRoute(e.arguments?.getString("id").orEmpty()) }

    composable(WorkspaceRoutes.TERMINAL) { e -> TerminalRoute(e.arguments?.getString("id").orEmpty()) }
}

private fun str(name: String) = navArgument(name) { type = NavType.StringType; defaultValue = "" }
