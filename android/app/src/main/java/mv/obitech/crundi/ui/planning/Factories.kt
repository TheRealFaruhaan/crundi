package mv.obitech.crundi.ui.planning

import mv.obitech.crundi.ui.components.errorText

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateMapOf
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
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonPrimitive
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.model.Cleanup
import mv.obitech.crundi.api.model.CleanupItem
import mv.obitech.crundi.api.model.Factory
import mv.obitech.crundi.api.model.FactoryDetail
import mv.obitech.crundi.api.model.FactoryRun
import mv.obitech.crundi.api.model.FactorySummary
import mv.obitech.crundi.api.model.FactoryTemplate
import mv.obitech.crundi.api.model.FactoryTrigger
import mv.obitech.crundi.api.model.HookSettings
import mv.obitech.crundi.api.model.RunPrecheck
import mv.obitech.crundi.api.model.TriggerCr
import mv.obitech.crundi.api.model.TriggerKanban
import mv.obitech.crundi.api.model.TriggerSchedule
import mv.obitech.crundi.api.str
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.Banner
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CodeBlock
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiCard
import mv.obitech.crundi.ui.components.CrundiChip
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiDivider
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.MarkdownText
import mv.obitech.crundi.ui.components.SectionHeader
import mv.obitech.crundi.ui.components.StatusBadge
import mv.obitech.crundi.ui.components.TopBar
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

object FactoryRoutes {
    const val RUN = "factories/run/{id}"
    const val DIFF = "factories/diff/{id}"
    const val FACTORY = "factories/factory/{id}"
    const val EDIT = "factories/edit/{id}?focus={focus}"
    const val START = "factories/start?factory={factory}"
    const val TRIGGER = "factories/trigger?id={id}"
    private fun enc(s: String) = java.net.URLEncoder.encode(s, "UTF-8")
    fun run(id: String) = "factories/run/" + enc(id)
    fun diff(id: String) = "factories/diff/" + enc(id)
    fun factory(id: String) = "factories/factory/" + enc(id)
    /** The editor; [focus] opens it at an issue's place (issueFocus). */
    fun edit(id: String, focus: String? = null) = "factories/edit/" + enc(id) + "?focus=" + enc(focus.orEmpty())
    fun start(factoryId: String?) = "factories/start?factory=" + enc(factoryId.orEmpty())
    fun trigger(id: String?) = "factories/trigger?id=" + enc(id.orEmpty())
}

enum class FactoriesTab(val label: String) { Library("Library"), Runs("Runs"), Triggers("Triggers"), Cleanup("Cleanup") }

/** What the factories front page shows (each tab loads on its own). */
data class FactoriesData(
    val factories: Load<List<FactorySummary>> = Load.Loading,
    val templates: List<FactoryTemplate> = emptyList(),
    val runs: Load<List<FactoryRun>> = Load.Loading,
    val triggers: Load<List<FactoryTrigger>> = Load.Loading,
    val cleanup: Load<Cleanup> = Load.Loading,
)

@Stable
class FactoriesCallbacks(
    val onStart: (String?) -> Unit = {},
    val onOpenFactory: (FactorySummary) -> Unit = {},
    val onCopy: (FactorySummary) -> Unit = {},
    val onDelete: (FactorySummary) -> Unit = {},
    val onUseTemplate: (FactoryTemplate) -> Unit = {},
    val onEdit: (FactorySummary) -> Unit = {},
    /** New factory: blank, from a template or a copy ([NewFactory]). */
    val onCreate: (NewFactory) -> Unit = {},
    val onExport: (FactorySummary) -> Unit = {},
    val onImport: () -> Unit = {},
    val onOpenRun: (FactoryRun) -> Unit = {},
    val onOpenTrigger: (FactoryTrigger?) -> Unit = {},
    val onToggleTrigger: (FactoryTrigger) -> Unit = {},
    val onAutoDelete: (Int) -> Unit = {},
    val onRemoveLeftovers: (List<String>) -> Unit = {},
    val onRetry: () -> Unit = {},
)

/** What New factory makes: [from] is "blank", "template" ([id] a template) or "copy" ([id] a factory). */
data class NewFactory(val name: String, val from: String = "blank", val id: String = "")

// ═══ Front page ═══

