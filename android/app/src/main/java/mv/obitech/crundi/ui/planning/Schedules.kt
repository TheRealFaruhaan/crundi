package mv.obitech.crundi.ui.planning

import mv.obitech.crundi.ui.components.errorText

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.launch
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.model.HookSettings
import mv.obitech.crundi.api.model.KanbanBoard
import mv.obitech.crundi.api.model.Schedule
import mv.obitech.crundi.api.model.ScheduleAction
import mv.obitech.crundi.api.model.ScheduleCondition
import mv.obitech.crundi.api.model.ScheduleServiceChoice
import mv.obitech.crundi.api.model.ScheduleWhen
import mv.obitech.crundi.api.model.Schedules
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.Banner
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiChip
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiDivider
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.TopBar
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

object ScheduleRoutes {
    const val EDIT = "schedules/edit?id={id}"
    fun edit(id: String?) = "schedules/edit?id=" + java.net.URLEncoder.encode(id.orEmpty(), "UTF-8")
}

private val DAY_SHORT = listOf("Su", "Mo", "Tu", "We", "Th", "Fr", "Sa")
val SCHED_MODELS = listOf("", "opus", "sonnet", "haiku", "fable", "mythos")
val SCHED_EFFORTS = listOf("", "low", "medium", "high", "xhigh", "max")

/** The desktop's terse "when": "Daily 09:00", "Mo,We,Fr 09:00", "2026-10-25 23:00", "When called". */
fun whenSummary(w: ScheduleWhen?): String {
    if (w?.mode == "webhook") return "When called"
    if (w == null || w.time.isBlank()) return "no time set"
    if (w.mode == "once") return "${w.date.ifBlank { "(no date)" }} ${w.time}"
    if (w.days.isEmpty()) return "no days · ${w.time}"
    if (w.days.toSet().size >= 7) return "Daily ${w.time}"
    return w.days.sorted().distinct().joinToString(",") { DAY_SHORT.getOrElse(it) { "?" } } + " " + w.time
}

/** The desktop's one line on what a schedule does. */
fun actionSummary(a: ScheduleAction, services: List<ScheduleServiceChoice> = emptyList()): String = when (a.kind) {
    "agent" -> listOfNotNull(
        "Agent", "Claude", if (a.mode == "skip") "skip-perms" else "normal", a.model.ifEmpty { null }, a.effort.ifEmpty { null },
        when (a.session) { "continue" -> "continue"; "resume" -> "resume ${a.sessionId.take(8)}"; else -> "new session" },
    ).joinToString(" · ")
    "chat" -> listOfNotNull("Background chat", a.model.ifEmpty { null }, a.effort.ifEmpty { null }, if (a.session == "resume") "one ongoing chat" else "fresh each run").joinToString(" · ")
    "command" -> "Run: ${a.command}"
    "service" -> "${a.op} service · ${services.firstOrNull { it.key == a.serviceKey }?.name ?: a.serviceKey.substringAfter(':')}"
    else -> a.kind
}

fun conditionSummary(c: ScheduleCondition, board: KanbanBoard? = null): String = when (c.type) {
    "usage" -> "${if (c.metric == "week") "weekly" else "5h"} usage ${c.op} ${c.value.toInt()}%"
    "terminals" -> "open terminals ${c.op} ${c.value.toInt()}"
    "kanban" -> "kanban ${if (c.todoId.isNotEmpty()) "subtask" else "task"} = ${c.status}"
    else -> c.type
}

// ═══ List ═══

@Stable
class SchedulesCallbacks(
    val onAdd: () -> Unit = {},
    val onEdit: (Schedule) -> Unit = {},
    val onToggle: (Schedule) -> Unit = {},
    val onDelete: (Schedule) -> Unit = {},
    val onRetry: () -> Unit = {},
)

