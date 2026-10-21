package mv.obitech.crundi.ui.workbench

import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
import mv.obitech.crundi.ui.chats.ChatRoute
import mv.obitech.crundi.ui.planning.PlanningPane
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
            PaneKind.Parked -> ParkedBody(v, { wb.resume(v) }, busy = wb.resuming[v.key] == true, preview = {
                val id = v.paneId
                when {
                    id == null -> GhostLines()
                    v.parkedAs == PaneKind.Chat -> mv.obitech.crundi.ui.chats.ParkedChatPreview(id)                 // ui.chats
                    else -> mv.obitech.crundi.ui.workspace.terminal.TerminalPreview(id)                          // ui.workspace
                }
            }) // Lead
            PaneKind.Pending -> TerminalLauncher({ wb.launch(v.key, it) }, busy = wb.launching[v.key] == true, error = wb.launchErrors[v.key]) // Lead
            PaneKind.Files -> FilesRoute()                                                  // ui.workspace
            PaneKind.Git -> GitRoute()                                                      // ui.workspace
            PaneKind.Media -> MediaRoute()                                                  // ui.workspace
            PaneKind.Browser -> mv.obitech.crundi.ui.browser.BrowserPane(v.cell)            // ui.workspace (WebView exception)
            PaneKind.Kanban -> PlanningPane(v)                                              // ui.planning
            PaneKind.Notes -> PlanningPane(v)                                               // ui.planning (picker, then one page: data pageId)
            PaneKind.MindMap -> PlanningPane(v)                                             // ui.planning
            PaneKind.Factory -> PlanningPane(v)                                             // ui.planning (factories, then one run: data runId)
            PaneKind.Panel -> mv.obitech.crundi.ui.panels.PanelPane(v.cell)                  // ui.operations (WebView exception)
        }
    }
}