@Composable
fun FactoriesRoute(onOpenRunInPane: ((FactoryRun) -> Unit)? = null) {
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val project = currentProject()
    val ctx = androidx.compose.ui.platform.LocalContext.current
    var tab by remember { mutableStateOf(FactoriesTab.Library) }
    var notice by remember { mutableStateOf<String?>(null) }
    var exporting by remember { mutableStateOf<Pair<String, String>?>(null) }
    val lib = rememberLoader(Unit, events = arrayOf("factories")) { api.factories.list() to runCatching { api.factories.templates() }.getOrDefault(emptyList()) }
    val runs = rememberLoader(Unit, events = arrayOf("factoryRun", "factories")) { api.factories.runs() }
    val triggers = rememberLoader(project, events = arrayOf("factories", "factoryRun")) { api.factories.triggers(project) }
    val cleanup = rememberLoader(tab == FactoriesTab.Cleanup) { if (tab == FactoriesTab.Cleanup) api.factories.cleanup() else Cleanup() }
    val actions = rememberActions { lib.refresh(); triggers.refresh(); cleanup.refresh() }
    // Import, as the web's: a factory file (.factory.json) from the phone, saved as a draft if it needs fixing.
    val importer = androidx.activity.compose.rememberLauncherForActivityResult(androidx.activity.result.contract.ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) actions.run {
            val text = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) { ctx.contentResolver.openInputStream(uri)?.use { it.readBytes().decodeToString() } }
                ?: throw ApiException(0, "That file could not be read.")
            val d = api.factories.import(text)
            notice = "Imported “${d.factory.name}”." + if (d.validation.ok) "" else " It is a draft: open it to fix ${d.validation.errors.size} problem${if (d.validation.errors.size == 1) "" else "s"}."
        }
    }
    val saver = androidx.activity.compose.rememberLauncherForActivityResult(androidx.activity.result.contract.ActivityResultContracts.CreateDocument("application/json")) { uri ->
        val text = exporting?.second
        exporting = null
        if (uri != null && text != null) actions.run(refresh = false) {
            kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) { ctx.contentResolver.openOutputStream(uri)?.use { it.write(text.encodeToByteArray()) } }
            notice = "Saved."
        }
    }
    val cb = remember {
        FactoriesCallbacks(
            onStart = { nav.to(FactoryRoutes.start(it)) },
            onOpenFactory = { nav.to(FactoryRoutes.factory(it.id)) },
            onCopy = { f -> actions.run { api.factories.duplicate(f.id) } },
            onDelete = { f -> actions.run { api.factories.delete(f.id) } },
            onUseTemplate = { t -> actions.run { val d = api.factories.fromTemplate(t.id); notice = "“${t.name}” is in your factories now. Change it however you like."; nav.to(FactoryRoutes.edit(d.factory.id)) } },
            onEdit = { nav.to(FactoryRoutes.edit(it.id)) },
            onCreate = { n ->
                actions.run {
                    val name = n.name.trim().ifEmpty { null }
                    val d = when (n.from) {
                        "template" -> api.factories.fromTemplate(n.id, name)
                        "copy" -> api.factories.duplicate(n.id, name)
                        else -> api.factories.create(name ?: "New factory")
                    }
                    nav.to(FactoryRoutes.edit(d.factory.id))
                }
            },
            onExport = { f -> actions.run(refresh = false) { exporting = FactoryFiles.fileName(f.name) to FactoryFiles.text(api.factories.export(f.id)) } },
            onImport = { importer.launch(arrayOf("application/json", "text/plain", "application/octet-stream", "*/*")) },
            onOpenRun = { r -> onOpenRunInPane?.invoke(r) ?: nav.to(FactoryRoutes.run(r.id)) },
            onOpenTrigger = { nav.to(FactoryRoutes.trigger(it?.id)) },
            onToggleTrigger = { t -> actions.run { api.factories.saveTrigger(t.copy(enabled = !t.enabled)) } },
            onAutoDelete = { d -> actions.run(refresh = false) { api.factories.setAutoDelete(d) } },
            onRemoveLeftovers = { ids -> actions.run { api.factories.removeLeftovers(ids) } },
            onRetry = { lib.refresh(); runs.refresh(); triggers.refresh(); cleanup.refresh() },
        )
    }
    val data = FactoriesData(
        factories = when (val l = lib.load) { is Load.Ready -> Load.Ready(l.value.first); is Load.Failed -> l; Load.Loading -> Load.Loading },
        templates = lib.load.valueOrNull?.second.orEmpty(),
        runs = runs.load, triggers = triggers.load, cleanup = cleanup.load,
    )
    Box(Modifier.fillMaxSize()) {
        FactoriesScreen(data, tab, { tab = it }, project, cb, notice = notice, onNotice = { notice = null })
        ActionError(actions)
    }
    exporting?.let { (name, text) ->
        CrundiDialog("Export factory", { exporting = null }, message = "A file with the team and the flow, to import on another Crundi. Its runs and triggers stay here.", dismiss = "Cancel") {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(top = 8.dp)) {
                CrundiButton("Share", { exporting = null; FactoryFiles.share(ctx, name, text) }, icon = "share")
                CrundiButton("Save as a file", { saver.launch(name) }, kind = ButtonKind.Primary, icon = "download")
            }
        }
    }
}

/** A factory as a file, as the web's Export: pretty JSON, named after it. */
object FactoryFiles {
    private val pretty = kotlinx.serialization.json.Json { prettyPrint = true }
    fun text(data: kotlinx.serialization.json.JsonObject): String = pretty.encodeToString(kotlinx.serialization.json.JsonObject.serializer(), data)
    fun fileName(name: String) = name.ifBlank { "factory" }.replace(Regex("[^a-zA-Z0-9-]+"), "-").lowercase() + ".factory.json"
    fun share(ctx: android.content.Context, name: String, text: String) {
        val send = android.content.Intent(android.content.Intent.ACTION_SEND).setType("application/json")
            .putExtra(android.content.Intent.EXTRA_SUBJECT, name).putExtra(android.content.Intent.EXTRA_TEXT, text)
        runCatching { ctx.startActivity(android.content.Intent.createChooser(send, "Export $name").addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)) }
    }
}

@Composable
fun FactoriesScreen(
    data: FactoriesData, tab: FactoriesTab, onTab: (FactoriesTab) -> Unit, project: String?, cb: FactoriesCallbacks = FactoriesCallbacks(), now: Long = System.currentTimeMillis(),
    notice: String? = null, onNotice: () -> Unit = {},
) {
    var deleting by remember { mutableStateOf<FactorySummary?>(null) }
    var creating by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxSize()) {
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 12.dp, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
            for (t in FactoriesTab.entries) {
                val badge = if (t == FactoriesTab.Runs) data.runs.valueOrNull?.count { it.needsYou() }?.takeIf { it > 0 } else null
                CrundiChip(if (badge != null) "${t.label} ($badge)" else t.label, t == tab, { onTab(t) })
            }
        }
        CrundiDivider()
        if (notice != null) Row(Modifier.fillMaxWidth().padding(start = 12.dp, end = 4.dp, top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Banner(notice, Modifier.weight(1f), kind = BadgeKind.Good, icon = "check")
            mv.obitech.crundi.ui.components.CrundiIconButton("x", onNotice, contentDescription = "Close")
        }
        when (tab) {
            FactoriesTab.Library -> LibraryTab(data, cb, now, onNew = { creating = true }) { deleting = it }
            FactoriesTab.Runs -> RunsTab(data.runs, cb, now)
            FactoriesTab.Triggers -> TriggersTab(data.triggers, data.factories.valueOrNull.orEmpty(), project, cb, now)
            FactoriesTab.Cleanup -> CleanupTab(data.cleanup, cb)
        }
    }
    deleting?.let { f ->
        CrundiDialog("Delete factory", { deleting = null }, message = "Delete “${f.name}”? Runs that already started keep their own copy.", confirm = "Delete factory", danger = true, onConfirm = { deleting = null; cb.onDelete(f) })
    }
    if (creating) {
        val form = remember { NewFactoryForm() }
        PlanningSheet("New factory", { creating = false }) { NewFactoryFields(form, data.templates, data.factories.valueOrNull.orEmpty()) { creating = false; cb.onCreate(it) } }
    }
}

/** What the New factory sheet is filling in. */
@Stable
class NewFactoryForm(name: String = "New factory", from: String = "blank", id: String = "") {
    var name by mutableStateOf(name)
    var from by mutableStateOf(from)
    var id by mutableStateOf(id)
}