@Composable
fun SchedulesRoute() {
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val project = currentProject()
    var thisProject by remember { mutableStateOf(true) }
    val filter = if (thisProject) project else null
    val loader = rememberLoader(filter, events = arrayOf("schedule")) { api.schedules.list(filter) }
    val actions = rememberActions { loader.refresh() }
    val cb = remember {
        SchedulesCallbacks(
            onAdd = { nav.to(ScheduleRoutes.edit(null)) },
            onEdit = { nav.to(ScheduleRoutes.edit(it.id)) },
            onToggle = { s -> loader.update { l -> l.copy(schedules = l.schedules.map { if (it.id == s.id) it.copy(enabled = !s.enabled) else it }) }; actions.run { api.schedules.setEnabled(s.id, !s.enabled) } },
            onDelete = { s -> actions.run { api.schedules.delete(s.id) } },
            onRetry = { loader.refresh() },
        )
    }
    Box(Modifier.fillMaxSize()) {
        SchedulesScreen(loader.load, project, thisProject, { thisProject = it }, cb)
        ActionError(actions)
    }
}

@Composable
fun SchedulesScreen(list: Load<Schedules>, project: String?, thisProject: Boolean, onThisProject: (Boolean) -> Unit, cb: SchedulesCallbacks = SchedulesCallbacks()) {
    var deleting by remember { mutableStateOf<Schedule?>(null) }
    Column(Modifier.fillMaxSize()) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            CrundiButton("Add schedule", cb.onAdd, kind = ButtonKind.Primary, icon = "plus")
            Spacer(Modifier.weight(1f))
            if (project != null) CrundiChip(if (thisProject) "This project" else "All projects", thisProject, { onThisProject(!thisProject) }, icon = "folder")
        }
        CrundiDivider()
        when (list) {
            is Load.Loading -> LoadingState(text = "Loading schedules")
            is Load.Failed -> ErrorState(list.message, onRetry = cb.onRetry)
            is Load.Ready -> if (list.value.schedules.isEmpty()) EmptyState(
                "clock", if (thisProject && project != null) "No scheduled tasks for this project" else "No scheduled tasks",
                body = "A schedule runs an agent, a background chat, a command or a service at a time you pick.", action = "Add schedule", onAction = cb.onAdd,
            ) else LazyColumn(Modifier.fillMaxSize(), contentPadding = androidx.compose.foundation.layout.PaddingValues(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                items(list.value.schedules, key = { it.id }) { s -> ScheduleRow(s, list.value.services, !thisProject || project == null, cb) { deleting = s } }
            }
        }
    }
    deleting?.let { s ->
        CrundiDialog("Delete schedule", { deleting = null }, message = "Delete schedule “${s.name}”?", confirm = "Delete schedule", danger = true, onConfirm = { deleting = null; cb.onDelete(s) })
    }
}

/** The desktop's small toggle: 30 by 18, accent when on. */
@Composable
fun MiniToggle(on: Boolean, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val shape = RoundedCornerShape(10.dp)
    Box(
        modifier.size(44.dp).clickable(onClick = onClick), contentAlignment = Alignment.Center,
    ) {
        Box(
            Modifier.size(width = 30.dp, height = 18.dp).clip(shape).background(if (on) CrundiColors.AccentDim else CrundiColors.BgTertiary)
                .border(Dimens.Hairline, if (on) CrundiColors.Accent else CrundiColors.Border, shape).padding(2.dp),
            contentAlignment = if (on) Alignment.CenterEnd else Alignment.CenterStart,
        ) { Box(Modifier.size(12.dp).clip(CircleShape).background(if (on) CrundiColors.Accent else CrundiColors.TextMuted)) }
    }
}

