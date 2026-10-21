package mv.obitech.crundi.ui.planning

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import kotlinx.coroutines.delay
import mv.obitech.crundi.api.model.KanbanBoard
import mv.obitech.crundi.api.model.KanbanEvent
import mv.obitech.crundi.api.model.KanbanTask
import mv.obitech.crundi.api.model.KanbanTodo
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiChip
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

/** The desktop's three views of the board. */
enum class KanbanView { Board, Trash, History }

/** The desktop's column titles (KANBAN_STATUS_LABELS). */
fun kanbanTitle(status: String) = when (status) {
    "backlog" -> "Backlog"; "todo" -> "To Do"; "in_progress" -> "In Progress"; "done" -> "Done"
    else -> status.replace('_', ' ').replaceFirstChar { it.uppercase() }
}

/** Everything the board can ask for. Each defaults to nothing, for tests. */
@Stable
class KanbanCallbacks(
    val onAddTask: (title: String, status: String) -> Unit = { _, _ -> },
    val onSaveTask: (task: KanbanTask, title: String, description: String, status: String) -> Unit = { _, _, _, _ -> },
    val onMove: (task: KanbanTask, status: String, index: Int) -> Unit = { _, _, _ -> },
    val onDeleteTask: (KanbanTask) -> Unit = {},
    val onRestoreTask: (KanbanTask) -> Unit = {},
    val onAddTodo: (KanbanTask, String) -> Unit = { _, _ -> },
    val onToggleTodo: (KanbanTask, KanbanTodo) -> Unit = { _, _ -> },
    val onRenameTodo: (KanbanTask, KanbanTodo, String) -> Unit = { _, _, _ -> },
    val onDeleteTodo: (KanbanTask, KanbanTodo) -> Unit = { _, _ -> },
    val onRestoreTodo: (KanbanTask, KanbanTodo) -> Unit = { _, _ -> },
    val onBrainstorm: (KanbanTask) -> Unit = {},
    val onOpenMindMap: () -> Unit = {},
    val onRetry: () -> Unit = {},
)

/** What the open dialog or sheet is (screen-local; tests can start with one open). */
sealed interface KanbanPopup {
    data class AddTask(val status: String) : KanbanPopup
    data class EditTask(val task: KanbanTask) : KanbanPopup
    data class TaskMenu(val task: KanbanTask) : KanbanPopup
    data class TodoMenu(val task: KanbanTask, val todo: KanbanTodo) : KanbanPopup
    data class RenameTodo(val task: KanbanTask, val todo: KanbanTodo) : KanbanPopup
    data class AddTodo(val task: KanbanTask) : KanbanPopup
}

// ─── Route ───

@Composable
fun KanbanRoute() = WithProject(Area.Kanban) { project -> KanbanProjectRoute(project) }

@Composable
private fun KanbanProjectRoute(project: String) {
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val loader = rememberLoader(project, events = arrayOf("kanban"), accept = { it.project == null || it.project.equals(project, true) }) {
        api.kanban.board(project, includeDeleted = true)
    }
    val actions = rememberActions { loader.refresh() }
    var view by remember(project) { mutableStateOf(KanbanView.Board) }
    val cb = remember(project) {
        KanbanCallbacks(
            onAddTask = { t, s -> actions.run { api.kanban.addTask(project, t, status = s) } },
            onSaveTask = { task, t, d, s ->
                actions.run {
                    api.kanban.updateTask(project, task.id, title = t, description = d)
                    if (s != task.status) api.kanban.moveTask(project, task.id, s)
                }
            },
            onMove = { task, s, i -> loader.update { it.moved(task.id, s, i) }; actions.run { api.kanban.moveTask(project, task.id, s, i) } },
            onDeleteTask = { t -> loader.update { b -> b.copy(tasks = b.tasks.filter { it.id != t.id }, deletedTasks = b.deletedTasks + t.copy(deleted = true)) }; actions.run { api.kanban.deleteTask(project, t.id) } },
            onRestoreTask = { t -> actions.run { api.kanban.restoreTask(project, t.id) } },
            onAddTodo = { t, text -> actions.run { api.kanban.addTodo(project, t.id, text) } },
            onToggleTodo = { t, d ->
                loader.update { it.withTodo(t.id, d.id) { x -> x.copy(done = !x.done) } }
                actions.run { api.kanban.updateTodo(project, t.id, d.id, done = !d.done) }
            },
            onRenameTodo = { t, d, text -> actions.run { api.kanban.updateTodo(project, t.id, d.id, text = text) } },
            onDeleteTodo = { t, d -> actions.run { api.kanban.deleteTodo(project, t.id, d.id) } },
            onRestoreTodo = { t, d -> actions.run { api.kanban.restoreTodo(project, t.id, d.id) } },
            onBrainstorm = { t ->
                actions.run(refresh = true) {
                    val root = api.mindmap.add(t.title)
                    api.mindmap.link(root.id, project, t.id)
                    for (d in t.todos.filter { !it.deleted }) api.mindmap.add(d.text, parentId = root.id)
                    nav.area(Area.MindMap)
                }
            },
            onOpenMindMap = { nav.area(Area.MindMap) },
            onRetry = { loader.refresh() },
        )
    }
    Box(Modifier.fillMaxSize()) {
        KanbanScreen(loader.load, view, { view = it }, cb)
        ActionError(actions)
    }
}

