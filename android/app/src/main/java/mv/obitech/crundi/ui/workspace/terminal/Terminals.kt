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
import mv.obitech.crundi.ui.components.errorText
import mv.obitech.crundi.ui.workspace.rememberToast

// ─── The list ───

data class TerminalsState(
    val project: String? = null,
    val loading: Boolean = false,
    val terminals: List<Pane> = emptyList(),
    val parked: List<Pane> = emptyList(),
    val busy: Boolean = false,
    val notice: String? = null,
    /** Why nothing has loaded (the server cannot be reached); shown with Try again instead of the spinner. */
    val loadError: String? = null,
)

fun dotOf(p: Pane): DotState = when (p.state) {
    AgentState.Working -> DotState.Working
    AgentState.NeedsInput -> DotState.NeedsInput
    AgentState.Waiting -> DotState.Waiting
    AgentState.Idle -> DotState.Running
    AgentState.Exited -> if (p.status == "running") DotState.Running else DotState.Idle
}

private fun stateWords(p: Pane): String = when (p.state) {
    AgentState.Working -> "working"
    AgentState.NeedsInput -> "needs input"
    AgentState.Waiting -> "waiting"
    AgentState.Idle -> "idle"
    AgentState.Exited -> if (p.status == "exited") "exited" else "running"
}

/** What "New terminal" can open, as the desktop's launch cell. */
enum class TerminalKind(val label: String, val detail: String, val icon: String) {
    Shell("Empty shell", "A plain shell in the project's folder", "terminal"),
    Claude("Claude", "Claude in a terminal, asking before it acts", "terminal"),
    ClaudeSkip("Claude, skip permissions", "Claude in a terminal that never asks", "shield-alert"),
}

/** The Terminals tab: the current project's terminals, the parked ones, and New terminal. */
@Composable
fun TerminalsScreen(
    state: TerminalsState,
    onOpen: (Pane) -> Unit = {},
    onNew: (TerminalKind) -> Unit = {},
    onClose: (Pane) -> Unit = {},
    onResume: (Pane) -> Unit = {},
    modifier: Modifier = Modifier,
    showNew: Boolean = false,
    onRetry: () -> Unit = {},
) {
    var newOpen by remember { mutableStateOf(showNew) }
    var closing by remember { mutableStateOf<Pane?>(null) }
    Column(modifier.fillMaxSize().background(CrundiColors.BgPrimary)) {
        if (state.project == null) {
            EmptyState("terminal", "Pick a project", body = "Terminals belong to a project. Choose one from the menu to see its terminals.")
            return@Column
        }
        PanelHeader("Terminals", meta = if (state.terminals.isEmpty()) state.project else "${state.project} · ${state.terminals.size} open") {
            CrundiIconButton("plus", { newOpen = true }, contentDescription = "New terminal", enabled = !state.busy)
        }
        if (state.notice != null) Banner(state.notice, Modifier.fillMaxWidth().padding(12.dp), kind = BadgeKind.Bad, icon = "alert-triangle")
        when {
            state.loading && state.loadError != null -> ErrorState(state.loadError, onRetry = onRetry)
            state.loading -> LoadingState(text = "Loading terminals")
            state.terminals.isEmpty() && state.parked.isEmpty() -> EmptyState(
                "terminal", "No terminals open",
                body = "Open a shell, or Claude in a terminal, in ${state.project}.",
                action = "New terminal", onAction = { newOpen = true },
            )
            else -> LazyColumn(Modifier.fillMaxSize().padding(horizontal = 8.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                item { Column(Modifier.padding(top = 6.dp)) {} }
                items(state.terminals, key = { it.id }) { p ->
                    ListRow(
                        p.label.ifEmpty { "Terminal" },
                        subtitle = stateWords(p),
                        leading = { StateDot(dotOf(p)) },
                        trailing = { CrundiIconButton("x", { closing = p }, contentDescription = "Close terminal", size = 16.dp) },
                        onClick = { onOpen(p) },
                    )
                }
                if (state.parked.isNotEmpty()) {
                    item { SectionHeader("Parked") }
                    items(state.parked, key = { "p-" + it.id }) { p ->
                        ListRow(
                            p.label.ifEmpty { "Terminal" },
                            subtitle = if (p.reason == "restart") "Put away when Crundi restarted" else "Put away to resume later",
                            leading = { StateDot(DotState.Idle) },
                            trailing = { CrundiButton("Resume", { onResume(p) }, kind = ButtonKind.Ghost, icon = "play") },
                        )
                    }
                }
            }
        }
    }
    if (newOpen) NewTerminalDialog(onPick = { newOpen = false; onNew(it) }, onDismiss = { newOpen = false })
    closing?.let { p ->
        CrundiDialog(
            "Close terminal?", onDismiss = { closing = null },
            message = "\"${p.label.ifEmpty { "Terminal" }}\" stops, and what it was running ends with it.",
            confirm = "Close terminal", danger = true, onConfirm = { closing = null; onClose(p) },
        )
    }
}

@Composable
fun NewTerminalDialog(onPick: (TerminalKind) -> Unit, onDismiss: () -> Unit) {
    CrundiDialog("New terminal", onDismiss = onDismiss, message = "What should it run?") {
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            for (k in TerminalKind.entries) ListRow(k.label, subtitle = k.detail, icon = k.icon, onClick = { onPick(k) })
        }
    }
}

