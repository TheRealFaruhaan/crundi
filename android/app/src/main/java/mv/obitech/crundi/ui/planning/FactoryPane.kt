package mv.obitech.crundi.ui.planning

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.delay
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import mv.obitech.crundi.api.model.FactoryRun
import mv.obitech.crundi.api.model.FactoryTrigger
import mv.obitech.crundi.api.str
import mv.obitech.crundi.ui.chats.ChatRoute
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiCard
import mv.obitech.crundi.ui.components.CrundiChip
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiDivider
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.StatusBadge
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens
import mv.obitech.crundi.ui.workbench.LocalPane
import mv.obitech.crundi.ui.workbench.LocalWorkbench
import mv.obitech.crundi.ui.workbench.PaneInfo
import mv.obitech.crundi.ui.workbench.PaneKind

/*
 * The workbench's Factory pane, as the web's project Factory panel
 * (app/vendor/crundi-factories.js mountPanel, specs/factory.md 5.2a):
 * - while a run of this project is going, its live graph fills the pane: the
 *   god's-eye view of tools/factory-flow/src/Viewer.jsx in mode 'panel'
 *   (top bar, needs-you banner, working-now strip, a camera that follows the
 *   work, a person's live transcript inside the pane, Details on demand);
 * - with none, what the web panel shows then: what a factory is, Start,
 *   earlier runs and the project's triggers.
 * The Factories tab keeps its own run page (FactoryRunRoute).
 */

/** Run statuses that make the pane the live graph, as the web panel's. */
internal val LIVE_STATUSES = setOf("starting", "running", "waiting", "paused", "crashed", "finishing")

/** A person's chat opened inside the pane. */
data class OpenChat(val chatId: String, val name: String)

/**
 * The words and choices of the live view, worked out from the run's summary
 * the way the web's Viewer does (pure, for tests).
 */
internal object LiveRun {
    private val stopped = setOf("crashed", "paused", "stopped", "finished")

    /** Someone at work, for the working-now strip. */
    data class Worker(val id: String, val nodeId: String, val name: String, val what: String, val state: String, val chatId: String, val blocked: Boolean)

    enum class Tone { Bad, You, Info }
    enum class Act { Recover, OpenResult, CarryOn, Answer, OpenManager, Resume }

    /** What needs the person, said once at the top, with the one action that answers it. */
    data class Needs(val tone: Tone, val text: String, val act: Act?, val label: String)

    /** The first line of [t], at most [n] characters (the web's first()). */
    fun first(t: String?, n: Int): String {
        val line = t.orEmpty().lineSequence().map { it.trim() }.firstOrNull { it.isNotEmpty() }.orEmpty()
        return if (line.length > n) line.take(n - 1) + "…" else line
    }

    /** Everyone at work right now, with what they are doing this second. */
    fun working(r: FactoryRun): List<Worker> {
        if (r.status in stopped) return emptyList()
        return r.tokens.filter { it.status == "work" || it.status == "answer" }.map { t ->
            val e = r.employees[t.employeeId]
            val name = t.employee.ifEmpty { e?.name?.ifEmpty { null } ?: r.snapshot?.roster?.firstOrNull { it.id == t.employeeId }?.name ?: "Someone" }
            val what = when {
                e?.held == true -> "Held by the Manager"
                t.job != null -> "Job: " + first(t.job, 50)
                else -> e?.activity?.text?.ifEmpty { null } ?: if (t.status == "answer") "Deciding" else "Starting"
            }
            Worker(t.id, t.nodeId, name, what, e?.activity?.state.orEmpty(), e?.chatId.orEmpty(), t.blocked)
        }
    }

    private fun num(o: kotlinx.serialization.json.JsonObject?, k: String) = (o?.get(k) as? JsonPrimitive)?.contentOrNull.orEmpty()

