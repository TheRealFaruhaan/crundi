package mv.obitech.crundi.ui.inbox

import androidx.lifecycle.compose.collectAsStateWithLifecycle
import android.content.Intent
import android.widget.Toast
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.Text
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.material3.rememberSwipeToDismissBoxState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.repeatOnLifecycle
import androidx.navigation.NavGraphBuilder
import androidx.navigation.NavType
import androidx.navigation.compose.composable
import androidx.navigation.navArgument
import kotlinx.coroutines.launch
import mv.obitech.crundi.api.ApiRequest
import mv.obitech.crundi.api.InboxApi
import mv.obitech.crundi.api.InboxEvent
import mv.obitech.crundi.api.InboxItem
import mv.obitech.crundi.api.InboxList
import mv.obitech.crundi.api.InboxMedia
import mv.obitech.crundi.api.InboxThread
import mv.obitech.crundi.api.InboxThreadList
import mv.obitech.crundi.api.rememberServerImage
import mv.obitech.crundi.ui.chats.ago
import mv.obitech.crundi.ui.chats.bytes
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiDivider
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.MarkdownText
import mv.obitech.crundi.ui.components.TopBar
import mv.obitech.crundi.ui.components.errorText
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.nav.DeepLinks
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.nav.Routes
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.shell.LocalHost
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/** Crundi's inbox: its threads ("inbox"), and one thread as a conversation ("inbox/thread?key=&open="). */
fun NavGraphBuilder.inboxGraph() {
    composable(Routes.INBOX) { InboxRoute() }
    composable(
        Routes.INBOX_THREAD,
        arguments = listOf(
            navArgument("key") { type = NavType.StringType; nullable = true; defaultValue = null },
            navArgument("open") { type = NavType.StringType; nullable = true; defaultValue = null },
        ),
    ) { e -> InboxThreadRoute(key = e.arguments?.getString("key"), open = e.arguments?.getString("open")) }
}

/**
 * Tells the window an inbox thread is in front of the person while this is
 * composed and resumed, again every three seconds (as ChatOnScreenEffect).
 */
@Composable
fun InboxOnScreenEffect(thread: String, host: mv.obitech.crundi.ui.shell.Host) {
    if (thread.isEmpty()) return
    val owner = androidx.lifecycle.compose.LocalLifecycleOwner.current
    LaunchedEffect(thread, owner, host) {
        owner.repeatOnLifecycle(androidx.lifecycle.Lifecycle.State.RESUMED) {
            try { while (true) { host.inboxOnScreen(thread, true); kotlinx.coroutines.delay(3_000) } }
            finally { host.inboxOnScreen(thread, false) }
        }
    }
}

private fun toast(ctx: android.content.Context, text: String) = Toast.makeText(ctx.applicationContext, text, Toast.LENGTH_LONG).show()

// ─── The threads ───

data class InboxThreadsUi(
    val list: InboxThreadList = InboxThreadList(),
    val refreshing: Boolean = false,
    val error: String? = null,
)

class InboxThreadsEvents(
    val onBack: () -> Unit = {},
    val onRefresh: () -> Unit = {},
    val onOpen: (InboxThread) -> Unit = {},
    val onDelete: (InboxThread) -> Unit = {},
    val onMarkAllRead: () -> Unit = {},
)

@Composable
fun InboxRoute() {
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var ui by remember { mutableStateOf(InboxThreadsUi()) }

    fun refresh(quiet: Boolean = false) {
        if (!quiet) ui = ui.copy(refreshing = true)
        scope.launch {
            try {
                val t = api.inbox.threads()
                ui = ui.copy(list = ui.list.loaded(t), refreshing = false, error = null)
            } catch (e: Exception) {
                ui = ui.copy(refreshing = false, error = if (ui.list.loaded) ui.error else errorText(e))
                if (ui.list.loaded && !quiet) toast(ctx, errorText(e))
            }
        }
    }

    LaunchedEffect(Unit) { refresh() }
    // Back to this screen or to the app: fetch again (a thread read or deleted, changes missed while away).
    LifecycleResumeEffect(Unit) { if (ui.list.loaded) refresh(quiet = true); onPauseOrDispose {} }
    // Live: any change may move a thread to the top or change its count.
    LaunchedEffect(Unit) {
        api.events.changes(InboxEvent.NAME).collect { e ->
            val ev = InboxEvent.parse(e.data) ?: return@collect
            ui = ui.copy(list = ui.list.copy(unread = ev.unread))
            refresh(quiet = true)
        }
    }

    InboxThreadsScreen(ui, InboxThreadsEvents(
        onBack = { nav.back() },
        onRefresh = { refresh() },
        onOpen = { nav.to(Routes.inboxThread(it.key)) },
        onDelete = { t ->
            ui = ui.copy(list = ui.list.delete(t.key))
            scope.launch { try { api.inbox.deleteThread(t.key) } catch (e: Exception) { toast(ctx, errorText(e)); refresh(quiet = true) } }
        },
        onMarkAllRead = {
            ui = ui.copy(list = ui.list.markAllRead())
            scope.launch { try { api.inbox.markAllRead() } catch (e: Exception) { toast(ctx, errorText(e)); refresh(quiet = true) } }
        },
    ))
}

