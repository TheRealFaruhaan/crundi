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
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.liveErrorText
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.collectAsState

/** The Workbench tab (Lead). */
fun NavGraphBuilder.workbenchGraph() {
    composable(Area.Workbench.key) { WorkbenchRoute() }
}

/** The Workbench with live data: the current project's controller from [LocalWorkbench]. */
@Composable
fun WorkbenchRoute() {
    val wb = LocalWorkbench.current ?: return
    val api = LocalAppServices.current.api
    // Nothing from the server yet: say so (loading, or why it is not coming) instead of an empty launcher.
    if (!wb.loaded && wb.project != null) {
        val status by remember(api) { api.events.status }.collectAsState()
        val failure by remember(api) { api.events.failure }.collectAsState()
        val why = liveErrorText(status, failure)
        if (why != null) ErrorState(why, Modifier.fillMaxSize(), onRetry = { api.events.reconnect() })
        else LoadingState(Modifier.fillMaxSize(), text = "Opening the workbench")
        return
    }
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
    wb.chooser?.let { c ->
        SessionChooserDialog(c, onNew = wb::chooseNew, onPick = wb::choosePick, onResume = wb::chooseResume, onBack = wb::chooserBack, onCancel = wb::cancelChooser)
    }
    val ctx = LocalContext.current
    LaunchedEffect(wb.notice) {
        wb.notice?.let { Toast.makeText(ctx.applicationContext, it, Toast.LENGTH_LONG).show(); wb.notice = null }
    }
}
