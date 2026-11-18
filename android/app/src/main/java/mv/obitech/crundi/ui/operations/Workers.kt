package mv.obitech.crundi.ui.operations

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.repeatOnLifecycle
import androidx.navigation.NavGraphBuilder
import androidx.navigation.compose.composable
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.conflate
import kotlinx.coroutines.launch
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.CrundiClient
import mv.obitech.crundi.api.DockerShellEvent
import mv.obitech.crundi.api.WorkersApi
import mv.obitech.crundi.api.model.Worker
import mv.obitech.crundi.api.model.WorkerCode
import mv.obitech.crundi.api.model.WorkerEvent
import mv.obitech.crundi.api.model.WorkerJob
import mv.obitech.crundi.api.model.WorkersOverview
import mv.obitech.crundi.api.model.parseWorkerLabels
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiDivider
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.CrundiSwitchRow
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.ErrorWords
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.StatusBadge
import mv.obitech.crundi.ui.components.TopBar
import mv.obitech.crundi.ui.components.errorOrUnsupported
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/*
 * Settings > Workers (capability "workers"; specs/workers-protocol.md), as
 * the web's Workers page: the owner's PCs working for this server. The list,
 * Add a PC (a join code and the line to run), and per worker a page with
 * its switches, labels, tools, log and history, Revoke and Remove, and a
 * terminal on it over the UI websocket (as a container's shell). Owner only.
 * The list is asked again on the server's `workers` push and whenever the
 * screen comes back to the front.
 */

object WorkerRoutes {
    const val WORKER = "settings/workers/{id}"
    const val TERMINAL = "settings/workers/{id}/terminal"
    fun worker(id: String) = "settings/workers/" + java.net.URLEncoder.encode(id, "UTF-8")
    fun terminal(id: String) = worker(id) + "/terminal"
}

fun NavGraphBuilder.workerRoutes() {
    composable(WorkerRoutes.WORKER) { e -> WorkerRoute(e.arguments?.getString("id").orEmpty()) }
    composable(WorkerRoutes.TERMINAL) { e -> WorkerTerminalRoute(e.arguments?.getString("id").orEmpty()) }
}

/**
 * The workers list as the screen holds it, fetched again on push and on
 * resume. With [jobsOf] (a PC's page) also that PC's jobs and, when one is
 * open, its log tail.
 */
class WorkersHolder(private val api: CrundiClient, private val scope: CoroutineScope, private val jobsOf: String? = null) {
    var list by mutableStateOf<Load<WorkersOverview>>(Load.Loading)
    /** Null: not loaded yet, or the server has no jobs (a phase-1 server); the page then shows no Jobs. */
    var jobs by mutableStateOf<List<WorkerJob>?>(null)
    /** The job unfolded, and its view with the log tail (null while it loads). */
    var openJob by mutableStateOf<String?>(null)
    var jobView by mutableStateOf<WorkerJob?>(null)
    /** When the last fetch began, for the once-a-second limit on pushes. */
    var lastFetch = 0L

    suspend fun fetch() {
        lastFetch = System.currentTimeMillis()
        list = try { Load.Ready(api.workers.list()) } catch (e: ApiException) {
            if (list is Load.Ready) list else Load.Failed(errorOrUnsupported(e))
        }
        val id = jobsOf ?: return
        // A failed ask keeps what was there (none yet: no section, never an error banner).
        try { jobs = api.workers.jobs(id) } catch (_: ApiException) { }
        val open = openJob ?: return
        try { api.workers.jobLog(open)?.let { if (openJob == open) jobView = it } } catch (_: ApiException) { /* gone */ }
    }

    /** Fold or unfold a job; unfolding loads its log. */
    fun toggleJob(job: String) {
        openJob = if (openJob == job) null else job
        jobView = null
        refresh()
    }

    fun refresh() { scope.launch { fetch() } }
}

@Composable
fun rememberWorkers(supported: Boolean, jobsOf: String? = null): WorkersHolder {
    val api = LocalAppServices.current.api
    val scope = rememberCoroutineScope()
    val h = remember(api, jobsOf) { WorkersHolder(api, scope, jobsOf) }
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    LaunchedEffect(supported, lifecycle) {
        if (!supported) return@LaunchedEffect
        // Each time the screen is in front: once now, then on every push.
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            h.fetch()
            // A running job's output pushes "workers" many times a second:
            // at most one refresh a second, the pushes in between folded in.
            api.workerChanges().conflate().collect {
                val wait = 1_000 - (System.currentTimeMillis() - h.lastFetch)
                if (wait > 0) delay(wait)
                h.fetch()
            }
        }
    }
    return h
}