    fun needs(r: FactoryRun): Needs? = when {
        r.status == "crashed" -> Needs(Tone.Bad, "It stopped: ${r.crash?.str("reason")?.ifEmpty { null } ?: "it was interrupted"}. Recover picks it up where it was.", Act.Recover, "Recover")
        r.status == "finishing" -> Needs(
            Tone.You, "Done. The result is in the Manager's chat: merge it, ask for changes, keep the branch or discard it.",
            if (r.managerChatId.isNotEmpty()) Act.OpenResult else null, "Open the result",
        )
        r.capHit != null -> {
            val limit = num(r.capHit, "limit")
            Needs(Tone.You, "It reached its limit ($limit ${if (num(r.capHit, "which") == "maxMinutes") "minutes" else "handoffs"}).", Act.CarryOn, "Let it carry on")
        }
        r.asks.isNotEmpty() -> Needs(Tone.You, "A question for you: " + r.asks[0].prompt.replace("**", ""), Act.Answer, "Answer")
        r.waitingOn == "manager" -> Needs(Tone.You, "The Manager is talking to you in its chat.", if (r.managerChatId.isNotEmpty()) Act.OpenManager else null, "Open its chat")
        r.paused != null -> {
            val by = r.paused.str("by")
            val reason = r.paused.str("reason").orEmpty()
            Needs(
                Tone.Info,
                when (by) {
                    "usage" -> "Paused: usage reached the limit set for this run; it carries on by itself after the window resets."
                    "manager" -> "Paused by the Manager" + if (reason.isNotEmpty()) ": $reason" else "."
                    else -> "Paused."
                },
                Act.Resume, "Resume",
            )
        }
        else -> null
    }

    /** The web's dur(): "12 min", "1 h 5 min". */
    fun dur(ms: Long): String {
        val m = Math.round(maxOf(0L, ms) / 60_000.0)
        return if (m >= 60) "${m / 60} h ${m % 60} min" else "$m min"
    }

    /** Handoffs, time, turns and spend, open findings. */
    fun progress(r: FactoryRun, now: Long): String = listOfNotNull(
        "${r.handoffs} handoff${if (r.handoffs == 1) "" else "s"}",
        if (r.createdAtMs > 0) dur(now - r.createdAtMs) else null,
        if (r.usage.turns > 0) "${r.usage.turns} turns" + if (r.usage.costUsd > 0) " · $" + "%.2f".format(r.usage.costUsd) else "" else null,
        if (r.findings.isNotEmpty()) "${r.findings.count { it.status == "open" || it.status == "claimed" }} open findings" else null,
    ).joinToString(" · ")

    /** The state chip's words, the web viewer's stateLabel. */
    fun stateLabel(status: String) = when (status) {
        "running" -> "Running"; "waiting" -> "Waiting on you"; "paused" -> "Paused"; "crashed" -> "Crashed"
        "finishing" -> "Done: your decision"; "finished" -> "Finished"; "stopped" -> "Stopped"; "starting" -> "Starting"
        else -> status
    }

    /**
     * Tapping a step: the chat of whoever is at work there, or of the step's
     * person while their session is live (the Manager's for the Manager), as
     * the web's onNodeClick in panel mode. Null: show the step's history.
     */
    fun chatAt(r: FactoryRun, nodeId: String): OpenChat? {
        val n = r.snapshot?.nodes?.firstOrNull { it.id == nodeId } ?: return null
        RunView(r, live = true).doing(n)?.let { d -> if (d.chatId.isNotEmpty()) return OpenChat(d.chatId, d.who.ifEmpty { "Chat" }) }
        return when (n.type) {
            "manager" -> r.managerChatId.takeIf { it.isNotEmpty() }?.let { OpenChat(it, "Manager") }
            "employee" -> n.employeeId?.let { r.employees[it] }?.takeIf { it.chatId.isNotEmpty() }?.let { e ->
                OpenChat(e.chatId, e.name.ifEmpty { r.snapshot.roster.firstOrNull { it.id == n.employeeId }?.name ?: "Chat" })
            }
            else -> null
        }
    }

    /** Pause while it moves, Resume while paused, Stop while it lives (the web viewer's controls). */
    fun controls(r: FactoryRun): List<String> = listOfNotNull(
        if (r.status in setOf("running", "waiting") && r.paused == null) "pause" else null,
        if (r.paused != null) "resume" else null,
        if (r.status in setOf("running", "waiting", "paused", "crashed", "finishing")) "stop" else null,
    )
}

