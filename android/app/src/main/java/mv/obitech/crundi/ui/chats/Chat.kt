package mv.obitech.crundi.ui.chats

import mv.obitech.crundi.ui.workbench.DropKind
import mv.obitech.crundi.ui.workbench.paneDropTarget
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
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.draw.clip
import mv.obitech.crundi.ui.planning.dashedBorder
import androidx.compose.foundation.layout.offset
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.sp
import mv.obitech.crundi.ui.workbench.LocalPane
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
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.mini.MiniChat
import mv.obitech.crundi.ui.mini.miniActivity
import mv.obitech.crundi.ui.mini.miniTarget
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
    /** The subagents (kind "agent") or the background commands ("task"). */
    data class Agents(val kind: String = "agent") : ChatSheet
    data class Agent(val toolUseId: String) : ChatSheet
    data class Picture(val media: ChatMedia) : ChatSheet
    /** A video, open in its own player over everything. */
    data class Video(val media: ChatMedia) : ChatSheet
}

/** Everything the chat screen draws. */
data class ChatUi(
    val chat: ChatState,
    /** The list's row for it (title, factory role), when known. */
    val pane: Pane? = null,
    val draft: String = "",
    /**
     * The suggestion a message just went out over (its uuid, or its text):
     * hidden at once, as on the web, rather than flashing back in the moment
     * before the server says the chat is working and withdraws it.
     */
    val retiredSuggestion: String? = null,
    /** Bumped on each message sent from here: the log goes to the bottom and follows the reply. */
    val sentSeq: Int = 0,
    val uploads: List<UploadUi> = emptyList(),
    val stopArmed: Boolean = false,
    /** Mid-turn sends not yet echoed by the server. */
    val localPending: List<PendingInjection> = emptyList(),
    /** How many log entries there were when each local one was sent (by its text), to tell when Claude took it. */
    val localSentAt: Map<String, Int> = emptyMap(),
    /** Folded things the person opened (tool ids; "!id" closes a live thought). */
    val open: Set<String> = emptySet(),
    /** Cards whose answer is on its way. */
    val busyEntries: Set<String> = emptySet(),
    val factoryDiffs: Map<String, String> = emptyMap(),
    val sheet: ChatSheet? = null,
    val notice: String? = null,
    val error: String? = null,
    val isBubble: Boolean = false,
    /** Inside a Workbench pane: the pane's frame draws the header, so this leaves its own out. */
    val inPane: Boolean = false,
    /** The composer's clock and its panel. */
    val schedule: ScheduleUi = ScheduleUi(),
    /** The keyboard's Enter sends (true) or makes a new line. */
    val enterSends: Boolean = true,
    /** The project's own icon (/api/project-icon?..), for the header's tile. */
    val projectIcon: String? = null,
    /** Show the panels docked to this chat (the live route turns it on). */
    val panelDock: Boolean = false,
) {
    val title get() = pane?.label?.takeIf { it.isNotEmpty() } ?: chat.title.ifEmpty { "Chat" }
    val pending get() = chat.pendingInjections + localPending.filter { l -> chat.pendingInjections.none { it.uuid == l.uuid && l.uuid.isNotEmpty() } }
    val busy get() = chat.state == AgentState.Working || pending.isNotEmpty()
}

