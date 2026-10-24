package mv.obitech.crundi.ui.planning

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay
import mv.obitech.crundi.api.model.FactoryRun
import mv.obitech.crundi.api.model.RunAsk
import mv.obitech.crundi.api.model.RunToken
import mv.obitech.crundi.api.str
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.Banner
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiCard
import mv.obitech.crundi.ui.components.CrundiChip
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiDivider
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.MarkdownText
import mv.obitech.crundi.ui.components.TopBar
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

@Stable
class RunCallbacks(
    val onBack: () -> Unit = {},
    val onPause: () -> Unit = {},
    val onResume: () -> Unit = {},
    val onStop: () -> Unit = {},
    val onRecover: (String) -> Unit = {},
    val onAnswer: (RunAsk, replyId: String, note: String) -> Unit = { _, _, _ -> },
    val onEndWait: (RunToken, outcome: String) -> Unit = { _, _ -> },
    val onManagerMode: (String) -> Unit = {},
    val onTeamMode: (String) -> Unit = {},
    val onFinish: (choice: String, note: String?) -> Unit = { _, _ -> },
    val onOpenChat: (String) -> Unit = {},
    val onDiff: () -> Unit = {},
    val onRetry: () -> Unit = {},
)

enum class RunTab(val label: String) { Run("Run"), Timeline("Timeline") }

@Composable
fun FactoryRunRoute(id: String, onAllRuns: (() -> Unit)? = null) {
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val loader = rememberLoader(id, events = arrayOf("factoryRun"), accept = { it.id == null || it.id == id }) { api.factories.run(id) }
    val actions = rememberActions { loader.refresh() }
    // The desktop polls every 4 s while a run is open; events do most of it here, this is the safety net.
    LaunchedEffect(loader) { while (true) { delay(10_000); if (loader.load.valueOrNull?.active != false) loader.refresh() } }
    val cb = remember(id) {
        RunCallbacks(
            onBack = { onAllRuns?.invoke() ?: nav.back() },
            onPause = { actions.run { api.factories.pause(id) } },
            onResume = { actions.run { api.factories.resume(id) } },
            onStop = { actions.run { api.factories.stop(id) } },
            onRecover = { n -> actions.run { api.factories.recover(id, n.ifBlank { null }) } },
            onAnswer = { a, r, n -> actions.run { api.factories.answerAsk(id, a.tokenId, r, n.ifBlank { null }) } },
            onEndWait = { t, o -> actions.run { api.factories.endWait(id, t.id, o) } },
            onManagerMode = { m -> actions.run { api.factories.setManagerMode(id, m) } },
            onTeamMode = { m -> actions.run { api.factories.setTeamMode(id, m) } },
            onFinish = { c, n -> actions.run { api.factories.finish(id, c, n) } },
            onOpenChat = { nav.chat(it) },
            onDiff = { nav.to(FactoryRoutes.diff(id)) },
            onRetry = { loader.refresh() },
        )
    }
    Box(Modifier.fillMaxSize()) {
        FactoryRunScreen(loader.load, cb)
        ActionError(actions)
    }
}

