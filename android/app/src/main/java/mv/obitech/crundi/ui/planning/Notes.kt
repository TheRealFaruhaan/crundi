package mv.obitech.crundi.ui.planning

import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.text.TextLayoutResult
import mv.obitech.crundi.ui.workbench.DropKind
import mv.obitech.crundi.ui.workbench.PaneDrop
import mv.obitech.crundi.ui.workbench.paneDropTarget

import androidx.compose.foundation.layout.heightIn

import mv.obitech.crundi.ui.components.errorText

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.model.NoteBlock
import mv.obitech.crundi.api.model.NotePage
import mv.obitech.crundi.api.model.NoteSummary
import mv.obitech.crundi.api.str
import mv.obitech.crundi.ui.components.Banner
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.CrundiChip
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiDivider
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.Highlighter
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.TopBar
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.shell.Area
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens
import java.util.UUID

object NotesRoutes {
    const val PAGE = "notes/page/{id}"
    fun page(id: String) = "notes/page/" + java.net.URLEncoder.encode(id, "UTF-8")
}

/** Work that must finish even when its screen has gone (the last save of a page). */
internal val PlanningScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

// ═══ Page list ═══

@Stable
class NotesListCallbacks(
    val onOpen: (NoteSummary) -> Unit = {},
    val onNew: () -> Unit = {},
    val onTrash: (NoteSummary) -> Unit = {},
    val onRestore: (NoteSummary) -> Unit = {},
    val onPurge: (NoteSummary) -> Unit = {},
    val onRetry: () -> Unit = {},
)

@Composable
fun NotesRoute(pane: NotePickerHooks? = null) = WithProject(Area.Notes) { project ->
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val loader = rememberLoader(project, events = arrayOf("notes"), accept = { it.project == null || it.project.equals(project, true) }) {
        api.notes.pages(project, includeDeleted = true)
    }
    val actions = rememberActions { loader.refresh() }
    var trash by remember(project) { mutableStateOf(false) }
    var query by remember(project) { mutableStateOf("") }
    val cb = remember(project) {
        NotesListCallbacks(
            onOpen = { s ->
                when {
                    pane == null -> nav.to(NotesRoutes.page(s.id))
                    pane.busy(s.id) -> actions.message = "That page is open in another pane"
                    else -> pane.onPick(s.id, s.title)
                }
            },
            onNew = { actions.run { val p = api.notes.create(project); if (pane != null) pane.onPick(p.id, p.title) else nav.to(NotesRoutes.page(p.id)) } },
            onTrash = { s -> loader.update { l -> l.map { if (it.id == s.id) it.copy(deleted = true) else it } }; actions.run { api.notes.delete(project, s.id) } },
            onRestore = { s -> actions.run { api.notes.restore(project, s.id) } },
            onPurge = { s -> actions.run { api.notes.purge(project, s.id) } },
            onRetry = { loader.refresh() },
        )
    }
    Box(Modifier.fillMaxSize()) {
        NotesListScreen(loader.load, trash, { trash = it }, query, { query = it }, cb)
        ActionError(actions)
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
fun NotesListScreen(
    pages: Load<List<NoteSummary>>,
    trash: Boolean,
    onTrashView: (Boolean) -> Unit,
    query: String,
    onQuery: (String) -> Unit,
    cb: NotesListCallbacks = NotesListCallbacks(),
    now: Long = System.currentTimeMillis(),
) {
    var menuFor by remember { mutableStateOf<NoteSummary?>(null) }
    var purgeFor by remember { mutableStateOf<NoteSummary?>(null) }
    Column(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxWidth().padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(
                Modifier.fillMaxWidth().clip(Dimens.Shape).dashedBorder(CrundiColors.Border, Dimens.Radius).clickable(onClick = cb.onNew).padding(vertical = 12.dp),
                horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically,
            ) {
                CrundiIcon("plus", size = 15.dp, tint = CrundiColors.TextPrimary)
                Text("New page", style = CrundiType.Body.copy(fontWeight = FontWeight.SemiBold), modifier = Modifier.padding(start = 6.dp))
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                CrundiTextField(query, onQuery, Modifier.weight(1f), placeholder = "Search pages", icon = "search")
                CrundiChip("Pages", !trash, { onTrashView(false) })
                CrundiChip("Trash", trash, { onTrashView(true) })
            }
        }
        CrundiDivider()
        when (pages) {
            is Load.Loading -> LoadingState(text = "Loading pages")
            is Load.Failed -> ErrorState(pages.message, onRetry = cb.onRetry)
            is Load.Ready -> {
                val q = query.trim().lowercase()
                val list = pages.value.filter { it.deleted == trash }
                    .filter { q.isEmpty() || it.title.lowercase().contains(q) || it.preview.lowercase().contains(q) }
                    .sortedByDescending { if (trash) it.deletedAt ?: it.updatedAt else it.updatedAt }
                when {
                    list.isEmpty() && q.isNotEmpty() -> EmptyState("search", "No page matches.", body = "Try other words, or clear the search.")
                    list.isEmpty() && trash -> EmptyState("trash", "The trash is empty.", body = "Pages you move to the trash wait here until you delete them for good.")
                    list.isEmpty() -> EmptyState("note", "No pages yet", body = "Start one with New page.", action = "New page", onAction = cb.onNew)
                    else -> LazyColumn(Modifier.fillMaxSize(), contentPadding = androidx.compose.foundation.layout.PaddingValues(8.dp)) {
                        items(list, key = { it.id }) { p ->
                            if (trash) TrashRow(p, cb, { purgeFor = p })
                            else Column(
                                Modifier.fillMaxWidth().clip(RoundedCornerShape(8.dp))
                                    .combinedClickable(onClick = { cb.onOpen(p) }, onLongClick = { menuFor = p })
                                    .padding(horizontal = 11.dp, vertical = 10.dp),
                                verticalArrangement = Arrangement.spacedBy(2.dp),
                            ) {
                                Text(p.title.ifBlank { "Untitled" }, style = CrundiType.Body.copy(fontWeight = FontWeight.SemiBold, color = if (p.title.isBlank()) CrundiColors.TextSecondary else CrundiColors.TextPrimary), maxLines = 1, overflow = TextOverflow.Ellipsis)
                                if (p.preview.isNotBlank()) Text(p.preview, style = CrundiType.Small.copy(fontSize = 12.5.sp), maxLines = 2, overflow = TextOverflow.Ellipsis)
                                Text("Edited " + ago(p.updatedAt, now), style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans))
                            }
                        }
                    }
                }
            }
        }
    }
    menuFor?.let { p ->
        PlanningSheet(p.title.ifBlank { "Untitled" }, { menuFor = null }) {
            MenuRow("Open", "file-text", { menuFor = null; cb.onOpen(p) })
            MenuRow("Move to trash", "trash", { menuFor = null; cb.onTrash(p) }, danger = true)
        }
    }
    purgeFor?.let { p ->
        CrundiDialog(
            "Delete for good", { purgeFor = null }, message = "Delete “${p.title.ifBlank { "Untitled" }}” for good? This cannot be undone.",
            confirm = "Delete for good", danger = true, onConfirm = { purgeFor = null; cb.onPurge(p) },
        )
    }
}