/** The clock for countdowns and "last seen": every second while a code is waiting, else every 30 s. */
@Composable
fun rememberNow(fast: Boolean): Long {
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(fast) { while (true) { now = System.currentTimeMillis(); delay(if (fast) 1_000 else 30_000) } }
    return now
}

// ─── Words ───

/** online, paused, revoked or offline, as a badge. */
fun workerState(w: Worker): Pair<String, BadgeKind> = when {
    w.revoked -> "revoked" to BadgeKind.Bad
    w.online && w.paused -> "paused" to BadgeKind.Warn
    w.online -> "online" to BadgeKind.Good
    w.paused -> "paused" to BadgeKind.Warn
    else -> "offline" to BadgeKind.Neutral
}

/** "linux 6.8 · x86_64 · 16 threads · 31 GB RAM". */
fun workerMachine(w: Worker): String {
    val i = w.info ?: return ""
    return listOfNotNull(
        listOf(i.os, i.osVersion).filter { it.isNotEmpty() }.joinToString(" ").ifEmpty { null },
        i.arch.ifEmpty { null },
        i.cpu?.threads?.takeIf { it > 0 }?.let { "$it thread${if (it == 1) "" else "s"}" },
        i.ramMb.takeIf { it > 0 }?.let { "${i.ramGb} GB RAM" },
    ).joinToString(" · ")
}

fun mb(v: Long): String = Fmt.bytes(v * 1024 * 1024)

/** "4:59" left of a code, or "expired". */
fun countdown(expiresMs: Long, now: Long): String {
    if (expiresMs <= 0) return ""
    val s = (expiresMs - now) / 1000
    return if (s <= 0) "expired" else "%d:%02d left".format(s / 60, s % 60)
}

/** "42 s", "3 min 5 s", "1 h 12 min": a job's duration, as the web. */
fun jobDuration(ms: Long): String {
    val s = Math.round(ms / 1000.0)
    return when {
        s < 60 -> "$s s"
        s < 3600 -> "${s / 60} min ${s % 60} s"
        else -> "${s / 3600} h ${(s % 3600) / 60} min"
    }
}

/** A job's state as its chip: done green; failed, lost, error, refused red; running yellow ("PC away" when the PC went offline). */
fun jobChip(j: WorkerJob): Pair<String, BadgeKind> {
    val kind = when {
        j.state == "done" -> BadgeKind.Good
        j.state in setOf("failed", "lost", "error", "refused") -> BadgeKind.Bad
        j.running -> BadgeKind.Warn
        else -> BadgeKind.Neutral
    }
    val word = when {
        j.running -> if (j.offline) "PC away" else j.phase?.ifEmpty { null } ?: j.state
        j.state == "failed" -> "failed (exit ${j.exitCode ?: "?"})"
        else -> j.state
    }
    return word to kind
}

/** "crundi · main 1a2b3c4 · 42 s · 5m ago · 2 files back (8.1 MB)", the line under a job's command. */
fun jobMeta(j: WorkerJob, now: Long): String = listOfNotNull(
    j.project?.ifEmpty { null },
    if (!j.branch.isNullOrEmpty()) j.branch + (j.commit?.takeIf { it.isNotEmpty() }?.let { " " + it.take(7) } ?: "") else j.source?.ifEmpty { null },
    j.durationMs?.let { jobDuration(it) },
    Fmt.ago(j.createdMs, now),
    j.artifacts?.takeIf { it.count > 0 }?.let { a -> "${a.count} file${if (a.count == 1) "" else "s"} back (${Fmt.bytes(a.bytes)})" },
).joinToString(" · ")

/** A history event in words: "terminal-opened" -> "Terminal opened", with what it carries. */
fun workerEventWords(e: WorkerEvent): String {
    val what = e.what.replace('-', ' ').replace('_', ' ').replaceFirstChar { it.uppercase() }.ifEmpty { "Event" }
    val extra = e.fields.filterKeys { it != "id" && it != "worker" && it != "ok" }.filterValues { it.isNotEmpty() }
    return what + if (extra.isNotEmpty()) " (" + extra.entries.joinToString(", ") { "${it.key} ${it.value}" } + ")" else ""
}

private fun serverUrl(api: CrundiClient) = api.transport.baseUrl.trimEnd('/')

// ─── The list ───

