package mv.obitech.crundi.ui.workspace.git

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import mv.obitech.crundi.api.model.GitCommit
import mv.obitech.crundi.api.model.GitCommitDetail
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.Banner
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiDivider
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.SectionHeader
import mv.obitech.crundi.ui.components.StatusBadge
import mv.obitech.crundi.ui.components.TopBar
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens
import mv.obitech.crundi.ui.workspace.WorkspaceRoutes
import mv.obitech.crundi.ui.workspace.errorText
import mv.obitech.crundi.ui.workspace.relativeTime
import mv.obitech.crundi.ui.workspace.rememberToast
import java.time.Instant

// ─── One file's diff ───

data class DiffState(
    val file: String,
    val staged: Boolean,
    val loading: Boolean = true,
    val error: String? = null,
    val parsed: DiffParser.File? = null,
    /** Chunks staged from this screen. */
    val stagedChunks: Set<Int> = emptySet(),
)

/**
 * One changed file, as a unified diff: chunk by chunk, with ↑ ↓ to jump
 * between chunks and, for unstaged changes, Stage on each chunk.
 */
@Composable
fun DiffScreen(state: DiffState, modifier: Modifier = Modifier, onBack: () -> Unit = {}, onStageChunk: ((Int) -> Unit)? = null, list: LazyListState = rememberLazyListState()) {
    val scope = rememberCoroutineScope()
    val p = state.parsed
    var current by remember { mutableIntStateOf(-1) }
    val starts = remember(p) { var at = 0; p?.hunks?.map { h -> at.also { at += 1 + h.lines.size } }.orEmpty() }
    Column(modifier.fillMaxSize().background(CrundiColors.BgPrimary)) {
        TopBar(state.file.substringAfterLast('/'), subtitle = (if (state.staged) "staged · " else "") + state.file, onBack = onBack) {
            if (p != null && p.hunks.isNotEmpty()) {
                CrundiIconButton("chevron-up", { current = (current - 1).coerceAtLeast(0); scope.launch { list.animateScrollToItem(starts[current]) } }, contentDescription = "Previous chunk", enabled = current > 0)
                Text(if (current < 0) "—/${p.hunks.size}" else "${current + 1}/${p.hunks.size}", style = CrundiType.Meta)
                CrundiIconButton("chevron-down", { current = (current + 1).coerceAtMost(p.hunks.lastIndex); scope.launch { list.animateScrollToItem(starts[current]) } }, contentDescription = "Next chunk", enabled = current < p.hunks.lastIndex)
            }
        }
        when {
            state.error != null -> ErrorState(state.error)
            state.loading -> LoadingState(text = "Loading the diff")
            p == null || p.hunks.isEmpty() -> EmptyState("file", if (p?.binary == true) "Binary file" else "No differences", body = if (p?.binary == true) "Git does not show the changes of a binary file." else "Nothing to show for this file. A new empty file or a mode change has no lines.")
            else -> {
                Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text("+${p.added}", style = CrundiType.Meta.copy(color = CrundiColors.Green))
                    Text("-${p.removed}", style = CrundiType.Meta.copy(color = CrundiColors.Red))
                    Text("${p.hunks.size} chunk${if (p.hunks.size == 1) "" else "s"}", style = CrundiType.Meta)
                }
                CrundiDivider()
                Box(Modifier.fillMaxSize().horizontalScroll(rememberScrollState())) {
                    LazyColumn(state = list) { diffItems(p, "d", state.stagedChunks, onStageChunk) }
                }
            }
        }
    }
}

@Composable
fun DiffRoute(file: String, staged: Boolean) {
    val services = LocalAppServices.current
    val api = services.api
    val nav = LocalNavigator.current
    val toast = rememberToast()
    val scope = rememberCoroutineScope()
    val project = services.shell.state.collectAsState().value.currentProject
    var state by remember(file, staged) { mutableStateOf(DiffState(file, staged)) }
    LaunchedEffect(file, staged, project) {
        val p = project ?: return@LaunchedEffect
        runCatching { api.git.diff(p, file, staged) }
            .onSuccess { d ->
                // An untracked file has no diff from git: show it all as added.
                val text = d.diff.ifEmpty { if (d.new.isNotEmpty() && d.old.isEmpty()) newFileDiff(file, d.new) else "" }
                state = state.copy(loading = false, parsed = DiffParser.parse(text).firstOrNull())
            }
            .onFailure { state = state.copy(loading = false, error = errorText(it)) }
    }
    DiffScreen(
        state, onBack = { nav.back() },
        onStageChunk = if (staged || state.parsed?.header?.contains("new file") == true || state.parsed == null) null else { i ->
            val p = project ?: return@DiffScreen
            val f = state.parsed ?: return@DiffScreen
            scope.launch {
                runCatching { api.git.stageHunk(p, DiffParser.patchFor(f, f.hunks[i])) }
                    .onSuccess { state = state.copy(stagedChunks = state.stagedChunks + i); toast("Chunk ${i + 1} staged") }
                    .onFailure { toast("Stage failed: " + errorText(it)) }
            }
        },
    )
}