/** Everything the chat screen can be asked to do. */
interface ChatActions : EntryActions, ComposerActions, ScheduleActions {
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

/**
 * One chat: the conversation, live, and the composer. Full screen and in a
 * bubble it has the compact page's own header (crundi-chat-page.js #cp-head:
 * back, the project's tile, the chat's title over its state and project, the
 * menu); inside a Workbench pane the pane's frame is the header. The pane's
 * text size (A-/A+) scales every word in it.
 */
@Composable
fun ChatScreen(ui: ChatUi, actions: ChatActions, modifier: Modifier = Modifier) {
    val c = ui.chat
    val pane = LocalPane.current
    val d = LocalDensity.current
    // A file path in Claude's text: a tap offers to open it in the file
    // viewer or copy it (the web opens it on click and offers copy on hover).
    var pathMenu by remember { mutableStateOf<String?>(null) }
    var viewing by remember { mutableStateOf<String?>(null) }
    var linkMenu by remember { mutableStateOf<String?>(null) }
    CompositionLocalProvider(
        LocalDensity provides Density(d.density, d.fontScale * (pane?.fontScale ?: 1f)),
        mv.obitech.crundi.ui.components.LocalPathTap provides { p: String -> pathMenu = p },
        mv.obitech.crundi.ui.components.LocalLinkTap provides { u: String -> linkMenu = u },
    ) {
    // A file (or folder) dragged here from Files: its reference goes into the
    // message box, as dropping it on a chat does on the web.
    val draftNow = androidx.compose.runtime.rememberUpdatedState(ui.draft)
    LaunchedEffect(c.id) {
        mv.obitech.crundi.ui.workbench.PaneDrop.inserts.collect { ins ->
            if (ins.key == "chat:" + c.id) { val d0 = draftNow.value; actions.draft(if (d0.isBlank()) ins.text + " " else d0.trimEnd() + " " + ins.text + " ") }
        }
    }
    val dropHere = mv.obitech.crundi.ui.workbench.PaneDrop.hovered("chat:" + c.id)
    BoxWithConstraints(
        modifier.fillMaxSize().background(CrundiColors.BgPrimary).miniTarget(c.id, "root").miniActivity(c.id)
            .paneDropTarget("chat:" + c.id, DropKind.Chat)
            .then(if (dropHere) Modifier.border(2.dp, CrundiColors.Accent) else Modifier),
    ) {
        val tall = maxHeight
        // Mini (ui.mini) watches this chat: its state, the box, what is queued, the suggestion.
        val sugOn = c.suggestion?.let { sug -> !ui.busy && !c.exited && sug.text.isNotBlank() && sug.uuid.ifEmpty { sug.text } != ui.retiredSuggestion } == true
        MiniChat(c.id, if (c.exited) "exited" else c.state.wire, ui.draft, ui.pending.map { it.text }, sugOn)
        Column(Modifier.fillMaxSize().imePadding()) {
            if (!ui.inPane) ChatHeader(ui, actions)
            if (!c.loaded) { LoadingState(Modifier.weight(1f), text = "Opening the chat"); return@Column }
            val entries = c.visible
            val act = activity(c)
            val list = rememberLazyListState(initialFirstVisibleItemIndex = (entries.size + 1).coerceAtLeast(0))
            val lastLen = entries.lastOrNull()?.let { it.text.length + it.status.length } ?: 0
            // Sending goes to the bottom, wherever the log was, and keeps
            // following what comes in for a while (the message and the reply
            // arrive after the send).
            var followUntil by remember { mutableStateOf(0L) }
            LaunchedEffect(ui.sentSeq) {
                if (ui.sentSeq == 0) return@LaunchedEffect
                followUntil = System.currentTimeMillis() + 8000
                val n = list.layoutInfo.totalItemsCount
                if (n > 0) list.animateScrollToItem(n - 1)
            }
            LaunchedEffect(entries.size, lastLen, act) {
                val info = list.layoutInfo
                val nearEnd = info.visibleItemsInfo.lastOrNull()?.index?.let { it >= info.totalItemsCount - 3 } ?: true
                if ((nearEnd || System.currentTimeMillis() < followUntil) && info.totalItemsCount > 0) list.animateScrollToItem(info.totalItemsCount - 1)
            }
            Box(Modifier.weight(1f).fillMaxWidth()) {
              // Panels Claude docked to this chat sit above the conversation, at most 42% of it (ui.panels).
              DockedPanels(ui.panelDock, c.id) {
              Box(Modifier.fillMaxSize()) {
                LazyColumn(
                    Modifier.fillMaxSize(), state = list,
                    contentPadding = PaddingValues(start = 12.dp, end = 12.dp, top = 12.dp, bottom = 48.dp),
                    verticalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    items(entries, key = { it.id.ifEmpty { it.seq.toString() } }) { e ->
                        ChatEntryView(e, c.id, ui.open, actions, c.agents, ui.busyEntries, ui.factoryDiffs)
                    }
                    if (act != null) item(key = "~activity") { Box(Modifier.miniTarget(c.id, "activity")) { ActivityRow(act) } }
                    if (c.exited) item(key = "~ended") {
                        Banner("This chat's Claude has ended" + (c.exitCode?.takeIf { it != 0 }?.let { " (code $it)" } ?: "") + ". Put it away and resume it from the Workbench, or start a new chat.", kind = BadgeKind.Neutral, icon = "info")
                    }
                }
                // Background commands and subagents, each a pill with a count (.cc-agbadges).
                val shown = c.agents.values.filter { !it.dismissed }
                // The floating dock (.cc-agdock): the badges, then the suggested
                // next message, at the right like a message of yours would be.
                Column(Modifier.align(Alignment.BottomEnd).padding(start = 40.dp, end = 10.dp, bottom = 8.dp), horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Row(Modifier.miniTarget(c.id, "badges"), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        AgentBadge("terminal", shown.filter { it.kind == "task" }, "background command", "background commands", actions::tasks)
                        AgentBadge("layers", shown.filter { it.kind != "task" }, "agent", "agents", actions::agents)
                    }
                    val sug = c.suggestion
                    if (sug != null && sugOn) SuggestionBubble(sug.text, { actions.useSuggestion(sug) }, actions::dismissSuggestion, Modifier.miniTarget(c.id, "sug"))
                }
              }
              }
            }
            if (ui.error != null) Banner(ui.error, Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 4.dp), kind = BadgeKind.Bad, icon = "alert-triangle")
            if (ui.notice != null) Banner(ui.notice, Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 4.dp), kind = BadgeKind.Good, icon = "check")
            Composer(
                ui.draft, actions,
                busy = ui.busy, ended = c.exited, stopArmed = ui.stopArmed,
                mode = c.permissionMode.ifEmpty { if (c.skipPermissions) "bypassPermissions" else "default" },
                model = c.model, stateWords = stateLine(c.state, c.exited), cost = c.totalCostUsd,
                commands = c.slashCommands, pending = ui.pending, suggestion = c.suggestion, uploads = ui.uploads,
                scheduled = ui.schedule.items.size, scheduleOpen = ui.schedule.open, onSchedule = actions::schedule,
                enterSends = ui.enterSends, miniId = c.id,
            )
        }
        if (ui.schedule.open && c.loaded) SchedulePanel(
            ui.schedule, actions,
            Modifier.align(Alignment.BottomCenter).padding(8.dp).heightIn(max = tall * 0.82f),
        )
        pathMenu?.let { p -> PathMenu(p, c.project.ifEmpty { ui.pane?.project.orEmpty() }, onOpen = { viewing = it }) { pathMenu = null } }
        viewing?.let { p -> mv.obitech.crundi.ui.workspace.files.FileViewDialog(p) { viewing = null } }
        linkMenu?.let { u -> if (u.startsWith("code:")) CodeMenu(u.removePrefix("code:")) { linkMenu = null } else LinkMenu(u) { linkMenu = null } }
        ui.sheet?.let { s ->
            if (s is ChatSheet.Picture) ImageViewer(s.media, onSave = { actions.media(s.media.copy(kind = "file-image")); actions.sheet(null) }, onClose = { actions.sheet(null) })
            else if (s is ChatSheet.Video) VideoViewer(s.media, onSave = { actions.media(s.media.copy(kind = "file-video")) }, onClose = { actions.sheet(null) })
            else SheetOverlay({ actions.sheet(null) }) { ChatSheetContent(s, ui, actions) }
        }
    }
    }
}

