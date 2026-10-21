package mv.obitech.crundi.ui.workspace.files

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.rememberTransformableState
import androidx.compose.foundation.gestures.transformable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
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
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import mv.obitech.crundi.Downloads
import mv.obitech.crundi.api.rememberServerImage
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.Highlighter
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.TopBar
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.workspace.errorText
import mv.obitech.crundi.ui.workspace.formatSize
import mv.obitech.crundi.ui.workspace.rememberToast
import java.net.URLEncoder

/** What a file screen is showing. */
sealed interface FileBody {
    data object Loading : FileBody
    data class Failed(val message: String) : FileBody
    /** Over the server's 1 MB, or not text: offer a download. */
    data class TooBig(val reason: String) : FileBody
    data class Image(val serverPath: String) : FileBody
    data class Text(val original: String) : FileBody
}

data class FileViewState(
    val path: String,
    val project: String? = null,
    val body: FileBody = FileBody.Loading,
    val editing: Boolean = false,
    val draft: String = "",
    val saving: Boolean = false,
    val downloading: Boolean = false,
) {
    val name get() = path.substringAfterLast('/')
    val dirty get() = editing && body is FileBody.Text && draft != body.original
}

/** Text split into lines, highlighted once as a whole (so comments over several lines stay right). */
fun highlightedLines(text: String, language: String?): List<AnnotatedString> {
    val hl = Highlighter.highlight(text, language)
    val out = ArrayList<AnnotatedString>()
    var start = 0
    while (true) {
        val nl = text.indexOf('\n', start)
        if (nl < 0) { if (start < text.length || out.isEmpty()) out += hl.subSequence(start, text.length); break }
        out += hl.subSequence(start, nl)
        start = nl + 1
    }
    return out
}

/**
 * One file: highlighted with line numbers, or an editor (Edit, then Save
 * when something changed), an image, or a download offer for what cannot be
 * shown. As the desktop's file window, full screen.
 */
@Composable
fun FileViewScreen(
    state: FileViewState,
    modifier: Modifier = Modifier,
    onBack: () -> Unit = {},
    onEdit: () -> Unit = {},
    onDraft: (String) -> Unit = {},
    onSave: () -> Unit = {},
    onCancelEdit: () -> Unit = {},
    onDownload: () -> Unit = {},
    onRetry: () -> Unit = {},
) {
    Column(modifier.fillMaxSize().background(CrundiColors.BgPrimary).imePadding()) {
        val rel = state.path.let { p -> state.project?.let { p.substringAfter("/$it/", p) } ?: p }
        TopBar(state.name, subtitle = rel, onBack = onBack) {
            if (state.editing) {
                CrundiButton("Cancel", onCancelEdit, kind = ButtonKind.Ghost)
                CrundiButton(if (state.saving) "Saving…" else "Save", onSave, kind = ButtonKind.Primary, enabled = state.dirty && !state.saving, modifier = Modifier.padding(end = 6.dp))
            } else {
                if (state.body is FileBody.Text) CrundiIconButton("pencil", onEdit, contentDescription = "Edit")
                CrundiIconButton("download", onDownload, contentDescription = "Download", enabled = !state.downloading)
            }
        }
        when (val b = state.body) {
            FileBody.Loading -> LoadingState(text = "Opening ${state.name}")
            is FileBody.Failed -> ErrorState(b.message, onRetry = onRetry)
            is FileBody.TooBig -> EmptyState("file", "This file cannot be shown here", body = b.reason, action = "Download", onAction = onDownload)
            is FileBody.Image -> ZoomImage(b.serverPath, state.name)
            is FileBody.Text -> if (state.editing) Editor(state.draft, onDraft) else CodeView(b.original, FileIcons.language(state.name))
        }
    }
}

@Composable
private fun CodeView(text: String, language: String?) {
    val lines = remember(text, language) { highlightedLines(text, language) }
    val digits = lines.size.toString().length
    val numberStyle = CrundiType.Code.copy(color = CrundiColors.TextMuted, textAlign = TextAlign.End)
    SelectionContainer {
        Box(Modifier.fillMaxSize().horizontalScroll(rememberScrollState())) {
            LazyColumn(Modifier.fillMaxSize().padding(vertical = 6.dp)) {
                items(lines.size) { i ->
                    Row {
                        Text((i + 1).toString(), style = numberStyle, modifier = Modifier.width((digits * 8 + 14).dp).padding(end = 10.dp))
                        Text(lines[i], style = CrundiType.Code, softWrap = false, modifier = Modifier.padding(end = 16.dp))
                    }
                }
            }
        }
    }
}

