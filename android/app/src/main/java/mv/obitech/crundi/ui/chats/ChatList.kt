package mv.obitech.crundi.ui.chats

import mv.obitech.crundi.ui.components.liveErrorText

import mv.obitech.crundi.ui.components.ErrorState

import mv.obitech.crundi.ui.components.errorText

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import mv.obitech.crundi.api.model.NewChat
import mv.obitech.crundi.api.model.Pane
import mv.obitech.crundi.api.model.ResumableSession
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.Banner
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiChip
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.DialogCard
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.components.ListRow
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.PanelHeader
import mv.obitech.crundi.ui.components.SectionHeader
import mv.obitech.crundi.ui.components.StateDot
import mv.obitech.crundi.ui.components.StatusBadge
import mv.obitech.crundi.ui.components.DotState
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.shell.LocalHost
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/** What the Chats front page shows. */
data class ChatsListState(
    /** The current project's alias; null asks the person to pick one. */
    val project: String?,
    /** False until the first state push. */
    val loaded: Boolean = true,
    val chats: List<Pane> = emptyList(),
    val parked: List<Pane> = emptyList(),
    val error: String? = null,
    /** Why nothing has loaded (the server cannot be reached); shown with Try again instead of the spinner. */
    val loadError: String? = null,
)

/** What a chat row does. */
interface ChatsListActions {
    fun open(chat: Pane) {}
    fun newChat() {}
    fun menu(chat: Pane) {}
    fun resume(parked: Pane) {}
    fun discard(parked: Pane) {}
    fun retry() {}
}

/** One row's subtitle: what it is doing, what it is waiting on, its model. */
fun chatSubtitle(p: Pane): String {
    val bits = mutableListOf<String>()
    val waiting = p.pendingEntries.count { it.isPending }
    when {
        waiting > 0 -> bits += if (waiting == 1) "1 waiting on you" else "$waiting waiting on you"
        p.status == "exited" -> bits += "ended"
        else -> bits += p.state.words()
    }
    if (p.planStatus == "proposed") bits += "plan to review"
    if (p.factory != null && p.factory.role.isNotEmpty()) bits += "factory " + p.factory.role.lowercase()
    if (p.collaborator != null && p.collaborator.name.isNotEmpty()) bits += p.collaborator.name
    shortModel(p.model).takeIf { it.isNotEmpty() }?.let { bits += it }
    if (p.skipPermissions) bits += "skip permissions"
    return bits.joinToString(" · ")
}

