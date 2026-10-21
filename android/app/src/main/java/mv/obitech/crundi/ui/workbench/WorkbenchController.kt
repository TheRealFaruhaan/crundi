package mv.obitech.crundi.ui.workbench

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import mv.obitech.crundi.api.CrundiClient
import mv.obitech.crundi.api.model.NewChat
import mv.obitech.crundi.api.model.ServerState
import mv.obitech.crundi.ui.components.errorText
import mv.obitech.crundi.ui.shell.Host

/**
 * The workbench of one project: its layout, its panel cells and launchers,
 * which pane is in use and which is full screen. One per project, made by
 * [rememberWorkbench] in CrundiApp and shared through [LocalWorkbench], so
 * the "+" beside the Workbench tab and the Workbench itself act on the same.
 *
 * The server's panes (chats, terminals, parked) come and go with the live
 * state; [sync] keeps the layout in step, as the web's mosaicSync does.
 */
class WorkbenchController(
    val project: String?,
    private val store: WorkbenchStore,
    private val api: CrundiClient?,
    private val scope: CoroutineScope,
    private val host: Host,
) : WorkbenchActions {
    var tree by mutableStateOf(project?.let { store.tree(it) } ?: Mosaic.leaf(null)); private set
    var cells by mutableStateOf(project?.let { store.cells(it) }.orEmpty()); private set
    var pending by mutableStateOf(project?.let { store.pending(it) }.orEmpty()); private set
    var layout by mutableStateOf(store.layout); private set
    var focused by mutableStateOf<String?>(null); private set
    var full by mutableStateOf<String?>(null); private set
    /** A pane asking for its name to be changed (the route shows the dialog). */
    var renaming by mutableStateOf<PaneView?>(null)
    /** Something that went wrong, for the route to show once. */
    var notice by mutableStateOf<String?>(null)
    /** Which column to bring into view (a pane was just added); the screen takes it and clears it. */
    var reveal by mutableStateOf<String?>(null)
    val fontScales = mutableStateMapOf<String, Float>()
    /** Launchers working, and why one failed. */
    val launching = mutableStateMapOf<String, Boolean>()
    val launchErrors = mutableStateMapOf<String, String>()
    /** Panes resuming. */
    val resuming = mutableStateMapOf<String, Boolean>()
    /** Bumped to reload a panel (the header's Refresh). */
    val refreshes = mutableStateMapOf<String, Int>()

    private var serverPanes: List<mv.obitech.crundi.api.model.Pane> = emptyList()
    /** Panes just made here, kept in the layout until the server lists them. */
    private val grace = HashMap<String, Long>()

    private fun save() { project ?: return; store.setTree(project, tree); store.setCells(project, cells); store.setPending(project, pending) }

    /** Every pane by key, as the frame draws it. */
    fun panes(): Map<String, PaneView> = buildMap {
        for (p in serverPanes) put("t:" + p.id, PaneView.ofPane(p))
        for (c in cells) put(c.key, PaneView.ofCell(c))
        for (p in pending) put(p.key, PaneView.ofPending(p))
    }

    fun ui() = WorkbenchUi(project, tree, panes(), layout, focused, full, fontScales.toMap())

    /** Follow the live state: the project's panes in their order, then launchers, then panels. */
    fun sync(state: ServerState?) {
        if (project == null || state == null) return
        serverPanes = state.terminals.filter { it.project.equals(project, true) }.sortedBy { it.order }
        val now = System.currentTimeMillis()
        grace.entries.removeAll { now - it.value > 20_000 || serverPanes.any { p -> "t:" + p.id == it.key } }
        val keys = serverPanes.map { "t:" + it.id } + grace.keys + pending.map { it.key } + cells.map { it.key }
        val next = tree.sync(keys)
        if (next != tree) { tree = next; save() }
        if (fontScales.isEmpty()) for (k in keys) store.fontScale(k).takeIf { it != 1f }?.let { fontScales[k] = it }
    }

    /** Add a panel (the "+" menu, or a slot's launcher into that slot). Kinds that exist once just come into view. */
    fun add(kind: PaneKind, leafId: String? = null) {
        if (project == null) return
        val key: String
        if (kind == PaneKind.Terminal) {
            val p = PendingCell(Mosaic.newId())
            pending = pending + p
            key = p.key
        } else {
            val have = if (kind.unique) cells.firstOrNull { it.kind == kind } else null
            if (have != null) { reveal = have.key; focused = have.key; return }
            val c = WbCell(Mosaic.newId(), kind)
            cells = cells + c
            key = c.key
        }
        tree = if (leafId != null) tree.assign(leafId, key) else tree.addKey(key)
        save()
        focused = key
        reveal = key
    }

    /** Share the width out again: every pane its own column, in order (the add menu's Fit). */
    fun fit() {
        tree = Mosaic.default(tree.keys())
        save()
    }

    fun toggleLayout() {
        layout = if (layout == WbLayout.Columns) WbLayout.Stack else WbLayout.Columns
        store.layout = layout
    }

    /** Start what a launcher was asked for, and put the new pane where the launcher was. */
    fun launch(pendingKey: String, mode: LaunchMode) {
        val p = project ?: return
        val a = api ?: return
        launching[pendingKey] = true
        launchErrors.remove(pendingKey)
        scope.launch {
            try {
                val created = when (mode) {
                    LaunchMode.Chat, LaunchMode.ChatSkip -> a.chats.create(NewChat(project = p, skipPermissions = (mode == LaunchMode.ChatSkip).takeIf { it }))
                    LaunchMode.Shell -> a.terminals.create(p, shell = true)
                    LaunchMode.Terminal, LaunchMode.TerminalSkip -> a.terminals.create(p, skipPermissions = (mode == LaunchMode.TerminalSkip).takeIf { it })
                }
                val newKey = "t:" + created.id
                grace[newKey] = System.currentTimeMillis()
                pending = pending.filter { it.key != pendingKey }
                tree = tree.leafOf(pendingKey)?.let { l -> tree.assign(l.id, newKey) } ?: tree.addKey(newKey)
                save()
                focused = newKey
            } catch (e: Exception) {
                launchErrors[pendingKey] = errorText(e)
            } finally {
                launching.remove(pendingKey)
            }
        }
    }

    private fun act(what: String, block: suspend () -> Unit) {
        scope.launch { try { block() } catch (e: Exception) { notice = "$what: " + errorText(e) } }
    }

    // ─── PaneActions ───

    override fun focus(v: PaneView) { focused = v.key }

    override fun full(v: PaneView, on: Boolean) { full = if (on) v.key else null; if (on) focused = v.key }

    override fun rename(v: PaneView) { renaming = v }

    fun applyRename(v: PaneView, title: String) {
        val id = v.paneId ?: return
        val t = title.trim().take(80)
        if (t.isEmpty()) return
        act("Rename failed") { if (v.kind == PaneKind.Chat) api?.chats?.rename(id, t) else api?.terminals?.rename(id, t) }
    }

    /**
     * Change what a panel cell shows, kept with the cell (notes: pageId and
     * title; factory: runId and title). A null value removes that key.
     */
    fun setCellData(key: String, vararg changes: Pair<String, String?>) {
        val cell = cells.firstOrNull { it.key == key } ?: return
        if (changes.all { (k, value) -> cell.data[k] == value }) return
        cells = cells.map { c ->
            if (c.key != key) c else {
                val d = c.data.toMutableMap()
                for ((k, value) in changes) if (value == null) d.remove(k) else d[k] = value
                c.copy(data = d)
            }
        }
        save()
    }

    override fun close(v: PaneView) {
        when (v.kind) {
            PaneKind.Chat -> act("Could not close the chat") { api?.chats?.close(v.paneId!!) }
            PaneKind.Terminal -> act("Could not close the terminal") { api?.terminals?.close(v.paneId!!) }
            PaneKind.Parked -> act("Could not close it") { api?.terminals?.discard(v.paneId!!) }
            PaneKind.Pending -> { pending = pending.filter { it.key != v.key }; tree = tree.removeKey(v.key); save() }
            else -> { cells = cells.filter { it.key != v.key }; tree = tree.removeKey(v.key); save() }
        }
        if (full == v.key) full = null
    }

    override fun font(v: PaneView, delta: Int) {
        val now = fontScales[v.key] ?: 1f
        val next = if (delta == 0) 1f else (now + 0.1f * delta).coerceIn(0.7f, 1.6f)
        if (next == 1f) fontScales.remove(v.key) else fontScales[v.key] = next
        store.setFontScale(v.key, next)
    }

    override fun bubble(v: PaneView) { v.paneId?.let { host.openBubble(it) } }

    override fun split(v: PaneView, dir: Mosaic.Dir) {
        val l = tree.leafOf(v.key) ?: return
        tree = tree.splitLeaf(l.id, dir); save()
    }

    override fun move(v: PaneView, by: Int) { tree = tree.moveColumn(v.key, by); save(); reveal = v.key }

    override fun refresh(v: PaneView) { refreshes[v.key] = (refreshes[v.key] ?: 0) + 1 }

    override fun resume(v: PaneView) {
        val id = v.paneId ?: return
        resuming[v.key] = true
        scope.launch {
            try {
                val newId = api?.terminals?.resume(id) ?: return@launch
                val newKey = "t:$newId"
                if (newKey != v.key) {
                    grace[newKey] = System.currentTimeMillis()
                    tree = tree.leafOf(v.key)?.let { l -> tree.assign(l.id, newKey) } ?: tree
                    save()
                }
            } catch (e: Exception) {
                notice = "Could not resume it: " + errorText(e)
            } finally { resuming.remove(v.key) }
        }
    }

    // ─── WorkbenchActions ───

    override fun launch(slotLeafId: String, kind: PaneKind) = add(kind, slotLeafId)

    override fun splitSlot(leafId: String, dir: Mosaic.Dir) { tree = tree.splitLeaf(leafId, dir); save() }

    override fun removeSlot(leafId: String) { tree = tree.collapse(leafId); save() }

    override fun resize(splitId: String, sizes: List<Float>) { tree = tree.resize(splitId, sizes); save() }
}

val LocalWorkbench = compositionLocalOf<WorkbenchController?> { null }

/** The workbench of the current project, kept in step with the live state. */
@Composable
fun rememberWorkbench(api: CrundiClient?, project: String?, store: WorkbenchStore, host: Host): WorkbenchController {
    val scope = rememberCoroutineScope()
    val wb = remember(api, project, store) { WorkbenchController(project, store, api, scope, host) }
    if (api != null) LaunchedEffect(wb) { api.events.state.collect { wb.sync(it) } }
    return wb
}