@Composable
private fun TrashRow(p: NoteSummary, cb: NotesListCallbacks, onPurge: () -> Unit) {
    Row(Modifier.fillMaxWidth().padding(horizontal = 11.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Column(Modifier.weight(1f)) {
            Text(p.title.ifBlank { "Untitled" }, style = CrundiType.Body.copy(fontWeight = FontWeight.SemiBold), maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(ago(p.deletedAt).let { if (it.isEmpty()) "In the trash" else "Trashed $it" }, style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans))
        }
        CrundiButton("Restore", { cb.onRestore(p) })
        CrundiButton("Delete for good", onPurge, kind = ButtonKind.Danger)
    }
}

// ═══ Page editor ═══

/** Block kinds, in the desktop's slash-menu order: (type, name, key glyph, markdown hint). */
val NOTE_BLOCK_KINDS = listOf(
    listOf("p", "Text", "T", ""),
    listOf("h1", "Heading 1", "H1", "#"),
    listOf("h2", "Heading 2", "H2", "##"),
    listOf("h3", "Heading 3", "H3", "###"),
    listOf("todo", "To-do", "☐", "[]"),
    listOf("bullet", "Bulleted list", "•", "-"),
    listOf("number", "Numbered list", "1.", "1."),
    listOf("quote", "Quote", "“", ">"),
    listOf("code", "Code", "</>", "```"),
    listOf("table", "Table", "▦", ""),
    listOf("divider", "Divider", "—", "---"),
)

private val TEXT_TYPES = setOf("p", "h1", "h2", "h3", "todo", "bullet", "number", "quote")

/** One block while editing. Text kinds keep their text as editing source ([NoteText]). */
data class EditBlock(
    val id: String,
    val type: String,
    val source: String = "",
    val checked: Boolean = false,
    val lang: String = "",
    val wrap: Boolean = false,
    val rows: List<List<String>> = emptyList(),
    val header: Boolean = false,
) {
    val isText get() = type in TEXT_TYPES

    fun toBlock() = NoteBlock(
        id, type,
        text = when { isText -> NoteText.sourceToHtml(source); type == "code" -> source; else -> "" },
        checked = checked && type == "todo", lang = lang, wrap = wrap,
        rows = if (type == "table") rows.map { r -> r.map { NoteText.sourceToHtml(it) } } else emptyList(), header = header,
    )

    companion object {
        fun from(b: NoteBlock) = EditBlock(
            b.id, b.type,
            source = when { b.type in TEXT_TYPES -> NoteText.htmlToSource(b.text); else -> b.text },
            checked = b.checked, lang = b.lang, wrap = b.wrap,
            rows = b.rows.map { r -> r.map { NoteText.htmlToSource(it) } }, header = b.header,
        )
        fun newId() = "b" + UUID.randomUUID().toString().replace("-", "").take(10)
    }
}

enum class SaveStatus(val label: String) { Loading("Loading…"), Editing("Editing…"), Saving("Saving…"), Saved("Saved"), Offline("Offline — will retry"), Failed("Not saved"), Trash("In the trash") }

/**
 * A page being edited: its title and blocks, which block has the caret,
 * and how saving is going. Plain Compose state, so a screenshot test can
 * build one from a page and the route can save it.
 */
@Stable
class NoteEditor(page: NotePage) {
    var id by mutableStateOf(page.id)
        private set
    var title by mutableStateOf(page.title)
    val blocks = mutableStateListOf<EditBlock>()
    var version by mutableIntStateOf(page.version)
    var updatedAt by mutableStateOf(page.updatedAt)
    var deleted by mutableStateOf(page.deleted)
    var status by mutableStateOf(if (page.deleted) SaveStatus.Trash else SaveStatus.Saved)
    var error by mutableStateOf<String?>(null)
    /** Bumped on every edit; [saved] is the value the server has. */
    var changes by mutableIntStateOf(0)
        private set
    var saved by mutableIntStateOf(0)
    var focused by mutableStateOf<String?>(null)
    var value by mutableStateOf(TextFieldValue())
    /** A sheet to open: "turn" (turn the focused block into) or "add" (add a block below it). */
    var menu by mutableStateOf<String?>(null)
    var linkAsk by mutableStateOf(false)

    init { load(page) }

    val dirty get() = changes != saved

    fun load(page: NotePage) {
        id = page.id; title = page.title; version = page.version; updatedAt = page.updatedAt; deleted = page.deleted
        blocks.clear(); blocks.addAll(page.blocks.map(EditBlock::from))
        if (blocks.isEmpty()) blocks.add(EditBlock(EditBlock.newId(), "p"))
        status = if (page.deleted) SaveStatus.Trash else SaveStatus.Saved
        focused = null
        saved = changes
    }

    fun touched() { if (!deleted) { changes++; if (status != SaveStatus.Saving) status = SaveStatus.Editing } }

    fun index(id: String) = blocks.indexOfFirst { it.id == id }
    fun block(id: String) = blocks.firstOrNull { it.id == id }

    fun set(b: EditBlock) { val i = index(b.id); if (i >= 0 && blocks[i] != b) { blocks[i] = b; touched() } }

    fun focus(id: String?, cursor: Int? = null) {
        focused = id
        val b = id?.let { block(it) } ?: return
        value = TextFieldValue(b.source, TextRange(cursor ?: b.source.length))
    }

    fun rename(t: String) { if (t != title) { title = t; touched() } }

    /** The caret's block took new text: Enter splits it, markdown at the start turns its kind, "/" opens the block menu. */
    fun edit(id: String, v: TextFieldValue) {
        val b = block(id) ?: return
        val old = b.source
        val t = v.text
        // Enter: a newline typed at the caret.
        if (t.length == old.length + 1 && v.selection.collapsed && v.selection.start > 0 && t[v.selection.start - 1] == '\n') {
            val at = v.selection.start
            val before = t.substring(0, at - 1)
            val after = t.substring(at)
            if (before.isEmpty() && after.isEmpty() && b.type in setOf("bullet", "number", "todo", "quote")) {
                set(b.copy(type = "p", source = "")); value = TextFieldValue(""); return
            }
            set(b.copy(source = before))
            val kind = if (b.type in setOf("bullet", "number", "todo")) b.type else "p"
            val nb = EditBlock(EditBlock.newId(), kind, after)
            blocks.add(index(id) + 1, nb); touched()
            focus(nb.id, 0)
            return
        }
        if (b.type == "p" && t.length > old.length) {
            val shortcut = listOf("### " to "h3", "## " to "h2", "# " to "h1", "- " to "bullet", "* " to "bullet", "1. " to "number", "1) " to "number", "[] " to "todo", "[ ] " to "todo", "[x] " to "todo", "> " to "quote")
                .firstOrNull { t.startsWith(it.first) && !old.startsWith(it.first) }
            if (shortcut != null) {
                val rest = t.removePrefix(shortcut.first)
                set(b.copy(type = shortcut.second, source = rest, checked = shortcut.first == "[x] ")); value = TextFieldValue(rest, TextRange(0)); return
            }
            if (t == "---") { set(b.copy(type = "divider", source = "")); val nb = EditBlock(EditBlock.newId(), "p"); blocks.add(index(id) + 1, nb); touched(); focus(nb.id); return }
            if (t.startsWith("```") && t.endsWith(" ") && !t.trim().contains(' ')) { set(b.copy(type = "code", source = "", lang = t.trim().removePrefix("```"))); focused = null; return }
            if (t == "/") { value = v; set(b.copy(source = t)); menu = "turn"; return }
        }
        value = v
        set(b.copy(source = t))
    }

    /** Backspace at the very start: a list item becomes text; text merges into the block above. */
    fun backspaceAtStart(id: String): Boolean {
        val i = index(id); if (i < 0) return false
        val b = blocks[i]
        if (b.type != "p") { set(b.copy(type = "p")); return true }
        if (i == 0) return false
        val prev = blocks[i - 1]
        if (!prev.isText) { blocks.removeAt(i - 1); touched(); return true }
        val at = prev.source.length
        blocks[i - 1] = prev.copy(source = prev.source + b.source)
        blocks.removeAt(index(id)); touched()
        focus(prev.id, at)
        return true
    }

    fun turnInto(id: String, type: String) {
        val b = block(id) ?: return
        val src = if (b.source == "/") "" else b.source
        val nb = when (type) {
            "table" -> b.copy(type = type, source = "", rows = if (b.rows.isEmpty()) List(3) { List(3) { "" } } else b.rows)
            "code" -> b.copy(type = type, source = NoteText.plain(NoteText.sourceToHtml(src)))
            "divider" -> b.copy(type = type, source = "")
            else -> b.copy(type = type, source = if (b.type == "code") src else src)
        }
        set(nb)
        if (nb.isText) focus(id) else focused = null
    }

    fun addBelow(id: String?, type: String) {
        val at = id?.let { index(it) + 1 } ?: blocks.size
        val nb = EditBlock(EditBlock.newId(), type, rows = if (type == "table") List(3) { List(3) { "" } } else emptyList())
        blocks.add(at.coerceIn(0, blocks.size), nb); touched()
        if (type == "divider") { val p = EditBlock(EditBlock.newId(), "p"); blocks.add(at + 1, p); focus(p.id) }
        else if (nb.isText) focus(nb.id) else focused = null
    }

    fun move(id: String, by: Int) {
        val i = index(id); val j = i + by
        if (i < 0 || j !in blocks.indices) return
        val b = blocks.removeAt(i); blocks.add(j, b); touched()
    }

    fun duplicate(id: String) { val i = index(id); if (i < 0) return; blocks.add(i + 1, blocks[i].copy(id = EditBlock.newId())); touched() }

    fun delete(id: String) {
        val i = index(id); if (i < 0) return
        blocks.removeAt(i)
        if (blocks.isEmpty()) blocks.add(EditBlock(EditBlock.newId(), "p"))
        touched()
        focused = null
    }

    /** Wrap the selection of the caret's block in a mark (`**`, `*`, `__`, `~~`, `` ` ``). */
    fun format(open: String, close: String = open) {
        val id = focused ?: return
        val v = value
        val s = v.selection.min; val e = v.selection.max
        val t = v.text.substring(0, s) + open + v.text.substring(s, e) + close + v.text.substring(e)
        val nv = TextFieldValue(t, if (s == e) TextRange(s + open.length) else TextRange(s, e + open.length + close.length))
        value = nv
        block(id)?.let { set(it.copy(source = t)) }
    }

    fun link(url: String) {
        val v = value
        if (v.selection.collapsed) format("[", "]($url)") else format("[", "]($url)")
    }

    fun toBlocks(): List<NoteBlock> = blocks.map { it.toBlock() }

    /** Where a block's text is drawn: its layout and place, and whether it shows the source as is (or rendered, marks hidden). */
    class DropSpot(var layout: TextLayoutResult? = null, var coords: LayoutCoordinates? = null, var raw: Boolean = true)
    /** Each block's area in the window and its text's [DropSpot], for something dropped from another pane. */
    val boxes = HashMap<String, Rect>()
    val spots = HashMap<String, DropSpot>()
    fun spot(id: String) = spots.getOrPut(id) { DropSpot() }
    fun blockAt(at: Offset): EditBlock? = blocks.firstOrNull { boxes[it.id]?.contains(at) == true }

    /**
     * Something dropped from another pane (a path, a task's reference) at
     * [at], as the web's drop on a notes page: into the text under the finger
     * at that point, padded with spaces like typed words (a code block takes
     * it as it is). Over a divider, a table or below the blocks it becomes a
     * paragraph of its own.
     */
    fun dropText(at: Offset, text: String) {
        if (deleted || text.isEmpty()) return
        val b = blockAt(at)
        if (b == null || !(b.isText || b.type == "code")) {
            val i = b?.let { index(it.id) + 1 } ?: blocks.size
            val nb = EditBlock(EditBlock.newId(), "p", source = "$text ")
            blocks.add(i.coerceIn(0, blocks.size), nb); touched(); focus(nb.id)
            return
        }
        val src = if (focused == b.id) value.text else b.source
        var pos = src.length
        val sp = spots[b.id]; val lay = sp?.layout; val c = sp?.coords
        if (lay != null && c != null && c.isAttached) {
            val o = lay.getOffsetForPosition(c.windowToLocal(at))
            pos = if (sp.raw) o.coerceIn(0, src.length) else sourceOffset(src, lay.layoutInput.text.text, o)
        }
        val ins = if (b.type == "code") text else (if (pos > 0 && !src[pos - 1].isWhitespace()) " " else "") + text + " "
        set(b.copy(source = src.substring(0, pos) + ins + src.substring(pos)))
        if (b.isText) focus(b.id, pos + ins.length)
    }

    // A point in rendered text (bold and link marks hidden) back to the same
    // point in its source: walk both, skipping source characters not shown.
    private fun sourceOffset(src: String, shown: String, o: Int): Int {
        if (o >= shown.length) return src.length
        var j = 0
        for (i in 0 until o) {
            while (j < src.length && src[j] != shown[i]) j++
            if (j < src.length) j++
        }
        return j.coerceIn(0, src.length)
    }
}

@Composable
fun NotePageRoute(id: String, pane: NotePaneHooks? = null) {
    val project = currentProject()
    if (project == null) { WithProject(Area.Notes) {}; return }
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    var load by remember(id) { mutableStateOf<Load<NoteEditor>>(Load.Loading) }
    var toast by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    fun fetch() = scope.launch {
        load = try { Load.Ready(NoteEditor(api.notes.page(project, id))) } catch (e: ApiException) { Load.Failed(if (e.notFound) "This page could not be opened. It may have been deleted." else errorText(e)) }
    }
    LaunchedEffect(id) { fetch() }
    val editor = (load as? Load.Ready)?.value

    suspend fun saveNow(ed: NoteEditor) {
        val mark = ed.changes
        ed.status = SaveStatus.Saving
        try {
            val r = withContext(NonCancellable) { api.notes.save(project, ed.id, ed.title, ed.toBlocks(), ed.version) }
            val c = r.conflict
            if (c != null) {
                val mine = ed.toBlocks(); val myTitle = ed.title.ifBlank { "Untitled" } + " (your copy)"
                withContext(NonCancellable) { api.notes.create(project, myTitle, mine) }
                ed.load(c)
                toast = "This page was changed elsewhere. Your version was saved as “$myTitle”."
            } else {
                ed.version = r.version; ed.updatedAt = r.updatedAt.ifEmpty { ed.updatedAt }
                ed.saved = mark
                ed.status = if (ed.dirty) SaveStatus.Editing else SaveStatus.Saved
                ed.error = null
            }
        } catch (e: ApiException) {
            ed.status = if (e.offline) SaveStatus.Offline else SaveStatus.Failed
            ed.error = errorText(e)
            if (!e.offline) throw e
        }
    }

    if (editor != null) {
        // Autosave: 700 ms after the last change; offline, try again every 4 s.
        LaunchedEffect(editor) {
            snapshotFlow { editor.changes }.collectLatest {
                if (!editor.dirty || editor.deleted) return@collectLatest
                delay(700)
                while (editor.dirty) {
                    try { saveNow(editor) } catch (_: ApiException) { break }
                    if (editor.status == SaveStatus.Offline) delay(4000)
                }
            }
        }
        // Someone else saved this page: take theirs when nothing here is unsaved.
        LaunchedEffect(editor) {
            api.events.changes("notes").collect { ev ->
                if (ev.id != editor.id || ev.data.str("by") == api.notes.clientId) return@collect
                val v = ev.data["version"]?.str()?.toDoubleOrNull()?.toInt() ?: Int.MAX_VALUE
                if (!editor.dirty && editor.status != SaveStatus.Saving && v > editor.version) {
                    runCatching { api.notes.page(project, editor.id) }.getOrNull()?.let { editor.load(it) }
                }
            }
        }
        // In a pane, the page's title is the pane's title.
        if (pane != null) LaunchedEffect(editor) {
            snapshotFlow { editor.title }.collectLatest { delay(400); pane.onTitle(it) }
        }
        // Leaving with unsaved edits: save them anyway.
        DisposableEffect(editor) {
            onDispose { if (editor.dirty && !editor.deleted) PlanningScope.launch { runCatching { saveNow(editor) } } }
        }
    }

    Box(Modifier.fillMaxSize()) {
        NotePageScreen(
            load, onBack = { pane?.onPages?.invoke() ?: nav.back() }, onRetry = { fetch() },
            onTrash = {
                scope.launch {
                    try { api.notes.delete(project, id); pane?.onPages?.invoke() ?: nav.back() } catch (e: ApiException) { toast = errorText(e) }
                }
            },
        )
        toast?.let { m ->
            Box(Modifier.align(Alignment.BottomCenter).padding(12.dp)) {
                Row(
                    Modifier.fillMaxWidth().background(CrundiColors.BgSecondary, Dimens.Shape).border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape).padding(start = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(m, style = CrundiType.Small.copy(color = CrundiColors.TextPrimary), modifier = Modifier.weight(1f).padding(vertical = 10.dp))
                    CrundiIconButton("x", { toast = null }, contentDescription = "Close", size = 14.dp)
                }
            }
        }
    }
}

@Composable
fun NotePageScreen(load: Load<NoteEditor>, onBack: () -> Unit = {}, onRetry: () -> Unit = {}, onTrash: () -> Unit = {}) {
    var menu by remember { mutableStateOf(false) }
    val ed = load.valueOrNull
    Column(Modifier.fillMaxSize().imePadding()) {
        val tools: @Composable RowScope.() -> Unit = {
            if (ed != null) SaveIndicator(ed.status)
            if (ed != null && !ed.deleted) CrundiIconButton("more", { menu = true }, contentDescription = "Page actions")
        }
        if (inPane()) PaneSubBar("Pages", onBack, tools)
        else TopBar(ed?.title?.ifBlank { "Untitled" } ?: "Notes", onBack = onBack, actions = tools)
        when (load) {
            is Load.Loading -> LoadingState(text = "Loading…")
            is Load.Failed -> ErrorState(load.message, onRetry = onRetry)
            is Load.Ready -> NoteEditorBody(load.value, Modifier.weight(1f))
        }
    }
    if (menu && ed != null) PlanningSheet(ed.title.ifBlank { "Untitled" }, { menu = false }, subtitle = "Last edited " + ago(ed.updatedAt)) {
        MenuRow("Move to trash", "trash", { menu = false; onTrash() }, danger = true)
    }
}

@Composable
private fun SaveIndicator(s: SaveStatus) {
    val c = when (s) {
        SaveStatus.Saved -> CrundiColors.Green
        SaveStatus.Failed -> CrundiColors.Red
        SaveStatus.Trash -> CrundiColors.TextMuted
        else -> CrundiColors.Yellow
    }
    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(horizontal = 6.dp)) {
        Box(Modifier.size(6.dp).clip(CircleShape).background(c))
        Text(s.label, style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontSize = 11.5.sp), modifier = Modifier.padding(start = 5.dp))
    }
}

