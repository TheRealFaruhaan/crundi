package mv.obitech.crundi.ui.workbench

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import mv.obitech.crundi.api.model.NewChat
import mv.obitech.crundi.api.model.ResumableSession
import mv.obitech.crundi.ui.components.DialogCard
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens
import java.text.DateFormat
import java.util.Date
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.roundToLong

/*
 * Choosing a conversation (the web's openSessionChooser / renderChatResume /
 * renderHeavyChoice). Every Claude launch asks first: a new session, or an
 * earlier one in this project, in the mode of the button that was pressed.
 * One open elsewhere still opens: the server forks it, so it arrives as a copy.
 */

/** A Claude launch waiting on the person's choice; [heavy] when the heavy choice is showing. */
data class SessionChooser(
    val pendingKey: String,
    val mode: LaunchMode,
    val list: List<ResumableSession>,
    /** Why the list did not load; New session is still offered. */
    val failed: String? = null,
    val heavy: ResumableSession? = null,
)

/** From here a conversation is heavy (the web's 100000). */
const val HEAVY_TOKENS = 100_000L

val ResumableSession.heavy get() = tokens >= HEAVY_TOKENS

val LaunchMode.isChat get() = this == LaunchMode.Chat || this == LaunchMode.ChatSkip

/** The chooser's name for each mode (the web's CR_MODE_LABEL). */
val LaunchMode.chooserLabel get() = when (this) {
    LaunchMode.Chat -> "UI Mode"
    LaunchMode.ChatSkip -> "UI Mode — Skip Permissions"
    LaunchMode.Terminal -> "Terminal"
    LaunchMode.TerminalSkip -> "Terminal — Skip Permissions"
    LaunchMode.Shell -> label
}

/** "142.0k tokens", or the file's size when the tokens are not known. */
fun sessionSize(s: ResumableSession): String =
    if (s.tokens != 0L) "%.1fk tokens".format(java.util.Locale.ROOT, s.tokens / 1000.0)
    else "${max(1L, (s.sizeBytes / 1024.0).roundToLong())} KB"

/** How long ago, as the web's relTime: just now, 5m ago, 3h ago, 2d ago, then the date. */
fun relTime(iso: String, now: Long = System.currentTimeMillis()): String {
    val t = runCatching { java.time.Instant.parse(iso).toEpochMilli() }.getOrNull() ?: return ""
    if (t == 0L) return ""
    val s = max(0.0, (now - t) / 1000.0)
    return when {
        s < 60 -> "just now"
        s < 3600 -> "${floor(s / 60).toLong()}m ago"
        s < 86400 -> "${floor(s / 3600).toLong()}h ago"
        s < 2592000 -> "${floor(s / 86400).toLong()}d ago"
        else -> DateFormat.getDateInstance(DateFormat.SHORT).format(Date(t))
    }
}

/** A row's meta line without its marks: short id, age, size. */
fun sessionMeta(s: ResumableSession, now: Long = System.currentTimeMillis()): String =
    s.id.take(8) + " · " + relTime(s.updatedAt, now) + " · " + sessionSize(s)

/**
 * The body of a new chat (the web's launchTerminal chatBody): skip
 * permissions and the session's way of starting, with the titles the web gives.
 */
fun chatLaunch(project: String, mode: LaunchMode, sessionMode: String?, resumeId: String?): NewChat {
    val skip = mode == LaunchMode.ChatSkip
    var title: String? = if (skip) "Chat (skip perms)" else null
    var how: String? = null
    var id: String? = null
    if (sessionMode != null) {
        how = sessionMode
        if (!resumeId.isNullOrEmpty()) id = resumeId
        if (sessionMode == "compact") title = "Chat (compacted)"
        else if (sessionMode == "new") title = "Chat (new)"
    } else if (!resumeId.isNullOrEmpty()) {
        how = "resume"; id = resumeId; title = "Chat (resumed)"
    }
    return NewChat(project = project, title = title, skipPermissions = skip.takeIf { it }, sessionMode = how, resumeId = id)
}