@Composable
private fun ScheduleRow(s: Schedule, services: List<ScheduleServiceChoice>, showProject: Boolean, cb: SchedulesCallbacks, onDelete: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgCard).border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm)
            .clickable { cb.onEdit(s) }.padding(start = 2.dp, end = 2.dp, top = 6.dp, bottom = 8.dp),
        verticalAlignment = Alignment.Top,
    ) {
        MiniToggle(s.enabled, { cb.onToggle(s) })
        Column(Modifier.weight(1f).alpha(if (s.enabled) 1f else 0.55f).padding(top = 10.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(s.name, style = CrundiType.Body.copy(fontSize = 14.5.sp, fontWeight = FontWeight.SemiBold), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
                if (showProject && s.project.isNotEmpty()) Text(s.project, style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, color = CrundiColors.AccentHover), modifier = Modifier.clip(RoundedCornerShape(8.dp)).background(CrundiColors.AccentDim).padding(horizontal = 6.dp, vertical = 1.dp))
            }
            val sum = actionSummary(s.action, services)
            if (sum.isNotEmpty()) Text(sum, style = CrundiType.Small, maxLines = 2, overflow = TextOverflow.Ellipsis)
            if (s.action.kind in setOf("agent", "chat") && s.action.prompt.isNotBlank()) Text(s.action.prompt, style = CrundiType.Small.copy(color = CrundiColors.TextMuted, fontFamily = CrundiType.Mono, fontSize = 12.sp), maxLines = 1, overflow = TextOverflow.Ellipsis)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(5.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                SmallChip(whenSummary(s.`when`), if (s.calledNotTimed) "link" else "clock")
                for (c in s.conditions) SmallChip(conditionSummary(c))
            }
            parseInstant(s.lastRun)?.let { Text("last run ${shortDateTime(it.toEpochMilli())}", style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontSize = 10.5.sp)) }
        }
        CrundiIconButton("trash", onDelete, contentDescription = "Delete", size = 16.dp)
    }
}

