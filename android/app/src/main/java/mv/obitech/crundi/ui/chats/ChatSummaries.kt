package mv.obitech.crundi.ui.chats

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import mv.obitech.crundi.api.model.ChatAgent
import mv.obitech.crundi.api.model.ChatEntry
import mv.obitech.crundi.api.model.ChatRecap
import mv.obitech.crundi.api.model.SlashCommand
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

// Claude Code's own summaries in a chat (the server's claude-ui.js): a recap
// of where the chat stands, side questions, and one-line labels for batches
// of tool calls. Field-only parts (a batch's label, an agent's progress line)
// need no capability; the recap and the side question each have one, and an
// older server has neither.

/** The recap card (capability "chat-recap"). */
object ChatRecaps {
    /** The handshake's name for it (src/client-version.js CAPABILITIES). */
    const val CAPABILITY = "chat-recap"
    fun supported(capabilities: Set<String>): Boolean = CAPABILITY in capabilities

    /**
     * The recap to show, or null: none on a server without it, and not one
     * the person has just closed or sent a message over ([retired], its
     * [ChatRecap.key]); the server's recap:null follows a moment later.
     */
    fun shown(recap: ChatRecap?, supported: Boolean, retired: String?): ChatRecap? =
        recap?.takeIf { supported && it.text.isNotBlank() && it.key != retired }
}

/**
 * Side questions (capability "chat-aside"): answered from the conversation
 * so far without adding anything to it, and even while Claude is busy.
 * Claude Code's own convention for one is a message starting "/btw ".
 */
object ChatAside {
    const val CAPABILITY = "chat-aside"
    fun supported(capabilities: Set<String>): Boolean = CAPABILITY in capabilities

    private val BTW = Regex("^/btw(?:\\s+([\\s\\S]*))?$", RegexOption.IGNORE_CASE)

    /**
     * What the composer does with [text]: null sends it as a message (an
     * older server has no side questions, so "/btw" goes to Claude as typed);
     * otherwise it is asked on the side, and that is the question. An empty
     * question ("/btw" alone) opens the sheet to write one.
     */
    fun question(text: String, supported: Boolean): String? {
        if (!supported) return null
        val m = BTW.matchEntire(text.trim()) ?: return null
        return m.groupValues[1].trim()
    }

    /** The slash list's entry for it, so it can be found by typing "/". */
    val COMMAND = SlashCommand("btw", "Ask on the side, without adding it to the chat", "<question>")

    /** The chat's slash commands, with /btw added when the server answers side questions. */
    fun commands(list: List<SlashCommand>, supported: Boolean): List<SlashCommand> =
        if (!supported || list.any { it.name.equals("btw", true) }) list else list + COMMAND

    const val NOT_ADDED = "Not added to the conversation."
}

/**
 * "Ask on the side" asked for from outside the chat: a workbench pane's
 * menu, where the chat's own header is not drawn. The chat with that id
 * opens its side-question sheet (as [ChatSecretsBus] does for secrets).
 */
object ChatAsideBus {
    private val flow = kotlinx.coroutines.flow.MutableSharedFlow<String>(extraBufferCapacity = 8)
    val opens: kotlinx.coroutines.flow.SharedFlow<String> get() = flow
    fun open(chatId: String) { flow.tryEmit(chatId) }
}

/** One row of the conversation: an entry as it is, or a batch of tool calls folded under its label. */
sealed interface ChatRow {
    val key: String
    data class One(val entry: ChatEntry) : ChatRow {
        override val key get() = entry.id.ifEmpty { entry.seq.toString() }
    }
    /** Consecutive tool entries of one batch ([group]), labelled [summary]. */
    data class Batch(val group: String, val summary: String, val entries: List<ChatEntry>) : ChatRow {
        /** Also the key it is opened by (in ChatUi.open): a batch split by a card between its calls opens each part alone. */
        override val key get() = "g:" + entries.first().id
        val running get() = entries.any { it.status == "running" }
        val failed get() = entries.any { it.isError }
    }
}