/** The Chats tab: the current project's chats, then the ones put away. */
@Composable
fun ChatsScreen(state: ChatsListState, actions: ChatsListActions, modifier: Modifier = Modifier) {
    Column(modifier.fillMaxSize().background(CrundiColors.BgPrimary)) {
        if (state.project == null) {
            EmptyState("message-square", "Pick a project", body = "Chats belong to a project. Open the menu and choose one.")
            return@Column
        }
        val running = state.chats.count { it.state == mv.obitech.crundi.api.model.AgentState.Working }
        PanelHeader(
            "Chats",
            meta = if (state.chats.isEmpty()) state.project else "${state.chats.size} open" + (if (running > 0) " · $running working" else "") + " · " + state.project,
        ) {
            CrundiButton("New chat", actions::newChat, kind = ButtonKind.Primary, icon = "plus")
        }
        if (state.error != null) Banner(state.error, Modifier.fillMaxWidth().padding(Dimens.ScreenPad), kind = BadgeKind.Bad, icon = "alert-triangle")
        when {
            !state.loaded && state.loadError != null -> ErrorState(state.loadError, onRetry = actions::retry)
            !state.loaded -> LoadingState(text = "Loading chats")
            state.chats.isEmpty() && state.parked.isEmpty() -> EmptyState(
                "message-square", "No chats in ${state.project}",
                body = "Start a chat to work with Claude here. It keeps running on the server when you leave.",
                action = "New chat", onAction = actions::newChat,
            )
            else -> LazyColumn(Modifier.fillMaxSize().padding(horizontal = 8.dp), contentPadding = androidx.compose.foundation.layout.PaddingValues(vertical = 8.dp)) {
                items(state.chats, key = { it.id }) { c ->
                    val waiting = c.pendingEntries.count { it.isPending }
                    ListRow(
                        title = c.label.ifEmpty { "Chat" },
                        subtitle = chatSubtitle(c),
                        leading = { StateDot(if (c.status == "exited") DotState.Idle else c.state.dot()) },
                        trailing = {
                            if (waiting > 0) StatusBadge("$waiting", BadgeKind.Accent)
                            CrundiIconButton("more", { actions.menu(c) }, contentDescription = "Chat actions", touch = 40.dp)
                        },
                        onClick = { actions.open(c) },
                    )
                }
                if (state.parked.isNotEmpty()) {
                    item { SectionHeader("Put away", trailing = { Text("${state.parked.size}", style = CrundiType.Meta) }) }
                    items(state.parked, key = { it.id }) { p ->
                        ListRow(
                            title = p.label.ifEmpty { "Chat" },
                            subtitle = listOfNotNull(
                                ago(p.parkedAt).takeIf { it.isNotEmpty() }?.let { "put away $it" },
                                p.reason.takeIf { it.isNotEmpty() },
                                if (p.tokens > 0) tokens(p.tokens) + " tokens" else null,
                            ).joinToString(" · "),
                            icon = if (p.paneKind == "terminal") "terminal" else "archive",
                            trailing = {
                                CrundiButton("Resume", { actions.resume(p) }, kind = ButtonKind.Ghost)
                                CrundiIconButton("trash", { actions.discard(p) }, contentDescription = "Discard", touch = 40.dp)
                            },
                        )
                    }
                }
            }
        }
    }
}

// ─── Starting a chat ───

/** The choices for a new chat, as the desktop's launcher and its conversation chooser. */
data class NewChatForm(
    val skipPermissions: Boolean = false,
    val model: String = "",
    val effort: String = "",
    /** new, continue, resume */
    val session: String = "new",
    val resumeId: String = "",
)

data class NewChatState(
    val project: String,
    val form: NewChatForm = NewChatForm(),
    val resumable: List<ResumableSession> = emptyList(),
    val loading: Boolean = false,
    /** The conversation "continue" would pick up is big or old. */
    val heavy: Boolean = false,
    val starting: Boolean = false,
    val error: String? = null,
)

val MODELS = listOf("" to "Default", "opus" to "Opus", "sonnet" to "Sonnet", "haiku" to "Haiku")
val EFFORTS = listOf("" to "Default", "low" to "Low", "medium" to "Medium", "high" to "High", "xhigh" to "Extra high", "max" to "Max")

