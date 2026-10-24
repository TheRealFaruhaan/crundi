package mv.obitech.crundi.ui.planning

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.json.JsonObject
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.bool
import mv.obitech.crundi.api.model.CrField
import mv.obitech.crundi.api.model.Factory
import mv.obitech.crundi.api.model.FactoryDetail
import mv.obitech.crundi.api.model.FactoryEdge
import mv.obitech.crundi.api.model.FactoryIssue
import mv.obitech.crundi.api.model.FactoryMember
import mv.obitech.crundi.api.model.FactoryNode
import mv.obitech.crundi.api.model.FactorySettings
import mv.obitech.crundi.api.model.FactoryValidation
import mv.obitech.crundi.api.model.Option
import mv.obitech.crundi.api.model.Position
import mv.obitech.crundi.api.model.SkillBrief
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.Banner
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiDivider
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.TopBar
import mv.obitech.crundi.ui.components.errorText
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/*
 * The factory editor on a phone: tools/factory-flow/src (Editor.jsx and its
 * panels) as the web's phone layout has it. The flow fills the screen; a bar
 * at the bottom opens Add, Edit, Team and Issues as a sheet over its lower
 * half. Touch first: tap a step to edit it, long-press and drag to move it,
 * long-press and let go to connect from it (then tap where it goes), tap a
 * connection's dot to change it. Changes save by themselves 1.2 seconds
 * after the last one, against the version they were made on (a change made
 * elsewhere meanwhile is a conflict, with Reload), and the server's
 * validation comes back with every save.
 */

enum class EditorTab(val label: String, val icon: String) { Add("Add", "plus"), Edit("Edit", "sliders"), Team("Team", "users"), Issues("Issues", "alert-triangle") }

/** As the web's save status: Saved, Saving…, Unsaved, Changed elsewhere, Not saved. */
enum class SaveState(val label: String) { Saved("Saved"), Saving("Saving…"), Dirty("Unsaved"), Conflict("Changed elsewhere"), Error("Not saved") }

/** What the Edit panel is about: a step or a connection (neither: the factory's settings). */
sealed interface EditSel {
    data class Node(val id: String) : EditSel
    data class Edge(val id: String) : EditSel
}

/** The editor's state: the factory as edited, the server's last copy, and what the screen shows. */
@Stable
class FactoryEditorState(detail: FactoryDetail, val skills: List<SkillBrief> = emptyList(), val data: FactoryEditorData = FactoryEditorData.bundled) {
    var factory by mutableStateOf(detail.factory)
        private set
    /** The server's last copy of the definition: saves go over it ([mv.obitech.crundi.api.FactoryJson]). */
    var raw: JsonObject? = detail.raw
    var version by mutableIntStateOf(detail.factory.version)
    var validation by mutableStateOf(detail.validation)
    var save by mutableStateOf(SaveState.Saved)
    var saveError by mutableStateOf<String?>(null)
    var selection by mutableStateOf<EditSel?>(null)
    var tab by mutableStateOf<EditorTab?>(null)
    var rosterSel by mutableStateOf<String?>(null)
    /** A step waiting for its person: whoever is added in the Team panel gets it. */
    var rosterAddTo by mutableStateOf<String?>(null)
    /** Connecting from a step's output: tap the step it goes to. */
    var connect by mutableStateOf<Pair<String, String>?>(null)
    var toast by mutableStateOf<String?>(null)
    var removing by mutableStateOf<String?>(null)
    var focusNode by mutableStateOf<String?>(null)
    var focusKey by mutableIntStateOf(0)
    /** The middle of the screen, in factory coordinates: where Add puts a step. */
    var centre = Position()
    val history = EditHistory()
    /** Bumped on every change (and undo, redo): the autosave waits on it. */
    var rev by mutableIntStateOf(0)
        private set
    var savedRev by mutableIntStateOf(0)
    val dirty get() = rev != savedRev
    val skillNames: Set<String> = skills.map { it.name }.toSet()

    /** A change, as the web's mutate: one undo step unless [key] repeats within 1.5 s (typing). */
    fun mutate(key: String = "", fn: (Factory) -> Factory) {
        val prev = factory
        val next = fn(prev)
        if (next == prev) return
        history.record(prev, key)
        factory = next
        rev++
        if (save != SaveState.Conflict) save = SaveState.Dirty
    }

    fun undo() { history.undo(factory)?.let { factory = it; rev++; save = SaveState.Dirty } }
    fun redo() { history.redo(factory)?.let { factory = it; rev++; save = SaveState.Dirty } }

    /** The server's copy, as Reload after a conflict: history goes. */
    fun reloaded(d: FactoryDetail) {
        factory = d.factory; raw = d.raw; version = d.factory.version; validation = d.validation
        history.clear(); rev++; savedRev = rev; save = SaveState.Saved; saveError = null
    }

    /** Fields of the settings an older server does not have are hidden. */
    fun has(setting: String): Boolean = (raw?.get("settings") as? JsonObject)?.containsKey(setting) ?: true
    /** Per-person presets (newer servers). */
    val hasMemberPresets: Boolean get() = (raw?.get("roster") as? kotlinx.serialization.json.JsonArray)?.firstOrNull()?.let { (it as? JsonObject)?.containsKey("presets") } ?: true

    /** Select, and open the sheet; a step is brought into view above it. */
    fun select(s: EditSel?) {
        selection = s
        if (s != null) tab = EditorTab.Edit
        val at = when (s) { is EditSel.Node -> s.id; is EditSel.Edge -> factory.edges.firstOrNull { it.id == s.id }?.source; null -> null }
        if (at != null) { focusNode = at; focusKey++ }
    }

    /** Tap on the empty canvas: as the web's phone, the sheet closes. */
    fun background() { if (connect != null) { connect = null; return }; selection = null; tab = null }

    fun tapNode(id: String) {
        val c = connect
        if (c == null) { select(EditSel.Node(id)); return }
        if (id == c.first) { connect = null; return }
        val r = FactoryEdit.connect(factory, c.first, c.second, id)
        if (r == null) { toast = "It cannot lead there."; return }
        mutate { r.first }
        connect = null
        if (r.second) toast = "That closes a loop, so it got a round limit of 3. Change it on the connection."
    }

    /** Long press: connect from this step, from its first output that leads nowhere yet. */
    fun startConnect(id: String, handle: String? = null) {
        val n = factory.nodes.firstOrNull { it.id == id } ?: return
        val outs = outputsFor(n)
        if (outs.isEmpty()) { toast = "End leads nowhere: only the Manager's Finish reaches it."; return }
        val h = handle ?: outs.firstOrNull { o -> o != "note" && factory.edges.none { it.kind == "flow" && it.source == id && it.sourceHandle == o } } ?: outs.first()
        connect = id to h
        selection = EditSel.Node(id)
        focusNode = id; focusKey++
    }

    /** The outputs a step can be connected from here, notes included for people. */
    fun outputsFor(n: FactoryNode) = FactoryEdit.outputsOf(n) + if (n.type in FactoryEdit.AGENTS) listOf("note") else emptyList()

    fun add(kind: String) {
        val (f, id) = FactoryEdit.addNode(factory, kind, Position(centre.x - 110, centre.y - 40))
        mutate { f }
        selection = EditSel.Node(id)
        if (kind == "employee:") { rosterAddTo = id; rosterSel = null; tab = EditorTab.Team } else tab = EditorTab.Edit
    }

    fun deleteSelected() {
        when (val s = selection) {
            is EditSel.Node -> mutate { FactoryEdit.deleteNodes(it, listOf(s.id)) }
            is EditSel.Edge -> mutate { FactoryEdit.deleteEdge(it, s.id) }
            null -> {}
        }
        selection = null
    }