/** What the live pane has open: a transcript, the Details drawer and its tab, the selected step; and whether the camera follows. */
@Stable
class LivePaneState(chat: OpenChat? = null, drawer: Boolean = false, follow: Boolean = true, selected: String? = null, tab: RunTab = RunTab.Run) {
    var chat by mutableStateOf(chat)
    var drawer by mutableStateOf(drawer)
    var follow by mutableStateOf(follow)
    var selected by mutableStateOf(selected)
    var tab by mutableStateOf(tab)
}

// ═══ The pane ═══

/** The Factory pane's body: the live graph while a run of this project is going, else the web panel's start page. */
@Composable
fun FactoryPaneRoute() {
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val project = LocalWorkbench.current?.project ?: currentProject()
    if (project == null) { EmptyState("factory", "Pick a project", body = "A Factory pane shows the current project's factory. Choose a project from the menu at the top left."); return }
    val runs = rememberLoader(project, events = arrayOf("factoryRun")) { api.factories.runs(project) }
    val triggers = rememberLoader(project, events = arrayOf("factories", "factoryRun")) { api.factories.triggers(project) }
    when (val l = runs.load) {
        is Load.Loading -> LoadingState(text = "Loading the factory")
        is Load.Failed -> ErrorState(l.message, onRetry = { runs.refresh() })
        is Load.Ready -> {
            val active = l.value.sortedByDescending { it.createdAtMs }.firstOrNull { it.status in LIVE_STATUSES }
            if (active != null) key(active.id) { FactoryLiveRoute(active.id, onEnded = { runs.refresh() }) }
            else FactoryPanelScreen(
                l.value, triggers.load,
                FactoryPanelCallbacks(
                    onStart = { nav.to(FactoryRoutes.start(null)) },
                    onOpenRun = { nav.to(FactoryRoutes.run(it.id)) },
                    onOpenTrigger = { nav.to(FactoryRoutes.trigger(it?.id)) },
                ),
            )
        }
    }
}

/** One run's live view in the pane, kept up to date every 3 s while it is on screen (the web viewer's ACTIVITY_MS). */
@Composable
fun FactoryLiveRoute(id: String, onEnded: () -> Unit = {}) {
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val loader = rememberLoader(id, events = arrayOf("factoryRun"), accept = { it.id == null || it.id == id }) { api.factories.run(id) }
    val actions = rememberActions { loader.refresh() }
    val state = remember(id) { LivePaneState() }
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    // Who is doing what changes all the time without changing the run: fetch
    // the summary every few seconds while the pane is on screen.
    LaunchedEffect(loader, lifecycle) { lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) { while (true) { delay(3_000); loader.refresh() } } }
    val ended by rememberUpdatedState(onEnded)
    val status = loader.load.valueOrNull?.status
    LaunchedEffect(status) { if (status != null && status !in LIVE_STATUSES) ended() }
    val cb = remember(id) {
        RunCallbacks(
            onPause = { actions.run { api.factories.pause(id) } },
            onResume = { actions.run { api.factories.resume(id) } },
            onStop = { actions.run { api.factories.stop(id) } },
            onRecover = { n -> actions.run { api.factories.recover(id, n.ifBlank { null }) } },
            onAnswer = { a, r, n -> actions.run { api.factories.answerAsk(id, a.tokenId, r, n.ifBlank { null }) } },
            onEndWait = { t, o -> actions.run { api.factories.endWait(id, t.id, o) } },
            onManagerMode = { m -> actions.run { api.factories.setManagerMode(id, m) } },
            onTeamMode = { m -> actions.run { api.factories.setTeamMode(id, m) } },
            onFinish = { c, n -> actions.run { api.factories.finish(id, c, n) } },
            // A person's chat opens as their live transcript inside the pane.
            onOpenChat = { chatId ->
                val r = loader.load.valueOrNull
                val name = when {
                    r == null -> "Chat"
                    chatId == r.managerChatId -> "Manager"
                    else -> r.employees.values.firstOrNull { it.chatId == chatId }?.name?.ifEmpty { null } ?: "Chat"
                }
                state.chat = OpenChat(chatId, name)
            },
            onDiff = { nav.to(FactoryRoutes.diff(id)) },
            onRetry = { loader.refresh() },
        )
    }
    Box(Modifier.fillMaxSize()) {
        FactoryLiveScreen(loader.load, cb, state, busy = actions.busy)
        ActionError(actions)
    }
}

