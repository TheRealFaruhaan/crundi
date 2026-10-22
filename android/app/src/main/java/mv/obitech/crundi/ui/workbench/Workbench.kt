package mv.obitech.crundi.ui.workbench

import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.PagerState
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.movableContentOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.theme.CrundiColors

/** Everything the workbench draws, for one project. */
data class WorkbenchUi(
    val project: String?,
    val tree: Mosaic,
    /** Every pane by its key (t:, w:, p:). A leaf whose key is missing here shows as empty. */
    val panes: Map<String, PaneView>,
    val layout: WbLayout = WbLayout.Columns,
    val focused: String? = null,
    /** The pane over the whole screen, if any. */
    val full: String? = null,
    val fontScales: Map<String, Float> = emptyMap(),
)

/** What the workbench itself does, besides each pane's header ([PaneActions]). */
interface WorkbenchActions : PaneActions {
    fun launch(slotLeafId: String, kind: PaneKind) {}
    fun splitSlot(leafId: String, dir: Mosaic.Dir) {}
    fun removeSlot(leafId: String) {}
    fun resize(splitId: String, sizes: List<Float>) {}
    object None : WorkbenchActions
}

/**
 * The Workbench tab: the project's panes as the mobile web lays them out.
 * Columns (the default): each top-level column fills the width and the row
 * snaps when swiped sideways, a thin bar at the bottom says where you are;
 * a column can be split top and bottom with a divider to drag. Stack: every
 * pane one above another, scrolled (the web's mobile layout toggle).
 *
 * [body] draws a pane's content (PaneBodies.kt in the app; fakes in tests).
 */
@Composable
fun WorkbenchScreen(
    ui: WorkbenchUi,
    actions: WorkbenchActions,
    modifier: Modifier = Modifier,
    pager: PagerState = rememberPagerState { ui.tree.columns().size },
    body: @Composable (PaneView) -> Unit,
) {
    if (ui.project == null) {
        EmptyState("terminal", "Pick a project", body = "The workbench holds a project's chats, terminals and panels. Open the menu and choose one.", modifier = modifier)
        return
    }
    val open = ui.panes.values.map { it.kind }.toSet()
    // Each pane's frame and content, movable: when the layout changes (a
    // split, a removed slot, a column added) the pane moves to its new place
    // with everything in it, instead of being built again; a chat was
    // "Opening the chat" all over after every split.
    val movables = remember { HashMap<String, @Composable (PaneView, WorkbenchUi, Boolean, @Composable (PaneView) -> Unit) -> Unit>() }
    movables.keys.retainAll(ui.panes.keys)
    val slot: @Composable (PaneView, Boolean) -> Unit = { v, focused ->
        val m = movables.getOrPut(v.key) {
            movableContentOf { pv: PaneView, u: WorkbenchUi, f: Boolean, b: @Composable (PaneView) -> Unit ->
                PaneFrame(pv, actions, Modifier.fillMaxSize().focusOnTouch { actions.focus(pv) }, focused = f) { PaneContent(pv, u, full = false, b) }
            }
        }
        m(v, ui, focused, body)
    }
    // Splits made from now on open with a short slide; the ones already there do not.
    val bornAt = remember { System.currentTimeMillis() }
    val node: @Composable (Mosaic) -> Unit = { n -> MosaicNode(n, ui, actions, open, body) }
    CompositionLocalProvider(LocalPaneSlot provides slot, LocalWorkbenchBorn provides bornAt) {
    Box(modifier.fillMaxSize().background(CrundiColors.BgPrimary)) {
        when {
            ui.layout == WbLayout.Stack -> StackLayout(ui, actions, open, body)
            else -> {
                val cols = ui.tree.columns()
                if (cols.size <= 1) Box(Modifier.fillMaxSize().padding(4.dp)) { node(ui.tree) }
                else {
                    HorizontalPager(pager, Modifier.fillMaxSize(), beyondViewportPageCount = 1, key = { cols.getOrNull(it)?.id ?: it }) { i ->
                        Box(Modifier.fillMaxSize().padding(start = 4.dp, end = 4.dp, top = 4.dp, bottom = 10.dp)) { cols.getOrNull(i)?.let { node(it) } }
                    }
                    ColumnIndicator(pager, cols.size, Modifier.align(Alignment.BottomCenter))
                }
            }
        }
    }
    }
    ui.full?.let { k ->
        val v = ui.panes[k] ?: return@let
        Dialog(onDismissRequest = { actions.full(v, false) }, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
            // Full screen means the whole screen: the status bar goes (a
            // swipe from the top shows it for a moment), and it comes back
            // when the pane leaves full screen. Before, the pane's header was
            // drawn under the clock and icons.
            val view = androidx.compose.ui.platform.LocalView.current
            androidx.compose.runtime.DisposableEffect(view) {
                val window = (view.parent as? androidx.compose.ui.window.DialogWindowProvider)?.window
                val ctl = window?.let { androidx.core.view.WindowCompat.getInsetsController(it, view) }
                ctl?.systemBarsBehavior = androidx.core.view.WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
                ctl?.hide(androidx.core.view.WindowInsetsCompat.Type.statusBars())
                onDispose { ctl?.show(androidx.core.view.WindowInsetsCompat.Type.statusBars()) }
            }
            PaneFrame(v, actions, Modifier.fillMaxSize(), focused = true, full = true) { PaneContent(v, ui, full = true, body) }
        }
    }
}

