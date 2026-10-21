package mv.obitech.crundi.ui.operations

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.navigation.NavGraphBuilder
import androidx.navigation.compose.composable
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.DockerLogEvent
import mv.obitech.crundi.api.DockerShellEvent
import mv.obitech.crundi.api.model.Container
import mv.obitech.crundi.api.model.ContainerDetail
import mv.obitech.crundi.api.model.DockerStats
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiChip
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiDivider
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.TopBar
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

object DockerRoutes {
    const val CONTAINER = "docker/container/{id}"
    fun container(id: String) = "docker/container/" + java.net.URLEncoder.encode(id, "UTF-8")
}

fun NavGraphBuilder.dockerRoutes() {
    composable(DockerRoutes.CONTAINER) { e -> ContainerRoute(e.arguments?.getString("id").orEmpty()) }
}

enum class ContainerTab { Overview, Logs, Stats, Shell, Inspect }

/** One log line: which stream, the time docker gave it, and its text (ANSI kept). */
data class LogLine(val stderr: Boolean, val time: String, val text: String)

/** One stats sample with the rates worked out from the one before. */
data class StatSample(val at: Long, val cpu: Double?, val mem: Long, val rx: Double, val tx: Double, val read: Double, val write: Double)

data class ShellView(val open: Boolean = false, val lines: List<String> = emptyList(), val ended: String? = null)

data class ContainerState(
    val detail: Load<ContainerDetail> = Load.Loading,
    val tab: ContainerTab = ContainerTab.Overview,
    val logs: List<LogLine> = emptyList(),
    val logStatus: String? = null,
    val follow: Boolean = true,
    val times: Boolean = false,
    val wrap: Boolean = true,
    val logFilter: String = "",
    val stats: List<StatSample> = emptyList(),
    val latest: DockerStats? = null,
    val shell: ShellView = ShellView(),
    val busy: Boolean = false,
    val note: Actions.Note? = null,
)

class ContainerEvents(
    val onBack: () -> Unit = {},
    val onTab: (ContainerTab) -> Unit = {},
    val onAct: (String) -> Unit = {},
    val onRename: (String) -> Unit = {},
    val onRemove: (volumes: Boolean, force: Boolean) -> Unit = { _, _ -> },
    val onFollow: (Boolean) -> Unit = {},
    val onTimes: (Boolean) -> Unit = {},
    val onWrap: (Boolean) -> Unit = {},
    val onLogFilter: (String) -> Unit = {},
    val onCopyLogs: () -> Unit = {},
    val onClearLogs: () -> Unit = {},
    val onShellInput: (String) -> Unit = {},
    val onShellRestart: () -> Unit = {},
    val onCopyJson: () -> Unit = {},
    val onCloseNote: () -> Unit = {},
)

private val tsRx = Regex("^(\\d{4}-\\d\\d-\\d\\dT\\d\\d:\\d\\d:\\d\\d(\\.\\d+)?Z?)\\s(.*)$")

/** Split docker's frames into lines, a leading ISO time taken off and kept apart. */
fun logLines(frames: List<mv.obitech.crundi.api.model.DockerLogFrame>): List<LogLine> = frames.flatMap { f ->
    f.text.split('\n').filter { it.isNotEmpty() }.map { raw ->
        val l = raw.trimEnd('\r')
        val m = tsRx.find(l)
        if (m != null) LogLine(f.stream == "stderr", m.groupValues[1].replace('T', ' ').take(23), m.groupValues[3]) else LogLine(f.stream == "stderr", "", l)
    }
}

private val pretty = Json { prettyPrint = true; prettyPrintIndent = "  " }

