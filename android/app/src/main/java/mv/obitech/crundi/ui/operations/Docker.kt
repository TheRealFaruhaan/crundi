package mv.obitech.crundi.ui.operations

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
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
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.model.Container
import mv.obitech.crundi.api.model.DiskUse
import mv.obitech.crundi.api.model.DockerDisk
import mv.obitech.crundi.api.model.DockerEngine
import mv.obitech.crundi.api.model.DockerImage
import mv.obitech.crundi.api.model.DockerJob
import mv.obitech.crundi.api.model.DockerNetwork
import mv.obitech.crundi.api.model.DockerPort
import mv.obitech.crundi.api.model.DockerStack
import mv.obitech.crundi.api.model.DockerStatus
import mv.obitech.crundi.api.model.DockerVolume
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiChip
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiDivider
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.DialogCard
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.StatusBadge
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.shell.Area
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/** The Docker tab's views, in the desktop's order. Forwards has its own area on the phone. */
enum class DockerView(val label: String, val icon: String) {
    Containers("Containers", "docker"), Stacks("Stacks", "layers"), Images("Images", "archive"),
    Volumes("Volumes", "hard-drive"), Networks("Networks", "network"), System("System", "server"),
}

object DockerFmt {
    /** The state a container is in, as the desktop colours it. */
    fun stateKey(c: Container) = if (c.running && c.health == "unhealthy") "unhealthy" else c.state.ifEmpty { "exited" }

    fun stateColor(key: String): Pair<Color, Color?> = when (key) {
        "running" -> CrundiColors.Green to CrundiColors.GreenDim
        "paused", "unhealthy" -> CrundiColors.Yellow to CrundiColors.YellowDim
        "restarting" -> CrundiColors.Accent to null
        "dead" -> CrundiColors.Red to null
        else -> CrundiColors.TextMuted to null
    }

    /** "5432→5432", "127.0.0.1:8080→80", "80/tcp" (not published). */
    fun port(p: DockerPort): String {
        if (p.public == 0) return "${p.private}/${p.type}"
        val ip = if (p.ip.isEmpty() || p.ip == "0.0.0.0" || p.ip == "::") "" else "${p.ip}:"
        return "$ip${p.public}→${p.private}" + if (p.type == "udp") "/udp" else ""
    }

    /** The desktop's "ago" for docker times: minutes, hours, days, months, years. */
    fun ago(ms: Long, now: Long = System.currentTimeMillis()): String {
        if (ms <= 0) return "—"
        val s = (now - ms) / 1000
        return when {
            s < 60 -> "just now"
            s < 3600 -> "${s / 60} min ago"
            s < 48 * 3600 -> "${s / 3600} h ago"
            s < 60L * 86400 -> "${s / 86400} days ago"
            s < 730L * 86400 -> "${s / (30L * 86400)} months ago"
            else -> "${s / (365L * 86400)} years ago"
        }
    }

    /** Every word of the filter appears in one of the fields. */
    fun matches(filter: String, vararg fields: String?): Boolean {
        val words = filter.lowercase().split(Regex("\\s+")).filter { it.isNotEmpty() }
        if (words.isEmpty()) return true
        val hay = fields.filterNotNull().joinToString(" ").lowercase()
        return words.all { it in hay }
    }
}

@Composable
fun ContainerDot(c: Container, modifier: Modifier = Modifier) {
    val (fg, halo) = DockerFmt.stateColor(DockerFmt.stateKey(c))
    Box(modifier.size(14.dp), contentAlignment = Alignment.Center) {
        if (halo != null) Box(Modifier.size(14.dp).clip(CircleShape).background(halo))
        Box(Modifier.size(8.dp).clip(CircleShape).background(fg))
    }
}

/** A small mono pill (the desktop's dk-chip). */
@Composable
fun Chip(text: String, kind: BadgeKind = BadgeKind.Neutral, onClick: (() -> Unit)? = null) {
    Text(
        text, style = CrundiType.Meta.copy(color = kind.fg, fontSize = 11.sp), maxLines = 1,
        modifier = Modifier.clip(RoundedCornerShape(10.dp)).background(kind.bg).then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier).padding(horizontal = 8.dp, vertical = 2.dp),
    )
}

