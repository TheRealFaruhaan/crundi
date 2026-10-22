package mv.obitech.crundi.ui.workspace.git

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import mv.obitech.crundi.api.model.GitFile
import mv.obitech.crundi.ui.workbench.paneDragSource
import mv.obitech.crundi.api.model.GitStatus
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiDivider
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens
import mv.obitech.crundi.ui.workspace.WorkspaceRoutes
import mv.obitech.crundi.ui.components.errorText
import mv.obitech.crundi.ui.workspace.rememberToast

/** What the Git tab is doing right now (one thing at a time, as the desktop's buttons). */
enum class GitBusy(val label: String) { Committing("Committing…"), Pushing("Pushing…"), Pulling("Pulling…") }

data class GitState(
    val project: String? = null,
    val status: GitStatus? = null,
    val loading: Boolean = false,
    /** "Not a git repository", or the server's words. */
    val error: String? = null,
    val message: String = "",
    val busy: GitBusy? = null,
)

/** The status letter the desktop shows: the index side for staged rows, the work tree side otherwise. */
fun statusLetter(f: GitFile): String {
    val c = if (f.staged) f.xy.getOrNull(0) else f.xy.getOrNull(1)
    return when {
        f.status == "untracked" -> "U"
        c == null || c == ' ' -> f.status.take(1).uppercase()
        else -> c.toString()
    }
}

private fun letterColor(l: String) = when (l) {
    "M" -> CrundiColors.Yellow
    "A" -> CrundiColors.Green
    "D" -> CrundiColors.Red
    "U" -> CrundiColors.Accent
    "R" -> CrundiColors.Sky
    else -> CrundiColors.TextSecondary
}

/**
 * The Git tab: the branch and how far it is from its upstream, staged and
 * unstaged changes (stage, unstage, discard, open the diff), and the commit
 * bar with Pull, Push and Commit, as the desktop's git panel.
 */
@Composable
fun GitScreen(
    state: GitState,
    modifier: Modifier = Modifier,
    onOpen: (GitFile) -> Unit = {},
    onStage: (List<GitFile>) -> Unit = {},
    onUnstage: (List<GitFile>) -> Unit = {},
    onDiscard: (GitFile) -> Unit = {},
    onMessage: (String) -> Unit = {},
    onCommit: () -> Unit = {},
    onPush: () -> Unit = {},
    onPull: () -> Unit = {},
    onHistory: () -> Unit = {},
    onRefresh: () -> Unit = {},
    /** The project's folder on the server, so a dragged row carries an absolute path as the web's does. */
    root: String? = null,
) {
    var discarding by remember { mutableStateOf<GitFile?>(null) }
    Column(modifier.fillMaxSize().background(CrundiColors.BgPrimary).imePadding()) {
        if (state.project == null) {
            EmptyState("git-branch", "Pick a project", body = "Git shows the changes of one project. Choose one from the menu.")
            return@Column
        }
        val s = state.status
        // The branch line (.git-branch).
        Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 4.dp).height(44.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiIcon("git-branch", size = Dimens.IconSm, tint = CrundiColors.TextSecondary)
            Text(s?.branch?.ifEmpty { null } ?: "—", style = CrundiType.Code.copy(color = CrundiColors.AccentHover, fontWeight = FontWeight.SemiBold), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
            if (s != null && (s.ahead > 0 || s.behind > 0)) Text(listOfNotNull(if (s.ahead > 0) "↑${s.ahead}" else null, if (s.behind > 0) "↓${s.behind}" else null).joinToString(" "), style = CrundiType.Meta)
            Box(Modifier.weight(1f))
            CrundiIconButton("git-commit", onHistory, contentDescription = "History")
            CrundiIconButton("refresh", onRefresh, contentDescription = "Refresh")
        }
        CrundiDivider()
        Box(Modifier.weight(1f).fillMaxWidth()) {
            when {
                state.error != null -> EmptyState(
                    "git-branch",
                    if (state.error.contains("not a git", true)) "Not a git repository" else "Could not read git",
                    body = if (state.error.contains("not a git", true)) "${state.project} has no git repository. Run git init in a terminal to start one." else state.error,
                    action = "Try again", onAction = onRefresh,
                )
                s == null -> LoadingState(text = "Reading git status")
                s.files.isEmpty() -> EmptyState("check", "No changes", body = "The working tree matches the last commit. Open the history to see what came before.", action = "Open history", onAction = onHistory)
                else -> LazyColumn(Modifier.fillMaxSize().padding(vertical = 4.dp)) {
                    if (s.staged.isNotEmpty()) {
                        item(key = "sh") { GitSection("Staged", s.staged.size, "minus", "Unstage all") { onUnstage(s.staged) } }
                        items(s.staged, key = { "s:" + it.file }) { f -> GitRow(f, root, onOpen = { onOpen(f) }, actions = { CrundiIconButton("minus", { onUnstage(listOf(f)) }, contentDescription = "Unstage", size = 15.dp, touch = 40.dp) }) }
                    }
                    if (s.unstaged.isNotEmpty()) {
                        item(key = "uh") { GitSection("Changes", s.unstaged.size, "plus", "Stage all") { onStage(s.unstaged) } }
                        items(s.unstaged, key = { "u:" + it.file }) { f ->
                            GitRow(f, root, onOpen = { onOpen(f) }, actions = {
                                CrundiIconButton("rotate-ccw", { discarding = f }, contentDescription = "Discard", size = 15.dp, touch = 40.dp)
                                CrundiIconButton("plus", { onStage(listOf(f)) }, contentDescription = "Stage", size = 15.dp, touch = 40.dp)
                            })
                        }
                    }
                }
            }
        }
        if (state.error == null) CommitBar(state, onMessage, onCommit, onPush, onPull)
    }
    discarding?.let { f ->
        CrundiDialog(
            "Discard changes?", onDismiss = { discarding = null },
            message = "Discard changes to ${f.file}? This cannot be undone." + if (f.status == "untracked") " The file is new, so it is deleted." else "",
            confirm = "Discard changes", danger = true, onConfirm = { discarding = null; onDiscard(f) },
        )
    }
}