/** New factory: a name, and where it starts from: blank, a template, or a copy of one of yours. */
@Composable
fun ColumnScope.NewFactoryFields(form: NewFactoryForm, templates: List<FactoryTemplate>, factories: List<FactorySummary>, onCreate: (NewFactory) -> Unit) {
    FieldLabel("Name")
    CrundiTextField(form.name, { form.name = it.take(80) }, Modifier.fillMaxWidth(), placeholder = "New factory")
    FieldLabel("Start from")
    @Composable fun choice(from: String, id: String, title: String, detail: String?) {
        val on = form.from == from && form.id == id
        Row(
            Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgCard).border(if (on) 1.5.dp else Dimens.Hairline, if (on) CrundiColors.Accent else CrundiColors.Border, Dimens.ShapeSm)
                .clickable { form.from = from; form.id = id; if (from != "blank" && (form.name.isBlank() || form.name == "New factory" || templates.any { it.name == form.name } || factories.any { "Copy of " + it.name == form.name })) form.name = if (from == "copy") "Copy of $title" else title }
                .padding(10.dp),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Box(Modifier.padding(top = 2.dp)) { PlanningCheckbox(on, { form.from = from; form.id = id }, 16.dp) }
            Column(Modifier.weight(1f)) {
                Text(title, style = CrundiType.Body.copy(fontWeight = FontWeight.Medium))
                if (!detail.isNullOrEmpty()) Text(detail, style = CrundiType.Small, maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
        }
    }
    choice("blank", "", "Blank", "Start, the Manager and End. Build the team and the flow yourself.")
    if (templates.isNotEmpty()) { Text("Templates", style = CrundiType.Small.copy(color = CrundiColors.TextSecondary)); for (t in templates) choice("template", t.id, t.name, t.goodFor) }
    if (factories.isNotEmpty()) { Text("A copy of", style = CrundiType.Small.copy(color = CrundiColors.TextSecondary)); for (f in factories) choice("copy", f.id, f.name, f.description) }
    CrundiButton("Create and edit", { onCreate(NewFactory(form.name, form.from, form.id)) }, kind = ButtonKind.Primary, icon = "plus", modifier = Modifier.fillMaxWidth())
}

/** The desktop's "needs you": crashed, finishing, waiting, or waiting on something. */
fun FactoryRun.needsYou() = status in setOf("crashed", "finishing", "waiting") || waitingOn.isNotEmpty() || awaitingFinish

@Composable
private fun TabBody(content: @Composable ColumnScope.() -> Unit) =
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(12.dp), verticalArrangement = Arrangement.spacedBy(10.dp), content = content)

@Composable
private fun LibraryTab(data: FactoriesData, cb: FactoriesCallbacks, now: Long, onNew: () -> Unit, onDelete: (FactorySummary) -> Unit) {
    when (val l = data.factories) {
        is Load.Loading -> LoadingState(text = "Loading factories")
        is Load.Failed -> ErrorState(l.message, onRetry = cb.onRetry)
        is Load.Ready -> TabBody {
            Text("Your factories", style = CrundiType.Heading)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                CrundiButton("New factory", onNew, kind = ButtonKind.Primary, icon = "plus")
                CrundiButton("Import", cb.onImport, kind = ButtonKind.Ghost, icon = "upload")
                CrundiButton("Start one", { cb.onStart(null) }, icon = "play", enabled = l.value.any { it.runnable })
            }
            if (l.value.isEmpty()) Text(
                "No factories of your own yet. A factory is a team: a Manager who owns the change request and employees with personas, wired together as a flow of steps and decisions. Start from one of the templates below, build your own with New factory, or ask any Claude chat to build one for you.",
                style = CrundiType.Small,
            )
            for (f in l.value) FactoryCard(f, cb, now, onDelete)
            if (data.templates.isNotEmpty()) {
                Spacer(Modifier.height(6.dp))
                Text("Templates", style = CrundiType.Heading)
                Text("Modelled on how real teams work. Use one as it is, copy it and change it to fit how your team works, or build your own from scratch.", style = CrundiType.Small)
                for (t in data.templates) CrundiCard(Modifier.fillMaxWidth()) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(t.name, style = CrundiType.Body.copy(fontWeight = FontWeight.SemiBold), modifier = Modifier.weight(1f))
                        StatusBadge("Template")
                    }
                    if (t.goodFor.isNotEmpty()) Text(t.goodFor, style = CrundiType.Small.copy(color = CrundiColors.TextPrimary), maxLines = 3, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(top = 4.dp))
                    if (t.description.isNotEmpty()) Text(t.description, style = CrundiType.Small, modifier = Modifier.padding(top = 4.dp))
                    if (t.team.isNotEmpty()) Text("Team: Manager, " + t.team.joinToString(", "), style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans), modifier = Modifier.padding(top = 4.dp))
                    CrundiButton("Use this", { cb.onUseTemplate(t) }, kind = ButtonKind.Primary, icon = "copy", modifier = Modifier.padding(top = 8.dp))
                }
            }
        }
    }
}

@Composable
private fun FactoryCard(f: FactorySummary, cb: FactoriesCallbacks, now: Long, onDelete: (FactorySummary) -> Unit) {
    CrundiCard(Modifier.fillMaxWidth(), onClick = { cb.onOpenFactory(f) }) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(f.name, style = CrundiType.Body.copy(fontWeight = FontWeight.SemiBold), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
            if (f.runnable) StatusBadge("Ready", BadgeKind.Good) else StatusBadge("Draft · ${f.errorCount} to fix", BadgeKind.Warn)
        }
        Text(f.description.ifBlank { "No description." }, style = CrundiType.Small, maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(top = 4.dp))
        Text(
            "${f.employeeCount} people · ${f.nodeCount} steps · edited ${ago(f.updatedAt, now)}",
            style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans), modifier = Modifier.padding(top = 4.dp),
        )
        FlowRow(Modifier.padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            CrundiButton("Edit", { cb.onEdit(f) }, kind = ButtonKind.Primary, icon = "pencil")
            CrundiButton("Start", { cb.onStart(f.id) }, icon = "play", enabled = f.runnable)
            CrundiButton("Copy", { cb.onCopy(f) }, kind = ButtonKind.Ghost, icon = "copy")
            CrundiButton("Export", { cb.onExport(f) }, kind = ButtonKind.Ghost, icon = "download")
            CrundiButton("Delete", { onDelete(f) }, kind = ButtonKind.Danger, icon = "trash")
        }
        if (!f.runnable) Text("Fix it in the editor first.", style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans), modifier = Modifier.padding(top = 4.dp))
    }
}

@Composable
private fun RunsTab(runs: Load<List<FactoryRun>>, cb: FactoriesCallbacks, now: Long) {
    when (runs) {
        is Load.Loading -> LoadingState(text = "Loading runs")
        is Load.Failed -> ErrorState(runs.message, onRetry = cb.onRetry)
        is Load.Ready -> if (runs.value.isEmpty()) EmptyState("factory", "No runs yet", body = "Start a factory from the Library.")
        else TabBody { for (r in runs.value.sortedByDescending { it.createdAtMs }) RunRow(r, now) { cb.onOpenRun(r) } }
    }
}