/** A docker job in flight (stack action, image pull): its title and the output so far. */
data class JobView(val id: String, val title: String, val job: DockerJob = DockerJob())

data class DockerState(
    val status: Load<DockerStatus> = Load.Loading,
    val view: DockerView = DockerView.Containers,
    val filter: String = "",
    val containers: Load<List<Container>> = Load.Loading,
    val stacks: Load<List<DockerStack>> = Load.Loading,
    val images: Load<List<DockerImage>> = Load.Loading,
    val volumes: Load<List<DockerVolume>> = Load.Loading,
    val networks: Load<List<DockerNetwork>> = Load.Loading,
    val engine: Load<DockerEngine> = Load.Loading,
    val disk: Load<DockerDisk> = Load.Loading,
    val job: JobView? = null,
    val busy: String? = null,
    val note: Actions.Note? = null,
)

class DockerEvents(
    val onView: (DockerView) -> Unit = {},
    val onFilter: (String) -> Unit = {},
    val onRefresh: () -> Unit = {},
    val onRetryStatus: () -> Unit = {},
    val onOpen: (Container) -> Unit = {},
    val onAct: (Container, String) -> Unit = { _, _ -> },
    val onStack: (DockerStack, String) -> Unit = { _, _ -> },
    val onPull: (String) -> Unit = {},
    val onRemoveImage: (DockerImage) -> Unit = {},
    val onRemoveVolume: (DockerVolume) -> Unit = {},
    val onRemoveNetwork: (DockerNetwork) -> Unit = {},
    /** images, images-all, containers, volumes, networks, build */
    val onPrune: (String) -> Unit = {},
    val onForwards: () -> Unit = {},
    val onCloseJob: () -> Unit = {},
    val onCloseNote: () -> Unit = {},
)

@Composable
fun DockerRoute() {
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val actions = rememberActions()
    val scope = rememberCoroutineScope()
    var view by rememberSaveable { mutableStateOf(DockerView.Containers) }
    var filters by remember { mutableStateOf(mapOf<DockerView, String>()) }
    var force by remember { mutableStateOf(false) }
    var job by remember { mutableStateOf<JobView?>(null) }
    val status = rememberLoad(force) { api.docker.status(force) }
    val ok = status.value.valueOrNull?.reachable == true
    val containers = rememberLoad(ok) { if (ok) api.docker.containers(stats = true) else emptyList() }
    val stacks = rememberLoad(ok) { if (ok) api.docker.stacks() else emptyList() }
    val images = rememberLoad(ok) { if (ok) api.docker.images() else emptyList() }
    val volumes = rememberLoad(ok) { if (ok) api.docker.volumes() else emptyList() }
    val networks = rememberLoad(ok) { if (ok) api.docker.networks() else emptyList() }
    val engine = rememberLoad(ok, view == DockerView.System) { if (ok) api.docker.engine() else DockerEngine() }
    val disk = rememberLoad(ok, view == DockerView.System) { if (ok && view == DockerView.System) api.docker.disk() else DockerDisk() }
    // The desktop's polling: containers every 3 s, stacks every 5 s, while on screen.
    LaunchedEffect(ok, view) {
        if (!ok) return@LaunchedEffect
        while (true) {
            delay(if (view == DockerView.Stacks) 5000 else 3000)
            if (view == DockerView.Stacks) stacks.reload() else containers.reload()
        }
    }
    val reloadAll = { containers.reload(); stacks.reload(); images.reload(); volumes.reload(); networks.reload(); engine.reload(); disk.reload() }
    fun follow(title: String, start: suspend () -> String) = actions.run("job") {
        val id = start()
        job = JobView(id, title)
        scope.launch {
            var from = 0
            var text = ""
            while (true) {
                val j = try { api.docker.job(id, from) } catch (e: ApiException) { job = job?.copy(job = DockerJob(title, text + "\n(lost track of this job)", done = true, code = -1)); break }
                text = (text + j.text).takeLast(300_000)
                from = j.next
                if (job?.id == id) job = job?.copy(job = j.copy(text = text))
                if (j.done) {
                    actions.note = Actions.Note(if ((j.code ?: 0) == 0) "$title finished" else "$title failed (exit ${j.code})", (j.code ?: 0) != 0)
                    reloadAll(); break
                }
                delay(700)
            }
        }
    }
    val verbs = mapOf("start" to "Started", "stop" to "Stopped", "restart" to "Restarted", "pause" to "Paused", "unpause" to "Resumed", "kill" to "Killed")
    DockerScreen(
        DockerState(
            status.value, view, filters[view].orEmpty(), containers.value, stacks.value, images.value, volumes.value, networks.value, engine.value, disk.value,
            job, actions.busy, actions.note,
        ),
        DockerEvents(
            onView = { view = it },
            onFilter = { filters = filters + (view to it) },
            onRefresh = reloadAll,
            onRetryStatus = { if (force) status.reload() else force = true },
            onOpen = { c -> nav.to(DockerRoutes.container(c.id)) },
            onAct = { c, verb -> actions.run(c.id, "${verbs[verb] ?: verb} ${c.name}", containers::reload) { api.docker.act(c.id, verb) } },
            onStack = { s, a -> follow("${s.name}: compose $a") { api.docker.stackAction(s.name, a) } },
            onPull = { ref -> follow("Pull $ref") { api.docker.pull(ref) } },
            onRemoveImage = { i -> actions.run(i.id, "Removed ${i.name}", images::reload) { api.docker.removeImage(i.id) } },
            onRemoveVolume = { v -> actions.run(v.name, "Removed ${v.name}", volumes::reload) { api.docker.removeVolume(v.name) } },
            onRemoveNetwork = { n -> actions.run(n.id, "Removed ${n.name}", networks::reload) { api.docker.removeNetwork(n.id) } },
            onPrune = { kind ->
                actions.run("prune-$kind", after = reloadAll) {
                    val r = api.docker.prune(kind)
                    actions.note = Actions.Note("Removed ${r.removed}" + (if (r.reclaimed > 0) ", freed ${Fmt.bytes(r.reclaimed)}" else ""), false)
                }
            },
            onForwards = { nav.area(Area.Forwards) },
            onCloseJob = { job = null },
            onCloseNote = actions::clear,
        ),
    )
}