/** The inbox's threads, stateless: newest activity first. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun InboxThreadsScreen(ui: InboxThreadsUi, events: InboxThreadsEvents = InboxThreadsEvents()) {
    val list = ui.list
    var deleting by remember { mutableStateOf<InboxThread?>(null) }
    Column(Modifier.fillMaxSize()) {
        TopBar("Inbox", subtitle = if (list.unread > 0) "${list.unread} unread" else null, onBack = events.onBack) {
            if (list.unread > 0) CrundiIconButton("check", events.onMarkAllRead, contentDescription = "Mark all read")
            CrundiIconButton("refresh", events.onRefresh, contentDescription = "Refresh")
        }
        when {
            !list.loaded && ui.error != null -> ErrorState(ui.error, onRetry = events.onRefresh)
            !list.loaded -> LoadingState()
            else -> PullToRefreshBox(ui.refreshing, events.onRefresh, Modifier.fillMaxWidth().weight(1f)) {
                LazyColumn(Modifier.fillMaxSize().navigationBarsPadding()) {
                    if (list.threads.isEmpty()) item {
                        EmptyState("mail", "Nothing here yet.", Modifier.fillParentMaxSize(), body = "Messages, pictures, files, alerts and questions from Crundi land here.")
                    }
                    items(list.threads, key = { it.key }) { t ->
                        // A swipe asks first: it deletes the whole thread.
                        val state = rememberSwipeToDismissBoxState(confirmValueChange = { v -> if (v == SwipeToDismissBoxValue.EndToStart) deleting = t; false })
                        SwipeToDismissBox(
                            state,
                            backgroundContent = {
                                Box(Modifier.fillMaxSize().background(CrundiColors.RedDim).padding(horizontal = 20.dp), contentAlignment = Alignment.CenterEnd) {
                                    CrundiIcon("trash", size = 18.dp, tint = CrundiColors.Red, contentDescription = "Delete thread")
                                }
                            },
                            enableDismissFromStartToEnd = false,
                        ) { Box(Modifier.background(CrundiColors.BgPrimary)) { ThreadRow(t) { events.onOpen(t) } } }
                        CrundiDivider()
                    }
                }
            }
        }
    }
    deleting?.let { t ->
        CrundiDialog(
            "Delete this thread?", onDismiss = { deleting = null },
            message = "All ${if (t.count > 0) "${t.count} " else ""}messages in “${t.name}” leave the inbox.",
            confirm = "Delete thread", danger = true, onConfirm = { deleting = null; events.onDelete(t) },
        )
    }
}

/** A round tile with an icon: accent while there is something unread. */
@Composable
private fun Tile(icon: String, unread: Boolean, size: androidx.compose.ui.unit.Dp = 40.dp, description: String? = null) {
    Box(
        Modifier.size(size).clip(CircleShape).background(if (unread) CrundiColors.AccentDim else CrundiColors.BgTertiary)
            .border(Dimens.Hairline, if (unread) CrundiColors.Accent else CrundiColors.Border, CircleShape),
        contentAlignment = Alignment.Center,
    ) { CrundiIcon(icon, size = size * 0.46f, tint = if (unread) CrundiColors.AccentHover else CrundiColors.TextSecondary, contentDescription = description) }
}