/** The board with [task] moved to [status] at [index] (among that column's other cards). */
fun KanbanBoard.moved(taskId: String, status: String, index: Int): KanbanBoard {
    val task = tasks.firstOrNull { it.id == taskId } ?: return this
    val rest = tasks.filter { it.id != taskId }
    val col = rest.filter { it.status == status && !it.deleted }
    val moved = task.copy(status = status)
    val out = rest.toMutableList()
    val at = if (index < col.size) out.indexOf(col[index]) else (out.indexOfLast { it.status == status }.let { if (it < 0) out.size else it + 1 })
    out.add(at.coerceIn(0, out.size), moved)
    return copy(tasks = out)
}

private fun KanbanBoard.withTodo(taskId: String, todoId: String, f: (KanbanTodo) -> KanbanTodo) =
    copy(tasks = tasks.map { t -> if (t.id != taskId) t else t.copy(todos = t.todos.map { if (it.id == todoId) f(it) else it }) })

// ─── Screen ───

@Composable
fun KanbanScreen(
    board: mv.obitech.crundi.ui.planning.Load<KanbanBoard>,
    view: KanbanView,
    onView: (KanbanView) -> Unit,
    cb: KanbanCallbacks = KanbanCallbacks(),
    initialPopup: KanbanPopup? = null,
) {
    var popup by remember { mutableStateOf(initialPopup) }
    Column(Modifier.fillMaxSize()) {
        val b = board.valueOrNull
        KanbanToolbar(view, b?.deletedTasks?.size ?: 0, onView, onAdd = { popup = KanbanPopup.AddTask("todo") })
        when (board) {
            is Load.Loading -> LoadingState(text = "Loading the board")
            is Load.Failed -> ErrorState(board.message, onRetry = cb.onRetry)
            is Load.Ready -> when (view) {
                KanbanView.Board -> KanbanBoardView(board.value, cb) { popup = it }
                KanbanView.Trash -> KanbanTrash(board.value, cb)
                KanbanView.History -> KanbanHistory(board.value.history)
            }
        }
    }
    val statuses = board.valueOrNull?.statuses ?: listOf("backlog", "todo", "in_progress", "done")
    when (val p = popup) {
        null -> {}
        is KanbanPopup.AddTask -> Dialog({ popup = null }) {
            TaskEditorCard("Add task", statuses, p.status, confirm = "Add task", onDismiss = { popup = null }) { t, _, s -> cb.onAddTask(t, s); popup = null }
        }
        is KanbanPopup.EditTask -> Dialog({ popup = null }) {
            TaskEditorCard("Edit task", statuses, p.task.status, p.task.title, p.task.description, confirm = "Save", withDescription = true, onDismiss = { popup = null }) { t, d, s ->
                cb.onSaveTask(p.task, t, d, s); popup = null
            }
        }
        is KanbanPopup.TaskMenu -> PlanningSheet(p.task.title, { popup = null }, subtitle = kanbanTitle(p.task.status)) {
            TaskMenuItems(p.task, statuses, cb, onEdit = { popup = KanbanPopup.EditTask(p.task) }, onAddTodo = { popup = KanbanPopup.AddTodo(p.task) }, close = { popup = null })
        }
        is KanbanPopup.TodoMenu -> PlanningSheet(p.todo.text, { popup = null }, subtitle = "Subtask of ${p.task.title}") {
            MenuRow(if (p.todo.done) "Mark not done" else "Mark done", "check", { cb.onToggleTodo(p.task, p.todo); popup = null })
            MenuRow("Rename", "pencil", { popup = KanbanPopup.RenameTodo(p.task, p.todo) })
            CrundiDivider()
            MenuRow("Delete", "trash", { cb.onDeleteTodo(p.task, p.todo); popup = null }, danger = true)
        }
        is KanbanPopup.AddTodo -> Dialog({ popup = null }) {
            TextAskCard("Add subtask", "Subtask", "", "Add subtask", onDismiss = { popup = null }, placeholder = "What is one step of it") { cb.onAddTodo(p.task, it); popup = null }
        }
        is KanbanPopup.RenameTodo -> Dialog({ popup = null }) {
            TextAskCard("Edit subtask", "Subtask", p.todo.text, "Save", onDismiss = { popup = null }) { cb.onRenameTodo(p.task, p.todo, it); popup = null }
        }
    }
}

