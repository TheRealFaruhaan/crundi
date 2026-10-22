package mv.obitech.crundi.ui.planning

import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import mv.obitech.crundi.ui.components.errorText

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.foundation.gestures.detectTransformGestures
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
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.layoutId
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import mv.obitech.crundi.api.model.KanbanBoard
import mv.obitech.crundi.api.model.MindMap
import mv.obitech.crundi.api.model.MindNode
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
import mv.obitech.crundi.ui.components.SectionHeader
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.shell.Area
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens
import kotlinx.coroutines.launch
import kotlin.math.max
import kotlin.math.min

@Stable
class MindMapCallbacks(
    val onAdd: (text: String, parentId: String?) -> Unit = { _, _ -> },
    val onSave: (node: MindNode, text: String, notes: List<String>) -> Unit = { _, _, _ -> },
    val onDelete: (MindNode) -> Unit = {},
    val onMove: (node: MindNode, parentId: String?, index: Int?) -> Unit = { _, _, _ -> },
    val onScope: (MindNode, String?) -> Unit = { _, _ -> },
    val onLink: (node: MindNode, project: String, taskId: String, todoId: String?) -> Unit = { _, _, _, _ -> },
    val onUnlink: (MindNode) -> Unit = {},
    /** The link picker wants a project's board. */
    val onNeedBoard: (String) -> Unit = {},
    val onOpenKanban: () -> Unit = {},
    val onRetry: () -> Unit = {},
)

/** What is open over the map. Screen-local; tests can start with one. */
sealed interface MindPopup {
    data class Details(val id: String) : MindPopup
    data class Edit(val id: String) : MindPopup
    data class Add(val parentId: String?) : MindPopup
    data class Link(val id: String) : MindPopup
    data class Move(val id: String) : MindPopup
    data class Delete(val id: String) : MindPopup
}

// ─── Route ───

@Composable
fun MindMapRoute() {
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val shell by LocalAppServices.current.shell.state.collectAsStateWithLifecycle()
    val loader = rememberLoader(Unit, events = arrayOf("mindmap", "kanban")) { api.mindmap.get() }
    val actions = rememberActions { loader.refresh() }
    val boards = remember { androidx.compose.runtime.mutableStateMapOf<String, Load<KanbanBoard>>() }
    val scope = androidx.compose.runtime.rememberCoroutineScope()
    val cb = remember {
        MindMapCallbacks(
            onAdd = { t, p -> actions.run { api.mindmap.add(t, p) } },
            onSave = { n, t, notes -> actions.run { api.mindmap.update(n.id, t, notes) } },
            onDelete = { n -> actions.run { api.mindmap.delete(n.id) } },
            onMove = { n, p, i -> actions.run { api.mindmap.move(n.id, p, i) } },
            onScope = { n, p -> actions.run { api.mindmap.scope(n.id, p) } },
            onLink = { n, p, t, d -> actions.run { api.mindmap.link(n.id, p, t, d) } },
            onUnlink = { n -> actions.run { api.mindmap.unlink(n.id) } },
            onNeedBoard = { p ->
                if (boards[p] !is Load.Ready) {
                    boards[p] = Load.Loading
                    scope.launch { boards[p] = try { Load.Ready(api.kanban.board(p)) } catch (e: mv.obitech.crundi.api.ApiException) { Load.Failed(errorText(e)) } }
                }
            },
            onOpenKanban = { nav.area(Area.Kanban) },
            onRetry = { loader.refresh() },
        )
    }
    Box(Modifier.fillMaxSize()) {
        MindMapScreen(loader.load, shell.projects.map { it.alias }, shell.currentProject, cb, boards)
        ActionError(actions)
    }
}


// ─── Layout ───

/** Where each shown idea sits, in dp, the tree growing left to right as on the desktop. */
object MindLayout {
    const val COL = 200f
    const val WIDTH = 168f
    const val GAP = 16f
    const val PAD = 24f
    const val ADD_H = 40f

    data class Place(val id: String, val x: Float, val y: Float, val h: Float, val parent: String?)

