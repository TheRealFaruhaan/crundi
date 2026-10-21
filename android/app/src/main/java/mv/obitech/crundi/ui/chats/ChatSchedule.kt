package mv.obitech.crundi.ui.chats

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import mv.obitech.crundi.api.model.ChatScheduleItem
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * The composer's clock: "write it now, send it when X happens" (src/chat-schedule.js).
 * Keyed by the chat's PROJECT, as the server keeps them, so the list is that
 * project's queued messages. [at] is epoch ms for the "At a time" trigger.
 */
data class ScheduleUi(
    val open: Boolean = false,
    val text: String = "",
    /** time, turn-end or limit-reset */
    val trigger: String = "time",
    val at: Long = soonest(),
    val items: List<ChatScheduleItem> = emptyList(),
    val loaded: Boolean = false,
    val loadFailed: Boolean = false,
    /** The limit-reset trigger rides on the limit warmer: off means that choice is greyed. */
    val limitWarmup: Boolean = true,
    /** The line under the triggers: what the trigger does, or what is wrong. */
    val note: String? = null,
    val saving: Boolean = false,
) {
    companion object {
        /** Half an hour ahead, on the minute: the web's default, so "later today" is one edit. */
        fun soonest(now: Long = System.currentTimeMillis()): Long = (now + 30 * 60_000L) / 60_000L * 60_000L
    }
}

/** What the schedule panel can be asked to do. */
interface ScheduleActions {
    fun schedule() {}
    fun scheduleClose() {}
    fun scheduleText(text: String) {}
    fun scheduleTrigger(trigger: String) {}
    fun schedulePickTime() {}
    fun scheduleSave() {}
    fun scheduleCancel(item: ChatScheduleItem) {}
}

/** What a trigger says under the choices (the web's syncTrig). */
fun triggerNote(trigger: String): String? = when (trigger) {
    "turn-end" -> "Sends as soon as the current turn finishes. If nothing is running, it waits for the next turn to end."
    "limit-reset" -> "Sends when the rolling 5-hour usage window rolls over."
    else -> null
}

/** When an item goes, in words (the web's trigLabel). */
fun triggerLabel(it: ChatScheduleItem): String = when (it.trigger.type) {
    "time" -> it.trigger.at?.let { at -> runCatching { whenText(java.time.Instant.parse(at).toEpochMilli()) }.getOrNull() ?: at } ?: "at a time"
    "turn-end" -> "when the turn ends"
    "limit-reset" -> "when the 5h window resets"
    else -> it.trigger.type
}

fun whenText(ms: Long): String = SimpleDateFormat("EEE d MMM, HH:mm", Locale.getDefault()).format(Date(ms))