/** A whole new file as a diff that adds every line. */
fun newFileDiff(file: String, content: String): String {
    val lines = content.removeSuffix("\n").split('\n')
    return "diff --git a/$file b/$file\nnew file\n--- /dev/null\n+++ b/$file\n@@ -0,0 +1,${lines.size} @@\n" + lines.joinToString("\n") { "+$it" } + "\n"
}

// ─── History ───

data class HistoryState(
    val commits: List<GitCommit> = emptyList(),
    val loading: Boolean = true,
    val more: Boolean = false,
    val loadingMore: Boolean = false,
    val error: String? = null,
    val branch: String? = null,
    val now: Instant = Instant.now(),
)

/** The branch's commits, newest first, 50 at a time. */
@Composable
fun HistoryScreen(state: HistoryState, modifier: Modifier = Modifier, onBack: () -> Unit = {}, onOpen: (GitCommit) -> Unit = {}, onMore: () -> Unit = {}, onRetry: () -> Unit = {}) {
    Column(modifier.fillMaxSize().background(CrundiColors.BgPrimary)) {
        TopBar("History", subtitle = state.branch, onBack = onBack)
        when {
            state.error != null -> ErrorState(state.error, onRetry = onRetry)
            state.loading -> LoadingState(text = "Reading the history")
            state.commits.isEmpty() -> EmptyState("git-commit", "No commits yet", body = "Stage some changes and commit them from the Git tab.")
            else -> LazyColumn(Modifier.fillMaxSize()) {
                items(state.commits, key = { it.hash }) { c -> CommitRow(c, state.now) { onOpen(c) } }
                if (state.more) item(key = "more") {
                    Box(Modifier.fillMaxWidth().padding(16.dp), contentAlignment = Alignment.Center) {
                        CrundiButton(if (state.loadingMore) "Loading…" else "Load older commits", onMore, enabled = !state.loadingMore)
                    }
                }
            }
        }
    }
}