@Composable
fun FactoryRunScreen(run: Load<FactoryRun>, cb: RunCallbacks = RunCallbacks(), initialSelected: String? = null, initialTab: RunTab = RunTab.Run, now: Long = System.currentTimeMillis()) {
    var selected by remember { mutableStateOf(initialSelected) }
    var tab by remember { mutableStateOf(initialTab) }
    var stopping by remember { mutableStateOf(false) }
    val r = run.valueOrNull
    Column(Modifier.fillMaxSize().imePadding()) {
        val status: @Composable RowScope.() -> Unit = {
            if (r != null) {
                val (label, color) = if (r.awaitingFinish) "Done: your decision" to CrundiColors.Sky else runStatus(r.status, r.waitingOn)
                OutlineChip(label, color, Modifier.padding(end = 4.dp))
            }
        }
        if (inPane()) PaneSubBar("All runs", cb.onBack, status)
        else TopBar(r?.factoryName ?: "Factory run", subtitle = r?.let { it.cr?.title?.ifBlank { null } ?: it.task.ifBlank { it.project } }, onBack = cb.onBack, actions = status)
        when (run) {
            is Load.Loading -> LoadingState(text = "Loading the run")
            is Load.Failed -> ErrorState(run.message, onRetry = cb.onRetry)
            is Load.Ready -> {
                val snap = run.value.snapshot
                Box(Modifier.fillMaxWidth().weight(0.46f)) {
                    if (snap != null) FactoryGraph(snap, run.value, selected, { selected = it; if (it != null) tab = RunTab.Run })
                    else Text("This run has no graph to show.", style = CrundiType.Small, modifier = Modifier.align(Alignment.Center))
                }
                CrundiDivider(strong = true)
                Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 6.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    CrundiChip(if (selected != null) "Step" else "Run", tab == RunTab.Run, { tab = RunTab.Run }, icon = if (selected != null) "user" else "factory")
                    CrundiChip("Timeline", tab == RunTab.Timeline, { tab = RunTab.Timeline }, icon = "list")
                    Spacer(Modifier.weight(1f))
                    RunControls(run.value, cb) { stopping = true }
                }
                CrundiDivider()
                Column(Modifier.fillMaxWidth().weight(0.54f).verticalScroll(rememberScrollState()).padding(Dimens.ScreenPad), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    when {
                        tab == RunTab.Timeline -> Timeline(run.value)
                        selected != null -> StepPane(run.value, selected!!, cb) { selected = null }
                        else -> RunPane(run.value, cb, now)
                    }
                    Spacer(Modifier.height(16.dp))
                }
            }
        }
    }
    if (stopping) CrundiDialog(
        "Stop this factory?", { stopping = false }, message = "Its team stops; the Manager's chat stays as an ordinary chat. The worktree is kept (see Cleanup).",
        confirm = "Stop", danger = true, onConfirm = { stopping = false; cb.onStop() },
    )
}

@Composable
private fun RunControls(r: FactoryRun, cb: RunCallbacks, onStop: () -> Unit) {
    val live = r.status in setOf("starting", "running", "waiting", "paused", "finishing")
    if (r.status == "paused" || r.waitingOn == "paused") CrundiButton("Resume", cb.onResume, icon = "play")
    else if (r.status in setOf("running", "waiting")) CrundiButton("Pause", cb.onPause, icon = "pause")
    if (live) CrundiButton("Stop", onStop, kind = ButtonKind.Ghost, icon = "stop")
}

@Composable
private fun KV(k: String, v: String) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.Top) {
        Text(k, style = CrundiType.Small, modifier = Modifier.width(96.dp))
        Text(v, style = CrundiType.Small.copy(color = CrundiColors.TextPrimary), modifier = Modifier.weight(1f))
    }
}

private fun money(usd: Double) = "$" + "%.2f".format(usd)

private fun duration(ms: Long): String {
    val m = ms / 60_000
    return when { m < 1 -> "under a minute"; m < 60 -> "$m min"; else -> "${m / 60}h ${m % 60}m" }
}