/** The panel over the bottom of the chat (.cc-schedpanel). */
@Composable
fun SchedulePanel(s: ScheduleUi, a: ScheduleActions, modifier: Modifier = Modifier) {
    val shape = Dimens.Shape
    Column(
        modifier.fillMaxWidth().shadow(10.dp, shape).clip(shape).background(CrundiColors.BgPrimary).border(Dimens.Hairline, CrundiColors.Border, shape),
    ) {
        Row(Modifier.fillMaxWidth().padding(start = 12.dp, end = 4.dp, top = 2.dp, bottom = 2.dp), verticalAlignment = Alignment.CenterVertically) {
            CrundiIcon("clock", size = 15.dp, tint = CrundiColors.AccentHover)
            Text("Schedule a message", style = CrundiType.Body.copy(fontWeight = FontWeight.SemiBold), modifier = Modifier.weight(1f).padding(start = 8.dp))
            CrundiIconButton("x", a::scheduleClose, contentDescription = "Close", size = 15.dp, touch = 40.dp)
        }
        mv.obitech.crundi.ui.components.CrundiDivider()
        Column(Modifier.verticalScroll(rememberScrollState()).padding(10.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiTextField(s.text, a::scheduleText, placeholder = "Message to send later…", singleLine = false, minLines = 3, maxLines = 6, modifier = Modifier.fillMaxWidth())
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                for ((key, label) in listOf("time" to "At a time", "turn-end" to "Turn ends", "limit-reset" to "Limit resets")) {
                    val off = key == "limit-reset" && !s.limitWarmup
                    val on = s.trigger == key
                    val sh = Dimens.ShapeSm
                    Box(
                        Modifier.weight(1f).heightIn(min = 38.dp).clip(sh)
                            .background(if (on) CrundiColors.AccentDim else CrundiColors.BgSecondary)
                            .border(Dimens.Hairline, if (on) CrundiColors.Accent else CrundiColors.Border, sh)
                            .clickable { a.scheduleTrigger(key) }.padding(horizontal = 6.dp, vertical = 8.dp),
                        contentAlignment = Alignment.Center,
                    ) {
                        Text(label, style = CrundiType.Small.copy(fontSize = 12.sp, color = when { off -> CrundiColors.TextMuted.copy(alpha = 0.55f); on -> CrundiColors.TextPrimary; else -> CrundiColors.TextSecondary }), maxLines = 1)
                    }
                }
            }
            if (s.trigger == "time") Row(
                Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgSecondary).border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm)
                    .clickable(onClickLabel = "Pick the time", onClick = a::schedulePickTime).padding(horizontal = 10.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                CrundiIcon("clock", size = 15.dp, tint = CrundiColors.TextSecondary)
                Text(whenText(s.at), style = CrundiType.Body, modifier = Modifier.weight(1f))
                Text("Change", style = CrundiType.Meta.copy(color = CrundiColors.AccentHover, fontFamily = CrundiType.Sans))
            }
            (s.note ?: triggerNote(s.trigger))?.let { Text(it, style = CrundiType.Small.copy(fontSize = 11.5.sp, color = CrundiColors.TextMuted)) }
            CrundiButton(if (s.saving) "Scheduling…" else "Schedule", a::scheduleSave, kind = ButtonKind.Primary, icon = "clock", enabled = !s.saving, modifier = Modifier.fillMaxWidth())
            mv.obitech.crundi.ui.components.CrundiDivider()
            when {
                s.loadFailed -> Text("Could not load scheduled messages.", style = ChatType.Small.copy(fontStyle = FontStyle.Italic, color = CrundiColors.TextMuted))
                !s.loaded -> Text("Loading…", style = ChatType.Small.copy(fontStyle = FontStyle.Italic, color = CrundiColors.TextMuted))
                s.items.isEmpty() -> Text("Nothing scheduled for this project.", style = ChatType.Small.copy(fontStyle = FontStyle.Italic, color = CrundiColors.TextMuted))
                else -> s.items.forEachIndexed { i, it -> ScheduledRow(it, next = i == 0) { a.scheduleCancel(it) } }
            }
        }
    }
}

/** One queued message (.cc-sched-item); the first is what goes next. */
@Composable
private fun ScheduledRow(it: ChatScheduleItem, next: Boolean, onCancel: () -> Unit) {
    val sh = Dimens.ShapeSm
    Row(
        Modifier.fillMaxWidth().clip(sh).background(CrundiColors.BgSecondary).border(Dimens.Hairline, if (next) CrundiColors.Accent else CrundiColors.BorderSubtle, sh).padding(start = 9.dp),
        verticalAlignment = Alignment.Top,
    ) {
        Column(Modifier.weight(1f).padding(vertical = 7.dp)) {
            Text(buildAnnotatedString {
                if (next) { withStyle(SpanStyle(color = CrundiColors.AccentHover, fontWeight = FontWeight.Bold)) { append("next") }; append(" · ") }
                append(triggerLabel(it))
            }, style = CrundiType.Meta.copy(fontSize = 10.5.sp))
            Text(it.text, style = ChatType.Small.copy(color = CrundiColors.TextSecondary), maxLines = 3, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(top = 2.dp))
            if (it.status == "failed" && it.error.isNotEmpty()) Text(it.error, style = CrundiType.Meta.copy(color = CrundiColors.Red), modifier = Modifier.padding(top = 2.dp))
        }
        CrundiIconButton("x", onCancel, contentDescription = "Cancel this message", size = 13.dp, touch = 36.dp)
    }
}

/** The count on the clock (how many messages are waiting for this project). */
@Composable
fun ScheduleCount(n: Int, modifier: Modifier = Modifier) {
    if (n <= 0) return
    Text(
        if (n > 9) "9+" else n.toString(),
        style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontSize = 9.5.sp, fontWeight = FontWeight.Bold, color = CrundiColors.TextPrimary),
        modifier = modifier.clip(RoundedCornerShape(99.dp)).background(CrundiColors.Accent).padding(horizontal = 4.5.dp),
    )
}
