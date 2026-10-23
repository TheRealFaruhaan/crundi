package mv.obitech.crundi.ui.operations

import mv.obitech.crundi.ui.components.errorText

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CheckboxDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.delay
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.model.Forward
import mv.obitech.crundi.api.model.NewForward
import mv.obitech.crundi.api.model.NewService
import mv.obitech.crundi.api.model.Service
import mv.obitech.crundi.api.model.ServiceStats
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiChip
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.PanelHeader
import mv.obitech.crundi.ui.components.StatusBadge
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.shell.LocalHost
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/** How a service is reached from outside: the desktop's four-way switch. */
enum class Exposure(val label: String) { Off("Off"), Tunnel("Tunnel"), Subdomain("Subdomain"), Path("Path") }

/** Everything the Services screen draws. */
data class ServicesState(
    val services: Load<List<Service>> = Load.Loading,
    /** Live cpu/memory with history, by service key (GET /api/stats every 2 s). */
    val stats: Map<String, ServiceStats> = emptyMap(),
    val forwards: List<Forward> = emptyList(),
    /** The subdomain domain, or "" when subdomains are not available. */
    val domain: String = "",
    val project: String? = null,
    val allProjects: Boolean = false,
    val collaborator: Boolean = false,
    /** Logs open under a card, by key. */
    val logs: Map<String, Load<List<String>>> = emptyMap(),
    val busy: String? = null,
    val note: Actions.Note? = null,
) {
    val shown: List<Service> get() = services.valueOrNull.orEmpty().filter { allProjects || project == null || it.alias.equals(project, true) }
    fun forwardOf(s: Service): Forward? = if (s.tunnelPort > 0) forwards.firstOrNull { it.port == s.tunnelPort } else null
    fun canEdit(s: Service) = !collaborator || s.mine
    /** Forwards on ports no shown service has, for this project or none. */
    val otherForwards: List<Forward>
        get() {
            val ports = services.valueOrNull.orEmpty().mapNotNull { it.tunnelPort.takeIf { p -> p > 0 } }.toSet()
            return forwards.filter { it.port !in ports && (allProjects || project == null || it.project.isEmpty() || it.project.equals(project, true)) }
        }
}

/** What the person can do on the Services screen. */
class ServicesEvents(
    val onAllProjects: (Boolean) -> Unit = {},
    val onRegister: (NewService) -> Unit = {},
    val onStart: (Service) -> Unit = {},
    val onStop: (Service) -> Unit = {},
    val onRestart: (Service) -> Unit = {},
    val onDelete: (Service) -> Unit = {},
    val onToggleLogs: (Service) -> Unit = {},
    val onSetPort: (Service, Int) -> Unit = { _, _ -> },
    /** Off or Tunnel at once; Subdomain and Path with the name and Public chosen in the card. */
    val onExpose: (Service, Exposure, String, Boolean) -> Unit = { _, _, _, _ -> },
    val onRemoveForward: (Forward) -> Unit = {},
    val onOpen: (String) -> Unit = {},
    val onCloseNote: () -> Unit = {},
    val onRetry: () -> Unit = {},
)

private val NAME_RX = Regex("^[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?$")

/** The desktop's default name for a forward: the last ":" part of the service name, made safe. */
fun forwardSlug(name: String): String =
    name.substringAfterLast(':').lowercase().replace(Regex("[^a-z0-9-]+"), "-").trim('-').take(32).ifEmpty { "app" }