    /** Go to what an issue is about: the person, the step (centred), or the connection. */
    fun pick(it: FactoryIssue) {
        it.employeeId?.let { e -> rosterSel = e; tab = EditorTab.Team; return }
        val id = it.nodeId ?: it.nodeIds.firstOrNull()
        if (id != null) { select(EditSel.Node(id)); return }
        it.edgeId?.let { e -> select(EditSel.Edge(e)) }
    }

    /** "node:<id>", "edge:<id>", "emp:<id>": where to open (an issue tapped on the factory's page). */
    fun focusOn(where: String) {
        val (k, v) = where.substringBefore(':') to where.substringAfter(':', "")
        if (v.isEmpty()) return
        when (k) {
            "node" -> pick(FactoryIssue(nodeId = v))
            "edge" -> pick(FactoryIssue(edgeId = v))
            "emp" -> pick(FactoryIssue(employeeId = v))
        }
    }

    fun issuesFor(node: String? = null, edge: String? = null, employee: String? = null): List<Pair<FactoryIssue, Boolean>> =
        (validation.errors.map { it to true } + validation.warnings.map { it to false }).filter { (i, _) ->
            (node != null && (i.nodeId == node || node in i.nodeIds)) || (edge != null && i.edgeId == edge) || (employee != null && i.employeeId == employee)
        }
}

// ═══ Route ═══

@Composable
fun FactoryEditRoute(id: String, focus: String = "") {
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val scope = rememberCoroutineScope()
    var load by remember { mutableStateOf<Load<FactoryEditorState>>(Load.Loading) }
    val lock = remember { Mutex() }
    var leaving by remember { mutableStateOf(false) }
    LaunchedEffect(id) {
        load = try {
            val d = api.factories.get(id)
            val skills = runCatching { api.factories.skills() }.getOrDefault(emptyList())
            Load.Ready(FactoryEditorState(d, skills).apply { focusOn(focus) })
        } catch (e: ApiException) { Load.Failed(errorText(e)) }
    }
    val st = load.valueOrNull

    /** Save what is not saved yet, one save at a time. False: it did not (a conflict, an error). */
    suspend fun saveNow(st: FactoryEditorState): Boolean = lock.withLock {
        if (!st.dirty || st.save == SaveState.Conflict) return@withLock !st.dirty
        val sent = st.rev
        st.save = SaveState.Saving
        try {
            val d = api.factories.save(st.factory, st.raw, st.version)
            st.version = d.factory.version; st.raw = d.raw ?: st.raw; st.validation = d.validation
            st.savedRev = sent
            st.save = if (st.dirty) SaveState.Dirty else SaveState.Saved
            st.saveError = null
            true
        } catch (e: ApiException) {
            if (e.body?.bool("conflict") == true) st.save = SaveState.Conflict else { st.save = SaveState.Error; st.saveError = errorText(e) }
            false
        }
    }

    if (st != null) LaunchedEffect(st, st.rev) {
        if (!st.dirty) return@LaunchedEffect
        delay(1200)
        // Not in this effect: the next change must not cancel a save in flight.
        scope.launch { saveNow(st) }
    }
    if (st != null) LaunchedEffect(st.toast) { if (st.toast != null) { delay(4000); st.toast = null } }

    fun back() {
        val s = st ?: run { nav.back(); return }
        if (leaving || !s.dirty) { nav.back(); return }
        scope.launch {
            if (saveNow(s)) nav.back() else { leaving = true; s.toast = "Not saved. Go back again to leave without saving." }
        }
    }
    BackHandler { if (st?.connect != null) st.connect = null else if (st?.tab != null) st.tab = null else back() }
    FactoryEditorScreen(
        load, onBack = { back() },
        onReload = { s -> scope.launch { runCatching { api.factories.get(id) }.onSuccess { s.reloaded(it) } } },
        onRetry = { s -> s.save = SaveState.Dirty; scope.launch { saveNow(s) } },
    )
}

// ═══ Screen ═══

@Composable
fun FactoryEditorScreen(
    load: Load<FactoryEditorState>,
    onBack: () -> Unit = {},
    onReload: (FactoryEditorState) -> Unit = {},
    onRetry: (FactoryEditorState) -> Unit = {},
) {
    val st = load.valueOrNull
    Column(Modifier.fillMaxSize().imePadding()) {
        TopBar(
            st?.factory?.name?.ifEmpty { null } ?: "Untitled factory", onBack = onBack,
            subtitle = st?.let { s -> s.save.label + if (s.validation.ok) "" else " · draft" },
        ) {
            if (st != null) {
                CrundiIconButton("undo", { st.undo() }, contentDescription = "Undo", enabled = st.rev >= 0 && st.history.canUndo)
                CrundiIconButton("redo", { st.redo() }, contentDescription = "Redo", enabled = st.rev >= 0 && st.history.canRedo)
                CrundiIconButton("layers", { st.mutate { FactoryEdit.layout(it) }; st.focusNode = it0(st); st.focusKey++ }, contentDescription = "Tidy the layout")
            }
        }
        when (load) {
            is Load.Loading -> LoadingState(text = "Opening the factory")
            is Load.Failed -> ErrorState(load.message)
            is Load.Ready -> EditorBody(load.value, onReload, onRetry)
        }
    }
}

private fun it0(st: FactoryEditorState) = st.factory.nodes.firstOrNull { it.type == "start" }?.id

@Composable
private fun ColumnScope.EditorBody(st: FactoryEditorState, onReload: (FactoryEditorState) -> Unit, onRetry: (FactoryEditorState) -> Unit) {
    when (st.save) {
        SaveState.Conflict -> SaveBanner("This factory was changed somewhere else. Reload it to see the latest version; your changes here are not saved.", "Reload") { onReload(st) }
        SaveState.Error -> SaveBanner(st.saveError ?: "Not saved.", "Retry") { onRetry(st) }
        else -> {}
    }
    BoxWithConstraints(Modifier.weight(1f).fillMaxWidth()) {
        val sheet = maxHeight * 0.56f
        Column(Modifier.fillMaxSize()) {
            Box(Modifier.weight(1f).fillMaxWidth()) {
                val f = st.factory
                val sel = st.selection
                val c = st.connect
                FactoryGraph(
                    f, null, (sel as? EditSel.Node)?.id, { id -> if (id != null) st.tapNode(id) },
                    edit = GraphEdit(
                        selectedEdge = (sel as? EditSel.Edge)?.id,
                        connectFrom = c,
                        canConnectTo = { t -> c != null && FactoryEdit.canConnect(f, c.first, c.second, t) },
                        onMove = { id, p -> st.mutate { FactoryEdit.moveNode(it, id, FactoryEdit.snap(p)) } },
                        onLongPress = { st.startConnect(it) },
                        onEdge = { st.select(EditSel.Edge(it)) },
                        onBackground = { st.background() },
                        onViewCentre = { st.centre = it },
                        focusNode = st.focusNode, focusKey = st.focusKey,
                    ),
                )
                if (c != null) ConnectBar(st, c, Modifier.align(Alignment.TopCenter))
                st.toast?.let { t ->
                    Text(
                        t, style = CrundiType.Small.copy(color = CrundiColors.TextPrimary),
                        modifier = Modifier.align(Alignment.BottomCenter).padding(12.dp).clip(Dimens.Shape).background(CrundiColors.BgTertiary).border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape).padding(horizontal = 12.dp, vertical = 9.dp),
                    )
                }
            }
            val tab = st.tab
            if (tab != null) Column(Modifier.fillMaxWidth().height(sheet).background(CrundiColors.BgSecondary)) {
                CrundiDivider(strong = true)
                Tabs(st, open = true)
                CrundiDivider()
                Column(Modifier.fillMaxWidth().weight(1f).verticalScroll(rememberScrollState()).padding(Dimens.ScreenPad), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    when (tab) {
                        EditorTab.Add -> AddPanel(st)
                        EditorTab.Team -> TeamPanel(st)
                        EditorTab.Issues -> IssuesPanel(st.validation) { st.pick(it) }
                        EditorTab.Edit -> {
                            val f = st.factory
                            val node = (st.selection as? EditSel.Node)?.let { s -> f.nodes.firstOrNull { it.id == s.id } }
                            val edge = (st.selection as? EditSel.Edge)?.let { s -> f.edges.firstOrNull { it.id == s.id } }
                            when {
                                node != null -> NodePanel(st, node)
                                edge != null -> EdgePanel(st, edge)
                                else -> SettingsPanel(st)
                            }
                        }
                    }
                    Spacer(Modifier.height(24.dp))
                }
            } else Column(Modifier.fillMaxWidth().background(CrundiColors.BgSecondary).navigationBarsPadding()) {
                CrundiDivider(strong = true)
                Tabs(st, open = false)
            }
        }
    }
    st.removing?.let { id ->
        val f = st.factory
        val r = f.roster.firstOrNull { it.id == id }
        val steps = FactoryEdit.stepsOf(f, id)
        CrundiDialog(
            "Remove from the team", { st.removing = null },
            message = "Remove ${r?.name?.ifEmpty { null } ?: "this person"} from the team" + (if (steps > 0) " and their $steps step${if (steps == 1) "" else "s"} from the flow" else "") + "?",
            confirm = "Remove", danger = true,
            onConfirm = { st.removing = null; st.mutate { FactoryEdit.removeEmployee(it, id) }; st.rosterSel = null },
        )
    }
}

