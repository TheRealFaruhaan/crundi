package mv.obitech.crundi.ui.workbench

import android.widget.Toast
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.navigation.NavGraphBuilder
import androidx.navigation.compose.composable
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.shell.Area

/** The Workbench tab (Lead). */
fun NavGraphBuilder.workbenchGraph() {
    composable(Area.Workbench.key) { WorkbenchRoute() }
}

/** The Workbench with live data: the current project's controller from [LocalWorkbench]. */
@Composable
fun WorkbenchRoute() {
    val wb = LocalWorkbench.current ?: return
    val ui = wb.ui()
    val cols = ui.tree.columns()
    val pager = rememberPagerState { cols.size }
    // A pane was just added or asked for: bring its column into view.
    LaunchedEffect(wb.reveal, cols.size) {
        val k = wb.reveal ?: return@LaunchedEffect
        val i = ui.tree.columnOf(k)
        if (i >= 0 && i < pager.pageCount) pager.animateScrollToPage(i)
        wb.reveal = null
    }
    WorkbenchScreen(ui, wb, pager = pager) { v -> PaneBody(v, wb) }

    wb.renaming?.let { v ->
        var name by remember(v.key) { mutableStateOf(v.title) }
        CrundiDialog(
            if (v.kind == PaneKind.Chat) "Rename chat" else "Rename terminal",
            onDismiss = { wb.renaming = null },
            confirm = "Rename", onConfirm = { wb.renaming = null; wb.applyRename(v, name) },
        ) { CrundiTextField(name, { name = it.take(80) }, placeholder = "Name", modifier = Modifier.fillMaxWidth()) }
    }
    val ctx = LocalContext.current
    LaunchedEffect(wb.notice) {
        wb.notice?.let { Toast.makeText(ctx.applicationContext, it, Toast.LENGTH_LONG).show(); wb.notice = null }
    }
}
