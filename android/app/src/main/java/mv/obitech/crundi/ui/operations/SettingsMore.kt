package mv.obitech.crundi.ui.operations

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay
import mv.obitech.crundi.api.model.LoginSession
import mv.obitech.crundi.api.model.MaintenanceTask
import mv.obitech.crundi.api.model.ServerLog
import mv.obitech.crundi.api.model.Stats
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiDivider
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.StatusBadge
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens
import java.util.Locale

// ─── This machine ───

data class MachineState(
    val stats: Load<Stats> = Load.Loading,
    val tasks: Load<List<MaintenanceTask>> = Load.Loading,
    val busy: String? = null,
    val note: Actions.Note? = null,
)

@Composable
fun MachineRoute() {
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val actions = rememberActions()
    val stats = rememberLoad { api.system.stats() }
    val tasks = rememberLoad { api.system.maintenance() }
    LaunchedEffect(Unit) { while (true) { delay(2000); stats.reload() } }
    MachineScreen(
        MachineState(stats.value, tasks.value, actions.busy, actions.note),
        onRun = { t -> actions.run(t.id, after = tasks::reload) { val freed = api.system.runMaintenance(t.id); actions.note = Actions.Note(if (freed > 0) "Freed ${Fmt.bytes(freed)}" else "Done.", false) } },
        onCloseNote = actions::clear,
        onBack = { nav.back() },
    )
}

private fun loadColor(p: Double?): androidx.compose.ui.graphics.Color = when {
    p == null -> CrundiColors.Green
    p >= 85 -> CrundiColors.Red
    p >= 60 -> CrundiColors.Yellow
    else -> CrundiColors.Green
}

