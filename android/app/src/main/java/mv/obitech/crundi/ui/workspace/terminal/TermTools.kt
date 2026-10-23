package mv.obitech.crundi.ui.workspace.terminal

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
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/** The keys of the web's term-key buttons, as bytes for the PTY (termSendKey's keyMap). */
object TermKeyMap {
    fun bytes(key: String, appCursor: Boolean): String = when (key) {
        "Escape" -> TermKeys.ESC
        "Tab" -> TermKeys.TAB
        "shift+tab" -> TermKeys.SHIFT_TAB
        "ArrowUp" -> TermKeys.arrow('A', appCursor)
        "ArrowDown" -> TermKeys.arrow('B', appCursor)
        "ArrowRight" -> TermKeys.arrow('C', appCursor)
        "ArrowLeft" -> TermKeys.arrow('D', appCursor)
        "Home" -> if (appCursor) "\u001bOH" else "\u001b[H"
        "End" -> if (appCursor) "\u001bOF" else "\u001b[F"
        "Enter" -> TermKeys.ENTER
        "Space" -> " "
        else -> if (key.startsWith("ctrl+") && key.length == 6) TermKeys.ctrl(key[5]) else ""
    }
}

/**
 * The keyboard tools panel (#term-tool-panel, mobile): Esc / Tab / Shift-Tab;
 * the Ctrl grid (C, Z, A, E, D, L, and Space across both columns); the d-pad
 * with Home, End and Enter; and Layout, which switches the workbench between
 * swipe columns and the stack. [onKey] gets the web's key names ([TermKeyMap]).
 */
@Composable
fun TermToolPanel(onKey: (String) -> Unit, onLayout: (() -> Unit)?, columns: Boolean = true, modifier: Modifier = Modifier) {
    Column(
        modifier
            .fillMaxWidth()
            .shadow(12.dp, Dimens.Shape)
            .background(CrundiColors.BgSecondary, Dimens.Shape)
            .border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape)
            .padding(8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                ToolKey("Esc", w = 52.dp) { onKey("Escape") }
                ToolKey("Tab", w = 52.dp) { onKey("Tab") }
                ToolKey("⇧Tab", w = 52.dp) { onKey("shift+tab") }
            }
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                for (pair in listOf("c" to "z", "a" to "e", "d" to "l")) Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    ToolKey("Ctrl+" + pair.first.uppercase(), w = 60.dp) { onKey("ctrl+" + pair.first) }
                    ToolKey("Ctrl+" + pair.second.uppercase(), w = 60.dp) { onKey("ctrl+" + pair.second) }
                }
                ToolKey("Space", w = 124.dp) { onKey("Space") }
            }
            Spacer(Modifier.weight(1f))
            // The d-pad: Home (two cells) End / . Up . / Left Enter Right / . Down .
            val c = 48.dp
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) { ToolKey("Home", w = c * 2 + 4.dp) { onKey("Home") }; ToolKey("End", w = c) { onKey("End") } }
                Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) { Spacer(Modifier.width(c)); ToolKey("▲", w = c) { onKey("ArrowUp") } }
                Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    ToolKey("◀", w = c) { onKey("ArrowLeft") }
                    ToolKey("Enter", w = c) { onKey("Enter") }
                    ToolKey("▶", w = c) { onKey("ArrowRight") }
                }
                Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) { Spacer(Modifier.width(c)); ToolKey("▼", w = c) { onKey("ArrowDown") } }
            }
        }
        if (onLayout != null) ToolKey("Layout", icon = "columns", active = columns, modifier = Modifier.fillMaxWidth(), onClick = onLayout)
    }
}

/** One .term-tool-btn: 32 dp high, mono 11 sp bold, lit while it is on. */
@Composable
private fun ToolKey(label: String, w: Dp? = null, icon: String? = null, active: Boolean = false, modifier: Modifier = Modifier, onClick: () -> Unit) {
    Row(
        modifier
            .then(if (w != null) Modifier.width(w) else Modifier)
            .height(32.dp)
            .clip(Dimens.ShapeSm)
            .background(if (active) CrundiColors.Accent else CrundiColors.BgPrimary)
            .border(Dimens.Hairline, if (active) CrundiColors.Accent else CrundiColors.Border, Dimens.ShapeSm)
            .clickable(onClick = onClick)
            .padding(horizontal = 5.dp),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val c = if (active) Color.White else CrundiColors.TextPrimary
        if (icon != null) { CrundiIcon(icon, size = 14.dp, tint = c); Spacer(Modifier.width(6.dp)) }
        Text(label, style = CrundiType.Code.copy(fontSize = 11.sp, fontWeight = FontWeight.SemiBold, color = c), maxLines = 1)
    }
}