@Composable
fun RunRow(r: FactoryRun, now: Long = System.currentTimeMillis(), onClick: () -> Unit) {
    val (label, color) = runStatus(r.status, r.waitingOn)
    CrundiCard(Modifier.fillMaxWidth(), onClick = onClick) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(
                androidx.compose.ui.text.buildAnnotatedString {
                    pushStyle(androidx.compose.ui.text.SpanStyle(fontWeight = FontWeight.SemiBold)); append(r.factoryName); pop()
                    if (r.project.isNotEmpty()) { pushStyle(androidx.compose.ui.text.SpanStyle(color = CrundiColors.TextMuted)); append(" in ${r.project}"); pop() }
                },
                style = CrundiType.Body, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f),
            )
            OutlineChip(if (r.awaitingFinish) "Done: your decision" else label, if (r.awaitingFinish) CrundiColors.Sky else color)
        }
        val task = r.cr?.title?.ifBlank { null } ?: r.task
        if (task.isNotBlank()) Text(task, style = CrundiType.Small, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(top = 3.dp))
        val live = r.employees.values.count { it.busy != null }
        Text(
            listOfNotNull("${r.handoffs} handoffs", "$live live sessions", ago(r.updatedAtMs.takeIf { it > 0 } ?: r.createdAtMs, now), r.outcome?.how?.ifEmpty { null }).joinToString(" · "),
            style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans), modifier = Modifier.padding(top = 3.dp),
        )
    }
}

fun triggerWhen(t: FactoryTrigger): String = when (t.kind) {
    "schedule" -> {
        val s = t.schedule ?: TriggerSchedule()
        val days = listOf("Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat")
        "At ${s.at} " + if (s.days.isEmpty() || s.days.size >= 7) "every day" else "on " + s.days.sorted().joinToString(", ") { days.getOrElse(it) { "?" } }
    }
    "kanban" -> "When a card enters ${kanbanTitle(t.kanban?.status.orEmpty())}"
    "webhook" -> "When its webhook is called"
    else -> t.kind
}

@Composable
private fun TriggersTab(triggers: Load<List<FactoryTrigger>>, factories: List<FactorySummary>, project: String?, cb: FactoriesCallbacks, now: Long) {
    if (project == null) { EmptyState("zap", "Pick a project", body = "Triggers start a project's runs by themselves. Choose a project from the menu at the top left."); return }
    when (triggers) {
        is Load.Loading -> LoadingState(text = "Loading triggers")
        is Load.Failed -> ErrorState(triggers.message, onRetry = cb.onRetry)
        is Load.Ready -> TabBody {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text("Triggers (${triggers.value.size})", style = CrundiType.Heading)
                    Text("Start a run in $project by itself.", style = CrundiType.Small)
                }
                CrundiButton("Add a trigger", { cb.onOpenTrigger(null) }, kind = ButtonKind.Primary, icon = "plus", enabled = factories.any { it.runnable })
            }
            if (triggers.value.isEmpty()) Text("No triggers yet. A trigger starts a factory at a time of day, when a kanban card enters a column, or when its webhook is called.", style = CrundiType.Small)
            for (t in triggers.value) CrundiCard(Modifier.fillMaxWidth(), onClick = { cb.onOpenTrigger(t) }) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text(t.name.ifBlank { "Trigger" }, style = CrundiType.Body.copy(fontWeight = FontWeight.SemiBold), modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
                    if (!t.enabled) StatusBadge("off")
                    MiniToggle(t.enabled, { cb.onToggleTrigger(t) })
                }
                Text(triggerWhen(t), style = CrundiType.Small)
                Text(
                    listOfNotNull(factories.firstOrNull { it.id == t.factoryId }?.name, t.history.lastOrNull()?.let { h -> "last: ${h.outcome} ${ago(jsonMs(h.at), now)}".trim() }).joinToString(" · "),
                    style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans), modifier = Modifier.padding(top = 2.dp),
                )
            }
        }
    }
}

internal fun jsonMs(e: kotlinx.serialization.json.JsonElement?): Long {
    val p = e as? JsonPrimitive ?: return 0
    p.content.toLongOrNull()?.let { return it }
    return parseInstant(p.content)?.toEpochMilli() ?: 0
}

@Composable
private fun CleanupTab(cleanup: Load<Cleanup>, cb: FactoriesCallbacks) {
    when (cleanup) {
        is Load.Loading -> LoadingState(text = "Looking for leftovers")
        is Load.Failed -> ErrorState(cleanup.message, onRetry = cb.onRetry)
        is Load.Ready -> {
            val picked = remember { mutableStateListOf<String>() }
            var days by remember(cleanup.value.autoDeleteDays) { mutableStateOf(cleanup.value.autoDeleteDays.toString()) }
            var confirm by remember { mutableStateOf(false) }
            TabBody {
                Text("Merging and discarding clean up after themselves. What is listed here was kept: stopped runs, kept branches, and folders no run knows about. Nothing is deleted unless you choose to.", style = CrundiType.Small)
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("Delete leftovers older than", style = CrundiType.Small.copy(color = CrundiColors.TextPrimary))
                    CrundiTextField(days, { v -> days = v.filter { it.isDigit() }.take(3); days.toIntOrNull()?.let(cb.onAutoDelete) }, Modifier.width(64.dp), mono = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number))
                    Text("days", style = CrundiType.Small.copy(color = CrundiColors.TextPrimary))
                }
                Text("0 = never; never one with uncommitted work, never a kept branch.", style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans))
                if (cleanup.value.items.isEmpty()) EmptyState("check", "Nothing to clean up.", Modifier.height(240.dp))
                for (it in cleanup.value.items) CleanupRow(it, it.id in picked) { if (it.id in picked) picked.remove(it.id) else picked.add(it.id) }
                if (cleanup.value.items.isNotEmpty()) CrundiButton("Delete selected (${picked.size})", { confirm = true }, kind = ButtonKind.Danger, icon = "trash", enabled = picked.isNotEmpty())
            }
            if (confirm) {
                val lose = cleanup.value.items.count { it.id in picked && uncommitted(it) > 0 }
                CrundiDialog(
                    "Delete leftovers", { confirm = false },
                    message = "Delete ${picked.size} item${if (picked.size == 1) "" else "s"}?" + if (lose > 0) " $lose of them hold work that will be lost." else "",
                    confirm = "Delete", danger = true, onConfirm = { confirm = false; cb.onRemoveLeftovers(picked.toList()); picked.clear() },
                )
            }
        }
    }
}

private fun uncommitted(c: CleanupItem): Int = when (val u = c.uncommitted) {
    is JsonPrimitive -> u.content.toIntOrNull() ?: if (u.content == "true") 1 else 0
    is kotlinx.serialization.json.JsonArray -> u.size
    else -> 0
}

