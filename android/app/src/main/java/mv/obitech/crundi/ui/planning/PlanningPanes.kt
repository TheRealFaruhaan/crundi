package mv.obitech.crundi.ui.planning

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import mv.obitech.crundi.ui.components.CrundiDivider
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens
import mv.obitech.crundi.ui.workbench.LocalPane
import mv.obitech.crundi.ui.workbench.LocalWorkbench
import mv.obitech.crundi.ui.workbench.PaneKind
import mv.obitech.crundi.ui.workbench.PaneView

/**
 * The planning areas inside a workbench pane (PaneBodies points its Kanban,
 * Notes, Mindmap and Factory lines here). As on the web:
 * - a Notes pane is a page picker until a page is chosen, then that one page
 *   (cell data pageId, its title shown in the pane header); "Pages" goes back
 *   to the picker, and a page open in another pane cannot be opened twice;
 * - a Factory pane is the project's Factory panel (FactoryPane.kt): while a
 *   run is going, its live graph fills the pane; with none, Start, earlier
 *   runs and triggers. A run opened in the pane by an older version (cell
 *   data runId) is let go: the pane follows the project's run by itself.
 * The pane's text size (A-/A+) scales every text in it.
 */
@Composable
fun PlanningPane(v: PaneView) = PaneScaled {
    val wb = LocalWorkbench.current
    val data = v.cell?.data.orEmpty()
    when (v.kind) {
        PaneKind.Kanban -> KanbanRoute()
        PaneKind.MindMap -> MindMapRoute()
        PaneKind.Notes -> {
            val pageId = data["pageId"]
            if (pageId != null) NotePageRoute(
                pageId,
                pane = NotePaneHooks(
                    onPages = { wb?.setCellData(v.key, "pageId" to null, "title" to null) },
                    onTitle = { t -> if (data["title"] != t) wb?.setCellData(v.key, "title" to t.ifBlank { "Untitled" }) },
                ),
            ) else NotesRoute(
                pane = NotePickerHooks(
                    busy = { id -> wb?.cells.orEmpty().any { it.kind == PaneKind.Notes && it.key != v.key && it.data["pageId"] == id } },
                    onPick = { id, title -> wb?.setCellData(v.key, "pageId" to id, "title" to title.ifBlank { "Untitled" }) },
                ),
            )
        }
        PaneKind.Factory -> {
            androidx.compose.runtime.LaunchedEffect(data["runId"]) { if (data["runId"] != null) wb?.setCellData(v.key, "runId" to null, "title" to null) }
            FactoryPaneRoute()
        }
        else -> Unit
    }
}

/** What a page in a Notes pane does instead of leaving: back to the picker, and its title to the pane header. */
class NotePaneHooks(val onPages: () -> Unit, val onTitle: (String) -> Unit)

/** What the picker in a Notes pane does instead of leaving: open the page in this pane, unless another pane has it. */
class NotePickerHooks(val busy: (String) -> Boolean, val onPick: (id: String, title: String) -> Unit)

/** The pane's text size (A-/A+ in its "..." menu) applied to everything inside. Outside a pane, nothing changes. */
@Composable
fun PaneScaled(content: @Composable () -> Unit) {
    val s = LocalPane.current?.fontScale ?: 1f
    if (s == 1f) { content(); return }
    val d = LocalDensity.current
    CompositionLocalProvider(LocalDensity provides Density(d.density, d.fontScale * s)) { content() }
}

/** True when this screen is a workbench pane's body (the frame draws the title, so leave the top bar out). */
@Composable
fun inPane(): Boolean = LocalPane.current != null

/**
 * The slim row a detail screen shows inside a pane in place of its top bar:
 * a way back within the pane ("Pages", "All runs") and the screen's own tools.
 */
@Composable
fun PaneSubBar(backLabel: String, onBack: () -> Unit, actions: @Composable RowScope.() -> Unit = {}) {
    Column(Modifier.fillMaxWidth()) {
        Row(Modifier.fillMaxWidth().height(40.dp).padding(horizontal = 6.dp), verticalAlignment = Alignment.CenterVertically) {
            Row(
                Modifier.clip(Dimens.Shape).clickable(onClick = onBack).padding(horizontal = 6.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                CrundiIcon("chevron-left", size = Dimens.IconSm, tint = CrundiColors.TextSecondary)
                Text(backLabel, style = CrundiType.Small.copy(color = CrundiColors.TextSecondary), modifier = Modifier.padding(start = 4.dp))
            }
            Spacer(Modifier.weight(1f))
            actions()
        }
        CrundiDivider()
    }
}
