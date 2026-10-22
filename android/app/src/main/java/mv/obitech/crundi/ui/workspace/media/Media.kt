package mv.obitech.crundi.ui.workspace.media

import android.graphics.Bitmap
import android.graphics.pdf.PdfRenderer
import android.os.ParcelFileDescriptor
import android.widget.MediaController
import android.widget.VideoView
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import mv.obitech.crundi.ui.workbench.paneDragSource
import androidx.compose.foundation.gestures.rememberTransformableState
import androidx.compose.foundation.gestures.transformable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import mv.obitech.crundi.Downloads
import mv.obitech.crundi.api.model.MediaItem
import mv.obitech.crundi.api.rememberServerImage
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiChip
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.TopBar
import mv.obitech.crundi.ui.icons.CrundiIcon
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
import java.io.File

/** The desktop's MEDIA_KIND_ICON. */
fun mediaIcon(kind: String) = when (kind) {
    "image" -> "image"; "video" -> "film"; "audio" -> "music"; "pdf" -> "file-text"; else -> "file"
}

enum class MediaKindFilter(val key: String?, val label: String, val icon: String?) {
    All(null, "All", null), Kanban("kanban", "Kanban", "kanban"), Mindmap("mindmap", "Mindmap", "mindmap"), Unlinked("unlinked", "Unlinked", null)
}

data class MediaState(
    val project: String? = null,
    val items: List<MediaItem>? = null,
    val error: String? = null,
    val query: String = "",
    val kind: MediaKindFilter = MediaKindFilter.All,
    /** True: this project (and unscoped); false: every project. */
    val thisProject: Boolean = true,
    val uploading: String? = null,
) {
    /** The desktop's mediaFilter: name, link label, project, extension or kind. */
    val shown: List<MediaItem>
        get() {
            val q = query.trim().lowercase()
            val all = items.orEmpty()
            if (q.isEmpty()) return all
            return all.filter { listOf(it.originalName, it.linkLabel.orEmpty(), it.projectName.orEmpty(), it.ext, it.kind).any { s -> s.lowercase().contains(q) } }
        }
}