/** The chat's docked panels (ui.panels.WithPanelDock) when [on]; just the content otherwise (screenshots of the chat alone). */
@Composable
private fun DockedPanels(on: Boolean, chatId: String, content: @Composable () -> Unit) {
    if (on) mv.obitech.crundi.ui.panels.WithPanelDock(chatId) { content() } else content()
}

/** The compact chat page's header (#cp-head): back, the tile, the title over a dot and the project, then the menu. */
@Composable
private fun ChatHeader(ui: ChatUi, actions: ChatActions) {
    val c = ui.chat
    val project = c.project.ifEmpty { ui.pane?.project.orEmpty() }
    Column(Modifier.fillMaxWidth().background(CrundiColors.BgSecondary)) {
        Row(
            Modifier.fillMaxWidth().statusBarsPadding().heightIn(min = 56.dp).padding(start = 4.dp, end = 4.dp, top = 8.dp, bottom = 8.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            CrundiIconButton("arrow-left", actions::back, contentDescription = if (ui.isBubble) "Put the bubble away" else "Back")
            MarkTile(chatMark(project, ui.title), ui.projectIcon, 38.dp)
            Column(Modifier.weight(1f)) {
                Text(ui.title, style = CrundiType.Body.copy(fontSize = 15.sp, fontWeight = FontWeight.SemiBold), maxLines = 1, overflow = TextOverflow.Ellipsis)
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Dot(if (c.exited) CrundiColors.TextMuted else c.state.dot().color, 7.dp)
                    Text(
                        listOfNotNull(
                            project.ifEmpty { null },
                            if (c.exited) "ended" else stateLine(c.state, false).takeIf { it != "idle" },
                            c.factory?.role?.takeIf { it.isNotEmpty() }?.let { "factory " + it.lowercase() },
                        ).joinToString(" · ").ifEmpty { "idle" },
                        style = CrundiType.Small.copy(fontSize = 12.sp, color = CrundiColors.TextSecondary), maxLines = 1, overflow = TextOverflow.Ellipsis,
                    )
                }
            }
            if (ui.isBubble) CrundiIconButton("maximize", actions::openInMain, contentDescription = "Open in Crundi")
            CrundiIconButton("more", { actions.sheet(ChatSheet.Menu) }, contentDescription = "Chat actions")
        }
        Box(Modifier.fillMaxWidth().height(Dimens.Hairline).background(CrundiColors.BorderSubtle))
    }
}