@Composable
fun MachineScreen(state: MachineState, onRun: (MaintenanceTask) -> Unit = {}, onCloseNote: () -> Unit = {}, onBack: () -> Unit = {}) {
    SettingsPageFrame(SettingsPage.Machine, state.note, onCloseNote, onBack) {
        ScrollBody {
            Group("This machine") {
                when (val l = state.stats) {
                    Load.Loading -> Text("Reading the machine…", style = CrundiType.Small)
                    is Load.Failed -> Text(l.message, style = CrundiType.Small.copy(color = CrundiColors.Red))
                    is Load.Ready -> {
                        val s = l.value.system
                        if (s == null) Text("The machine's numbers are for the owner.", style = CrundiType.Small) else {
                            val cpu = s.cpu.overall
                            KeyValue("CPU", Fmt.pct(cpu), valueColor = loadColor(cpu))
                            CoreBars(s.cpu.cores)
                            Sparkline(s.history.cpu.map { it?.toFloat() }, loadColor(cpu), 100f, Modifier.fillMaxWidth().height(28.dp))
                            CrundiDivider()
                            val memPct = if (s.mem.total > 0) s.mem.used * 100.0 / s.mem.total else null
                            KeyValue("Memory", "${Fmt.bytes(s.mem.used)} of ${Fmt.bytes(s.mem.total)}")
                            Meter(((memPct ?: 0.0) / 100).toFloat(), color = loadColor(memPct))
                            if (s.mem.swapTotal > 0) {
                                KeyValue("Swap", "${Fmt.bytes(s.mem.swapUsed)} of ${Fmt.bytes(s.mem.swapTotal)}")
                                Meter((s.mem.swapUsed.toFloat() / s.mem.swapTotal))
                            }
                            val diskPct = if (s.disk.total > 0) s.disk.used * 100.0 / s.disk.total else null
                            KeyValue("Disk", "${Fmt.bytes(s.disk.used)} of ${Fmt.bytes(s.disk.total)}")
                            Meter(((diskPct ?: 0.0) / 100).toFloat(), color = loadColor(diskPct))
                            CrundiDivider()
                            s.load?.let { KeyValue("Load", String.format(Locale.US, "%.2f  %.2f  %.2f", it.one, it.five, it.fifteen)) }
                            KeyValue("Traffic", "down ${Fmt.rate(s.net.rxPerSec)}   up ${Fmt.rate(s.net.txPerSec)}")
                            KeyValue("Uptime", Fmt.span((s.uptimeSec * 1000).toLong()))
                            KeyValue("Crundi", "${Fmt.span((s.processUptimeSec * 1000).toLong())} · ${Fmt.bytes(s.processRssBytes)}")
                            KeyValue("Machine", s.host.hostname)
                            KeyValue("Processor", "${s.host.cores} cores · ${s.host.cpuModel}")
                            KeyValue("System", "${s.host.platform} ${s.host.release} · ${s.host.arch}")
                        }
                    }
                }
            }
            Group("Reclaim disk space") {
                when (val l = state.tasks) {
                    Load.Loading -> Text("Measuring…", style = CrundiType.Small)
                    is Load.Failed -> Text("Could not read reclaimable space.", style = CrundiType.Small.copy(color = CrundiColors.Red))
                    is Load.Ready -> {
                        val tasks = l.value.filter { it.available }
                        if (tasks.isEmpty()) Text("Nothing on this machine to reclaim.", style = CrundiType.Small)
                        else {
                            val total = tasks.sumOf { it.bytes }
                            Text(if (total > 0) "${Fmt.bytes(total)} can be reclaimed" else "Nothing to reclaim right now", style = CrundiType.Heading)
                            for (t in tasks) { CrundiDivider(); TaskRow(t, state.busy == t.id, onRun) }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun CoreBars(cores: List<Double?>) {
    if (cores.isEmpty()) return
    Row(Modifier.fillMaxWidth().height(26.dp), horizontalArrangement = Arrangement.spacedBy(3.dp), verticalAlignment = Alignment.Bottom) {
        for (c in cores) Box(Modifier.weight(1f).fillMaxHeight().clip(Dimens.ShapeSm).background(CrundiColors.BgTertiary), contentAlignment = Alignment.BottomCenter) {
            Box(Modifier.fillMaxWidth().fillMaxHeight(((c ?: 0.0) / 100).toFloat().coerceIn(0.02f, 1f)).background(loadColor(c)))
        }
    }
}

@Composable
private fun TaskRow(t: MaintenanceTask, busy: Boolean, onRun: (MaintenanceTask) -> Unit) {
    var confirm by remember { mutableStateOf(false) }
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(t.label, style = CrundiType.Body)
                if (t.elevated) StatusBadge("root", BadgeKind.Warn)
                if (t.danger) StatusBadge("destructive", BadgeKind.Bad)
            }
            if (t.description.isNotEmpty()) Text(t.description, style = CrundiType.Small)
            Text(if (t.bytes > 0) Fmt.bytes(t.bytes) else "—", style = CrundiType.Meta)
        }
        CrundiButton(if (busy) (if (t.danger) "Removing…" else "Clearing…") else if (t.danger) "Remove" else "Clear", { confirm = true },
            kind = if (t.danger) ButtonKind.Danger else ButtonKind.Secondary, enabled = !busy)
    }
    if (confirm) CrundiDialog(if (t.danger) "Remove it?" else "Clear it?", { confirm = false }, message = t.description.ifEmpty { null },
        confirm = if (t.danger) "Remove" else "Clear", danger = t.danger, onConfirm = { confirm = false; onRun(t) })
}

// ─── Server logs ───

data class LogsState(val lines: Load<List<ServerLog>> = Load.Loading, val note: Actions.Note? = null)

@Composable
fun LogsRoute() {
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val actions = rememberActions()
    val clip = LocalClipboardManager.current
    val backlog = rememberLoad { api.system.logs() }
    val live = remember { mutableStateListOf<ServerLog>() }
    LaunchedEffect(Unit) { api.serverLogs().collect { live += it; if (live.size > 2000) live.removeRange(0, live.size - 2000) } }
    val lines: Load<List<ServerLog>> = when (val b = backlog.value) { is Load.Ready -> Load.Ready(b.value + live); else -> b }
    LogsScreen(
        LogsState(lines, actions.note),
        onCopy = { lines.valueOrNull?.let { l -> clip.setText(AnnotatedString(l.joinToString("\n") { logLine(it) })); actions.note = Actions.Note("Logs copied", false) } },
        onCloseNote = actions::clear,
        onBack = { nav.back() },
    )
}

private val timeFmt = java.time.format.DateTimeFormatter.ofPattern("HH:mm:ss")
private fun logLine(l: ServerLog) = "[" + timeFmt.format(java.time.Instant.ofEpochMilli(l.time).atZone(java.time.ZoneId.systemDefault())) + "] " + l.text

@Composable
fun LogsScreen(state: LogsState, onCopy: () -> Unit = {}, onCloseNote: () -> Unit = {}, onBack: () -> Unit = {}) {
    SettingsPageFrame(SettingsPage.Logs, state.note, onCloseNote, onBack, actions = { CrundiIconButton("copy", onCopy, contentDescription = "Copy") }) {
        when (val l = state.lines) {
            Load.Loading -> LoadingState(text = "(loading...)")
            is Load.Failed -> ErrorState(l.message)
            is Load.Ready -> if (l.value.isEmpty()) EmptyState("file-text", "No log lines yet", body = "Lines appear here as the server writes them.") else {
                val list = rememberLazyListState()
                LaunchedEffect(l.value.size) { list.scrollToItem(maxOf(0, l.value.size - 1)) }
                LazyColumn(Modifier.fillMaxSize().background(CrundiColors.BgPrimary), state = list, contentPadding = PaddingValues(horizontal = 12.dp, vertical = 8.dp)) {
                    items(l.value) { line ->
                        Text(
                            logLine(line),
                            style = CrundiType.Code.copy(fontSize = 11.5.sp, lineHeight = 16.sp, color = when (line.level) {
                                "error" -> CrundiColors.Red
                                "warn" -> CrundiColors.Yellow
                                else -> CrundiColors.TextSecondary
                            }),
                        )
                    }
                }
            }
        }
    }
}

// ─── Signed-in devices ───

data class SessionsState(val sessions: Load<List<LoginSession>> = Load.Loading, val busy: String? = null, val note: Actions.Note? = null, val now: Long = System.currentTimeMillis())

@Composable
fun SessionsRoute() {
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val actions = rememberActions()
    val list = rememberLoad { api.system.sessions() }
    SessionsScreen(
        SessionsState(list.value, actions.busy, actions.note),
        onRevoke = { s -> actions.run(s.id, "Session revoked", list::reload) { api.system.revokeSession(s.id) } },
        onRevokeOthers = {
            actions.run("others", after = list::reload) {
                val n = api.system.revokeOtherSessions()
                actions.note = Actions.Note(if (n > 0) "Signed out $n other session${if (n == 1) "" else "s"}" else "Nothing else was signed in", false)
            }
        },
        onCloseNote = actions::clear,
        onBack = { nav.back() },
    )
}

/** "Pixel 7 (Crundi app)", "Chrome on Windows": what a sign-in is, in words. */
fun deviceName(s: LoginSession): String {
    s.device?.name?.takeIf { it.isNotEmpty() }?.let { return "$it (Crundi app)" }
    val ua = s.userAgent
    val browser = when {
        "Crundi" in ua -> "Crundi app"
        "Edg/" in ua -> "Edge"
        "Firefox" in ua -> "Firefox"
        "Chrome" in ua -> "Chrome"
        "Safari" in ua -> "Safari"
        else -> "A browser"
    }
    val os = when {
        "Android" in ua -> "Android"
        "iPhone" in ua || "iPad" in ua -> "iOS"
        "Windows" in ua -> "Windows"
        "Mac OS" in ua -> "macOS"
        "Linux" in ua -> "Linux"
        else -> null
    }
    return if (os != null) "$browser on $os" else browser
}

@Composable
fun SessionsScreen(state: SessionsState, onRevoke: (LoginSession) -> Unit = {}, onRevokeOthers: () -> Unit = {}, onCloseNote: () -> Unit = {}, onBack: () -> Unit = {}) {
    var confirmOthers by remember { mutableStateOf(false) }
    var revoke by remember { mutableStateOf<LoginSession?>(null) }
    SettingsPageFrame(SettingsPage.Sessions, state.note, onCloseNote, onBack) {
        when (val l = state.sessions) {
            Load.Loading -> LoadingState(text = "Loading…")
            is Load.Failed -> ErrorState(l.message)
            is Load.Ready -> if (l.value.isEmpty()) EmptyState("smartphone", "No active sessions.") else ScrollBody {
                Group("Signed-in devices") {
                    for ((i, s) in l.value.withIndex()) {
                        if (i > 0) CrundiDivider()
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            IconDisc(if (s.device != null || "Android" in s.userAgent) "smartphone" else "globe", CrundiColors.TextSecondary, CrundiColors.BgTertiary)
                            Column(Modifier.weight(1f)) {
                                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                    Text(deviceName(s), style = CrundiType.Body)
                                    if (s.current) StatusBadge("this device", BadgeKind.Accent)
                                }
                                Text("${s.ip} · signed in ${Fmt.ago(s.createdAt, state.now)} · last used ${Fmt.ago(s.lastSeenAt, state.now)}", style = CrundiType.Meta)
                            }
                            if (!s.current) CrundiButton(if (state.busy == s.id) "Revoking…" else "Revoke", { revoke = s }, kind = ButtonKind.Danger, enabled = state.busy != s.id)
                        }
                    }
                }
                val others = l.value.count { !it.current }
                if (others > 0) Row(Modifier.fillMaxWidth().padding(horizontal = Dimens.ScreenPad, vertical = 8.dp), horizontalArrangement = Arrangement.End) {
                    CrundiButton(if (others == 1) "Sign out the other device" else "Sign out the other $others devices", { confirmOthers = true }, kind = ButtonKind.Danger, icon = "x", enabled = state.busy != "others")
                }
            }
        }
    }
    revoke?.let { s -> CrundiDialog("Revoke this sign-in?", { revoke = null }, message = "${deviceName(s)} is signed out and its tokens stop working.", confirm = "Revoke", danger = true, onConfirm = { revoke = null; onRevoke(s) }) }
    if (confirmOthers) CrundiDialog("Sign them all out?", { confirmOthers = false }, message = "Every other device signed in to this server is signed out. This phone stays signed in.",
        confirm = "Sign them out", danger = true, onConfirm = { confirmOthers = false; onRevokeOthers() })
}