private fun bytes(n: Long): String = when {
    n <= 0 -> "-"
    n < 1024 * 1024 -> "${n / 1024} KB"
    n < 1024L * 1024 * 1024 -> "%.1f MB".format(n / 1048576.0)
    else -> "%.1f GB".format(n / 1073741824.0)
}

@Composable
private fun CleanupRow(c: CleanupItem, picked: Boolean, onToggle: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgCard).border(Dimens.Hairline, if (picked) CrundiColors.Accent else CrundiColors.Border, Dimens.ShapeSm).clickable(onClick = onToggle).padding(10.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Box(Modifier.padding(top = 2.dp)) { PlanningCheckbox(picked, onToggle, 18.dp) }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(
                when (c.kind) { "branch" -> "Kept branch ${c.branch}"; "orphan" -> "Unknown worktree folder"; else -> "Worktree of a ${c.status.ifEmpty { "finished" }} run" },
                style = CrundiType.Body.copy(fontWeight = FontWeight.Medium),
            )
            Text(listOf(c.project, c.factoryName, c.path).filter { it.isNotEmpty() }.joinToString(" · "), style = CrundiType.Meta, maxLines = 2, overflow = TextOverflow.Ellipsis)
            if (c.why.isNotEmpty()) Text(c.why, style = CrundiType.Small)
            val u = uncommitted(c)
            if (u > 0) Text("$u uncommitted change${if (u == 1) "" else "s"} in it: deleting loses them.", style = CrundiType.Small.copy(color = CrundiColors.Yellow))
        }
        Column(horizontalAlignment = Alignment.End) {
            Text(if (c.kind == "branch") "-" else bytes(c.bytes), style = CrundiType.Meta)
            Text("${c.ageMs / 86_400_000}d", style = CrundiType.Meta)
        }
    }
}

// ═══ One factory ═══

@Composable
fun FactoryDetailRoute(id: String) {
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val loader = rememberLoader(id, events = arrayOf("factories"), accept = { it.id == null || it.id == id }) { api.factories.get(id) }
    val actions = rememberActions { loader.refresh() }
    var deleting by remember { mutableStateOf(false) }
    Box(Modifier.fillMaxSize()) {
        FactoryDetailScreen(
            loader.load, onBack = { nav.back() }, onStart = { nav.to(FactoryRoutes.start(id)) },
            onEdit = { focus -> nav.to(FactoryRoutes.edit(id, focus)) },
            onCopy = { actions.run { val d = api.factories.duplicate(id); nav.to(FactoryRoutes.factory(d.factory.id)) } },
            onDelete = { deleting = true }, onRetry = { loader.refresh() },
        )
        ActionError(actions)
    }
    if (deleting) CrundiDialog(
        "Delete factory", { deleting = false }, message = "Delete “${loader.load.valueOrNull?.factory?.name.orEmpty()}”? Runs that already started keep their own copy.",
        confirm = "Delete factory", danger = true, onConfirm = { deleting = false; actions.run(refresh = false) { api.factories.delete(id); nav.back() } },
    )
}

@Composable
fun FactoryDetailScreen(
    d: Load<FactoryDetail>, onBack: () -> Unit = {}, onStart: () -> Unit = {}, onCopy: () -> Unit = {}, onDelete: () -> Unit = {}, onRetry: () -> Unit = {},
    /** Open the editor, at an issue's place when one was tapped (issueFocus; "" for none). */
    onEdit: (String) -> Unit = {},
) {
    var selected by remember { mutableStateOf<String?>(null) }
    val f = d.valueOrNull
    Column(Modifier.fillMaxSize()) {
        TopBar(f?.factory?.name ?: "Factory", subtitle = f?.let { "${it.factory.roster.size} people · ${it.factory.nodes.size} steps · version ${it.factory.version}" }, onBack = onBack) {
            if (f != null) {
                CrundiButton("Edit", { onEdit("") }, kind = ButtonKind.Ghost, icon = "pencil")
                CrundiButton("Start", onStart, kind = ButtonKind.Primary, icon = "play", enabled = f.validation.ok, modifier = Modifier.padding(end = 8.dp))
            }
        }
        when (d) {
            is Load.Loading -> LoadingState()
            is Load.Failed -> ErrorState(d.message, onRetry = onRetry)
            is Load.Ready -> Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
                Box(Modifier.fillMaxWidth().height(380.dp)) { FactoryGraph(d.value.factory, null, selected, { selected = it }) }
                CrundiDivider(strong = true)
                Column(Modifier.padding(Dimens.ScreenPad), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        if (d.value.validation.ok) StatusBadge("Ready", BadgeKind.Good) else StatusBadge("Draft · ${d.value.validation.errors.size} to fix", BadgeKind.Warn)
                        Spacer(Modifier.weight(1f))
                        CrundiButton("Copy", onCopy, kind = ButtonKind.Ghost, icon = "copy")
                        CrundiButton("Delete", onDelete, kind = ButtonKind.Danger, icon = "trash")
                    }
                    Text(d.value.factory.description.ifBlank { "No description." }, style = CrundiType.Small.copy(color = CrundiColors.TextPrimary))
                    val sel = d.value.factory.nodes.firstOrNull { it.id == selected }
                    if (sel != null) CrundiCard(Modifier.fillMaxWidth()) {
                        Text(sel.label.ifEmpty { lookOf(sel.type).kind }, style = CrundiType.Heading)
                        Text(nodeBody(sel, d.value.factory), style = CrundiType.Small)
                        d.value.factory.roster.firstOrNull { it.id == sel.employeeId }?.persona?.takeIf { it.isNotBlank() }?.let { Text(it, style = CrundiType.Small.copy(color = CrundiColors.TextPrimary), maxLines = 8, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(top = 6.dp)) }
                    }
                    val issues = d.value.validation.errors.map { it to true } + d.value.validation.warnings.map { it to false }
                    if (issues.isNotEmpty()) {
                        SectionHeader("Issues", Modifier.padding(horizontal = 0.dp))
                        for ((i, err) in issues) Banner(i.message + if (i.fix.isNotEmpty()) " ${i.fix}" else "", Modifier.fillMaxWidth().clip(Dimens.ShapeSm).clickable { onEdit(issueFocus(i)) }, kind = if (err) BadgeKind.Bad else BadgeKind.Warn, icon = "alert-triangle")
                        Text("Tap one to fix it in the editor.", style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans))
                    }
                    SectionHeader("Team", Modifier.padding(horizontal = 0.dp))
                    for (m in d.value.factory.roster) Row(verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.padding(vertical = 4.dp)) {
                        CrundiIcon("user", size = 16.dp, tint = CrundiColors.TextSecondary, modifier = Modifier.padding(top = 2.dp))
                        Column(Modifier.weight(1f)) {
                            Text(m.name, style = CrundiType.Body.copy(fontWeight = FontWeight.Medium))
                            Text(listOf(m.model.ifEmpty { "default model" }, m.effort.ifEmpty { null }, if (m.memory == "fresh") "fresh each time" else "keeps memory").filterNotNull().joinToString(" · "), style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans))
                            if (m.persona.isNotBlank()) Text(m.persona, style = CrundiType.Small, maxLines = 3, overflow = TextOverflow.Ellipsis)
                        }
                    }
                    Spacer(Modifier.height(24.dp))
                }
            }
        }
    }
}