@Composable
fun WorkersRoute() {
    val app = LocalAppServices.current
    val api = app.api
    val nav = LocalNavigator.current
    val clip = LocalClipboardManager.current
    val shell by app.shell.state.collectAsStateWithLifecycle()
    val supported = WorkersApi.supported(shell.capabilities)
    val actions = rememberActions()
    val workers = rememberWorkers(supported)
    var shown by remember { mutableStateOf<WorkerCode?>(null) }
    val codes = workers.list.valueOrNull?.codes.orEmpty()
    val now = rememberNow(shown != null || codes.isNotEmpty())
    WorkersScreen(
        workers.list, supported, now, actions.busy, actions.note, serverUrl(api),
        onAdd = { actions.run("code") { shown = api.workers.code(); workers.refresh() } },
        onOpen = { nav.to(WorkerRoutes.worker(it.id)) },
        onShowCode = { shown = it },
        onCopy = { label, text -> clip.setText(AnnotatedString(text)); actions.note = Actions.Note("$label copied", false) },
        onRetry = { workers.list = Load.Loading; workers.refresh() },
        onCloseNote = actions::clear,
        onBack = { nav.back() },
    )
    shown?.let { c -> JoinCodeDialog(c, now, serverUrl(api), onCopy = { label, text -> clip.setText(AnnotatedString(text)); actions.note = Actions.Note("$label copied", false) }) { shown = null } }
}

@Composable
fun WorkersScreen(
    list: Load<WorkersOverview>,
    supported: Boolean,
    now: Long,
    busy: String? = null,
    note: Actions.Note? = null,
    server: String = "",
    onAdd: () -> Unit = {},
    onOpen: (Worker) -> Unit = {},
    onShowCode: (WorkerCode) -> Unit = {},
    onCopy: (String, String) -> Unit = { _, _ -> },
    onRetry: () -> Unit = {},
    onCloseNote: () -> Unit = {},
    onBack: () -> Unit = {},
) {
    Column(Modifier.fillMaxSize()) {
        TopBar("Workers", onBack = onBack, subtitle = list.valueOrNull?.let { ov ->
            val on = ov.workers.count { it.online }
            "$on of ${ov.workers.size} online"
        })
        NoteBanner(note, onCloseNote)
        if (!supported) {
            EmptyState("server", "Workers", body = ErrorWords.NOT_SUPPORTED)
            return@Column
        }
        when (list) {
            Load.Loading -> LoadingState(text = "Loading workers")
            is Load.Failed -> ErrorState(list.message, onRetry = onRetry)
            is Load.Ready -> Column(
                Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = Dimens.ScreenPad, vertical = 12.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                val ov = list.value
                Text("Your PCs, working for this server: builds, renders and commands run on them. Each one joins with a code and can be paused, switched off for chats or commands, or removed.", style = CrundiType.Small)
                Row { CrundiButton("Add a PC", onAdd, kind = ButtonKind.Primary, icon = "plus", enabled = busy != "code") }
                val waiting = ov.codes.filter { it.expiresMs == 0L || it.expiresMs > now }
                if (waiting.isNotEmpty()) HpCard {
                    Text("Join codes waiting", style = CrundiType.Heading)
                    for (c in waiting) Row(
                        Modifier.fillMaxWidth().clip(Dimens.ShapeSm).clickable { onShowCode(c) }.padding(vertical = 4.dp),
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        CrundiIcon("key", size = Dimens.IconSm, tint = CrundiColors.TextSecondary)
                        Text(c.code, style = CrundiType.Code, modifier = Modifier.weight(1f))
                        Text(countdown(c.expiresMs, now), style = CrundiType.Meta)
                        CrundiIconButton("copy", { onCopy("Code", c.code) }, contentDescription = "Copy the code", size = Dimens.IconSm, touch = 32.dp)
                    }
                }
                if (ov.workers.isEmpty()) Text("No workers yet. Add a PC: run the join line on it, or switch on \"Use this PC as a worker\" in the Crundi PC client.", style = CrundiType.Small)
                for (w in ov.workers.sortedWith(compareByDescending<Worker> { it.online }.thenBy { it.title.lowercase() })) WorkerCard(w, now) { onOpen(w) }
            }
        }
    }
}

@Composable
fun OnlineDot(online: Boolean, modifier: Modifier = Modifier) {
    Box(modifier.size(8.dp).clip(CircleShape).background(if (online) CrundiColors.Green else CrundiColors.TextMuted))
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun WorkerLabels(w: Worker) {
    if (w.labels.isEmpty() && w.customLabels.isEmpty()) return
    FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        for (l in w.derivedLabels) StatusBadge(l, BadgeKind.Neutral)
        for (l in w.customLabels) StatusBadge(l, BadgeKind.Accent)
    }
}