@Composable
private fun SaveBanner(text: String, action: String, onAction: () -> Unit) {
    Row(Modifier.fillMaxWidth().background(CrundiColors.BgSecondary).padding(horizontal = 12.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        CrundiIcon("alert-triangle", size = Dimens.IconSm, tint = CrundiColors.Yellow)
        Text(text, style = CrundiType.Small.copy(color = CrundiColors.TextPrimary), modifier = Modifier.weight(1f))
        CrundiButton(action, onAction, kind = ButtonKind.Primary)
    }
}

/** The phone bar (closed) or the sheet's tabs (open), as the web's ff-phonebar and ff-tabs. */
@Composable
private fun Tabs(st: FactoryEditorState, open: Boolean) {
    Row(Modifier.fillMaxWidth().padding(horizontal = 6.dp, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        for (t in EditorTab.entries) {
            val bad = t == EditorTab.Issues && !st.validation.ok
            val on = open && st.tab == t
            val color = when { bad -> CrundiColors.Red; on -> CrundiColors.Accent; else -> CrundiColors.TextSecondary }
            Column(
                Modifier.weight(1f).clip(Dimens.ShapeSm).clickable { st.tab = t }.padding(vertical = 6.dp),
                horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                CrundiIcon(t.icon, size = 18.dp, tint = color)
                Text(if (t == EditorTab.Issues && bad) "Fix ${st.validation.errors.size}" else t.label, style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, color = color, fontWeight = if (on) FontWeight.SemiBold else null))
            }
        }
        if (open) CrundiIconButton("x", { st.tab = null }, contentDescription = "Close")
    }
}

/** While connecting: which output, and how to finish. */
@Composable
private fun ConnectBar(st: FactoryEditorState, c: Pair<String, String>, modifier: Modifier) {
    val f = st.factory
    val n = f.nodes.firstOrNull { it.id == c.first } ?: return
    Column(
        modifier.padding(10.dp).fillMaxWidth().clip(Dimens.Shape).background(CrundiColors.BgSecondary).border(Dimens.Hairline, CrundiColors.Accent, Dimens.Shape).padding(10.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("Connect “${FactoryEdit.nodeName(f, n)}”", style = CrundiType.Body.copy(fontWeight = FontWeight.SemiBold), modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
            CrundiButton("Cancel", { st.connect = null }, kind = ButtonKind.Ghost)
        }
        val outs = st.outputsFor(n)
        if (outs.size > 1) Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            for (h in outs) mv.obitech.crundi.ui.components.CrundiChip(outLabel(n, h), h == c.second, { st.connect = c.first to h })
        }
        Text(if (c.second == "note") "Tap the person the note is for." else "Tap the step it leads to.", style = CrundiType.Small)
    }
}

private fun outLabel(n: FactoryNode, h: String) = when (h) { "out" -> "Next"; "note" -> "A note"; else -> FactoryEdit.handleLabel(n, h).ifEmpty { h } }

// ═══ Add (Palette.jsx) ═══

private val FLOW_ITEMS = listOf(
    Triple("decision:yesno", "git-branch", "Yes / no question"),
    Triple("decision:one", "git-branch", "Pick one"),
    Triple("decision:many", "git-branch", "Pick any (parallel)"),
    Triple("join", "git-merge", "Join"),
    Triple("ask", "help-circle", "Ask me"),
    Triple("wait", "clock", "Wait"),
)

@Composable
private fun ColumnScope.AddPanel(st: FactoryEditorState) {
    val people = st.factory.roster.filter { it.id != FactoryEdit.MANAGER_ID }
    Subtitle("People")
    for (r in people) AddRow("user", r.name.ifEmpty { "Unnamed" }) { st.add("employee:" + r.id) }
    AddRow("plus", "Someone new", ghost = true) { st.add("employee:") }
    Subtitle("Flow")
    for ((k, icon, label) in FLOW_ITEMS) AddRow(icon, label, hint = when (k) { "decision:many" -> "Comes with its Join"; "wait" -> "A time, a command, a signal"; else -> null }) { st.add(k) }
    Hint("Tap to add it in the middle of the view. Long-press a step and drag to move it; long-press and let go to connect it to the next step, or to leave a note for someone. Tap a connection's dot to change it.")
}

@Composable
private fun AddRow(icon: String, label: String, hint: String? = null, ghost: Boolean = false, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(if (ghost) CrundiColors.BgSecondary else CrundiColors.BgCard).border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm).clickable(onClick = onClick).padding(horizontal = 12.dp, vertical = 11.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        CrundiIcon(icon, size = 16.dp, tint = CrundiColors.TextSecondary)
        Text(label, style = CrundiType.Body, modifier = Modifier.weight(1f))
        if (hint != null) Text(hint, style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans))
    }
}

// ═══ Edit: a step (Inspector.jsx NodeInspector) ═══

@Composable
private fun PanelTitle(kind: String, onDelete: (() -> Unit)? = null, deleteLabel: String = "Delete", onBack: (() -> Unit)? = null) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        if (onBack != null) CrundiIconButton("chevron-left", onBack, contentDescription = "Back to the team")
        Text(kind.uppercase(), style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontWeight = FontWeight.SemiBold, color = CrundiColors.TextSecondary), modifier = Modifier.weight(1f))
        if (onDelete != null) CrundiIconButton("trash", onDelete, contentDescription = deleteLabel, tint = CrundiColors.Red)
    }
}