/** The Terminals tab's front page, with live data. */
@Composable
fun TerminalsRoute() {
    val services = LocalAppServices.current
    val api = services.api
    val nav = LocalNavigator.current
    val shell by services.shell.state.collectAsState()
    val project = shell.currentProject
    val server by remember(api) { api.events.state }.collectAsState(initial = api.events.lastState)
    val live by api.events.status.collectAsState()
    val scope = rememberCoroutineScope()
    var busy by remember { mutableStateOf(false) }
    var notice by remember { mutableStateOf<String?>(null) }
    val toast = rememberToast()
    val state = TerminalsState(
        project = project,
        loading = server == null,
        terminals = server?.terminalsOf(project).orEmpty(),
        parked = server?.parked(project).orEmpty().filter { it.paneKind == "terminal" },
        busy = busy,
        notice = notice,
        loadError = if (server == null) liveErrorText(live) else null,
    )
    TerminalsScreen(
        state,
        onRetry = { api.events.reconnect() },
        onOpen = { nav.to(WorkspaceRoutes.terminal(it.id)) },
        onNew = { kind ->
            if (project == null) return@TerminalsScreen
            busy = true
            scope.launch {
                runCatching {
                    api.terminals.create(project, shell = kind == TerminalKind.Shell, cols = 60, rows = 30, skipPermissions = if (kind == TerminalKind.ClaudeSkip) true else null)
                }.onSuccess { notice = null; nav.to(WorkspaceRoutes.terminal(it.id)) }.onFailure { notice = "Could not open a terminal: " + errorText(it) }
                busy = false
            }
        },
        onClose = { p -> scope.launch { runCatching { api.terminals.close(p.id) }.onFailure { toast("Could not close it: " + errorText(it)) } } },
        onResume = { p -> scope.launch { runCatching { api.terminals.resume(p.id) }.onSuccess { nav.to(WorkspaceRoutes.terminal(it)) }.onFailure { toast("Could not resume it: " + errorText(it)) } } },
    )
}

// ─── One terminal ───

/** Everything a terminal screen draws, besides the emulator itself. */
data class TerminalUi(
    val title: String,
    val subtitle: String = "",
    val dot: DotState = DotState.Running,
    val exited: Int? = null,
    val fontSize: Int = 12,
    val ctrl: Boolean = false,
    val input: String = "",
    val connected: Boolean = true,
)