@Composable
private fun WorkerCard(w: Worker, now: Long, onOpen: () -> Unit) {
    val i = w.info
    HpCard(Modifier.alpha(if (w.online) 1f else 0.75f).clip(Dimens.Shape).clickable(onClickLabel = "Open ${w.title}", onClick = onOpen)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OnlineDot(w.online)
            Text(w.title, style = CrundiType.Heading, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (w.paused) StatusBadge("paused", BadgeKind.Warn)
            else if (w.busy && !w.revoked) StatusBadge("running a job", BadgeKind.Good)
            if (w.revoked) StatusBadge("revoked", BadgeKind.Bad)
            if (w.kind.isNotEmpty()) StatusBadge(w.kind, BadgeKind.Accent)
            CrundiIcon("chevron-right", size = Dimens.IconSm, tint = CrundiColors.TextMuted)
        }
        val machine = workerMachine(w)
        Text(listOfNotNull(w.version.ifEmpty { null }?.let { "v$it" }, machine.ifEmpty { null }).joinToString(" · ").ifEmpty { "No details yet" }, style = CrundiType.Meta)
        if (i != null && i.gpus.isNotEmpty()) Text("GPU: " + i.gpus.joinToString(", ") { it.name.ifEmpty { it.vendor } }, style = CrundiType.Small, maxLines = 2, overflow = TextOverflow.Ellipsis)
        i?.disk?.takeIf { it.totalMb > 0 || it.freeMb > 0 }?.let { d -> Text("${mb(d.freeMb)} free" + if (d.totalMb > 0) " of ${mb(d.totalMb)}" else "", style = CrundiType.Small) }
        Text(
            when {
                w.online && w.connectedMs > 0 -> "Connected for ${Fmt.span(now - w.connectedMs)}"
                w.online -> "Connected"
                w.lastSeenMs > 0 -> "Last seen ${Fmt.ago(w.lastSeenMs, now)}"
                else -> "Never connected"
            },
            style = CrundiType.Small,
        )
        WorkerLabels(w)
    }
}

// ─── Add a PC ───

@Composable
fun JoinCodeDialog(c: WorkerCode, now: Long, server: String, onCopy: (String, String) -> Unit, onClose: () -> Unit) {
    val join = "crundi-worker join $server ${c.code}"
    CrundiDialog("Add a PC", onClose, confirm = "Done", onConfirm = onClose, dismiss = null) {
        Column(Modifier.heightIn(max = 560.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            SelectionContainer {
                Text(
                    c.code, textAlign = TextAlign.Center,
                    style = CrundiType.Code.copy(fontSize = 34.sp, lineHeight = 42.sp, fontWeight = FontWeight.Bold, letterSpacing = 4.sp, color = CrundiColors.AccentHover),
                    modifier = Modifier.fillMaxWidth().clip(Dimens.Shape).background(CrundiColors.BgPrimary).border(Dimens.Hairline, CrundiColors.BorderSubtle, Dimens.Shape).padding(vertical = 14.dp),
                )
            }
            val left = countdown(c.expiresMs, now)
            if (left.isNotEmpty()) Text(
                if (left == "expired") "This code has expired. Make another with Add a PC." else "Single use, $left.",
                style = CrundiType.Small.copy(color = if (left == "expired") CrundiColors.Red else CrundiColors.TextSecondary),
                modifier = Modifier.fillMaxWidth(), textAlign = TextAlign.Center,
            )
            Text("On a PC without the Crundi client, install crundi-worker and run:", style = CrundiType.Small)
            WorkerCopyLine("Join line", join, onCopy)
            CrundiDivider()
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.Top) {
                CrundiIcon("info", size = Dimens.IconSm, tint = CrundiColors.TextSecondary, modifier = Modifier.padding(top = 2.dp))
                Text("A PC with the Crundi PC client needs no code: switch on \"Use this PC as a worker\" in its settings.", style = CrundiType.Small)
            }
        }
    }
}

@Composable
fun WorkerCopyLine(label: String, text: String, onCopy: (String, String) -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(label, style = CrundiType.Small, modifier = Modifier.weight(1f))
            CrundiIconButton("copy", { onCopy(label, text) }, contentDescription = "Copy $label", size = Dimens.IconSm, touch = 32.dp)
        }
        SelectionContainer {
            Text(
                text, style = CrundiType.Code.copy(fontSize = 12.sp, lineHeight = 17.sp),
                modifier = Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgPrimary).border(Dimens.Hairline, CrundiColors.BorderSubtle, Dimens.ShapeSm).padding(8.dp),
            )
        }
    }
}

// ─── One worker ───