@Composable
fun NoteEditorBody(ed: NoteEditor, modifier: Modifier = Modifier) {
    val readOnly = ed.deleted
    // A path or a reference dragged here from another pane goes in where it is let go.
    val dropKey = "notes:" + (mv.obitech.crundi.ui.workbench.LocalPane.current?.key ?: ed.id)
    LaunchedEffect(ed, dropKey) {
        PaneDrop.inserts.collect { ins -> if (ins.key == dropKey) ed.dropText(ins.at, ins.text) }
    }
    val dropHere = !readOnly && PaneDrop.hovered(dropKey)
    val dropOver = if (dropHere) ed.blockAt(PaneDrop.finger)?.id else null
    Column(
        modifier.fillMaxWidth()
            .then(if (readOnly) Modifier else Modifier.paneDropTarget(dropKey, DropKind.Notes))
            .then(if (dropHere) Modifier.border(2.dp, CrundiColors.Accent) else Modifier),
    ) {
        if (readOnly) Banner("This page is in the trash. Restore it from the Notes tab to edit it.", Modifier.fillMaxWidth().padding(12.dp), kind = BadgeKind.Warn, icon = "trash")
        Column(
            Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState()).padding(start = 18.dp, end = 16.dp, top = 16.dp, bottom = 120.dp),
        ) {
            BasicTextField(
                ed.title, ed::rename, readOnly = readOnly,
                textStyle = TextStyle(fontSize = 26.sp, fontWeight = FontWeight.Bold, lineHeight = 32.sp, color = CrundiColors.TextPrimary),
                cursorBrush = SolidColor(CrundiColors.AccentHover),
                modifier = Modifier.fillMaxWidth().padding(bottom = 14.dp),
                decorationBox = { inner ->
                    Box { if (ed.title.isEmpty()) Text("Untitled", style = TextStyle(fontSize = 26.sp, fontWeight = FontWeight.Bold, color = CrundiColors.TextMuted)); inner() }
                },
            )
            var number = 0
            for (b in ed.blocks) {
                number = if (b.type == "number") number + 1 else 0
                NoteBlockView(ed, b, number, readOnly, dropOver = b.id == dropOver)
            }
            if (!readOnly) {
                Row(
                    Modifier.fillMaxWidth().padding(top = 8.dp).clip(Dimens.ShapeSm).clickable { ed.addBelow(null, "p") }.padding(vertical = 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    CrundiIcon("plus", size = 14.dp, tint = CrundiColors.TextMuted)
                    Text("Add a block", style = CrundiType.Small.copy(color = CrundiColors.TextMuted), modifier = Modifier.padding(start = 6.dp))
                }
            }
        }
        if (ed.focused != null && !readOnly) EditorBar(ed)
    }
    if (ed.menu != null && ed.focused != null) {
        val id = ed.focused!!
        val add = ed.menu == "add" || (ed.block(id)?.source?.let { it.isNotEmpty() && it != "/" } == true)
        PlanningSheet(if (add) "Add a block below" else "Turn this into, or add", { ed.menu = null }) {
            for (k in NOTE_BLOCK_KINDS) {
                Row(
                    Modifier.fillMaxWidth().clip(Dimens.ShapeSm).clickable {
                        ed.menu = null
                        if (add) ed.addBelow(id, k[0]) else ed.turnInto(id, k[0])
                    }.padding(vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Box(Modifier.size(28.dp).border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm).background(CrundiColors.BgTertiary, Dimens.ShapeSm), contentAlignment = Alignment.Center) {
                        Text(k[2], style = CrundiType.Meta.copy(color = CrundiColors.TextPrimary, fontWeight = FontWeight.SemiBold))
                    }
                    Text(k[1], style = CrundiType.Body, modifier = Modifier.weight(1f))
                    if (k[3].isNotEmpty()) Text(k[3], style = CrundiType.Meta)
                }
            }
        }
    }
    if (ed.linkAsk) {
        Dialog({ ed.linkAsk = false }) {
            TextAskCard("Link", "Address", "https://", "Apply", onDismiss = { ed.linkAsk = false }, placeholder = "Paste or type a link") { ed.link(it); ed.linkAsk = false }
        }
    }
}