/**
 * The entries as rows: consecutive tool entries of the same batch (the same
 * `group`, with its label) become one [ChatRow.Batch]; everything else, and a
 * tool entry with no batch yet, stays a row of its own, drawn as before.
 *
 * A subagent's row (a tool entry with an agent record in [agents] that is
 * not a background "task") is never folded away, as on the web: it is the
 * way into the agent's transcript. It stays a row of its own where it is,
 * and the rest of its batch folds around it, under one label. A batch that
 * is nothing but subagents has no label row at all.
 */
fun foldToolBatches(entries: List<ChatEntry>, agents: Map<String, ChatAgent> = emptyMap()): List<ChatRow> {
    fun subagent(e: ChatEntry) = e.kind == "tool" && agents[e.toolUseId]?.let { it.kind != "task" } == true
    val rows = ArrayList<ChatRow>(entries.size)
    // The open batch: its index in [rows], while only its own subagent rows have come after it.
    var open = -1
    for (e in entries) {
        val batched = e.kind == "tool" && e.group.isNotEmpty() && e.groupSummary.isNotEmpty()
        val last = (rows.getOrNull(open) as? ChatRow.Batch)?.takeIf { batched && it.group == e.group }
        when {
            batched && subagent(e) -> { rows.add(ChatRow.One(e)); if (last == null) open = -1 }
            last != null -> rows[open] = last.copy(summary = e.groupSummary, entries = last.entries + e)
            batched -> { rows.add(ChatRow.Batch(e.group, e.groupSummary, listOf(e))); open = rows.size - 1 }
            else -> { rows.add(ChatRow.One(e)); open = -1 }
        }
    }
    return rows
}

/**
 * A batch of tool calls as one row: its label and how many calls, folded;
 * a tap shows each call's own row under it. What Claude showed (show_image
 * and the like) stays in sight while folded.
 */
@Composable
fun ToolBatchRow(b: ChatRow.Batch, chatId: String, open: Set<String>, actions: EntryActions, agents: Map<String, ChatAgent> = emptyMap(), busy: Set<String> = emptySet()) {
    val isOpen = b.key in open
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(
            Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgSecondary).border(Dimens.Hairline, CrundiColors.BorderSubtle, Dimens.ShapeSm)
                .clickable(onClickLabel = if (isOpen) "Fold the calls" else "Show each call") { actions.toggle(b.key) }
                .padding(horizontal = 9.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(7.dp),
        ) {
            Box(Modifier.size(10.dp), contentAlignment = Alignment.Center) {
                when {
                    b.running -> Spinner(9.dp)
                    b.failed -> Dot(CrundiColors.Red)
                    else -> Dot(CrundiColors.Green)
                }
            }
            // The label gives way to the count, which is always seen.
            Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                Text(b.summary, style = ChatType.Small.copy(color = CrundiColors.TextPrimary), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
                Text("· ${b.entries.size}", style = CrundiType.Code.copy(fontSize = 11.5.sp, color = CrundiColors.TextMuted), maxLines = 1)
            }
            CrundiIcon(if (isOpen) "chevron-down" else "chevron-right", size = 12.dp, tint = CrundiColors.TextMuted)
        }
        if (isOpen) Row(Modifier.fillMaxWidth().height(androidx.compose.foundation.layout.IntrinsicSize.Min)) {
            // A rule down the left says these calls belong to the row above.
            Box(Modifier.padding(start = 4.dp, end = 8.dp).width(2.dp).fillMaxHeight().background(CrundiColors.BorderSubtle))
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                for (e in b.entries) androidx.compose.runtime.key(e.id) { ChatEntryView(e, chatId, open, actions, agents, busy) }
            }
        } else for (e in b.entries) {
            val media = if (e.status == "done") e.media(chatId) else emptyList()
            if (media.isNotEmpty()) MediaGroup(media, actions)
        }
    }
}

/**
 * The recap card above the composer: where the chat stands, for someone
 * coming back to it. In the suggestion bubble's family (BgSecondary, a
 * small caption), but full width, as it is about the whole chat.
 */