/** The thin bar under the columns, as the browser's scroll bar on the mobile web: where you are along the row. */
@Composable
private fun ColumnIndicator(pager: PagerState, count: Int, modifier: Modifier) {
    BoxWithConstraints(modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 3.dp).height(4.dp)) {
        val track = maxWidth
        val thumb = track / count
        val pos = (pager.currentPage + pager.currentPageOffsetFraction).coerceIn(0f, (count - 1).toFloat())
        Box(Modifier.offset(x = thumb * pos).width(thumb).fillMaxHeight().clip(RoundedCornerShape(2.dp)).background(CrundiColors.TextMuted.copy(alpha = 0.55f)))
    }
}

@Composable
private fun MosaicNode(n: Mosaic, ui: WorkbenchUi, actions: WorkbenchActions, open: Set<PaneKind>, body: @Composable (PaneView) -> Unit) {
    when (n) {
        is Mosaic.Leaf -> {
            val v = n.key?.let { ui.panes[it] }
            if (v == null) EmptySlot(open, { actions.launch(n.id, it) }, { actions.splitSlot(n.id, it) }, if (ui.tree.leaves().size > 1) ({ actions.removeSlot(n.id) }) else null)
            else if (ui.full == v.key) Box(Modifier.fillMaxSize().clip(RoundedCornerShape(10.dp)).background(CrundiColors.BgSecondary))
            else LocalPaneSlot.current(v, ui.focused == v.key)
        }
        is Mosaic.Split -> SplitNode(n, ui, actions, open, body)
    }
}

/** A split: its children by their sizes, a divider between each to drag. Rows inside a column stay side by side. */
@Composable
private fun SplitNode(s: Mosaic.Split, ui: WorkbenchUi, actions: WorkbenchActions, open: Set<PaneKind>, body: @Composable (PaneView) -> Unit) {
    var sizes by remember(s.id, s.sizes) { mutableStateOf(s.sizes) }
    // A split just made grows its new part from nothing (the web's slot
    // sliding in), rather than the layout jumping.
    val born = LocalWorkbenchBorn.current
    val grow = remember(s.id) { androidx.compose.animation.core.Animatable(if (System.currentTimeMillis() - born > 800) 0f else 1f) }
    LaunchedEffect(s.id) { grow.animateTo(1f, androidx.compose.animation.core.tween(260)) }
    fun weightOf(i: Int): Float {
        val base = sizes.getOrElse(i) { 1f }.coerceAtLeast(1f)
        val last = s.kids.lastIndex
        if (grow.value >= 1f || last < 1) return base
        val newest = sizes.getOrElse(last) { 1f }.coerceAtLeast(1f)
        return when (i) {
            last -> (newest * grow.value).coerceAtLeast(0.01f)
            last - 1 -> base + newest * (1f - grow.value)
            else -> base
        }
    }
    var total by remember { mutableIntStateOf(1) }
    val density = LocalDensity.current
    val vertical = s.dir == Mosaic.Dir.Col
    val content: @Composable (Int) -> Unit = { i ->
        if (i > 0) Divider(vertical, onDrag = { px ->
            val d = px / total * 100f
            val a = sizes[i - 1] + d; val b = sizes[i] - d
            if (a >= 12f && b >= 12f) sizes = sizes.toMutableList().also { it[i - 1] = a; it[i] = b }
        }, onEnd = { actions.resize(s.id, sizes) })
    }
    if (vertical) Column(Modifier.fillMaxSize().onSizeChanged { total = it.height.coerceAtLeast(1) }) {
        s.kids.forEachIndexed { i, kid ->
            content(i)
            Box(Modifier.fillMaxWidth().weight(weightOf(i)).clipToBounds()) { MosaicNode(kid, ui, actions, open, body) }
        }
    } else Row(Modifier.fillMaxSize().onSizeChanged { total = it.width.coerceAtLeast(1) }) {
        s.kids.forEachIndexed { i, kid ->
            content(i)
            Box(Modifier.fillMaxHeight().weight(weightOf(i)).clipToBounds()) { MosaicNode(kid, ui, actions, open, body) }
        }
    }
}