@Composable
private fun SmallChip(t: String, icon: String? = null) {
    Row(
        Modifier.clip(Dimens.ShapeSm).background(CrundiColors.BgTertiary).border(Dimens.Hairline, CrundiColors.BorderSubtle, Dimens.ShapeSm).padding(horizontal = 7.dp, vertical = 2.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        if (icon != null) mv.obitech.crundi.ui.icons.CrundiIcon(icon, size = 11.dp, tint = CrundiColors.TextSecondary)
        Text(t, style = CrundiType.Meta.copy(color = CrundiColors.TextSecondary, fontSize = 11.sp))
    }
}

// ═══ Editor ═══

/** The schedule being edited, field by field (the desktop's modal). */
@Stable
class ScheduleForm(s: Schedule) {
    val id = s.id
    var name by mutableStateOf(s.name)
    var project by mutableStateOf(s.project)
    var enabled by mutableStateOf(s.enabled)
    var whenMode by mutableStateOf(s.`when`.mode)
    var time by mutableStateOf(s.`when`.time)
    var date by mutableStateOf(s.`when`.date)
    val days = mutableStateListOf(*s.`when`.days.toTypedArray())
    var kind by mutableStateOf(s.action.kind)
    var model by mutableStateOf(s.action.model)
    var effort by mutableStateOf(s.action.effort)
    var mode by mutableStateOf(s.action.mode.ifEmpty { "normal" }.let { if (it == "skip") "skip" else "normal" })
    var session by mutableStateOf(s.action.session.ifEmpty { "new" })
    var sessionId by mutableStateOf(s.action.sessionId)
    var prompt by mutableStateOf(s.action.prompt)
    var cwd by mutableStateOf(s.action.cwd)
    var command by mutableStateOf(s.action.command)
    var serviceKey by mutableStateOf(s.action.serviceKey)
    var op by mutableStateOf(s.action.op.ifEmpty { "start" })
    val conditions = mutableStateListOf(*s.conditions.toTypedArray())
    /** "When called": how calls prove who they are from, the key and secret as the server last said. */
    var hook by mutableStateOf(Webhooks.filled(s.webhook))
    private val base = s

    val calledNotTimed get() = whenMode == "webhook"

    /** Pick One-time, Recurring or When called. A webhook runs a background chat or a service only, so it moves an agent or command job to a chat. */
    fun pickWhen(mode: String) {
        whenMode = mode
        if (mode == "webhook" && kind !in Webhooks.ACTIONS) kind = "chat"
    }

    /** The first thing missing, in the desktop's words, or null. */
    fun problem(services: List<ScheduleServiceChoice>): String? = when {
        name.isBlank() -> "Give the schedule a name"
        project.isBlank() -> "Pick a project"
        calledNotTimed && kind !in Webhooks.ACTIONS -> "A webhook can run a background chat or a service action"
        calledNotTimed -> actionProblem()
        !Regex("^\\d{1,2}:\\d{2}$").matches(time) -> "Set a time"
        whenMode == "once" && !Regex("^\\d{4}-\\d{2}-\\d{2}$").matches(date) -> "Pick a date for the one-time run"
        whenMode == "recurring" && days.isEmpty() -> "Pick at least one day"
        else -> actionProblem()
    }

    private fun actionProblem(): String? = when {
        kind == "agent" && prompt.isBlank() -> "Agent needs a prompt"
        kind == "chat" && prompt.isBlank() -> "The chat needs a prompt"
        kind == "command" && command.isBlank() -> "Enter a command"
        kind == "service" && serviceKey.isBlank() -> "Pick a service"
        else -> null
    }

    /**
     * What a save sends. A timed WHEN only when One-time or Recurring is
     * picked; When called keeps the stored time, date and days; a mode this
     * app does not know goes back as it came. The webhook settings go back
     * whenever the schedule had them or is called, edited where the editor
     * edits them (the server merges them with what it has); an older server
     * that sent none gets none.
     */
    fun toSchedule(tz: String) = base.copy(
        name = name.trim(), project = project, enabled = enabled, tz = tz,
        `when` = when (whenMode) {
            "once", "recurring" -> ScheduleWhen(whenMode, time, if (whenMode == "once") date else "", if (whenMode == "recurring") days.sorted() else emptyList())
            "webhook" -> ScheduleWhen("webhook", time, date, days.sorted())
            else -> base.`when`
        },
        webhook = if (calledNotTimed || base.webhook != null) mergeHook(base.webhook, hook) else null,
        action = ScheduleAction(kind, "claude", mode, model, effort, prompt, if (kind == "chat" && session == "continue") "new" else session, sessionId, cwd, command, serviceKey, op),
        conditions = conditions.toList(),
    )

    companion object {
        /** [edited]'s editable fields over what the server sent, keeping anything else it sent. */
        fun mergeHook(sent: HookSettings?, edited: HookSettings) = (sent ?: HookSettings()).copy(
            auth = edited.auth, signatureHeader = edited.signatureHeader, timestampHeader = edited.timestampHeader, idempotency = edited.idempotency,
        )

        fun blank(project: String?) = Schedule(
            project = project.orEmpty(),
            action = ScheduleAction(kind = "agent", mode = "normal"),
            `when` = ScheduleWhen("recurring", "09:00", "", listOf(0, 1, 2, 3, 4, 5, 6)),
        )
    }
}

@Composable
fun ScheduleEditRoute(id: String) {
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val shell by LocalAppServices.current.shell.state.collectAsStateWithLifecycle()
    val scope = rememberCoroutineScope()
    var form by remember(id) { mutableStateOf<Load<ScheduleForm>>(Load.Loading) }
    var services by remember { mutableStateOf<List<ScheduleServiceChoice>>(emptyList()) }
    var board by remember { mutableStateOf<KanbanBoard?>(null) }
    var message by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    var hooksInData by remember { mutableStateOf(false) }
    var rotating by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(id) {
        var next: Load<ScheduleForm>
        try {
            val all = api.schedules.list(null)
            services = all.services
            hooksInData = Webhooks.inSchedules(all.schedules)
            val s = if (id.isEmpty()) ScheduleForm(ScheduleForm.blank(shell.currentProject)) else all.schedules.firstOrNull { it.id == id }?.let { ScheduleForm(it) }
            next = if (s == null) Load.Failed("This schedule is gone. It may have been deleted elsewhere.") else Load.Ready(s)
        } catch (e: ApiException) { next = Load.Failed(errorText(e)) }
        form = next
    }
    val f = form.valueOrNull
    LaunchedEffect(f?.project) {
        val p = f?.project ?: return@LaunchedEffect
        if (p.isEmpty()) return@LaunchedEffect
        services = runCatching { api.schedules.list(p).services }.getOrDefault(services)
        board = runCatching { api.kanban.board(p) }.getOrNull()
    }
    val tz = java.time.ZoneId.systemDefault().id
    suspend fun save(ff: ScheduleForm): Schedule? {
        ff.problem(services.filter { it.alias.equals(ff.project, true) })?.let { message = it; return null }
        return try {
            if (ff.id.isEmpty()) api.schedules.add(ff.toSchedule(tz)) else api.schedules.update(ff.toSchedule(tz))
        } catch (e: ApiException) { message = "Save failed: ${errorText(e)}"; null }
    }
    ScheduleEditScreen(
        form, shell.projects.map { it.alias }, services, board, message, busy,
        hooks = hooksInData || Webhooks.supported(shell), serverHost = shell.serverHost,
        onBack = { nav.back() },
        onSave = { ff ->
            scope.launch {
                busy = true
                val isNew = ff.id.isEmpty()
                val saved = save(ff)
                // A new webhook schedule stays open, as on the web: its URL only exists now.
                if (saved != null && isNew && saved.calledNotTimed) { form = Load.Ready(ScheduleForm(saved)); message = SAVED_URL_BELOW }
                else if (saved != null) nav.back()
                busy = false
            }
        },
        onRotate = { rotating = it },
        onCopied = { message = it },
        onCreateChat = { ff ->
            scope.launch {
                busy = true
                val saved = save(ff)
                if (saved != null) {
                    try { val s = api.schedules.provisionChat(saved.id); ff.sessionId = s.action.sessionId; message = "Conversation created" } catch (e: ApiException) { message = errorText(e) }
                }
                busy = false
            }
        },
        onDismissMessage = { message = null },
    )
    val which = rotating
    if (which != null && f != null) CrundiDialog(
        if (which == "secret") "New secret" else "New key", { rotating = null }, message = rotateQuestion(which),
        confirm = if (which == "secret") "Make a new secret" else "Make a new key", danger = true,
        onConfirm = {
            rotating = null
            scope.launch {
                try { val s = api.schedules.rotateHook(f.id, which); f.hook = f.hook.copy(key = s.webhook?.key, secret = s.webhook?.secret) }
                catch (e: ApiException) { message = errorText(e) }
            }
        },
    )
}

private const val SAVED_URL_BELOW = "Saved. Its URL is below."
/** The editor's notes that are good news, shown green. */
private val GOOD_NEWS = setOf("Conversation created", SAVED_URL_BELOW, "URL copied", "Secret copied")

@Composable
fun ScheduleEditScreen(
    form: Load<ScheduleForm>,
    projects: List<String>,
    services: List<ScheduleServiceChoice>,
    board: KanbanBoard?,
    message: String? = null,
    busy: Boolean = false,
    /** The server has "When called" (Webhooks.supported, or the data shows it). */
    hooks: Boolean = false,
    serverHost: String = "",
    onBack: () -> Unit = {},
    onSave: (ScheduleForm) -> Unit = {},
    onCreateChat: (ScheduleForm) -> Unit = {},
    /** Replace the webhook's key or secret ("key", "secret"); the caller confirms. */
    onRotate: (String) -> Unit = {},
    onCopied: (String) -> Unit = {},
    onDismissMessage: () -> Unit = {},
) {
    val f = form.valueOrNull
    Column(Modifier.fillMaxSize().imePadding()) {
        TopBar(if (f == null || f.id.isEmpty()) "New schedule" else "Edit schedule", onBack = onBack) {
            if (f != null) CrundiButton(if (busy) "Saving…" else "Save", { onSave(f) }, kind = ButtonKind.Primary, enabled = !busy, modifier = Modifier.padding(end = 8.dp))
        }
        when (form) {
            is Load.Loading -> LoadingState()
            is Load.Failed -> ErrorState(form.message)
            is Load.Ready -> Column(
                Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(Dimens.ScreenPad),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                if (message != null) Box(Modifier.clickable(onClick = onDismissMessage)) {
                    Banner(message, Modifier.fillMaxWidth(), kind = if (message in GOOD_NEWS) BadgeKind.Good else BadgeKind.Bad, icon = if (message in GOOD_NEWS) "check" else "alert-triangle")
                }
                ScheduleFields(form.value, projects, services.filter { it.alias.equals(form.value.project, true) }, board, busy, onCreateChat, hooks, serverHost, onRotate, onCopied)
                Spacer(Modifier.height(40.dp))
            }
        }
    }
}

@Composable
private fun Label(t: String) = Text(t.uppercase(), style = CrundiType.Section.copy(fontSize = 11.5.sp), modifier = Modifier.padding(top = 10.dp))

@Composable
private fun Seg(options: List<Pair<String, String>>, value: String, onPick: (String) -> Unit) {
    Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        for ((k, label) in options) CrundiChip(label, k == value, { onPick(k) })
    }
}