@Composable
fun WorkerRoute(id: String) {
    val app = LocalAppServices.current
    val api = app.api
    val nav = LocalNavigator.current
    val clip = LocalClipboardManager.current
    val shell by app.shell.state.collectAsStateWithLifecycle()
    val supported = WorkersApi.supported(shell.capabilities)
    val actions = rememberActions()
    val workers = rememberWorkers(supported, jobsOf = id)
    var history by remember { mutableStateOf<Load<List<WorkerEvent>>?>(null) }
    var shown by remember { mutableStateOf<WorkerCode?>(null) }
    val now = rememberNow(shown != null)
    val w = workers.list.valueOrNull?.workers?.firstOrNull { it.id == id }
    fun change(done: String?, after: () -> Unit = {}, block: suspend () -> Unit) =
        actions.run(id, done, after = { workers.refresh(); after() }, block = block)
    fun loadHistory() {
        history = history?.takeIf { it is Load.Ready } ?: Load.Loading
        actions.run("history") {
            history = try { Load.Ready(api.workers.history(id)) } catch (e: ApiException) { Load.Failed(errorOrUnsupported(e)) }
        }
    }
    WorkerScreen(
        id, w, workers.list, now, actions.busy == id, actions.note, history,
        WorkerJobsView(workers.jobs, workers.openJob, workers.jobView, actions.busy?.startsWith("job:") == true),
        WorkerEvents(
            onRename = { n -> change("Renamed to $n") { api.workers.rename(id, n) } },
            onAllowChats = { on -> change(if (on) "Chats allowed" else "Chats not allowed") { api.workers.set(id, allowChats = on) } },
            onAllowCommands = { on -> change(if (on) "Commands allowed" else "Commands not allowed") { api.workers.set(id, allowCommands = on) } },
            onPaused = { on -> change(if (on) "Paused" else "Resumed") { api.workers.set(id, paused = on) } },
            onLabels = { l -> change("Labels saved") { api.workers.set(id, labels = l) } },
            onHistory = ::loadHistory,
            onTerminal = { nav.to(WorkerRoutes.terminal(id)) },
            onRevoke = { change("Revoked") { api.workers.revoke(id) } },
            onReissue = { change(null) { api.workers.reissue(id)?.let { shown = it } } },
            onRemove = { change("Removed", after = { nav.back() }) { api.workers.remove(id) } },
            onJob = workers::toggleJob,
            onCancelJob = { job -> actions.run("job:$job", "Cancelling", after = { workers.refresh() }) { api.workers.cancelJob(job) } },
            onRetry = { workers.list = Load.Loading; workers.refresh() },
            onCloseNote = actions::clear,
            onBack = { nav.back() },
        ),
    )
    shown?.let { c -> JoinCodeDialog(c, now, serverUrl(api), onCopy = { label, text -> clip.setText(AnnotatedString(text)); actions.note = Actions.Note("$label copied", false) }) { shown = null } }
}

/** The Jobs of a PC's page: [jobs] null shows no section (not loaded, or a server without jobs). */
data class WorkerJobsView(
    val jobs: List<WorkerJob>? = null,
    val open: String? = null,
    /** The open job with its log tail; null while it loads. */
    val view: WorkerJob? = null,
    val cancelling: Boolean = false,
)

class WorkerEvents(
    val onRename: (String) -> Unit = {},
    val onAllowChats: (Boolean) -> Unit = {},
    val onAllowCommands: (Boolean) -> Unit = {},
    val onPaused: (Boolean) -> Unit = {},
    val onLabels: (List<String>) -> Unit = {},
    val onHistory: () -> Unit = {},
    val onTerminal: () -> Unit = {},
    val onRevoke: () -> Unit = {},
    val onReissue: () -> Unit = {},
    val onRemove: () -> Unit = {},
    val onJob: (String) -> Unit = {},
    val onCancelJob: (String) -> Unit = {},
    val onRetry: () -> Unit = {},
    val onCloseNote: () -> Unit = {},
    val onBack: () -> Unit = {},
)