/**
 * The live graph filling the pane. [transcript] draws a person's chat (the
 * real chat in the app; a still one in screenshots).
 */
@Composable
fun FactoryLiveScreen(
    run: Load<FactoryRun>,
    cb: RunCallbacks = RunCallbacks(),
    state: LivePaneState = remember { LivePaneState() },
    busy: Boolean = false,
    now: Long = System.currentTimeMillis(),
    transcript: @Composable (OpenChat) -> Unit = { PaneTranscript(it.chatId) },
) {
    var stopping by remember { mutableStateOf(false) }
    androidx.activity.compose.BackHandler(enabled = state.chat != null || state.drawer) {
        if (state.chat != null) state.chat = null else state.drawer = false
    }
    BoxWithConstraints(Modifier.fillMaxSize().background(CrundiColors.BgPrimary)) {
        // A narrow pane: the bar goes icon-only and the run's name keeps its room (the web's 560 px).
        val compact = maxWidth < 560.dp
        val drawerWidth = minOf(360.dp, maxWidth * 0.92f)
        when (run) {
            is Load.Loading -> LoadingState(text = "Loading the run")
            is Load.Failed -> ErrorState(run.message, onRetry = cb.onRetry)
            is Load.Ready -> Column(Modifier.fillMaxSize().imePadding()) {
                val r = run.value
                LiveTopBar(r, state, compact, busy, now, cb) { stopping = true }
                LiveRun.needs(r)?.let { n ->
                    NeedsBar(n, enabled = !busy) {
                        when (n.act) {
                            LiveRun.Act.Recover -> cb.onRecover("")
                            LiveRun.Act.CarryOn, LiveRun.Act.Resume -> cb.onResume()
                            LiveRun.Act.OpenResult, LiveRun.Act.OpenManager -> state.chat = OpenChat(r.managerChatId, "Manager")
                            LiveRun.Act.Answer -> { state.selected = null; state.tab = RunTab.Run; state.drawer = true }
                            null -> Unit
                        }
                    }
                }
                WorkingStrip(r) { w -> state.chat = OpenChat(w.chatId, w.name) }
                Box(Modifier.fillMaxWidth().weight(1f)) {
                    val snap = r.snapshot
                    if (snap != null) FactoryGraph(
                        snap, r, state.selected,
                        onSelect = { nodeId ->
                            if (nodeId == null) state.selected = null
                            else {
                                val chat = LiveRun.chatAt(r, nodeId)
                                if (chat != null) state.chat = chat
                                else { state.selected = nodeId; state.tab = RunTab.Run; state.drawer = true }
                            }
                        },
                        live = true, follow = state.follow && state.chat == null, onManualMove = { if (state.follow) state.follow = false },
                    ) else Text("This run has no graph to show.", style = CrundiType.Small, modifier = Modifier.align(Alignment.Center))
                    state.chat?.let { c ->
                        Column(Modifier.fillMaxSize().background(CrundiColors.BgPrimary).blockTouchesBelow()) {
                            Row(Modifier.fillMaxWidth().height(30.dp).padding(horizontal = 12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                CrundiIcon(if (c.chatId == r.managerChatId) "crown" else "user", size = 13.dp, tint = CrundiColors.TextSecondary)
                                Text("Transcript: ${c.name}", style = CrundiType.Small.copy(color = CrundiColors.TextSecondary), maxLines = 1, overflow = TextOverflow.Ellipsis)
                            }
                            CrundiDivider()
                            Box(Modifier.fillMaxWidth().weight(1f)) { key(c.chatId) { transcript(c) } }
                        }
                    }
                    if (state.drawer) DetailsDrawer(r, state, cb, now, Modifier.align(Alignment.CenterEnd).width(drawerWidth))
                }
            }
        }
    }
    if (stopping) CrundiDialog(
        "Stop this factory?", { stopping = false }, message = "Its team stops; the Manager's chat stays as an ordinary chat. The worktree is kept (see Cleanup).",
        confirm = "Stop", danger = true, onConfirm = { stopping = false; cb.onStop() },
    )
}

/** The real chat, as a pane shows it (no header of its own), at the pane's text size once (the pane already scaled it). */
@Composable
private fun PaneTranscript(chatId: String) {
    val p = LocalPane.current
    CompositionLocalProvider(LocalPane provides (p?.copy(fontScale = 1f) ?: PaneInfo("factory-transcript", PaneKind.Factory))) { ChatRoute(chatId) }
}

/** Touches over this layer stop here, not at the graph beneath. */
private fun Modifier.blockTouchesBelow() = this.then(Modifier.pointerInput(Unit) { awaitPointerEventScope { while (true) awaitPointerEvent() } })

@Composable
private fun LiveTopBar(r: FactoryRun, state: LivePaneState, compact: Boolean, busy: Boolean, now: Long, cb: RunCallbacks, onStop: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().height(44.dp).padding(horizontal = if (compact) 8.dp else 10.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(if (compact) 4.dp else 6.dp),
    ) {
        if (state.chat != null) BarButton("Graph", "chevron-left", compact, primary = true, description = "Back to the graph") { state.chat = null }
        Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(
                r.snapshot?.name?.ifEmpty { null } ?: r.factoryName.ifEmpty { "Factory" },
                style = CrundiType.Body.copy(fontWeight = FontWeight.SemiBold), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false),
            )
            OutlineChip(LiveRun.stateLabel(r.status), runStatus(r.status, "").second)
            val progress = LiveRun.progress(r, now)
            if (!compact && progress.isNotEmpty()) Text(progress, style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
        }
        if (state.chat == null) BarButton("Follow", "route", compact, primary = state.follow, description = "Follow: keep the view on where people are working") { state.follow = !state.follow }
        BarButton("Details", "sliders", compact, primary = state.drawer, description = "Details: the run and its timeline") { state.drawer = !state.drawer }
        for (a in LiveRun.controls(r)) when (a) {
            "pause" -> BarButton("Pause", "pause", compact, enabled = !busy, onClick = cb.onPause)
            "resume" -> BarButton("Resume", "play", compact, enabled = !busy, onClick = cb.onResume)
            "stop" -> BarButton("Stop", "stop", compact, ghost = true, enabled = !busy, onClick = onStop)
        }
    }
    CrundiDivider()
}