// ═══ Start a run ═══

@Stable
class StartForm(factoryId: String?, project: String?) {
    var factoryId by mutableStateOf(factoryId.orEmpty())
    var project by mutableStateOf(project.orEmpty())
    var title by mutableStateOf("")
    val sections = mutableStateMapOf<String, String>()
    var mergeMode by mutableStateOf("")
    var error by mutableStateOf<String?>(null)
}

@Composable
fun StartRunRoute(factoryId: String) {
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val shell by LocalAppServices.current.shell.state.collectAsStateWithLifecycle()
    val form = remember { StartForm(factoryId.ifEmpty { null }, shell.currentProject) }
    val factories = rememberLoader(Unit) { api.factories.list() }
    var precheck by remember { mutableStateOf<RunPrecheck?>(null) }
    var busy by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    LaunchedEffect(factories.load) {
        val l = factories.load.valueOrNull ?: return@LaunchedEffect
        if (form.factoryId.isEmpty() || l.none { it.id == form.factoryId && it.runnable }) form.factoryId = l.firstOrNull { it.runnable }?.id.orEmpty()
    }
    LaunchedEffect(form.project) { precheck = null; if (form.project.isNotEmpty()) precheck = runCatching { api.factories.precheck(form.project) }.getOrNull() }
    StartRunScreen(
        form, factories.load, shell.projects.map { it.alias }, precheck, busy, onBack = { nav.back() },
        onOpenRun = { nav.to(FactoryRoutes.run(it)) },
        onStart = {
            val f = factories.load.valueOrNull?.firstOrNull { it.id == form.factoryId }
            form.error = when {
                form.project.isEmpty() -> "Pick a project."
                f == null -> "Pick a factory."
                form.title.isBlank() -> "Give the change request a title."
                else -> f.crTemplate.firstOrNull { it.required && form.sections[it.id].isNullOrBlank() }?.let { "Fill in “${it.label}”." }
            }
            if (form.error == null && f != null) scope.launch {
                busy = true
                try {
                    val run = api.factories.start(form.project, f.id, form.title.trim(), form.sections.filterValues { it.isNotBlank() }, mergeMode = form.mergeMode.ifEmpty { null })
                    nav.back(); nav.to(FactoryRoutes.run(run.id))
                } catch (e: ApiException) { form.error = errorText(e) }
                busy = false
            }
        },
    )
}

@Composable
fun StartRunScreen(
    form: StartForm,
    factories: Load<List<FactorySummary>>,
    projects: List<String>,
    precheck: RunPrecheck?,
    busy: Boolean = false,
    onBack: () -> Unit = {},
    onStart: () -> Unit = {},
    onOpenRun: (String) -> Unit = {},
) {
    Column(Modifier.fillMaxSize().imePadding()) {
        TopBar("Start a factory", onBack = onBack) {
            CrundiButton(if (busy) "Starting…" else "Start", onStart, kind = ButtonKind.Primary, icon = "play", enabled = !busy, modifier = Modifier.padding(end = 8.dp))
        }
        when (factories) {
            is Load.Loading -> LoadingState()
            is Load.Failed -> ErrorState(factories.message)
            is Load.Ready -> Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(Dimens.ScreenPad), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                form.error?.let { Banner(it, Modifier.fillMaxWidth(), kind = BadgeKind.Bad, icon = "alert-triangle") }
                FieldLabel("Project")
                ChipRow(projects.map { it to it }, form.project) { form.project = it }
                FieldLabel("Factory")
                ChipRow(factories.value.map { it.id to (if (it.runnable) it.name else "${it.name} (draft: ${it.errorCount} to fix)") }, form.factoryId, enabled = { id -> factories.value.firstOrNull { it.id == id }?.runnable == true }) { form.factoryId = it }
                precheck?.let { p ->
                    p.active?.let { a ->
                        Banner("A factory is already ${if (a.status == "crashed") "waiting to be recovered" else "running"} here (“${a.factoryName}”). One at a time per project.", Modifier.fillMaxWidth(), kind = BadgeKind.Bad, icon = "alert-triangle")
                        CrundiButton("Open it", { onOpenRun(a.id) }, icon = "external-link")
                    }
                    if (p.branch.isNotEmpty()) Text("The team works in its own worktree, on a new branch cut from ${p.branch}. Your checkout is not touched.", style = CrundiType.Small)
                    if (p.dirty) Banner("You have uncommitted changes. The run starts from the last commit on ${p.branch}, so it will not see them.", Modifier.fillMaxWidth(), kind = BadgeKind.Warn, icon = "alert-triangle")
                }
                Spacer(Modifier.height(4.dp))
                CrundiCard(Modifier.fillMaxWidth(), padding = 14.dp) {
                    Text("Change request", style = CrundiType.Heading)
                    Text("The Manager gets all of it; every step gets the title, goal and the checks for done, and can open the rest.", style = CrundiType.Small, modifier = Modifier.padding(bottom = 6.dp))
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        RequiredLabel("Title", true)
                        CrundiTextField(form.title, { form.title = it.take(120) }, Modifier.fillMaxWidth(), placeholder = "One line: what should change")
                        val f = factories.value.firstOrNull { it.id == form.factoryId }
                        for (s in f?.crTemplate.orEmpty()) {
                            RequiredLabel(s.label, s.required)
                            CrundiTextField(
                                form.sections[s.id].orEmpty(), { form.sections[s.id] = it }, Modifier.fillMaxWidth(),
                                placeholder = s.hint + if (s.list) " One per line." else "", singleLine = false, minLines = if (s.list) 3 else 2,
                            )
                        }
                    }
                }
                FieldLabel("When the work is done")
                ChipRow(listOf("" to "As the factory says", "ask" to "Show me first, I decide", "auto" to "Merge automatically (with Undo)"), form.mergeMode) { form.mergeMode = it }
                Spacer(Modifier.height(32.dp))
            }
        }
    }
}