@Composable
private fun GitSection(title: String, count: Int, icon: String, action: String, onAction: () -> Unit) {
    Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 4.dp, top = 6.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(
            buildAnnotatedString { append(title.uppercase()); withStyle(SpanStyle(color = CrundiColors.TextSecondary)) { append(" ($count)") } },
            style = CrundiType.Section, modifier = Modifier.weight(1f),
        )
        CrundiIconButton(icon, onAction, contentDescription = action, size = 15.dp, touch = 40.dp)
    }
}

@Composable
private fun GitRow(f: GitFile, root: String?, onOpen: () -> Unit, actions: @Composable () -> Unit) {
    val letter = statusLetter(f)
    Row(
        // Hold and drag onto a chat, terminal or notes page: its path, as the web's git rows.
        Modifier.fillMaxWidth().paneDragSource("file", if (root.isNullOrEmpty()) f.file else absPath(root, f.file), f.file.substringAfterLast('/')).clickable(onClick = onOpen).padding(start = 16.dp, end = 4.dp).height(Dimens.Touch),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(letter, style = CrundiType.Code.copy(fontSize = 12.sp, fontWeight = FontWeight.Bold, color = letterColor(letter)), textAlign = TextAlign.Center, modifier = Modifier.width(16.dp))
        val dir = f.file.substringBeforeLast('/', "")
        Text(
            buildAnnotatedString {
                if (dir.isNotEmpty()) withStyle(SpanStyle(color = CrundiColors.TextMuted)) { append("$dir/") }
                append(f.file.substringAfterLast('/'))
            },
            style = CrundiType.Code.copy(fontSize = 12.sp), maxLines = 1, overflow = TextOverflow.StartEllipsis, modifier = Modifier.weight(1f),
        )
        if ((f.add ?: 0) > 0) Text("+${f.add}", style = CrundiType.Meta.copy(color = CrundiColors.Green))
        if ((f.del ?: 0) > 0) Text("-${f.del}", style = CrundiType.Meta.copy(color = CrundiColors.Red))
        actions()
    }
}