@Composable
private fun IssueList(issues: List<Pair<FactoryIssue, Boolean>>, onPick: ((FactoryIssue) -> Unit)? = null) {
    for ((i, err) in issues) Row(
        Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(if (err) CrundiColors.RedDim else CrundiColors.BgCard)
            .border(Dimens.Hairline, (if (err) CrundiColors.Red else CrundiColors.Yellow).copy(alpha = 0.5f), Dimens.ShapeSm)
            .then(if (onPick != null) Modifier.clickable { onPick(i) } else Modifier).padding(10.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        CrundiIcon("alert-triangle", size = 14.dp, tint = if (err) CrundiColors.Red else CrundiColors.Yellow, modifier = Modifier.padding(top = 2.dp))
        Column(Modifier.weight(1f)) {
            Text(i.message, style = CrundiType.Small.copy(color = CrundiColors.TextPrimary))
            if (i.fix.isNotEmpty()) Text(i.fix, style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans))
        }
        if (onPick != null) CrundiIcon("chevron-right", size = 14.dp, tint = CrundiColors.TextMuted, modifier = Modifier.padding(top = 2.dp))
    }
}

@Composable
private fun ColumnScope.NodePanel(st: FactoryEditorState, node: FactoryNode) {
    val f = st.factory
    val kind = FactoryEdit.KIND_LABEL[node.type] ?: node.type
    fun set(key: String = "", fn: (FactoryNode) -> FactoryNode) = st.mutate(key) { FactoryEdit.updateNode(it, node.id, fn) }
    PanelTitle(kind, onDelete = if (node.type !in FactoryEdit.FIXED) ({ st.deleteSelected() }) else null)
    IssueList(st.issuesFor(node = node.id))
    Field("Label", "Shown on the canvas; also how the team refers to this step.") {
        CrundiTextField(node.label, { v -> set("label:" + node.id) { it.copy(label = v.take(120)) } }, Modifier.fillMaxWidth(), placeholder = kind)
    }
    when (node.type) {
        "employee" -> {
            val employees = f.roster.filter { it.id != FactoryEdit.MANAGER_ID }
            Field("Who does this step") {
                ChipRow(employees.map { it.id to it.name.ifEmpty { "Unnamed" } } + ("__new" to "+ Add someone to the team"), node.employeeId.orEmpty()) { v ->
                    if (v == "__new") { st.rosterAddTo = node.id; st.rosterSel = null; st.tab = EditorTab.Team } else set { it.copy(employeeId = v) }
                }
            }
            if (!node.employeeId.isNullOrEmpty() && f.roster.any { it.id == node.employeeId }) CrundiButton("Edit their persona", { st.rosterSel = node.employeeId; st.tab = EditorTab.Team }, kind = ButtonKind.Ghost, icon = "user")
        }
        "manager" -> CrundiButton("Edit the Manager's persona", { st.rosterSel = FactoryEdit.MANAGER_ID; st.tab = EditorTab.Team }, kind = ButtonKind.Ghost, icon = "crown")
        "decision" -> {
            val d = node.decision ?: mv.obitech.crundi.api.model.FactoryDecision()
            Field("Question", "Answered at handoff. You can name skills with /skill-name.") {
                CrundiTextField(d.question, { v -> set("q:" + node.id) { it.copy(decision = d.copy(question = v)) } }, Modifier.fillMaxWidth(), placeholder = "Does it need a UI change?", singleLine = false, minLines = 2)
                MentionChips(d.question, st)
            }
            Field("Kind of answer") {
                ChipRow(listOf("yesno" to "Yes or no", "one" to "Pick one", "many" to "Pick any (runs them in parallel)"), d.kind) { k -> st.mutate { FactoryEdit.setDecisionKind(it, node.id, k) } }
            }
            if (d.kind != "yesno") Field("Options") {
                ChoiceList(d.options, 2, "Add option", { items -> set("opts:" + node.id) { it.copy(decision = d.copy(options = items)) } },
                    onRemove = { o -> st.mutate { FactoryEdit.removeOption(it, node.id, o) } }, onAdd = { st.mutate { FactoryEdit.addOption(it, node.id) } })
            }
            if (d.kind == "yesno") {
                Toggle(d.review, "This is a review gate (approval)") { v -> set { it.copy(decision = d.copy(review = v)) } }
                if (d.review) Hint("\"No\" needs at least one open blocking finding saying what must change; \"yes\" needs none left open. Findings carry over between rounds, so reviewers check last round's requests first, and nits become follow-ups instead of blocking.")
            }
            Field("Who answers", "Usually whoever arrives here. After a Join or an Ask me, someone has to be named.") {
                ChipRow(listOf("" to "Whoever arrives") + f.roster.map { it.id to it.name.ifEmpty { it.id } }, d.answeredBy.orEmpty()) { v -> set { it.copy(decision = d.copy(answeredBy = v.ifEmpty { null })) } }
            }
        }
        "ask" -> {
            val a = node.ask ?: mv.obitech.crundi.api.model.FactoryAsk()
            Field("What you are asked", "You get a notification and the run waits for your reply.") {
                CrundiTextField(a.prompt, { v -> set("ask:" + node.id) { it.copy(ask = a.copy(prompt = v)) } }, Modifier.fillMaxWidth(), placeholder = "Ship it?", singleLine = false, minLines = 2)
                MentionChips(a.prompt, st)
            }
            Field("Replies") {
                ChoiceList(a.replies, 1, "Add reply", { items -> set("rep:" + node.id) { it.copy(ask = a.copy(replies = items)) } },
                    onRemove = { o -> st.mutate { FactoryEdit.removeReply(it, node.id, o) } }, onAdd = { st.mutate { FactoryEdit.addReply(it, node.id) } })
            }
        }
        "wait" -> WaitFields(st, node)
        "join" -> {
            val forks = f.nodes.filter { it.type == "decision" && it.decision?.kind == "many" }
            Field("Joins the branches of", "The pick-any decision whose parallel branches meet here.") {
                if (forks.isEmpty()) Hint("There is no pick-any decision yet.")
                ChipRow(forks.map { it.id to (it.label.ifEmpty { null } ?: it.decision?.question?.ifEmpty { null } ?: "Decision") }, node.join?.forkId.orEmpty()) { v -> set { it.copy(join = mv.obitech.crundi.api.model.FactoryJoin(v)) } }
            }
        }
        "start" -> Hint("The task you give when starting the factory enters here and goes to the Manager.")
        "end" -> Hint("Only the Manager's Finish reaches here. The Manager then presents the result and you decide whether to merge it.")
    }
    Connections(st, node)
}