/** The run's own pane (what needs you, the CR, facts, findings, permissions); also the workbench pane's Details drawer, with its own [hint]. */
@Composable
internal fun ColumnScope.RunPane(r: FactoryRun, cb: RunCallbacks, now: Long, hint: String = "Tap a step to see what happened there and open that person's chat.") {
    // What needs the person first.
    if (r.status == "crashed") CrashCard(r, cb)
    if (r.capHit != null) Panel("It reached its limit", CrundiColors.Yellow) {
        Text("It reached its limit (${r.capHit.str("reason") ?: r.capHit.str("kind") ?: "time or handoffs"}).", style = CrundiType.Small.copy(color = CrundiColors.TextPrimary))
        CrundiButton("Let it carry on", cb.onResume, kind = ButtonKind.Primary)
    }
    for (a in r.asks) AskCard(a, cb)
    for (t in r.tokens.filter { it.wait != null }) WaitCard(t, cb, now)
    if (r.awaitingFinish || r.status == "finishing") FinishCard(r, cb)
    r.outcome?.takeIf { !it.how.isNullOrEmpty() }?.let { OutcomeCard(r, cb) }
    for (p in r.permissions.filter { !it.sent }) Banner("${p.employee.ifEmpty { "Someone" }} asks to use ${p.toolName}. It went to the Manager of this factory.", Modifier.fillMaxWidth(), kind = BadgeKind.Warn, icon = "shield-alert")

    r.cr?.let { cr -> CrCard(cr) }
    val live = r.employees.values.count { it.busy != null }
    KV("Task", r.cr?.title?.ifBlank { null } ?: r.task.ifBlank { "-" })
    KV("Project", r.project)
    KV("Handoffs", r.handoffs.toString())
    KV("Branch", listOf(r.branch, r.parentBranch.takeIf { it.isNotEmpty() }?.let { "from $it" }).filterNotNull().joinToString(" "))
    KV("Sessions", "$live live")
    KV("Spent", "${money(r.usage.costUsd)} · ${r.usage.turns} turns" + if (r.usage.chats > 0) " in ${r.usage.chats} chats" else "")
    if (r.createdAtMs > 0) KV("Running for", duration(now - r.createdAtMs))
    if (r.managerChatId.isNotEmpty()) {
        Text("The Manager is talking to you in its chat.", style = CrundiType.Small)
        CrundiButton("Open the Manager's chat", { cb.onOpenChat(r.managerChatId) }, icon = "crown")
    }
    if (r.findings.isNotEmpty()) Findings(r)
    if (r.active) {
        CrundiDivider()
        Text("Permissions", style = CrundiType.Heading)
        Text("The Manager's own actions", style = CrundiType.Small)
        ChipRow(listOf("normal" to "Ask me", "bypass" to "I trust the Manager"), r.managerMode.ifEmpty { "normal" }, onPick = cb.onManagerMode)
        Text("The team's actions", style = CrundiType.Small)
        ChipRow(listOf("ask" to "Ask the Manager", "bypass" to "Bypass too"), r.teamMode.ifEmpty { "ask" }, onPick = cb.onTeamMode)
    }
    Text(hint, style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans))
}

@Composable
private fun Panel(title: String, accent: androidx.compose.ui.graphics.Color, content: @Composable ColumnScope.() -> Unit) {
    Column(
        Modifier.fillMaxWidth().clip(Dimens.Shape).background(CrundiColors.BgCard).border(Dimens.Hairline, accent.copy(alpha = 0.5f), Dimens.Shape).padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(title, style = CrundiType.Heading.copy(color = accent))
        content()
    }
}

@Composable
private fun CrashCard(r: FactoryRun, cb: RunCallbacks) {
    var note by remember { mutableStateOf("") }
    Panel("It stopped", CrundiColors.Red) {
        Text("It stopped: ${r.crash?.str("reason") ?: r.crash?.str("message") ?: r.error.ifEmpty { "something went wrong" }}", style = CrundiType.Small.copy(color = CrundiColors.TextPrimary))
        CrundiTextField(note, { note = it }, Modifier.fillMaxWidth(), placeholder = "Anything the team should know (optional)", singleLine = false, minLines = 2)
        CrundiButton("Recover", { cb.onRecover(note) }, kind = ButtonKind.Primary, icon = "refresh")
    }
}

@Composable
private fun AskCard(a: RunAsk, cb: RunCallbacks) {
    var note by remember(a.tokenId) { mutableStateOf("") }
    Panel("A question for you", CrundiColors.Ask) {
        MarkdownText(a.prompt, compact = true)
        CrundiTextField(note, { note = it }, Modifier.fillMaxWidth(), placeholder = "Optional note", singleLine = false, minLines = 2)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            a.replies.forEachIndexed { i, rep -> CrundiButton(rep.label, { cb.onAnswer(a, rep.id, note) }, kind = if (i == 0) ButtonKind.Primary else ButtonKind.Secondary) }
        }
    }
}