/** The suggested next message (.cc-sug-row): "Suggested, tap to send", then a dashed bubble; a tap sends it, the x drops it. */
@Composable
private fun SuggestionBubble(text: String, onSend: () -> Unit, onDismiss: () -> Unit, mini: Modifier = Modifier) {
    Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(3.dp)) {
        Text("Suggested, tap to send", style = ChatType.Small.copy(fontSize = 10.5.sp, color = CrundiColors.TextMuted),
            modifier = Modifier.padding(end = 4.dp).clip(RoundedCornerShape(4.dp)).background(CrundiColors.BgPrimary).padding(horizontal = 4.dp))
        val shape = RoundedCornerShape(topStart = 12.dp, topEnd = 12.dp, bottomEnd = 3.dp, bottomStart = 12.dp)
        // The text and its x side by side, centred: the x sat in a box taller
        // than a line, so the bubble had an empty band under the words.
        Row(
            mini.shadow(6.dp, shape).clip(shape).background(CrundiColors.BgSecondary)
                .dashedBorder(CrundiColors.Accent.copy(alpha = 0.55f), 12.dp, 1.dp, 4f)
                .clickable(onClickLabel = "Send it", onClick = onSend).padding(start = 11.dp, top = 5.dp, bottom = 5.dp, end = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(text, style = ChatType.Small.copy(color = CrundiColors.TextSecondary), modifier = Modifier.weight(1f, fill = false))
            Box(Modifier.padding(start = 4.dp).size(22.dp).clip(RoundedCornerShape(6.dp)).clickable(onClickLabel = "Dismiss suggestion", onClick = onDismiss), contentAlignment = Alignment.Center) {
                CrundiIcon("x", size = 12.dp, tint = CrundiColors.TextMuted)
            }
        }
    }
}