@Composable
fun DockerScreen(state: DockerState, events: DockerEvents = DockerEvents()) {
    Column(Modifier.fillMaxSize()) {
        when (val st = state.status) {
            Load.Loading -> LoadingState(text = "Loading…")
            is Load.Failed -> ErrorState(st.message, onRetry = events.onRetryStatus)
            is Load.Ready -> when {
                !st.value.installed -> EmptyState("docker", "Docker is not installed", body = "Install Docker Engine on this machine and this tab will manage it.")
                !st.value.reachable -> EmptyState("docker", "Docker is installed, but Crundi cannot reach it", body = st.value.error.ifEmpty { null }, action = "Try again", onAction = events.onRetryStatus)
                else -> {
                    DockerNav(state, events)
                    NoteBanner(state.note, events.onCloseNote)
                    Box(Modifier.weight(1f)) {
                        when (state.view) {
                            DockerView.Containers -> ContainersView(state, events)
                            DockerView.Stacks -> StacksView(state, events)
                            DockerView.Images -> ImagesView(state, events)
                            DockerView.Volumes -> VolumesView(state, events)
                            DockerView.Networks -> NetworksView(state, events)
                            DockerView.System -> SystemView(state, events)
                        }
                    }
                }
            }
        }
    }
    state.job?.let { JobDialog(it, events.onCloseJob) }
}

@Composable
private fun DockerNav(state: DockerState, events: DockerEvents) {
    Column(Modifier.fillMaxWidth().background(CrundiColors.BgSecondary)) {
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = Dimens.ScreenPad, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            for (v in DockerView.entries) {
                val n = when (v) {
                    DockerView.Containers -> state.containers.valueOrNull?.size
                    DockerView.Stacks -> state.stacks.valueOrNull?.size
                    DockerView.Images -> state.images.valueOrNull?.size
                    DockerView.Volumes -> state.volumes.valueOrNull?.size
                    DockerView.Networks -> state.networks.valueOrNull?.size
                    DockerView.System -> null
                }
                CrundiChip(v.label + (n?.let { " $it" } ?: ""), selected = v == state.view, onClick = { events.onView(v) }, icon = v.icon)
            }
            CrundiChip("Forwards", selected = false, onClick = events.onForwards, icon = "globe")
        }
        if (state.view != DockerView.System) Row(Modifier.fillMaxWidth().padding(start = Dimens.ScreenPad, end = 4.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            CrundiTextField(state.filter, events.onFilter, Modifier.weight(1f), placeholder = "Filter", icon = "search")
            CrundiIconButton("refresh", events.onRefresh, contentDescription = "Refresh")
        }
        CrundiDivider(strong = true)
    }
}

