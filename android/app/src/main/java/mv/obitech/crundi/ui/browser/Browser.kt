package mv.obitech.crundi.ui.browser

import androidx.compose.runtime.Composable
import mv.obitech.crundi.ui.shell.NotBuiltYet
import mv.obitech.crundi.ui.workbench.WbCell

/**
 * Owned by ui.workspace. The Browsers tab and the Browser pane: one of the
 * two places a WebView is allowed (approved exception, see
 * android-architecture.md): URL bar, back / forward / reload, and the
 * desktop's device emulation (BRZ_DEVICES in src/webapp-html.js).
 */
@Composable fun BrowsersRoute() = NotBuiltYet("Browsers", "globe", "ui.workspace")

/** A Browser pane in the workbench. [cell].data: url, device. */
@Composable fun BrowserPane(cell: WbCell?) = NotBuiltYet("Browser", "globe", "ui.workspace")