/** The row over the keyboard while a block has the caret: block kind, marks, move, delete. */
@Composable
private fun EditorBar(ed: NoteEditor) {
    val id = ed.focused ?: return
    Column(Modifier.fillMaxWidth().background(CrundiColors.BgSecondary)) {
        CrundiDivider(strong = true)
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            CrundiIconButton("plus", { ed.menu = "add" }, contentDescription = "Add a block below")
            CrundiIconButton("layers", { ed.menu = "turn" }, contentDescription = "Turn into")
            BarText("B", FontWeight.Bold) { ed.format("**") }
            BarText("I", italic = true) { ed.format("*") }
            BarText("U", underline = true) { ed.format("__") }
            BarText("S", strike = true) { ed.format("~~") }
            BarText("</>", mono = true) { ed.format("`") }
            CrundiIconButton("link", { ed.linkAsk = true }, contentDescription = "Link")
            CrundiIconButton("chevron-up", { ed.move(id, -1) }, contentDescription = "Move up")
            CrundiIconButton("chevron-down", { ed.move(id, 1) }, contentDescription = "Move down")
            CrundiIconButton("copy", { ed.duplicate(id) }, contentDescription = "Duplicate")
            CrundiIconButton("trash", { ed.delete(id) }, contentDescription = "Delete", tint = CrundiColors.Red)
            CrundiIconButton("check", { ed.focused = null }, contentDescription = "Done", tint = CrundiColors.AccentHover)
        }
    }
}