/** A top-bar button: icon and label, the icon alone on a narrow pane (its words stay for TalkBack). */
@Composable
private fun BarButton(label: String, icon: String, compact: Boolean, primary: Boolean = false, ghost: Boolean = false, enabled: Boolean = true, description: String = label, onClick: () -> Unit) {
    val fg = if (primary) Color.White else CrundiColors.TextPrimary
    Row(
        Modifier.height(32.dp).alpha(if (enabled) 1f else 0.45f).clip(Dimens.ShapeSm)
            .background(when { primary -> CrundiColors.Accent; ghost -> Color.Transparent; else -> CrundiColors.BgTertiary })
            .border(Dimens.Hairline, if (primary || ghost) Color.Transparent else CrundiColors.Border, Dimens.ShapeSm)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .semantics { contentDescription = description }
            .padding(horizontal = if (compact) 8.dp else 10.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        CrundiIcon(icon, size = 14.dp, tint = if (ghost) CrundiColors.TextSecondary else fg)
        if (!compact) Text(label, style = CrundiType.Small.copy(color = if (ghost) CrundiColors.TextSecondary else fg, fontWeight = FontWeight.Medium), maxLines = 1, softWrap = false)
    }
}

@Composable
private fun NeedsBar(n: LiveRun.Needs, enabled: Boolean, onAct: () -> Unit) {
    val (bg, fg) = when (n.tone) {
        LiveRun.Tone.Bad -> CrundiColors.RedDim to CrundiColors.Red
        LiveRun.Tone.You -> CrundiColors.YellowDim to CrundiColors.Yellow
        LiveRun.Tone.Info -> CrundiColors.BgCard to CrundiColors.TextSecondary
    }
    Row(Modifier.fillMaxWidth().background(bg).padding(horizontal = 12.dp, vertical = 7.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        CrundiIcon("alert-triangle", size = 14.dp, tint = fg)
        Text(n.text, style = CrundiType.Small.copy(color = fg), maxLines = 3, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
        if (n.act != null) BarButton(n.label, when (n.act) {
            LiveRun.Act.Recover -> "rotate-ccw"; LiveRun.Act.CarryOn, LiveRun.Act.Resume -> "play"
            LiveRun.Act.Answer -> "help-circle"; LiveRun.Act.OpenResult, LiveRun.Act.OpenManager -> "crown"
        }, compact = false, primary = true, enabled = enabled, onClick = onAct)
    }
    CrundiDivider()
}

/** Everyone at work right now and what they are doing this second; each opens that person's transcript. */
@Composable
private fun WorkingStrip(r: FactoryRun, onOpen: (LiveRun.Worker) -> Unit) {
    val working = LiveRun.working(r)
    val seats = r.waitingForSeat
    if (working.isEmpty() && seats.isEmpty()) return
    val pill = RoundedCornerShape(50)
    Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 10.dp, vertical = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        for (w in working) Row(
            Modifier.widthIn(max = 300.dp).clip(pill).background(CrundiColors.BgCard)
                .border(Dimens.Hairline, if (w.blocked) CrundiColors.Yellow else CrundiColors.Border, pill)
                .alpha(if (w.chatId.isEmpty()) 0.7f else 1f)
                .clickable(enabled = w.chatId.isNotEmpty(), role = Role.Button) { onOpen(w) }
                .semantics { if (w.chatId.isNotEmpty()) contentDescription = "Open ${w.name}'s transcript: ${w.what}" }
                .padding(start = 9.dp, end = 11.dp, top = 5.dp, bottom = 5.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Pulse(w.state == "working", blocked = w.blocked)
            Text(w.name, style = CrundiType.Small.copy(color = CrundiColors.TextPrimary, fontWeight = FontWeight.SemiBold), maxLines = 1, softWrap = false)
            Text(if (w.blocked) "Needs permission" else w.what, style = CrundiType.Small, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
        }
        for (s in seats) Row(
            Modifier.alpha(0.6f).dashedBorder(CrundiColors.Border, 14.dp).padding(start = 9.dp, end = 11.dp, top = 5.dp, bottom = 5.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Pulse(false)
            Text(s.employee.ifEmpty { "Someone" }, style = CrundiType.Small.copy(color = CrundiColors.TextPrimary, fontWeight = FontWeight.SemiBold), maxLines = 1, softWrap = false)
            Text("Waiting its turn", style = CrundiType.Small, maxLines = 1, softWrap = false)
        }
    }
    CrundiDivider()
}

/** Details: the run (or the selected step) and its timeline, over the right of the graph. */
@Composable
private fun DetailsDrawer(r: FactoryRun, state: LivePaneState, cb: RunCallbacks, now: Long, modifier: Modifier) {
    Row(modifier.fillMaxHeight().blockTouchesBelow()) {
        Box(Modifier.width(Dimens.Hairline).fillMaxHeight().background(CrundiColors.Border))
        Column(Modifier.weight(1f).fillMaxHeight().background(CrundiColors.BgSecondary)) {
            Row(Modifier.fillMaxWidth().padding(start = 10.dp, end = 2.dp, top = 4.dp, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                val sel = state.selected
                CrundiChip(if (sel != null) "Step" else "Run", state.tab == RunTab.Run, { state.tab = RunTab.Run }, icon = if (sel != null) "user" else "sliders")
                CrundiChip("Timeline", state.tab == RunTab.Timeline, { state.tab = RunTab.Timeline }, icon = "route")
                Spacer(Modifier.weight(1f))
                CrundiIconButton("x", { state.drawer = false }, contentDescription = "Close the details", touch = 40.dp)
            }
            CrundiDivider()
            Column(Modifier.fillMaxWidth().weight(1f).verticalScroll(rememberScrollState()).padding(12.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                val sel = state.selected
                when {
                    state.tab == RunTab.Timeline -> Timeline(r)
                    sel != null -> StepPane(r, sel, cb, transcript = true) { state.selected = null }
                    else -> RunPane(r, cb, now, hint = "Tap someone at work to read their transcript live; tap a finished step to see what happened there.")
                }
                Spacer(Modifier.height(16.dp))
            }
        }
    }
}

// ═══ No run going: the web panel's start page ═══

@Stable
class FactoryPanelCallbacks(
    val onStart: () -> Unit = {},
    val onOpenRun: (FactoryRun) -> Unit = {},
    val onOpenTrigger: (FactoryTrigger?) -> Unit = {},
)

/** What a factory is and Start, the earlier runs (folded, the last 8) and the project's triggers, as the web panel without a run. */
@Composable
fun FactoryPanelScreen(runs: List<FactoryRun>, triggers: Load<List<FactoryTrigger>>, cb: FactoryPanelCallbacks = FactoryPanelCallbacks(), now: Long = System.currentTimeMillis(), initialRunsOpen: Boolean = false) {
    var runsOpen by remember { mutableStateOf(initialRunsOpen) }
    var triggersOpen by remember { mutableStateOf<Boolean?>(null) }
    val past = runs.sortedByDescending { it.createdAtMs }.take(8)
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text(
            "A factory is a team of Claude employees, run by a Manager, that takes a large piece of work from start to finish in its own worktree. One runs at a time per project.",
            style = CrundiType.Small,
        )
        CrundiButton("Start a factory", cb.onStart, kind = ButtonKind.Primary, icon = "play")
        if (past.isNotEmpty()) {
            Fold("Earlier runs (${past.size})", runsOpen) { runsOpen = !runsOpen }
            if (runsOpen) for (r in past) RunRow(r, now) { cb.onOpenRun(r) }
        }
        val list = triggers.valueOrNull.orEmpty()
        // Open when there are any, as the web's <details open>.
        val tOpen = triggersOpen ?: list.isNotEmpty()
        Fold("Triggers" + (if (list.isNotEmpty()) " (${list.size})" else "") + ": start a run by itself", tOpen) { triggersOpen = !tOpen }
        if (tOpen) {
            when (triggers) {
                is Load.Loading -> Text("Loading triggers", style = CrundiType.Small)
                is Load.Failed -> Text(triggers.message, style = CrundiType.Small.copy(color = CrundiColors.Red))
                is Load.Ready -> if (list.isEmpty()) Text(
                    "Start a factory at a time of day, when a kanban card enters a column, or when a webhook is called (CI, an issue tracker). The change request is filled in from the card or the call.",
                    style = CrundiType.Small,
                )
            }
            for (t in list) CrundiCard(Modifier.fillMaxWidth().alpha(if (t.enabled) 1f else 0.6f), onClick = { cb.onOpenTrigger(t) }) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text(t.name.ifBlank { "Trigger" }, style = CrundiType.Body.copy(fontWeight = FontWeight.SemiBold), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
                    if (!t.enabled) StatusBadge("off")
                }
                Text(triggerWhen(t), style = CrundiType.Small, modifier = Modifier.padding(top = 3.dp))
                t.history.firstOrNull()?.let { h ->
                    Text(
                        listOf(ago(jsonMs(h.at), now), h.outcome).filter { it.isNotEmpty() }.joinToString(": ") + (h.note?.takeIf { it.isNotEmpty() }?.let { " ($it)" } ?: ""),
                        style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans), modifier = Modifier.padding(top = 3.dp),
                    )
                }
            }
            CrundiButton("Add a trigger", { cb.onOpenTrigger(null) }, icon = "plus")
        }
    }
}

/** A folding section heading, as the web's <summary>. */
@Composable
private fun Fold(title: String, open: Boolean, onToggle: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clip(Dimens.ShapeSm).clickable(role = Role.Button, onClick = onToggle).padding(vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        CrundiIcon(if (open) "chevron-down" else "chevron-right", size = 14.dp, tint = CrundiColors.TextSecondary)
        Text(title, style = CrundiType.Small.copy(color = CrundiColors.TextSecondary), modifier = Modifier.weight(1f))
    }
}