    /**
     * [heights] by id (dp; unknown = 48). [children] gives the shown children of an id
     * (ids starting with "add:" are add boxes). Returns places keyed by id and the total height.
     */
    fun layout(roots: List<String>, children: (String) -> List<String>, heights: (String) -> Float): Pair<Map<String, Place>, Float> {
        val out = LinkedHashMap<String, Place>()
        var cursor = PAD
        fun shift(id: String, by: Float) {
            out[id]?.let { out[id] = it.copy(y = it.y + by) }
            if (!id.startsWith("add:")) for (c in children(id)) shift(c, by)
        }
        fun place(id: String, depth: Int, parent: String?) {
            val h = heights(id)
            val kids = if (id.startsWith("add:")) emptyList() else children(id)
            val x = PAD + depth * COL
            if (kids.isEmpty()) {
                out[id] = Place(id, x, cursor + h / 2, h, parent)
                cursor += h + GAP
                return
            }
            val start = cursor
            for (k in kids) place(k, depth + 1, id)
            var y = (out[kids.first()]!!.y + out[kids.last()]!!.y) / 2
            if (y - h / 2 < start) {
                val by = start - (y - h / 2)
                for (k in kids) shift(k, by)
                y += by; cursor += by
            }
            out[id] = Place(id, x, y, h, parent)
            cursor = max(cursor, y + h / 2 + GAP)
        }
        for (r in roots) place(r, 0, null)
        return out to cursor + PAD - GAP
    }
}

// ─── Screen ───

@Composable
fun MindMapScreen(
    map: Load<MindMap>,
    projects: List<String> = emptyList(),
    currentProject: String? = null,
    cb: MindMapCallbacks = MindMapCallbacks(),
    boards: Map<String, Load<KanbanBoard>> = emptyMap(),
    initialPopup: MindPopup? = null,
    initialFilter: String = "",
    initialSelected: String? = null,
) {
    var popup by remember { mutableStateOf(initialPopup) }
    var filter by remember { mutableStateOf(initialFilter) }
    val hidden = remember { mutableStateListOf<String>() }
    val collapsed = remember { mutableStateListOf<String>() }
    var selected by remember { mutableStateOf(initialSelected) }
    // Typed in place, as on the desktop: the idea being edited, and where a
    // new one is being typed (drafting; draftParent null = top level).
    var editing by remember { mutableStateOf<String?>(null) }
    var drafting by remember { mutableStateOf(false) }
    var draftParent by remember { mutableStateOf<String?>(null) }
    fun startDraft(parent: String?) { editing = null; draftParent = parent; drafting = true; if (parent != null) selected = parent }
    Column(Modifier.fillMaxSize()) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiButton("Add idea", { startDraft(null) }, kind = ButtonKind.Primary, icon = "plus")
            CrundiTextField(filter, { filter = it }, Modifier.weight(1f), placeholder = "Filter ideas…", icon = "search")
        }
        val m = map.valueOrNull
        if (m != null) ProjectBar(m, hidden)
        CrundiDivider()
        when (map) {
            is Load.Loading -> LoadingState(text = "Loading the mind map")
            is Load.Failed -> ErrorState(map.message, onRetry = cb.onRetry)
            is Load.Ready -> if (map.value.nodes.isEmpty() && !drafting) EmptyState(
                "mindmap", "No ideas yet",
                body = "Add one, or use “Brainstorm in the Mind map” on a Kanban card to extend a task here.",
                action = "Add idea", onAction = { startDraft(null) },
            ) else MindCanvas(
                map.value, filter.trim(), hidden, collapsed, selected,
                editing = editing, draft = if (drafting) (draftParent ?: "") else null,
                // One tap edits the idea where it is; two (or a long press) open its details.
                onTap = { drafting = false; selected = it; editing = it },
                onOpen = { drafting = false; editing = null; selected = it; popup = MindPopup.Details(it) },
                onAdd = { startDraft(it) },
                onEdited = { n, text -> if (editing == n.id) editing = null; if (text != null && text.isNotBlank() && text.trim() != n.text) cb.onSave(n, text.trim(), n.notes) },
                onMoveTo = { n, parent -> cb.onMove(n, parent, null) },
                onDrafted = { text -> drafting = false; if (text != null && text.isNotBlank()) cb.onAdd(text.trim(), draftParent) },
            )
        }
    }
    val m = map.valueOrNull ?: return
    fun node(id: String) = m.nodes.firstOrNull { it.id == id }
    when (val p = popup) {
        null -> {}
        // "Add child" from an idea's details: the details close and the new
        // idea is typed on the map, where it will sit.
        is MindPopup.Add -> LaunchedEffect(p) { popup = null; startDraft(p.parentId) }
        is MindPopup.Details -> node(p.id)?.let { n ->
            PlanningSheet(n.text, { popup = null }) { IdeaDetails(m, n, cb, { popup = it }, collapsed) }
        }
        is MindPopup.Edit -> node(p.id)?.let { n -> Dialog({ popup = null }) { IdeaEditCard(n, { popup = MindPopup.Details(n.id) }) { t, notes -> cb.onSave(n, t, notes); popup = MindPopup.Details(n.id) } } }
        is MindPopup.Link -> node(p.id)?.let { n ->
            PlanningSheet("Scope to a project — or link a task", { popup = null }) { LinkPicker(m, n, projects, currentProject, boards, cb) { popup = null } }
        }
        is MindPopup.Move -> node(p.id)?.let { n -> PlanningSheet("Move “${n.text}”", { popup = null }) { MovePicker(m, n, cb) { popup = null } } }
        is MindPopup.Delete -> node(p.id)?.let { n ->
            val under = descendants(m, n.id).size
            CrundiDialog(
                "Delete idea", { popup = null },
                message = if (under > 0) "Delete this idea and the $under idea${if (under == 1) "" else "s"} under it?" else "Delete this idea?",
                confirm = "Delete", danger = true, onConfirm = { cb.onDelete(n); popup = null },
            )
        }
    }
}