/** The gutter between two panes (.mosaic-gutter): an 8dp strip with a hairline, accent while it is dragged. */
@Composable
private fun Divider(vertical: Boolean, onDrag: (Float) -> Unit, onEnd: () -> Unit) {
    var dragging by remember { mutableStateOf(false) }
    val line = if (dragging) CrundiColors.Accent else CrundiColors.Border
    if (vertical) Box(
        Modifier.fillMaxWidth().height(10.dp).pointerInput(Unit) {
            detectVerticalDragGestures(onDragStart = { dragging = true }, onDragEnd = { dragging = false; onEnd() }, onDragCancel = { dragging = false }) { c, dy -> c.consume(); onDrag(dy) }
        },
        contentAlignment = Alignment.Center,
    ) { Box(Modifier.width(36.dp).height(if (dragging) 3.dp else 2.dp).clip(RoundedCornerShape(2.dp)).background(line)) }
    else Box(
        Modifier.fillMaxHeight().width(10.dp).pointerInput(Unit) {
            detectHorizontalDragGestures(onDragStart = { dragging = true }, onDragEnd = { dragging = false; onEnd() }, onDragCancel = { dragging = false }) { c, dx -> c.consume(); onDrag(dx) }
        },
        contentAlignment = Alignment.Center,
    ) { Box(Modifier.height(36.dp).width(if (dragging) 3.dp else 2.dp).clip(RoundedCornerShape(2.dp)).background(line)) }
}

/** The stack layout: every pane in order, each at least 320dp high (the web's flat mobile stack). */
@Composable
private fun StackLayout(ui: WorkbenchUi, actions: WorkbenchActions, open: Set<PaneKind>, body: @Composable (PaneView) -> Unit) {
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val leaves = ui.tree.leaves()
        val each = maxOf(320.dp, (maxHeight - 8.dp) / leaves.size.coerceAtLeast(1))
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(4.dp)) {
            for (l in leaves) Box(Modifier.fillMaxWidth().height(each).padding(bottom = 6.dp)) {
                MosaicNode(l, ui, actions, open, body)
            }
        }
    }
}

/** Draws a pane where it stands in the layout, keeping it alive as it moves (see WorkbenchScreen). */
private val LocalPaneSlot = androidx.compose.runtime.staticCompositionLocalOf<@Composable (PaneView, Boolean) -> Unit> { { _, _ -> } }
private val LocalWorkbenchBorn = androidx.compose.runtime.staticCompositionLocalOf { 0L }

/** A pane's body with [LocalPane] set, so the screen inside knows it is in a pane. */
@Composable
private fun PaneContent(v: PaneView, ui: WorkbenchUi, full: Boolean, body: @Composable (PaneView) -> Unit) {
    CompositionLocalProvider(LocalPane provides PaneInfo(v.key, v.kind, ui.fontScales[v.key] ?: 1f, full)) {
        Box(Modifier.fillMaxSize()) { body(v) }
    }
}

/** Mark a pane as the one in use when it is touched, without taking the touch from what is inside it. */
private fun Modifier.focusOnTouch(onFocus: () -> Unit): Modifier = pointerInput(Unit) {
    awaitPointerEventScope {
        while (true) {
            val e = awaitPointerEvent(PointerEventPass.Initial)
            if (e.changes.any { it.pressed && !it.previousPressed }) onFocus()
        }
    }
}

/** Used by the add menu's "Fit" and the empty workbench: at least one slot to launch into. */
fun Mosaic.withSlot(): Mosaic = if (leaves().isEmpty()) Mosaic.leaf(null) else this