@Composable
private fun Note(t: String) = Text(t, style = CrundiType.Small.copy(fontSize = 12.5.sp, color = CrundiColors.TextMuted))

@Composable
private fun ScheduleFields(
    f: ScheduleForm, projects: List<String>, services: List<ScheduleServiceChoice>, board: KanbanBoard?, busy: Boolean, onCreateChat: (ScheduleForm) -> Unit,
    hooks: Boolean = false, serverHost: String = "", onRotate: (String) -> Unit = {}, onCopied: (String) -> Unit = {},
) {
    Label("Name")
    CrundiTextField(f.name, { f.name = it }, Modifier.fillMaxWidth(), placeholder = "My scheduled task")
    Label("Project")
    if (projects.isEmpty()) Note("No projects") else Seg(projects.map { it to it }, f.project) { f.project = it; f.serviceKey = "" }
    Label("When")
    Seg(listOf("once" to "One-time", "recurring" to "Recurring") + (if (hooks || f.calledNotTimed) listOf("webhook" to "When called") else emptyList()), f.whenMode) { f.pickWhen(it) }
    if (f.calledNotTimed) {
        // Called, not timed: a public URL for alerts (Grafana), CI and the like.
        Note("Runs a background chat (or a service action) when its URL is called. Calls that arrive while it is still working go into the same chat, marked Mid-run, so it can tell whether it is the same issue. With One ongoing chat, every run comes back to the same conversation.")
        HookAuthFields(f.hook, { f.hook = it }) { Label(it) }
        if (f.id.isNotEmpty() && !f.hook.key.isNullOrEmpty()) HookAddress(Webhooks.scheduleUrl(serverHost, f.id, f.hook), f.hook, { Label(it) }, onRotate, onCopied)
        else Note("Save the schedule to get its URL.")
    } else if (f.whenMode == "once") CrundiTextField(f.date, { f.date = it }, Modifier.fillMaxWidth(), placeholder = "YYYY-MM-DD", mono = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number))
    else Row(horizontalArrangement = Arrangement.spacedBy(5.dp)) {
        for (d in 0..6) {
            val on = d in f.days
            Box(
                Modifier.size(width = 40.dp, height = 36.dp).clip(Dimens.ShapeSm).background(if (on) CrundiColors.AccentDim else CrundiColors.BgTertiary)
                    .border(Dimens.Hairline, if (on) CrundiColors.Accent else CrundiColors.Border, Dimens.ShapeSm)
                    .clickable { if (on) f.days.remove(d) else f.days.add(d) },
                contentAlignment = Alignment.Center,
            ) { Text(DAY_SHORT[d], style = CrundiType.Small.copy(fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = if (on) CrundiColors.AccentHover else CrundiColors.TextSecondary)) }
        }
    }
    if (!f.calledNotTimed) CrundiTextField(f.time, { f.time = it }, Modifier.width(120.dp), placeholder = "09:00", mono = true, icon = "clock", keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number))

    Label("Action")
    // A webhook runs a background chat or a service only (src/schedule-hooks.js).
    Seg(listOf("agent" to "Agent", "chat" to "Background chat", "command" to "CLI command", "service" to "Service").filter { !f.calledNotTimed || it.first in Webhooks.ACTIONS }, f.kind) { f.kind = it }
    when (f.kind) {
        "agent", "chat" -> {
            if (f.kind == "chat") Note("Opens a chat, runs the prompt, and closes itself when the work finishes. It stays open if it needs you or hits an error, and it will not close while it is waiting on a background command.")
            else { Label("Agent"); Seg(listOf("claude" to "Claude"), "claude") {} }
            Label("Model"); Seg(SCHED_MODELS.map { it to it.ifEmpty { "Default" } }, f.model) { f.model = it }
            Label("Effort"); Seg(SCHED_EFFORTS.map { it to it.ifEmpty { "Default" } }, f.effort) { f.effort = it }
            Label("Mode"); Seg(listOf("normal" to "Normal", "skip" to "Skip permissions"), f.mode) { f.mode = it }
            if (f.kind == "agent") {
                Label("Session"); Seg(listOf("new" to "New", "continue" to "Continue last", "resume" to "Resume id"), f.session) { f.session = it }
                if (f.session == "resume") CrundiTextField(f.sessionId, { f.sessionId = it }, Modifier.fillMaxWidth(), placeholder = "session id e.g. 8441d374-fd1a-...", mono = true)
            } else {
                Label("Folder (blank = the project root)")
                CrundiTextField(f.cwd, { f.cwd = it }, Modifier.fillMaxWidth(), placeholder = "/absolute/path", mono = true)
                Label("Conversation"); Seg(listOf("new" to "Fresh each run", "resume" to "One ongoing chat"), if (f.session == "resume") "resume" else "new") { f.session = it }
                if (f.session == "resume") when {
                    f.sessionId.isNotEmpty() -> Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Note("Resuming ${f.sessionId.take(8)}…"); CrundiButton("Start a new one", { f.sessionId = "" }, kind = ButtonKind.Ghost)
                    }
                    f.id.isEmpty() -> Note("Save the schedule first, then create the conversation.")
                    else -> Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Note("No conversation yet.")
                        CrundiButton(if (busy) "Creating…" else "Create it now", { onCreateChat(f) }, enabled = !busy)
                    }
                }
            }
            Label("Prompt")
            CrundiTextField(f.prompt, { f.prompt = it }, Modifier.fillMaxWidth(), placeholder = when { f.kind == "agent" -> "What should the agent do?"; f.calledNotTimed -> "Look into this alert: {{payload.message}}"; else -> "What should it do each time?" }, mono = true, singleLine = false, minLines = 3)
            if (f.kind == "chat" && f.calledNotTimed) Note("Read the call with ${Webhooks.PLACEHOLDERS}. Anything a call does not have comes out empty and never stops it, and the chat always gets the full request as well.")
            Note(
                if (f.kind == "agent") "Tip: you can include slash commands like /frontend-design or /loop right in the prompt. They run just like typing them in the terminal."
                else "The job can rewrite this prompt for its next run with the schedule_update tool, so it can leave notes for itself.",
            )
        }
        "command" -> { Label("Command"); CrundiTextField(f.command, { f.command = it }, Modifier.fillMaxWidth(), placeholder = "e.g. npm run build", mono = true) }
        "service" -> {
            Label("Service")
            if (services.isEmpty()) Note("No services for this project") else {
                LaunchedEffect(services) { if (services.none { it.key == f.serviceKey }) f.serviceKey = services.first().key }
                Seg(services.map { it.key to it.name }, f.serviceKey) { f.serviceKey = it }
            }
            Label("Operation"); Seg(listOf("start" to "Start if stopped", "stop" to "Stop if running", "toggle" to "Toggle"), f.op) { f.op = it }
        }
    }

    Label("Extra conditions (optional, all must be true)")
    f.conditions.forEachIndexed { i, c -> ConditionRow(c, board, { f.conditions[i] = it }) { f.conditions.removeAt(i) } }
    FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        CrundiButton("5h usage", { f.conditions.add(ScheduleCondition("usage", "5h", "below", 50.0)) }, kind = ButtonKind.Ghost, icon = "plus")
        CrundiButton("Weekly usage", { f.conditions.add(ScheduleCondition("usage", "week", "below", 50.0)) }, kind = ButtonKind.Ghost, icon = "plus")
        CrundiButton("Open terminals", { f.conditions.add(ScheduleCondition("terminals", op = "below", value = 1.0)) }, kind = ButtonKind.Ghost, icon = "plus")
        CrundiButton("Kanban status", { f.conditions.add(ScheduleCondition("kanban", status = "done")) }, kind = ButtonKind.Ghost, icon = "plus")
    }
    CrundiSwitchRowCompact("Enabled", f.enabled) { f.enabled = it }
}