@Composable
fun ServicesRoute() {
    val app = LocalAppServices.current
    val api = app.api
    val host = LocalHost.current
    val shell by app.shell.state.collectAsStateWithLifecycle()
    val live by api.events.state.collectAsStateWithLifecycle(initialValue = null)
    var all by remember { mutableStateOf(false) }
    val actions = rememberActions()
    val list = rememberLoad { api.services.list() }
    val fwd = rememberLoad { api.forwards.list() }
    val options = rememberLoad { runCatching { api.forwards.options() }.getOrNull() }
    var stats by remember { mutableStateOf<Map<String, ServiceStats>>(emptyMap()) }
    val logs = remember { mutableStateMapOf<String, Load<List<String>>>() }

    // The state push keeps status live; the list call adds cpu and containers. Merge, live wins on status.
    val services: Load<List<Service>> = when (val l = list.value) {
        is Load.Ready -> {
            val pushed = live?.services
            if (pushed == null) l else Load.Ready(pushed.map { p -> l.value.firstOrNull { it.key == p.key }?.let { p.copy(cpuPct = it.cpuPct, containers = it.containers) } ?: p })
        }
        else -> live?.services?.let { Load.Ready(it) } ?: l
    }
    LaunchedEffect(Unit) {
        while (true) {
            runCatching { stats = api.system.stats().services }
            delay(2000)
        }
    }
    val reload = { list.reload(); fwd.reload() }
    val domain = options.value.valueOrNull?.let { o -> if (o.choices.any { it.id == "subdomain" && it.available }) o.domain.ifEmpty { o.choices.first { it.id == "subdomain" }.suggestedDomain } else "" }
        ?: fwd.value.valueOrNull?.domain.orEmpty()

    ServicesScreen(
        state = ServicesState(
            services = services,
            stats = stats,
            forwards = fwd.value.valueOrNull?.forwards.orEmpty(),
            domain = domain,
            project = shell.currentProject,
            allProjects = all,
            collaborator = live?.role == "collaborator",
            logs = logs.toMap(),
            busy = actions.busy,
            note = actions.note,
        ),
        events = ServicesEvents(
            onAllProjects = { all = it },
            onRegister = { s ->
                actions.run("register", after = reload) {
                    val r = api.services.register(s.copy(alias = shell.currentProject))
                    actions.note = Actions.Note(if (r.pendingApproval) "Sent to the owner to approve" else "Registered ${s.name}", false)
                }
            },
            onStart = { s -> actions.run(s.key, "Started ${s.name}", reload) { api.services.start(s.key) } },
            onStop = { s -> actions.run(s.key, "Stopped ${s.name}", reload) { api.services.stop(s.key) } },
            onRestart = { s -> actions.run(s.key, "Restarted ${s.name}", reload) { api.services.restart(s.key) } },
            onDelete = { s -> actions.run(s.key, "Deleted ${s.key}", reload) { api.services.delete(s.key) } },
            onToggleLogs = { s ->
                if (logs.containsKey(s.key)) logs.remove(s.key) else {
                    logs[s.key] = Load.Loading
                    actions.run(s.key) {
                        logs[s.key] = try { Load.Ready(api.services.logs(s.key)) } catch (e: ApiException) { Load.Failed(errorText(e)) }
                    }
                }
            },
            onSetPort = { s, port ->
                actions.run(s.key, if (port > 0) "Tunnel port set to :$port" else "Tunnel port cleared", reload) {
                    api.services.setTunnel(s.key, s.tunnelEnabled && port > 0, port)
                }
            },
            onExpose = { s, mode, name, public ->
                val current = ServicesState(forwards = fwd.value.valueOrNull?.forwards.orEmpty()).forwardOf(s)
                actions.run(s.key, after = reload) {
                    if (current != null) api.forwards.remove(current.host)
                    when (mode) {
                        Exposure.Off -> { api.services.setTunnel(s.key, false, s.tunnelPort); actions.note = Actions.Note("No longer exposed", false) }
                        Exposure.Tunnel -> { api.services.setTunnel(s.key, true, s.tunnelPort); actions.note = Actions.Note("Starting a tunnel", false) }
                        else -> {
                            if (s.tunnelEnabled) api.services.setTunnel(s.key, false, s.tunnelPort)
                            val f = api.forwards.add(NewForward(port = s.tunnelPort, name = name, mode = mode.name.lowercase(), public = public, project = s.key.substringBefore(':')))
                            actions.note = Actions.Note("Reachable at ${f.url}", false)
                        }
                    }
                }
            },
            onRemoveForward = { f -> actions.run(f.host, "Removed", reload) { api.forwards.remove(f.host) } },
            onOpen = host::openExternal,
            onCloseNote = actions::clear,
            onRetry = reload,
        ),
    )
}