@Composable
private fun WaitCard(t: RunToken, cb: RunCallbacks, now: Long) {
    val w = t.wait ?: return
    val deadline = jsonMs(w.deadline); val timeout = jsonMs(w.timeoutAt)
    val head = when (w.kind) {
        "command" -> "trying (${w.tries} so far)"
        "signal" -> w.what.ifEmpty { "waiting for a signal" }
        else -> if (deadline > now) "${(deadline - now + 59_999) / 60_000} min left" else "any moment now"
    }
    Panel("${t.label.ifEmpty { "Wait" }}: $head", CrundiColors.WaitLight) {
        if (w.kind in setOf("command", "signal") && timeout > now) Text("Gives up in ${duration(timeout - now)}.", style = CrundiType.Small)
        if (w.last.isNotEmpty()) Text("Last: ${w.last}", style = CrundiType.Meta)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiButton("End the wait now", { cb.onEndWait(t, "done") }, kind = ButtonKind.Primary)
            if (w.kind in setOf("command", "signal")) CrundiButton("Treat as timed out", { cb.onEndWait(t, "timeout") })
        }
    }
}

@Composable
private fun FinishCard(r: FactoryRun, cb: RunCallbacks) {
    var asking by remember { mutableStateOf(false) }
    var note by remember { mutableStateOf("") }
    var sure by remember { mutableStateOf(false) }
    LaunchedEffect(sure) { if (sure) { delay(3000); sure = false } }
    Panel("Factory result · ${r.factoryName}", CrundiColors.Sky) {
        r.outcome?.summary?.takeIf { it.isNotBlank() }?.let { MarkdownText(it, compact = true) }
        Text("Look it over, then choose what happens to it.", style = CrundiType.Small)
        CrundiButton("Show the full diff", cb.onDiff, kind = ButtonKind.Ghost, icon = "git-commit")
        if (asking) {
            CrundiTextField(note, { note = it }, Modifier.fillMaxWidth(), placeholder = "What should change?", singleLine = false, minLines = 3)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                CrundiButton("Send to the Manager", { cb.onFinish("changes", note); asking = false }, kind = ButtonKind.Primary, enabled = note.isNotBlank())
                CrundiButton("Cancel", { asking = false })
            }
        } else FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiButton("Merge into ${r.parentBranch.ifEmpty { "the branch" }}", { cb.onFinish("merge", null) }, kind = ButtonKind.Primary, icon = "git-merge")
            CrundiButton("Request changes", { asking = true }, icon = "pencil")
            CrundiButton("Keep branch", { cb.onFinish("keep", null) }, icon = "git-branch")
            CrundiButton(if (sure) "Sure? Discard" else "Discard", { if (sure) cb.onFinish("discard", null) else sure = true }, kind = ButtonKind.Danger, icon = "trash")
        }
    }
}

@Composable
private fun OutcomeCard(r: FactoryRun, cb: RunCallbacks) {
    val o = r.outcome ?: return
    val text = when (o.how) {
        "merged", "auto" -> "Merged into ${r.parentBranch} as one commit ${o.commit.take(7)}."
        "undone" -> "Undone. ${r.parentBranch} is back where it was; the work is kept on ${r.branch}."
        "kept" -> "Kept on the branch ${r.branch}. Nothing was merged."
        "discarded" -> "Discarded. The worktree and branch are gone."
        else -> o.how
    }
    Panel("Factory result", CrundiColors.Green) {
        if (o.summary.isNotBlank()) MarkdownText(o.summary, compact = true)
        Text(text, style = CrundiType.Small.copy(color = CrundiColors.TextPrimary))
        if (o.how in setOf("merged", "auto")) CrundiButton("Undo", { cb.onFinish("undo", null) }, icon = "rotate-ccw")
    }
}