/** The Media tab: upload, search, filters, and the grid of cards. */
@Composable
fun MediaScreen(
    state: MediaState,
    modifier: Modifier = Modifier,
    onQuery: (String) -> Unit = {},
    onKind: (MediaKindFilter) -> Unit = {},
    onScope: (Boolean) -> Unit = {},
    onUpload: () -> Unit = {},
    onOpen: (MediaItem) -> Unit = {},
    onDownload: (MediaItem) -> Unit = {},
    onCopyPath: (MediaItem) -> Unit = {},
    onDelete: (MediaItem) -> Unit = {},
    onRetry: () -> Unit = {},
) {
    var deleting by remember { mutableStateOf<MediaItem?>(null) }
    Column(modifier.fillMaxSize().background(CrundiColors.BgPrimary)) {
        Row(Modifier.fillMaxWidth().padding(start = 12.dp, end = 12.dp, top = 10.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            CrundiButton(if (state.uploading != null) "Uploading…" else "Upload", onUpload, kind = ButtonKind.Primary, icon = "upload", enabled = state.uploading == null)
            CrundiTextField(state.query, onQuery, placeholder = "Search media…", icon = "search", modifier = Modifier.weight(1f))
        }
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 12.dp, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            for (k in MediaKindFilter.entries) CrundiChip(k.label, state.kind == k, { onKind(k) }, icon = k.icon)
            Box(Modifier.padding(horizontal = 4.dp))
            if (state.project != null) CrundiChip("This project", state.thisProject, { onScope(true) })
            CrundiChip("All projects", !state.thisProject || state.project == null, { onScope(false) })
        }
        when {
            state.error != null -> ErrorState(state.error, onRetry = onRetry)
            state.items == null -> LoadingState(text = "Loading media")
            state.items.isEmpty() -> EmptyState("images", "No media here yet", body = "Upload files, or attach them to a Kanban task or subtask, or a Mindmap idea.", action = "Upload", onAction = onUpload)
            state.shown.isEmpty() -> EmptyState("search", "No media matches “${state.query.trim()}”", body = "Search looks at names, links, projects and file types.")
            else -> LazyVerticalGrid(
                GridCells.Adaptive(150.dp),
                modifier = Modifier.fillMaxSize(),
                contentPadding = androidx.compose.foundation.layout.PaddingValues(12.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                items(state.shown, key = { it.id }) { MediaCard(it, onOpen, onDownload, onCopyPath, onDelete = { m -> deleting = m }) }
            }
        }
    }
    deleting?.let { m ->
        CrundiDialog(
            "Delete this media file?", onDismiss = { deleting = null },
            message = "\"${m.originalName}\" is removed for good and cannot be brought back.",
            confirm = "Delete file", danger = true, onConfirm = { deleting = null; onDelete(m) },
        )
    }
}

@Composable
private fun MediaCard(m: MediaItem, onOpen: (MediaItem) -> Unit, onDownload: (MediaItem) -> Unit, onCopyPath: (MediaItem) -> Unit, onDelete: (MediaItem) -> Unit) {
    // The whole card is held and dragged onto a chat, terminal or notes page
    // for its path, as the web's media card.
    Column(Modifier.paneDragSource("media", m.path, m.originalName).clip(Dimens.Shape).background(CrundiColors.BgCard).border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape)) {
        Box(Modifier.fillMaxWidth().height(110.dp).background(CrundiColors.BgSecondary).clickable { onOpen(m) }, contentAlignment = Alignment.Center) {
            val img = if (m.kind == "image") rememberServerImage(m.rawPath, maxPx = 400) else null
            if (img != null) Image(img, contentDescription = m.originalName, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
            else Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
                CrundiIcon(mediaIcon(m.kind), size = 34.dp, tint = CrundiColors.TextMuted)
                Text(m.ext.removePrefix(".").ifEmpty { m.kind }.uppercase(), style = CrundiType.Meta.copy(letterSpacing = 0.06.sp))
            }
            Box(Modifier.align(Alignment.TopStart).padding(6.dp).background(CrundiColors.Scrim, Dimens.ShapeSm).padding(horizontal = 5.dp, vertical = 3.dp)) {
                CrundiIcon(mediaIcon(m.kind), size = 13.dp, tint = Color.White)
            }
        }
        Column(Modifier.padding(horizontal = 9.dp, vertical = 7.dp), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(m.originalName, style = CrundiType.Small.copy(color = CrundiColors.TextPrimary, fontWeight = FontWeight.SemiBold, fontSize = 12.sp, lineHeight = 15.sp), maxLines = 2, overflow = TextOverflow.Ellipsis)
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                m.projectName?.let { Text(it, style = CrundiType.Meta, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false)) }
                val word = when (m.link?.type) { "node" -> "Idea"; "todo" -> "Subtask"; "task" -> "Task"; else -> null }
                when {
                    word == null -> Text("Unlinked", style = CrundiType.Meta)
                    m.linkStatus == "deleted" -> Text("$word (deleted)", style = CrundiType.Meta.copy(textDecoration = TextDecoration.LineThrough))
                    else -> Text(m.linkLabel ?: word, style = CrundiType.Meta.copy(color = CrundiColors.Green), maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
        Row(Modifier.fillMaxWidth().padding(start = 9.dp, end = 9.dp, bottom = 9.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            if (m.kind != "other") CardAction("image", "View", Modifier.weight(1f)) { onOpen(m) } else CardAction("download", "Get", Modifier.weight(1f)) { onDownload(m) }
            if (m.path.isNotEmpty()) CardAction("copy", null) { onCopyPath(m) }
            CardAction("trash", null, danger = true) { onDelete(m) }
        }
    }
}

@Composable
private fun CardAction(icon: String, label: String?, modifier: Modifier = Modifier, danger: Boolean = false, onClick: () -> Unit) {
    Row(
        modifier.height(34.dp).clip(Dimens.ShapeSm).background(CrundiColors.BgTertiary).border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm).clickable(onClick = onClick).padding(horizontal = 10.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp, Alignment.CenterHorizontally),
    ) {
        val c = if (danger) CrundiColors.Red else CrundiColors.TextSecondary
        CrundiIcon(icon, size = 13.dp, tint = c)
        if (label != null) Text(label, style = CrundiType.Meta.copy(color = c, fontSize = 11.sp))
    }
}

/** The Media tab with live data (refetched when the server says media changed). */
@Composable
fun MediaRoute() {
    val services = LocalAppServices.current
    val api = services.api
    val nav = LocalNavigator.current
    val ctx = LocalContext.current
    val clipboard = LocalClipboardManager.current
    val toast = rememberToast()
    val scope = rememberCoroutineScope()
    val project = services.shell.state.collectAsState().value.currentProject
    var state by remember(project) { mutableStateOf(MediaState(project = project, thisProject = project != null)) }
    var reload by remember { mutableIntStateOf(0) }
    LaunchedEffect(project, state.kind, state.thisProject, reload) {
        runCatching { api.media.list(project = if (state.thisProject) project else null, kind = state.kind.key) }
            .onSuccess { state = state.copy(items = it, error = null) }
            .onFailure { state = state.copy(error = errorText(it)) }
    }
    LaunchedEffect(api) { api.events.changes("media").collect { reload++ } }
    val pick = rememberFilePicker { files ->
        scope.launch {
            for (f in files) {
                state = state.copy(uploading = f.name)
                runCatching { api.media.upload(f.name, f.size, project = if (state.thisProject) project else null, open = f.open) }
                    .onSuccess { toast("Uploaded ${f.name}") }.onFailure { toast("Upload failed: " + errorText(it)) }
            }
            state = state.copy(uploading = null); reload++
        }
    }
    MediaScreen(
        state,
        onQuery = { state = state.copy(query = it) },
        onKind = { state = state.copy(kind = it, items = null) },
        onScope = { state = state.copy(thisProject = it && project != null, items = null) },
        onUpload = pick,
        onOpen = { nav.to(WorkspaceRoutes.mediaItem(it.id)) },
        onDownload = { m -> scope.launch { saveMedia(ctx, m, api, toast) } },
        onCopyPath = { clipboard.setText(AnnotatedString(it.path)); toast("Path copied") },
        onDelete = { m -> scope.launch { runCatching { api.media.delete(m.id) }.onSuccess { toast("Deleted ${m.originalName}"); reload++ }.onFailure { toast("Delete failed: " + errorText(it)) } } },
        onRetry = { reload++ },
    )
}

private suspend fun saveMedia(ctx: android.content.Context, m: MediaItem, api: mv.obitech.crundi.api.CrundiClient, toast: (String) -> Unit) {
    runCatching { Downloads.save(ctx, m.originalName, m.mime) { sink -> api.media.download(m, sink) } }
        .onSuccess { toast("Saved ${it.name} to ${it.where}") }
        .onFailure { toast("Download failed: " + errorText(it)) }
}

// ─── One item ───

/** What the viewer holds: the item, and for video, audio and PDF the file fetched to the phone. */
data class MediaViewState(val item: MediaItem? = null, val error: String? = null, val local: File? = null, val fetching: Boolean = false)

/** One media item full screen: a picture to zoom, a player, PDF pages, or a download. */
@Composable
fun MediaViewScreen(state: MediaViewState, modifier: Modifier = Modifier, onBack: () -> Unit = {}, onDownload: () -> Unit = {}) {
    val m = state.item
    Column(modifier.fillMaxSize().background(CrundiColors.BgPrimary)) {
        TopBar(m?.originalName ?: "Media", subtitle = m?.let { listOfNotNull(formatSize(it.size).ifEmpty { null }, it.projectName, it.linkLabel).joinToString(" · ") }, onBack = onBack) {
            if (m != null) CrundiIconButton("download", onDownload, contentDescription = "Download")
        }
        when {
            state.error != null -> ErrorState(state.error)
            m == null -> LoadingState()
            m.kind == "image" -> ZoomableServerImage(m)
            m.kind in setOf("video", "audio", "pdf") && state.local == null -> LoadingState(text = if (state.fetching) "Fetching ${m.originalName}" else null)
            m.kind == "video" || m.kind == "audio" -> Player(state.local!!, audio = m.kind == "audio", name = m.originalName)
            m.kind == "pdf" -> PdfPages(state.local!!)
            else -> EmptyState(mediaIcon(m.kind), "No preview for this file", body = "Download it to open it in another app.", action = "Download", onAction = onDownload)
        }
    }
}

@Composable
private fun ZoomableServerImage(m: MediaItem) {
    val img = rememberServerImage(m.rawPath, maxPx = 2048)
    var scale by remember { mutableFloatStateOf(1f) }
    var dx by remember { mutableFloatStateOf(0f) }
    var dy by remember { mutableFloatStateOf(0f) }
    val t = rememberTransformableState { z, pan, _ -> scale = (scale * z).coerceIn(1f, 8f); dx += pan.x; dy += pan.y }
    Box(Modifier.fillMaxSize().transformable(t), contentAlignment = Alignment.Center) {
        if (img == null) LoadingState(text = "Loading ${m.originalName}")
        else Image(img, contentDescription = m.originalName, contentScale = ContentScale.Fit, modifier = Modifier.fillMaxSize().graphicsLayer { scaleX = scale; scaleY = scale; translationX = dx; translationY = dy })
    }
}

@Composable
private fun Player(file: File, audio: Boolean, name: String) {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        if (audio) Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.align(Alignment.TopCenter).padding(top = 80.dp)) {
            CrundiIcon("music", size = 48.dp, tint = CrundiColors.TextMuted)
            Text(name, style = CrundiType.Heading)
        }
        AndroidView(
            factory = { c ->
                VideoView(c).apply {
                    val mc = MediaController(c); mc.setAnchorView(this); setMediaController(mc)
                    setVideoPath(file.absolutePath)
                    setOnPreparedListener { start(); mc.show(0) }
                }
            },
            onRelease = { it.stopPlayback() },
            modifier = if (audio) Modifier.fillMaxWidth().height(120.dp).align(Alignment.BottomCenter) else Modifier.fillMaxWidth().aspectRatio(16f / 9f),
        )
    }
}