@Composable
fun ContainerRoute(id: String) {
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val clip = LocalClipboardManager.current
    val actions = rememberActions()
    val scope = rememberCoroutineScope()
    var tab by rememberSaveable { mutableStateOf(ContainerTab.Overview) }
    val detail = rememberLoad(id) { api.docker.container(id) }
    LaunchedEffect(id) { while (true) { delay(4000); detail.reload() } }
    val c = detail.value.valueOrNull?.container
    // Logs: live over the socket while the Logs tab is open and Follow is on.
    var logs by remember { mutableStateOf(listOf<LogLine>()) }
    var logStatus by remember { mutableStateOf<String?>(null) }
    var follow by remember { mutableStateOf(true) }
    var times by remember { mutableStateOf(false) }
    var wrap by remember { mutableStateOf(true) }
    var filter by remember { mutableStateOf("") }
    var restartTick by remember { mutableIntStateOf(0) }
    LaunchedEffect(tab == ContainerTab.Logs, follow, restartTick, c?.running) {
        if (tab != ContainerTab.Logs || !follow) return@LaunchedEffect
        logs = emptyList(); logStatus = null
        api.dockerLogs(id, 500).collect { e ->
            when (e) {
                is DockerLogEvent.Lines -> logs = (logs + logLines(e.frames)).takeLast(5000)
                DockerLogEvent.Ended -> logStatus = if (c?.running == false) "The container stopped. Start it to follow the log again." else "The log stream ended."
                is DockerLogEvent.Failed -> logStatus = e.error.ifEmpty { "Could not follow the log" }
            }
        }
    }
    // Stats: a sample every 2 s, the last 120.
    var samples by remember { mutableStateOf(listOf<StatSample>()) }
    var latest by remember { mutableStateOf<DockerStats?>(null) }
    LaunchedEffect(tab == ContainerTab.Stats, c?.running) {
        if (tab != ContainerTab.Stats || c?.running != true) return@LaunchedEffect
        var prev: DockerStats? = null
        while (true) {
            val s = try { api.docker.stats(id) } catch (_: ApiException) { null }
            if (s != null) {
                latest = s
                val p = prev
                val dt = if (p != null) (s.at - p.at).coerceAtLeast(1) / 1000.0 else 0.0
                fun rate(a: Long, b: Long?) = if (b == null || dt == 0.0) 0.0 else ((a - b).coerceAtLeast(0)) / dt
                samples = (samples + StatSample(s.at, s.cpuPct, s.memory, rate(s.netRx, p?.netRx), rate(s.netTx, p?.netTx), rate(s.blkRead, p?.blkRead), rate(s.blkWrite, p?.blkWrite))).takeLast(120)
                prev = s
            }
            delay(2000)
        }
    }
    // Shell: opens when the tab is first shown, stays open when the tab changes.
    var shell by remember { mutableStateOf(ShellView()) }
    var shellTick by remember { mutableIntStateOf(0) }
    var shellObj by remember { mutableStateOf<mv.obitech.crundi.api.DockerShell?>(null) }
    var wantShell by remember { mutableStateOf(false) }
    LaunchedEffect(tab, c?.running) { if (tab == ContainerTab.Shell && c?.running == true) wantShell = true }
    LaunchedEffect(wantShell, shellTick) {
        if (!wantShell) return@LaunchedEffect
        val sh = api.dockerShell(id, 80, 24)
        shellObj = sh
        shell = ShellView(open = true)
        sh.events.collect { e ->
            shell = when (e) {
                is DockerShellEvent.Data -> shell.copy(lines = Ansi.fold(shell.lines, e.text))
                is DockerShellEvent.Exited -> shell.copy(open = false, ended = if (e.code == 126 || e.code == 127) "This image may not include a shell (exit ${e.code})." else "The shell ended (exit ${e.code}).")
                is DockerShellEvent.Failed -> shell.copy(open = false, ended = e.error.ifEmpty { "Could not open a shell" })
            }
        }
    }
    val verbs = mapOf("start" to "Started", "stop" to "Stopped", "restart" to "Restarted", "pause" to "Paused", "unpause" to "Resumed", "kill" to "Killed")
    ContainerScreen(
        ContainerState(detail.value, tab, logs, logStatus, follow, times, wrap, filter, samples, latest, shell, actions.busy == id, actions.note),
        ContainerEvents(
            onBack = { nav.back() },
            onTab = { tab = it },
            onAct = { verb -> actions.run(id, "${verbs[verb] ?: verb} ${c?.name.orEmpty()}", after = { detail.reload(); if (verb != "stop" && verb != "kill" && verb != "pause") restartTick++ }) { api.docker.act(id, verb) } },
            onRename = { n -> actions.run(id, "Renamed to $n", detail::reload) { api.docker.rename(id, n) } },
            onRemove = { v, f -> actions.run(id, "Removed ${c?.name.orEmpty()}", after = { nav.back() }) { api.docker.remove(id, force = f, volumes = v) } },
            onFollow = { follow = it },
            onTimes = { times = it },
            onWrap = { wrap = it },
            onLogFilter = { filter = it },
            onCopyLogs = {
                clip.setText(AnnotatedString(logs.joinToString("\n") { (if (times && it.time.isNotEmpty()) it.time + " " else "") + Ansi.strip(it.text) }))
                actions.note = Actions.Note("Copied", false)
            },
            onClearLogs = { logs = emptyList() },
            onShellInput = { s -> scope.launch { shellObj?.input(s) } },
            onShellRestart = { shell = ShellView(); wantShell = true; shellTick++ },
            onCopyJson = {
                detail.value.valueOrNull?.inspect?.let { clip.setText(AnnotatedString(pretty.encodeToString(JsonElement.serializer(), it))); actions.note = Actions.Note("Copied", false) }
            },
            onCloseNote = actions::clear,
        ),
    )
}