@Composable
private fun RequiredLabel(t: String, required: Boolean) = Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
    FieldLabel(t)
    if (required) Text("required", style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontStyle = androidx.compose.ui.text.font.FontStyle.Italic))
}

@Composable
internal fun ChipRow(options: List<Pair<String, String>>, value: String, enabled: (String) -> Boolean = { true }, onPick: (String) -> Unit) {
    FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        for ((k, label) in options) CrundiChip(label, k == value, { if (enabled(k)) onPick(k) }, modifier = if (enabled(k)) Modifier else Modifier.alpha(0.5f))
    }
}

// ═══ Trigger editor ═══

@Composable
fun TriggerEditRoute(id: String) {
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val shell by LocalAppServices.current.shell.state.collectAsStateWithLifecycle()
    val project = shell.currentProject.orEmpty()
    val scope = rememberCoroutineScope()
    var load by remember { mutableStateOf<Load<Pair<FactoryTrigger, List<FactorySummary>>>>(Load.Loading) }
    var message by remember { mutableStateOf<String?>(null) }
    var confirm by remember { mutableStateOf<String?>(null) }
    var hooksInData by remember { mutableStateOf(false) }
    LaunchedEffect(id) {
        load = try {
            val fs = api.factories.list()
            val all = if (id.isEmpty()) runCatching { api.factories.triggers(null) }.getOrDefault(emptyList()) else api.factories.triggers(null)
            hooksInData = Webhooks.inTriggers(all)
            val t = if (id.isEmpty()) FactoryTrigger(project = project, factoryId = fs.firstOrNull { it.runnable }?.id.orEmpty(), kind = "kanban", kanban = TriggerKanban("todo"),
                cr = TriggerCr("{{card.title}}", mapOf("goal" to "{{card.description}}", "done" to "{{card.todos}}")))
            else all.firstOrNull { it.id == id } ?: throw ApiException(404, "This trigger is gone. It may have been deleted elsewhere.")
            Load.Ready(t to fs)
        } catch (e: ApiException) { Load.Failed(errorText(e)) }
    }
    fun act(block: suspend () -> Unit) = scope.launch { try { block() } catch (e: ApiException) { message = errorText(e) } }
    TriggerEditScreen(
        load, shell.serverHost, message,
        hooks = hooksInData || Webhooks.supported(shell),
        onBack = { nav.back() },
        onSave = { t ->
            act {
                val saved = api.factories.saveTrigger(t)
                // A new webhook trigger stays open, as on the web: its URL only exists now.
                if (t.id.isEmpty() && saved.kind == "webhook") { load = Load.Ready(saved to load.valueOrNull!!.second); message = "Saved. Its URL is below." }
                else nav.back()
            }
        },
        onDelete = { confirm = "delete" },
        onRun = { t -> act { val r = api.factories.runTrigger(t.id); message = if (r.str("outcome") == "queued" || r.str("queued") != null) "Queued: a run is going in this project." else "Started." } },
        onRotate = { which -> confirm = which },
        onCopied = { message = it },
    )
    val t = load.valueOrNull?.first
    val c = confirm
    if (c != null && t != null) CrundiDialog(
        when (c) { "delete" -> "Delete trigger"; "secret" -> "New secret"; else -> "New key" }, { confirm = null },
        message = if (c == "delete") "Delete this trigger?" else rotateQuestion(c),
        confirm = when (c) { "delete" -> "Delete trigger"; "secret" -> "Make a new secret"; else -> "Make a new key" }, danger = c == "delete",
        onConfirm = {
            confirm = null
            if (c == "delete") act { api.factories.deleteTrigger(t.id); nav.back() }
            else act { val nt = api.factories.rotateTrigger(t.id, c); load = Load.Ready(nt to load.valueOrNull!!.second) }
        },
    )
}