@Composable
private fun PdfPages(file: File) {
    val pages = remember(file) { mutableStateOf<List<Bitmap>?>(null) }
    var failed by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(file) {
        withContext(Dispatchers.IO) {
            runCatching {
                ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY).use { fd ->
                    PdfRenderer(fd).use { r ->
                        (0 until minOf(r.pageCount, 50)).map { i ->
                            r.openPage(i).use { p ->
                                val w = 1080; val h = (w.toFloat() * p.height / p.width).toInt()
                                Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888).also { b -> b.eraseColor(android.graphics.Color.WHITE); p.render(b, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY) }
                            }
                        }
                    }
                }
            }.onSuccess { pages.value = it }.onFailure { failed = "This PDF could not be opened here. Download it to read it in another app." }
        }
    }
    when {
        failed != null -> ErrorState(failed!!)
        pages.value == null -> LoadingState(text = "Rendering pages")
        else -> LazyColumn(Modifier.fillMaxSize(), contentPadding = androidx.compose.foundation.layout.PaddingValues(12.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            items(pages.value!!.size) { i -> Image(pages.value!![i].asImageBitmap(), contentDescription = "Page ${i + 1}", modifier = Modifier.fillMaxWidth(), contentScale = ContentScale.FillWidth) }
        }
    }
}