/** Loading, failure and empty for one list; [body] draws the rows. */
@Composable
private fun <T> ListFrame(load: Load<List<T>>, emptyTitle: String, emptyBody: String?, emptyIcon: String, body: LazyListScope.(List<T>) -> Unit, header: (LazyListScope.(List<T>) -> Unit)? = null) {
    when (load) {
        Load.Loading -> LoadingState(text = "Loading…")
        is Load.Failed -> ErrorState("Docker did not answer: ${load.message}")
        is Load.Ready -> if (load.value.isEmpty() && header == null) EmptyState(emptyIcon, emptyTitle, body = emptyBody)
        else LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(vertical = 8.dp)) {
            header?.invoke(this, load.value)
            if (load.value.isEmpty()) item { EmptyState(emptyIcon, emptyTitle, body = emptyBody, modifier = Modifier.heightIn(max = 260.dp)) }
            else body(load.value)
        }
    }
}

private fun LazyListScope.noMatch(none: Boolean) {
    if (none) item { Text("Nothing matches the filter.", style = CrundiType.Small, modifier = Modifier.padding(Dimens.ScreenPad)) }
}

// ─── Containers ───

@Composable
private fun ContainersView(state: DockerState, events: DockerEvents) {
    ListFrame(state.containers, "No containers", "Nothing has been created on this machine yet.", "docker", header = { all ->
        if (all.isNotEmpty()) item {
            Column(Modifier.padding(horizontal = Dimens.ScreenPad, vertical = 4.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                    BigStat("${all.count { it.state == "running" }}", "running")
                    BigStat("${all.count { it.state == "paused" }}", "paused")
                    BigStat("${all.count { it.state != "running" && it.state != "paused" }}", "stopped")
                    BigStat(Fmt.pct(all.mapNotNull { it.stats?.cpuPct }.sum()), "cpu, of one core")
                    BigStat(Fmt.bytes(all.sumOf { it.stats?.memory ?: 0 }), "memory")
                }
                @OptIn(ExperimentalLayoutApi::class)
                FlowRow(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    for (c in all) Box(Modifier.size(16.dp).clip(RoundedCornerShape(3.dp)).background(DockerFmt.stateColor(DockerFmt.stateKey(c)).first).clickable { events.onOpen(c) })
                }
            }
            CrundiDivider(Modifier.padding(top = 8.dp))
        }
    }, body = { all ->
        val shown = all.filter { c -> DockerFmt.matches(state.filter, c.name, c.image, c.status, c.project, c.service, c.ports.joinToString(" ") { DockerFmt.port(it) }) }
        noMatch(shown.isEmpty())
        items(shown, key = { it.id }) { c -> ContainerRow(c, state.busy == c.id, events) }
    })
}

@Composable
private fun BigStat(value: String, label: String) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Text(value, style = CrundiType.Code.copy(fontSize = 16.sp, fontWeight = FontWeight.SemiBold))
        Text(label.uppercase(), style = CrundiType.Section.copy(fontSize = 9.sp))
    }
}