@Composable
fun ContainerScreen(state: ContainerState, events: ContainerEvents = ContainerEvents()) {
    val c = state.detail.valueOrNull?.container
    var rename by remember { mutableStateOf(false) }
    var remove by remember { mutableStateOf(false) }
    var confirm by remember { mutableStateOf<String?>(null) }
    Column(Modifier.fillMaxSize()) {
        TopBar(
            c?.name ?: "Container",
            subtitle = c?.let { listOfNotNull(it.image, it.status, it.project.ifEmpty { null }).joinToString(" · ") },
            onBack = events.onBack,
            actions = {
                if (c != null) {
                    CrundiIconButton("pencil", { rename = true }, contentDescription = "Rename")
                    CrundiIconButton("trash", { remove = true }, contentDescription = "Remove", tint = CrundiColors.Red)
                }
            },
        )
        when (val d = state.detail) {
            Load.Loading -> LoadingState(text = "Loading…")
            is Load.Failed -> ErrorState(d.message)
            is Load.Ready -> {
                val ct = d.value.container
                Column(Modifier.fillMaxWidth().background(CrundiColors.BgSecondary)) {
                    Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = Dimens.ScreenPad, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                        ContainerDot(ct)
                        when (ct.state) {
                            "running" -> {
                                CrundiButton("Stop", { confirm = "stop" }, kind = ButtonKind.Danger, icon = "stop", enabled = !state.busy)
                                CrundiButton("Restart", { events.onAct("restart") }, icon = "rotate-ccw", enabled = !state.busy)
                                CrundiButton("Pause", { events.onAct("pause") }, icon = "pause", enabled = !state.busy)
                                CrundiButton("Kill", { confirm = "kill" }, kind = ButtonKind.Danger, icon = "zap", enabled = !state.busy)
                            }
                            "paused" -> {
                                CrundiButton("Resume", { events.onAct("unpause") }, kind = ButtonKind.Primary, icon = "play", enabled = !state.busy)
                                CrundiButton("Stop", { confirm = "stop" }, kind = ButtonKind.Danger, icon = "stop", enabled = !state.busy)
                            }
                            else -> CrundiButton("Start", { events.onAct("start") }, kind = ButtonKind.Primary, icon = "play", enabled = !state.busy)
                        }
                    }
                    Segments(ContainerTab.entries.map { it.name to it.name }, state.tab.name, { events.onTab(ContainerTab.valueOf(it)) }, Modifier.fillMaxWidth().padding(horizontal = Dimens.ScreenPad).padding(bottom = 8.dp))
                    CrundiDivider(strong = true)
                }
                NoteBanner(state.note, events.onCloseNote)
                Box(Modifier.weight(1f)) {
                    when (state.tab) {
                        ContainerTab.Overview -> OverviewTab(d.value)
                        ContainerTab.Logs -> LogsTab(state, events)
                        ContainerTab.Stats -> StatsTab(ct, state)
                        ContainerTab.Shell -> ShellTab(ct, state.shell, events)
                        ContainerTab.Inspect -> InspectTab(d.value, events)
                    }
                }
            }
        }
    }
    if (c != null) {
        confirm?.let { v ->
            CrundiDialog(if (v == "kill") "Kill it now?" else "Stop ${c.name}?", { confirm = null },
                message = if (v == "kill") "Docker sends SIGKILL: the processes inside get no chance to finish what they are doing." else null,
                confirm = if (v == "kill") "Kill" else "Stop", danger = true, onConfirm = { confirm = null; events.onAct(v) })
        }
        if (rename) {
            var name by remember { mutableStateOf(c.name) }
            CrundiDialog("Rename ${c.name}", { rename = false }, confirm = "Rename", onConfirm = { rename = false; if (name.isNotBlank() && name != c.name) events.onRename(name.trim()) }) {
                CrundiTextField(name, { name = it }, placeholder = "New name", mono = true)
            }
        }
        if (remove) {
            var volumes by remember { mutableStateOf(false) }
            var force by remember { mutableStateOf(true) }
            CrundiDialog("Remove ${c.name}?", { remove = false }, message = "This cannot be undone.", confirm = "Remove", danger = true, onConfirm = { remove = false; events.onRemove(volumes, force) }) {
                CheckRow("Also its anonymous volumes", volumes) { volumes = it }
                if (c.running || c.state == "paused") CheckRow("Stop it first", force) { force = it }
            }
        }
    }
}