@Composable
fun WorkerScreen(
    id: String,
    w: Worker?,
    list: Load<WorkersOverview>,
    now: Long,
    busy: Boolean = false,
    note: Actions.Note? = null,
    history: Load<List<WorkerEvent>>? = null,
    jobs: WorkerJobsView = WorkerJobsView(),
    events: WorkerEvents = WorkerEvents(),
) {
    var rename by remember { mutableStateOf(false) }
    var revoke by remember { mutableStateOf(false) }
    var remove by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxSize()) {
        TopBar(
            w?.title ?: "Worker", onBack = events.onBack,
            subtitle = w?.let { listOf(workerState(it).first, it.kind, it.version.ifEmpty { null }?.let { v -> "v$v" }).filter { s -> !s.isNullOrEmpty() }.joinToString(" · ") },
            actions = {
                if (w != null) {
                    CrundiIconButton("pencil", { rename = true }, contentDescription = "Rename", enabled = !busy)
                    if (w.online && !w.revoked) CrundiIconButton("terminal", events.onTerminal, contentDescription = "Open terminal")
                }
            },
        )
        NoteBanner(note, events.onCloseNote)
        when {
            list is Load.Loading -> { LoadingState(text = "Loading worker"); return@Column }
            list is Load.Failed -> { ErrorState(list.message, onRetry = events.onRetry); return@Column }
            w == null -> { EmptyState("server", "No such worker", body = "It may have been removed ($id)."); return@Column }
        }
        if (w == null) return@Column
        val i = w.info
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(vertical = 8.dp)) {
            Group("Status") {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    OnlineDot(w.online)
                    val (word, kind) = workerState(w)
                    StatusBadge(word, kind)
                    Text(
                        when {
                            w.online && w.connectedMs > 0 -> "connected for ${Fmt.span(now - w.connectedMs)}"
                            w.online -> "connected"
                            w.lastSeenMs > 0 -> "last seen ${Fmt.ago(w.lastSeenMs, now)}"
                            else -> "never connected"
                        },
                        style = CrundiType.Small, modifier = Modifier.weight(1f),
                    )
                }
                if (w.paused && !w.pausedBy.isNullOrEmpty()) Text(if (w.pausedBy == "pc") "Paused from the PC itself." else "Paused by ${w.pausedBy}.", style = CrundiType.Small.copy(color = CrundiColors.Yellow))
                if (w.revoked) Text("Its key was revoked: it cannot connect until it joins again with a new code.", style = CrundiType.Small.copy(color = CrundiColors.Red))
                Column {
                    if (w.kind.isNotEmpty()) KeyValue("Kind", w.kind)
                    if (w.version.isNotEmpty()) KeyValue("Version", w.version)
                    if (i != null) {
                        val os = listOf(i.os, i.osVersion, i.arch).filter { it.isNotEmpty() }.joinToString(" ")
                        if (os.isNotEmpty()) KeyValue("System", os)
                        if (i.hostname.isNotEmpty()) KeyValue("Host name", i.hostname)
                        i.cpu?.let { c -> KeyValue("CPU", listOfNotNull(c.model.ifEmpty { null }, c.cores.takeIf { it > 0 }?.let { "$it cores" }, c.threads.takeIf { it > 0 }?.let { "$it threads" }).joinToString(" · ")) }
                        if (i.ramMb > 0) KeyValue("Memory", "${i.ramGb} GB" + if (i.ramFreeMb > 0) " · ${mb(i.ramFreeMb)} free" else "")
                        i.load?.takeIf { it.isNotEmpty() }?.let { l -> KeyValue("Load", l.joinToString(" ") { "%.2f".format(it) }) }
                        if (i.gpus.isNotEmpty()) KeyValue("GPU", i.gpus.joinToString(", ") { g -> g.name.ifEmpty { g.vendor } + (g.vramMb?.takeIf { it > 0 }?.let { " (${mb(it)})" } ?: "") })
                        i.disk?.let { d -> KeyValue("Free disk", mb(d.freeMb) + if (d.totalMb > 0) " of ${mb(d.totalMb)}" else "") }
                        (i.workDir.ifEmpty { i.disk?.path.orEmpty() }).takeIf { it.isNotEmpty() }?.let { KeyValue("Work folder", it) }
                        i.onBattery?.let { KeyValue("Power", if (it) "on battery" else "plugged in") }
                        i.idleSec?.let { KeyValue("Idle", Fmt.span(it * 1000)) }
                    }
                    if (w.createdMs > 0) KeyValue("Joined", Fmt.date(w.createdMs))
                }
            }
            Group("Allowed") {
                CrundiSwitchRow("Allow chats", w.allowChats, events.onAllowChats, detail = "Chats may run on this PC", enabled = !busy)
                CrundiSwitchRow("Allow commands", w.allowCommands, events.onAllowCommands, detail = "Chats may run commands on this PC", enabled = !busy)
                CrundiSwitchRow("Paused", w.paused, events.onPaused, detail = "Holds new work; your own terminal still opens", enabled = !busy)
            }
            WorkerJobsGroup(w, jobs, now, events)
            Group("Labels", detail = "Jobs ask for labels. Grey ones come from the PC itself; add your own, separated by commas.") {
                WorkerLabels(w)
                LabelsEditor(w, busy, events.onLabels)
            }
            val tools = i?.toolVersions.orEmpty()
            Group("Tools found") {
                if (tools.isEmpty()) Text("None reported yet.", style = CrundiType.Small)
                else Column { for ((name, v) in tools) KeyValue(name, v) }
                i?.claudeLoggedIn?.let { Text(if (it) "Claude is signed in on this PC." else "Claude is not signed in on this PC.", style = CrundiType.Small) }
            }
            val logs = w.logLines.takeLast(50)
            Group("Recent log") {
                if (logs.isEmpty()) Text("Nothing logged yet.", style = CrundiType.Small)
                else SelectionContainer {
                    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        for (l in logs) Text(
                            (if (l.at > 0) Fmt.date(l.at) + "  " else "") + l.text,
                            style = CrundiType.Code.copy(fontSize = 12.sp, lineHeight = 16.sp, color = logColor(l.level)),
                        )
                    }
                }
            }
            Group("History") {
                when (history) {
                    null -> Row { CrundiButton("Show history", events.onHistory, icon = "clock") }
                    Load.Loading -> Text("Loading…", style = CrundiType.Small)
                    is Load.Failed -> { Text(history.message, style = CrundiType.Small.copy(color = CrundiColors.Red)); Row { CrundiButton("Try again", events.onHistory, icon = "refresh") } }
                    is Load.Ready -> {
                        if (history.value.isEmpty()) Text("Nothing yet.", style = CrundiType.Small)
                        for ((n, e) in history.value.withIndex()) {
                            if (n > 0) CrundiDivider()
                            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                                Text(Fmt.date(e.at), style = CrundiType.Meta, modifier = Modifier.padding(top = 2.dp))
                                Text(workerEventWords(e), style = CrundiType.Small.copy(color = CrundiColors.TextPrimary), modifier = Modifier.weight(1f))
                            }
                        }
                        Row { CrundiButton("Refresh", events.onHistory, kind = ButtonKind.Ghost, icon = "refresh") }
                    }
                }
            }
            @OptIn(ExperimentalLayoutApi::class)
            FlowRow(
                Modifier.fillMaxWidth().padding(horizontal = Dimens.ScreenPad, vertical = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                if (w.online && !w.revoked) CrundiButton("Open terminal", events.onTerminal, kind = ButtonKind.Primary, icon = "terminal")
                if (w.revoked) CrundiButton("New join code", events.onReissue, icon = "key", enabled = !busy)
                else CrundiButton("Revoke", { revoke = true }, kind = ButtonKind.Danger, icon = "ban", enabled = !busy)
                CrundiButton("Remove", { remove = true }, kind = ButtonKind.Danger, icon = "trash", enabled = !busy)
            }
        }
    }
    if (w != null) {
        if (rename) {
            var name by remember { mutableStateOf(w.title) }
            CrundiDialog("Rename ${w.title}", { rename = false }, confirm = "Rename", onConfirm = { if (name.isNotBlank()) { rename = false; events.onRename(name.trim()) } }) {
                CrundiTextField(name, { name = it.take(60) }, placeholder = "Name")
            }
        }
        if (revoke) CrundiDialog(
            "Revoke ${w.title}?", { revoke = false },
            message = "Its key stops working at once and it disconnects. The worker stays listed as revoked; it can join again with a new code.",
            confirm = "Revoke", danger = true, onConfirm = { revoke = false; events.onRevoke() },
        )
        if (remove) CrundiDialog(
            "Remove ${w.title}?", { remove = false },
            message = "Its key stops working and it is forgotten here, with its history. To use the PC again, it joins with a new code.",
            confirm = "Remove", danger = true, onConfirm = { remove = false; events.onRemove() },
        )
    }
}

