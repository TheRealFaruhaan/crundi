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
}

/** The bottom of a chat: what is queued, the suggestion, uploads, slash list, the box and its controls. */
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
    agentsRunning: Int = 0,
    agentsTotal: Int = 0,
) {
    Column(modifier.fillMaxWidth().background(CrundiColors.BgSecondary)) {
        Box(Modifier.fillMaxWidth().height(Dimens.Hairline).background(CrundiColors.Border))
        Column(Modifier.fillMaxWidth().padding(8.dp)) {
            for (p in pending) QueueCard(p) { actions.recall(p) }
            if (suggestion != null && !busy && !ended && draft.isEmpty()) {
                Row(
                    Modifier.padding(bottom = 7.dp).clip(RoundedCornerShape(99.dp)).background(CrundiColors.BgTertiary).border(Dimens.Hairline, CrundiColors.Border, RoundedCornerShape(99.dp))
                        .clickable { actions.useSuggestion(suggestion) }.padding(start = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    CrundiIcon("zap", size = 13.dp, tint = CrundiColors.AccentHover)
                    Text(suggestion.text, style = ChatType.Small.copy(color = CrundiColors.TextPrimary), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(start = 6.dp).widthIn(max = 280.dp))
                    CrundiIconButton("x", actions::dismissSuggestion, contentDescription = "Dismiss suggestion", size = 13.dp, touch = 34.dp)
                }
            }
            for (u in uploads) Row(Modifier.fillMaxWidth().padding(bottom = 6.dp, start = 2.dp, end = 2.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(u.name, style = CrundiType.Meta, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                Box(Modifier.weight(2f).height(4.dp).clip(RoundedCornerShape(99.dp)).background(CrundiColors.BgTertiary)) {
                    Box(Modifier.fillMaxWidth(u.fraction.coerceIn(0f, 1f)).height(4.dp).clip(RoundedCornerShape(99.dp)).background(CrundiColors.Accent))
                }
                Text("${(u.fraction * 100).toInt()}%", style = CrundiType.Meta, modifier = Modifier.widthIn(min = 34.dp))
            }
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
                modifier = Modifier.fillMaxWidth(),
            )
            Row(Modifier.fillMaxWidth().padding(top = 7.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(7.dp)) {
                BoxButton("paperclip", "Attach a file", actions::attach, enabled = !ended && uploads.isEmpty())
                val m = permMode(mode)
                BoxButton(m.icon, "Permission mode: ${m.name}. Change", actions::modeSheet, tint = m.color, enabled = !ended,
                    border = if (m.key == "plan") CrundiColors.Sky.copy(alpha = 0.45f) else if (m.key == "bypassPermissions") CrundiColors.Red.copy(alpha = 0.5f) else CrundiColors.Border)
                Row(
                    Modifier.height(40.dp).clip(Dimens.ShapeSm).background(CrundiColors.BgPrimary).border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm)
                        .clickable(enabled = !ended, onClick = actions::modelSheet).padding(horizontal = 10.dp),
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp),
                ) {
                    Text(shortModel(model).ifEmpty { "model" }, style = CrundiType.Meta.copy(color = CrundiColors.TextSecondary), maxLines = 1)
                    CrundiIcon("chevron-down", size = 11.dp, tint = CrundiColors.TextMuted)
                }
                if (agentsTotal > 0) Row(
                    Modifier.height(40.dp).clip(RoundedCornerShape(99.dp)).background(CrundiColors.BgPrimary).border(Dimens.Hairline, CrundiColors.Border, RoundedCornerShape(99.dp))
                        .clickable(onClick = actions::agents).padding(horizontal = 10.dp),
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    if (agentsRunning > 0) Spinner(10.dp) else CrundiIcon("layers", size = 14.dp, tint = CrundiColors.TextSecondary)
                    Text("$agentsTotal", style = CrundiType.Meta.copy(color = CrundiColors.TextPrimary, fontWeight = FontWeight.SemiBold))
                }
                Spacer(Modifier.weight(1f))
                if (busy) Row(
                    Modifier.height(40.dp).clip(Dimens.ShapeSm).background(if (stopArmed) CrundiColors.RedDim else CrundiColors.BgPrimary)
                        .border(Dimens.Hairline, if (stopArmed) CrundiColors.Red else CrundiColors.Border, Dimens.ShapeSm)
                        .clickable(onClick = actions::stop).padding(horizontal = 11.dp),
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp),
                ) {
                    CrundiIcon("stop", size = 15.dp, tint = CrundiColors.Red, contentDescription = "Stop")
                    if (stopArmed) Text("Sure?", style = CrundiType.Meta.copy(color = CrundiColors.Red, fontFamily = CrundiType.Sans))
                }
                CrundiButton("Send", actions::send, kind = ButtonKind.Primary, icon = "send", enabled = !ended && draft.isNotBlank())
            }
            Row(Modifier.fillMaxWidth().padding(top = 6.dp, start = 2.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                if (busy) Spinner(8.dp)
                Text(stateWords, style = CrundiType.Meta.copy(color = if (busy) CrundiColors.AccentHover else CrundiColors.TextMuted, fontFamily = CrundiType.Sans, fontSize = 10.5.sp))
                Text(permMode(mode).name.lowercase(), style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontSize = 10.5.sp))
                Spacer(Modifier.weight(1f))
                if (cost > 0) Text("$" + "%.2f".format(cost), style = CrundiType.Meta.copy(fontSize = 10.5.sp))
            }
        }
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