@Composable
private fun BarText(t: String, weight: FontWeight? = null, italic: Boolean = false, underline: Boolean = false, strike: Boolean = false, mono: Boolean = false, onClick: () -> Unit) {
    Box(Modifier.size(Dimens.Touch).clip(Dimens.ShapeSm).clickable(onClick = onClick), contentAlignment = Alignment.Center) {
        Text(
            t,
            style = CrundiType.Body.copy(
                fontWeight = weight ?: FontWeight.Medium,
                fontStyle = if (italic) androidx.compose.ui.text.font.FontStyle.Italic else null,
                textDecoration = when { underline -> TextDecoration.Underline; strike -> TextDecoration.LineThrough; else -> null },
                fontFamily = if (mono) CrundiType.Mono else CrundiType.Sans,
                fontSize = if (mono) 12.sp else 15.sp,
                color = CrundiColors.TextSecondary,
            ),
        )
    }
}

private fun styleFor(type: String): TextStyle = when (type) {
    "h1" -> TextStyle(fontSize = 22.sp, fontWeight = FontWeight.Bold, lineHeight = 29.sp, color = CrundiColors.TextPrimary)
    "h2" -> TextStyle(fontSize = 18.5.sp, fontWeight = FontWeight.SemiBold, lineHeight = 25.sp, color = CrundiColors.TextPrimary)
    "h3" -> TextStyle(fontSize = 16.sp, fontWeight = FontWeight.SemiBold, lineHeight = 22.sp, color = CrundiColors.TextPrimary)
    "quote" -> TextStyle(fontSize = 15.sp, lineHeight = 24.sp, color = CrundiColors.TextSecondary)
    else -> TextStyle(fontSize = 15.sp, lineHeight = 24.sp, color = CrundiColors.TextPrimary)
}