@Composable
private fun WorkerJobsGroup(w: Worker, jobs: WorkerJobsView, now: Long, events: WorkerEvents) {
    val list = jobs.jobs ?: return
    val hint = if (w.canRunJobs) "Commands chats ran here (worker_run)" else if (w.online) "This worker is too old to run commands. Update it on the PC." else null
    Group("Jobs", detail = hint) {
        if (list.isEmpty()) Text("None yet. A chat runs one with worker_run, e.g. a release build.", style = CrundiType.Small)
        for ((n, j) in list.take(15).withIndex()) {
            if (n > 0) CrundiDivider()
            val open = jobs.open == j.job
            Column(
                Modifier.fillMaxWidth().clip(Dimens.ShapeSm).clickable(onClickLabel = if (open) "Fold the job" else "Show the job's output") { events.onJob(j.job) }.padding(vertical = 4.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Text(j.command.ifEmpty { j.job }, style = CrundiType.Code.copy(fontSize = 13.sp, lineHeight = 18.sp, color = CrundiColors.TextPrimary), maxLines = if (open) 6 else 2, overflow = TextOverflow.Ellipsis)
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    val (word, kind) = jobChip(j)
                    StatusBadge(word, kind)
                    Text(jobMeta(j, now), style = CrundiType.Meta, modifier = Modifier.weight(1f), maxLines = 2, overflow = TextOverflow.Ellipsis)
                }
            }
            if (open) JobDetail(j, jobs, events)
        }
    }
}