/** A file going up from the input bar's paperclip (.term-upload). */
data class TermUpload(val name: String, val done: Long, val total: Long)

/**
 * The terminal's input bar (#term-input-bar): the keyboard tools toggle, the
 * text box (grows up to five lines), attach (uploads to crundi_attachments
 * and puts the path into the text), and Send. Typed text goes as a whole
 * line on Send, as on the web; an empty Send is Enter.
 */
@Composable
fun TermInputBar(
    text: String,
    onText: (String) -> Unit,
    onSend: () -> Unit,
    toolsOpen: Boolean,
    onTools: () -> Unit,
    onAttach: (() -> Unit)?,
    upload: TermUpload? = null,
    enabled: Boolean = true,
    modifier: Modifier = Modifier,
) {
    Column(modifier.fillMaxWidth().background(CrundiColors.BgSecondary)) {
        Box(Modifier.fillMaxWidth().height(Dimens.Hairline).background(CrundiColors.Border))
        if (upload != null) {
            Row(Modifier.fillMaxWidth().padding(start = 10.dp, end = 10.dp, top = 6.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(upload.name, style = CrundiType.Small, maxLines = 1, modifier = Modifier.weight(1f, fill = false))
                val f = if (upload.total > 0) (upload.done.toFloat() / upload.total).coerceIn(0f, 1f) else 0f
                Box(Modifier.width(90.dp).height(4.dp).clip(Dimens.ShapeSm).background(CrundiColors.BgTertiary)) {
                    Box(Modifier.fillMaxWidth(f).height(4.dp).background(CrundiColors.Accent))
                }
                Text("${(f * 100).toInt()}%", style = CrundiType.Small.copy(color = CrundiColors.TextMuted))
            }
        }
        Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 6.dp), verticalAlignment = Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            BarButton("settings", "Keyboard tools", if (toolsOpen) BarLook.On else BarLook.Plain, onClick = onTools)
            CrundiTextField(
                text, onText,
                placeholder = "Type here... (Send button)",
                mono = true,
                singleLine = false,
                maxLines = 5,
                modifier = Modifier.weight(1f),
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, autoCorrectEnabled = true, imeAction = ImeAction.Default),
                keyboardActions = KeyboardActions.Default,
            )
            if (onAttach != null) BarButton("paperclip", "Attach a file", BarLook.Tertiary, enabled = upload == null, onClick = onAttach)
            BarButton("send", "Send to terminal", BarLook.Accent, enabled = enabled, onClick = onSend)
        }
    }
}

private enum class BarLook { Plain, On, Tertiary, Accent }

/** The bar's square 34 dp buttons (.term-tool-toggle, .term-attach-btn, .term-send-btn). */
@Composable
private fun BarButton(icon: String, label: String, look: BarLook, enabled: Boolean = true, onClick: () -> Unit) {
    val bg = when (look) { BarLook.Plain -> CrundiColors.BgPrimary; BarLook.Tertiary -> CrundiColors.BgTertiary; BarLook.On, BarLook.Accent -> CrundiColors.Accent }
    val border = when (look) { BarLook.On, BarLook.Accent -> CrundiColors.Accent; else -> CrundiColors.Border }
    val tint = when (look) { BarLook.On, BarLook.Accent -> Color.White; else -> CrundiColors.TextSecondary }
    Box(
        Modifier
            .padding(bottom = 3.dp)
            .size(38.dp)
            .clip(Dimens.ShapeSm)
            .background(if (enabled) bg else bg.copy(alpha = 0.5f))
            .border(Dimens.Hairline, border, Dimens.ShapeSm)
            .clickable(enabled = enabled, onClickLabel = label, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) { CrundiIcon(icon, size = 17.dp, tint = if (enabled) tint else tint.copy(alpha = 0.5f), contentDescription = label) }
}