@Composable
fun ServicesScreen(state: ServicesState, events: ServicesEvents = ServicesEvents(), registering: Boolean = false) {
    var showRegister by remember { mutableStateOf(registering) }
    Column(Modifier.fillMaxSize()) {
        PanelHeader(
            title = if (state.project == null || state.allProjects) "Services · all projects" else "Services",
            meta = state.services.valueOrNull?.let { "${state.shown.count { it.running }} of ${state.shown.size} running" },
        ) {
            if (!state.collaborator || state.project != null)
                CrundiButton("Register service", { showRegister = true }, kind = ButtonKind.Primary, icon = "plus")
        }
        if (state.project != null) Row(Modifier.padding(horizontal = Dimens.ScreenPad, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiChip(state.project, selected = !state.allProjects, onClick = { events.onAllProjects(false) }, icon = "folder")
            CrundiChip("All projects", selected = state.allProjects, onClick = { events.onAllProjects(true) }, icon = "layers")
        }
        NoteBanner(state.note, events.onCloseNote)
        when (val s = state.services) {
            Load.Loading -> LoadingState(text = "Loading services")
            is Load.Failed -> ErrorState(s.message, onRetry = events.onRetry)
            is Load.Ready -> {
                val shown = state.shown
                LazyColumn(
                    Modifier.fillMaxSize(),
                    contentPadding = androidx.compose.foundation.layout.PaddingValues(Dimens.ScreenPad),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    if (shown.isEmpty()) item {
                        Box(Modifier.fillMaxWidth().height(300.dp)) {
                            EmptyState(
                                "server",
                                if (state.project != null && !state.allProjects) "No services registered for this project" else "No services registered",
                                body = "Register a dev server, a worker or a compose stack to start, stop and expose it from here.",
                                action = "Register service",
                                onAction = { showRegister = true },
                            )
                        }
                    }
                    items(shown, key = { it.key }) { svc -> ServiceCard(svc, state, events) }
                    val others = state.otherForwards
                    if (others.isNotEmpty() && !state.collaborator) item { OtherForwards(others, events) }
                }
            }
        }
    }
    if (showRegister) RegisterServiceDialog(state.project, onDismiss = { showRegister = false }, onRegister = { showRegister = false; events.onRegister(it) })
}

@Composable
private fun ServiceCard(s: Service, state: ServicesState, events: ServicesEvents) {
    val st = state.stats[s.key]
    val busy = state.busy == s.key
    val edit = state.canEdit(s)
    val fwd = state.forwardOf(s)
    var confirmDelete by remember { mutableStateOf(false) }
    var portDialog by remember { mutableStateOf(false) }
    var naming by remember { mutableStateOf<Exposure?>(null) }
    val statusKind = when (s.status) { "running" -> BadgeKind.Good; "crashed", "error" -> BadgeKind.Bad; else -> BadgeKind.Neutral }

    Column(
        Modifier
            .fillMaxWidth()
            .clip(Dimens.Shape)
            .background(CrundiColors.BgCard)
            .border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape)
            .padding(horizontal = 14.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Box(Modifier.size(14.dp), contentAlignment = Alignment.Center) {
                if (s.running) Box(Modifier.size(14.dp).clip(CircleShape).background(CrundiColors.GreenDim))
                Box(Modifier.size(8.dp).clip(CircleShape).background(statusKind.fg.takeIf { statusKind != BadgeKind.Neutral } ?: CrundiColors.TextMuted))
            }
            Column(Modifier.weight(1f)) {
                Text(s.name.ifEmpty { s.key }, style = CrundiType.Heading, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (state.allProjects || state.project == null) Text(s.alias, style = CrundiType.Meta)
            }
            StatusBadge(s.status.uppercase(), statusKind)
        }
        Text(
            buildString { append(s.command); if (!s.uptime.isNullOrEmpty() && s.running) append(" · up ${s.uptime}") },
            style = CrundiType.Meta.copy(fontSize = 12.sp),
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
        if (s.running) StatStrip(s, st)
        @OptIn(ExperimentalLayoutApi::class)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            if (s.running) {
                CrundiButton("Stop", { events.onStop(s) }, icon = "stop", enabled = !busy)
                CrundiButton("Restart", { events.onRestart(s) }, icon = "rotate-ccw", enabled = !busy)
            } else {
                CrundiButton("Start", { events.onStart(s) }, kind = ButtonKind.Primary, icon = "play", enabled = !busy)
            }
            CrundiButton(if (state.logs.containsKey(s.key)) "Hide logs" else "Logs", { events.onToggleLogs(s) }, icon = "file-text")
            if (edit && s.tunnelPort <= 0 && !s.tunnelEnabled) CrundiButton("Expose", { portDialog = true }, icon = "globe")
            if (edit) CrundiButton("Delete", { confirmDelete = true }, kind = ButtonKind.Danger, icon = "trash", enabled = !busy)
        }
        if (s.tunnelPort > 0 || s.tunnelEnabled || fwd != null) {
            ExposureRow(s, fwd, edit, state.domain, events, onPort = { portDialog = true }, onPick = { mode ->
                when (mode) {
                    Exposure.Off, Exposure.Tunnel -> { naming = null; events.onExpose(s, mode, "", false) }
                    else -> naming = mode
                }
            }, naming = naming)
            naming?.let { mode ->
                NamingForm(mode, state.domain, initial = fwd?.host ?: forwardSlug(s.name.ifEmpty { s.key }), initialPublic = fwd?.public == true,
                    onCancel = { naming = null }, onSave = { name, pub -> naming = null; events.onExpose(s, mode, name, pub) })
            }
        }
        state.logs[s.key]?.let { LogBox(it) }
    }

    if (confirmDelete) CrundiDialog(
        title = "Delete service \"${s.key}\"?",
        message = "It is stopped and taken off this list. Its project and files stay.",
        confirm = "Delete service",
        danger = true,
        onConfirm = { confirmDelete = false; events.onDelete(s) },
        onDismiss = { confirmDelete = false },
    )
    if (portDialog) PortDialog(s.tunnelPort, onDismiss = { portDialog = false }, onSave = { portDialog = false; events.onSetPort(s, it) })
}

@Composable
private fun StatStrip(s: Service, st: ServiceStats?) {
    val cpu = st?.cpuPct ?: s.cpuPct
    val mem = st?.memory ?: s.memory
    Row(
        Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgPrimary).padding(horizontal = 10.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        StatCell("CPU", Fmt.pct(cpu), st?.cpuHistory.orEmpty().map { it?.toFloat() }, sparkColor(cpu), max = 100f, Modifier.weight(1f))
        StatCell("Memory", Fmt.bytes(mem), st?.memHistory.orEmpty().map { it?.toFloat() }, CrundiColors.Sky, max = null, Modifier.weight(1f))
        val badge = when {
            s.containers != null -> s.containers.size.let { if (it == 0) "no containers" else if (it == 1) "1 container" else "$it containers" }
            (st?.pid ?: s.pid) != null -> "pid ${st?.pid ?: s.pid}"
            else -> ""
        }
        if (badge.isNotEmpty()) Text(badge, style = CrundiType.Meta)
    }
}

private fun sparkColor(cpu: Double?): Color = when {
    cpu == null -> CrundiColors.Green
    cpu >= 85 -> CrundiColors.Red
    cpu >= 60 -> CrundiColors.Yellow
    else -> CrundiColors.Green
}

@Composable
private fun StatCell(label: String, value: String, history: List<Float?>, color: Color, max: Float?, modifier: Modifier) {
    Column(modifier) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(label.uppercase(), style = CrundiType.Section.copy(fontSize = 10.sp))
            Text(value, style = CrundiType.Code.copy(fontSize = 12.sp))
        }
        Sparkline(history, color, max, Modifier.fillMaxWidth().height(18.dp).padding(top = 2.dp))
    }
}

