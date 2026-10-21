package mv.obitech.crundi.ui.chats

import mv.obitech.crundi.ui.components.errorText

import android.content.ContentValues
import android.content.Context
import android.net.Uri
import android.os.Build
import android.provider.MediaStore
import android.provider.OpenableColumns
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
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
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import mv.obitech.crundi.Pickers
import mv.obitech.crundi.api.ApiRequest
import mv.obitech.crundi.api.model.AgentState
import mv.obitech.crundi.api.model.ChatEntry
import mv.obitech.crundi.api.model.ChatMedia
import mv.obitech.crundi.api.model.ChatState
import mv.obitech.crundi.api.model.ChatSuggestion
import mv.obitech.crundi.api.model.Pane
import mv.obitech.crundi.api.model.PendingInjection
import mv.obitech.crundi.api.rememberServerImage
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.Banner
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.DialogCard
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.TopBar
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.shell.ChatOnScreenEffect
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.shell.LocalHost
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/** A sheet over the chat. */
sealed interface ChatSheet {
    data object Menu : ChatSheet
    data object Mode : ChatSheet
    data object Model : ChatSheet
    data class Rename(val value: String) : ChatSheet
    data object Close : ChatSheet
    data object Agents : ChatSheet
    data class Agent(val toolUseId: String) : ChatSheet
    data class Picture(val media: ChatMedia) : ChatSheet
}

/** Everything the chat screen draws. */
data class ChatUi(
    val chat: ChatState,
    /** The list's row for it (title, factory role), when known. */
    val pane: Pane? = null,
    val draft: String = "",
    val uploads: List<UploadUi> = emptyList(),
    val stopArmed: Boolean = false,
    /** Mid-turn sends not yet echoed by the server. */
    val localPending: List<PendingInjection> = emptyList(),
    /** Folded things the person opened (tool ids; "!id" closes a live thought). */
    val open: Set<String> = emptySet(),
    /** Cards whose answer is on its way. */
    val busyEntries: Set<String> = emptySet(),
    val factoryDiffs: Map<String, String> = emptyMap(),
    val sheet: ChatSheet? = null,
    val notice: String? = null,
    val error: String? = null,
    val isBubble: Boolean = false,
) {
    val title get() = pane?.label?.takeIf { it.isNotEmpty() } ?: chat.title.ifEmpty { "Chat" }
    val pending get() = chat.pendingInjections + localPending.filter { l -> chat.pendingInjections.none { it.uuid == l.uuid && l.uuid.isNotEmpty() } }
    val busy get() = chat.state == AgentState.Working || pending.isNotEmpty()
}

/** Everything the chat screen can be asked to do. */
interface ChatActions : EntryActions, ComposerActions {
    fun back() {}
    fun sheet(s: ChatSheet?) {}
    fun setMode(mode: String) {}
    fun setModel(model: String) {}
    fun rename(title: String) {}
    fun close() {}
    fun park() {}
    fun bubble() {}
    fun openInMain() {}
    fun dismissAgents() {}
}

private fun activity(c: ChatState): String? {
    if (c.state != AgentState.Working) return null
    val last = c.visible.lastOrNull() ?: return "Working"
    if (last.streaming || (last.kind == "tool" && last.status == "running")) return null
    return when (last.kind) { "user" -> "Thinking"; "tool" -> "Working"; else -> "Working" }
}