// ─── Overview ───

private fun jsonText(e: JsonElement?): String = when (e) {
    null -> ""
    is JsonArray -> e.joinToString(" ") { (it as? JsonPrimitive)?.content ?: it.toString() }
    is JsonPrimitive -> e.content
    else -> e.toString()
}

private fun whenText(iso: String): String {
    val ms = Fmt.iso(iso)
    return if (ms <= 0 || iso.startsWith("0001")) "" else "${Fmt.date(ms)} (${DockerFmt.ago(ms)})"
}

@Composable
private fun OverviewTab(d: ContainerDetail) {
    val o = d.overview
    val c = d.container
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(vertical = 8.dp)) {
        Group("State") {
            KeyValue("Status", o.state + if (o.health.isNotEmpty()) " (${o.health})" else "", valueColor = DockerFmt.stateColor(DockerFmt.stateKey(c)).first)
            if (o.state == "running" || o.state == "paused") whenText(o.startedAt).takeIf { it.isNotEmpty() }?.let { KeyValue("Started", it) }
            else whenText(o.finishedAt).takeIf { it.isNotEmpty() }?.let { KeyValue("Finished", it) }
            if (o.state == "exited") KeyValue("Exit code", "${o.exitCode}" + if (o.oomKilled) " (out of memory)" else "")
            if (o.error.isNotEmpty()) KeyValue("Error", o.error, valueColor = CrundiColors.Red)
            KeyValue("Restarts", "${o.restartCount}")
            KeyValue("Restart policy", o.restartPolicy.ifEmpty { "no" })
            whenText(o.created).takeIf { it.isNotEmpty() }?.let { KeyValue("Created", it) }
        }
        Group("Runs") {
            KeyValue("Image", o.image.ifEmpty { c.image })
            KeyValue("Image id", o.imageId.removePrefix("sha256:").take(12))
            jsonText(o.entrypoint).takeIf { it.isNotEmpty() }?.let { KeyValue("Entrypoint", it) }
            jsonText(o.cmd).takeIf { it.isNotEmpty() }?.let { KeyValue("Command", it) }
            if (o.workingDir.isNotEmpty()) KeyValue("Working dir", o.workingDir)
            if (o.user.isNotEmpty()) KeyValue("User", o.user)
            if (o.hostname.isNotEmpty()) KeyValue("Hostname", o.hostname)
            if (o.project.isNotEmpty()) KeyValue("Compose", "${o.project} / ${o.service}")
            KeyValue("Id", c.fullId.ifEmpty { c.id }.take(24))
        }
        Group("Ports") {
            if (o.ports.isEmpty()) Text("No ports.", style = CrundiType.Small)
            for (p in o.ports) KeyValue("${p.private}/${p.type}", if (p.public > 0) "${p.ip.ifEmpty { "0.0.0.0" }}:${p.public}" else "not published")
        }
        Group("Networks") {
            if (o.networks.isEmpty()) Text("Not on any network.", style = CrundiType.Small)
            for (n in o.networks) KeyValue(n.name, n.ip.ifEmpty { "no address" } + if (n.gateway.isNotEmpty()) "  via ${n.gateway}" else "")
        }
        Group("Mounts") {
            if (o.mounts.isEmpty()) Text("No mounts.", style = CrundiType.Small)
            for (m in o.mounts) KeyValue(
                m.destination,
                (if (m.type == "volume") "volume " + (if (m.name.length > 40) m.name.take(12) + "…" else m.name) else m.source) + if (!m.rw) "  (read-only)" else "",
            )
        }
        Group("Environment") {
            if (o.env.isEmpty()) Text("None.", style = CrundiType.Small)
            for (e in o.env) Row(Modifier.fillMaxWidth().padding(vertical = 3.dp)) {
                Text(e.key, style = CrundiType.Small, modifier = Modifier.weight(0.38f), maxLines = 2, overflow = TextOverflow.Ellipsis)
                if (e.hidden) Text("hidden", style = CrundiType.Small.copy(fontStyle = FontStyle.Italic, color = CrundiColors.TextMuted), modifier = Modifier.weight(0.62f))
                else Text(e.value, style = CrundiType.Code.copy(fontSize = 13.sp), modifier = Modifier.weight(0.62f), maxLines = 3, overflow = TextOverflow.Ellipsis)
            }
            if (o.env.any { it.hidden }) Text("Values that look like secrets are hidden here. Inspect shows everything.", style = CrundiType.Meta)
        }
        if (o.labels.isNotEmpty()) {
            var open by remember { mutableStateOf(false) }
            Group("Labels (${o.labels.size})") {
                CrundiButton(if (open) "Hide labels" else "Show labels", { open = !open }, kind = ButtonKind.Ghost, icon = if (open) "chevron-up" else "chevron-down")
                if (open) for ((k, v) in o.labels.toSortedMap()) KeyValue(k, v)
            }
        }
    }
}