/** The body of the "New chat" dialog. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun NewChatCard(state: NewChatState, onChange: (NewChatForm) -> Unit, onStart: () -> Unit, onDismiss: () -> Unit, modifier: Modifier = Modifier) {
    val f = state.form
    DialogCard(
        "New chat", modifier,
        message = "In ${state.project}. Claude runs on the server and keeps going when you leave.",
        confirm = if (state.starting) "Starting" else "Start chat", onConfirm = { if (!state.starting) onStart() },
        onDismiss = onDismiss,
    ) {
        Column(Modifier.heightIn(max = 460.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text("PERMISSIONS", style = CrundiType.Section)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                CrundiChip("Ask permissions", !f.skipPermissions, { onChange(f.copy(skipPermissions = false)) }, icon = "shield")
                CrundiChip("Skip permissions", f.skipPermissions, { onChange(f.copy(skipPermissions = true)) }, icon = "shield-alert")
            }
            if (f.skipPermissions) Text("Claude will not ask before editing files or running commands.", style = CrundiType.Small.copy(color = CrundiColors.Yellow))
            Text("MODEL", style = CrundiType.Section)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                for ((k, l) in MODELS) CrundiChip(l, f.model == k, { onChange(f.copy(model = k)) })
            }
            Text("EFFORT", style = CrundiType.Section)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                for ((k, l) in EFFORTS) CrundiChip(l, f.effort == k, { onChange(f.copy(effort = k)) })
            }
            Text("CONVERSATION", style = CrundiType.Section)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                CrundiChip("New", f.session == "new", { onChange(f.copy(session = "new", resumeId = "")) }, icon = "plus")
                CrundiChip("Continue the last", f.session == "continue", { onChange(f.copy(session = "continue", resumeId = "")) }, icon = "play")
                CrundiChip("Resume earlier", f.session == "resume", { onChange(f.copy(session = "resume")) }, icon = "rotate-ccw")
            }
            if (f.session == "continue" && state.heavy) {
                Banner("The last conversation is large. Continuing it uses more of your limits; a new one starts fresh.", kind = BadgeKind.Warn, icon = "alert-triangle")
            }
            if (f.session == "resume") {
                when {
                    state.loading -> Text("Loading conversations", style = CrundiType.Meta)
                    state.resumable.isEmpty() -> Text("No earlier conversations in this project.", style = CrundiType.Small)
                    else -> Column {
                        for (r in state.resumable.take(12)) ListRow(
                            title = r.title.ifEmpty { r.id.take(8) },
                            subtitle = listOfNotNull(
                                r.ageHours.let { h -> if (h < 1) "under an hour ago" else if (h < 48) "${h.toInt()}h ago" else "${(h / 24).toInt()}d ago" },
                                if (r.tokens > 0) tokens(r.tokens) + " tokens" else bytes(r.sizeBytes),
                                if (r.inUse) "open in another chat (opens a copy)" else null,
                            ).joinToString(" · "),
                            active = f.resumeId == r.id,
                            icon = "message-square",
                            onClick = { onChange(f.copy(resumeId = r.id)) },
                        )
                    }
                }
            }
            if (state.error != null) Banner(state.error, kind = BadgeKind.Bad, icon = "alert-triangle")
        }
    }
}

/** The body of the rename dialog (chats and terminals). */
@Composable
fun RenameCard(what: String, value: String, onValue: (String) -> Unit, onSave: () -> Unit, onDismiss: () -> Unit, modifier: Modifier = Modifier) {
    DialogCard("Rename $what", modifier, confirm = "Rename", onConfirm = onSave, onDismiss = onDismiss) {
        CrundiTextField(value, { onValue(it.take(80)) }, placeholder = "Name", modifier = Modifier.fillMaxWidth())
        Text("${value.length}/80", style = CrundiType.Meta, modifier = Modifier.align(Alignment.End))
    }
}

// ─── The route ───