@Composable
private fun WaitFields(st: FactoryEditorState, node: FactoryNode) {
    val w = node.wait ?: mv.obitech.crundi.api.model.FactoryWait()
    fun set(key: String = "", fn: (mv.obitech.crundi.api.model.FactoryWait) -> mv.obitech.crundi.api.model.FactoryWait) = st.mutate(key) { FactoryEdit.updateNode(it, node.id) { n -> n.copy(wait = fn(n.wait ?: w)) } }
    Field("Wait for") {
        ChipRow(listOf(
            "command" to "A command to succeed (CI, a deploy, a health check)",
            "signal" to "A signal (a URL called by CI or a hook, or you)",
            "time" to "A length of time",
            "until" to "A time of day",
        ), w.kind) { k -> st.mutate { FactoryEdit.setWaitKind(it, node.id, k) } }
    }
    when (w.kind) {
        "time" -> Field("Minutes") { NumField("wm:" + node.id, w.minutes.toInt(), 1, 10080) { v -> set("wm:" + node.id) { it.copy(minutes = (if (v == 0) 1 else v).toDouble()) } } }
        "until" -> Field("Until (this server's time)", "The next time the clock says this; tomorrow if it has passed today.") {
            CrundiTextField(w.at, { v -> set("wa:" + node.id) { it.copy(at = v) } }, Modifier.width(120.dp), placeholder = "07:30", mono = true)
        }
        "command" -> {
            Field("Command", "Run in the run's worktree; the wait is over when it exits 0. E.g. gh run watch --exit-status, curl -fs localhost:4310/health, npm test.") {
                CrundiTextField(w.command, { v -> set("wc:" + node.id) { it.copy(command = v) } }, Modifier.fillMaxWidth(), placeholder = "gh run watch --exit-status", mono = true)
            }
            Field("Try every (seconds)") { NumField("we:" + node.id, w.every.toInt(), 15, 3600) { v -> set("we:" + node.id) { it.copy(every = (if (v == 0) 60 else v).toDouble()) } } }
        }
        "signal" -> Field("Waiting for", "Shown on the live graph. Each run gets a secret URL to POST to; you can also end the wait from the run.") {
            CrundiTextField(w.what, { v -> set("ww:" + node.id) { it.copy(what = v) } }, Modifier.fillMaxWidth(), placeholder = "The staging deploy to finish")
        }
    }
    if (w.kind == "command" || w.kind == "signal") Field("Give up after (minutes)", "Then it goes down Timed out, so the run never waits forever.") {
        NumField("wt:" + node.id, w.timeout.toInt(), 1, 10080) { v -> set("wt:" + node.id) { it.copy(timeout = (if (v == 0) 60 else v).toDouble()) } }
    }
    Hint("What came in (the last summary, any note) goes on to whoever is next, with what happened during the wait. Waits survive a restart.")
}

/** The phone's stand-in for dragging from a handle: where each output leads, and Connect. */
@Composable
private fun Connections(st: FactoryEditorState, node: FactoryNode) {
    val f = st.factory
    val outs = FactoryEdit.connectionsOf(f, node)
    val notes = f.edges.filter { it.kind == "note" && it.source == node.id }
    if (outs.isEmpty() && node.type !in FactoryEdit.AGENTS) return
    Subtitle("Where it goes")
    for ((h, e) in outs) Row(
        Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgCard).border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm)
            .then(if (e != null) Modifier.clickable { st.select(EditSel.Edge(e.id)) } else Modifier).padding(horizontal = 10.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(outLabel(node, h), style = CrundiType.Small.copy(color = CrundiColors.TextSecondary, fontWeight = FontWeight.Medium), modifier = Modifier.width(76.dp), maxLines = 1, overflow = TextOverflow.Ellipsis)
        val target = e?.let { x -> f.nodes.firstOrNull { it.id == x.target } }
        Text(
            if (target != null) "→ " + FactoryEdit.nodeName(f, target) + if (e.maxRounds > 0) " · max ${e.maxRounds}" else "" else if (h == "out" && node.type == "manager") "Optional" else "Nowhere yet",
            style = CrundiType.Small.copy(color = if (target != null) CrundiColors.TextPrimary else CrundiColors.TextMuted), modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis,
        )
        if (h != "finish") CrundiButton(if (e == null) "Connect" else "Change", { st.startConnect(node.id, h) }, kind = ButtonKind.Ghost)
    }
    if (node.type in FactoryEdit.AGENTS) {
        for (e in notes) Row(
            Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgCard).border(Dimens.Hairline, CrundiColors.Yellow.copy(alpha = 0.4f), Dimens.ShapeSm).clickable { st.select(EditSel.Edge(e.id)) }.padding(horizontal = 10.dp, vertical = 10.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text("Note", style = CrundiType.Small.copy(color = CrundiColors.Yellow, fontWeight = FontWeight.Medium), modifier = Modifier.width(76.dp))
            Text("→ " + FactoryEdit.nodeName(f, f.nodes.firstOrNull { it.id == e.target }) + if (e.noteLabel.isNotEmpty()) ": " + e.noteLabel else "", style = CrundiType.Small.copy(color = CrundiColors.TextPrimary), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
        }
        CrundiButton("Leave a note for someone", { st.startConnect(node.id, "note") }, kind = ButtonKind.Ghost, icon = "message-square")
    }
}

// ═══ Edit: a connection (Inspector.jsx EdgeInspector) ═══

@Composable
private fun ColumnScope.EdgePanel(st: FactoryEditorState, edge: FactoryEdge) {
    val f = st.factory
    fun set(key: String = "", fn: (FactoryEdge) -> FactoryEdge) = st.mutate(key) { FactoryEdit.updateEdge(it, edge.id, fn) }
    val src = f.nodes.firstOrNull { it.id == edge.source }
    val dst = f.nodes.firstOrNull { it.id == edge.target }
    val note = edge.kind == "note"
    PanelTitle(if (note) "Note" else "Connection", onDelete = if (edge.sourceHandle != "finish") ({ st.deleteSelected() }) else null)
    Hint(FactoryEdit.nodeName(f, src) + (if (!note && src != null && edge.sourceHandle != "out") " (${FactoryEdit.handleLabel(src, edge.sourceHandle)})" else "") + " → " + FactoryEdit.nodeName(f, dst))
    IssueList(st.issuesFor(edge = edge.id))
    if (note) {
        Hint("A note carries a message, not control: it waits in ${FactoryEdit.nodeName(f, dst)}'s inbox until the flow reaches them.")
        Field("What the note is for") { CrundiTextField(edge.noteLabel, { v -> set("nl:" + edge.id) { it.copy(noteLabel = v.take(500)) } }, Modifier.fillMaxWidth(), placeholder = "Instructions for reviewers") }
    } else {
        Field("Round limit", "Set it on the connection that goes back round a loop. When it runs out, the Manager takes over. Empty means no limit.") {
            NumField("mr:" + edge.id, edge.maxRounds, 0, 100, placeholder = "No limit") { v -> set("mr:" + edge.id) { it.copy(maxRounds = v) } }
        }
        Toggle(edge.requireNote, "Require a note on this route") { v -> set { it.copy(requireNote = v) } }
        if (edge.requireNote) Field("What the note says") { CrundiTextField(edge.noteLabel, { v -> set("nl:" + edge.id) { it.copy(noteLabel = v.take(500)) } }, Modifier.fillMaxWidth(), placeholder = "What to change") }
        if (src != null && edge.sourceHandle != "finish") CrundiButton("Lead it somewhere else", { st.startConnect(src.id, edge.sourceHandle) }, kind = ButtonKind.Ghost, icon = "git-branch")
    }
}

// ═══ Edit: the factory (Inspector.jsx FactorySettings, CrTemplate.jsx) ═══

