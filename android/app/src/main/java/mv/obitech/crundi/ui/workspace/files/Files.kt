package mv.obitech.crundi.ui.workspace.files

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import mv.obitech.crundi.Downloads
import mv.obitech.crundi.api.model.FileSearch
import mv.obitech.crundi.api.model.Folder
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.Banner
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiDivider
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.ListRow
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens
import mv.obitech.crundi.ui.workspace.WorkspaceRoutes
import mv.obitech.crundi.ui.components.errorText
import mv.obitech.crundi.ui.workspace.formatSize
import mv.obitech.crundi.ui.workspace.rememberFilePicker
import mv.obitech.crundi.ui.workspace.rememberToast

/** One thing in the list: a folder's entry or a search hit. [path] is absolute. */
data class FileItem(val name: String, val path: String, val isDir: Boolean, val size: Long = 0, val detail: String? = null)

/** Cut or copied, waiting to be pasted. */
data class FileClip(val paths: List<String>, val copy: Boolean) {
    val names get() = paths.map { it.substringAfterLast('/') }
}

/** A transfer in progress (an upload or a download), for the strip under the toolbar. */
data class Transfer(val label: String, val done: Long, val total: Long)

data class FilesState(
    val project: String? = null,
    val folder: Folder? = null,
    val loading: Boolean = false,
    val error: String? = null,
    val query: String = "",
    val search: FileSearch? = null,
    val searching: Boolean = false,
    val clip: FileClip? = null,
    val transfer: Transfer? = null,
    val notice: String? = null,
) {
    val items: List<FileItem>
        get() = if (query.isNotBlank()) search?.entries.orEmpty().map { FileItem(it.name, it.path, it.isDir, detail = it.rel.substringBeforeLast('/', "").ifEmpty { null }) }
        else folder?.entries.orEmpty().map { FileItem(it.name, it.path, it.isDir, it.size) }
}

/** What the row menu can do. */
sealed interface FileAction {
    data class Open(val item: FileItem) : FileAction
    data class Cut(val item: FileItem) : FileAction
    data class Copy(val item: FileItem) : FileAction
    /** Paste the clip into [dir] (a folder row, or the current folder). */
    data class Paste(val dir: String) : FileAction
    data class CopyPath(val item: FileItem) : FileAction
    data class Download(val item: FileItem) : FileAction
    data class Delete(val item: FileItem) : FileAction
}