@Composable
fun TriggerEditScreen(
    load: Load<Pair<FactoryTrigger, List<FactorySummary>>>,
    serverHost: String = "",
    message: String? = null,
    /** The server has signed webhooks (Webhooks.supported, or the data shows it): Key or Signature, idempotency, Tell its Manager. */
    hooks: Boolean = false,
    onBack: () -> Unit = {},
    onSave: (FactoryTrigger) -> Unit = {},
    onDelete: () -> Unit = {},
    onRun: (FactoryTrigger) -> Unit = {},
    /** Replace the key or ("secret") the secret; the caller confirms. */
    onRotate: (String) -> Unit = {},
    onCopied: (String) -> Unit = {},
) {
    val ready = load.valueOrNull
    // Kept across a new key or secret: the address below reads the saved trigger, the fields keep what is typed.
    var t by remember(ready?.first?.id) { mutableStateOf(ready?.first ?: FactoryTrigger()) }
    Column(Modifier.fillMaxSize().imePadding()) {
        TopBar(if (ready == null || t.id.isEmpty()) "New trigger" else "Trigger", onBack = onBack) {
            if (ready != null) CrundiButton("Save", { onSave(t) }, kind = ButtonKind.Primary, icon = "check", enabled = t.factoryId.isNotEmpty(), modifier = Modifier.padding(end = 8.dp))
        }
        when (load) {
            is Load.Loading -> LoadingState()
            is Load.Failed -> ErrorState(load.message)
            is Load.Ready -> Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(Dimens.ScreenPad), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                message?.let { Banner(it, Modifier.fillMaxWidth()) }
                FieldLabel("Name")
                CrundiTextField(t.name, { t = t.copy(name = it) }, Modifier.fillMaxWidth(), placeholder = "Cards to the team")
                FieldLabel("Factory")
                ChipRow(load.value.second.filter { it.runnable }.map { it.id to it.name }, t.factoryId) { t = t.copy(factoryId = it) }
                FieldLabel("Start when")
                ChipRow(listOf("kanban" to "A kanban card enters a column", "schedule" to "A time of day", "webhook" to "A webhook is called"), t.kind) { k ->
                    t = t.copy(kind = k, schedule = t.schedule ?: TriggerSchedule().takeIf { k == "schedule" }, kanban = t.kanban ?: TriggerKanban("todo").takeIf { k == "kanban" })
                    // New webhook triggers default to telling the running Manager, as on the web.
                    if (hooks && k == "webhook" && t.id.isEmpty() && t.ifBusy == "queue") t = t.copy(ifBusy = "manager")
                }
                when (t.kind) {
                    "schedule" -> {
                        val s = t.schedule ?: TriggerSchedule()
                        FieldLabel("At (this server's time)")
                        CrundiTextField(s.at, { t = t.copy(schedule = s.copy(at = it)) }, Modifier.width(120.dp), mono = true, icon = "clock")
                        FieldLabel("On (none = every day)")
                        Row(horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                            for ((i, d) in listOf("Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat").withIndex()) CrundiChip(d, i in s.days, {
                                t = t.copy(schedule = s.copy(days = if (i in s.days) s.days - i else (s.days + i).sorted()))
                            })
                        }
                    }
                    "kanban" -> {
                        FieldLabel("Column")
                        ChipRow(listOf("backlog", "todo", "in_progress", "done").map { it to kanbanTitle(it) }, t.kanban?.status.orEmpty()) { t = t.copy(kanban = TriggerKanban(it)) }
                    }
                    "webhook" -> if (hooks) {
                        Text("Public URL, so every call must prove it is from you: the key, or a signature made with the secret. Calls are answered at once; the change request below is filled from the call's JSON.", style = CrundiType.Small)
                        HookAuthFields(Webhooks.filled(t.webhook), { h -> t = t.copy(webhook = TriggerHooks.edited(t.webhook, h)) }) { FieldLabel(it) }
                        val saved = load.value.first
                        if (saved.id.isEmpty() || saved.kind != "webhook" || saved.webhook?.key.isNullOrEmpty()) Text("Save the trigger to get its webhook address.", style = CrundiType.Small)
                        else {
                            val sh = Webhooks.filled(saved.webhook)
                            HookAddress(Webhooks.triggerUrl(serverHost, saved.id, sh), sh, { FieldLabel(it) }, onRotate, onCopied)
                            if (saved.seenCount > 0) Text("${saved.seenCount} idempotency key${if (saved.seenCount == 1) "" else "s"} remembered (30 days).", style = CrundiType.Meta)
                        }
                    } else {
                        if (t.id.isEmpty()) Text("Save the trigger to get its webhook address.", style = CrundiType.Small)
                        else {
                            FieldLabel("Webhook URL (POST, JSON body)")
                            CodeBlock("https://$serverHost/api/factory-hook/trigger/${t.id}?key=${t.webhook?.key.orEmpty()}", highlight = false)
                            CrundiButton("New key", { onRotate("key") }, icon = "refresh")
                        }
                    }
                }
                Spacer(Modifier.height(6.dp))
                Text("Change request", style = CrundiType.Heading)
                Text(
                    "Filled in when it fires. Placeholders: " + when (t.kind) {
                        "kanban" -> "{{card.title}}, {{card.description}}, {{card.todos}}, {{date}}"
                        // {{body.x}} is the old name; servers before 1.21.1 know only it.
                        "webhook" -> if (hooks) "{{payload.some.field}}, {{payload.alerts.0.labels.alertname}} (arrays by index), {{payload}} (all of it), {{headers.name}}, {{query.name}}, {{date}}" else "{{body.x}}, {{body}}, {{date}}"
                        else -> "{{date}}, {{time}}"
                    } + if (hooks && t.kind == "webhook") ". Anything a call does not have comes out empty and never stops it; the Manager gets the full request as well." else ". Only the title has to come out filled.",
                    style = CrundiType.Small,
                )
                FieldLabel("Title")
                CrundiTextField(t.cr.title, { t = t.copy(cr = t.cr.copy(title = it)) }, Modifier.fillMaxWidth(), mono = true)
                val tpl = load.value.second.firstOrNull { it.id == t.factoryId }?.crTemplate.orEmpty()
                for (s in tpl) {
                    FieldLabel(s.label)
                    CrundiTextField(t.cr.sections[s.id].orEmpty(), { v -> t = t.copy(cr = t.cr.copy(sections = t.cr.sections + (s.id to v))) }, Modifier.fillMaxWidth(), mono = true, singleLine = false, minLines = 2, placeholder = s.hint)
                }
                FieldLabel("When the work is done")
                ChipRow(listOf("" to "As the factory says", "ask" to "Show me first, I decide", "auto" to "Merge automatically (with Undo)"), t.mergeMode) { t = t.copy(mergeMode = it) }
                FieldLabel("If a run is already going")
                ChipRow(
                    (if (hooks || t.ifBusy == "manager") listOf("manager" to "Tell its Manager (a run of the same factory)") else emptyList()) + listOf("queue" to "Wait its turn, then start", "skip" to "Skip it"),
                    t.ifBusy,
                ) { t = t.copy(ifBusy = it) }
                if (t.ifBusy == "manager") Text("A call that arrives while a run of this factory is going goes to its Manager as a message, marked mid-run, with the whole request; it decides whether it needs acting on. A run of another factory: it waits its turn.", style = CrundiType.Small)
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("On", style = CrundiType.Body, modifier = Modifier.weight(1f))
                    MiniToggle(t.enabled, { t = t.copy(enabled = !t.enabled) })
                }
                if (t.history.isNotEmpty()) {
                    SectionHeader("What it did (${t.history.size})", Modifier.padding(horizontal = 0.dp))
                    for (h in t.history.takeLast(10).reversed()) Text(
                        listOfNotNull(shortDateTime(jsonMs(h.at)), h.outcome, h.source.ifEmpty { null }, h.title, h.note).joinToString(" · "),
                        style = CrundiType.Small,
                    )
                }
                if (t.id.isNotEmpty()) Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(top = 8.dp)) {
                    CrundiButton("Run now", { onRun(t) }, icon = "play")
                    CrundiButton("Delete", onDelete, kind = ButtonKind.Danger, icon = "trash")
                }
                Spacer(Modifier.height(32.dp))
            }
        }
    }
}

// ═══ Run diff ═══

@Composable
fun RunDiffRoute(id: String) {
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val loader = rememberLoader(id) { api.factories.diff(id) }
    Column(Modifier.fillMaxSize()) {
        TopBar("The run's changes", onBack = { nav.back() })
        when (val l = loader.load) {
            is Load.Loading -> LoadingState(text = "Reading the diff")
            is Load.Failed -> ErrorState(l.message, onRetry = { loader.refresh() })
            is Load.Ready -> Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(12.dp)) {
                if (l.value.truncated) Banner("The diff is long; this is the start of it.", Modifier.fillMaxWidth().padding(bottom = 8.dp), kind = BadgeKind.Warn)
                if (l.value.diff.isBlank()) Text("No changes.", style = CrundiType.Small) else CodeBlock(l.value.diff, language = "diff")
            }
        }
    }
}


/** A trigger's webhook settings as the editor changes them. */
object TriggerHooks {
    /** [edited]'s fields over what the server sent, keeping the key and secret it sent. */
    fun edited(sent: HookSettings?, edited: HookSettings) = (sent ?: HookSettings()).copy(
        auth = edited.auth, signatureHeader = edited.signatureHeader, timestampHeader = edited.timestampHeader, idempotency = edited.idempotency,
    )
}