@Composable
private fun ContainerRow(c: Container, busy: Boolean, events: DockerEvents) {
    var confirmStop by remember { mutableStateOf(false) }
    Row(
        Modifier.fillMaxWidth().clickable { events.onOpen(c) }.padding(horizontal = Dimens.ScreenPad, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        ContainerDot(c)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(c.name, style = CrundiType.Body, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text("${c.image} · ${c.status}", style = CrundiType.Meta, maxLines = 1, overflow = TextOverflow.Ellipsis)
            @OptIn(ExperimentalLayoutApi::class)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                if (c.project.isNotEmpty()) Chip(c.project)
                for (p in c.ports.filter { it.public > 0 }.take(3)) Chip(DockerFmt.port(p))
                when (c.health) { "healthy" -> Chip("healthy", BadgeKind.Good); "unhealthy" -> Chip("unhealthy", BadgeKind.Bad); "starting" -> Chip("starting", BadgeKind.Warn) }
            }
        }
        if (c.running) Column(horizontalAlignment = Alignment.End) {
            Text(Fmt.pct(c.stats?.cpuPct), style = CrundiType.Code.copy(fontSize = 12.sp))
            Text(Fmt.bytes(c.stats?.memory), style = CrundiType.Meta)
        }
        when {
            c.state == "running" -> CrundiIconButton("stop", { confirmStop = true }, contentDescription = "Stop", enabled = !busy, touch = 40.dp)
            c.state == "paused" -> CrundiIconButton("play", { events.onAct(c, "unpause") }, contentDescription = "Resume", enabled = !busy, touch = 40.dp)
            else -> CrundiIconButton("play", { events.onAct(c, "start") }, contentDescription = "Start", tint = CrundiColors.Green, enabled = !busy, touch = 40.dp)
        }
    }
    CrundiDivider(Modifier.padding(horizontal = Dimens.ScreenPad))
    if (confirmStop) CrundiDialog("Stop ${c.name}?", { confirmStop = false }, confirm = "Stop", danger = true, onConfirm = { confirmStop = false; events.onAct(c, "stop") })
}

// ─── Stacks ───

@Composable
private fun StacksView(state: DockerState, events: DockerEvents) {
    ListFrame(state.stacks, "No Compose projects", "Projects started with docker compose show here, with their services.", "layers", body = { all ->
        val shown = all.filter { s -> DockerFmt.matches(state.filter, s.name, s.directory, s.containers.joinToString(" ") { it.name + " " + it.service }) }
        noMatch(shown.isEmpty())
        items(shown, key = { it.name }) { s -> StackCard(s, state.busy == "job", events) }
    })
}

@Composable
private fun StackCard(s: DockerStack, busy: Boolean, events: DockerEvents) {
    var confirm by remember { mutableStateOf<String?>(null) }
    Column(
        Modifier.padding(horizontal = Dimens.ScreenPad, vertical = 6.dp).fillMaxWidth().clip(Dimens.Shape).background(CrundiColors.BgCard)
            .border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape).padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiIcon("layers", size = Dimens.Icon, tint = CrundiColors.TextSecondary)
            Text(s.name, style = CrundiType.Heading, modifier = Modifier.weight(1f))
            Chip("${s.live}/${s.total} running", when { s.total > 0 && s.live == s.total -> BadgeKind.Good; s.live == 0 -> BadgeKind.Neutral; else -> BadgeKind.Warn })
        }
        Text((s.files.joinToString(", ").ifEmpty { "compose file unknown" }) + if (!s.filesPresent) "  (not found on this machine)" else "", style = CrundiType.Meta)
        @OptIn(ExperimentalLayoutApi::class)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiButton("Up", { events.onStack(s, "up") }, kind = ButtonKind.Primary, icon = "arrow-up", enabled = s.filesPresent && !busy)
            CrundiButton("Pull", { events.onStack(s, "pull") }, icon = "download", enabled = s.filesPresent && !busy)
            if (s.live > 0) {
                CrundiButton("Restart", { events.onStack(s, "restart") }, icon = "rotate-ccw", enabled = !busy)
                CrundiButton("Stop", { confirm = "stop" }, kind = ButtonKind.Danger, icon = "stop", enabled = !busy)
            } else CrundiButton("Start", { events.onStack(s, "start") }, icon = "play", enabled = !busy)
            CrundiButton("Down", { confirm = "down" }, kind = ButtonKind.Danger, icon = "trash", enabled = !busy)
        }
        if (s.containers.isNotEmpty()) CrundiDivider()
        for (c in s.containers) Row(
            Modifier.fillMaxWidth().clip(Dimens.ShapeSm).clickable { events.onOpen(c) }.padding(vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            ContainerDot(c)
            Column(Modifier.weight(1f)) {
                Text(c.service.ifEmpty { c.name }, style = CrundiType.Body)
                Text(c.name, style = CrundiType.Meta)
            }
            Text(c.status, style = CrundiType.Meta, maxLines = 1)
        }
    }
    confirm?.let { a ->
        CrundiDialog(if (a == "down") "Down: remove its containers?" else "Stop ${s.name}?", { confirm = null },
            message = if (a == "down") "docker compose down stops and removes the stack's containers and networks. Volumes stay." else null,
            confirm = if (a == "down") "Down" else "Stop", danger = true, onConfirm = { confirm = null; events.onStack(s, a) })
    }
}