// ─── Logs ───

@Composable
private fun LogsTab(state: ContainerState, events: ContainerEvents) {
    Column(Modifier.fillMaxSize()) {
        Row(Modifier.fillMaxWidth().padding(start = Dimens.ScreenPad, end = 4.dp, top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            CrundiTextField(state.logFilter, events.onLogFilter, Modifier.weight(1f), placeholder = "Filter lines", icon = "search")
            CrundiIconButton("copy", events.onCopyLogs, contentDescription = "Copy")
            CrundiIconButton("x", events.onClearLogs, contentDescription = "Clear")
        }
        Row(Modifier.fillMaxWidth().padding(horizontal = Dimens.ScreenPad, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiChip("Follow", state.follow, { events.onFollow(!state.follow) }, icon = if (state.follow) "check" else null)
            CrundiChip("Times", state.times, { events.onTimes(!state.times) }, icon = if (state.times) "check" else null)
            CrundiChip("Wrap", state.wrap, { events.onWrap(!state.wrap) }, icon = if (state.wrap) "check" else null)
        }
        val shown = if (state.logFilter.isBlank()) state.logs else state.logs.filter { Ansi.strip(it.text).contains(state.logFilter, ignoreCase = true) }
        val list = rememberLazyListState()
        LaunchedEffect(shown.size, state.follow) { if (state.follow && shown.isNotEmpty()) list.scrollToItem(shown.size - 1) }
        val h = rememberScrollState()
        Box(Modifier.weight(1f).fillMaxWidth().background(CrundiColors.BgPrimary).then(if (state.wrap) Modifier else Modifier.horizontalScroll(h))) {
            SelectionContainer {
                LazyColumn(Modifier.fillMaxSize(), state = list, contentPadding = PaddingValues(horizontal = 12.dp, vertical = 8.dp)) {
                    if (shown.isEmpty() && state.logStatus == null) item { Text(if (state.follow) "Waiting for log lines…" else "Follow is off.", style = CrundiType.Meta.copy(fontStyle = FontStyle.Italic)) }
                    items(shown) { l ->
                        val base = if (l.stderr) Color(0xFFFCA5A5) else CrundiColors.TextSecondary
                        Text(
                            buildAnnotated(if (state.times) l.time else "", Ansi.render(l.text, base)),
                            style = CrundiType.Code.copy(fontSize = 11.5.sp, lineHeight = 16.sp), softWrap = state.wrap,
                        )
                    }
                    state.logStatus?.let { s -> item { Text("\u2014 $s", style = CrundiType.Meta.copy(fontStyle = FontStyle.Italic)) } }
                }
            }
        }
    }
}

private fun buildAnnotated(time: String, text: AnnotatedString): AnnotatedString = if (time.isEmpty()) text else androidx.compose.ui.text.buildAnnotatedString {
    pushStyle(androidx.compose.ui.text.SpanStyle(color = CrundiColors.TextMuted)); append(time); append("  "); pop()
    append(text)
}

// ─── Stats ───

@Composable
private fun StatsTab(c: Container, state: ContainerState) {
    if (!c.running) { Text("The container is not running, so there is nothing to measure.", style = CrundiType.Small, modifier = Modifier.padding(Dimens.ScreenPad)); return }
    val s = state.stats
    val last = s.lastOrNull()
    val st = state.latest
    if (last == null || st == null) { LoadingState(text = "Measuring…"); return }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(Dimens.ScreenPad), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        ChartCard("CPU", Fmt.pct(last.cpu), "of one core · ${st.cpus} available", listOf(s.map { (it.cpu ?: 0.0).toFloat() } to CrundiColors.Accent), max = 100f)
        ChartCard("Memory", Fmt.bytes(last.mem), "of ${Fmt.bytes(st.memoryLimit)} (${Fmt.pct(if (st.memoryLimit > 0) st.memory * 100.0 / st.memoryLimit else null)})",
            listOf(s.map { it.mem.toFloat() } to CrundiColors.Green), max = null)
        ChartCard("Network", Fmt.rate(last.rx), "in", listOf(s.map { it.rx.toFloat() } to Color(0xFF22D3EE), s.map { it.tx.toFloat() } to Color(0xFFC084FC)), max = null,
            legend = "in ${Fmt.rate(last.rx)} · ${Fmt.bytes(st.netRx)} total    out ${Fmt.rate(last.tx)} · ${Fmt.bytes(st.netTx)} total")
        ChartCard("Disk", Fmt.rate(last.read), "read", listOf(s.map { it.read.toFloat() } to Color(0xFFFBBF24), s.map { it.write.toFloat() } to Color(0xFFF87171)), max = null,
            legend = "read ${Fmt.bytes(st.blkRead)} total    write ${Fmt.bytes(st.blkWrite)} total")
        Text("${st.pids} processes · a sample every 2 seconds, the last ${maxOf(1, s.size * 2 / 60)} minutes", style = CrundiType.Meta)
    }
}

@Composable
private fun ChartCard(title: String, value: String, sub: String, series: List<Pair<List<Float>, Color>>, max: Float?, legend: String? = null) {
    Column(
        Modifier.fillMaxWidth().clip(Dimens.Shape).background(CrundiColors.BgCard).border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape).padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(verticalAlignment = Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(title.uppercase(), style = CrundiType.Section, modifier = Modifier.weight(1f))
            Text(value, style = CrundiType.Code.copy(fontSize = 16.sp, fontWeight = FontWeight.SemiBold))
            Text(sub, style = CrundiType.Meta)
        }
        AreaChart(series, max, Modifier.fillMaxWidth().height(70.dp))
        if (legend != null) Text(legend, style = CrundiType.Meta)
    }
}