/** The Files tab: search, the path, the folder, and a menu on each row. */
@Composable
fun FilesScreen(
    state: FilesState,
    modifier: Modifier = Modifier,
    onQuery: (String) -> Unit = {},
    onFolder: (String?) -> Unit = {},
    onUp: () -> Unit = {},
    onUpload: () -> Unit = {},
    onAction: (FileAction) -> Unit = {},
    onRetry: () -> Unit = {},
    menuFor: FileItem? = null,
) {
    var menu by remember { mutableStateOf(menuFor) }
    var deleting by remember { mutableStateOf<FileItem?>(null) }
    Column(modifier.fillMaxSize().background(CrundiColors.BgPrimary)) {
        if (state.project == null) {
            EmptyState("folder", "Pick a project", body = "Files belong to a project. Choose one from the menu to browse its folder.")
            return@Column
        }
        Box(Modifier.fillMaxWidth().padding(start = 12.dp, end = 12.dp, top = 10.dp, bottom = 6.dp)) {
            CrundiTextField(state.query, onQuery, placeholder = "Search files & folders…", icon = "search", mono = true, modifier = Modifier.fillMaxWidth())
        }
        if (state.query.isBlank()) Breadcrumb(state, onFolder, onUp, onUpload, onPaste = { state.folder?.path?.let { onAction(FileAction.Paste(it)) } })
        else Text(
            when {
                state.searching && state.search == null -> "Searching…"
                else -> { val n = state.search?.entries?.size ?: 0; "$n${if (state.search?.truncated == true) "+" else ""} match${if (n == 1) "" else "es"} · gitignored files excluded" }
            },
            style = CrundiType.Meta, modifier = Modifier.padding(horizontal = 16.dp, vertical = 6.dp),
        )
        CrundiDivider()
        state.transfer?.let { t ->
            Column(Modifier.fillMaxWidth().background(CrundiColors.BgSecondary).padding(horizontal = 16.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(t.label + if (t.total > 0) " · ${formatSize(t.done).ifEmpty { "0 B" }} of ${formatSize(t.total)}" else "", style = CrundiType.Small, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (t.total > 0) LinearProgressIndicator(progress = { (t.done.toFloat() / t.total).coerceIn(0f, 1f) }, color = CrundiColors.Accent, trackColor = CrundiColors.BgTertiary, modifier = Modifier.fillMaxWidth().height(3.dp))
                else LinearProgressIndicator(color = CrundiColors.Accent, trackColor = CrundiColors.BgTertiary, modifier = Modifier.fillMaxWidth().height(3.dp))
            }
        }
        state.notice?.let { Banner(it, Modifier.fillMaxWidth().padding(12.dp), kind = BadgeKind.Warn, icon = "info") }
        when {
            state.error != null -> ErrorState(state.error, onRetry = onRetry)
            state.loading && state.folder == null -> LoadingState(text = "Loading files")
            state.query.isNotBlank() && state.search != null && state.items.isEmpty() -> EmptyState("search", "No matches", body = "Gitignored files are not searched. Try part of the name.")
            state.query.isBlank() && state.items.isEmpty() && state.folder != null -> EmptyState("folder", "This folder is empty", body = "Upload a file to put it here.", action = "Upload", onAction = onUpload)
            else -> LazyColumn(Modifier.fillMaxSize()) {
                if (state.query.isBlank() && state.folder?.parent != null && state.folder.path != state.folder.root) item(key = "..") {
                    FileRow(FileItem("..", state.folder.parent, true), cut = false, onOpen = onUp, onMore = null)
                }
                items(state.items, key = { it.path }) { item ->
                    FileRow(item, cut = state.clip?.copy == false && item.path in state.clip.paths, onOpen = { onAction(FileAction.Open(item)) }, onMore = { menu = item })
                }
            }
        }
    }
    menu?.let { item ->
        FileMenu(item, state.clip, onDismiss = { menu = null }) { a ->
            menu = null
            if (a is FileAction.Delete) deleting = a.item else onAction(a)
        }
    }
    deleting?.let { item ->
        CrundiDialog(
            "Delete \"${item.name}\"?", onDismiss = { deleting = null },
            message = "This permanently removes it from disk and cannot be undone.",
            confirm = if (item.isDir) "Delete folder" else "Delete file", danger = true,
            onConfirm = { deleting = null; onAction(FileAction.Delete(item)) },
        )
    }
}

@Composable
private fun Breadcrumb(state: FilesState, onFolder: (String?) -> Unit, onUp: () -> Unit, onUpload: () -> Unit, onPaste: () -> Unit) {
    val f = state.folder
    Row(Modifier.fillMaxWidth().padding(start = 12.dp, end = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        Row(Modifier.weight(1f).horizontalScroll(rememberScrollState(Int.MAX_VALUE)), verticalAlignment = Alignment.CenterVertically) {
            val crumbs = f?.crumbs.orEmpty()
            if (crumbs.isEmpty()) Text("—", style = CrundiType.Code.copy(color = CrundiColors.TextMuted))
            crumbs.forEachIndexed { i, c ->
                if (i > 0) Text("/", style = CrundiType.Code.copy(color = CrundiColors.TextMuted), modifier = Modifier.padding(horizontal = 3.dp))
                val last = i == crumbs.lastIndex
                Text(
                    c.name.ifEmpty { "/" },
                    style = CrundiType.Code.copy(color = if (last) CrundiColors.TextPrimary else CrundiColors.Accent),
                    maxLines = 1,
                    modifier = Modifier.clickable(enabled = !last) { onFolder(c.path) }.padding(vertical = 10.dp),
                )
            }
        }
        if (state.clip != null) CrundiIconButton("clipboard", onPaste, contentDescription = "Paste here", tint = CrundiColors.AccentHover)
        CrundiIconButton("arrow-up", onUp, contentDescription = "Up to parent folder", enabled = f?.parent != null && f.path != f.root)
        CrundiIconButton("upload", onUpload, contentDescription = "Upload to this folder", enabled = state.transfer == null)
    }
}

@Composable
private fun FileRow(item: FileItem, cut: Boolean, onOpen: () -> Unit, onMore: (() -> Unit)?) {
    Row(
        Modifier.fillMaxWidth().alpha(if (cut) 0.45f else 1f).clickable(onClick = onOpen).padding(start = 12.dp, end = 0.dp).height(Dimens.Touch),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        if (item.name == "..") Box(Modifier.width(22.dp), contentAlignment = Alignment.Center) { mv.obitech.crundi.ui.icons.CrundiIcon("arrow-up", size = 16.dp, tint = CrundiColors.TextMuted) }
        else FileMark(item.name, item.isDir)
        Column(Modifier.weight(1f)) {
            Text(item.name, style = CrundiType.Code, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (item.detail != null) Text(item.detail, style = CrundiType.Meta, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        if (!item.isDir) Text(formatSize(item.size), style = CrundiType.Meta)
        if (onMore != null) CrundiIconButton("more", onMore, contentDescription = "More for ${item.name}", size = 16.dp, tint = CrundiColors.TextMuted)
        else Box(Modifier.width(8.dp))
    }
}

/** The row's menu, as the desktop's context menu. */
@Composable
fun FileMenu(item: FileItem, clip: FileClip?, onDismiss: () -> Unit, onAction: (FileAction) -> Unit) {
    CrundiDialog(item.name, onDismiss = onDismiss, dismiss = null) {
        Column {
            ListRow("Open", icon = if (item.isDir) "folder" else "file", onClick = { onAction(FileAction.Open(item)) })
            ListRow("Cut", icon = "scissors", onClick = { onAction(FileAction.Cut(item)) })
            ListRow("Copy", icon = "copy", onClick = { onAction(FileAction.Copy(item)) })
            if (clip != null && item.isDir) {
                val what = if (clip.paths.size == 1) "\"${clip.names[0]}\"" else "${clip.paths.size} items"
                ListRow("Paste $what inside \"${item.name}\"", icon = "clipboard", onClick = { onAction(FileAction.Paste(item.path)) })
            }
            ListRow("Copy path", icon = "copy", onClick = { onAction(FileAction.CopyPath(item)) })
            if (!item.isDir) ListRow("Download", icon = "download", onClick = { onAction(FileAction.Download(item)) })
            CrundiDivider(Modifier.padding(vertical = 4.dp))
            Row(Modifier.fillMaxWidth().clickable { onAction(FileAction.Delete(item)) }.padding(horizontal = Dimens.RowPadH, vertical = Dimens.RowPadV), horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
                mv.obitech.crundi.ui.icons.CrundiIcon("trash", size = Dimens.Icon, tint = CrundiColors.Red)
                Text("Delete", style = CrundiType.Body.copy(color = CrundiColors.Red))
            }
        }
    }
}

/** The Files tab with live data. */
@Composable
fun FilesRoute() {
    val services = LocalAppServices.current
    val api = services.api
    val nav = LocalNavigator.current
    val ctx = LocalContext.current
    val clipboard = LocalClipboardManager.current
    val toast = rememberToast()
    val scope = rememberCoroutineScope()
    val shell by services.shell.state.collectAsState()
    val project = shell.currentProject
    var dir by rememberSaveable(project) { mutableStateOf<String?>(null) }
    var state by remember(project) { mutableStateOf(FilesState(project = project, loading = true)) }
    var reload by remember { mutableIntStateOf(0) }

    LaunchedEffect(project, dir, reload) {
        if (project == null) return@LaunchedEffect
        state = state.copy(loading = true, error = null)
        runCatching { api.files.list(project, dir) }
            .onSuccess { f ->
                state = state.copy(folder = f, loading = false, notice = if (f.fellBack) "That folder no longer exists. Showing ${f.path.substringAfterLast('/')} instead." else null)
            }
            .onFailure { state = state.copy(loading = false, error = errorText(it)) }
    }
    LaunchedEffect(project, state.query) {
        val q = state.query.trim()
        if (project == null || q.isEmpty()) { state = state.copy(search = null, searching = false); return@LaunchedEffect }
        delay(220)
        state = state.copy(searching = true)
        runCatching { api.files.search(project, q) }
            .onSuccess { state = state.copy(search = it, searching = false) }
            .onFailure { state = state.copy(searching = false, search = FileSearch()); toast("Search failed: " + errorText(it)) }
    }
    val pick = rememberFilePicker { files ->
        val p = project ?: return@rememberFilePicker
        val target = state.folder?.path ?: return@rememberFilePicker
        scope.launch {
            for (f in files) {
                state = state.copy(transfer = Transfer("Uploading ${f.name}", 0, f.size))
                runCatching {
                    api.files.upload(p, target, f.name, f.size, progress = { d, t -> state = state.copy(transfer = Transfer("Uploading ${f.name}", d, t)) }, open = f.open)
                }.onSuccess { toast("Uploaded ${f.name}") }.onFailure { toast("Upload failed: " + errorText(it)) }
            }
            state = state.copy(transfer = null)
            reload++
        }
    }
    FilesScreen(
        state,
        onQuery = { state = state.copy(query = it) },
        onFolder = { dir = it },
        onUp = { state.folder?.parent?.let { dir = it } },
        onUpload = pick,
        onRetry = { reload++ },
        onAction = { a ->
            val p = project ?: return@FilesScreen
            when (a) {
                is FileAction.Open -> if (a.item.isDir) { state = state.copy(query = ""); dir = a.item.path } else nav.to(WorkspaceRoutes.file(a.item.path))
                is FileAction.Cut -> { state = state.copy(clip = FileClip(listOf(a.item.path), copy = false)); toast("Cut ${a.item.name}. Open a folder and paste.") }
                is FileAction.Copy -> { state = state.copy(clip = FileClip(listOf(a.item.path), copy = true)); toast("Copied ${a.item.name}. Open a folder and paste.") }
                is FileAction.CopyPath -> { clipboard.setText(AnnotatedString(a.item.path)); toast("Path copied") }
                is FileAction.Paste -> {
                    val clip = state.clip ?: return@FilesScreen
                    scope.launch {
                        runCatching { api.files.transfer(p, clip.paths, a.dir, copy = clip.copy) }
                            .onSuccess { r ->
                                val renamed = r.done.count { it.renamed }
                                toast(
                                    when {
                                        r.failed.isNotEmpty() -> "${r.failed.size} could not be ${if (clip.copy) "copied" else "moved"}: ${r.failed.first().error}"
                                        renamed > 0 -> "${if (clip.copy) "Copied" else "Moved"} ${r.done.size}, $renamed renamed to keep what was there"
                                        else -> "${if (clip.copy) "Copied" else "Moved"} ${r.done.size} item${if (r.done.size == 1) "" else "s"}"
                                    },
                                )
                                if (!clip.copy) state = state.copy(clip = null)
                                reload++
                            }
                            .onFailure { toast((if (clip.copy) "Copy" else "Move") + " failed: " + errorText(it)) }
                    }
                }
                is FileAction.Download -> scope.launch {
                    state = state.copy(transfer = Transfer("Downloading ${a.item.name}", 0, a.item.size))
                    runCatching {
                        Downloads.save(ctx, a.item.name) { sink -> api.files.download(p, a.item.path, sink) { d, t -> state = state.copy(transfer = Transfer("Downloading ${a.item.name}", d, t)) } }
                    }.onSuccess { toast("Saved ${it.name} to ${it.where}") }.onFailure { toast("Download failed: " + errorText(it)) }
                    state = state.copy(transfer = null)
                }
                is FileAction.Delete -> scope.launch {
                    runCatching { api.files.delete(p, a.item.path) }
                        .onSuccess { toast("Deleted ${a.item.name}"); reload++; if (state.query.isNotBlank()) state = state.copy(query = state.query + "") }
                        .onFailure { toast("Delete failed: " + errorText(it)) }
                }
            }
        },
    )
}