private fun descendants(m: MindMap, id: String): List<MindNode> = m.children(id).flatMap { listOf(it) + descendants(m, it.id) }

@Composable
private fun ProjectBar(m: MindMap, hidden: MutableList<String>) {
    val projects = m.nodes.mapNotNull { it.effectiveProject }.distinct().sorted()
    if (projects.isEmpty()) return
    Row(
        Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(start = 12.dp, end = 12.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text("Projects:", style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, color = CrundiColors.TextSecondary))
        for (p in projects + "") {
            val shown = p !in hidden
            val label = p.ifEmpty { "General" }
            Row(
                Modifier.clip(RoundedCornerShape(20.dp)).background(if (shown) CrundiColors.BgTertiary else Color.Transparent)
                    .border(Dimens.Hairline, CrundiColors.Border, RoundedCornerShape(20.dp))
                    .clickable { if (shown) hidden.add(p) else hidden.remove(p) }.padding(horizontal = 10.dp, vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp),
            ) {
                if (shown) Box(Modifier.size(7.dp).clip(CircleShape).background(CrundiColors.Green))
                Text(label, style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontSize = 12.sp, color = if (shown) CrundiColors.TextPrimary else CrundiColors.TextMuted, textDecoration = if (shown) null else TextDecoration.LineThrough))
            }
        }
        if (hidden.isNotEmpty()) CrundiChip("Show all", false, { hidden.clear() })
    }
}