@Composable
fun TerminalScreen(
    ui: TerminalUi,
    emu: TermEmulator,
    version: Long,
    onBack: () -> Unit = {},
    onSend: (String) -> Unit = {},
    onInput: (String) -> Unit = {},
    onSubmit: () -> Unit = {},
    onCtrl: () -> Unit = {},
    onFont: (Int) -> Unit = {},
    onRename: () -> Unit = {},
    onSize: (Int, Int) -> Unit = { _, _ -> },
    modifier: Modifier = Modifier,
) {
    Column(modifier.fillMaxSize().background(CrundiColors.BgPrimary).imePadding()) {
        TopBar(ui.title, subtitle = ui.subtitle, onBack = onBack) {
            StateDot(ui.dot)
            CrundiIconButton("minus", { onFont(-1) }, contentDescription = "Smaller text", size = 16.dp)
            CrundiIconButton("plus", { onFont(1) }, contentDescription = "Larger text", size = 16.dp)
            CrundiIconButton("pencil", onRename, contentDescription = "Rename terminal", size = 16.dp)
        }
        if (ui.exited != null) Banner("The process ended (code ${ui.exited}). Close this terminal or open a new one.", Modifier.fillMaxWidth().padding(8.dp), kind = BadgeKind.Warn, icon = "alert-triangle")
        else if (!ui.connected) Banner("Reconnecting to the server", Modifier.fillMaxWidth().padding(8.dp), kind = BadgeKind.Warn, icon = "refresh")
        TerminalView(emu, version, ui.fontSize, Modifier.weight(1f).fillMaxWidth(), onSize = onSize)
        TerminalKeyRow(ui.ctrl, onCtrl, emu.appCursorKeys, onSend)
        Row(
            Modifier.fillMaxWidth().background(CrundiColors.BgSecondary).padding(start = 8.dp, end = 4.dp, bottom = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            CrundiTextField(
                ui.input, onInput,
                placeholder = if (ui.ctrl) "Ctrl + the next key" else "Type here, then send",
                mono = true,
                modifier = Modifier.weight(1f),
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false, imeAction = ImeAction.Send),
                keyboardActions = KeyboardActions(onSend = { onSubmit() }),
            )
            CrundiIconButton("send", onSubmit, contentDescription = "Send", tint = CrundiColors.AccentHover, enabled = ui.exited == null)
        }
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
    val stream: TerminalStream = remember(id) { api.terminal(id) }
    val emu = remember(id) { TermEmulator(60, 30) }
    var version by remember { mutableLongStateOf(0L) }
    var exited by remember { mutableStateOf<Int?>(null) }
    var fontSize by rememberSaveable { mutableIntStateOf(12) }
    var ctrl by remember { mutableStateOf(false) }
    var input by rememberSaveable { mutableStateOf("") }
    var renaming by remember { mutableStateOf(false) }
    val pane by remember(api, id) { api.events.state.map { s -> s?.terminals?.firstOrNull { it.id == id } } }.collectAsState(initial = null)
    val status by remember(api) { api.socket.status }.collectAsState(initial = null)

    LaunchedEffect(id) {
        emu.respond = { stream.input(it) }
        stream.output.collect { chunk ->
            emu.write(chunk)
            TerminalStream.exitCode(chunk)?.let { exited = it }
            version = emu.version
        }
    }
    fun send(s: String) { if (exited == null) stream.input(s) }
    TerminalScreen(
        TerminalUi(
            title = pane?.label?.ifEmpty { null } ?: emu.title ?: "Terminal",
            subtitle = (pane?.project ?: "") + (pane?.let { " · " + stateWords(it) } ?: ""),
            dot = pane?.let(::dotOf) ?: DotState.Idle,
            exited = exited, fontSize = fontSize, ctrl = ctrl, input = input,
            connected = status != LiveStatus.Reconnecting && status != LiveStatus.Offline,
        ),
        emu, version,
        onBack = { nav.back() },
        onSend = { k -> if (ctrl && k.length == 1) { ctrl = false }; send(k) },
        onInput = { v ->
            if (ctrl && v.length > input.length) {
                // Ctrl is held: the key just typed goes now, as a control key.
                ctrl = false
                send(TermKeys.ctrl(v.last()))
            } else input = v
        },
        onSubmit = {
            val text = input
            input = ""
            if (text.isEmpty()) send(TermKeys.ENTER)
            else if (emu.bracketedPaste && text.contains('\n')) send("\u001b[200~$text\u001b[201~\r")
            else send(text + TermKeys.ENTER)
        },
        onCtrl = { ctrl = !ctrl },
        onFont = { d -> fontSize = (fontSize + d).coerceIn(8, 20) },
        onRename = { renaming = true },
        onSize = { c, r -> emu.resize(c, r); version = emu.version; stream.resize(c, r) },
    )
    if (renaming) {
        var name by remember { mutableStateOf(pane?.title.orEmpty()) }
        CrundiDialog(
            "Rename terminal", onDismiss = { renaming = false }, confirm = "Rename",
            onConfirm = {
                renaming = false
                scope.launch { runCatching { api.terminals.rename(id, name.trim()) }.onFailure { toast("Could not rename it: " + errorText(it)) } }
            },
        ) { CrundiTextField(name, { name = it }, placeholder = "Terminal", modifier = Modifier.fillMaxWidth()) }
    }
}