@Composable
private fun CrCard(cr: mv.obitech.crundi.api.model.RunCr) {
    var open by remember { mutableStateOf(false) }
    CrundiCard(Modifier.fillMaxWidth()) {
        Row(Modifier.fillMaxWidth().clickable { open = !open }, verticalAlignment = Alignment.CenterVertically) {
            Text("Change request", style = CrundiType.Body.copy(fontWeight = FontWeight.SemiBold), modifier = Modifier.weight(1f))
            CrundiIcon(if (open) "chevron-up" else "chevron-down", size = 16.dp, tint = CrundiColors.TextSecondary)
        }
        if (open) Column(Modifier.padding(top = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(cr.title, style = CrundiType.Body)
            for (s in cr.sections.filter { it.text.isNotBlank() }) {
                Text(s.label, style = CrundiType.Small.copy(fontWeight = FontWeight.SemiBold, color = CrundiColors.TextPrimary))
                MarkdownText(s.text, compact = true)
            }
        }
    }
}

@Composable
private fun Findings(r: FactoryRun) {
    var open by remember { mutableStateOf(true) }
    val openCount = r.findings.count { it.status in setOf("open", "claimed") }
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(Modifier.fillMaxWidth().clickable { open = !open }, verticalAlignment = Alignment.CenterVertically) {
            Text("Review findings: $openCount open of ${r.findings.size}", style = CrundiType.Body.copy(fontWeight = FontWeight.SemiBold), modifier = Modifier.weight(1f))
            CrundiIcon(if (open) "chevron-up" else "chevron-down", size = 16.dp, tint = CrundiColors.TextSecondary)
        }
        if (open) for (f in r.findings) {
            val st = when (f.status) { "claimed" -> "said fixed"; "closed" -> "verified"; else -> f.status }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Box(Modifier.padding(top = 6.dp).size(7.dp).clip(CircleShape).background(when { f.status in setOf("closed", "dismissed") -> CrundiColors.TextMuted; f.blocking -> CrundiColors.Red; else -> CrundiColors.Yellow }))
                Column(Modifier.weight(1f)) {
                    Text(
                        listOfNotNull(f.id, st, if (f.blocking) "blocking" else "nit", if (f.reopened > 0) "reopened ${f.reopened}x" else null, f.by.ifEmpty { null }).joinToString(" · "),
                        style = CrundiType.Meta,
                    )
                    Text(f.text + if (f.where.isNotEmpty()) " (${f.where})" else "", style = CrundiType.Small.copy(color = if (f.status in setOf("closed", "dismissed")) CrundiColors.TextMuted else CrundiColors.TextPrimary))
                }
            }
        }
    }
}