private fun placeholderFor(type: String) = when (type) {
    "h1" -> "Heading 1"; "h2" -> "Heading 2"; "h3" -> "Heading 3"; "todo" -> "To-do"; "bullet", "number" -> "List"; "quote" -> "Quote"
    else -> "Type '/' for blocks"
}

@Composable
private fun NoteBlockView(ed: NoteEditor, b: EditBlock, number: Int, readOnly: Boolean, dropOver: Boolean = false) {
    val top = when (b.type) { "h1" -> 14.dp; "h2" -> 10.dp; "h3" -> 6.dp; else -> 1.dp }
    Row(Modifier.fillMaxWidth().padding(top = top, bottom = 1.dp)
        .onGloballyPositioned { ed.boxes[b.id] = it.boundsInWindow() }
        .then(if (dropOver) Modifier.background(CrundiColors.AccentDim, Dimens.ShapeSm) else Modifier).then(if (b.type == "quote") Modifier.height(IntrinsicSize.Min) else Modifier), verticalAlignment = Alignment.Top) {
        when (b.type) {
            "bullet" -> Box(Modifier.width(22.dp).padding(top = 9.dp), contentAlignment = Alignment.TopCenter) { Box(Modifier.size(6.dp).clip(CircleShape).background(CrundiColors.TextSecondary)) }
            "number" -> Text("$number.", style = styleFor("p").copy(color = CrundiColors.TextSecondary, textAlign = TextAlign.End), modifier = Modifier.width(22.dp).padding(end = 4.dp))
            "todo" -> Box(Modifier.padding(top = 4.dp, end = 8.dp)) { PlanningCheckbox(b.checked, { if (!readOnly) ed.set(b.copy(checked = !b.checked)) }, size = 17.dp) }
            "quote" -> Box(Modifier.padding(end = 12.dp).width(3.dp).fillMaxHeight().background(CrundiColors.Border))
        }
        Box(Modifier.weight(1f)) {
            when {
                b.isText -> TextBlock(ed, b, readOnly)
                b.type == "code" -> CodeNoteBlock(ed, b, readOnly)
                b.type == "table" -> TableNoteBlock(ed, b, readOnly)
                b.type == "divider" -> Box(
                    Modifier.fillMaxWidth().clickable(enabled = !readOnly) { ed.focused = null }.padding(vertical = 14.dp).height(1.dp).background(CrundiColors.Border),
                )
            }
        }
    }
}

