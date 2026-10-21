package mv.obitech.crundi.ui.chats

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import mv.obitech.crundi.api.model.AgentState
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.DotState
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/** A chat's or terminal's state as the sidebar's dot. */
fun AgentState.dot(running: Boolean = true): DotState = when (this) {
    AgentState.NeedsInput -> DotState.NeedsInput
    AgentState.Working -> DotState.Working
    AgentState.Waiting -> DotState.Waiting
    AgentState.Idle -> if (running) DotState.Running else DotState.Idle
    AgentState.Exited -> DotState.Idle
}

/** The desktop's words for a chat's state (the composer's status line). */
fun AgentState.words(): String = when (this) {
    AgentState.NeedsInput -> "needs you"
    AgentState.Working -> "working"
    AgentState.Waiting -> "waiting"
    AgentState.Idle -> "idle"
    AgentState.Exited -> "ended"
}

/** "claude-opus-5-5" -> "opus 5.5"; "sonnet" stays. */
fun shortModel(model: String): String {
    if (model.isBlank()) return ""
    val m = Regex("^claude-([a-z]+)-(\\d+)(?:-(\\d+))?").find(model) ?: return model
    val ver = m.groupValues[2] + (m.groupValues[3].takeIf { it.isNotEmpty() && it.length <= 2 }?.let { ".$it" } ?: "")
    return m.groupValues[1] + " " + ver
}

/** "3m ago", "2h ago", "yesterday", from epoch ms. */
fun ago(ms: Long, now: Long = System.currentTimeMillis()): String {
    if (ms <= 0) return ""
    val s = (now - ms) / 1000
    return when {
        s < 45 -> "just now"
        s < 3600 -> "${(s / 60).coerceAtLeast(1)}m ago"
        s < 86_400 -> "${s / 3600}h ago"
        s < 172_800 -> "yesterday"
        else -> "${s / 86_400}d ago"
    }
}

fun bytes(n: Long): String = when {
    n < 1024 -> "$n B"
    n < 1024 * 1024 -> "%.0f KB".format(n / 1024.0)
    n < 1024L * 1024 * 1024 -> "%.1f MB".format(n / 1024.0 / 1024)
    else -> "%.1f GB".format(n / 1024.0 / 1024 / 1024)
}

fun tokens(n: Long): String = if (n >= 1000) "%.1fk".format(n / 1000.0) else n.toString()

/** The chat's little spinner (.cc-spin). */
@Composable
fun Spinner(size: Dp = 10.dp) {
    // Still in inspection mode (screenshot tests): a spinning one never lets Robolectric go idle.
    if (androidx.compose.ui.platform.LocalInspectionMode.current)
        CircularProgressIndicator(progress = { 0.3f }, color = CrundiColors.Accent, trackColor = CrundiColors.Border, strokeWidth = 1.5.dp, modifier = Modifier.size(size))
    else
        CircularProgressIndicator(color = CrundiColors.Accent, trackColor = CrundiColors.Border, strokeWidth = 1.5.dp, modifier = Modifier.size(size))
}

/** A filled dot, as .cc-dot-ok / .cc-dot-err. */
@Composable
fun Dot(color: androidx.compose.ui.graphics.Color, size: Dp = 6.dp) {
    Box(Modifier.size(size).clip(CircleShape).background(color))
}

/** A pill tag, as .cc-tag (ok green, no red). */
@Composable
fun Tag(text: String, ok: Boolean? = null) {
    val c = when (ok) { true -> CrundiColors.Green; false -> CrundiColors.Red; null -> CrundiColors.TextSecondary }
    val border = when (ok) { true -> CrundiColors.Green; false -> CrundiColors.Red; null -> CrundiColors.BorderSubtle }
    Text(
        text, style = CrundiType.Small.copy(color = c, fontSize = CrundiType.Meta.fontSize), maxLines = 1,
        modifier = Modifier.clip(RoundedCornerShape(99.dp)).background(CrundiColors.BgTertiary).border(Dimens.Hairline, border, RoundedCornerShape(99.dp)).padding(horizontal = 8.dp, vertical = 1.dp),
    )
}

/** One choice of an [ActionSheet]. */
data class SheetAction(val label: String, val icon: String, val danger: Boolean = false, val detail: String? = null, val onClick: () -> Unit)

/** The body of an action sheet: a heading and rows. Drawn bare in screenshot tests. */
@Composable
fun ActionSheetCard(title: String, actions: List<SheetAction>, onDismiss: () -> Unit, modifier: Modifier = Modifier) {
    SheetCard(title, onDismiss, modifier) {
        for (a in actions) {
            Row(
                Modifier.fillMaxWidth().heightIn(min = 48.dp).clip(Dimens.ShapeSm).clickable { onDismiss(); a.onClick() }.padding(horizontal = 10.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                CrundiIcon(a.icon, size = Dimens.Icon, tint = if (a.danger) CrundiColors.Red else CrundiColors.TextSecondary)
                Column(Modifier.weight(1f)) {
                    Text(a.label, style = CrundiType.Body.copy(color = if (a.danger) CrundiColors.Red else CrundiColors.TextPrimary))
                    if (a.detail != null) Text(a.detail, style = CrundiType.Small, maxLines = 2, overflow = TextOverflow.Ellipsis)
                }
            }
        }
    }
}

/** The desktop's small modal (.cc-modal): uppercase heading with a close cross, then content. */
@Composable
fun SheetCard(title: String, onDismiss: () -> Unit, modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Column(
        modifier.widthIn(max = 440.dp).fillMaxWidth()
            .background(CrundiColors.BgSecondary, Dimens.Shape)
            .border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape)
            .padding(6.dp),
    ) {
        Row(Modifier.fillMaxWidth().padding(start = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(title.uppercase(), style = CrundiType.Section.copy(color = CrundiColors.TextSecondary), modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
            CrundiIconButton("x", onDismiss, contentDescription = "Close", size = 16.dp, touch = 40.dp)
        }
        content()
    }
}

/** A sheet in a dialog window. */
@Composable
fun SheetDialog(onDismiss: () -> Unit, content: @Composable () -> Unit) {
    Dialog(onDismissRequest = onDismiss) { Box(Modifier.padding(12.dp)) { content() } }
}