@Composable
private fun CountBadge(n: Int) {
    Text(
        if (n > 99) "99+" else n.toString(),
        style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontSize = 11.sp, fontWeight = FontWeight.Bold, color = Color.White, lineHeight = 14.sp),
        modifier = Modifier.clip(RoundedCornerShape(99.dp)).background(CrundiColors.Accent).padding(horizontal = 7.dp, vertical = 1.dp),
    )
}

@Composable
private fun ThreadRow(t: InboxThread, onClick: () -> Unit) {
    val unread = t.unread > 0
    Row(
        Modifier.fillMaxWidth().clickable(onClickLabel = "Open", onClick = onClick).padding(horizontal = Dimens.ScreenPad, vertical = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically,
    ) {
        if (t.kind == "chat" || t.kind == "project") {
            // The project's own icon where it has one (as the drawer shows it), else its initials.
            val shell by LocalAppServices.current.shell.state.collectAsStateWithLifecycle()
            val key = t.project.ifBlank { t.name }
            val p = shell.projects.firstOrNull { it.alias.equals(key, ignoreCase = true) } ?: shell.projects.firstOrNull { it.name.equals(key, ignoreCase = true) }
            val mark = mv.obitech.crundi.ui.chats.projectMark(p?.name ?: key)
            if (t.kind == "project") mv.obitech.crundi.ui.chats.MarkTile(mark, p?.icon, 40.dp)
            else Box(Modifier.size(40.dp), contentAlignment = Alignment.Center) {
                Tile(InboxThread.iconOf(t.kind), unread)
                Box(Modifier.align(Alignment.BottomEnd)) { mv.obitech.crundi.ui.chats.MarkTile(mark, p?.icon, 18.dp) }
            }
        } else Tile(InboxThread.iconOf(t.kind), unread)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(
                    t.name, Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis,
                    style = CrundiType.Body.copy(color = if (unread) CrundiColors.TextPrimary else CrundiColors.TextSecondary, fontWeight = if (unread) FontWeight.SemiBold else FontWeight.Normal),
                )
                Text(ago(t.at), style = CrundiType.Meta.copy(color = if (unread) CrundiColors.AccentHover else CrundiColors.TextMuted), maxLines = 1)
            }
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(t.last?.line.orEmpty(), Modifier.weight(1f), style = CrundiType.Small.copy(color = if (unread) CrundiColors.TextSecondary else CrundiColors.TextMuted), maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (unread) CountBadge(t.unread)
            }
            if (t.subtitle.isNotBlank()) Text(t.subtitle, style = CrundiType.Meta, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

// ─── One thread ───

data class InboxThreadUi(
    val key: String = "",
    /** The thread's own entry (title, chat), when known. */
    val thread: InboxThread? = null,
    /** Its items, newest first (drawn bottom up: the newest at the bottom). */
    val list: InboxList = InboxList(),
    val loadingOlder: Boolean = false,
    val error: String? = null,
    /** The item a notification pointed at: scrolled to and marked. */
    val focus: String? = null,
) {
    val title: String get() = thread?.name ?: when {
        key == "cru" -> "Cru"
        key == "system" -> "Crundi"
        key.startsWith("project:") -> key.removePrefix("project:")
        else -> list.items.firstOrNull { it.chatTitle.isNotBlank() }?.chatTitle ?: "Inbox"
    }
    val subtitle: String? get() = (thread?.subtitle ?: list.items.firstOrNull { it.project.isNotBlank() }?.project)?.takeIf { it.isNotBlank() }
    val chatId: String get() = thread?.chatId?.takeIf { it.isNotEmpty() } ?: key.takeIf { it.startsWith("chat:") }?.removePrefix("chat:").orEmpty()
}

class InboxThreadEvents(
    val onBack: () -> Unit = {},
    val onOlder: () -> Unit = {},
    val onRetry: () -> Unit = {},
    val onOpenChat: () -> Unit = {},
    val onMarkRead: () -> Unit = {},
    val onDeleteThread: () -> Unit = {},
    val onDelete: (InboxItem) -> Unit = {},
    val onLink: (InboxItem) -> Unit = {},
    val onPicture: (InboxMedia) -> Unit = {},
    val onDownload: (InboxMedia) -> Unit = {},
    val onShare: (InboxMedia) -> Unit = {},
)

@Composable
fun InboxThreadRoute(key: String?, open: String? = null) {
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val host = LocalHost.current
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var ui by remember { mutableStateOf(InboxThreadUi(key = key.orEmpty(), focus = open)) }
    var picture by remember { mutableStateOf<InboxMedia?>(null) }

    fun markRead() {
        val k = ui.key.ifEmpty { return }
        if (ui.list.items.none { !it.read } && (ui.thread?.unread ?: 0) == 0) return
        ui = ui.copy(list = ui.list.markAllRead(), thread = ui.thread?.copy(unread = 0))
        scope.launch { runCatching { api.inbox.markThreadRead(k) } }
    }
    suspend fun loadFirst() {
        try {
            // Only the item known (a notification): ask which thread it is in.
            if (ui.key.isEmpty()) {
                val id = ui.focus ?: throw IllegalStateException("No thread to show.")
                val thread = api.inbox.item(id).item?.thread?.takeIf { it.isNotEmpty() } ?: throw IllegalStateException("That message is no longer in the inbox.")
                ui = ui.copy(key = thread)
            }
            val page = api.inbox.list(thread = ui.key)
            ui = ui.copy(list = ui.list.first(page), error = null)
            runCatching { api.inbox.threads().threads.firstOrNull { it.key == ui.key } }.getOrNull()?.let { ui = ui.copy(thread = it) }
            markRead()
        } catch (e: Exception) {
            if (!ui.list.loaded) ui = ui.copy(error = if (e is IllegalStateException) e.message else errorText(e))
        }
    }
    fun older() {
        val before = ui.list.oldest ?: return
        if (ui.loadingOlder || !ui.list.hasMore || ui.key.isEmpty()) return
        ui = ui.copy(loadingOlder = true)
        scope.launch {
            try { val page = api.inbox.list(thread = ui.key, before = before); ui = ui.copy(list = ui.list.older(page), loadingOlder = false); markRead() }
            catch (_: Exception) { ui = ui.copy(loadingOlder = false) }
        }
    }

    LaunchedEffect(Unit) { loadFirst() }
    // In front of the owner: the thread's notification goes, and new ones for it are left out (Host.inboxOnScreen).
    InboxOnScreenEffect(ui.key, host)
    LifecycleResumeEffect(Unit) { if (ui.list.loaded) scope.launch { loadFirst() }; onPauseOrDispose {} }
    // Live: this thread's items come in (and are read, since the owner is looking at them).
    LaunchedEffect(Unit) {
        api.events.changes(InboxEvent.NAME).collect { e ->
            val ev = InboxEvent.parse(e.data) ?: return@collect
            if (ui.key.isEmpty() || !ev.touches(ui.key)) return@collect
            if (ev.item != null) { ui = ui.copy(list = ui.list.apply(ev, ui.key)); markRead() }
            else loadFirst()
        }
    }
    // From a notification: page back until its item is loaded (at most a few pages).
    LaunchedEffect(ui.focus, ui.list) {
        val id = ui.focus ?: return@LaunchedEffect
        if (ui.list.loaded && ui.list.byId(id) == null && ui.list.hasMore && ui.list.items.size < 5 * InboxApi.PAGE) older()
    }

    fun download(m: InboxMedia, share: Boolean) {
        scope.launch {
            try {
                val saved = mv.obitech.crundi.Downloads.save(ctx, m.name.ifEmpty { m.url.substringAfterLast('/').substringBefore('?').ifEmpty { "file" } }, m.mime.ifEmpty { null }, m.size) { sink ->
                    api.transport.download(ApiRequest("GET", m.url.substringBefore('?'), mv.obitech.crundi.ui.chats.queryOf(m.url)), sink)
                }
                if (share && saved.uri.scheme == "content") {
                    val send = Intent(Intent.ACTION_SEND).setType(m.mime.ifEmpty { "application/octet-stream" }).putExtra(Intent.EXTRA_STREAM, saved.uri).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                    ctx.startActivity(Intent.createChooser(send, "Share ${saved.name}").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                } else Toast.makeText(ctx.applicationContext, "Saved ${saved.name} to ${saved.where}", Toast.LENGTH_SHORT).show()
            } catch (e: Exception) { toast(ctx, errorText(e)) }
        }
    }

    InboxThreadScreen(ui, InboxThreadEvents(
        onBack = { if (host.isBubble) host.close() else nav.back() },
        onOlder = { older() },
        onRetry = { ui = ui.copy(error = null); scope.launch { loadFirst() } },
        onOpenChat = { ui.chatId.takeIf { it.isNotEmpty() }?.let { id -> if (!host.openChat(id, ui.title, ui.thread?.project ?: ui.subtitle.orEmpty())) nav.chat(id) } },
        onMarkRead = { markRead() },
        onDeleteThread = {
            val k = ui.key
            scope.launch {
                try { api.inbox.deleteThread(k); if (host.isBubble) host.close() else nav.back() } catch (e: Exception) { toast(ctx, errorText(e)) }
            }
        },
        onDelete = { item ->
            ui = ui.copy(list = ui.list.delete(listOf(item.id)))
            scope.launch { try { api.inbox.delete(listOf(item.id)) } catch (e: Exception) { toast(ctx, errorText(e)); loadFirst() } }
        },
        onLink = { item ->
            val route = DeepLinks.route(item.link)
            when {
                route != null && !host.isBubble -> nav.to(route)
                item.link.startsWith("https://") || item.link.startsWith("http://") -> host.openExternal(item.link)
                else -> host.openInMain(item.link)
            }
        },
        onPicture = { picture = it },
        onDownload = { download(it, share = false) },
        onShare = { download(it, share = true) },
    ))
    picture?.let { m ->
        mv.obitech.crundi.ui.chats.ImageViewer(
            mv.obitech.crundi.api.model.ChatMedia(kind = "image", path = m.url, filename = m.name, size = m.size),
            onSave = { picture = null; download(m, share = false) },
            onClose = { picture = null },
        )
    }
}

/**
 * One thread as a conversation, stateless: oldest at the top, newest at the
 * bottom, opened at the bottom (or at the item a notification named).
 */
@Composable
fun InboxThreadScreen(ui: InboxThreadUi, events: InboxThreadEvents = InboxThreadEvents()) {
    var confirmDelete by remember { mutableStateOf(false) }
    val items = ui.list.items
    // reverseLayout: index 0 (the newest) sits at the bottom, and the list opens there.
    val state = rememberLazyListState()
    val focusIndex = ui.focus?.let { f -> items.indexOfFirst { it.id == f } } ?: -1
    LaunchedEffect(focusIndex) { if (focusIndex >= 0) state.scrollToItem(focusIndex) }
    Column(Modifier.fillMaxSize()) {
        TopBar(ui.title, subtitle = ui.subtitle, onBack = events.onBack) {
            if (ui.chatId.isNotEmpty()) CrundiIconButton("message-square", events.onOpenChat, contentDescription = "Open chat")
            CrundiIconButton("check", events.onMarkRead, contentDescription = "Mark read")
            CrundiIconButton("trash", { confirmDelete = true }, contentDescription = "Delete thread")
        }
        when {
            !ui.list.loaded && ui.error != null -> ErrorState(ui.error, onRetry = events.onRetry)
            !ui.list.loaded -> LoadingState()
            items.isEmpty() -> EmptyState("mail", "Nothing here yet.")
            else -> LazyColumn(
                Modifier.fillMaxWidth().weight(1f).navigationBarsPadding(), state = state, reverseLayout = true,
                contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = Dimens.ScreenPad, vertical = 12.dp),
                // Few messages sit at the bottom, by the newest, as in a conversation.
                verticalArrangement = Arrangement.spacedBy(10.dp, Alignment.Bottom),
            ) {
                items(items, key = { it.id }) { item -> Message(item, item.id == ui.focus, events) }
                if (ui.list.hasMore) item(key = "older") {
                    LaunchedEffect(ui.list.oldest) { events.onOlder() }
                    Box(Modifier.fillMaxWidth().padding(8.dp), contentAlignment = Alignment.Center) {
                        Text(if (ui.loadingOlder) "Loading older…" else "", style = CrundiType.Meta)
                    }
                }
            }
        }
    }
    if (confirmDelete) CrundiDialog(
        "Delete this thread?", onDismiss = { confirmDelete = false },
        message = "Every message in “${ui.title}” leaves the inbox.",
        confirm = "Delete thread", danger = true, onConfirm = { confirmDelete = false; events.onDeleteThread() },
    )
}

private fun stamp(at: Long): String {
    if (at <= 0) return ""
    val d = java.util.Date(at)
    val day = java.util.Calendar.getInstance().apply { timeInMillis = at }
    val now = java.util.Calendar.getInstance()
    val today = day.get(java.util.Calendar.YEAR) == now.get(java.util.Calendar.YEAR) && day.get(java.util.Calendar.DAY_OF_YEAR) == now.get(java.util.Calendar.DAY_OF_YEAR)
    val time = java.text.DateFormat.getTimeInstance(java.text.DateFormat.SHORT).format(d)
    return if (today) time else java.text.DateFormat.getDateInstance(java.text.DateFormat.MEDIUM).format(d) + ", " + time
}

/** One message of a thread: a card with its kind, title and time, its text, pictures and files. Long-press for Delete. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun Message(item: InboxItem, focused: Boolean, events: InboxThreadEvents) {
    var menu by remember { mutableStateOf(false) }
    Box {
        Column(
            Modifier.fillMaxWidth().widthIn(max = 560.dp).clip(Dimens.Shape)
                .background(if (item.read) CrundiColors.BgSecondary else CrundiColors.AccentDim)
                .border(Dimens.Hairline, if (focused) CrundiColors.Accent else CrundiColors.Border, Dimens.Shape)
                .combinedClickable(onClick = {}, onLongClick = { menu = true }, onLongClickLabel = "Delete")
                .padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                CrundiIcon(InboxItem.iconOf(item.kind), size = 15.dp, tint = if (item.read) CrundiColors.TextMuted else CrundiColors.AccentHover, contentDescription = InboxItem.kindLabel(item.kind))
                Text(
                    item.title.ifBlank { InboxItem.kindLabel(item.kind) }, Modifier.weight(1f), maxLines = 2, overflow = TextOverflow.Ellipsis,
                    style = CrundiType.Body.copy(fontWeight = FontWeight.SemiBold, color = CrundiColors.TextPrimary),
                )
                Text(stamp(item.at), style = CrundiType.Meta, maxLines = 1)
                CrundiIconButton("more", { menu = true }, contentDescription = "Message menu", size = 16.dp, touch = 28.dp)
            }
            if (item.text.isNotBlank()) MarkdownText(item.text, compact = true)
            for (m in item.media) if (m.isImage) Picture(m, events) else FileCard(m, events)
            if (item.link.isNotEmpty()) CrundiButton("Open", { events.onLink(item) }, icon = "external-link", kind = ButtonKind.Ghost)
        }
        DropdownMenu(menu, { menu = false }, containerColor = CrundiColors.BgSecondary) {
            DropdownMenuItem(
                text = { Text("Delete", style = CrundiType.Body.copy(color = CrundiColors.Red)) },
                leadingIcon = { CrundiIcon("trash", size = 16.dp, tint = CrundiColors.Red) },
                onClick = { menu = false; events.onDelete(item) },
            )
        }
    }
}

@Composable
private fun Picture(m: InboxMedia, events: InboxThreadEvents) {
    val img = rememberServerImage(m.url, 1280)
    Box(
        Modifier.fillMaxWidth().heightIn(min = 120.dp).clip(Dimens.ShapeSm).background(CrundiColors.BgTertiary)
            .clickable(onClickLabel = "Enlarge") { events.onPicture(m) },
        contentAlignment = Alignment.Center,
    ) {
        if (img != null) Image(img, m.name, Modifier.fillMaxWidth(), contentScale = ContentScale.FillWidth)
        else Box(Modifier.fillMaxWidth().heightIn(min = 180.dp), contentAlignment = Alignment.Center) { CrundiIcon("image", size = 28.dp, tint = CrundiColors.TextMuted, contentDescription = m.name) }
    }
}

@Composable
private fun FileCard(m: InboxMedia, events: InboxThreadEvents) {
    Column(
        Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgTertiary).border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm).padding(10.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
            CrundiIcon("file", size = 20.dp, tint = CrundiColors.TextSecondary)
            Column(Modifier.weight(1f)) {
                Text(m.name.ifEmpty { "File" }, style = CrundiType.Body, maxLines = 2, overflow = TextOverflow.Ellipsis)
                val meta = listOf(if (m.size > 0) bytes(m.size) else "", m.mime).filter { it.isNotEmpty() }.joinToString(" · ")
                if (meta.isNotEmpty()) Text(meta, style = CrundiType.Meta, maxLines = 1)
            }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiButton("Download", { events.onDownload(m) }, icon = "download")
            CrundiButton("Share", { events.onShare(m) }, icon = "share")
        }
    }
}
