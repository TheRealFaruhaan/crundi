package mv.obitech.crundi.ui.panels

import androidx.compose.runtime.Composable
import mv.obitech.crundi.ui.shell.NotBuiltYet
import mv.obitech.crundi.ui.workbench.WbCell

/**
 * Owned by ui.operations. Panels (the web's widgets): the Panels tab with its
 * sideways list, a Panel pane in the workbench, a chat's widget dock, and the
 * top bar's panel chips. The other place a WebView is allowed (approved
 * exception, see android-architecture.md), and only to render widget HTML.
 */
@Composable fun PanelsRoute() = NotBuiltYet("Panels", "panel", "ui.operations")

/** A Panel pane. [cell].data: widgetId (empty: let the person pick one). */
@Composable fun PanelPane(cell: WbCell?) = NotBuiltYet("Panel", "panel", "ui.operations")