@Composable
private fun MindCanvas(
    m: MindMap,
    filter: String,
    hidden: List<String>,
    collapsed: MutableList<String>,
    selected: String?,
    editing: String?,
    /** Where a new idea is being typed: its parent's id, "" for the top level, null for none. */
    draft: String?,
    onTap: (String) -> Unit,
    onOpen: (String) -> Unit,
    onAdd: (String) -> Unit,
    onEdited: (MindNode, String?) -> Unit,
    onDrafted: (String?) -> Unit,
    /** Dragged onto another idea (its new parent) and let go. */
    onMoveTo: (MindNode, String) -> Unit = { _, _ -> },
) {
    // Hold an idea to drag it; let go over another idea to move it under that
    // one (the desktop's drag to move). Ideas say where they are on screen.
    val boxes = remember { androidx.compose.runtime.mutableStateMapOf<String, androidx.compose.ui.geometry.Rect>() }
    var dragId by remember { mutableStateOf<String?>(null) }
    var finger by remember { mutableStateOf(Offset.Zero) }
    var rootAt by remember { mutableStateOf(Offset.Zero) }
    fun under(at: Offset): String? {
        val d = dragId ?: return null
        val banned = setOf(d) + descendants(m, d).map { it.id }
        return boxes.entries.firstOrNull { it.key !in banned && it.value.contains(at) }?.key
    }
    val q = filter.lowercase()
    fun matches(n: MindNode) = q.isNotEmpty() && (n.text.lowercase().contains(q) || n.notes.any { it.lowercase().contains(q) } ||
        n.linkedTaskInfo?.let { it.title.lowercase().contains(q) || it.text.lowercase().contains(q) } == true)
    val byId = remember(m) { m.nodes.associateBy { it.id } }
    val roots = m.nodes.filter { it.parentId == null || byId[it.parentId] == null }.filter { (it.effectiveProject ?: "") !in hidden }.map { it.id } +
        (if (draft == "") listOf("draft:") else emptyList())
    fun kids(id: String): List<String> {
        if (id.startsWith("add:") || id.startsWith("draft:")) return emptyList()
        // The box being typed into stands where the idea will be, in place of "add idea".
        if (draft == id) return m.children(id).filter { (it.effectiveProject ?: "") !in hidden }.map { it.id } + "draft:$id"
        if (q.isEmpty() && id in collapsed) return emptyList()
        val c = m.children(id).filter { (it.effectiveProject ?: "") !in hidden }.map { it.id }
        return if (q.isEmpty() && id == selected) c + "add:$id" else c
    }
    val heights = remember { androidx.compose.runtime.mutableStateMapOf<String, Float>() }
    val (places, total) = MindLayout.layout(roots, ::kids) { id -> heights[id] ?: if (id.startsWith("add:")) MindLayout.ADD_H else 48f }
    val width = (places.values.maxOfOrNull { it.x } ?: 0f) + MindLayout.WIDTH + MindLayout.PAD

    var scale by remember { mutableFloatStateOf(1f) }
    var offset by remember { mutableStateOf(Offset.Zero) }
    var view by remember { mutableStateOf(IntSize.Zero) }
    var fitted by remember { mutableStateOf(false) }
    val density = androidx.compose.ui.platform.LocalDensity.current.density
    fun fit() {
        if (view.height == 0) return
        val s = min(1f, min(view.height / (total * density), view.width / (width * density))).coerceAtLeast(0.25f)
        scale = s; offset = Offset.Zero
    }
    LaunchedEffect(view, total > 0) { if (!fitted && view.height > 0) { val s = view.height / (total * density); scale = if (s < 1f) max(0.25f, s) else 1f; fitted = true } }

    // A tap on the map itself ends any typing there: the box loses focus
    // (which saves it) and the keyboard goes.
    val focusManager = androidx.compose.ui.platform.LocalFocusManager.current
    val keyboard = androidx.compose.ui.platform.LocalSoftwareKeyboardController.current
    Box(
        Modifier.fillMaxSize().clipToBounds().onSizeChanged { view = it }
            .androidx_onPlacedMind { rootAt = it }
            .pointerInput(Unit) { detectTapGestures { focusManager.clearFocus(); keyboard?.hide() } }
            .drawBehind {
                val step = 22.dp.toPx() * scale
                if (step > 4f) {
                    var x = offset.x % step; while (x < size.width) { var y = offset.y % step; while (y < size.height) { drawCircle(CrundiColors.BorderSubtle, 1.dp.toPx(), Offset(x, y)); y += step }; x += step }
                }
            }
            .pointerInput(Unit) {
                detectTransformGestures { centroid, pan, zoom, _ ->
                    val ns = (scale * zoom).coerceIn(0.15f, 3f)
                    offset = (offset - centroid) * (ns / scale) + centroid + pan
                    scale = ns
                }
            },
    ) {
        Layout(
            content = {
                for (pl in places.values) {
                    if (pl.id.startsWith("draft:")) {
                        Box(Modifier.layoutId(pl.id).width(MindLayout.WIDTH.dp).onSizeChanged { heights[pl.id] = it.height / density }) {
                            InlineIdea("", placeholder = "New idea", onDone = onDrafted)
                        }
                    } else if (pl.id.startsWith("add:")) {
                        val parent = pl.id.removePrefix("add:")
                        Row(
                            Modifier.layoutId(pl.id).width(MindLayout.WIDTH.dp).height(MindLayout.ADD_H.dp).alpha(0.6f)
                                .clip(RoundedCornerShape(14.dp)).dashedBorder(CrundiColors.TextMuted, 14.dp, 1.5.dp, 5f)
                                .clickable { onAdd(parent) }.padding(horizontal = 11.dp),
                            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp),
                        ) {
                            CrundiIcon("plus", size = 13.dp, tint = CrundiColors.TextSecondary)
                            Text("add idea", style = CrundiType.Small)
                        }
                    } else byId[pl.id]?.let { n ->
                        val dim = q.isNotEmpty() && !matches(n)
                        var coords by remember { mutableStateOf<androidx.compose.ui.layout.LayoutCoordinates?>(null) }
                        Box(
                            Modifier.layoutId(pl.id).width(MindLayout.WIDTH.dp).onSizeChanged { heights[n.id] = it.height / density }
                                .alpha(if (dim) 0.18f else if (dragId == n.id) 0.4f else 1f)
                                .onGloballyPositioned { c -> coords = c; boxes[n.id] = c.boundsInWindow() }
                                .then(if (n.id == editing) Modifier else Modifier.pointerInput(n.id) {
                                    detectDragGesturesAfterLongPress(
                                        onDragStart = { at -> dragId = n.id; finger = coords?.localToWindow(at) ?: at },
                                        onDrag = { c, _ -> c.consume(); finger = coords?.localToWindow(c.position) ?: finger },
                                        onDragEnd = { val t = under(finger); dragId = null; if (t != null && t != n.parentId) onMoveTo(n, t) },
                                        onDragCancel = { dragId = null },
                                    )
                                }),
                        ) {
                            if (n.id == editing) InlineIdea(n.text, placeholder = "Idea", onDone = { onEdited(n, it) })
                            else IdeaNode(
                                n, root = n.parentId == null, hit = matches(n), selected = n.id == selected,
                                hasKids = m.children(n.id).isNotEmpty(), collapsed = n.id in collapsed && q.isEmpty(),
                                onTap = { onTap(n.id) }, onOpen = { onOpen(n.id) }, onToggle = { if (n.id in collapsed) collapsed.remove(n.id) else collapsed.add(n.id) },
                                dropTarget = dragId != null && under(finger) == n.id,
                            )
                        }
                    }
                }
            },
            modifier = Modifier
                .wrapContentSize(Alignment.TopStart, unbounded = true)
                .graphicsLayer { translationX = offset.x; translationY = offset.y; scaleX = scale; scaleY = scale; transformOrigin = TransformOrigin(0f, 0f) }
                .drawBehind {
                    val d = density
                    for (pl in places.values) {
                        val parent = pl.parent?.let { places[it] } ?: continue
                        val sx = (parent.x + MindLayout.WIDTH) * d; val sy = parent.y * d
                        val ex = pl.x * d; val ey = pl.y * d
                        val mx = (sx + ex) / 2
                        val path = Path().apply { moveTo(sx, sy); cubicTo(mx, sy, mx, ey, ex, ey) }
                        val add = pl.id.startsWith("add:")
                        drawPath(path, if (add) CrundiColors.Border.copy(alpha = 0.45f) else CrundiColors.Border, style = Stroke(2.dp.toPx(), pathEffect = if (add) PathEffect.dashPathEffect(floatArrayOf(5.dp.toPx(), 5.dp.toPx())) else null))
                    }
                },
        ) { measurables, _ ->
            val placeables = measurables.map { it.layoutId as String to it.measure(Constraints()) }
            layout((width * density).toInt(), (total * density).toInt()) {
                for ((id, p) in placeables) {
                    val pl = places[id] ?: continue
                    p.place((pl.x * density).toInt(), ((pl.y * density) - p.height / 2f).toInt())
                }
            }
        }
        // The held idea, under the finger.
        dragId?.let { id -> byId[id] }?.let { d ->
            Box(
                Modifier.offset { androidx.compose.ui.unit.IntOffset((finger.x - rootAt.x - 60.dp.toPx()).toInt(), (finger.y - rootAt.y - 30.dp.toPx()).toInt()) }
                    .width(MindLayout.WIDTH.dp).shadow(10.dp, RoundedCornerShape(14.dp)).clip(RoundedCornerShape(14.dp))
                    .background(CrundiColors.BgCard).border(2.dp, CrundiColors.Accent, RoundedCornerShape(14.dp)).padding(horizontal = 11.dp, vertical = 8.dp),
            ) { Text(d.text, style = CrundiType.Body.copy(fontSize = 13.3.sp, fontWeight = FontWeight.SemiBold), maxLines = 2, overflow = TextOverflow.Ellipsis) }
        }
        Column(Modifier.align(Alignment.BottomEnd).padding(12.dp).background(CrundiColors.BgSecondary, Dimens.Shape).border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape)) {
            CrundiIconButton("plus", { scale = (scale * 1.25f).coerceAtMost(3f) }, contentDescription = "Zoom in")
            CrundiIconButton("minus", { scale = (scale / 1.25f).coerceAtLeast(0.15f) }, contentDescription = "Zoom out")
            CrundiIconButton("maximize", { fit() }, contentDescription = "Fit the map")
        }
    }
}