@Composable
private fun KanbanToolbar(view: KanbanView, trashCount: Int, onView: (KanbanView) -> Unit, onAdd: () -> Unit) {
    Column {
        Row(
            Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 12.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            CrundiButton("Add task", onAdd, kind = ButtonKind.Primary, icon = "plus")
            Spacer(Modifier.width(4.dp))
            CrundiChip("Board", view == KanbanView.Board, { onView(KanbanView.Board) }, icon = "kanban")
            CrundiChip("Trash ($trashCount)", view == KanbanView.Trash, { onView(KanbanView.Trash) }, icon = "trash")
            CrundiChip("History", view == KanbanView.History, { onView(KanbanView.History) }, icon = "clock")
        }
        CrundiDivider()
    }
}

// ─── Board with drag ───

/** Where a dragged card is and where it would land. Bounds are in root coordinates. */
@Stable
class KanbanDrag {
    var task by mutableStateOf<KanbanTask?>(null)
    var pointer by mutableStateOf(Offset.Zero)
    var grab = Offset.Zero
    var width = 0
    val cards = mutableStateMapOf<String, Rect>()
    val columns = mutableStateMapOf<String, Rect>()

    /** (status, index among that column's other cards) for the pointer now. */
    fun target(board: KanbanBoard): Pair<String, Int>? {
        val t = task ?: return null
        if (columns.isEmpty()) return null
        val col = columns.entries.firstOrNull { it.value.contains(pointer) }?.key
            ?: columns.entries.minByOrNull { e -> kotlin.math.abs(e.value.center.y - pointer.y) }?.key ?: return null
        val others = board.column(col).filter { it.id != t.id }
        val idx = others.indexOfFirst { o -> (cards[o.id]?.center?.y ?: Float.MAX_VALUE) > pointer.y }
        return col to (if (idx < 0) others.size else idx)
    }
}