/** A line of recent values (needs two points), scaled to [max] or 1.25 times the largest. */
@Composable
fun Sparkline(values: List<Float?>, color: Color, max: Float?, modifier: Modifier = Modifier) {
    val pts = values.filterNotNull()
    if (pts.size < 2) { Box(modifier); return }
    val top = max?.let { maxOf(it, pts.max()) } ?: (pts.max() * 1.25f).coerceAtLeast(1f)
    Canvas(modifier) {
        val step = size.width / (pts.size - 1)
        val path = Path()
        pts.forEachIndexed { i, v ->
            val p = Offset(i * step, size.height - (v / top) * size.height)
            if (i == 0) path.moveTo(p.x, p.y) else path.lineTo(p.x, p.y)
        }
        drawPath(path, color, style = Stroke(width = 1.5.dp.toPx()))
    }
}

@Composable
private fun ExposureRow(s: Service, fwd: Forward?, edit: Boolean, domain: String, events: ServicesEvents, onPort: () -> Unit, onPick: (Exposure) -> Unit, naming: Exposure?) {
    val active = naming ?: when {
        fwd != null -> if (fwd.mode == "subdomain") Exposure.Subdomain else Exposure.Path
        s.tunnelEnabled -> Exposure.Tunnel
        else -> Exposure.Off
    }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        DashedRule()
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiIcon("globe", size = Dimens.IconSm, tint = CrundiColors.TextSecondary)
            if (edit) CrundiChip(if (s.tunnelPort > 0) "Port ${s.tunnelPort}" else "Set port", selected = false, onClick = onPort)
            else StatusBadge("port ${s.tunnelPort}")
            Box(Modifier.weight(1f))
            if (fwd != null && edit) CrundiIconButton("trash", { events.onRemoveForward(fwd) }, contentDescription = "Remove this forward", tint = CrundiColors.Red, size = Dimens.IconSm, touch = 36.dp)
        }
        if (edit) Segments(
            Exposure.entries.filter { it != Exposure.Subdomain || domain.isNotEmpty() || fwd?.mode == "subdomain" }.map { it.name to it.label },
            active.name,
            { id ->
                val mode = Exposure.valueOf(id)
                if (s.tunnelPort <= 0 && mode != Exposure.Off) onPort() else onPick(mode)
            },
            Modifier.fillMaxWidth(),
        )
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            when {
                fwd != null -> {
                    Text(fwd.url, style = CrundiType.Meta.copy(color = CrundiColors.AccentHover, fontSize = 12.sp), maxLines = 1, overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f, fill = false).clickable { events.onOpen(fwd.url) })
                    if (fwd.public) StatusBadge("PUBLIC", BadgeKind.Warn)
                }
                s.tunnelStatus == "active" && !s.tunnelUrl.isNullOrEmpty() ->
                    Text(s.tunnelUrl, style = CrundiType.Meta.copy(color = CrundiColors.AccentHover, fontSize = 12.sp), maxLines = 1, overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.clickable { events.onOpen(s.tunnelUrl) })
                s.tunnelStatus == "connecting" -> Text("starting…", style = CrundiType.Meta.copy(color = CrundiColors.Yellow, fontSize = 12.sp))
                s.tunnelStatus == "error" -> Text("error", style = CrundiType.Meta.copy(color = CrundiColors.Red, fontSize = 12.sp))
                s.tunnelEnabled -> Text("idle", style = CrundiType.Meta.copy(fontSize = 12.sp))
                else -> Text("off: not reachable from outside", style = CrundiType.Meta.copy(fontSize = 12.sp))
            }
        }
    }
}

