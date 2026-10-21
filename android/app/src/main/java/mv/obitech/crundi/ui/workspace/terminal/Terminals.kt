package mv.obitech.crundi.ui.workspace.terminal

import mv.obitech.crundi.ui.components.liveErrorText

import mv.obitech.crundi.ui.components.ErrorState

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import mv.obitech.crundi.api.LiveStatus
import mv.obitech.crundi.api.TerminalStream
import mv.obitech.crundi.api.model.AgentState
import mv.obitech.crundi.api.model.Pane
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.Banner
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.DotState
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.components.ListRow
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.PanelHeader
import mv.obitech.crundi.ui.components.SectionHeader
import mv.obitech.crundi.ui.components.StateDot
import mv.obitech.crundi.ui.components.TopBar
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.workspace.WorkspaceRoutes
import mv.obitech.crundi.ui.workspace.rememberFilePicker
import mv.obitech.crundi.ui.workbench.LocalPane
import mv.obitech.crundi.ui.workbench.LocalWorkbench
import mv.obitech.crundi.ui.workbench.WbLayout
import mv.obitech.crundi.ui.components.errorText
import mv.obitech.crundi.ui.workspace.rememberToast

// ─── State words ───

fun dotOf(p: Pane): DotState = when (p.state) {
    AgentState.Working -> DotState.Working
    AgentState.NeedsInput -> DotState.NeedsInput
    AgentState.Waiting -> DotState.Waiting
    AgentState.Idle -> DotState.Running
    AgentState.Exited -> if (p.status == "running") DotState.Running else DotState.Idle
}

fun stateWords(p: Pane): String = when (p.state) {
    AgentState.Working -> "working"
    AgentState.NeedsInput -> "needs input"
    AgentState.Waiting -> "waiting"
    AgentState.Idle -> "idle"
    AgentState.Exited -> if (p.status == "exited") "exited" else "running"
}

// ─── One terminal ───

/** Everything a terminal screen draws, besides the emulator itself. */
data class TerminalUi(
    val title: String,
    val subtitle: String = "",
    val dot: DotState = DotState.Running,
    val exited: Int? = null,
    val fontSize: Int = 12,
    val input: String = "",
    val connected: Boolean = true,
    /** The keyboard tools panel is open. */
    val tools: Boolean = false,
    /** Select mode: plain selectable text instead of the live view. */
    val selecting: Boolean = false,
    val upload: TermUpload? = null,
    /** The workbench is in swipe columns (lights Layout); null outside the workbench (no Layout button). */
    val columns: Boolean? = null,
)

/**
 * One terminal. Inside a workbench pane ([inPane]) the pane frame draws the
 * title, dot, text size and rename, so this draws only the view, the tools
 * and the input bar, as the mobile web's terminal cell does. Full screen from
 * a notification or a route, it has its own top bar.
 */
@Composable
fun TerminalScreen(
    ui: TerminalUi,
    emu: TermEmulator,
    version: Long,
    inPane: Boolean = false,
    onBack: () -> Unit = {},
    onSend: (String) -> Unit = {},
    onInput: (String) -> Unit = {},
    onSubmit: () -> Unit = {},
    onFont: (Int) -> Unit = {},
    onRename: () -> Unit = {},
    onTools: () -> Unit = {},
    onSelect: () -> Unit = {},
    onAttach: (() -> Unit)? = null,
    onLayout: (() -> Unit)? = null,
    onSize: (Int, Int) -> Unit = { _, _ -> },
    radialAt: androidx.compose.ui.geometry.Offset? = null,
    startAtTop: Boolean = false,
    modifier: Modifier = Modifier,
) {
    Column(modifier.fillMaxSize().background(CrundiColors.BgPrimary).then(if (inPane) Modifier else Modifier.imePadding())) {
        if (!inPane) TopBar(ui.title, subtitle = ui.subtitle, onBack = onBack) {
            StateDot(ui.dot)
            CrundiIconButton("minus", { onFont(-1) }, contentDescription = "Smaller text", size = 16.dp)
            CrundiIconButton("plus", { onFont(1) }, contentDescription = "Larger text", size = 16.dp)
            CrundiIconButton("pencil", onRename, contentDescription = "Rename terminal", size = 16.dp)
        }
        if (ui.exited != null) Banner("The process ended (code ${ui.exited}). Close this terminal or open a new one.", Modifier.fillMaxWidth().padding(8.dp), kind = BadgeKind.Warn, icon = "alert-triangle")
        else if (!ui.connected) Banner("Reconnecting to the server", Modifier.fillMaxWidth().padding(8.dp), kind = BadgeKind.Warn, icon = "refresh")
        val appCursor = emu.appCursorKeys
        androidx.compose.foundation.layout.Box(Modifier.weight(1f).fillMaxWidth()) {
            TerminalView(
                emu, version, ui.fontSize, Modifier.fillMaxSize(), onSize = onSize,
                onKey = if (ui.exited == null) onSend else null,
                selecting = ui.selecting, onSelect = onSelect, radialAt = radialAt, follow = !startAtTop,
            )
            if (ui.tools) TermToolPanel(
                onKey = { k -> onSend(TermKeyMap.bytes(k, appCursor)) },
                onLayout = onLayout?.takeIf { ui.columns != null },
                columns = ui.columns == true,
                modifier = Modifier.align(Alignment.BottomCenter).padding(8.dp),
            )
        }
        TermInputBar(
            ui.input, onInput, onSubmit,
            toolsOpen = ui.tools, onTools = onTools,
            onAttach = onAttach, upload = ui.upload,
            enabled = ui.exited == null,
        )
    }
}