@Composable
private fun CrundiSwitchRowCompact(label: String, on: Boolean, onChange: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth().padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(label, style = CrundiType.Body, modifier = Modifier.weight(1f))
        MiniToggle(on, { onChange(!on) })
    }
}

@Composable
private fun ConditionRow(c: ScheduleCondition, board: KanbanBoard?, onChange: (ScheduleCondition) -> Unit, onRemove: () -> Unit) {
    Column(
        Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgTertiary).padding(start = 10.dp, top = 4.dp, bottom = 8.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                when (c.type) { "usage" -> if (c.metric == "week") "Weekly usage" else "5h usage"; "terminals" -> "Open terminals"; else -> "Kanban status" },
                style = CrundiType.Small.copy(color = CrundiColors.TextPrimary, fontWeight = FontWeight.Medium), modifier = Modifier.weight(1f),
            )
            CrundiIconButton("x", onRemove, contentDescription = "Remove condition", size = 14.dp, touch = 36.dp)
        }
        if (c.type == "usage" || c.type == "terminals") Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp), modifier = Modifier.padding(end = 10.dp)) {
            for (op in listOf("below", "above", "equal")) CrundiChip(op, c.op == op, { onChange(c.copy(op = op)) })
            CrundiTextField(
                c.value.toInt().toString(), { v -> v.filter { it.isDigit() }.take(3).toIntOrNull()?.let { onChange(c.copy(value = it.toDouble())) } },
                Modifier.width(72.dp), mono = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
            )
            if (c.type == "usage") Text("%", style = CrundiType.Small)
        } else {
            val tasks = board?.tasks?.filter { !it.deleted }.orEmpty()
            val task = tasks.firstOrNull { it.id == c.taskId }
            if (board == null) Note("Loading the board…")
            else Seg(listOf("" to "— pick task —") + tasks.map { it.id to it.title }, c.taskId) { onChange(c.copy(taskId = it, todoId = "")) }
            if (task != null) Seg(listOf("" to "whole task") + task.todos.filter { !it.deleted }.map { it.id to it.text }, c.todoId) { onChange(c.copy(todoId = it, status = if (it.isEmpty()) "done" else "done")) }
            val statuses = if (c.todoId.isNotEmpty()) listOf("done" to "done", "pending" to "pending") else (board?.statuses ?: listOf("backlog", "todo", "in_progress", "done")).map { it to kanbanTitle(it) }
            Seg(statuses, c.status) { onChange(c.copy(status = it)) }
        }
    }
}