@Composable
fun DashedRule(modifier: Modifier = Modifier) {
    Canvas(modifier.fillMaxWidth().height(1.dp)) {
        drawLine(CrundiColors.Border, Offset(0f, 0f), Offset(size.width, 0f), strokeWidth = 1.dp.toPx(),
            pathEffect = androidx.compose.ui.graphics.PathEffect.dashPathEffect(floatArrayOf(6f, 5f)))
    }
}

@Composable
private fun NamingForm(mode: Exposure, domain: String, initial: String, initialPublic: Boolean, onCancel: () -> Unit, onSave: (String, Boolean) -> Unit) {
    var name by remember { mutableStateOf(initial) }
    var public by remember { mutableStateOf(initialPublic) }
    var error by remember { mutableStateOf<String?>(null) }
    var confirmPublic by remember { mutableStateOf(false) }
    fun save() {
        val n = name.trim().lowercase()
        error = when {
            n.isEmpty() -> "A name is required"
            !NAME_RX.matches(n) -> "Lowercase letters, digits and hyphens only, not starting or ending with one."
            else -> null
        }
        if (error == null) { if (public) confirmPublic = true else onSave(n, false) }
    }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        DashedRule()
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            if (mode == Exposure.Path) Text("/tunnel/", style = CrundiType.Code)
            CrundiTextField(name, { name = it.take(32) }, Modifier.weight(1f), placeholder = "name", mono = true)
            Text(if (mode == Exposure.Path) "/" else ".$domain", style = CrundiType.Code, maxLines = 1)
        }
        CheckRow("Public: reachable without signing in to Crundi", public) { public = it }
        error?.let { Text(it, style = CrundiType.Small.copy(color = CrundiColors.Red)) }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiButton("Save", { save() }, kind = ButtonKind.Primary, icon = "check")
            CrundiButton("Cancel", onCancel)
        }
    }
    if (confirmPublic) CrundiDialog(
        title = "Publish ${name.trim().lowercase()}?",
        message = "Anyone who knows the address reaches it without signing in to Crundi. Only do this for something that has its own sign-in.",
        confirm = "Publish",
        danger = true,
        onConfirm = { confirmPublic = false; onSave(name.trim().lowercase(), true) },
        onDismiss = { confirmPublic = false },
    )
}