@Composable
private fun CommitBar(state: GitState, onMessage: (String) -> Unit, onCommit: () -> Unit, onPush: () -> Unit, onPull: () -> Unit) {
    Column(Modifier.fillMaxWidth().background(CrundiColors.BgSecondary)) {
        CrundiDivider(strong = true)
        Column(Modifier.padding(horizontal = 12.dp, vertical = 10.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiTextField(state.message, onMessage, placeholder = "Commit message…", singleLine = false, minLines = 2, maxLines = 5, modifier = Modifier.fillMaxWidth())
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                val busy = state.busy != null
                CrundiButton(if (state.busy == GitBusy.Pulling) "Pulling…" else "Pull", onPull, icon = "chevron-down", enabled = !busy, modifier = Modifier.weight(1f))
                CrundiButton(if (state.busy == GitBusy.Pushing) "Pushing…" else "Push", onPush, icon = "chevron-up", enabled = !busy, modifier = Modifier.weight(1f))
                CrundiButton(
                    if (state.busy == GitBusy.Committing) "Committing…" else "Commit", onCommit, kind = ButtonKind.Primary, icon = "check",
                    enabled = !busy && state.status?.staged?.isNotEmpty() == true, modifier = Modifier.weight(1f),
                )
            }
        }
    }
}

/** The Git tab with live data: refreshed every 8 seconds while open, as the desktop. */
@Composable
fun GitRoute() {
    val services = LocalAppServices.current
    val api = services.api
    val nav = LocalNavigator.current
    val toast = rememberToast()
    val scope = rememberCoroutineScope()
    val shell by services.shell.state.collectAsState()
    val project = shell.currentProject
    var state by remember(project) { mutableStateOf(GitState(project = project)) }
    var reload by remember { mutableIntStateOf(0) }

    suspend fun load() {
        val p = project ?: return
        runCatching { api.git.status(p) }
            .onSuccess { state = state.copy(status = it, error = null) }
            .onFailure { state = state.copy(error = errorText(it)) }
    }
    LaunchedEffect(project, reload) {
        if (project == null) return@LaunchedEffect
        while (isActive) { load(); delay(8_000) }
    }
    fun act(what: String, block: suspend () -> Unit) = scope.launch {
        runCatching { block() }.onFailure { toast("$what failed: " + errorText(it)) }
        load()
    }
    GitScreen(
        state,
        onOpen = { nav.to(WorkspaceRoutes.diff(it.file, it.staged)) },
        onStage = { fs -> act("Stage") { api.git.stage(project!!, fs.map { it.file }) } },
        onUnstage = { fs -> act("Unstage") { api.git.unstage(project!!, fs.map { it.file }) } },
        onDiscard = { f -> act("Discard") { api.git.discard(project!!, f.file) } },
        onMessage = { state = state.copy(message = it) },
        onCommit = {
            val msg = state.message.trim()
            if (msg.isEmpty()) { toast("Enter a commit message"); return@GitScreen }
            state = state.copy(busy = GitBusy.Committing)
            scope.launch {
                runCatching { api.git.commit(project!!, msg) }
                    .onSuccess { state = state.copy(message = ""); toast("Committed") }
                    .onFailure { toast("Commit failed: " + errorText(it)) }
                state = state.copy(busy = null); load()
            }
        },
        onPush = {
            state = state.copy(busy = GitBusy.Pushing)
            scope.launch {
                runCatching { api.git.push(project!!) }.onSuccess { toast("Pushed") }.onFailure { toast("Push failed: " + errorText(it)) }
                state = state.copy(busy = null); load()
            }
        },
        onPull = {
            state = state.copy(busy = GitBusy.Pulling)
            scope.launch {
                runCatching { api.git.pull(project!!) }.onSuccess { toast("Pulled") }.onFailure { toast("Pull failed: " + errorText(it)) }
                state = state.copy(busy = null); load()
            }
        },
        onHistory = { nav.to(WorkspaceRoutes.HISTORY) },
        onRefresh = { reload++ },
        root = shell.project?.path,
    )
}

/** [rel] (as git reports it, with forward slashes) under [root], with the root's own separator. */
fun absPath(root: String, rel: String): String {
    if (rel.startsWith("/") || Regex("^[A-Za-z]:[\\\\/]").containsMatchIn(rel)) return rel
    val sep = if (root.contains('\\') && !root.contains('/')) '\\' else '/'
    return root.trimEnd('/', '\\') + sep + rel.replace('/', sep)
}
