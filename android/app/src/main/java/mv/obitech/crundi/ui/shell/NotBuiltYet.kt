package mv.obitech.crundi.ui.shell

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import mv.obitech.crundi.ui.components.EmptyState

/**
 * CHANGE 2 ONLY: what a new area or pane shows until its owner builds it.
 * Every use must be gone before the run is done (grep NotBuiltYet).
 */
@Composable
fun NotBuiltYet(what: String, icon: String, owner: String, modifier: Modifier = Modifier) {
    EmptyState(icon = icon, title = what, body = "Not built yet. $owner builds this.", modifier = modifier)
}