/** One chat: the conversation, live, and the composer. The same screen in the app and in a bubble. */
@Composable
fun ChatScreen(ui: ChatUi, actions: ChatActions, modifier: Modifier = Modifier) {
    val c = ui.chat
    Box(modifier.fillMaxSize().background(CrundiColors.BgPrimary)) {
        Column(Modifier.fillMaxSize().imePadding()) {
            TopBar(
                ui.title,
                subtitle = listOfNotNull(
                    if (c.exited) "ended" else c.state.words(),
                    shortModel(c.model).takeIf { it.isNotEmpty() },
                    c.factory?.role?.takeIf { it.isNotEmpty() }?.let { "factory " + it.lowercase() },
                ).joinToString(" · "),
                onBack = actions::back,
                actions = {
                    if (ui.isBubble) CrundiIconButton("maximize", actions::openInMain, contentDescription = "Open in Crundi")
                    else CrundiIconButton("layers", actions::bubble, contentDescription = "Open as bubble")
                    CrundiIconButton("more", { actions.sheet(ChatSheet.Menu) }, contentDescription = "Chat actions")
                },
            )
            if (!c.loaded) { LoadingState(Modifier.weight(1f), text = "Opening the chat"); return@Column }
            val entries = c.visible
            val act = activity(c)
            val list = rememberLazyListState(initialFirstVisibleItemIndex = (entries.size + 1).coerceAtLeast(0))
            val lastLen = entries.lastOrNull()?.let { it.text.length + it.status.length } ?: 0
            LaunchedEffect(entries.size, lastLen, act) {
                val info = list.layoutInfo
                val nearEnd = info.visibleItemsInfo.lastOrNull()?.index?.let { it >= info.totalItemsCount - 3 } ?: true
                if (nearEnd && info.totalItemsCount > 0) list.animateScrollToItem(info.totalItemsCount - 1)
            }
            LazyColumn(
                Modifier.weight(1f).fillMaxWidth(), state = list,
                contentPadding = PaddingValues(start = 12.dp, end = 12.dp, top = 12.dp, bottom = 8.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                items(entries, key = { it.id.ifEmpty { it.seq.toString() } }) { e ->
                    ChatEntryView(e, c.id, ui.open, actions, c.agents, ui.busyEntries, ui.factoryDiffs)
                }
                if (act != null) item(key = "~activity") { ActivityRow(act) }
                if (c.exited) item(key = "~ended") {
                    Banner("This chat's Claude has ended" + (c.exitCode?.takeIf { it != 0 }?.let { " (code $it)" } ?: "") + ". Start a new chat or resume this conversation from Chats.", kind = BadgeKind.Neutral, icon = "info")
                }
            }
            if (ui.error != null) Banner(ui.error, Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 4.dp), kind = BadgeKind.Bad, icon = "alert-triangle")
            if (ui.notice != null) Banner(ui.notice, Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 4.dp), kind = BadgeKind.Good, icon = "check")
            val agents = c.agents.values.filter { !it.dismissed }
            Composer(
                ui.draft, actions,
                busy = ui.busy, ended = c.exited, stopArmed = ui.stopArmed,
                mode = c.permissionMode.ifEmpty { if (c.skipPermissions) "bypassPermissions" else "default" },
                model = c.model, stateWords = if (c.exited) "ended" else c.state.words(), cost = c.totalCostUsd,
                commands = c.slashCommands, pending = ui.pending, suggestion = c.suggestion, uploads = ui.uploads,
                agentsRunning = agents.count { it.running }, agentsTotal = agents.size,
            )
        }
        ui.sheet?.let { s -> SheetOverlay({ actions.sheet(null) }) { ChatSheetContent(s, ui, actions) } }
    }
}

/** A sheet drawn over the screen (in the window, so screenshots see it). */
@Composable
fun SheetOverlay(onDismiss: () -> Unit, content: @Composable () -> Unit) {
    Box(
        Modifier.fillMaxSize().background(CrundiColors.Scrim).clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, onClick = onDismiss).padding(16.dp),
        contentAlignment = Alignment.Center,
    ) {
        Box(Modifier.clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {}) { content() }
    }
}