/** One terminal, live: output into the emulator, keys and text back out. */
@Composable
fun TerminalRoute(id: String) {
    val services = LocalAppServices.current
    val api = services.api
    val nav = LocalNavigator.current
    val toast = rememberToast()
    val scope = rememberCoroutineScope()
    val pane = LocalPane.current
    val wb = LocalWorkbench.current
    val stream: TerminalStream = remember(id) { api.terminal(id) }
    val emu = remember(id) { TermEmulator(60, 30) }
    var version by remember { mutableLongStateOf(0L) }
    var exited by remember { mutableStateOf<Int?>(null) }
    var fontSize by rememberSaveable { mutableIntStateOf(12) }
    var input by rememberSaveable { mutableStateOf("") }
    var tools by rememberSaveable { mutableStateOf(false) }
    var selecting by remember { mutableStateOf(false) }
    var upload by remember { mutableStateOf<TermUpload?>(null) }
    var renaming by remember { mutableStateOf(false) }
    val live by remember(api, id) { api.events.state.map { s -> s?.terminals?.firstOrNull { it.id == id } } }.collectAsState(initial = null)
    val status by remember(api) { api.socket.status }.collectAsState(initial = null)
    val project = live?.project ?: services.shell.state.collectAsState().value.currentProject

    LaunchedEffect(id) {
        emu.respond = { stream.input(it) }
        stream.output.collect { chunk ->
            emu.write(chunk)
            TerminalStream.exitCode(chunk)?.let { exited = it }
            version = emu.version
        }
    }
    fun send(s: String) { if (exited == null && s.isNotEmpty()) stream.input(s) }
    val pick = rememberFilePicker { files ->
        val p = project ?: return@rememberFilePicker
        scope.launch {
            for (f in files) {
                upload = TermUpload(f.name, 0, f.size)
                runCatching { api.chats.attach(p, f.name, f.size, progress = { d, t -> upload = TermUpload(f.name, d, t) }, open = f.open) }
                    .onSuccess { a -> input = if (input.isEmpty() || input.endsWith(" ")) input + a.path else input + " " + a.path }
                    .onFailure { toast("Upload failed: " + errorText(it)) }
            }
            upload = null
        }
    }
    // In a pane the frame's A- / A+ sets the size, as the web's pane menu does.
    val size = if (pane != null) (12 * pane.fontScale).toInt().coerceIn(8, 22) else fontSize
    // The key panel steps aside while a chat pane is the one in use (spec 8, as the web's mobile tool panel).
    val chatFocused = pane != null && wb?.focused?.let { k -> wb.panes()[k]?.kind == mv.obitech.crundi.ui.workbench.PaneKind.Chat } == true
    TerminalScreen(
        TerminalUi(
            title = live?.label?.ifEmpty { null } ?: emu.title ?: "Terminal",
            subtitle = (live?.project ?: "") + (live?.let { " · " + stateWords(it) } ?: ""),
            dot = live?.let(::dotOf) ?: DotState.Idle,
            exited = exited, fontSize = size, input = input,
            connected = status != LiveStatus.Reconnecting && status != LiveStatus.Offline,
            tools = tools && !chatFocused, selecting = selecting, upload = upload,
            columns = if (pane != null && wb != null) wb.layout == WbLayout.Columns else null,
        ),
        emu, version,
        inPane = pane != null && !pane.full,
        onBack = { nav.back() },
        onSend = ::send,
        onInput = { input = it },
        onSubmit = {
            val text = input
            input = ""
            if (text.isEmpty()) send(TermKeys.ENTER)
            else if (emu.bracketedPaste && text.contains('\n')) send("\u001b[200~$text\u001b[201~\r")
            else send(text + TermKeys.ENTER)
        },
        onFont = { d -> fontSize = (fontSize + d).coerceIn(8, 20) },
        onRename = { renaming = true },
        onTools = { tools = !tools },
        onSelect = { selecting = !selecting },
        onAttach = if (project != null) pick else null,
        onLayout = wb?.let { { it.toggleLayout() } },
        onSize = { c, r -> emu.resize(c, r); version = emu.version; stream.resize(c, r) },
    )
    if (renaming) {
        var name by remember { mutableStateOf(live?.title.orEmpty()) }
        CrundiDialog(
            "Rename terminal", onDismiss = { renaming = false }, confirm = "Rename",
            onConfirm = {
                renaming = false
                scope.launch { runCatching { api.terminals.rename(id, name.trim()) }.onFailure { toast("Could not rename it: " + errorText(it)) } }
            },
        ) { CrundiTextField(name, { name = it }, placeholder = "Terminal", modifier = Modifier.fillMaxWidth()) }
    }
}

/**
 * What a parked terminal last showed (GET /api/panes/<id>/preview), drawn
 * through the emulator, for behind the parked veil's blur and Resume.
 */
@Composable
fun TerminalPreview(id: String) {
    val api = LocalAppServices.current.api
    val emu = remember(id) { TermEmulator(80, 24) }
    var version by remember { mutableLongStateOf(0L) }
    LaunchedEffect(id) {
        runCatching { api.terminals.preview(id) }.onSuccess { p -> if (p.screen.isNotEmpty()) { emu.write(p.screen); version = emu.version } }
    }
    TerminalView(emu, version, 11, Modifier.fillMaxSize())
}