@Composable
private fun KanbanBoardView(board: KanbanBoard, cb: KanbanCallbacks, open: (KanbanPopup) -> Unit) {
    val drag = remember { KanbanDrag() }
    val scroll = rememberScrollState()
    var origin by remember { mutableStateOf(Offset.Zero) }
    val target = drag.target(board)
    val statuses = board.statuses + board.tasks.map { it.status }.filter { it !in board.statuses && it.isNotEmpty() }.distinct()

    // Scroll while a card is held near the top or bottom edge.
    var viewport by remember { mutableStateOf(Rect.Zero) }
    LaunchedEffect(drag.task) {
        while (drag.task != null) {
            val edge = 72f
            val dy = when {
                drag.pointer.y < viewport.top + edge -> -18f
                drag.pointer.y > viewport.bottom - edge -> 18f
                else -> 0f
            }
            if (dy != 0f) scroll.scrollBy(dy)
            delay(16)
        }
    }

    Box(Modifier.fillMaxSize().onGloballyPositioned { val r = it.boundsInRoot(); origin = r.topLeft; viewport = r }) {
        Column(Modifier.fillMaxSize().verticalScroll(scroll).padding(12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            for (s in statuses) {
                KanbanColumn(s, board.column(s), s !in board.statuses, drag, target, cb, open,
                    onDrop = { t, st, i -> cb.onMove(t, st, i) }, board = board)
            }
            if (board.tasks.none { !it.deleted }) {
                Text("No tasks yet. Add one to start the board.", style = CrundiType.Small.copy(color = CrundiColors.TextMuted), modifier = Modifier.fillMaxWidth().padding(8.dp))
            }
            Spacer(Modifier.height(40.dp))
        }
        // The card under the finger.
        drag.task?.let { t ->
            val d = LocalDensity.current
            Box(
                Modifier
                    .offset { IntOffset((drag.pointer.x - drag.grab.x - origin.x).toInt(), (drag.pointer.y - drag.grab.y - origin.y).toInt()) }
                    .width(with(d) { drag.width.toDp() })
                    .rotate(1.5f)
                    .alpha(0.92f)
                    .shadow(16.dp, Dimens.Shape),
            ) { KanbanCard(t, KanbanCallbacks(), {}, dragging = true) }
        }
    }
}

@Composable
private fun KanbanColumn(
    status: String,
    tasks: List<KanbanTask>,
    other: Boolean,
    drag: KanbanDrag,
    target: Pair<String, Int>?,
    cb: KanbanCallbacks,
    open: (KanbanPopup) -> Unit,
    onDrop: (KanbanTask, String, Int) -> Unit,
    board: KanbanBoard,
) {
    val over = target?.first == status
    val border = when { over -> CrundiColors.Accent; other -> CrundiColors.Yellow; else -> CrundiColors.Border }
    DisposableEffect(status) { onDispose { drag.columns.remove(status) } }
    Column(
        Modifier
            .fillMaxWidth()
            .onGloballyPositioned { drag.columns[status] = it.boundsInRoot() }
            .clip(Dimens.Shape)
            .background(CrundiColors.BgSecondary)
            .border(Dimens.Hairline, border, Dimens.Shape),
    ) {
        Row(Modifier.fillMaxWidth().padding(start = 12.dp, end = 12.dp, top = 11.dp, bottom = 9.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(
                if (other) "Other" else kanbanTitle(status),
                style = CrundiType.Body.copy(fontSize = 13.5.sp, fontWeight = FontWeight.SemiBold, color = if (other) CrundiColors.Yellow else CrundiColors.TextPrimary),
                modifier = Modifier.weight(1f),
            )
            Text(
                tasks.size.toString(),
                style = CrundiType.Meta.copy(color = CrundiColors.TextSecondary, fontWeight = FontWeight.SemiBold, fontFamily = CrundiType.Sans),
                modifier = Modifier.clip(RoundedCornerShape(50)).background(CrundiColors.BgTertiary).defaultMinSize(minWidth = 22.dp).padding(horizontal = 7.dp, vertical = 1.dp),
            )
        }
        CrundiDivider()
        Column(Modifier.fillMaxWidth().padding(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            val visible = tasks.filter { it.id != drag.task?.id }
            for ((i, t) in tasks.withIndex()) {
                val vi = visible.indexOf(t)
                if (over && vi >= 0 && target?.second == vi) DropLine()
                Box(
                    Modifier
                        .onGloballyPositioned { drag.cards[t.id] = it.boundsInRoot() }
                        .alpha(if (drag.task?.id == t.id) 0.35f else 1f)
                        .pointerInput(t.id, i) {
                            detectDragGesturesAfterLongPress(
                                onDragStart = { off ->
                                    val r = drag.cards[t.id] ?: return@detectDragGesturesAfterLongPress
                                    drag.grab = off; drag.width = r.width.toInt(); drag.pointer = r.topLeft + off; drag.task = t
                                },
                                onDrag = { ch, amt -> ch.consume(); drag.pointer += amt },
                                onDragEnd = {
                                    val tg = drag.target(board); val d = drag.task
                                    drag.task = null
                                    if (tg != null && d != null) {
                                        val oldIdx = board.column(d.status).indexOfFirst { it.id == d.id }
                                        if (tg.first != d.status || tg.second != oldIdx) onDrop(d, tg.first, tg.second)
                                    }
                                },
                                onDragCancel = { drag.task = null },
                            )
                        },
                ) { KanbanCard(t, cb, open) }
                DisposableEffect(t.id) { onDispose { drag.cards.remove(t.id) } }
            }
            if (over && target?.second == tasks.count { it.id != drag.task?.id }) DropLine()
            if (!other) AddTaskSlot { open(KanbanPopup.AddTask(status)) }
        }
    }
}

@Composable
private fun DropLine() = Box(Modifier.fillMaxWidth().padding(horizontal = 2.dp).height(2.dp).background(CrundiColors.Accent, RoundedCornerShape(1.dp)))

@Composable
private fun AddTaskSlot(onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .clip(Dimens.Shape)
            .dashedBorder(CrundiColors.Border, Dimens.Radius)
            .clickable(onClick = onClick)
            .padding(horizontal = 10.dp, vertical = 11.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        CrundiIcon("plus", size = 14.dp, tint = CrundiColors.TextMuted)
        Text("Add task", style = CrundiType.Small.copy(color = CrundiColors.TextMuted))
    }
}

/** A dashed 1px outline, as the desktop's "add" slots. */
fun Modifier.dashedBorder(color: Color, radius: androidx.compose.ui.unit.Dp, width: androidx.compose.ui.unit.Dp = 1.dp, dash: Float = 6f) = this.then(
    Modifier.drawBehind {
        val stroke = androidx.compose.ui.graphics.drawscope.Stroke(width.toPx(), pathEffect = androidx.compose.ui.graphics.PathEffect.dashPathEffect(floatArrayOf(dash * density, dash * density / 1.5f)))
        drawRoundRect(color, style = stroke, cornerRadius = androidx.compose.ui.geometry.CornerRadius(radius.toPx()))
    },
)


// ─── Card ───

@Composable
fun KanbanCard(task: KanbanTask, cb: KanbanCallbacks, open: (KanbanPopup) -> Unit, dragging: Boolean = false) {
    var adding by remember(task.id) { mutableStateOf(false) }
    var newTodo by remember(task.id) { mutableStateOf("") }
    val todos = task.todos.filter { !it.deleted }
    Column(
        Modifier
            .fillMaxWidth()
            .clip(Dimens.Shape)
            .background(CrundiColors.BgCard)
            .border(Dimens.Hairline, if (dragging) CrundiColors.Accent.copy(alpha = 0.55f) else CrundiColors.Border, Dimens.Shape)
            .clickable { open(KanbanPopup.EditTask(task)) }
            .padding(start = 12.dp, end = 4.dp, top = 6.dp, bottom = if (todos.isEmpty() && !adding) 8.dp else 4.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(task.title, style = CrundiType.Body.copy(fontSize = 14.5.sp, fontWeight = FontWeight.SemiBold, lineHeight = 19.sp), modifier = Modifier.weight(1f).padding(vertical = 4.dp))
            if (task.mindmapNodes.isNotEmpty()) {
                Row(
                    Modifier.clip(RoundedCornerShape(50)).background(CrundiColors.AccentDim).clickable(onClick = cb.onOpenMindMap).padding(start = 5.dp, end = 7.dp, top = 2.dp, bottom = 2.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(3.dp),
                ) {
                    CrundiIcon("mindmap", size = 11.dp, tint = CrundiColors.AccentHover)
                    Text(task.mindmapNodes.size.toString(), style = CrundiType.Meta.copy(color = CrundiColors.AccentHover, fontWeight = FontWeight.SemiBold, fontFamily = CrundiType.Sans))
                }
            }
            CrundiIconButton("more", { open(KanbanPopup.TaskMenu(task)) }, contentDescription = "Task actions", size = 16.dp, touch = 36.dp)
        }
        if (task.description.isNotBlank()) {
            Text(task.description, style = CrundiType.Small.copy(fontSize = 13.sp, lineHeight = 19.sp), modifier = Modifier.padding(end = 8.dp, bottom = 4.dp))
        }
        if (todos.isNotEmpty()) {
            Column(Modifier.padding(top = 4.dp, end = 4.dp)) {
                for (d in todos) TodoRow(d, { cb.onToggleTodo(task, d) }, { open(KanbanPopup.TodoMenu(task, d)) })
            }
            val done = todos.count { it.done }
            Row(Modifier.fillMaxWidth().padding(top = 6.dp, end = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Box(Modifier.weight(1f).height(3.dp).clip(RoundedCornerShape(3.dp)).background(CrundiColors.BgTertiary)) {
                    Box(Modifier.fillMaxWidth(done.toFloat() / todos.size).height(3.dp).background(if (done == todos.size) CrundiColors.Green else CrundiColors.Accent))
                }
                Text("$done of ${todos.size}", style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans))
            }
        }
        if (adding) {
            CrundiTextField(
                newTodo, { newTodo = it }, Modifier.fillMaxWidth().padding(top = 6.dp, end = 8.dp), placeholder = "New subtask, Enter to add",
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                keyboardActions = KeyboardActions(onDone = { if (newTodo.isNotBlank()) cb.onAddTodo(task, newTodo.trim()); newTodo = "" }),
            )
        }
        if (todos.isNotEmpty() || adding) Row(Modifier.padding(top = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            SmallGhost(if (adding) "Done" else "Subtask", if (adding) "check" else "plus") {
                if (adding && newTodo.isNotBlank()) cb.onAddTodo(task, newTodo.trim())
                newTodo = ""; adding = !adding
            }
        }
    }
}

@Composable
private fun SmallGhost(text: String, icon: String, onClick: () -> Unit) {
    Row(
        Modifier.clip(Dimens.ShapeSm).clickable(onClick = onClick).padding(horizontal = 6.dp, vertical = 7.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        CrundiIcon(icon, size = 13.dp, tint = CrundiColors.TextMuted)
        Text(text, style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontSize = 12.sp, color = CrundiColors.TextSecondary))
    }
}

@Composable
private fun TodoRow(todo: KanbanTodo, onToggle: () -> Unit, onMore: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clip(Dimens.ShapeSm).clickable(onClick = onToggle).padding(start = 0.dp, top = 3.dp, bottom = 3.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        PlanningCheckbox(todo.done, onToggle)
        Text(
            todo.text,
            style = CrundiType.Small.copy(
                fontSize = 13.sp,
                color = if (todo.done) CrundiColors.TextMuted else CrundiColors.TextPrimary,
                textDecoration = if (todo.done) TextDecoration.LineThrough else null,
            ),
            modifier = Modifier.weight(1f),
        )
        CrundiIconButton("more", onMore, contentDescription = "Subtask actions", size = 15.dp, touch = 30.dp, tint = CrundiColors.TextMuted)
    }
}

/** The desktop's checkbox: 16dp, 1.5dp #4a4a68 border, radius 4; checked = accent with a white tick. */
@Composable
fun PlanningCheckbox(checked: Boolean, onToggle: () -> Unit, size: androidx.compose.ui.unit.Dp = 16.dp) {
    val shape = RoundedCornerShape(4.dp)
    Box(
        Modifier
            .size(size)
            .clip(shape)
            .background(if (checked) CrundiColors.Accent else CrundiColors.BgPrimary)
            .border(1.5.dp, if (checked) CrundiColors.Accent else CrundiColors.TextMuted, shape)
            .clickable(onClick = onToggle),
        contentAlignment = Alignment.Center,
    ) { if (checked) CrundiIcon("check", size = size - 4.dp, tint = Color.White) }
}

@Composable
internal fun TaskMenuItems(task: KanbanTask, statuses: List<String>, cb: KanbanCallbacks, onEdit: () -> Unit, onAddTodo: () -> Unit, close: () -> Unit) {
    MenuRow("Edit", "pencil", onEdit)
    MenuRow("Add subtask", "plus", onAddTodo)
    MenuRow("Brainstorm in the Mind map", "mindmap", { cb.onBrainstorm(task); close() })
    SectionHeader("Move to", Modifier.padding(top = 4.dp))
    Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        for (s in statuses) CrundiChip(kanbanTitle(s), s == task.status, { if (s != task.status) cb.onMove(task, s, Int.MAX_VALUE / 2); close() })
    }
    CrundiDivider()
    MenuRow("Delete task", "trash", { cb.onDeleteTask(task); close() }, danger = true)
}

/** The add/edit task dialog body (title, column, description). */
@Composable
fun TaskEditorCard(
    title: String,
    statuses: List<String>,
    status: String,
    initialTitle: String = "",
    initialDescription: String = "",
    confirm: String,
    withDescription: Boolean = false,
    onDismiss: () -> Unit,
    onSave: (String, String, String) -> Unit,
) {
    var t by remember { mutableStateOf(initialTitle) }
    var d by remember { mutableStateOf(initialDescription) }
    var s by remember { mutableStateOf(status) }
    DialogCard(title, Modifier.padding(16.dp), confirm = confirm, onConfirm = { if (t.isNotBlank()) onSave(t.trim(), d, s) }, onDismiss = onDismiss) {
        FieldLabel("Task title")
        CrundiTextField(t, { t = it }, Modifier.fillMaxWidth(), placeholder = "What needs doing")
        FieldLabel("Column")
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            for (x in statuses) CrundiChip(kanbanTitle(x), x == s, { s = x })
        }
        if (withDescription) {
            FieldLabel("Description")
            CrundiTextField(d, { d = it }, Modifier.fillMaxWidth(), placeholder = "Details, links, what done looks like", singleLine = false, minLines = 3)
        }
    }
}

/** A one-field text dialog body, as the desktop's askText. */
@Composable
fun TextAskCard(title: String, label: String, initial: String, confirm: String, onDismiss: () -> Unit, multiline: Boolean = false, placeholder: String = "", onSave: (String) -> Unit) {
    var v by remember { mutableStateOf(initial) }
    DialogCard(title, Modifier.padding(16.dp), confirm = confirm, onConfirm = { if (v.isNotBlank()) onSave(v.trim()) }, onDismiss = onDismiss) {
        FieldLabel(label)
        CrundiTextField(v, { v = it }, Modifier.fillMaxWidth(), placeholder = placeholder, singleLine = !multiline, minLines = if (multiline) 3 else 1)
    }
}

// ─── Trash and history ───

@Composable
private fun KanbanTrash(board: KanbanBoard, cb: KanbanCallbacks) {
    val deletedTodos = board.tasks.flatMap { t -> t.todos.filter { it.deleted }.map { t to it } }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(bottom = 24.dp)) {
        SectionHeader("Deleted tasks")
        if (board.deletedTasks.isEmpty()) Muted("No deleted tasks.")
        for (t in board.deletedTasks) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp).clip(Dimens.Shape).background(CrundiColors.BgCard).border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape).padding(start = 12.dp, end = 6.dp, top = 6.dp, bottom = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(Modifier.weight(1f).alpha(0.6f)) {
                    Text(t.title, style = CrundiType.Body.copy(fontWeight = FontWeight.SemiBold), maxLines = 2, overflow = TextOverflow.Ellipsis)
                    Text(listOf(kanbanTitle(t.status), ago(t.deletedAt).let { if (it.isEmpty()) "" else "deleted $it" }).filter { it.isNotEmpty() }.joinToString(" · "), style = CrundiType.Meta)
                }
                CrundiButton("Restore", { cb.onRestoreTask(t) }, icon = "rotate-ccw")
            }
        }
        SectionHeader("Deleted todos", Modifier.padding(top = 8.dp))
        if (deletedTodos.isEmpty()) Muted("No deleted todos.")
        for ((t, d) in deletedTodos) {
            Row(Modifier.fillMaxWidth().padding(horizontal = Dimens.ScreenPad, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    androidx.compose.ui.text.buildAnnotatedString {
                        append(d.text)
                        pushStyle(androidx.compose.ui.text.SpanStyle(color = CrundiColors.TextMuted)); append(" in "); pop()
                        pushStyle(androidx.compose.ui.text.SpanStyle(color = CrundiColors.TextSecondary)); append(t.title); pop()
                    },
                    style = CrundiType.Body, modifier = Modifier.weight(1f),
                )
                CrundiButton("Restore", { cb.onRestoreTodo(t, d) }, kind = ButtonKind.Ghost)
            }
        }
    }
}

@Composable
private fun KanbanHistory(history: List<KanbanEvent>) {
    if (history.isEmpty()) { EmptyState("clock", "No history yet", body = "Adding, moving and finishing tasks shows up here."); return }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(vertical = 6.dp)) {
        for (e in history.sortedByDescending { it.ts }) {
            Column(Modifier.fillMaxWidth().padding(horizontal = Dimens.ScreenPad, vertical = 8.dp)) {
                Text(parseInstant(e.ts)?.let { shortDateTime(it.toEpochMilli()) } ?: e.ts, style = CrundiType.Meta)
                Text(e.message, style = CrundiType.Small.copy(color = CrundiColors.TextPrimary))
            }
            CrundiDivider(Modifier.padding(horizontal = Dimens.ScreenPad))
        }
    }
}

@Composable
internal fun Muted(text: String) = Text(text, style = CrundiType.Small.copy(color = CrundiColors.TextMuted), modifier = Modifier.padding(horizontal = Dimens.ScreenPad, vertical = 6.dp))