// ─── Images, volumes, networks ───

@Composable
private fun ImagesView(state: DockerState, events: DockerEvents) {
    var ref by remember { mutableStateOf("") }
    var prune by remember { mutableStateOf<String?>(null) }
    ListFrame(state.images, "No images", null, "archive", header = { all ->
        item {
            Column(Modifier.padding(horizontal = Dimens.ScreenPad, vertical = 4.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    CrundiTextField(ref, { ref = it }, Modifier.weight(1f), placeholder = "Pull an image, e.g. nginx:alpine", mono = true)
                    CrundiButton("Pull", { if (ref.isNotBlank()) { events.onPull(ref.trim()); ref = "" } }, kind = ButtonKind.Primary, icon = "download", enabled = state.busy != "job")
                }
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    CrundiButton("Clean untagged", { prune = "images" })
                    CrundiButton("Clean unused", { prune = "images-all" })
                }
                Text("${all.size} images, ${all.count { it.usedBy.isNotEmpty() }} in use · ${Fmt.bytes(all.sumOf { it.size })}", style = CrundiType.Meta)
            }
            CrundiDivider(Modifier.padding(top = 6.dp))
        }
    }, body = { all ->
        val shown = all.filter { i -> DockerFmt.matches(state.filter, i.name, i.id, i.tags.joinToString(" "), i.usedBy.joinToString(" ")) }
        noMatch(shown.isEmpty())
        items(shown, key = { it.id }) { i ->
            ResourceRow(
                i.name, listOfNotNull(i.id, Fmt.bytes(i.size), DockerFmt.ago(i.created * 1000), i.tags.drop(1).takeIf { it.isNotEmpty() }?.let { "also " + it.joinToString(", ") }).joinToString(" · "),
                chips = {
                    i.usedBy.take(4).forEach { Chip(it, BadgeKind.Good) }
                    if (i.usedBy.size > 4) Chip("+${i.usedBy.size - 4}")
                    if (i.usedBy.isEmpty()) Chip(if (i.dangling) "untagged" else "unused", if (i.dangling) BadgeKind.Warn else BadgeKind.Neutral)
                },
                inUse = i.usedBy.isNotEmpty(), inUseWhy = "In use: remove its containers first", removeTitle = "Remove ${i.name}?", busy = state.busy == i.id,
                onRemove = { events.onRemoveImage(i) },
            )
        }
    })
    prune?.let { k -> PruneDialog(k, { prune = null }, events.onPrune) }
}