@Composable
@OptIn(ExperimentalFoundationApi::class)
private fun IdeaNode(n: MindNode, root: Boolean, hit: Boolean, selected: Boolean, hasKids: Boolean, collapsed: Boolean, onTap: () -> Unit, onOpen: () -> Unit, onToggle: () -> Unit, dropTarget: Boolean = false) {
    val shape = RoundedCornerShape(14.dp)
    val linked = n.linkedTask != null
    val border = when {
        dropTarget || selected || hit -> CrundiColors.AccentHover
        linked -> CrundiColors.Green
        root || n.project != null -> CrundiColors.Accent
        else -> CrundiColors.Border
    }
    Box {
        Column(
            Modifier.fillMaxWidth().padding(end = if (hasKids) 9.dp else 0.dp)
                .shadow(4.dp, shape).clip(shape)
                .background(if (root) CrundiColors.AccentDim.compositeOver(CrundiColors.BgCard) else CrundiColors.BgCard)
                .border(if (selected || hit) 2.dp else Dimens.Hairline, border, shape)
                .then(if (dropTarget) Modifier.background(CrundiColors.AccentDim) else Modifier)
                // A hold drags the idea (see MindCanvas); the pencil and a double tap open its menu.
                .combinedClickable(onClick = onTap, onDoubleClick = onOpen).padding(horizontal = 11.dp, vertical = 8.dp),
        ) {
            Text(n.text, style = CrundiType.Body.copy(fontSize = 13.3.sp, fontWeight = FontWeight.SemiBold, lineHeight = 17.sp), maxLines = 2, overflow = TextOverflow.Ellipsis)
            val chips = ideaChips(n, root)
            if (chips.isNotEmpty()) FlowRow(Modifier.padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(5.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                for (c in chips) IdeaChip(c)
            }
        }
        if (hasKids) Box(
            Modifier.align(Alignment.CenterEnd).size(18.dp).clip(CircleShape).background(CrundiColors.BgTertiary).border(Dimens.Hairline, CrundiColors.Border, CircleShape).clickable(onClick = onToggle),
            contentAlignment = Alignment.Center,
        ) { CrundiIcon(if (collapsed) "plus" else "minus", size = 11.dp, tint = CrundiColors.TextSecondary) }
        // Selected: a pencil beside it opens what can be done with it.
        if (selected) Box(
            Modifier.align(Alignment.CenterEnd).offset(x = if (hasKids) 30.dp else 20.dp).size(26.dp).clip(CircleShape)
                .background(CrundiColors.Accent).clickable(onClickLabel = "Edit idea", onClick = onOpen),
            contentAlignment = Alignment.Center,
        ) { CrundiIcon("pencil", size = 13.dp, tint = Color.White) }
    }
}

private fun Modifier.androidx_onPlacedMind(at: (Offset) -> Unit) = this.then(Modifier.onGloballyPositioned { at(it.positionInWindow()) })

/**
 * An idea typed where it stands (the desktop's mm-draft and mm-edit-input).
 * The keyboard's Done saves; leaving the box saves too, since on a phone that
 * is how the keyboard goes away, and losing what was typed is worse. Empty
 * or unchanged text saves nothing. [onDone] gets the text, once.
 */
@Composable
private fun InlineIdea(initial: String, placeholder: String, onDone: (String?) -> Unit) {
    var text by remember { mutableStateOf(androidx.compose.ui.text.input.TextFieldValue(initial, androidx.compose.ui.text.TextRange(initial.length))) }
    val focus = remember { FocusRequester() }
    var had by remember { mutableStateOf(false) }
    var finished by remember { mutableStateOf(false) }
    fun finish() { if (!finished) { finished = true; onDone(text.text) } }
    val shape = RoundedCornerShape(14.dp)
    Box(
        Modifier.fillMaxWidth().shadow(4.dp, shape).clip(shape).background(CrundiColors.BgCard)
            .border(2.dp, CrundiColors.AccentHover, shape).padding(horizontal = 11.dp, vertical = 8.dp),
    ) {
        if (text.text.isEmpty()) Text(placeholder, style = CrundiType.Body.copy(fontSize = 13.3.sp, color = CrundiColors.TextMuted, lineHeight = 17.sp))
        BasicTextField(
            text, { text = it },
            Modifier.fillMaxWidth().focusRequester(focus).onFocusChanged { if (it.isFocused) had = true else if (had) finish() },
            textStyle = CrundiType.Body.copy(fontSize = 13.3.sp, fontWeight = FontWeight.SemiBold, lineHeight = 17.sp, color = CrundiColors.TextPrimary),
            cursorBrush = SolidColor(CrundiColors.AccentHover),
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Done),
            keyboardActions = KeyboardActions(onDone = { finish() }),
            maxLines = 7,
        )
    }
    LaunchedEffect(Unit) { focus.requestFocus() }
    // Gone without Done or losing focus (another idea was tapped): still saved.
    androidx.compose.runtime.DisposableEffect(Unit) { onDispose { finish() } }
}

