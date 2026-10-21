package mv.obitech.crundi.ui.workbench

import android.content.Context
import android.content.SharedPreferences
import androidx.compose.runtime.staticCompositionLocalOf
import mv.obitech.crundi.api.model.AgentState
import mv.obitech.crundi.api.model.Pane
import org.json.JSONArray
import org.json.JSONObject

/**
 * What can sit in a workbench slot, with the icon and name the web gives it
 * (WB_KIND_META and the add menu). [unique]: at most one per project, as the
 * web's addWbPanel refuses duplicates (notes may repeat, one page each).
 */
enum class PaneKind(val key: String, val label: String, val icon: String, val unique: Boolean = true) {
    Chat("chat", "Chat", "message-square", false),
    Terminal("terminal", "Terminal", "terminal", false),
    Parked("parked", "Parked", "archive", false),
    Pending("pending", "New terminal", "terminal", false),
    Files("files", "Files", "folder"),
    Git("git", "Git", "git-branch"),
    Kanban("kanban", "Kanban", "kanban"),
    Notes("notes", "Notes", "note", false),
    MindMap("mindmap", "Mindmap", "mindmap"),
    Media("media", "Media", "images"),
    Browser("browser", "Browser", "globe", false),
    Factory("factory", "Factory", "factory"),
    Panel("widget", "Panel", "panel", false),
    ;

    companion object {
        fun byKey(k: String) = entries.firstOrNull { it.key == k }
        /** The panels a person can add (the add menu and the empty slot), in the web's order. */
        val addable = listOf(Terminal, Files, Git, Kanban, Notes, MindMap, Media, Browser, Factory, Panel)
    }
}

/**
 * A workbench cell that is not a server pane: Files, Git, Kanban, Notes,
 * Mindmap, Media, Browser, Factory, Panel. Kept on this phone per project,
 * as the web keeps them in its own storage. [data]: what the cell shows
 * (notes: pageId; widget: widgetId; browser: url, device; factory: runId).
 */
data class WbCell(val id: String, val kind: PaneKind, val data: Map<String, String> = emptyMap()) {
    val key get() = "w:$id"
}

/** A terminal pane being made: the launcher (Empty Shell, Claude UI Mode, Terminal). */
data class PendingCell(val id: String) {
    val key get() = "p:$id"
}

/** How the panes are laid out on a phone: swipe columns (the default), or stacked one above another. */
enum class WbLayout { Columns, Stack }

/** One pane as the frame draws it. Built from the server's pane, a WbCell, or a pending launcher. */
data class PaneView(
    val key: String,
    val kind: PaneKind,
    val title: String,
    /** Working, needs input, waiting: the dot and the badge. Null for panels. */
    val state: AgentState? = null,
    val exited: Boolean = false,
    val pane: Pane? = null,
    val cell: WbCell? = null,
) {
    val paneId: String? get() = pane?.id
    /** The kind of a parked pane: what it will be again. */
    val parkedAs: PaneKind get() = when { pane?.paneKind == "ui" -> PaneKind.Chat; else -> PaneKind.Terminal }

    companion object {
        fun ofPane(p: Pane): PaneView = PaneView(
            key = "t:" + p.id,
            kind = when { p.isParked -> PaneKind.Parked; p.isChat -> PaneKind.Chat; else -> PaneKind.Terminal },
            title = p.title.ifEmpty { if (p.isChat || p.paneKind == "ui") "Chat" else "Terminal" },
            state = if (p.isParked || p.status == "exited") null else p.state,
            exited = p.status == "exited" || p.isParked,
            pane = p,
        )

        fun ofCell(c: WbCell): PaneView = PaneView(c.key, c.kind, c.data["title"]?.ifEmpty { null } ?: c.kind.label, cell = c)
        fun ofPending(p: PendingCell): PaneView = PaneView(p.key, PaneKind.Pending, "New terminal")
    }
}

/** What a pane body knows about the frame it is in (null outside a pane). Screens use it to leave out a header the frame already draws. */
data class PaneInfo(val key: String, val kind: PaneKind, val fontScale: Float = 1f, val full: Boolean = false)

val LocalPane = staticCompositionLocalOf<PaneInfo?> { null }

/**
 * Where the workbench is kept on this phone, per server and project: the
 * layout tree, the panel cells, the new-terminal launchers, each pane's text
 * size, and the layout choice (columns or stack), as the web keeps them in
 * localStorage (crundi_wb_mosaic, crundi_wb_cells, crundi_mobile_mosaic).
 */
class WorkbenchStore(private val prefs: SharedPreferences?, private val server: String) {
    private val mem = HashMap<String, String>()
    private fun get(k: String): String? = prefs?.getString(k, null) ?: mem[k]
    private fun put(k: String, v: String?) {
        if (prefs != null) prefs.edit().apply { if (v == null) remove(k) else putString(k, v) }.apply()
        else if (v == null) mem.remove(k) else mem[k] = v
    }
    private fun k(what: String, project: String) = "wb.$what.$server.$project"

    fun tree(project: String): Mosaic? = Mosaic.fromJson(get(k("mosaic", project)))
    fun setTree(project: String, t: Mosaic) = put(k("mosaic", project), t.toJson())

    fun cells(project: String): List<WbCell> = runCatching {
        val a = JSONArray(get(k("cells", project)) ?: "[]")
        List(a.length()) { i ->
            val o = a.getJSONObject(i)
            val d = o.optJSONObject("data")
            WbCell(o.getString("id"), PaneKind.byKey(o.getString("kind")) ?: PaneKind.Files, d?.keys()?.asSequence()?.associateWith { d.optString(it) }.orEmpty())
        }
    }.getOrDefault(emptyList())

    fun setCells(project: String, cells: List<WbCell>) = put(k("cells", project), JSONArray(cells.map { c ->
        JSONObject().put("id", c.id).put("kind", c.kind.key).put("data", JSONObject(c.data))
    }).toString())

    fun pending(project: String): List<PendingCell> = get(k("pending", project))?.split(',')?.filter { it.isNotBlank() }?.map { PendingCell(it) }.orEmpty()
    fun setPending(project: String, p: List<PendingCell>) = put(k("pending", project), p.joinToString(",") { it.id }.ifEmpty { null })

    fun fontScale(key: String): Float = get("wb.font.$server.$key")?.toFloatOrNull() ?: 1f
    fun setFontScale(key: String, s: Float) = put("wb.font.$server.$key", if (s == 1f) null else s.toString())

    var layout: WbLayout
        get() = if (get("wb.layout") == "stack") WbLayout.Stack else WbLayout.Columns
        set(v) = put("wb.layout", if (v == WbLayout.Stack) "stack" else null)

    companion object {
        fun forServer(ctx: Context, serverId: String) = WorkbenchStore(ctx.getSharedPreferences("crundi.workbench", Context.MODE_PRIVATE), serverId)
        /** In memory only: tests and previews. */
        fun memory() = WorkbenchStore(null, "test")
    }
}

val LocalWorkbenchStore = staticCompositionLocalOf { WorkbenchStore.memory() }