@Composable
fun RecapCard(recap: ChatRecap, onDismiss: () -> Unit, modifier: Modifier = Modifier) {
    val shape = RoundedCornerShape(10.dp)
    Row(
        modifier.fillMaxWidth().clip(shape).background(CrundiColors.BgSecondary)
            .border(Dimens.Hairline, CrundiColors.Accent.copy(alpha = 0.45f), shape)
            .padding(start = 11.dp, top = 7.dp, bottom = 8.dp, end = 4.dp),
        verticalAlignment = Alignment.Top,
    ) {
        CrundiIcon("clock", size = 13.dp, tint = CrundiColors.AccentHover, modifier = Modifier.padding(top = 2.dp, end = 8.dp))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(recap.caption, style = ChatType.Small.copy(fontSize = 10.5.sp, color = CrundiColors.TextMuted, fontWeight = FontWeight.SemiBold))
            // A recap is a sentence or two; a long one scrolls rather than pushing the box off screen.
            Text(recap.text, style = ChatType.Small.copy(color = CrundiColors.TextPrimary), modifier = Modifier.heightIn(max = 120.dp).verticalScroll(rememberScrollState()))
        }
        Box(Modifier.padding(start = 4.dp).size(26.dp).clip(RoundedCornerShape(6.dp)).clickable(onClickLabel = "Close the recap", onClick = onDismiss), contentAlignment = Alignment.Center) {
            CrundiIcon("x", size = 12.dp, tint = CrundiColors.TextMuted)
        }
    }
}

/**
 * The side question's sheet: "On the side". First a box to write the
 * question (from the menu, or "/btw" alone), then a spinner while it is
 * answered, then the question and its answer, or why there is none. The
 * answer is kept only here, in memory.
 */
@Composable
fun AsideSheet(s: ChatSheet.Aside, actions: ChatActions, onDismiss: () -> Unit) {
    SheetCard("On the side", onDismiss) {
        Column(Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 4.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            if (!s.asked) {
                Text("Ask about this chat without adding to it. It is answered from the conversation so far, even while Claude is working.", style = ChatType.Small.copy(color = CrundiColors.TextSecondary))
                CrundiTextField(
                    s.question, { actions.sheet(s.copy(question = it.take(2000))) }, placeholder = "Your question", singleLine = false, maxLines = 6,
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Send),
                    keyboardActions = KeyboardActions(onSend = { if (s.question.isNotBlank()) actions.askAside(s.question) }),
                    modifier = Modifier.fillMaxWidth(),
                )
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End)) {
                    CrundiButton("Close", onDismiss, kind = ButtonKind.Ghost)
                    CrundiButton("Ask", { actions.askAside(s.question) }, kind = ButtonKind.Primary, icon = "send", enabled = s.question.isNotBlank())
                }
                return@Column
            }
            Text(s.question, style = ChatType.Small.copy(color = CrundiColors.TextSecondary, fontStyle = FontStyle.Italic), maxLines = 4, overflow = TextOverflow.Ellipsis)
            Box(Modifier.fillMaxWidth().height(Dimens.Hairline).background(CrundiColors.BorderSubtle))
            when {
                s.error != null -> ErrorBox(s.error)
                s.answer == null -> Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(vertical = 6.dp)) {
                    Spinner(12.dp)
                    Text("Asking… this can take a few seconds.", style = ChatType.Small.copy(color = CrundiColors.TextMuted))
                }
                else -> SelectionContainer {
                    Column(Modifier.heightIn(max = 380.dp).verticalScroll(rememberScrollState())) {
                        mv.obitech.crundi.ui.components.MarkdownText(s.answer, compact = true)
                    }
                }
            }
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(ChatAside.NOT_ADDED, style = ChatType.Small.copy(color = CrundiColors.TextMuted), modifier = Modifier.weight(1f))
                if (s.error != null || s.answer != null) CrundiButton("Ask another", { actions.sheet(ChatSheet.Aside(question = if (s.error != null) s.question else "")) }, kind = ButtonKind.Ghost)
                CrundiButton("Close", onDismiss)
            }
        }
    }
}
