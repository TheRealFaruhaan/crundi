package mv.obitech.crundi.ui.operations

import mv.obitech.crundi.ui.components.errorText

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.Banner
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens
import java.util.Locale
import kotlin.math.abs

/** Something fetched from the server: still coming, failed (in words fit to show), or here. */
sealed interface Load<out T> {
    data object Loading : Load<Nothing>
    data class Failed(val message: String) : Load<Nothing>
    data class Ready<T>(val value: T) : Load<T>

    val valueOrNull: T? get() = (this as? Ready<T>)?.value
}

/**
 * Fetch [fetch] when the keys change and whenever the returned reload is
 * called. A reload keeps the old value on screen while the new one comes
 * (no flash of the spinner); only the first fetch shows Loading.
 */
class Loader<T>(val state: MutableState<Load<T>>, private val tick: () -> Unit) {
    val value: Load<T> get() = state.value
    fun reload() = tick()
}

@Composable
fun <T> rememberLoad(vararg keys: Any?, fetch: suspend () -> T): Loader<T> {
    val state = remember(*keys) { mutableStateOf<Load<T>>(Load.Loading) }
    var tick by remember(*keys) { mutableIntStateOf(0) }
    LaunchedEffect(tick, *keys) {
        state.value = try { Load.Ready(fetch()) } catch (e: ApiException) {
            if (state.value is Load.Ready) state.value else Load.Failed(errorText(e))
        }
    }
    return remember(state) { Loader(state) { tick++ } }
}

/**
 * Runs the actions a screen offers (start, stop, delete) and keeps the one
 * line to say about the last one: what failed, or what was done. The screen
 * shows [note] as a banner.
 */
class Actions(private val scope: CoroutineScope) {
    var note by mutableStateOf<Note?>(null)
    var busy by mutableStateOf<String?>(null)

    data class Note(val text: String, val error: Boolean)

    /** Run [block]; [key] marks what is busy (a row's key) so its buttons can wait. */
    fun run(key: String = "", done: String? = null, after: () -> Unit = {}, block: suspend () -> Unit) {
        scope.launch {
            busy = key
            try {
                block()
                if (done != null) note = Note(done, false)
                after()
            } catch (e: ApiException) {
                note = Note(errorText(e), true)
            } finally {
                if (busy == key) busy = null
            }
        }
    }

    fun clear() { note = null }
}

@Composable
fun rememberActions(): Actions {
    val scope = rememberCoroutineScope()
    val a = remember(scope) { Actions(scope) }
    // A done-note goes by itself after a few seconds; an error stays until closed.
    LaunchedEffect(a.note) { if (a.note?.error == false) { delay(4000); a.note = null } }
    return a
}