@Composable
private fun CommitRow(c: GitCommit, now: Instant, onClick: () -> Unit) {
    Row(Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 10.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Box(Modifier.padding(top = 3.dp)) { mv.obitech.crundi.ui.icons.CrundiIcon("git-commit", size = Dimens.IconSm, tint = if (c.parents.size > 1) CrundiColors.Sky else CrundiColors.TextMuted) }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(c.subject.ifEmpty { "(no message)" }, style = CrundiType.Body, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(c.short, style = CrundiType.Meta.copy(color = CrundiColors.AccentHover))
                Text("${c.author} · ${relativeTime(c.date, now)}", style = CrundiType.Meta, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            val refs = c.refs.map { it.removePrefix("HEAD -> ") }.filter { it != "HEAD" }
            if (refs.isNotEmpty()) FlowRow(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                for (r in refs) StatusBadge(r.removePrefix("tag: "), if (r.startsWith("tag: ")) BadgeKind.Warn else if (r.contains('/')) BadgeKind.Neutral else BadgeKind.Accent)
            }
        }
    }
    CrundiDivider()
}

@Composable
fun HistoryRoute() {
    val services = LocalAppServices.current
    val api = services.api
    val nav = LocalNavigator.current
    val scope = rememberCoroutineScope()
    val project = services.shell.state.collectAsState().value.currentProject
    var state by remember(project) { mutableStateOf(HistoryState()) }
    var reload by remember { mutableIntStateOf(0) }
    LaunchedEffect(project, reload) {
        val p = project ?: run { state = state.copy(loading = false, error = "Pick a project from the menu first."); return@LaunchedEffect }
        state = state.copy(loading = true, error = null)
        val branch = runCatching { api.git.status(p).branch }.getOrNull()
        runCatching { api.git.log(p) }
            .onSuccess { state = state.copy(commits = it.commits, more = it.more, loading = false, branch = branch, now = Instant.now()) }
            .onFailure { state = state.copy(loading = false, error = errorText(it)) }
    }
    HistoryScreen(
        state, onBack = { nav.back() }, onRetry = { reload++ },
        onOpen = { nav.to(WorkspaceRoutes.commit(it.hash)) },
        onMore = {
            val p = project ?: return@HistoryScreen
            state = state.copy(loadingMore = true)
            scope.launch {
                runCatching { api.git.log(p, skip = state.commits.size) }
                    .onSuccess { state = state.copy(commits = state.commits + it.commits, more = it.more, loadingMore = false) }
                    .onFailure { state = state.copy(loadingMore = false, error = errorText(it)) }
            }
        },
    )
}

// ─── One commit ───

data class CommitState(
    val hash: String,
    val detail: GitCommitDetail? = null,
    val files: List<DiffParser.File> = emptyList(),
    val error: String? = null,
    val now: Instant = Instant.now(),
)

/** One commit: its message, who and when, the files it changed, and each file's diff. */
@Composable
fun CommitScreen(state: CommitState, modifier: Modifier = Modifier, onBack: () -> Unit = {}) {
    val d = state.detail
    val screenW = LocalConfiguration.current.screenWidthDp.dp
    Column(modifier.fillMaxSize().background(CrundiColors.BgPrimary)) {
        TopBar(d?.subject ?: "Commit", subtitle = state.hash.take(7), onBack = onBack)
        when {
            state.error != null -> ErrorState(state.error)
            d == null -> LoadingState(text = "Loading the commit")
            else -> Box(Modifier.fillMaxSize().horizontalScroll(rememberScrollState())) {
                LazyColumn(Modifier.fillMaxSize()) {
                    item(key = "head") {
                        Column(Modifier.width(screenW).padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                            Text(d.subject, style = CrundiType.Heading)
                            if (d.body.isNotBlank()) Text(d.body, style = CrundiType.Small.copy(color = CrundiColors.TextPrimary))
                            Text("${d.author} <${d.email}>", style = CrundiType.Meta)
                            Text("${relativeTime(d.date, state.now)} · ${d.hash}", style = CrundiType.Meta)
                            if (d.truncated) Banner("This commit is large. Its diff is cut short here.", Modifier.fillMaxWidth(), kind = BadgeKind.Warn, icon = "alert-triangle")
                        }
                        CrundiDivider()
                    }
                    item(key = "files") {
                        Column(Modifier.width(screenW)) {
                            SectionHeader("${d.files.size} file${if (d.files.size == 1) "" else "s"} changed")
                            for (f in d.files) Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                Text(f.file, style = CrundiType.Code.copy(fontSize = CrundiType.Meta.fontSize), maxLines = 1, overflow = TextOverflow.StartEllipsis, modifier = Modifier.weight(1f))
                                if (f.binary) Text("binary", style = CrundiType.Meta)
                                else { Text("+${f.add}", style = CrundiType.Meta.copy(color = CrundiColors.Green)); Text("-${f.del}", style = CrundiType.Meta.copy(color = CrundiColors.Red)) }
                            }
                            Box(Modifier.padding(bottom = 8.dp))
                        }
                    }
                    state.files.forEachIndexed { fi, f ->
                        item(key = "f$fi") {
                            Column(Modifier.widthIn(min = screenW)) {
                                CrundiDivider(strong = true)
                                Text(f.name, style = CrundiType.Code.copy(fontWeight = FontWeight.SemiBold), modifier = Modifier.background(CrundiColors.BgSecondary).fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp))
                            }
                        }
                        diffItems(f, "f$fi")
                    }
                }
            }
        }
    }
}

@Composable
fun CommitRoute(hash: String) {
    val services = LocalAppServices.current
    val api = services.api
    val nav = LocalNavigator.current
    val project = services.shell.state.collectAsState().value.currentProject
    var state by remember(hash) { mutableStateOf(CommitState(hash)) }
    LaunchedEffect(hash, project) {
        val p = project ?: return@LaunchedEffect
        runCatching { api.git.show(p, hash) }
            .onSuccess { state = state.copy(detail = it, files = DiffParser.parse(it.diff), now = Instant.now()) }
            .onFailure { state = state.copy(error = errorText(it)) }
    }
    CommitScreen(state, onBack = { nav.back() })
}

