package mv.obitech.crundi.ui.workbench

import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
import mv.obitech.crundi.ui.chats.ChatRoute
import mv.obitech.crundi.ui.planning.FactoriesRoute
import mv.obitech.crundi.ui.planning.FactoryRunRoute
import mv.obitech.crundi.ui.planning.KanbanRoute
import mv.obitech.crundi.ui.planning.MindMapRoute
import mv.obitech.crundi.ui.planning.NotePageRoute
import mv.obitech.crundi.ui.planning.NotesRoute
import mv.obitech.crundi.ui.workspace.files.FilesRoute
import mv.obitech.crundi.ui.workspace.git.GitRoute
import mv.obitech.crundi.ui.workspace.media.MediaRoute
import mv.obitech.crundi.ui.workspace.terminal.TerminalRoute

/**
 * What each kind of pane shows: the area screens, reused inside the pane.
 * One line per kind, and each line belongs to the kind's owner (see
 * android-architecture.md): change your own line to point at your pane
 * screen; leave the others. Inside, LocalPane says the screen is in a pane
 * (leave out a header the frame already draws; honour fontScale).
 */
@Composable
fun PaneBody(v: PaneView, wb: WorkbenchController) {
    key(wb.refreshes[v.key] ?: 0) {
        when (v.kind) {
            PaneKind.Chat -> ChatRoute(v.paneId!!)                                          // ui.chats
            PaneKind.Terminal -> TerminalRoute(v.paneId!!)                                  // ui.workspace
            PaneKind.Parked -> ParkedBody(v, { wb.resume(v) }, busy = wb.resuming[v.key] == true, preview = { val id = v.paneId; if (v.parkedAs == PaneKind.Chat && id != null) mv.obitech.crundi.ui.chats.ParkedChatPreview(id) else GhostLines() }) // Lead (preview: ui.chats / ui.workspace)
            PaneKind.Pending -> TerminalLauncher({ wb.launch(v.key, it) }, busy = wb.launching[v.key] == true, error = wb.launchErrors[v.key]) // Lead
            PaneKind.Files -> FilesRoute()                                                  // ui.workspace
            PaneKind.Git -> GitRoute()                                                      // ui.workspace
            PaneKind.Media -> MediaRoute()                                                  // ui.workspace
            PaneKind.Browser -> mv.obitech.crundi.ui.browser.BrowserPane(v.cell)            // ui.workspace (WebView exception)
            PaneKind.Kanban -> KanbanRoute()                                                // ui.planning
            PaneKind.Notes -> v.cell?.data?.get("pageId")?.let { NotePageRoute(it) } ?: NotesRoute() // ui.planning
            PaneKind.MindMap -> MindMapRoute()                                              // ui.planning
            PaneKind.Factory -> v.cell?.data?.get("runId")?.let { FactoryRunRoute(it) } ?: FactoriesRoute() // ui.planning
            PaneKind.Panel -> mv.obitech.crundi.ui.panels.PanelPane(v.cell)                  // ui.operations (WebView exception)
        }
    }
}