/** The last action's note as a banner, with a close button. */
@Composable
fun NoteBanner(note: Actions.Note?, onClose: () -> Unit, modifier: Modifier = Modifier) {
    if (note == null) return
    Row(modifier.fillMaxWidth().padding(horizontal = Dimens.ScreenPad, vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
        Banner(
            note.text,
            Modifier.weight(1f),
            kind = if (note.error) BadgeKind.Bad else BadgeKind.Good,
            icon = if (note.error) "alert-triangle" else "check",
        )
        CrundiIconButton("x", onClose, contentDescription = "Close", size = Dimens.IconSm, touch = 36.dp)
    }
}

/** Formatting the way the desktop does it. */
object Fmt {
    /** 61235200 -> "58.4 MB" (the desktop's formatBytes: 1024 steps, one decimal under 100). */
    fun bytes(b: Long?): String {
        if (b == null || b < 0) return "\u2014"
        if (b < 1024) return "$b B"
        val units = listOf("KB", "MB", "GB", "TB")
        var v = b / 1024.0
        var i = 0
        while (v >= 1024 && i < units.size - 1) { v /= 1024; i++ }
        return if (v >= 100) "${v.toLong()} ${units[i]}" else String.format(Locale.US, "%.1f %s", v, units[i])
    }

    fun pct(p: Double?): String = if (p == null) "\u2014" else if (p >= 100) "${p.toInt()}%" else String.format(Locale.US, "%.1f%%", p)

    fun rate(perSec: Double?): String = if (perSec == null) "\u2014" else bytes(perSec.toLong()) + "/s"

    /** A span in words: "2h 14m", "3d 4h", "45s". */
    fun span(ms: Long): String {
        val s = abs(ms) / 1000
        val d = s / 86400; val h = (s % 86400) / 3600; val m = (s % 3600) / 60
        return when {
            d > 0 -> "${d}d ${h}h"
            h > 0 -> "${h}h ${m}m"
            m > 0 -> "${m}m"
            else -> "${s}s"
        }
    }

    /** "5m ago", "2h ago", "3d ago", or "just now". */
    fun ago(atMs: Long, now: Long = System.currentTimeMillis()): String {
        if (atMs <= 0) return "never"
        val d = now - atMs
        return if (d < 60_000) "just now" else span(d).substringBefore(' ') + " ago"
    }

    /** ISO time to epoch ms, 0 if it does not parse. */
    fun iso(s: String?): Long = s?.let { runCatching { java.time.Instant.parse(it).toEpochMilli() }.getOrNull() } ?: 0L

    /** "21 Oct 14:02" in the phone's zone. */
    fun date(ms: Long): String = if (ms <= 0) "-" else
        java.time.format.DateTimeFormatter.ofPattern("d MMM HH:mm", Locale.UK).format(java.time.Instant.ofEpochMilli(ms).atZone(java.time.ZoneId.systemDefault()))
}

/** A thin horizontal meter (cpu, memory, usage): [value] 0..1 in [color] on --bg-tertiary. */
@Composable
fun Meter(value: Float, modifier: Modifier = Modifier, color: Color = CrundiColors.Accent, height: androidx.compose.ui.unit.Dp = 6.dp) {
    val shape = RoundedCornerShape(height / 2)
    Box(modifier.fillMaxWidth().height(height).clip(shape).background(CrundiColors.BgTertiary)) {
        Box(Modifier.fillMaxWidth(value.coerceIn(0f, 1f)).height(height).clip(shape).background(color))
    }
}

/** Green under 70%, yellow under 90%, red above: the desktop's usage colours. */
fun levelColor(fraction: Float): Color = when {
    fraction >= 0.9f -> CrundiColors.Red
    fraction >= 0.7f -> CrundiColors.Yellow
    else -> CrundiColors.Green
}

/** A label and its value on one line ("Memory   58.4 MB"), as the desktop's key/value grids. */
@Composable
fun KeyValue(key: String, value: String, modifier: Modifier = Modifier, mono: Boolean = true, valueColor: Color = CrundiColors.TextPrimary) {
    Row(modifier.fillMaxWidth().padding(vertical = 3.dp), verticalAlignment = Alignment.Top) {
        Text(key, style = CrundiType.Small, modifier = Modifier.weight(0.38f))
        Text(
            value,
            style = (if (mono) CrundiType.Code.copy(fontSize = CrundiType.Small.fontSize) else CrundiType.Body).copy(color = valueColor),
            modifier = Modifier.weight(0.62f),
            maxLines = 3,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** A row of segment buttons (sub-tabs inside an area): the selected one in accent. */
@Composable
fun Segments(items: List<Pair<String, String>>, selected: String, onSelect: (String) -> Unit, modifier: Modifier = Modifier) {
    Row(
        modifier
            .clip(Dimens.ShapeSm)
            .background(CrundiColors.BgTertiary)
            .border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm)
            .padding(3.dp),
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        for ((id, label) in items) {
            val on = id == selected
            Box(
                Modifier
                    .weight(1f)
                    .clip(RoundedCornerShape(4.dp))
                    .background(if (on) CrundiColors.AccentDim else Color.Transparent)
                    .clickable { onSelect(id) }
                    .padding(vertical = 8.dp),
                contentAlignment = Alignment.Center,
            ) {
                Text(label, style = CrundiType.Small.copy(color = if (on) CrundiColors.AccentHover else CrundiColors.TextSecondary), maxLines = 1)
            }
        }
    }
}

/** A small round icon in a tinted circle, for the leading edge of a card. */
@Composable
fun IconDisc(icon: String, tint: Color, bg: Color, modifier: Modifier = Modifier) {
    Box(modifier.size(34.dp).clip(RoundedCornerShape(8.dp)).background(bg), contentAlignment = Alignment.Center) {
        CrundiIcon(icon, size = Dimens.Icon, tint = tint)
    }
}

/** A titled group of fields inside a scrolling settings-like screen. */
@Composable
fun Group(title: String, modifier: Modifier = Modifier, detail: String? = null, content: @Composable () -> Unit) {
    Column(modifier.fillMaxWidth().padding(horizontal = Dimens.ScreenPad, vertical = 8.dp)) {
        Text(title.uppercase(), style = CrundiType.Section, modifier = Modifier.padding(bottom = 4.dp))
        if (detail != null) Text(detail, style = CrundiType.Small, modifier = Modifier.padding(bottom = 8.dp))
        Column(
            Modifier
                .fillMaxWidth()
                .clip(Dimens.Shape)
                .background(CrundiColors.BgSecondary)
                .border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape)
                .padding(Dimens.S3),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) { content() }
    }
}