private fun Color.compositeOver(bg: Color): Color {
    val a = alpha
    return Color(red * a + bg.red * (1 - a), green * a + bg.green * (1 - a), blue * a + bg.blue * (1 - a), 1f)
}

private data class ChipSpec(val icon: String, val text: String, val fg: Color, val bg: Color, val dashed: Boolean = false)

private fun ideaChips(n: MindNode, root: Boolean): List<ChipSpec> {
    val out = mutableListOf<ChipSpec>()
    val li = n.linkedTaskInfo
    if (n.linkedTask != null) out += when {
        li == null || li.missing -> ChipSpec("link", "removed", CrundiColors.TextMuted, CrundiColors.BgTertiary)
        li.kind == "todo" -> ChipSpec(if (li.done) "check" else "circle", li.text.cut(), CrundiColors.Green, CrundiColors.GreenDim)
        else -> ChipSpec("link", li.title.cut(), CrundiColors.Green, CrundiColors.GreenDim)
    }
    if (n.project != null) out += ChipSpec("folder", n.project.cut(), CrundiColors.AccentHover, CrundiColors.AccentDim)
    if (n.notes.isNotEmpty()) out += ChipSpec("note", "${n.notes.size} note${if (n.notes.size == 1) "" else "s"}", CrundiColors.TextSecondary, CrundiColors.BgTertiary)
    return out
}