@Composable
private fun ChatSheetContent(s: ChatSheet, ui: ChatUi, a: ChatActions) {
    val close = { a.sheet(null) }
    when (s) {
        ChatSheet.Menu -> ActionSheetCard(ui.title, chatMenu(
            ui.pane, ui.isBubble,
            rename = { a.sheet(ChatSheet.Rename(ui.title)) },
            bubble = a::bubble, park = a::park,
            close = { a.sheet(ChatSheet.Close) },
            openInMain = a::openInMain,
            stop = if (ui.busy) ({ a.stop(); a.stop() }) else null,
        ), onDismiss = close)
        ChatSheet.Mode -> SheetCard("Permission mode", close) {
            val cur = ui.chat.permissionMode.ifEmpty { "default" }
            val modes = if (cur == "bypassPermissions") PERM_MODES + BYPASS_MODE else PERM_MODES
            for (m in modes) OptionRow(m.icon, m.name, m.desc, m.color, on = m.key == cur, enabled = m.key != "bypassPermissions") { a.setMode(m.key); close() }
        }
        ChatSheet.Model -> SheetCard("Model", close) {
            val cur = ui.chat.model
            for ((k, l) in MODELS.drop(1)) OptionRow("zap", l, when (k) {
                "opus" -> "The most capable, for hard and long work."
                "sonnet" -> "Fast and strong, for most work."
                else -> "The quickest, for small things."
            }, CrundiColors.AccentHover, on = cur.contains(k)) { a.setModel(k); close() }
            if (cur.isNotEmpty()) Text("Now: $cur", style = CrundiType.Meta, modifier = Modifier.padding(10.dp))
        }
        is ChatSheet.Rename -> RenameCard("chat", s.value, { a.sheet(ChatSheet.Rename(it)) }, onSave = { a.rename(s.value); close() }, onDismiss = close)
        ChatSheet.Close -> DialogCard(
            "Close chat", message = "Its Claude ends now. The conversation stays on the server and can be resumed from a new chat.",
            confirm = "Close chat", danger = true, onConfirm = { close(); a.close() }, onDismiss = close,
        )
        ChatSheet.Agents -> SheetCard("Agents", close) {
            val agents = ui.chat.agents.values.filter { !it.dismissed }
            if (agents.isEmpty()) Text("No agents in this chat.", style = CrundiType.Small, modifier = Modifier.padding(10.dp))
            Column(Modifier.heightIn(max = 420.dp)) {
                for (ag in agents) Row(
                    Modifier.fillMaxWidth().clickable { a.sheet(ChatSheet.Agent(ag.toolUseId)) }.padding(horizontal = 8.dp, vertical = 9.dp),
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(9.dp),
                ) {
                    if (ag.running) Spinner(10.dp) else Dot(if (ag.status == "failed") CrundiColors.Red else CrundiColors.Green)
                    Column(Modifier.weight(1f)) {
                        Text(ag.description.ifEmpty { ag.subagentType.ifEmpty { "Agent" } }, style = CrundiType.Body.copy(color = if (ag.running) CrundiColors.TextPrimary else CrundiColors.TextSecondary), maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Text(listOfNotNull(ag.subagentType.takeIf { it.isNotEmpty() }, ag.step.ifEmpty { ag.status }.takeIf { it.isNotEmpty() }, "${ag.count} steps").joinToString(" · "), style = CrundiType.Meta, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                    CrundiIcon("chevron-right", size = 14.dp, tint = CrundiColors.TextMuted)
                }
            }
            if (agents.any { !it.running }) Row(Modifier.fillMaxWidth().padding(6.dp), horizontalArrangement = Arrangement.End) {
                mv.obitech.crundi.ui.components.CrundiButton("Dismiss finished", { a.dismissAgents(); close() }, kind = mv.obitech.crundi.ui.components.ButtonKind.Ghost)
            }
        }
        is ChatSheet.Agent -> {
            val ag = ui.chat.agents[s.toolUseId]
            SheetCard(ag?.description?.ifEmpty { null } ?: ag?.subagentType ?: "Agent", close) {
                if (ag == null) { Text("This agent has gone.", style = CrundiType.Small, modifier = Modifier.padding(10.dp)); return@SheetCard }
                Text(listOfNotNull(ag.subagentType.ifEmpty { null }, ag.status.ifEmpty { null }, ag.step.ifEmpty { null }).joinToString(" · "), style = CrundiType.Meta, modifier = Modifier.padding(horizontal = 8.dp))
                LazyColumn(Modifier.heightIn(max = 520.dp).padding(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    if (ag.prompt.isNotEmpty()) item { ChatEntryView(ChatEntry("prompt", "user", text = truncate(ag.prompt, 1200)), ui.chat.id, ui.open, a) }
                    items(ag.messages, key = { "ag-" + it.id }) { e -> ChatEntryView(e, ui.chat.id, ui.open, a) }
                    if (ag.summary.isNotEmpty()) item { ChatEntryView(ChatEntry("summary", "assistant-text", text = ag.summary), ui.chat.id, ui.open, a) }
                }
            }
        }
        is ChatSheet.Picture -> Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
            val img = rememberServerImage(s.media.path, 2048)
            if (img != null) Image(img, contentDescription = s.media.caption.ifEmpty { "Image" }, contentScale = ContentScale.Fit, modifier = Modifier.fillMaxWidth())
            else CrundiIcon("image", size = 40.dp, tint = CrundiColors.TextMuted)
            if (s.media.caption.isNotEmpty()) Text(s.media.caption, style = CrundiType.Small.copy(color = CrundiColors.TextPrimary))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                mv.obitech.crundi.ui.components.CrundiButton("Save to Downloads", { a.media(s.media.copy(kind = "file-image")); close() }, icon = "download")
                mv.obitech.crundi.ui.components.CrundiButton("Close", close)
            }
        }
    }
}

@Composable
private fun OptionRow(icon: String, name: String, desc: String, tint: androidx.compose.ui.graphics.Color, on: Boolean, enabled: Boolean = true, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 52.dp)
            .background(if (on) CrundiColors.AccentDim else androidx.compose.ui.graphics.Color.Transparent, Dimens.ShapeSm)
            .clickable(enabled = enabled, onClick = onClick).padding(10.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Box(Modifier.size(32.dp).background(CrundiColors.BgPrimary, Dimens.ShapeSm), contentAlignment = Alignment.Center) { CrundiIcon(icon, size = 16.dp, tint = tint) }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(name, style = CrundiType.Body.copy(fontWeight = FontWeight.SemiBold))
            Text(desc, style = CrundiType.Small.copy(fontSize = CrundiType.Meta.fontSize, lineHeight = CrundiType.Meta.lineHeight))
        }
        if (on) CrundiIcon("check", size = 16.dp, tint = CrundiColors.AccentHover)
    }
}