@Composable
private fun ColumnScope.SettingsPanel(st: FactoryEditorState) {
    val f = st.factory
    val s = f.settings
    fun set(key: String = "", fn: (FactorySettings) -> FactorySettings) = st.mutate(key) { it.copy(settings = fn(it.settings)) }
    PanelTitle("Factory")
    Field("Name") { CrundiTextField(f.name, { v -> st.mutate("name") { it.copy(name = v.take(80)) } }, Modifier.fillMaxWidth()) }
    Field("Description", "What this factory is for. /skill-name works here too.") {
        CrundiTextField(f.description, { v -> st.mutate("desc") { it.copy(description = v) } }, Modifier.fillMaxWidth(), singleLine = false, minLines = 3)
        MentionChips(f.description, st)
    }
    if (st.has("crTemplate")) CrTemplateEditor(st)
    Field("Manager's permissions", if (s.managerMode == "bypass") "The Manager runs unchecked and approves its team's requests itself. Choose this only if you trust it. Some things stay refused: sudo, restarting Crundi, pushing, the server's ports." else "The Manager's own actions ask you. It still decides its team's permission requests.") {
        ChipRow(listOf("normal" to "Normal: ask me for the Manager's own actions", "bypass" to "Bypass: I trust the Manager"), s.managerMode) { v -> set { it.copy(managerMode = v) } }
    }
    val teamBypass = st.has("teamMode") && s.teamMode == "bypass"
    if (st.has("teamMode")) {
        Field(
            "The team's permissions",
            if (s.teamMode == "bypass") "The team runs unchecked too: nobody asks for anything, which is fastest. The fixed refusals still apply, but nothing stops them writing outside their worktree, so only for projects you trust them with."
            else if (s.gatekeeper.enabled) "Requests go to the Gatekeeper, which passes the unclear ones to the Manager." else "Requests go to the Manager, whose decision is final.",
        ) {
            ChipRow(listOf("ask" to "Ask the Manager" + if (s.gatekeeper.enabled) " (through the Gatekeeper)" else "", "bypass" to "Bypass too"), s.teamMode) { v -> set { it.copy(teamMode = v) } }
        }
        if (s.managerMode == "bypass" && s.teamMode != "bypass") Column(
            Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgCard).border(Dimens.Hairline, CrundiColors.Yellow.copy(alpha = 0.5f), Dimens.ShapeSm).padding(10.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Text("You trust the Manager with bypass, and it would approve most of its team's requests anyway. Giving the team bypass too saves every one of those round trips.", style = CrundiType.Small.copy(color = CrundiColors.TextPrimary))
            CrundiButton("Give the team bypass too", { set { it.copy(teamMode = "bypass") } })
        }
    }
    if (!teamBypass) {
        if (st.has("presets")) Field("Everyone may, without asking", "Each person can have more in the Team panel.") {
            for (p in st.data.presets) Toggle(p.key in s.presets, p.label) { v -> set { it.copy(presets = if (v) (it.presets + p.key).distinct() else it.presets - p.key) } }
        }
        if (st.has("rules")) Field("More rules for everyone", "One per line, Claude Code style: Bash(docker compose up:*), WebFetch(domain:docs.example.com). Too broad or forbidden rules are ignored.") {
            LinesField("rules", s.rules) { v -> set("rules") { it.copy(rules = v) } }
        }
        if (st.has("managerRules")) {
            Toggle(s.managerRules, "The Manager may set standing rules") { v -> set { it.copy(managerRules = v) } }
            Hint(if (s.managerRules) "So it is not asked the same thing again and again: a rule it grants applies to that person (or everyone, if it says so) for the rest of the run." else "It answers every request on its own.")
        }
        if (st.has("gatekeeper")) {
            Toggle(s.gatekeeper.enabled, "A Gatekeeper answers requests first") { v -> set { it.copy(gatekeeper = it.gatekeeper.copy(enabled = v)) } }
            if (s.gatekeeper.enabled) Field("Gatekeeper model", "A small, cheap model is enough: it follows the Manager's policy and passes the unclear requests on, so the Manager's own conversation stays on the work.") {
                ChipRow(listOf("haiku" to "Haiku", "sonnet" to "Sonnet", "opus" to "Opus"), s.gatekeeper.model) { v -> set { it.copy(gatekeeper = it.gatekeeper.copy(model = v)) } }
            }
        }
    }
    Field("When the work is done", "Either way it is a single squash commit, and nothing is ever pushed.") {
        ChipRow(listOf("ask" to "Show me first, I decide", "auto" to "Merge automatically (with Undo)"), s.mergeMode) { v -> set { it.copy(mergeMode = v) } }
    }
    if (st.has("autoRecover")) Toggle(s.autoRecover, "Recover by itself after a crash") { v -> set { it.copy(autoRecover = v) } }
    Field("Setup command", "Runs in each new worktree before the team starts, e.g. npm ci.") {
        CrundiTextField(s.setupCommand, { v -> set("setup") { it.copy(setupCommand = v.take(500)) } }, Modifier.fillMaxWidth(), placeholder = "npm ci", mono = true)
    }
    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Field("Max handoffs", modifier = Modifier.weight(1f)) { NumField("caps1", s.caps.maxHandoffs, 1, 10000) { v -> set("caps1") { it.copy(caps = it.caps.copy(maxHandoffs = if (v == 0) 1 else v)) } } }
        Field("Max minutes", modifier = Modifier.weight(1f)) { NumField("caps2", s.caps.maxMinutes, 1, 10080) { v -> set("caps2") { it.copy(caps = it.caps.copy(maxMinutes = if (v == 0) 1 else v)) } } }
    }
    Hint("When a limit is reached the run waits for you; you can let it carry on.")
    if (st.has("tickMinutes")) Field(
        "Manager check-in every (minutes)",
        if (s.tickMinutes > 0) "A wake-up every ${s.tickMinutes} minutes, only while the Manager is idle; skipped otherwise. Each one is a short Manager turn."
        else "Blank: the Manager chooses at the start of each run (5 minutes to 24 hours). A wake-up only while it is idle, so it can check on things.",
    ) {
        NumField("tick", s.tickMinutes, 0, 1440, placeholder = "The Manager chooses", blankZero = true) { v -> set("tick") { it.copy(tickMinutes = if (v > 0) v.coerceIn(5, 1440) else 0) } }
    }
    if (st.has("limits")) {
        Subtitle("Usage")
        Hint("Your Claude usage is shared with your own chats. These keep a run inside what you choose; each can also be changed on a running run.")
        val l = s.limits
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Field("5-hour, pause at %", modifier = Modifier.weight(1f)) { NumField("lim1", l.pauseAt5h, 50, 100) { v -> set("lim1") { it.copy(limits = it.limits.copy(pauseAt5h = if (v == 0) 100 else v)) } } }
            Field("Weekly, pause at %", modifier = Modifier.weight(1f)) { NumField("lim2", l.pauseAtWeek, 50, 100) { v -> set("lim2") { it.copy(limits = it.limits.copy(pauseAtWeek = if (v == 0) 100 else v)) } } }
        }
        Hint(if (l.pauseAt5h < 100 || l.pauseAtWeek < 100) "It pauses there and carries on by itself after the window resets, leaving the rest for you." else "At 100% it pauses only when a window is full, and carries on by itself after the reset.")
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Field("People at once", "0: no limit", modifier = Modifier.weight(1f)) { NumField("lim3", l.maxParallel, 0, 20) { v -> set("lim3") { it.copy(limits = it.limits.copy(maxParallel = v)) } } }
            Field("One at a time above %", "Of the 5-hour. 0: off", modifier = Modifier.weight(1f)) { NumField("lim4", l.oneAtATimeAbove, 0, 100) { v -> set("lim4") { it.copy(limits = it.limits.copy(oneAtATimeAbove = v)) } } }
        }
        Hint("Parallel steps beyond the limit wait their turn instead of all running together, which spreads the load. Each person's model is in the Team panel: a cheaper model for routine roles stretches usage too.")
    }
}

