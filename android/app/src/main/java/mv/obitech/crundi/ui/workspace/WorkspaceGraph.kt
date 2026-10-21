package mv.obitech.crundi.ui.workspace

import androidx.compose.runtime.Composable
import androidx.navigation.NavGraphBuilder
import androidx.navigation.compose.composable
import mv.obitech.crundi.ui.shell.Area
import mv.obitech.crundi.ui.shell.AreaPlaceholder

/** Owned by the workspace developer: Git, Files, Media. Add detail routes here, prefixed with the area key. */
fun NavGraphBuilder.workspaceGraph() {
    composable(Area.Git.key) { GitScreen() }
    composable(Area.Files.key) { FilesScreen() }
    composable(Area.Media.key) { MediaScreen() }
}

@Composable fun GitScreen() = AreaPlaceholder(Area.Git)
@Composable fun FilesScreen() = AreaPlaceholder(Area.Files)
@Composable fun MediaScreen() = AreaPlaceholder(Area.Media)