// ─── The route ───

/**
 * One chat, live. The same composable is the main window's chat screen and
 * the whole of a bubble (ChatActivity, `LocalHost.current.isBubble`): in a
 * bubble the back arrow closes the bubble and a menu item opens the chat in the app.
 */
@Composable
fun ChatRoute(chatId: String) {
    val host = LocalHost.current
    val nav = LocalNavigator.current
    val api = LocalAppServices.current.api
    val ctx = LocalContext.current
    ChatOnScreenEffect(chatId, host)
    val chat by remember(chatId) { api.chat(chatId) }.collectAsState(ChatState(chatId))
    val server by api.events.state.collectAsState(initial = api.events.lastState)
    var ui by remember(chatId) { mutableStateOf(ChatUi(ChatState(chatId), isBubble = host.isBubble)) }
    val scope = rememberCoroutineScope()
    val view = ui.copy(chat = chat, pane = server?.terminals?.firstOrNull { it.id == chatId })

    // Mid-turn sends leave once the turn ends or the server lists them.
    LaunchedEffect(chat.state, chat.pendingInjections) {
        if (chat.state != AgentState.Working) ui = ui.copy(localPending = emptyList())
    }
    LaunchedEffect(ui.notice) { if (ui.notice != null) { delay(3500); ui = ui.copy(notice = null) } }

    fun act(entry: String? = null, block: suspend () -> Unit) = scope.launch {
        ui = ui.copy(error = null, busyEntries = if (entry != null) ui.busyEntries + entry else ui.busyEntries)
        try { block() } catch (e: Exception) { ui = ui.copy(error = errorText(e)) }
        if (entry != null) ui = ui.copy(busyEntries = ui.busyEntries - entry)
    }

    val picker = rememberLauncherForActivityResult(ActivityResultContracts.GetMultipleContents()) { uris ->
        Pickers.active = false
        val project = chat.project.ifEmpty { view.pane?.project.orEmpty() }
        if (uris.isEmpty() || project.isEmpty()) return@rememberLauncherForActivityResult
        scope.launch {
            for (u in uris) {
                val (name, size) = withContext(Dispatchers.IO) { nameAndSize(ctx, u) }
                ui = ui.copy(uploads = ui.uploads + UploadUi(name, 0f))
                try {
                    val att = api.chats.attach(project, name, size, progress = { sent, total ->
                        val f = if (total > 0) sent.toFloat() / total else 0f
                        ui = ui.copy(uploads = ui.uploads.map { if (it.name == name) it.copy(fraction = f) else it })
                    }) { ctx.contentResolver.openInputStream(u) ?: throw java.io.FileNotFoundException(name) }
                    ui = ui.copy(draft = (ui.draft.trimEnd() + (if (ui.draft.isBlank()) "" else " ") + att.path + " "))
                } catch (e: Exception) {
                    ui = ui.copy(error = "Could not upload $name: " + errorText(e))
                }
                ui = ui.copy(uploads = ui.uploads.filter { it.name != name })
            }
        }
    }

    ChatScreen(view, object : ChatActions {
        override fun back() { if (host.isBubble) host.close() else nav.back() }
        override fun sheet(s: ChatSheet?) { ui = ui.copy(sheet = s) }
        override fun toggle(id: String) { ui = ui.copy(open = if (id in ui.open) ui.open - id else ui.open + id) }
        override fun draft(text: String) { ui = ui.copy(draft = text) }
        override fun send() {
            val text = ui.draft.trim()
            if (text.isEmpty()) return
            val wasBusy = view.busy
            ui = ui.copy(draft = "", stopArmed = false)
            scope.launch {
                try {
                    val r = api.chats.send(chatId, text)
                    if (r.injected || wasBusy) ui = ui.copy(localPending = ui.localPending + PendingInjection(text, r.uuid, if (r.uuid.isNotEmpty()) "queued" else "sent"))
                } catch (e: Exception) {
                    ui = ui.copy(draft = text, error = "The message did not go: " + errorText(e))
                }
            }
        }
        override fun stop() {
            if (!ui.stopArmed) {
                ui = ui.copy(stopArmed = true)
                scope.launch { delay(3000); ui = ui.copy(stopArmed = false) }
                return
            }
            ui = ui.copy(stopArmed = false)
            act { api.chats.stop(chatId) }
        }
        override fun attach() { Pickers.active = true; picker.launch("*/*") }
        override fun modeSheet() { ui = ui.copy(sheet = ChatSheet.Mode) }
        override fun modelSheet() { ui = ui.copy(sheet = ChatSheet.Model) }
        override fun agents() { ui = ui.copy(sheet = ChatSheet.Agents) }
        override fun openAgent(toolUseId: String) { ui = ui.copy(sheet = ChatSheet.Agent(toolUseId)) }
        override fun recall(p: PendingInjection) {
            act {
                if (api.chats.recall(chatId, p.uuid)) ui = ui.copy(draft = (p.text + if (ui.draft.isBlank()) "" else "\n" + ui.draft), localPending = ui.localPending.filter { it.uuid != p.uuid })
                else ui = ui.copy(notice = "Claude already has it")
            }
        }
        override fun useSuggestion(s: ChatSuggestion) { ui = ui.copy(draft = s.text) }
        override fun dismissSuggestion() { act { api.chats.dismissSuggestion(chatId) } }
        override fun dismissAgents() { act { api.chats.dismissAgents(chatId, all = true) } }
        override fun setMode(mode: String) { act { api.chats.setPermissionMode(chatId, mode) } }
        override fun setModel(model: String) { act { api.chats.setModel(chatId, model) } }
        override fun rename(title: String) { if (title.isNotBlank()) act { api.chats.rename(chatId, title.trim()) } }
        override fun close() { act { api.chats.close(chatId); back() } }
        override fun park() { act { api.terminals.park(chatId); back() } }
        override fun bubble() { host.openBubble(chatId) }
        override fun openInMain() { host.openInMain("/chat/$chatId") }
        override fun allow(e: ChatEntry, always: Boolean) { act(e.id) { api.chats.allow(chatId, e.requestId, if (always) true else null) } }
        override fun deny(e: ChatEntry) { act(e.id) { api.chats.deny(chatId, e.requestId) } }
        override fun answer(e: ChatEntry, answers: Map<String, String>) {
            act(e.id) {
                if (e.status == "unanswered" || e.status == "cancelled") api.chats.answerClosed(chatId, e.id, answers)
                else api.chats.answer(chatId, e, answers)
            }
        }
        override fun dismissQuestion(e: ChatEntry) { act(e.id) { api.chats.dismissQuestion(chatId, e.requestId) } }
        override fun factory(e: ChatEntry, choice: String, note: String?) {
            val run = (e.raw["runId"] as? kotlinx.serialization.json.JsonPrimitive)?.content ?: return
            act(e.id) { api.factories.finish(run, choice, note) }
        }
        override fun factoryDiff(e: ChatEntry) {
            val run = (e.raw["runId"] as? kotlinx.serialization.json.JsonPrimitive)?.content ?: return
            act(e.id) { val d = api.factories.diff(run); ui = ui.copy(factoryDiffs = ui.factoryDiffs + (e.id to (d.diff + if (d.truncated) "\n… (cut short)" else ""))) }
        }
        override fun media(m: ChatMedia) {
            when {
                m.kind == "image" -> ui = ui.copy(sheet = ChatSheet.Picture(m))
                m.kind == "embed" && m.openUrl != null -> host.openExternal(m.openUrl)
                m.path != null -> act {
                    val name = saveToDownloads(ctx, m.filename.ifEmpty { m.path.substringAfter("name=", "file").substringBefore('&') }) { sink ->
                        api.transport.download(ApiRequest("GET", m.path.substringBefore('?'), queryOf(m.path)), sink).filename
                    }
                    ui = ui.copy(notice = "Saved $name to Downloads")
                }
            }
        }
    })
}

