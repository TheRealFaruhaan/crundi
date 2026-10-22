package mv.obitech.crundi.ui.chats

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import java.util.Locale
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import mv.obitech.crundi.api.model.AgentState
import mv.obitech.crundi.api.model.ChatSuggestion
import mv.obitech.crundi.api.model.PendingInjection
import mv.obitech.crundi.api.model.SlashCommand
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/** The permission modes the CLI accepts at runtime, with the desktop's names, icons and words. */
data class PermMode(val key: String, val name: String, val icon: String, val desc: String, val color: Color)

val PERM_MODES = listOf(
    PermMode("default", "Ask permissions", "shield", "Asks you before editing files or running commands.", CrundiColors.TextSecondary),
    PermMode("acceptEdits", "Accept edits", "pencil", "Edits files without asking. Still asks before running commands.", CrundiColors.AccentHover),
    PermMode("auto", "Auto", "zap", "Decides for itself what is safe to do, and asks about the rest.", CrundiColors.AccentHover),
    PermMode("plan", "Plan mode", "clipboard", "Reads and plans, changes nothing until you approve the plan.", CrundiColors.Sky),
    PermMode("dontAsk", "Don't ask", "ban", "Never asks. Anything not already allowed is refused.", CrundiColors.Yellow),
)
val BYPASS_MODE = PermMode("bypassPermissions", "Bypass permissions", "shield-alert", "Runs everything without asking. Set when the chat started; it cannot be changed here.", CrundiColors.Red)

fun permMode(key: String): PermMode = PERM_MODES.firstOrNull { it.key == key } ?: if (key == "bypassPermissions") BYPASS_MODE else PERM_MODES[0]

/** A file going up to the server for the message. */
data class UploadUi(val name: String, val fraction: Float)

/** Commands matching what is typed after "/", as the desktop's slash list. */
fun slashMatches(draft: String, commands: List<SlashCommand>): List<SlashCommand> {
    if (!draft.startsWith("/") || draft.contains(' ') || draft.contains('\n')) return emptyList()
    val q = draft.drop(1).lowercase()
    return commands.filter { it.name.lowercase().startsWith(q) } + commands.filter { !it.name.lowercase().startsWith(q) && it.name.lowercase().contains(q) && q.isNotEmpty() }
}

/** What the composer's controls do. */
interface ComposerActions {
    fun draft(text: String) {}
    fun send() {}
    fun stop() {}
    fun attach() {}
    fun modeSheet() {}
    fun modelSheet() {}
    fun recall(p: PendingInjection) {}
    fun useSuggestion(s: ChatSuggestion) {}
    fun dismissSuggestion() {}
    fun agents() {}
    fun tasks() {}
    /** Swap what the keyboard's Enter does: send, or a new line. */
    fun toggleEnter() {}
}

/** The status line's first words (the web's stateLbl). */
fun stateLine(state: AgentState, exited: Boolean): String = when {
    exited -> "exited"
    state == AgentState.Working -> "working…"
    state == AgentState.NeedsInput -> "needs your input"
    else -> "idle"
}

/**
 * The bottom of a chat, as the mobile web lays it out (.cc-narrow): what is
 * queued, the suggestion, uploads and the slash list; then the message box on
 * its own full-width row; under it attach, the schedule clock, Stop (only
 * while a turn runs; the first tap asks "Sure?") and a wide Send, which says
 * Queue while Claude is busy; and the status line: state, the permission-mode
 * icon, the Enter toggle, the model and the cost.
 */