/** One of the floating pills over the log (.cc-agbadge): icon, count, a spinner while any runs, else a green dot. */
@Composable
private fun AgentBadge(icon: String, list: List<mv.obitech.crundi.api.model.ChatAgent>, one: String, many: String, onClick: () -> Unit) {
    if (list.isEmpty()) return
    val running = list.count { it.running }
    val shape = RoundedCornerShape(99.dp)
    Row(
        Modifier.heightIn(min = 32.dp).shadow(6.dp, shape).clip(shape).background(CrundiColors.BgSecondary).border(Dimens.Hairline, CrundiColors.Border, shape)
            .clickable(onClickLabel = "${list.size} " + (if (list.size == 1) one else many) + if (running > 0) ", $running running" else ", all finished", onClick = onClick)
            .padding(start = 9.dp, end = 11.dp, top = 4.dp, bottom = 4.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        CrundiIcon(icon, size = 15.dp, tint = if (running > 0) CrundiColors.AccentHover else CrundiColors.TextSecondary)
        Text("${list.size}", style = CrundiType.Code.copy(fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = CrundiColors.TextPrimary))
        if (running > 0) Spinner(10.dp) else Dot(CrundiColors.Green)
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
        is ChatSheet.Agents -> SheetCard(if (s.kind == "task") "Background commands" else "Agents", close) {
            val agents = ui.chat.agents.values.filter { !it.dismissed && (it.kind == "task") == (s.kind == "task") }
            if (agents.isEmpty()) Text(if (s.kind == "task") "No background commands in this chat." else "No agents in this chat.", style = CrundiType.Small, modifier = Modifier.padding(10.dp))
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
        is ChatSheet.Video -> {}
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
    val services = LocalAppServices.current
    val shellData by services.shell.state.collectAsState()
    val prefs = remember { ctx.getSharedPreferences("crundi_chat", Context.MODE_PRIVATE) }
    val inPane = LocalPane.current != null
    var ui by remember(chatId) { mutableStateOf(ChatUi(ChatState(chatId), isBubble = host.isBubble, enterSends = prefs.getString("enter", "send") != "newline")) }
    val scope = rememberCoroutineScope()
    val paneRow = server?.terminals?.firstOrNull { it.id == chatId }
    val project = chat.project.ifEmpty { paneRow?.project.orEmpty() }
    val view = ui.copy(chat = chat, pane = paneRow, inPane = inPane, projectIcon = shellData.projects.firstOrNull { it.alias == project }?.icon, panelDock = true)

    // The clock's list: loaded with the chat, again when a turn ends (turn-end
    // items go then), and every minute while the chat is on screen.
    suspend fun loadSchedule() {
        if (project.isEmpty()) return
        try {
            val r = api.chats.scheduled(project)
            ui = ui.copy(schedule = ui.schedule.copy(items = r.items, limitWarmup = r.limitWarmup, loaded = true, loadFailed = false))
        } catch (e: Exception) {
            ui = ui.copy(schedule = ui.schedule.copy(loaded = true, loadFailed = true))
        }
    }
    LaunchedEffect(project, chat.state) {
        while (true) { loadSchedule(); delay(60_000) }
    }

    // Mid-turn sends leave once the turn ends or the server lists them.
    // Also the moment Claude takes one: it shows in the log as your message,
    // which happened mid-turn too, and the card stayed until the turn ended.
    LaunchedEffect(chat.state, chat.pendingInjections, chat.messages.size) {
        if (chat.state != AgentState.Working) { ui = ui.copy(localPending = emptyList(), localSentAt = emptyMap()); return@LaunchedEffect }
        if (ui.localPending.isEmpty()) return@LaunchedEffect
        val left = ui.localPending.filter { p ->
            val from = ui.localSentAt[p.text] ?: return@filter true
            val msgs = chat.messages
            (from until msgs.size).none { i -> msgs[i].kind == "user" && msgs[i].text.trim() == p.text.trim() }
        }
        if (left.size != ui.localPending.size) ui = ui.copy(localPending = left, localSentAt = ui.localSentAt.filterKeys { k -> left.any { it.text == k } })
    }
    LaunchedEffect(ui.notice) { if (ui.notice != null) { delay(3500); ui = ui.copy(notice = null) } }

    fun act(entry: String? = null, block: suspend () -> Unit) = scope.launch {
        ui = ui.copy(error = null, busyEntries = if (entry != null) ui.busyEntries + entry else ui.busyEntries)
        try { block() } catch (e: Exception) { ui = ui.copy(error = errorText(e)) }
        if (entry != null) ui = ui.copy(busyEntries = ui.busyEntries - entry)
    }

    // Uploads attachments (picked, or pasted from the keyboard) and puts their paths in the draft.
    val uploadUris: (List<android.net.Uri>) -> Unit = uploadUris@{ uris ->
        if (uris.isEmpty() || project.isEmpty()) return@uploadUris
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
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.GetMultipleContents()) { uris ->
        Pickers.active = false
        uploadUris(uris)
    }

    ChatScreen(view, object : ChatActions {
        override fun pasteImages(uris: List<android.net.Uri>) = uploadUris(uris)
        override fun back() { if (host.isBubble) host.close() else nav.back() }
        override fun sheet(s: ChatSheet?) { ui = ui.copy(sheet = s) }
        override fun toggle(id: String) { ui = ui.copy(open = if (id in ui.open) ui.open - id else ui.open + id) }
        override fun draft(text: String) { ui = ui.copy(draft = text) }
        override fun send() {
            val text = ui.draft.trim()
            if (text.isEmpty()) return
            val wasBusy = view.busy
            ui = ui.copy(draft = "", stopArmed = false, sentSeq = ui.sentSeq + 1, retiredSuggestion = ui.chat.suggestion?.let { it.uuid.ifEmpty { it.text } } ?: ui.retiredSuggestion)
            scope.launch {
                try {
                    val r = api.chats.send(chatId, text)
                    if (r.injected || wasBusy) ui = ui.copy(
                        localPending = ui.localPending + PendingInjection(text, r.uuid, if (r.uuid.isNotEmpty()) "queued" else "sent"),
                        localSentAt = ui.localSentAt + (text to ui.chat.messages.size),
                    )
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
        override fun toggleEnter() {
            val sends = !ui.enterSends
            prefs.edit().putString("enter", if (sends) "send" else "newline").apply()
            ui = ui.copy(enterSends = sends)
        }
        override fun schedule() {
            if (ui.schedule.open) { scheduleClose(); return }
            // Opens with what is in the box already copied in: write it, then decide it goes later.
            ui = ui.copy(schedule = ui.schedule.copy(open = true, text = ui.draft, note = null, at = ScheduleUi.soonest()))
            scope.launch { loadSchedule() }
        }
        override fun scheduleClose() { ui = ui.copy(schedule = ui.schedule.copy(open = false, note = null)) }
        override fun scheduleText(text: String) { ui = ui.copy(schedule = ui.schedule.copy(text = text.take(20_000), note = null)) }
        override fun scheduleTrigger(trigger: String) {
            if (trigger == "limit-reset" && !ui.schedule.limitWarmup) {
                ui = ui.copy(schedule = ui.schedule.copy(note = "Turn on the limit warmer in Settings to use this: it is what notices the window rolling over."))
                return
            }
            ui = ui.copy(schedule = ui.schedule.copy(trigger = trigger, note = null))
        }
        override fun schedulePickTime() {
            val cal = java.util.Calendar.getInstance().apply { timeInMillis = ui.schedule.at }
            android.app.DatePickerDialog(ctx, { _, y, m, day ->
                android.app.TimePickerDialog(ctx, { _, h, min ->
                    val c2 = java.util.Calendar.getInstance().apply { set(y, m, day, h, min, 0); set(java.util.Calendar.MILLISECOND, 0) }
                    ui = ui.copy(schedule = ui.schedule.copy(at = c2.timeInMillis, note = null))
                }, cal.get(java.util.Calendar.HOUR_OF_DAY), cal.get(java.util.Calendar.MINUTE), android.text.format.DateFormat.is24HourFormat(ctx)).show()
            }, cal.get(java.util.Calendar.YEAR), cal.get(java.util.Calendar.MONTH), cal.get(java.util.Calendar.DAY_OF_MONTH)).apply {
                datePicker.minDate = System.currentTimeMillis() - 60_000
            }.show()
        }
        override fun scheduleSave() {
            val sc = ui.schedule
            val text = sc.text.trim()
            if (text.isEmpty()) { ui = ui.copy(schedule = sc.copy(note = "Write a message first.")); return }
            if (project.isEmpty()) { ui = ui.copy(schedule = sc.copy(note = "This chat has no project to schedule for.")); return }
            if (sc.trigger == "time" && sc.at <= System.currentTimeMillis()) { ui = ui.copy(schedule = sc.copy(note = "Pick a time that has not passed yet.")); return }
            ui = ui.copy(schedule = sc.copy(saving = true, note = null))
            scope.launch {
                try {
                    api.chats.schedule(project, text, sc.trigger, if (sc.trigger == "time") java.time.Instant.ofEpochMilli(sc.at).toString() else null)
                    // Queued now, so the box it came from is cleared: leaving it invites sending it twice.
                    if (ui.draft.trim() == text) ui = ui.copy(draft = "")
                    ui = ui.copy(schedule = ui.schedule.copy(text = "", saving = false, note = null))
                    loadSchedule()
                } catch (e: Exception) {
                    ui = ui.copy(schedule = ui.schedule.copy(saving = false, note = "Could not schedule that: " + errorText(e)))
                }
            }
        }
        override fun scheduleCancel(item: mv.obitech.crundi.api.model.ChatScheduleItem) {
            ui = ui.copy(schedule = ui.schedule.copy(items = ui.schedule.items.filter { it.id != item.id }))
            scope.launch {
                try { api.chats.unschedule(item.id) } catch (e: Exception) { ui = ui.copy(error = "Could not cancel it: " + errorText(e)) }
                loadSchedule()
            }
        }
        override fun modeSheet() { ui = ui.copy(sheet = ChatSheet.Mode) }
        override fun modelSheet() { ui = ui.copy(sheet = ChatSheet.Model) }
        override fun agents() { ui = ui.copy(sheet = ChatSheet.Agents("agent")) }
        override fun tasks() { ui = ui.copy(sheet = ChatSheet.Agents("task")) }
        override fun openAgent(toolUseId: String) { ui = ui.copy(sheet = ChatSheet.Agent(toolUseId)) }
        override fun recall(p: PendingInjection) {
            act {
                if (api.chats.recall(chatId, p.uuid)) ui = ui.copy(draft = (p.text + if (ui.draft.isBlank()) "" else "\n" + ui.draft), localPending = ui.localPending.filter { it.uuid != p.uuid })
                else ui = ui.copy(notice = "Claude already has it")
            }
        }
        // A tap sends it as written (the web's sendSuggestion), whatever is in the box.
        override fun useSuggestion(s: ChatSuggestion) {
            val text = s.text.trim()
            if (text.isEmpty() || view.busy) return
            ui = ui.copy(retiredSuggestion = s.uuid.ifEmpty { s.text }, sentSeq = ui.sentSeq + 1)
            scope.launch {
                try { api.chats.send(chatId, text) } catch (e: Exception) { ui = ui.copy(draft = text, error = "The message did not go: " + errorText(e)) }
            }
        }
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
        override fun answer(e: ChatEntry, answers: Map<String, String>, notes: Map<String, String>) {
            act(e.id) {
                if (e.status == "unanswered" || e.status == "cancelled") {
                    // As the web words it: the notes go into the message itself.
                    val text = if (notes.isEmpty()) null else "Answering the question from before the session restarted:\n" +
                        answers.entries.joinToString("\n") { (q, a) -> "$q -> $a" + (notes[q]?.let { " (note: $it)" } ?: "") }
                    api.chats.answerClosed(chatId, e.id, answers, text)
                } else api.chats.answer(chatId, e, answers, notes)
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
                m.kind == "video" -> ui = ui.copy(sheet = ChatSheet.Video(m))
                m.kind == "embed" && m.openUrl != null -> host.openExternal(m.openUrl)
                m.path != null -> act {
                    // Through the app's downloads: a notification with progress, then tap to open.
                    val name = m.filename.ifEmpty { m.path.substringAfter("name=", "file").substringBefore('&') }
                    // The file's card shows the progress, then where it went.
                    val key = m.path
                    val dl = mv.obitech.crundi.MediaDownloads.states
                    dl[key] = mv.obitech.crundi.MediaDownloads.State(0, m.size)
                    try {
                        val saved = mv.obitech.crundi.Downloads.save(ctx, name, size = m.size, onProgress = { n -> dl[key] = mv.obitech.crundi.MediaDownloads.State(n, m.size) }) { sink ->
                            api.transport.download(ApiRequest("GET", m.path.substringBefore('?'), queryOf(m.path)), sink)
                        }
                        dl[key] = mv.obitech.crundi.MediaDownloads.State(saved.size, saved.size, savedWhere = saved.where + "/" + saved.name)
                    } catch (e: Exception) { dl[key] = mv.obitech.crundi.MediaDownloads.State(failed = true); throw e }
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
    val tmp = java.io.File.createTempFile("crundi-download", null, ctx.cacheDir)   // the prefix must be 3+ characters ("dl" threw)
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

/** Open a path from the chat in the file viewer, or copy it. Relative paths are the project's. */
@Composable
private fun PathMenu(path: String, project: String, onOpen: (String) -> Unit, onDismiss: () -> Unit) {
    val clipboard = androidx.compose.ui.platform.LocalClipboardManager.current
    val ctx = androidx.compose.ui.platform.LocalContext.current
    val shell by mv.obitech.crundi.ui.shell.LocalAppServices.current.shell.state.collectAsState()
    val root = shell.projects.firstOrNull { it.alias.equals(project, true) }?.path.orEmpty()
    val bare = path.replace(Regex(":\\d+(:\\d+)?$"), "")
    val full = if (bare.startsWith("/") || root.isEmpty()) bare else root.trimEnd('/') + "/" + bare.removePrefix("./")
    mv.obitech.crundi.ui.components.CrundiDialog(bare.substringAfterLast('/').ifEmpty { bare }, onDismiss = onDismiss, dismiss = null) {
        Column {
            // What Claude wrote, as it wrote it; the full path is only for opening.
            Text(path, style = mv.obitech.crundi.ui.theme.CrundiType.Code.copy(color = CrundiColors.TextMuted), modifier = Modifier.padding(horizontal = 4.dp, vertical = 2.dp))
            // Over the chat, which stays as it is underneath.
            mv.obitech.crundi.ui.components.ListRow("Open", icon = "file", onClick = { onDismiss(); onOpen(full) })
            // Copy text: exactly what Claude wrote. Copy path: the full path worked out from it.
            mv.obitech.crundi.ui.components.ListRow("Copy text", icon = "copy", onClick = {
                clipboard.setText(androidx.compose.ui.text.AnnotatedString(path)); onDismiss()
                android.widget.Toast.makeText(ctx, "Text copied", android.widget.Toast.LENGTH_SHORT).show()
            })
            if (full != path) mv.obitech.crundi.ui.components.ListRow("Copy path", icon = "copy", onClick = {
                clipboard.setText(androidx.compose.ui.text.AnnotatedString(full)); onDismiss()
                android.widget.Toast.makeText(ctx, "Path copied", android.widget.Toast.LENGTH_SHORT).show()
            })
        }
    }
}

/** A link in the chat: open it, or copy it. */
@Composable
private fun LinkMenu(url: String, onDismiss: () -> Unit) {
    val clipboard = androidx.compose.ui.platform.LocalClipboardManager.current
    val ctx = androidx.compose.ui.platform.LocalContext.current
    val host = mv.obitech.crundi.ui.shell.LocalHost.current
    val name = runCatching { java.net.URI(url).host }.getOrNull()?.removePrefix("www.") ?: "Link"
    mv.obitech.crundi.ui.components.CrundiDialog(name, onDismiss = onDismiss, dismiss = null) {
        Column {
            Text(url, style = mv.obitech.crundi.ui.theme.CrundiType.Code.copy(color = CrundiColors.TextMuted), maxLines = 3, overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis, modifier = Modifier.padding(horizontal = 4.dp, vertical = 2.dp))
            mv.obitech.crundi.ui.components.ListRow("Open", icon = "external-link", onClick = { onDismiss(); host.openExternal(url) })
            mv.obitech.crundi.ui.components.ListRow("Copy link", icon = "copy", onClick = {
                clipboard.setText(androidx.compose.ui.text.AnnotatedString(url)); onDismiss()
                android.widget.Toast.makeText(ctx, "Link copied", android.widget.Toast.LENGTH_SHORT).show()
            })
        }
    }
}

/** Code in Claude's text, tapped: copy it (the web shows a copy button by it). */
@Composable
private fun CodeMenu(code: String, onDismiss: () -> Unit) {
    val clipboard = androidx.compose.ui.platform.LocalClipboardManager.current
    val ctx = androidx.compose.ui.platform.LocalContext.current
    mv.obitech.crundi.ui.components.CrundiDialog("Code", onDismiss = onDismiss, dismiss = null) {
        Column {
            Text(code, style = mv.obitech.crundi.ui.theme.CrundiType.Code.copy(color = CrundiColors.TextPrimary), maxLines = 6, overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis, modifier = Modifier.padding(horizontal = 4.dp, vertical = 4.dp))
            mv.obitech.crundi.ui.components.ListRow("Copy", icon = "copy", onClick = {
                clipboard.setText(androidx.compose.ui.text.AnnotatedString(code)); onDismiss()
                android.widget.Toast.makeText(ctx, "Copied", android.widget.Toast.LENGTH_SHORT).show()
            })
        }
    }
}