/** The chooser as a dialog over the workbench. */
@Composable
fun SessionChooserDialog(c: SessionChooser, onNew: () -> Unit, onPick: (ResumableSession) -> Unit, onResume: (ResumableSession, String) -> Unit, onBack: () -> Unit, onCancel: () -> Unit) {
    Dialog(onDismissRequest = onCancel) {
        SessionChooserCard(c, onNew, onPick, onResume, onBack, onCancel, Modifier.padding(16.dp))
    }
}

/** The chooser's card, without the window (also what the screenshot tests draw). */
@Composable
fun SessionChooserCard(
    c: SessionChooser,
    onNew: () -> Unit = {},
    onPick: (ResumableSession) -> Unit = {},
    onResume: (ResumableSession, String) -> Unit = { _, _ -> },
    onBack: () -> Unit = {},
    onCancel: () -> Unit = {},
    modifier: Modifier = Modifier,
    now: Long = System.currentTimeMillis(),
) {
    DialogCard("Open " + c.mode.chooserLabel, modifier, dismiss = "Cancel", onDismiss = onCancel) {
        // The list scrolls inside the card (the web's .cr-list), so Cancel stays in reach.
        Column(Modifier.weight(1f, fill = false).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            val h = c.heavy
            if (h != null) HeavyChoice(h, onResume, onBack) else {
                ChooserItem(onNew, accent = true) {
                    Text("New session", style = ItemTitle.copy(color = CrundiColors.AccentHover))
                    Text("Start a fresh conversation", style = CrundiType.Meta)
                }
                if (c.failed != null) Text("Could not load earlier conversations: " + c.failed, style = CrundiType.Small.copy(color = CrundiColors.TextMuted), modifier = Modifier.padding(vertical = 8.dp, horizontal = 2.dp))
                for (s in c.list) ChooserItem({ onPick(s) }) {
                    Text(s.title, style = ItemTitle)
                    Text(buildAnnotatedString {
                        append(sessionMeta(s, now))
                        if (s.heavy) { append(" · "); withStyle(SpanStyle(color = CrundiColors.Yellow, fontWeight = FontWeight.Bold)) { append("heavy") } }
                        if (s.inUse) { append(" · "); withStyle(SpanStyle(color = CrundiColors.Accent, fontWeight = FontWeight.Bold)) { append("open elsewhere — opens a copy") } }
                    }, style = CrundiType.Meta)
                }
            }
        }
    }
}

/**
 * A heavy conversation in UI Mode resends its whole transcript on every
 * turn: offer compacting first. Both resume the conversation that was picked.
 */
@Composable
private fun ColumnScope.HeavyChoice(s: ResumableSession, onResume: (ResumableSession, String) -> Unit, onBack: () -> Unit) {
    Text(buildAnnotatedString {
        withStyle(SpanStyle(fontWeight = FontWeight.Bold, color = CrundiColors.TextPrimary)) { append(s.title) }
        append("\n")
        append(sessionSize(s) + " — continuing loads all of it into every turn. ")
        append("Compacting loads it once, then summarises it so later turns stay small.")
        if (s.inUse) { append("\n"); withStyle(SpanStyle(fontWeight = FontWeight.Bold, color = CrundiColors.Accent)) { append("It is open elsewhere, so this opens a copy.") } }
    }, style = CrundiType.Small.copy(lineHeight = CrundiType.Body.lineHeight), modifier = Modifier.padding(start = 2.dp, end = 2.dp, top = 2.dp, bottom = 6.dp))
    ChooserItem({ onResume(s, "compact") }, accent = true) { Text("Compact first (recommended)", style = ItemTitle.copy(color = CrundiColors.AccentHover)) }
    ChooserItem({ onResume(s, "resume") }) { Text("Load the full conversation", style = ItemTitle) }
    ChooserItem(onBack) { Text("← Back to the list", style = CrundiType.Meta) }
}

private val ItemTitle = CrundiType.Body.copy(fontWeight = FontWeight.SemiBold)

/** One row (.cr-item): bordered, the accent border for the first choice (.cr-new). */
@Composable
private fun ChooserItem(onClick: () -> Unit, accent: Boolean = false, content: @Composable ColumnScope.() -> Unit) {
    Column(
        Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgPrimary)
            .border(Dimens.Hairline, if (accent) CrundiColors.Accent else CrundiColors.Border, Dimens.ShapeSm)
            .clickable(onClick = onClick).padding(horizontal = 12.dp, vertical = 9.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp),
        content = content,
    )
}