@Composable
private fun TextBlock(ed: NoteEditor, b: EditBlock, readOnly: Boolean) {
    var style = styleFor(b.type)
    if (b.type == "todo" && b.checked) style = style.copy(color = CrundiColors.TextMuted, textDecoration = TextDecoration.LineThrough)
    if (ed.focused == b.id && !readOnly) {
        val fr = remember { FocusRequester() }
        BasicTextField(
            ed.value, { ed.edit(b.id, it) },
            textStyle = style, cursorBrush = SolidColor(CrundiColors.AccentHover),
            onTextLayout = { ed.spot(b.id).apply { layout = it; raw = true } },
            modifier = Modifier.fillMaxWidth().onGloballyPositioned { ed.spot(b.id).coords = it }.focusRequester(fr).onPreviewKeyEvent { e ->
                e.type == KeyEventType.KeyDown && e.key == Key.Backspace && ed.value.selection.collapsed && ed.value.selection.start == 0 && ed.backspaceAtStart(b.id)
            },
            decorationBox = { inner -> Box { if (ed.value.text.isEmpty()) Text(placeholderFor(b.type), style = style.copy(color = CrundiColors.TextMuted, textDecoration = null)); inner() } },
        )
        LaunchedEffect(b.id) { fr.requestFocus() }
    } else {
        val text = remember(b.source) { NoteText.annotated(NoteText.parseSource(b.source)) }
        Text(
            if (text.isEmpty()) AnnotatedString(" ") else text,
            style = style,
            onTextLayout = { ed.spot(b.id).apply { layout = it; raw = false } },
            modifier = Modifier.fillMaxWidth().onGloballyPositioned { ed.spot(b.id).coords = it }.clickable(enabled = !readOnly, indication = null, interactionSource = remember { androidx.compose.foundation.interaction.MutableInteractionSource() }) { ed.focus(b.id) },
        )
    }
}

private val CODE_LANGS = listOf("" to "Plain text", "bash" to "Bash", "js" to "JavaScript", "ts" to "TypeScript", "json" to "JSON", "kotlin" to "Kotlin", "python" to "Python", "html" to "HTML", "css" to "CSS", "sql" to "SQL", "yaml" to "YAML", "go" to "Go", "rust" to "Rust", "java" to "Java", "diff" to "Diff", "markdown" to "Markdown")

