package mv.obitech.crundi.ui.workbench

import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChanged
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
      // A browser, a panel or a terminal is an Android view inside the pane:
      // it gets the touch but does not mark it used, so the row of columns
      // took every sideways drag and you could not pan or select in it.
      // Here those drags are marked used once the view has had them, and the
      // columns are swiped from the pane's header or another pane instead.
      // The browser and panels hold their own touches at the view (ClaimTouchesFrame);
      // marking them used here as well reached those views as cancelled.
      val owns = v.kind == PaneKind.Terminal
      androidx.compose.foundation.layout.Box(if (owns) androidx.compose.ui.Modifier.keepTouches() else androidx.compose.ui.Modifier) {
        when (v.kind) {
            PaneKind.Chat -> ChatRoute(v.paneId!!)                                          // ui.chats
            PaneKind.Terminal -> TerminalRoute(v.paneId!!)                                  // ui.workspace
            PaneKind.Parked -> ParkedBody(v, { mode -> wb.resume(v, mode) }, busy = wb.resuming[v.key] == true, busyMode = wb.resumingMode[v.key] ?: "resume", preview = {
                val id = v.paneId
                when {
                    id == null -> GhostLines()
                    v.parkedAs == PaneKind.Chat -> mv.obitech.crundi.ui.chats.ParkedChatPreview(id)                 // ui.chats
                    else -> mv.obitech.crundi.ui.workspace.terminal.TerminalPreview(id)                          // ui.workspace
                }
            }) // Lead
            PaneKind.Pending -> TerminalLauncher({ wb.pick(v.key, it) }, busy = wb.launching[v.key] == true, error = wb.launchErrors[v.key]) // Lead
            PaneKind.Files -> FilesRoute()                                                  // ui.workspace
            PaneKind.Git -> GitRoute()                                                      // ui.workspace
            PaneKind.Media -> MediaRoute()                                                  // ui.workspace
            PaneKind.Browser -> mv.obitech.crundi.ui.browser.BrowserPane(v.cell)            // ui.workspace (WebView exception)
            PaneKind.Kanban -> PlanningPane(v)                                              // ui.planning
            PaneKind.Notes -> PlanningPane(v)                                               // ui.planning (picker, then one page: data pageId)
            PaneKind.MindMap -> PlanningPane(v)                                             // ui.planning
            PaneKind.Factory -> PlanningPane(v)                                             // ui.planning (the project's factory: its live graph during a run)
            PaneKind.Panel -> mv.obitech.crundi.ui.panels.PanelPane(v.cell)                  // ui.operations (WebView exception)
        }
      }
    }
}

/** Mark every move inside as used after the children have seen it, so a parent's swipe never starts here. */
private fun androidx.compose.ui.Modifier.keepTouches(): androidx.compose.ui.Modifier = this.then(
    androidx.compose.ui.Modifier.pointerInput(Unit) {
        awaitPointerEventScope {
            while (true) {
                val e = awaitPointerEvent(androidx.compose.ui.input.pointer.PointerEventPass.Main)
                for (c in e.changes) if (c.positionChanged()) c.consume()
            }
        }
    },
)