@Composable
private fun CrTemplateEditor(st: FactoryEditorState) {
    var open by remember { mutableStateOf(false) }
    val t = st.factory.settings.crTemplate
    val lim = st.data.crLimits
    fun set(key: String = "", fn: (List<CrField>) -> List<CrField>) = st.mutate(key) { it.copy(settings = it.settings.copy(crTemplate = fn(it.settings.crTemplate))) }
    fun move(i: Int, by: Int) = set { a -> val j = i + by; if (j < 0 || j >= a.size) a else a.toMutableList().also { m -> val x = m[i]; m[i] = m[j]; m[j] = x } }
    val isDefault = t.map { it.copy() } == st.data.crTemplate
    Column(Modifier.fillMaxWidth().clip(Dimens.ShapeSm).border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm)) {
        Row(Modifier.fillMaxWidth().clickable { open = !open }.padding(horizontal = 10.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("Change request form", style = CrundiType.Body.copy(fontWeight = FontWeight.Medium), modifier = Modifier.weight(1f))
            CrundiIcon(if (open) "chevron-up" else "chevron-down", size = 16.dp, tint = CrundiColors.TextSecondary)
        }
        if (open) Column(Modifier.padding(start = 10.dp, end = 10.dp, bottom = 10.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Hint("What the person fills in to start a run. The Manager gets all of it; every step gets the title, the goal and the \"done\" checks. The title and attachments are always there.")
            t.forEachIndexed { i, sec ->
                Column(Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgCard).padding(8.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        CrundiTextField(sec.label, { v -> set("crl" + sec.id) { a -> a.mapIndexed { k, x -> if (k == i) x.copy(label = v.take(lim.label)) else x } } }, Modifier.weight(1f), placeholder = "Section name")
                        CrundiIconButton("chevron-up", { move(i, -1) }, contentDescription = "Move up", enabled = i > 0, touch = 36.dp)
                        CrundiIconButton("chevron-down", { move(i, 1) }, contentDescription = "Move down", enabled = i < t.size - 1, touch = 36.dp)
                        CrundiIconButton("trash", { set { a -> a.filterIndexed { k, _ -> k != i } } }, contentDescription = "Remove", enabled = t.size > 1, touch = 36.dp)
                    }
                    CrundiTextField(sec.hint, { v -> set("crh" + sec.id) { a -> a.mapIndexed { k, x -> if (k == i) x.copy(hint = v.take(lim.hint)) else x } } }, Modifier.fillMaxWidth(), placeholder = "Hint: what to write here")
                    Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                        Toggle(sec.required, "Required") { v -> set { a -> a.mapIndexed { k, x -> if (k == i) x.copy(required = v) else x } } }
                        Toggle(sec.list, "A list") { v -> set { a -> a.mapIndexed { k, x -> if (k == i) x.copy(list = v) else x } } }
                    }
                }
            }
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                CrundiButton("Add a section", { set { a -> a + CrField(FactoryEdit.genId("s"), "New section", "", false, false) } }, icon = "plus", enabled = t.size < lim.sections)
                if (!isDefault && st.data.crTemplate.isNotEmpty()) CrundiButton("Back to the default", { set { st.data.crTemplate } }, kind = ButtonKind.Ghost)
            }
        }
    }
}

// ═══ Team (Roster.jsx) ═══

@Composable
private fun ColumnScope.TeamPanel(st: FactoryEditorState) {
    val f = st.factory
    val r = f.roster.firstOrNull { it.id == st.rosterSel }
    if (r == null) {
        PanelTitle("Team")
        for (x in f.roster) {
            val iss = st.issuesFor(employee = x.id).size
            val steps = FactoryEdit.stepsOf(f, x.id)
            Row(
                Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgCard).border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm).clickable { st.rosterSel = x.id }.padding(horizontal = 12.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                CrundiIcon(if (x.id == FactoryEdit.MANAGER_ID) "crown" else "user", size = 15.dp, tint = if (x.id == FactoryEdit.MANAGER_ID) CrundiColors.Yellow else CrundiColors.TextSecondary)
                Text(x.name.ifEmpty { "Unnamed" }, style = CrundiType.Body.copy(fontWeight = FontWeight.Medium), modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text((if (x.id == FactoryEdit.MANAGER_ID) "Manager" else "$steps step${if (steps == 1) "" else "s"}") + if (iss > 0) " · $iss to check" else "", style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, color = if (iss > 0) CrundiColors.Yellow else CrundiColors.TextMuted))
            }
        }
        Subtitle(if (st.rosterAddTo != null) "Add someone for this step" else "Add someone")
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            for (p in st.data.personas) CrundiButton(p.name, { addPerson(st, p) }, icon = "plus", kind = ButtonKind.Secondary)
            CrundiButton("Blank", { addPerson(st, null) }, icon = "plus", kind = ButtonKind.Secondary)
        }
        Hint("Templates are starting points: change the persona however you like.")
        return
    }
    val isManager = r.id == FactoryEdit.MANAGER_ID
    fun set(key: String = "", fn: (FactoryMember) -> FactoryMember) = st.mutate(key) { FactoryEdit.updateMember(it, r.id, fn) }
    PanelTitle(if (isManager) "Manager" else "Employee", onDelete = if (!isManager) ({ st.removing = r.id }) else null, deleteLabel = "Remove from the team", onBack = { st.rosterSel = null; st.rosterAddTo = null })
    IssueList(st.issuesFor(employee = r.id))
    Field("Name") { CrundiTextField(r.name, { v -> set("rn:" + r.id) { it.copy(name = v.take(80)) } }, Modifier.fillMaxWidth()) }
    Field("Persona", "Who they are and how they work. They read it and act as it says. /skill-name names a skill they should use.") {
        CrundiTextField(r.persona, { v -> set("rp:" + r.id) { it.copy(persona = v) } }, Modifier.fillMaxWidth(), singleLine = false, minLines = 6, maxLines = 14)
        MentionChips(r.persona, st)
    }
    Field("Model") { ChipRow(FactoryEdit.MODELS, r.model) { v -> set { it.copy(model = v) } } }
    Field("Effort") { ChipRow(FactoryEdit.EFFORTS, r.effort) { v -> set { it.copy(effort = v) } } }
    if (!isManager) {
        Field("Memory", if (r.memory == "fresh") "A new session each time: useful for reviewers, who should look with fresh eyes." else "One session for the whole run: they remember earlier steps.") {
            ChipRow(listOf("keep" to "Keeps memory", "fresh" to "Fresh each time"), r.memory) { v -> set { it.copy(memory = v) } }
        }
        Toggle(FactoryEdit.canEditFiles(r), "Can edit files") { v -> set { FactoryEdit.setCanEditFiles(it, v) } }
    }
    Field("Pinned skills", "Comma separated. Same as naming them in the persona.") {
        TextListField("rs:" + r.id, r.skills, "frontend-design, run", stripSlash = true) { v -> set("rs:" + r.id) { it.copy(skills = v) } }
        val offer = st.skills.map { it.name }.distinct().filter { it !in r.skills }
        if (offer.isNotEmpty()) {
            Hint("On this machine (tap to pin):")
            Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                for (n in offer.take(40)) mv.obitech.crundi.ui.components.CrundiChip("/$n", false, { set { it.copy(skills = it.skills + n) } })
            }
        }
    }
    val s = f.settings
    if (!isManager && st.hasMemberPresets && !(st.has("teamMode") && s.teamMode == "bypass")) Field("This person may also, without asking", "On top of what everyone may (factory settings).") {
        for (p in st.data.presets) {
            val all = p.key in s.presets
            Toggle(p.key in r.presets || all, p.label + if (all) " (everyone has this)" else "") { v -> if (!all) set { it.copy(presets = if (v) (it.presets + p.key).distinct() else it.presets - p.key) } }
        }
    }
    var more by remember(r.id) { mutableStateOf(false) }
    Row(Modifier.fillMaxWidth().clip(Dimens.ShapeSm).clickable { more = !more }.padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
        Text("More tool rules", style = CrundiType.Small.copy(color = CrundiColors.TextSecondary, fontWeight = FontWeight.Medium), modifier = Modifier.weight(1f))
        CrundiIcon(if (more) "chevron-up" else "chevron-down", size = 14.dp, tint = CrundiColors.TextSecondary)
    }
    if (more) {
        Field("Always allowed (this person only)", "Claude Code rules, comma separated, e.g. Bash(npm test:*).") {
            TextListField("ta:" + r.id, r.tools.allow, "", mono = true) { v -> set("ta:" + r.id) { it.copy(tools = it.tools.copy(allow = v)) } }
        }
        Field("Never allowed") { TextListField("td:" + r.id, r.tools.deny, "", mono = true) { v -> set("td:" + r.id) { it.copy(tools = it.tools.copy(deny = v)) } } }
    }
    if (!isManager) { val n = FactoryEdit.stepsOf(f, r.id); Hint("In $n step${if (n == 1) "" else "s"} of the flow.") }
}