/** One step: who, now, visits and what was said there. [transcript]: the chat opens as a live transcript in the pane, as the web's panel words it. */
@Composable
internal fun ColumnScope.StepPane(r: FactoryRun, id: String, cb: RunCallbacks, transcript: Boolean = false, onClose: () -> Unit) {
    val snap = r.snapshot ?: return
    val n = snap.nodes.firstOrNull { it.id == id } ?: return
    val view = RunView(r)
    val tokens = view.tokens(id)
    val member = snap.roster.firstOrNull { it.id == n.employeeId }
    val emp = n.employeeId?.let { r.employees[it] }
    val visits = r.history.filter { it.nodeId == id }
    Row(verticalAlignment = Alignment.CenterVertically) {
        CrundiIcon(lookOf(n.type).icon, size = 16.dp, tint = lookOf(n.type).head)
        Text(n.label.ifEmpty { lookOf(n.type).kind }, style = CrundiType.Heading, modifier = Modifier.weight(1f).padding(start = 8.dp))
        CrundiButton("Whole run", onClose, kind = ButtonKind.Ghost)
    }
    val now = when {
        tokens.any { it.status == "ask" } -> "waiting for your reply"
        tokens.any { it.status == "join" } -> "waiting for the other branches"
        tokens.any { it.status == "wait" } -> "waiting"
        tokens.any { it.status == "answer" } -> "deciding"
        tokens.any { it.status == "finished" } -> "finished"
        tokens.isNotEmpty() -> "working on it"
        visits.isNotEmpty() || view.visited(id) -> "done"
        else -> "not reached yet"
    } + if (emp?.retired == true) " · session stopped for now" else ""
    KV("Who", when (n.type) { "manager" -> "The Manager"; "employee" -> member?.name ?: "Nobody assigned"; else -> lookOf(n.type).kind })
    KV("Now", now)
    KV("Visits", visits.size.toString())
    val chat = if (n.type == "manager") r.managerChatId else emp?.chatId.orEmpty()
    val what = if (transcript) "transcript" else "chat"
    if (chat.isNotEmpty()) CrundiButton(if (n.type == "manager") "Open the Manager's $what" else "Open their $what", { cb.onOpenChat(chat) }, icon = if (n.type == "manager") "crown" else "message-square")
    for (t in tokens.filter { it.wait != null }) WaitCard(t, cb, System.currentTimeMillis())
    for (a in r.asks.filter { it.nodeId == id }) AskCard(a, cb)
    for (v in visits.reversed()) CrundiCard(Modifier.fillMaxWidth()) {
        Row {
            Text("${v.by.ifEmpty { "Someone" }} · ${if (v.type == "handoff") "finished" else v.type}", style = CrundiType.Small.copy(color = CrundiColors.TextPrimary, fontWeight = FontWeight.Medium), modifier = Modifier.weight(1f))
            Text("#${v.seq}", style = CrundiType.Meta)
        }
        if (v.summary.isNotBlank()) MarkdownText(v.summary, compact = true, modifier = Modifier.padding(top = 4.dp))
        (v.answers as? kotlinx.serialization.json.JsonObject)?.takeIf { it.isNotEmpty() }?.let { a ->
            Text("Answers: " + a.entries.joinToString(", ") { "${it.key}: ${it.value.str() ?: it.value.toString()}" }, style = CrundiType.Meta, modifier = Modifier.padding(top = 4.dp))
        }
    }
    if (visits.isEmpty() && tokens.isEmpty()) Text("Nothing has happened here yet.", style = CrundiType.Small)
}

@Composable
internal fun Timeline(r: FactoryRun) {
    if (r.history.isEmpty()) { Text("Nothing has happened yet.", style = CrundiType.Small); return }
    for (h in r.history.reversed()) {
        val (line, color) = when (h.type) {
            "start" -> "Started" to CrundiColors.TextPrimary
            "handoff" -> "${h.by.ifEmpty { "Someone" }} handed off" to CrundiColors.TextPrimary
            "finish" -> "${h.by.ifEmpty { "The Manager" }} finished" to CrundiColors.Green
            "escalate" -> "${h.by.ifEmpty { "Someone" }} sent it to the Manager" to CrundiColors.Yellow
            "crash" -> "It stopped" to CrundiColors.Yellow
            "merged", "merge" -> "Merged" to CrundiColors.Green
            "kept" -> "Kept on its branch" to CrundiColors.Green
            "discarded" -> "Discarded" to CrundiColors.TextSecondary
            "undone" -> "Merge undone" to CrundiColors.Yellow
            "ask" -> "You answered: ${h.reply}" to CrundiColors.Ask
            else -> (h.type.replaceFirstChar { it.uppercase() } + if (h.by.isNotEmpty()) " · ${h.by}" else "") to CrundiColors.TextPrimary
        }
        Row(Modifier.fillMaxWidth().padding(vertical = 2.dp)) {
            Text("#${h.seq}", style = CrundiType.Meta, modifier = Modifier.width(44.dp))
            Column(Modifier.weight(1f)) {
                Text(line, style = CrundiType.Small.copy(color = color, fontWeight = FontWeight.Medium))
                val sub = listOf(h.summary, h.note, h.detail).firstOrNull { it.isNotBlank() }
                if (sub != null) Text(sub, style = CrundiType.Small.copy(fontSize = 12.5.sp), maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
        }
        CrundiDivider()
    }
}