/** A checkbox with its words, the whole row tappable. */
@Composable
fun CheckRow(label: String, checked: Boolean, modifier: Modifier = Modifier, onChange: (Boolean) -> Unit) {
    Row(modifier.fillMaxWidth().clip(Dimens.ShapeSm).clickable { onChange(!checked) }.padding(vertical = 2.dp), verticalAlignment = Alignment.CenterVertically) {
        Checkbox(checked, onChange, colors = CheckboxDefaults.colors(checkedColor = CrundiColors.Accent, uncheckedColor = CrundiColors.Border, checkmarkColor = Color.White))
        Text(label, style = CrundiType.Small.copy(color = CrundiColors.TextPrimary))
    }
}

@Composable
private fun LogBox(logs: Load<List<String>>) {
    val scroll = rememberScrollState()
    LaunchedEffect(logs) { scroll.scrollTo(scroll.maxValue) }
    Box(
        Modifier.fillMaxWidth().heightIn(max = 220.dp).clip(Dimens.ShapeSm).background(CrundiColors.BgPrimary)
            .border(Dimens.Hairline, CrundiColors.BorderSubtle, Dimens.ShapeSm).verticalScroll(scroll).padding(10.dp),
    ) {
        when (logs) {
            Load.Loading -> Text("Loading…", style = CrundiType.Meta)
            is Load.Failed -> Text("Error: ${logs.message}", style = CrundiType.Meta.copy(color = CrundiColors.Red))
            is Load.Ready -> SelectionContainer {
                Column {
                    if (logs.value.isEmpty()) Text("(no logs)", style = CrundiType.Meta)
                    for (line in logs.value) Text(
                        line,
                        style = CrundiType.Meta.copy(fontSize = 11.5.sp, color = if (line.startsWith("[err] ")) CrundiColors.Red.copy(alpha = 0.85f) else CrundiColors.TextSecondary),
                    )
                }
            }
        }
    }
}