private fun addPerson(st: FactoryEditorState, p: PersonaTemplate?) {
    val m = FactoryEdit.newEmployee(st.factory, p, st.skillNames.takeIf { it.isNotEmpty() })
    val to = st.rosterAddTo
    st.mutate { FactoryEdit.addEmployee(it, m, to) }
    st.rosterSel = m.id
    st.rosterAddTo = null
}

// ═══ Issues (Inspector.jsx IssuesPane) ═══

@Composable
private fun ColumnScope.IssuesPanel(v: FactoryValidation, onPick: (FactoryIssue) -> Unit) {
    val all = v.errors.map { it to true } + v.warnings.map { it to false }
    PanelTitle(if (v.ok) "Ready to run" else "${v.errors.size} to fix")
    when {
        v.ok && all.isEmpty() -> Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiIcon("check", size = 14.dp, tint = CrundiColors.Green, modifier = Modifier.padding(top = 2.dp))
            Text("Every path can finish, every loop has a limit, every answer is wired.", style = CrundiType.Small.copy(color = CrundiColors.Green))
        }
        v.ok -> Hint("It can run. The warnings below are worth a look.")
        else -> Hint("It is saved as a draft and cannot run until these are fixed. Tap one to go to it.")
    }
    IssueList(all, onPick)
    Hint("Checked by the server each time the factory saves.")
}

// ═══ Fields (fields.jsx) ═══

@Composable
private fun Field(label: String, hint: String? = null, modifier: Modifier = Modifier.fillMaxWidth(), content: @Composable ColumnScope.() -> Unit) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(5.dp)) {
        FieldLabel(label)
        content()
        if (hint != null) Hint(hint)
    }
}

@Composable
private fun Hint(text: String) = Text(text, style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans))

@Composable
private fun Subtitle(text: String) = Text(text, style = CrundiType.Small.copy(color = CrundiColors.TextPrimary, fontWeight = FontWeight.SemiBold), modifier = Modifier.padding(top = 4.dp))

/** A checkbox and its words, as the web's ff-toggle. */
@Composable
private fun Toggle(checked: Boolean, label: String, onChange: (Boolean) -> Unit) {
    Row(Modifier.clip(Dimens.ShapeSm).clickable { onChange(!checked) }.padding(vertical = 5.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        PlanningCheckbox(checked, { onChange(!checked) }, 18.dp)
        Text(label, style = CrundiType.Small.copy(color = CrundiColors.TextPrimary))
    }
}

/**
 * A whole number, as the web's Num: kept between [min] and [max]; empty is 0.
 * What is typed stays as typed while it is being typed.
 */
@Composable
private fun NumField(key: String, value: Int, min: Int, max: Int, placeholder: String = "", blankZero: Boolean = false, onChange: (Int) -> Unit) {
    var text by remember(key) { mutableStateOf(if (blankZero && value == 0) "" else value.toString()) }
    val shown = if ((text.toIntOrNull() ?: 0).coerceIn(min, max) == value || text.isEmpty()) text else value.toString()
    CrundiTextField(
        shown, { v -> text = v.filter { it.isDigit() }.take(6); onChange(if (text.isEmpty()) 0 else text.toInt().coerceIn(min, max)) },
        Modifier.width(120.dp), placeholder = placeholder, mono = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
    )
}

/** A comma separated list (skills, tool rules). */
@Composable
private fun TextListField(key: String, value: List<String>, placeholder: String, mono: Boolean = false, stripSlash: Boolean = false, onChange: (List<String>) -> Unit) {
    var text by remember(key) { mutableStateOf(value.joinToString(", ")) }
    val shown = if (FactoryEdit.splitList(text, stripSlash) == value) text else value.joinToString(", ")
    CrundiTextField(shown, { v -> text = v; onChange(FactoryEdit.splitList(v, stripSlash)) }, Modifier.fillMaxWidth(), placeholder = placeholder, mono = mono)
}

/** One per line (the factory's rules). */
@Composable
private fun LinesField(key: String, value: List<String>, onChange: (List<String>) -> Unit) {
    var text by remember(key) { mutableStateOf(value.joinToString("\n")) }
    val parse = { v: String -> v.split('\n').map { it.trim() }.filter { it.isNotEmpty() } }
    val shown = if (parse(text) == value) text else value.joinToString("\n")
    CrundiTextField(shown, { v -> text = v; onChange(parse(v)) }, Modifier.fillMaxWidth(), mono = true, singleLine = false, minLines = 3)
}

/** Decision options, Ask me replies: a label each, remove (down to [min]), add. */
@Composable
private fun ChoiceList(items: List<Option>, min: Int, addLabel: String, onChange: (List<Option>) -> Unit, onRemove: (String) -> Unit, onAdd: () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        items.forEachIndexed { i, it ->
            Row(verticalAlignment = Alignment.CenterVertically) {
                CrundiTextField(it.label, { v -> onChange(items.map { x -> if (x.id == it.id) x.copy(label = v.take(120)) else x }) }, Modifier.weight(1f), placeholder = "Choice ${i + 1}")
                CrundiIconButton("x", { onRemove(it.id) }, contentDescription = "Remove", enabled = items.size > min, touch = 40.dp)
            }
        }
        CrundiButton(addLabel, onAdd, kind = ButtonKind.Ghost, icon = "plus")
    }
}

/** The skills a text names, marked when this machine does not have them (fields.jsx SkillText's chips). */
@Composable
private fun MentionChips(text: String, st: FactoryEditorState) {
    val names = FactoryEdit.findMentions(text)
    if (names.isEmpty()) return
    FlowRow(horizontalArrangement = Arrangement.spacedBy(5.dp), verticalArrangement = Arrangement.spacedBy(5.dp)) {
        for (n in names) {
            val known = st.skillNames.isEmpty() || n in st.skillNames
            Row(
                Modifier.clip(androidx.compose.foundation.shape.RoundedCornerShape(6.dp)).background(if (known) CrundiColors.GreenDim else CrundiColors.BgCard)
                    .border(Dimens.Hairline, (if (known) CrundiColors.Green else CrundiColors.Yellow).copy(alpha = 0.5f), androidx.compose.foundation.shape.RoundedCornerShape(6.dp)).padding(horizontal = 6.dp, vertical = 2.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                CrundiIcon(if (known) "check" else "alert-triangle", size = 11.dp, tint = if (known) CrundiColors.Green else CrundiColors.Yellow)
                Text("/$n", style = CrundiType.Meta.copy(color = if (known) CrundiColors.Green else CrundiColors.Yellow))
            }
        }
    }
    if (names.any { st.skillNames.isNotEmpty() && it !in st.skillNames }) Hint("Marked: not installed for all projects. It will need to exist in the project you run this in.")
}

/** Kept for the FactoryDetail page: where an issue is, for the editor to open there. */
fun issueFocus(i: FactoryIssue): String = when {
    i.employeeId != null -> "emp:" + i.employeeId
    i.nodeId != null -> "node:" + i.nodeId
    i.nodeIds.isNotEmpty() -> "node:" + i.nodeIds.first()
    i.edgeId != null -> "edge:" + i.edgeId
    else -> ""
}