@Composable
fun ChatsRoute() {
    val services = LocalAppServices.current
    val nav = LocalNavigator.current
    val host = LocalHost.current
    val api = services.api
    val shell by services.shell.state.collectAsState()
    val server by api.events.state.collectAsState(initial = api.events.lastState)
    val live by api.events.status.collectAsState()
    val project = shell.currentProject
    val scope = rememberCoroutineScope()
    var error by remember { mutableStateOf<String?>(null) }
    var newChat by remember { mutableStateOf<NewChatState?>(null) }
    var menuFor by remember { mutableStateOf<Pane?>(null) }
    var renaming by remember { mutableStateOf<Pair<Pane, String>?>(null) }
    var closing by remember { mutableStateOf<Pane?>(null) }

    fun act(block: suspend () -> Unit) = scope.launch {
        error = null
        try { block() } catch (e: Exception) { error = errorText(e) }
    }

    val state = ChatsListState(
        project = project,
        loaded = server != null,
        chats = server?.chats(project).orEmpty(),
        parked = server?.parked(project).orEmpty().filter { it.paneKind != "terminal" },
        error = error,
        loadError = if (server == null) liveErrorText(live) else null,
    )
    ChatsScreen(state, object : ChatsListActions {
        override fun retry() = api.events.reconnect()
        override fun open(chat: Pane) = nav.chat(chat.id)
        override fun newChat() {
            val p = project ?: return
            newChat = NewChatState(p, loading = true)
            scope.launch {
                val pre = runCatching { api.chats.preflight(p) }.getOrNull()
                val list = runCatching { api.chats.resumable(p) }.getOrDefault(emptyList())
                newChat = newChat?.copy(loading = false, heavy = pre?.heavy == true, resumable = list)
            }
        }
        override fun menu(chat: Pane) { menuFor = chat }
        override fun resume(parked: Pane) { act { val id = api.terminals.resume(parked.id); nav.chat(id) } }
        override fun discard(parked: Pane) { act { api.terminals.discard(parked.id) } }
    })

    newChat?.let { nc ->
        mv.obitech.crundi.ui.chats.SheetDialog({ newChat = null }) {
            NewChatCard(nc, { newChat = nc.copy(form = it) }, onStart = {
                val f = nc.form
                if (f.session == "resume" && f.resumeId.isEmpty()) { newChat = nc.copy(error = "Pick the conversation to resume."); return@NewChatCard }
                newChat = nc.copy(starting = true, error = null)
                scope.launch {
                    try {
                        val created = api.chats.create(NewChat(
                            project = nc.project,
                            title = if (f.skipPermissions) "Chat (skip perms)" else null,
                            model = f.model.ifEmpty { null }, effort = f.effort.ifEmpty { null },
                            skipPermissions = f.skipPermissions.takeIf { it },
                            sessionMode = f.session.takeIf { it != "new" } ?: "new",
                            resumeId = f.resumeId.ifEmpty { null },
                        ))
                        newChat = null
                        nav.chat(created.id)
                    } catch (e: Exception) {
                        newChat = newChat?.copy(starting = false, error = "The chat did not start: " + errorText(e))
                    }
                }
            }, onDismiss = { newChat = null })
        }
    }
    menuFor?.let { c ->
        SheetDialog({ menuFor = null }) {
            ActionSheetCard(c.label.ifEmpty { "Chat" }, chatMenu(c, host.isBubble,
                rename = { renaming = c to c.label },
                bubble = { host.openBubble(c.id) },
                park = { act { api.terminals.park(c.id) } },
                close = { closing = c },
            ), onDismiss = { menuFor = null })
        }
    }
    renaming?.let { (c, v) ->
        SheetDialog({ renaming = null }) {
            RenameCard("chat", v, { renaming = c to it }, onSave = {
                renaming = null
                if (v.isNotBlank()) act { api.chats.rename(c.id, v.trim()) }
            }, onDismiss = { renaming = null })
        }
    }
    closing?.let { c ->
        CrundiDialog(
            "Close chat", onDismiss = { closing = null },
            message = "Its Claude ends now. The conversation stays on the server and can be resumed from a new chat.",
            confirm = "Close chat", danger = true,
            onConfirm = { closing = null; act { api.chats.close(c.id) } },
        )
    }
    LaunchedEffect(project) { error = null }
}

/** The actions a chat offers from its row or its top bar. */
fun chatMenu(c: Pane?, inBubble: Boolean, rename: () -> Unit, bubble: () -> Unit, park: () -> Unit, close: () -> Unit, openInMain: (() -> Unit)? = null, stop: (() -> Unit)? = null): List<SheetAction> = buildList {
    add(SheetAction("Rename", "pencil", onClick = rename))
    if (inBubble) openInMain?.let { add(SheetAction("Open in Crundi", "maximize", onClick = it)) }
    else add(SheetAction("Open as bubble", "layers", detail = "Keep it floating over other apps", onClick = bubble))
    stop?.let { add(SheetAction("Stop Claude", "stop", detail = "Interrupt the turn it is on", onClick = it)) }
    add(SheetAction("Put away", "archive", detail = "End its Claude now and keep it to resume later", onClick = park))
    if (c?.factory == null || c.factory.role.isEmpty()) add(SheetAction("Close chat", "x", danger = true, onClick = close))
}