@Composable
private fun VolumesView(state: DockerState, events: DockerEvents) {
    var prune by remember { mutableStateOf<String?>(null) }
    ListFrame(state.volumes, "No volumes", null, "hard-drive", header = { all ->
        item {
            Row(Modifier.fillMaxWidth().padding(horizontal = Dimens.ScreenPad, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                Text("${all.size} volumes, ${all.count { it.usedBy.isEmpty() }} not used by any container. Sizes are under System.", style = CrundiType.Meta, modifier = Modifier.weight(1f))
                CrundiButton("Clean unused", { prune = "volumes" })
            }
            CrundiDivider(Modifier.padding(top = 6.dp))
        }
    }, body = { all ->
        val shown = all.filter { v -> DockerFmt.matches(state.filter, v.name, v.project, v.driver, v.usedBy.joinToString(" ")) }
        noMatch(shown.isEmpty())
        items(shown, key = { it.name }) { v ->
            ResourceRow(
                if (v.anonymous) "anonymous ${v.name.take(12)}" else v.name,
                listOfNotNull(v.driver, v.project.ifEmpty { null }, DockerFmt.ago(Fmt.iso(v.created))).joinToString(" · "),
                chips = { if (v.usedBy.isEmpty()) Chip("unused") else v.usedBy.forEach { Chip(it, BadgeKind.Good) } },
                inUse = v.usedBy.isNotEmpty(), inUseWhy = "In use", removeTitle = "Delete its data?", busy = state.busy == v.name,
                removeBody = "The volume ${v.name} and everything in it are deleted for good.",
                onRemove = { events.onRemoveVolume(v) },
            )
        }
    })
    prune?.let { k -> PruneDialog(k, { prune = null }, events.onPrune) }
}

@Composable
private fun NetworksView(state: DockerState, events: DockerEvents) {
    var prune by remember { mutableStateOf<String?>(null) }
    ListFrame(state.networks, "No networks", null, "network", header = { all ->
        item {
            Row(Modifier.fillMaxWidth().padding(horizontal = Dimens.ScreenPad, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                Text("${all.size} networks", style = CrundiType.Meta, modifier = Modifier.weight(1f))
                CrundiButton("Clean unused", { prune = "networks" })
            }
            CrundiDivider(Modifier.padding(top = 6.dp))
        }
    }, body = { all ->
        val shown = all.filter { n -> DockerFmt.matches(state.filter, n.name, n.driver, n.subnet, n.project, n.usedBy.joinToString(" ")) }
        noMatch(shown.isEmpty())
        items(shown, key = { it.id }) { n ->
            ResourceRow(
                n.name,
                listOfNotNull(n.driver, n.scope, n.subnet.ifEmpty { null }, n.gateway.ifEmpty { null }?.let { "via $it" }).joinToString(" · "),
                chips = {
                    if (n.builtin) Chip("built in")
                    if (n.internal) Chip("internal")
                    n.usedBy.take(6).forEach { Chip(it, BadgeKind.Good) }
                    if (n.usedBy.size > 6) Chip("+${n.usedBy.size - 6}")
                },
                inUse = n.builtin || n.usedBy.isNotEmpty(), inUseWhy = if (n.builtin) "Docker's own" else "In use", removeTitle = "Remove ${n.name}?", busy = state.busy == n.id,
                onRemove = { events.onRemoveNetwork(n) },
            )
        }
    })
    prune?.let { k -> PruneDialog(k, { prune = null }, events.onPrune) }
}

@Composable
private fun ResourceRow(title: String, sub: String, chips: @Composable () -> Unit, inUse: Boolean, inUseWhy: String, removeTitle: String, busy: Boolean, onRemove: () -> Unit, removeBody: String? = null) {
    var confirm by remember { mutableStateOf(false) }
    Row(Modifier.fillMaxWidth().padding(horizontal = Dimens.ScreenPad, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(title, style = CrundiType.Body, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(sub, style = CrundiType.Meta, maxLines = 2, overflow = TextOverflow.Ellipsis)
            @OptIn(ExperimentalLayoutApi::class)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) { chips() }
        }
        CrundiIconButton("trash", { confirm = true }, contentDescription = if (inUse) inUseWhy else "Remove", tint = CrundiColors.Red, enabled = !inUse && !busy, touch = 40.dp, size = Dimens.IconSm)
    }
    CrundiDivider(Modifier.padding(horizontal = Dimens.ScreenPad))
    if (confirm) CrundiDialog(removeTitle, { confirm = false }, message = removeBody, confirm = "Remove", danger = true, onConfirm = { confirm = false; onRemove() })
}

private val PRUNE_WORDS = mapOf(
    "images" to "Remove untagged images?",
    "images-all" to "Remove every unused image?",
    "containers" to "Remove every stopped container?",
    "volumes" to "Delete every unused volume and its data?",
    "networks" to "Remove unused networks?",
    "build" to "Clear the whole build cache?",
)

@Composable
private fun PruneDialog(kind: String, onDismiss: () -> Unit, onPrune: (String) -> Unit) {
    CrundiDialog(PRUNE_WORDS[kind] ?: "Clean up?", onDismiss,
        message = "Docker is shared by every project on this machine. A stopped container may be one someone means to start again.",
        confirm = "Clean", danger = true, onConfirm = { onDismiss(); onPrune(kind) })
}

// ─── System ───

@Composable
private fun SystemView(state: DockerState, events: DockerEvents) {
    var prune by remember { mutableStateOf<String?>(null) }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(vertical = 8.dp)) {
        Group("Engine") {
            when (val l = state.engine) {
                Load.Loading -> Text("Loading…", style = CrundiType.Small)
                is Load.Failed -> Text(l.message, style = CrundiType.Small.copy(color = CrundiColors.Red))
                is Load.Ready -> with(l.value) {
                    KeyValue("Docker", "$version (API $apiVersion)")
                    KeyValue("Compose", compose.ifEmpty { "not installed" })
                    KeyValue("System", os)
                    KeyValue("Kernel", kernel)
                    KeyValue("Machine", "$cpus CPUs · ${Fmt.bytes(memory)} · $arch")
                    KeyValue("Storage", storageDriver)
                    KeyValue("Cgroups", cgroup)
                    KeyValue("Data", rootDir)
                    CrundiDivider()
                    KeyValue("Containers", "$containers ($running running, $paused paused, $stopped stopped)")
                    KeyValue("Images", "$images")
                    for (w in warnings) Text(w, style = CrundiType.Meta.copy(color = CrundiColors.Yellow))
                }
            }
        }
        Group("Disk use", detail = "Docker is shared by every project on this machine. A stopped container may be one someone means to start again.") {
            when (val l = state.disk) {
                Load.Loading -> Text("Measuring… Docker walks every layer for this, which can take a while.", style = CrundiType.Small)
                is Load.Failed -> Text(l.message, style = CrundiType.Small.copy(color = CrundiColors.Red))
                is Load.Ready -> {
                    val d = l.value
                    val rows = listOf(
                        Triple("Images", "Cleaning removes every image no container uses", d.images to "images-all"),
                        Triple("Stopped containers", "Their writable layers; running ones are kept", d.containers to "containers"),
                        Triple("Volumes", "Data in volumes no container uses. Deleted for good.", d.volumes to "volumes"),
                        Triple("Build cache", "Rebuilt on the next build", d.build to "build"),
                    )
                    for ((i, r) in rows.withIndex()) {
                        if (i > 0) CrundiDivider()
                        DiskRow(r.first, r.second, r.third.first, kind = r.third.second, busy = state.busy == "prune-${r.third.second}") { prune = it }
                    }
                }
            }
        }
    }
    prune?.let { k -> PruneDialog(k, { prune = null }, events.onPrune) }
}

@Composable
private fun DiskRow(label: String, note: String, use: DiskUse?, kind: String, busy: Boolean, onClean: (String) -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(label + (use?.count?.let { " ($it)" } ?: ""), style = CrundiType.Body)
            Text(use?.error?.ifEmpty { null } ?: note, style = CrundiType.Small)
            Text("${Fmt.bytes(use?.size)} · ${Fmt.bytes(use?.reclaimable)} reclaimable", style = CrundiType.Meta)
        }
        CrundiButton("Clean", { onClean(kind) }, kind = ButtonKind.Danger, enabled = !busy && (kind == "containers" || (use?.reclaimable ?: 0) > 0 || (use?.error ?: "").isNotEmpty()))
    }
}