@Composable
fun Composer(
    draft: String,
    actions: ComposerActions,
    modifier: Modifier = Modifier,
    busy: Boolean = false,
    ended: Boolean = false,
    stopArmed: Boolean = false,
    mode: String = "default",
    model: String = "",
    stateWords: String = "idle",
    cost: Double = 0.0,
    commands: List<SlashCommand> = emptyList(),
    pending: List<PendingInjection> = emptyList(),
    suggestion: ChatSuggestion? = null,
    uploads: List<UploadUi> = emptyList(),
    /** Messages waiting on the clock for this chat's project: the count on it. */
    scheduled: Int = 0,
    scheduleOpen: Boolean = false,
    onSchedule: () -> Unit = {},
    /** Enter sends (the keyboard's action key is Send) or makes a new line. */
    enterSends: Boolean = true,
) {
    Column(modifier.fillMaxWidth().background(CrundiColors.BgSecondary)) {
        Box(Modifier.fillMaxWidth().height(Dimens.Hairline).background(CrundiColors.Border))
        Column(Modifier.fillMaxWidth().padding(8.dp)) {
            for (p in pending) QueueCard(p) { actions.recall(p) }
            // The suggestion floats over the log now (Chat.kt SuggestionBubble), as on the web.
            val matches = slashMatches(draft, commands)
            if (matches.isNotEmpty()) {
                LazyColumn(
                    Modifier.fillMaxWidth().padding(bottom = 4.dp).heightIn(max = 210.dp).clip(Dimens.ShapeSm).background(CrundiColors.BgSecondary)
                        .border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm),
                ) {
                    items(matches, key = { it.name }) { c ->
                        Row(
                            Modifier.fillMaxWidth().clickable { actions.draft("/" + c.name + " ") }.padding(horizontal = 10.dp, vertical = 9.dp),
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                        ) {
                            Text("/" + c.name, style = CrundiType.Code.copy(fontSize = 12.sp, color = CrundiColors.AccentHover, fontWeight = FontWeight.SemiBold), maxLines = 1)
                            Text(c.description.ifEmpty { c.argumentHint }, style = ChatType.Small.copy(color = CrundiColors.TextMuted), maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                    }
                }
            }
            CrundiTextField(
                draft, actions::draft,
                placeholder = if (ended) "Session ended." else "Message Claude…",
                singleLine = false, maxLines = 6, enabled = !ended,
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = if (enterSends) ImeAction.Send else ImeAction.Default),
                keyboardActions = KeyboardActions(onSend = { actions.send() }),
                modifier = Modifier.fillMaxWidth(),
            )
            // Upload progress sits between the box and its buttons (.cc-up).
            for (u in uploads) Row(Modifier.fillMaxWidth().padding(top = 6.dp, start = 2.dp, end = 2.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(u.name, style = CrundiType.Meta, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                Box(Modifier.weight(2f).height(4.dp).clip(RoundedCornerShape(99.dp)).background(CrundiColors.BgTertiary)) {
                    Box(Modifier.fillMaxWidth(u.fraction.coerceIn(0f, 1f)).height(4.dp).clip(RoundedCornerShape(99.dp)).background(CrundiColors.Accent))
                }
                Text("${(u.fraction * 100).toInt()}%", style = CrundiType.Meta, modifier = Modifier.widthIn(min = 34.dp))
            }
            Row(Modifier.fillMaxWidth().padding(top = 7.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(7.dp)) {
                BoxButton("paperclip", "Attach a file", actions::attach, enabled = !ended && uploads.isEmpty())
                Box {
                    BoxButton(
                        "clock", if (scheduled > 0) "Schedule a message. $scheduled waiting" else "Schedule a message", onSchedule,
                        tint = if (scheduleOpen || scheduled > 0) CrundiColors.AccentHover else CrundiColors.TextSecondary,
                        border = if (scheduleOpen) CrundiColors.Accent else CrundiColors.Border,
                    )
                    ScheduleCount(scheduled, Modifier.align(Alignment.TopEnd).padding(top = 2.dp, end = 2.dp))
                }
                if (busy) Row(
                    Modifier.height(40.dp).widthIn(min = 40.dp).clip(Dimens.ShapeSm).background(if (stopArmed) CrundiColors.RedDim else CrundiColors.BgPrimary)
                        .border(Dimens.Hairline, if (stopArmed) CrundiColors.Red else CrundiColors.Border, Dimens.ShapeSm)
                        .clickable(onClickLabel = if (stopArmed) "Stop: tap again to be sure" else "Stop", onClick = actions::stop).padding(horizontal = 12.dp),
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp, Alignment.CenterHorizontally),
                ) {
                    CrundiIcon("stop", size = 15.dp, tint = CrundiColors.Red, contentDescription = "Stop")
                    if (stopArmed) Text("Sure?", style = CrundiType.Meta.copy(color = CrundiColors.Red, fontFamily = CrundiType.Sans))
                }
                CrundiButton(
                    if (busy) "Queue" else "Send", actions::send, kind = ButtonKind.Primary, icon = if (busy) "layers" else "send",
                    enabled = !ended && draft.isNotBlank(), modifier = Modifier.weight(1f).height(40.dp),
                )
            }
            Row(Modifier.fillMaxWidth().padding(top = 7.dp, start = 2.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                val lively = busy || stateWords == "needs your input"
                if (busy) Spinner(8.dp)
                Text(stateWords, style = CrundiType.Meta.copy(color = if (lively) CrundiColors.AccentHover else CrundiColors.TextMuted, fontFamily = CrundiType.Sans, fontSize = 10.5.sp), maxLines = 1)
                val m = permMode(mode)
                Box(
                    Modifier.height(28.dp).widthIn(min = 34.dp).clip(RoundedCornerShape(5.dp)).background(CrundiColors.BgPrimary)
                        .border(Dimens.Hairline, if (m.key == "plan") CrundiColors.Sky.copy(alpha = 0.45f) else if (m.key == "bypassPermissions") CrundiColors.Red.copy(alpha = 0.5f) else CrundiColors.Border, RoundedCornerShape(5.dp))
                        .clickable(enabled = !ended, onClickLabel = "Change the permission mode", onClick = actions::modeSheet),
                    contentAlignment = Alignment.Center,
                ) { CrundiIcon(m.icon, size = 14.dp, tint = m.color, contentDescription = "Permission mode: ${m.name}") }
                EnterToggle(enterSends, actions::toggleEnter)
                Spacer(Modifier.weight(1f))
                if (model.isNotEmpty()) Text(
                    model, style = CrundiType.Meta.copy(fontSize = 10.5.sp), maxLines = 1, overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.widthIn(max = 130.dp).clip(RoundedCornerShape(4.dp)).clickable(enabled = !ended, onClickLabel = "Change the model", onClick = actions::modelSheet).padding(vertical = 4.dp),
                )
                if (cost > 0) Text("$" + "%.4f".format(Locale.US, cost), style = CrundiType.Meta.copy(fontSize = 10.5.sp))
            }
        }
    }
}

/** The "sends" switch (.cc-toggle .cc-sw): the thumb sits over what the Enter key does now. */
@Composable
private fun EnterToggle(sends: Boolean, onClick: () -> Unit) {
    Row(
        Modifier.clip(RoundedCornerShape(99.dp)).clickable(onClickLabel = if (sends) "Enter sends. Make it a new line" else "Enter makes a new line. Make it send", onClick = onClick).padding(vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        Row(Modifier.clip(RoundedCornerShape(99.dp)).background(CrundiColors.BgTertiary).padding(2.dp)) {
            for ((i, label) in listOf("\u23CE", "\u2303\u23CE").withIndex()) {
                val on = (i == 0) == sends
                Text(
                    label, style = CrundiType.Code.copy(fontSize = 10.5.sp, fontWeight = FontWeight.SemiBold, color = if (on) CrundiColors.AccentHover else CrundiColors.TextSecondary.copy(alpha = 0.45f)),
                    modifier = Modifier.clip(RoundedCornerShape(99.dp))
                        .then(if (on) Modifier.background(CrundiColors.AccentDim).border(Dimens.Hairline, CrundiColors.Accent, RoundedCornerShape(99.dp)) else Modifier)
                        .padding(horizontal = 7.dp, vertical = 1.dp),
                )
            }
        }
        Text("sends", style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontSize = 10.5.sp, color = CrundiColors.TextSecondary))
    }
}

@Composable
private fun BoxButton(icon: String, label: String, onClick: () -> Unit, tint: Color = CrundiColors.TextSecondary, enabled: Boolean = true, border: Color = CrundiColors.Border) {
    Box(
        Modifier.size(40.dp).clip(Dimens.ShapeSm).background(CrundiColors.BgPrimary).border(Dimens.Hairline, border, Dimens.ShapeSm)
            .clickable(enabled = enabled, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) { CrundiIcon(icon, size = 16.dp, tint = if (enabled) tint else CrundiColors.TextMuted, contentDescription = label) }
}

/** A message sent mid-turn that Claude has not taken yet (.cc-queue): queued ones can be taken back. */
@Composable
private fun QueueCard(p: PendingInjection, onRecall: () -> Unit) {
    val recallable = p.state == "queued"
    val shape = RoundedCornerShape(10.dp)
    Column(
        Modifier.fillMaxWidth().padding(bottom = 7.dp).clip(shape)
            .background(if (recallable) CrundiColors.Accent.copy(alpha = 0.08f) else CrundiColors.TextSecondary.copy(alpha = 0.09f))
            .border(Dimens.Hairline, if (recallable) CrundiColors.Accent.copy(alpha = 0.55f) else CrundiColors.TextSecondary.copy(alpha = 0.45f), shape)
            .clickable(enabled = recallable, onClick = onRecall).padding(horizontal = 11.dp, vertical = 7.dp),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(if (recallable) "SENDING" else "SENT", style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontWeight = FontWeight.Bold, fontSize = 10.5.sp, color = if (recallable) CrundiColors.AccentHover else CrundiColors.TextMuted))
            Text(
                when { recallable -> "not read yet"; p.state == "started" -> "Claude is reading it"; else -> "waiting for Claude to pick it up" },
                style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontSize = 10.5.sp, color = if (recallable) CrundiColors.AccentHover.copy(alpha = 0.7f) else CrundiColors.TextMuted),
            )
        }
        Text(p.text, style = ChatType.Small.copy(color = if (recallable) CrundiColors.TextPrimary else CrundiColors.TextSecondary), maxLines = 4, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(top = 3.dp))
        Text(if (recallable) "Tap to take it back" else "Claude has this one; it joins the conversation as it is read", style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontSize = 10.5.sp), modifier = Modifier.padding(top = 4.dp))
    }
}