private fun String.cut() = if (length > 20) take(19) + "…" else this

@Composable
private fun IdeaChip(c: ChipSpec) {
    Row(
        Modifier.clip(RoundedCornerShape(8.dp)).background(c.bg).padding(horizontal = 6.dp, vertical = 1.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        CrundiIcon(c.icon, size = 10.dp, tint = c.fg)
        Text(c.text, style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontSize = 10.5.sp, color = c.fg), maxLines = 1)
    }
}

// ─── Sheets ───

@Composable
internal fun IdeaDetails(m: MindMap, n: MindNode, cb: MindMapCallbacks, go: (MindPopup?) -> Unit, collapsed: MutableList<String>) {
    val li = n.linkedTaskInfo
    val line = when {
        n.linkedTask != null && (li == null || li.missing) -> "Linked item no longer exists"
        li?.kind == "todo" -> "Subtask ${if (li.done) "done" else "open"}: ${li.text} · in ${li.taskTitle}"
        li != null -> "Kanban: ${li.project} · ${li.title} [${kanbanTitle(li.status)}]"
        n.project != null -> "Scoped to project “${n.project}” (no task linked)"
        n.effectiveProject != null -> "In project “${n.effectiveProject}” (from the idea above)"
        else -> null
    }
    if (line != null) Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        CrundiIcon(if (n.linkedTask != null) "link" else "folder", size = 14.dp, tint = if (n.linkedTask != null) CrundiColors.Green else CrundiColors.AccentHover)
        Text(line, style = CrundiType.Small.copy(color = CrundiColors.TextPrimary), modifier = Modifier.weight(1f))
        if (li != null && !li.missing) CrundiButton("Open", cb.onOpenKanban, kind = ButtonKind.Ghost)
    }
    if (n.notes.isNotEmpty()) Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        for (note in n.notes) Row(verticalAlignment = Alignment.Top) {
            Box(Modifier.padding(top = 7.dp, end = 10.dp).size(6.dp).clip(CircleShape).background(CrundiColors.Accent))
            Text(note, style = CrundiType.Body)
        }
    } else Text("No notes yet.", style = CrundiType.Small.copy(color = CrundiColors.TextMuted))
    val kids = m.children(n.id).size
    Text(
        listOfNotNull(if (kids > 0) "$kids idea${if (kids == 1) "" else "s"} under it" else null, ago(n.updatedAt).takeIf { it.isNotEmpty() }?.let { "edited $it" }).joinToString(" · "),
        style = CrundiType.Meta,
    )
    CrundiDivider()
    MenuRow("Edit", "pencil", { go(MindPopup.Edit(n.id)) })
    MenuRow("Add child", "plus", { go(MindPopup.Add(n.id)) })
    if (n.linkedTask != null || n.project != null) MenuRow("Unlink", "link", { cb.onUnlink(n); go(null) })
    else MenuRow("Link / scope", "link", { go(MindPopup.Link(n.id)) })
    MenuRow("Move", "layers", { go(MindPopup.Move(n.id)) })
    if (kids > 0) MenuRow(if (n.id in collapsed) "Show the ideas under it" else "Fold the ideas under it", if (n.id in collapsed) "eye" else "eye-off", {
        if (n.id in collapsed) collapsed.remove(n.id) else collapsed.add(n.id); go(null)
    })
    CrundiDivider()
    MenuRow("Delete", "trash", { go(MindPopup.Delete(n.id)) }, danger = true)
}