@Composable
private fun CodeNoteBlock(ed: NoteEditor, b: EditBlock, readOnly: Boolean) {
    val clipboard = LocalClipboardManager.current
    var langMenu by remember { mutableStateOf(false) }
    var editing by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(8.dp)
    Column(Modifier.fillMaxWidth().padding(vertical = 4.dp).clip(shape).background(CrundiColors.BgSecondary).border(Dimens.Hairline, CrundiColors.BorderSubtle, shape)) {
        Row(Modifier.fillMaxWidth().padding(start = 10.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(
                CODE_LANGS.firstOrNull { it.first == b.lang }?.second ?: b.lang.ifEmpty { "Plain text" },
                style = CrundiType.Meta.copy(color = CrundiColors.TextSecondary),
                modifier = Modifier.clip(Dimens.ShapeSm).clickable(enabled = !readOnly) { langMenu = true }.padding(vertical = 6.dp, horizontal = 4.dp),
            )
            Spacer(Modifier.weight(1f))
            Text(
                if (b.wrap) "Wrapping" else "Wrap",
                style = CrundiType.Meta.copy(color = if (b.wrap) CrundiColors.AccentHover else CrundiColors.TextMuted),
                modifier = Modifier.clip(Dimens.ShapeSm).clickable(enabled = !readOnly) { ed.set(b.copy(wrap = !b.wrap)) }.padding(6.dp),
            )
            CrundiIconButton("copy", { clipboard.setText(AnnotatedString(b.source)) }, contentDescription = "Copy code", size = 13.dp, touch = 32.dp, tint = CrundiColors.TextMuted)
            if (!readOnly) CrundiIconButton("trash", { ed.delete(b.id) }, contentDescription = "Delete block", size = 13.dp, touch = 32.dp, tint = CrundiColors.TextMuted)
        }
        val style = CrundiType.Code.copy(fontSize = 12.5.sp, lineHeight = 19.sp)
        val scroll = if (b.wrap) Modifier else Modifier.horizontalScroll(rememberScrollState())
        Box(Modifier.fillMaxWidth().then(scroll).padding(start = 14.dp, end = 14.dp, bottom = 12.dp)) {
            if (editing && !readOnly) {
                val fr = remember { FocusRequester() }
                BasicTextField(
                    b.source, { ed.set(b.copy(source = it.replace("\t", "  "))) }, textStyle = style, cursorBrush = SolidColor(CrundiColors.AccentHover),
                    onTextLayout = { ed.spot(b.id).apply { layout = it; raw = true } },
                    modifier = Modifier.onGloballyPositioned { ed.spot(b.id).coords = it }.focusRequester(fr).then(if (b.wrap) Modifier.fillMaxWidth() else Modifier.widthIn(min = 200.dp)),
                    visualTransformation = { t -> androidx.compose.ui.text.input.TransformedText(Highlighter.highlight(t.text, b.lang), androidx.compose.ui.text.input.OffsetMapping.Identity) },
                )
                LaunchedEffect(Unit) { fr.requestFocus() }
            } else {
                Text(
                    remember(b.source, b.lang) { Highlighter.highlight(b.source.ifEmpty { " " }, b.lang) }, style = style, softWrap = b.wrap,
                    onTextLayout = { ed.spot(b.id).apply { layout = it; raw = true } },
                    modifier = Modifier.onGloballyPositioned { ed.spot(b.id).coords = it }.clickable(enabled = !readOnly, indication = null, interactionSource = remember { androidx.compose.foundation.interaction.MutableInteractionSource() }) { ed.focused = null; editing = true },
                )
            }
        }
    }
    LaunchedEffect(ed.focused) { if (ed.focused != null) editing = false }
    if (langMenu) PlanningSheet("Language", { langMenu = false }) {
        for ((k, name) in CODE_LANGS) MenuRow(name, if (k == b.lang) "check" else "file", { ed.set(b.copy(lang = k)); langMenu = false })
    }
}

@Composable
private fun TableNoteBlock(ed: NoteEditor, b: EditBlock, readOnly: Boolean) {
    var active by remember { mutableStateOf(false) }
    var editingCell by remember { mutableStateOf<Pair<Int, Int>?>(null) }
    val cols = b.rows.maxOfOrNull { it.size } ?: 0
    Column(Modifier.fillMaxWidth().padding(vertical = 4.dp)) {
        Column(Modifier.horizontalScroll(rememberScrollState()).border(Dimens.Hairline, CrundiColors.Border)) {
            b.rows.forEachIndexed { r, row ->
                Row(Modifier.height(IntrinsicSize.Min)) {
                    for (c in 0 until cols) {
                        val head = b.header && r == 0
                        Box(
                            Modifier.width(120.dp).fillMaxHeight().background(if (head) CrundiColors.BgSecondary else CrundiColors.BgPrimary)
                                .border(Dimens.Hairline, CrundiColors.Border).padding(horizontal = 9.dp, vertical = 6.dp),
                        ) {
                            val st = CrundiType.Body.copy(fontSize = 14.sp, fontWeight = if (head) FontWeight.SemiBold else null)
                            val src = row.getOrElse(c) { "" }
                            // A cell reads as formatted text (bold, ~~struck~~, links) like
                            // a paragraph does, and shows its marks only while it is edited.
                            if (readOnly || editingCell != r to c) Text(
                                NoteText.annotated(NoteText.parseSource(src)), style = st,
                                modifier = Modifier.fillMaxWidth().heightIn(min = 20.dp)
                                    .then(if (readOnly) Modifier else Modifier.clickable { editingCell = r to c }),
                            )
                            else {
                                val focus = remember { FocusRequester() }
                                var had by remember { mutableStateOf(false) }
                                BasicTextField(
                                    src,
                                    { v -> active = true; ed.set(b.copy(rows = b.rows.mapIndexed { ri, rr -> if (ri != r) rr else List(cols) { ci -> if (ci == c) v else rr.getOrElse(ci) { "" } } })) },
                                    textStyle = st, cursorBrush = SolidColor(CrundiColors.AccentHover),
                                    modifier = Modifier.fillMaxWidth().focusRequester(focus).onFocusChanged {
                                        if (it.isFocused) had = true else if (had && editingCell == r to c) editingCell = null
                                    },
                                )
                                LaunchedEffect(Unit) { focus.requestFocus() }
                            }
                        }
                    }
                }
            }
        }
        if (!readOnly) {
            Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                TableBtn("+ Row") { ed.set(b.copy(rows = b.rows + listOf(List(cols) { "" }))) }
                TableBtn("+ Column") { if (cols < 20) ed.set(b.copy(rows = b.rows.map { it + "" })) }
                TableBtn("− Row", b.rows.size > 1) { ed.set(b.copy(rows = b.rows.dropLast(1))) }
                TableBtn("− Column", cols > 1) { ed.set(b.copy(rows = b.rows.map { it.dropLast(1) })) }
                TableBtn("Header row", selected = b.header) { ed.set(b.copy(header = !b.header)) }
                TableBtn("Delete table") { ed.delete(b.id) }
            }
        }
    }
}

@Composable
private fun TableBtn(t: String, enabled: Boolean = true, selected: Boolean = false, onClick: () -> Unit) {
    Text(
        t,
        style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontSize = 11.5.sp, color = if (selected) CrundiColors.AccentHover else if (enabled) CrundiColors.TextSecondary else CrundiColors.TextMuted),
        modifier = Modifier.clip(Dimens.ShapeSm).background(if (selected) CrundiColors.AccentDim else CrundiColors.BgTertiary)
            .border(Dimens.Hairline, if (selected) CrundiColors.Accent else CrundiColors.Border, Dimens.ShapeSm)
            .clickable(enabled = enabled, onClick = onClick).padding(horizontal = 9.dp, vertical = 6.dp),
    )
}