@Composable
private fun Editor(draft: String, onDraft: (String) -> Unit) {
    BasicTextField(
        draft, onDraft,
        textStyle = CrundiType.Code,
        cursorBrush = SolidColor(CrundiColors.AccentHover),
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false),
        modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()).horizontalScroll(rememberScrollState()).padding(12.dp),
    )
}

@Composable
private fun ZoomImage(serverPath: String, name: String) {
    val img = rememberServerImage(serverPath, maxPx = 2048)
    var scale by remember { mutableFloatStateOf(1f) }
    var dx by remember { mutableFloatStateOf(0f) }
    var dy by remember { mutableFloatStateOf(0f) }
    val t = rememberTransformableState { z, pan, _ -> scale = (scale * z).coerceIn(1f, 8f); dx += pan.x; dy += pan.y }
    Box(Modifier.fillMaxSize().transformable(t), contentAlignment = Alignment.Center) {
        if (img == null) LoadingState(text = "Loading $name")
        else Image(img, contentDescription = name, contentScale = ContentScale.Fit, modifier = Modifier.fillMaxSize().graphicsLayer { scaleX = scale; scaleY = scale; translationX = dx; translationY = dy })
    }
}

/** The server path that shows a project's file as a picture (with the phone's login). */
fun fileImagePath(project: String, file: String) =
    "/api/files/download?project=" + URLEncoder.encode(project, "UTF-8") + "&file=" + URLEncoder.encode(file, "UTF-8")

/** One file, live. */
@Composable
fun FileViewRoute(path: String) {
    val services = LocalAppServices.current
    val api = services.api
    val nav = LocalNavigator.current
    val ctx = LocalContext.current
    val toast = rememberToast()
    val scope = rememberCoroutineScope()
    val shell by services.shell.state.collectAsState()
    val project = shell.currentProject
    var state by remember(path) { mutableStateOf(FileViewState(path, project)) }
    var reload by remember { mutableIntStateOf(0) }
    var leaving by remember { mutableStateOf(false) }

    LaunchedEffect(path, project, reload) {
        val p = project ?: run { state = state.copy(body = FileBody.Failed("Pick a project from the menu first.")); return@LaunchedEffect }
        if (FileIcons.isImage(state.name)) { state = state.copy(project = p, body = FileBody.Image(fileImagePath(p, path))); return@LaunchedEffect }
        state = state.copy(project = p, body = FileBody.Loading)
        runCatching { api.files.read(p, path) }
            .onSuccess { r ->
                state = state.copy(body = if (r.content.indexOf('\u0000') >= 0) FileBody.TooBig("It looks like a binary file (${formatSize(r.size)}). Download it to open it in another app.") else FileBody.Text(r.content))
            }
            .onFailure { e ->
                val msg = errorText(e)
                state = state.copy(body = if (msg.contains("too large", true)) FileBody.TooBig("Files over 1 MB open as a download. Save it to the phone to read it.") else FileBody.Failed(msg))
            }
    }
    BackHandler(enabled = state.dirty) { leaving = true }
    FileViewScreen(
        state,
        onBack = { if (state.dirty) leaving = true else nav.back() },
        onEdit = { (state.body as? FileBody.Text)?.let { state = state.copy(editing = true, draft = it.original) } },
        onDraft = { state = state.copy(draft = it) },
        onCancelEdit = { if (state.dirty) leaving = true else state = state.copy(editing = false) },
        onSave = {
            val p = state.project ?: return@FileViewScreen
            val text = state.draft
            state = state.copy(saving = true)
            scope.launch {
                runCatching { api.files.write(p, path, text) }
                    .onSuccess { state = state.copy(saving = false, body = FileBody.Text(text)); toast("Saved") }
                    .onFailure { state = state.copy(saving = false); toast("Save failed: " + errorText(it)) }
            }
        },
        onDownload = {
            val p = state.project ?: return@FileViewScreen
            state = state.copy(downloading = true)
            scope.launch {
                runCatching { Downloads.save(ctx, state.name) { sink -> api.files.download(p, path, sink) } }
                    .onSuccess { toast("Saved ${it.name} to ${it.where}") }
                    .onFailure { toast("Download failed: " + errorText(it)) }
                state = state.copy(downloading = false)
            }
        },
        onRetry = { reload++ },
    )
    if (leaving) CrundiDialog(
        "Discard your changes?", onDismiss = { leaving = false },
        message = "What you changed in ${state.name} has not been saved.",
        confirm = "Discard changes", danger = true,
        onConfirm = { leaving = false; state = state.copy(editing = false, draft = "") },
    )
}