internal fun queryOf(path: String): Map<String, String?> = path.substringAfter('?', "").split('&').filter { it.isNotEmpty() }.associate {
    java.net.URLDecoder.decode(it.substringBefore('='), "UTF-8") to java.net.URLDecoder.decode(it.substringAfter('=', ""), "UTF-8")
}

private fun nameAndSize(ctx: Context, u: Uri): Pair<String, Long> {
    var name = u.lastPathSegment?.substringAfterLast('/') ?: "file"
    var size = -1L
    runCatching {
        ctx.contentResolver.query(u, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)?.use { c ->
            if (c.moveToFirst()) {
                c.getString(0)?.let { name = it }
                if (!c.isNull(1)) size = c.getLong(1)
            }
        }
    }
    if (size < 0) size = runCatching { ctx.contentResolver.openInputStream(u)?.use { s -> var n = 0L; val b = ByteArray(65536); while (true) { val r = s.read(b); if (r < 0) break; n += r }; n } }.getOrNull() ?: 0L
    return name to size
}

/** Stream a download into the phone's Downloads. Answers the file's name. */
private suspend fun saveToDownloads(ctx: Context, fallback: String, fetch: suspend (java.io.OutputStream) -> String?): String = withContext(Dispatchers.IO) {
    val tmp = java.io.File.createTempFile("dl", null, ctx.cacheDir)
    try {
        val name = (tmp.outputStream().use { fetch(it) } ?: fallback).ifEmpty { fallback }.replace('/', '_')
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            val values = ContentValues().apply { put(MediaStore.Downloads.DISPLAY_NAME, name); put(MediaStore.Downloads.IS_PENDING, 1) }
            val uri = ctx.contentResolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values) ?: throw java.io.IOException("Downloads is not available")
            ctx.contentResolver.openOutputStream(uri)?.use { out -> tmp.inputStream().use { it.copyTo(out) } }
            ctx.contentResolver.update(uri, ContentValues().apply { put(MediaStore.Downloads.IS_PENDING, 0) }, null, null)
        } else {
            @Suppress("DEPRECATION")
            val dir = android.os.Environment.getExternalStoragePublicDirectory(android.os.Environment.DIRECTORY_DOWNLOADS)
            val target = (ctx.getExternalFilesDir(android.os.Environment.DIRECTORY_DOWNLOADS) ?: dir)
            target.mkdirs()
            tmp.copyTo(java.io.File(target, name), overwrite = true)
        }
        name
    } finally { tmp.delete() }
}