/** Edit an idea's text and notes (the desktop's details dialog in edit mode). */
@Composable
fun IdeaEditCard(n: MindNode, onDismiss: () -> Unit, onSave: (String, List<String>) -> Unit) {
    var text by remember { mutableStateOf(n.text) }
    val notes = remember { mutableStateListOf(*n.notes.toTypedArray()) }
    var err by remember { mutableStateOf<String?>(null) }
    DialogCard("Edit idea", Modifier.padding(16.dp), confirm = "Save", onConfirm = {
        if (text.isBlank()) err = "Idea text is required" else onSave(text.trim(), notes.map { it.trim() }.filter { it.isNotEmpty() })
    }, onDismiss = onDismiss) {
        FieldLabel("Idea")
        CrundiTextField(text, { text = it; err = null }, Modifier.fillMaxWidth(), singleLine = false, maxLines = 4)
        if (err != null) Text(err!!, style = CrundiType.Small.copy(color = CrundiColors.Red))
        FieldLabel("Notes")
        notes.forEachIndexed { i, v ->
            Row(verticalAlignment = Alignment.CenterVertically) {
                CrundiTextField(v, { notes[i] = it }, Modifier.weight(1f), placeholder = "Note…", singleLine = false, maxLines = 4)
                CrundiIconButton("x", { notes.removeAt(i) }, contentDescription = "Remove note", size = 14.dp)
            }
        }
        CrundiButton("Add note", { notes.add("") }, kind = ButtonKind.Ghost, icon = "plus")
    }
}

@Composable
internal fun LinkPicker(m: MindMap, n: MindNode, projects: List<String>, current: String?, boards: Map<String, Load<KanbanBoard>>, cb: MindMapCallbacks, close: () -> Unit) {
    val inherited = n.parentId?.let { pid -> m.nodes.firstOrNull { it.id == pid }?.effectiveProject }
    var project by remember { mutableStateOf(inherited ?: n.effectiveProject ?: current ?: projects.firstOrNull() ?: "") }
    var q by remember { mutableStateOf("") }
    LaunchedEffect(project) { if (project.isNotEmpty()) cb.onNeedBoard(project) }
    if (inherited != null) Text("Scoped to “$inherited” (inherited from the idea above): only tasks in this project can be linked.", style = CrundiType.Small)
    else {
        FieldLabel("Project")
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            for (p in projects) CrundiChip(p, p == project, { project = p })
        }
        if (project.isNotEmpty()) CrundiButton("Scope to “$project” (no task)", { cb.onScope(n, project); close() }, icon = "folder", modifier = Modifier.fillMaxWidth())
        Text("or link a specific task", style = CrundiType.Meta, modifier = Modifier.fillMaxWidth(), textAlign = androidx.compose.ui.text.style.TextAlign.Center)
    }
    CrundiTextField(q, { q = it }, Modifier.fillMaxWidth(), placeholder = "Search tasks & subtasks…", icon = "search")
    when (val b = boards[project]) {
        null, is Load.Loading -> Text("Loading…", style = CrundiType.Small)
        is Load.Failed -> Text(b.message, style = CrundiType.Small.copy(color = CrundiColors.Red))
        is Load.Ready -> {
            val s = q.trim().lowercase()
            var any = false
            for (t in b.value.tasks.filter { !it.deleted }) {
                val todos = t.todos.filter { !it.deleted && (s.isEmpty() || it.text.lowercase().contains(s)) }
                val taskHit = s.isEmpty() || t.title.lowercase().contains(s)
                if (!taskHit && todos.isEmpty()) continue
                any = true
                MenuRow("${t.title}  [${kanbanTitle(t.status)}]", "kanban", { cb.onLink(n, project, t.id, null); close() })
                for (d in todos) Box(Modifier.padding(start = 24.dp)) { MenuRow(d.text, if (d.done) "check" else "circle", { cb.onLink(n, project, t.id, d.id); close() }) }
            }
            if (!any) Text("No matching tasks or subtasks.", style = CrundiType.Small.copy(color = CrundiColors.TextMuted))
        }
    }
}

@Composable
private fun MovePicker(m: MindMap, n: MindNode, cb: MindMapCallbacks, close: () -> Unit) {
    val siblings = m.nodes.filter { it.parentId == n.parentId }
    val i = siblings.indexOfFirst { it.id == n.id }
    if (i > 0) MenuRow("Move up", "chevron-up", { cb.onMove(n, n.parentId, i - 1); close() })
    if (i in 0 until siblings.size - 1) MenuRow("Move down", "chevron-down", { cb.onMove(n, n.parentId, i + 1); close() })
    if (n.parentId != null) MenuRow("Make it a top-level idea", "arrow-up", { cb.onMove(n, null, null); close() })
    val banned = (descendants(m, n.id).map { it.id } + n.id + listOfNotNull(n.parentId)).toSet()
    SectionHeader("Put it under")
    for (o in m.nodes.filter { it.id !in banned }) {
        val depth = generateSequence(o) { x -> m.nodes.firstOrNull { it.id == x.parentId } }.count() - 1
        Box(Modifier.padding(start = (depth * 14).dp)) { MenuRow(o.text, "mindmap", { cb.onMove(n, o.id, null); close() }) }
    }
}