@Composable
fun AreaChart(series: List<Pair<List<Float>, Color>>, max: Float?, modifier: Modifier = Modifier) {
    val top = max ?: ((series.flatMap { it.first }.maxOrNull() ?: 0f) * 1.25f).coerceAtLeast(1f)
    Canvas(modifier) {
        drawLine(CrundiColors.BorderSubtle, Offset(0f, size.height), Offset(size.width, size.height), 1f)
        for ((values, color) in series) {
            if (values.size < 2) continue
            val step = size.width / (values.size - 1)
            val line = Path()
            values.forEachIndexed { i, v -> val y = size.height - (v / top).coerceIn(0f, 1f) * size.height; if (i == 0) line.moveTo(0f, y) else line.lineTo(i * step, y) }
            val fill = Path().apply { addPath(line); lineTo((values.size - 1) * step, size.height); lineTo(0f, size.height); close() }
            drawPath(fill, color.copy(alpha = 0.15f))
            drawPath(line, color, style = Stroke(1.5.dp.toPx()))
        }
    }
}

// ─── Shell ───

@Composable
private fun ShellTab(c: Container, shell: ShellView, events: ContainerEvents) {
    if (!c.running) { Text("Start the container to open a shell in it.", style = CrundiType.Small, modifier = Modifier.padding(Dimens.ScreenPad)); return }
    var line by remember { mutableStateOf("") }
    Column(Modifier.fillMaxSize().imePadding()) {
        Row(Modifier.fillMaxWidth().padding(start = Dimens.ScreenPad, end = 8.dp, top = 6.dp, bottom = 6.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("A shell inside ${c.name} (bash when the image has it, else sh)", style = CrundiType.Small, modifier = Modifier.weight(1f))
            CrundiButton("New shell", events.onShellRestart, kind = ButtonKind.Ghost, icon = "refresh")
        }
        val list = rememberLazyListState()
        LaunchedEffect(shell.lines.size) { if (shell.lines.isNotEmpty()) list.scrollToItem(shell.lines.size - 1) }
        LazyColumn(Modifier.weight(1f).fillMaxWidth().background(CrundiColors.BgPrimary), state = list, contentPadding = PaddingValues(10.dp)) {
            items(shell.lines) { l -> Text(Ansi.render(l, CrundiColors.TextPrimary), style = CrundiType.Code.copy(fontSize = 12.sp, lineHeight = 16.sp)) }
            shell.ended?.let { e -> item { Text(e, style = CrundiType.Meta.copy(fontStyle = FontStyle.Italic, color = CrundiColors.Yellow), modifier = Modifier.padding(top = 6.dp)) } }
            if (!shell.open && shell.ended == null && shell.lines.isEmpty()) item { Text("Opening a shell…", style = CrundiType.Meta) }
        }
        if (shell.ended != null) Row(Modifier.padding(8.dp)) { CrundiButton("Open again", events.onShellRestart, kind = ButtonKind.Primary, icon = "terminal") }
        // The keys a phone keyboard does not have.
        Row(Modifier.fillMaxWidth().background(CrundiColors.BgSecondary).horizontalScroll(rememberScrollState()).padding(horizontal = 8.dp, vertical = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            for ((label, seq) in listOf("Esc" to "\u001b", "Tab" to "\t", "Ctrl-C" to "\u0003", "Ctrl-D" to "\u0004", "\u2190" to "\u001b[D", "\u2191" to "\u001b[A", "\u2193" to "\u001b[B", "\u2192" to "\u001b[C"))
                CrundiChip(label, false, { events.onShellInput(seq) })
        }
        Row(Modifier.fillMaxWidth().background(CrundiColors.BgSecondary).padding(start = 8.dp, end = 4.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            CrundiTextField(line, { line = it }, Modifier.weight(1f), placeholder = "Type a command", mono = true, enabled = shell.open,
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send), keyboardActions = KeyboardActions(onSend = { events.onShellInput(line + "\r"); line = "" }))
            CrundiIconButton("send", { events.onShellInput(line + "\r"); line = "" }, contentDescription = "Send", tint = CrundiColors.AccentHover, enabled = shell.open)
        }
    }
}

// ─── Inspect ───

@Composable
private fun InspectTab(d: ContainerDetail, events: ContainerEvents) {
    val text = remember(d.inspect) { d.inspect?.let { pretty.encodeToString(JsonObject.serializer(), it) } ?: "{}" }
    Column(Modifier.fillMaxSize()) {
        Row(Modifier.fillMaxWidth().padding(horizontal = Dimens.ScreenPad, vertical = 8.dp), horizontalArrangement = Arrangement.End) {
            CrundiButton("Copy JSON", events.onCopyJson, icon = "copy")
        }
        SelectionContainer {
            Text(
                text, style = CrundiType.Code.copy(fontSize = 11.5.sp, lineHeight = 16.sp, color = CrundiColors.TextSecondary),
                modifier = Modifier.fillMaxSize().background(CrundiColors.BgPrimary).verticalScroll(rememberScrollState()).horizontalScroll(rememberScrollState()).padding(12.dp),
                softWrap = false,
            )
        }
    }
}