// ─── Job output ───

@Composable
fun JobCard(job: JobView, onClose: () -> Unit, modifier: Modifier = Modifier) {
    val j = job.job
    DialogCard(job.title, modifier, confirm = null, dismiss = "Hide", onDismiss = onClose) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiIcon("terminal", size = Dimens.IconSm, tint = CrundiColors.TextSecondary)
            when {
                !j.done -> StatusBadge("running", BadgeKind.Info)
                (j.code ?: 0) == 0 -> StatusBadge("done", BadgeKind.Good)
                else -> StatusBadge("exit ${j.code}", BadgeKind.Bad)
            }
            if (!j.done) Text("Hide keeps it running.", style = CrundiType.Meta)
        }
        val scroll = rememberScrollState()
        LaunchedEffect(j.text.length) { scroll.scrollTo(scroll.maxValue) }
        Text(
            j.text.ifEmpty { "Starting…" }, style = CrundiType.Code.copy(fontSize = 11.5.sp, lineHeight = 16.sp, color = CrundiColors.TextSecondary),
            modifier = Modifier.fillMaxWidth().heightIn(min = 120.dp, max = 380.dp).clip(Dimens.ShapeSm).background(CrundiColors.BgPrimary).verticalScroll(scroll).padding(10.dp),
        )
    }
}

@Composable
private fun JobDialog(job: JobView, onClose: () -> Unit) {
    androidx.compose.ui.window.Dialog(onDismissRequest = onClose) { JobCard(job, onClose, Modifier.padding(16.dp)) }
}