@Composable
private fun OtherForwards(list: List<Forward>, events: ServicesEvents) {
    Column(
        Modifier.fillMaxWidth().clip(Dimens.Shape).background(CrundiColors.BgSecondary).border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape).padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text("Other forwards", style = CrundiType.Heading)
        Text("Not linked to a service. Manage them under Forwards.", style = CrundiType.Small)
        for ((i, f) in list.withIndex()) {
            if (i > 0) DashedRule()
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Column(Modifier.weight(1f)) {
                    Text(f.url.ifEmpty { f.host }, style = CrundiType.Meta.copy(color = CrundiColors.AccentHover, fontSize = 12.sp), maxLines = 1, overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.clickable { events.onOpen(f.url) })
                    Text("port ${f.port}" + (f.project.takeIf { it.isNotEmpty() }?.let { " · $it" } ?: ""), style = CrundiType.Meta)
                }
                if (f.public) StatusBadge("PUBLIC", BadgeKind.Warn)
                CrundiIconButton("trash", { events.onRemoveForward(f) }, contentDescription = "Remove this forward", tint = CrundiColors.Red, size = Dimens.IconSm, touch = 36.dp)
            }
        }
    }
}

@Composable
private fun PortDialog(current: Int, onDismiss: () -> Unit, onSave: (Int) -> Unit) {
    var text by remember { mutableStateOf(if (current > 0) current.toString() else "") }
    CrundiDialog(
        title = "Tunnel port",
        message = "Local port to expose. Enter 0 (or leave it blank) to clear.",
        confirm = "Save port",
        onConfirm = { onSave(text.trim().toIntOrNull()?.coerceIn(0, 65535) ?: 0) },
        onDismiss = onDismiss,
    ) {
        CrundiTextField(text, { t -> text = t.filter(Char::isDigit).take(5) }, placeholder = "3000", mono = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number))
    }
}

@Composable
fun RegisterServiceForm(project: String?, onCancel: () -> Unit, onRegister: (NewService) -> Unit, modifier: Modifier = Modifier) {
    var name by remember { mutableStateOf("") }
    var command by remember { mutableStateOf("") }
    var cwd by remember { mutableStateOf("") }
    var stop by remember { mutableStateOf("") }
    var port by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    Column(modifier, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        if (project != null) Text("In $project", style = CrundiType.Meta)
        CrundiTextField(name, { name = it }, placeholder = "Service name", mono = true)
        CrundiTextField(command, { command = it }, placeholder = "Command (e.g. npm run dev)", mono = true)
        CrundiTextField(cwd, { cwd = it }, placeholder = "Working directory (optional)", mono = true)
        CrundiTextField(stop, { stop = it }, placeholder = "Stop command (optional, e.g. docker compose down)", mono = true)
        CrundiTextField(port, { t -> port = t.filter(Char::isDigit).take(5) }, placeholder = "Port to expose (optional)", mono = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number))
        error?.let { Text(it, style = CrundiType.Small.copy(color = CrundiColors.Red)) }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp, Alignment.End)) {
            CrundiButton("Cancel", onCancel)
            CrundiButton("Register", {
                error = when {
                    name.isBlank() -> "Service name is required"
                    command.isBlank() -> "Command is required"
                    else -> null
                }
                if (error == null) onRegister(NewService(name.trim(), command.trim(), project, cwd.trim().ifEmpty { null }, stop.trim().ifEmpty { null }, port.toIntOrNull()?.takeIf { it > 0 }))
            }, kind = ButtonKind.Primary, icon = "check")
        }
    }
}

@Composable
private fun RegisterServiceDialog(project: String?, onDismiss: () -> Unit, onRegister: (NewService) -> Unit) {
    CrundiDialog(title = "Register a service", onDismiss = onDismiss, dismiss = null) {
        RegisterServiceForm(project, onDismiss, onRegister)
    }
}