@Composable
private fun JobDetail(j: WorkerJob, jobs: WorkerJobsView, events: WorkerEvents) {
    val v = jobs.view
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        if (j.running) Row { CrundiButton("Cancel", { events.onCancelJob(j.job) }, kind = ButtonKind.Danger, icon = "stop", enabled = !jobs.cancelling) }
        if (v != null && v.error.isNotEmpty()) Text(v.error, style = CrundiType.Small.copy(color = CrundiColors.Red))
        if (v != null && v.notes.isNotEmpty()) Text(v.notes.joinToString("\n"), style = CrundiType.Small)
        val scroll = rememberScrollState()
        val text = if (v == null) "Loading…" else v.tail?.ifEmpty { null } ?: "(no output yet)"
        // Kept at the end as the output grows, as the web's log box.
        LaunchedEffect(text, scroll.maxValue) { scroll.scrollTo(scroll.maxValue) }
        SelectionContainer {
            Box(
                Modifier.fillMaxWidth().heightIn(max = 320.dp).clip(Dimens.ShapeSm).background(CrundiColors.BgPrimary)
                    .border(Dimens.Hairline, CrundiColors.BorderSubtle, Dimens.ShapeSm).verticalScroll(scroll).padding(8.dp),
            ) {
                Text(text, style = CrundiType.Code.copy(fontSize = 11.sp, lineHeight = 15.sp, color = CrundiColors.TextSecondary), modifier = Modifier.horizontalScroll(rememberScrollState()))
            }
        }
    }
}

private fun logColor(level: String): Color = when (level) {
    "error" -> CrundiColors.Red
    "warn" -> CrundiColors.Yellow
    else -> CrundiColors.TextSecondary
}

@Composable
private fun LabelsEditor(w: Worker, busy: Boolean, onSave: (List<String>) -> Unit) {
    val initial = w.customLabels.joinToString(", ")
    var text by remember(initial) { mutableStateOf(initial) }
    var error by remember { mutableStateOf<String?>(null) }
    CrundiTextField(text, { text = it.take(800); error = null }, placeholder = "gpu-box, fast-disk, office", mono = true)
    error?.let { Text(it, style = CrundiType.Small.copy(color = CrundiColors.Red)) }
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        CrundiButton("Save labels", {
            val (labels, why) = parseWorkerLabels(text)
            error = why
            if (labels != null) onSave(labels)
        }, icon = "check", enabled = !busy && text.trim() != initial)
    }
}

// ─── Terminal ───

@Composable
fun WorkerTerminalRoute(id: String) {
    val app = LocalAppServices.current
    val api = app.api
    val nav = LocalNavigator.current
    val shell by app.shell.state.collectAsStateWithLifecycle()
    val workers = rememberWorkers(WorkersApi.supported(shell.capabilities))
    val w = workers.list.valueOrNull?.workers?.firstOrNull { it.id == id }
    var view by remember { mutableStateOf(ShellView()) }
    var tick by remember { mutableStateOf(0) }
    var term by remember { mutableStateOf<mv.obitech.crundi.api.WorkerTerminal?>(null) }
    val scope = rememberCoroutineScope()
    // Open on entering; leaving the page cancels the flow, which closes the terminal.
    LaunchedEffect(tick) {
        val t = api.workerTerminal(id, 80, 24)
        term = t
        view = ShellView(open = true)
        t.events.collect { e ->
            view = when (e) {
                is DockerShellEvent.Data -> view.copy(lines = Ansi.fold(view.lines, e.text))
                is DockerShellEvent.Exited -> view.copy(open = false, ended = "The terminal ended (exit ${e.code}).")
                is DockerShellEvent.Failed -> view.copy(open = false, ended = e.error.ifEmpty { "The terminal could not open" })
            }
        }
    }
    Column(Modifier.fillMaxSize()) {
        TopBar("Terminal", subtitle = w?.title ?: id, onBack = { nav.back() })
        ShellPane(
            "A shell on ${w?.title ?: "the PC"}, in its work folder", view,
            onInput = { s -> scope.launch { term?.input(s) } },
            onRestart = { view = ShellView(); tick++ },
            opening = "Opening a terminal…",
        )
    }
}