@Composable
fun MediaViewRoute(id: String) {
    val services = LocalAppServices.current
    val api = services.api
    val nav = LocalNavigator.current
    val ctx = LocalContext.current
    val toast = rememberToast()
    val scope = rememberCoroutineScope()
    var state by remember(id) { mutableStateOf(MediaViewState()) }
    LaunchedEffect(id) {
        val m = runCatching { api.media.list(project = null).firstOrNull { it.id == id } }.getOrElse { state = state.copy(error = errorText(it)); return@LaunchedEffect }
        if (m == null) { state = state.copy(error = "This media file is gone. It may have been deleted."); return@LaunchedEffect }
        state = state.copy(item = m)
        if (m.kind in setOf("video", "audio", "pdf")) {
            state = state.copy(fetching = true)
            val dir = File(ctx.cacheDir, "media").apply { mkdirs() }
            val f = File(dir, m.id + m.ext)
            runCatching {
                if (!f.exists() || f.length() != m.size) withContext(Dispatchers.IO) { f.outputStream().use { out -> api.media.download(m, out) } }
            }.onSuccess { state = state.copy(local = f, fetching = false) }
                .onFailure { f.delete(); state = state.copy(error = "Could not fetch it: " + errorText(it), fetching = false) }
        }
    }
    MediaViewScreen(state, onBack = { nav.back() }, onDownload = { state.item?.let { m -> scope.launch { saveMedia(ctx, m, api, toast) } } })
}
