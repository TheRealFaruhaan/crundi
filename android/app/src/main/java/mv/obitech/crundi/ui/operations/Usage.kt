package mv.obitech.crundi.ui.operations

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.delay
import mv.obitech.crundi.api.model.Usage
import mv.obitech.crundi.api.model.UsagePoint
import mv.obitech.crundi.api.model.UsageWindow
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.CrundiChip
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.PanelHeader
import mv.obitech.crundi.ui.components.StatusBadge
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

private const val HOUR = 3_600_000L
const val FIVE_HOURS = 5 * HOUR
const val WEEK = 7 * 24 * HOUR

data class UsageState(
    val usage: Load<Usage?> = Load.Loading,
    val history: Load<List<UsagePoint>> = Load.Loading,
    val range: String = "week",
    val showWeek: Boolean = true,
    val showFive: Boolean = false,
    val now: Long = System.currentTimeMillis(),
)

class UsageEvents(
    val onRange: (String) -> Unit = {},
    val onShowWeek: (Boolean) -> Unit = {},
    val onShowFive: (Boolean) -> Unit = {},
    val onRefresh: () -> Unit = {},
)

/** The share of a window gone by at [now]: 0 at its start, 1 at its reset. */
fun elapsed(w: UsageWindow?, windowMs: Long, now: Long): Float {
    val reset = w?.resetsAtMs ?: 0L
    if (reset <= 0) return 0f
    return ((now - (reset - windowMs)).toFloat() / windowMs).coerceIn(0f, 1f)
}

/** "2d 4h", "3h 12m", "45m": the desktop's fmtRemaining. */
fun remaining(ms: Long): String {
    if (ms <= 0) return "—"
    val m = ms / 60_000
    val d = m / 1440; val h = (m % 1440) / 60; val mm = m % 60
    return when { d > 0 -> "${d}d ${h}h"; h > 0 -> "${h}h ${mm}m"; else -> "${mm}m" }
}

@Composable
fun UsageRoute() {
    val api = LocalAppServices.current.api
    val live by api.events.usage.collectAsStateWithLifecycle(initialValue = null)
    var range by remember { mutableStateOf("week") }
    var week by remember { mutableStateOf(true) }
    var five by remember { mutableStateOf(false) }
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    val first = rememberLoad { api.usage.now() }
    val history = rememberLoad(range) { api.usage.history(range) }
    LaunchedEffect(Unit) { while (true) { delay(30_000); now = System.currentTimeMillis() } }
    val usage: Load<Usage?> = live?.let { Load.Ready(it) } ?: first.value
    UsageScreen(
        UsageState(usage, history.value, range, week, five, now),
        UsageEvents(onRange = { range = it }, onShowWeek = { week = it }, onShowFive = { five = it }, onRefresh = { first.reload(); history.reload() }),
    )
}

@Composable
fun UsageScreen(state: UsageState, events: UsageEvents = UsageEvents()) {
    Column(Modifier.fillMaxSize()) {
        val u = state.usage.valueOrNull
        PanelHeader(
            "Claude usage",
            meta = u?.let { listOfNotNull(it.subscriptionType?.let { p -> "Plan: $p" }, Fmt.iso(it.fetchedAt).takeIf { t -> t > 0 }?.let { t -> "updated " + Fmt.ago(t, state.now) }).joinToString(" · ") },
        ) { CrundiIconButton("refresh", events.onRefresh, contentDescription = "Refresh") }
        when (val l = state.usage) {
            Load.Loading -> LoadingState(text = "Claude usage: loading…")
            is Load.Failed -> ErrorState("Claude usage unavailable: ${l.message}", onRetry = events.onRefresh)
            is Load.Ready -> {
                val usage = l.value
                if (usage == null) EmptyState("bar-chart", "No usage to show", body = "Usage is the owner's account. A collaborator's sign-in does not see it.")
                else Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(Dimens.ScreenPad), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    if (!usage.ok && usage.error.isNotEmpty()) StatusBadge("last known: refresh failed: ${usage.error}", BadgeKind.Warn)
                    WindowCard("Weekly", "wk", usage.week, WEEK, state.now)
                    WindowCard("5-hour", "5h", usage.fiveHour, FIVE_HOURS, state.now)
                    usage.weekOpus?.let { WindowCard("Weekly · Opus", "opus", it, WEEK, state.now, compact = true) }
                    usage.weekSonnet?.let { WindowCard("Weekly · Sonnet", "sonnet", it, WEEK, state.now, compact = true) }
                    HistoryCard(state, events)
                }
            }
        }
    }
}

/**
 * One window as the desktop's top-bar strip, made big: the usage bar over the
 * time bar, the over-pace slice in red when usage runs ahead of the clock.
 */
@Composable
private fun WindowCard(title: String, tag: String, w: UsageWindow?, windowMs: Long, now: Long, compact: Boolean = false) {
    val used = (w?.utilization ?: 0) / 100f
    val time = elapsed(w, windowMs, now)
    val ahead = ((used - time) * 100).toInt()
    Column(
        Modifier.fillMaxWidth().clip(Dimens.Shape).background(CrundiColors.BgCard).border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape).padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(verticalAlignment = Alignment.Bottom) {
            Text(title, style = CrundiType.Heading, modifier = Modifier.weight(1f))
            Text(if (w == null) "—" else "${w.utilization}%", style = CrundiType.Code.copy(fontSize = if (compact) 15.sp else 22.sp, fontWeight = FontWeight.SemiBold, color = levelColor(used)))
        }
        PaceBar(used, time)
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            CrundiIcon("clock", size = 13.dp, tint = CrundiColors.TextMuted)
            val reset = w?.resetsAtMs ?: 0
            Text(if (reset > 0) "resets in ${remaining(reset - now)} · ${Fmt.date(reset)}" else "no window open", style = CrundiType.Meta.copy(fontSize = 12.sp), modifier = Modifier.weight(1f))
            if (w != null && reset > 0) when {
                ahead > 0 -> StatusBadge("$ahead points ahead", if (ahead >= 25) BadgeKind.Bad else BadgeKind.Warn)
                else -> StatusBadge("on pace", BadgeKind.Good)
            }
        }
        if (!compact) Text("$tag ${w?.utilization ?: 0}% used · ${(time * 100).toInt()}% of the window gone", style = CrundiType.Meta)
    }
}

@Composable
fun PaceBar(used: Float, time: Float, modifier: Modifier = Modifier) {
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Canvas(Modifier.fillMaxWidth().height(12.dp).clip(Dimens.ShapeSm)) {
            drawRect(CrundiColors.BgTertiary)
            drawRect(CrundiColors.Accent, size = size.copy(width = size.width * used.coerceIn(0f, 1f)))
            if (used > time) {
                val gap = ((used - time) / 0.25f).coerceIn(0f, 1f)
                val c = androidx.compose.ui.graphics.lerp(CrundiColors.Accent, CrundiColors.Red, gap)
                drawRect(c, topLeft = Offset(size.width * time, 0f), size = size.copy(width = size.width * (used - time).coerceAtMost(1f - time)))
            }
        }
        Meter(time, color = CrundiColors.TextMuted.copy(alpha = 0.6f), height = 4.dp)
    }
}

@Composable
private fun HistoryCard(state: UsageState, events: UsageEvents) {
    Column(
        Modifier.fillMaxWidth().clip(Dimens.Shape).background(CrundiColors.BgCard).border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape).padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text("Claude usage over time", style = CrundiType.Heading)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiChip("Weekly", state.showWeek, { events.onShowWeek(!state.showWeek) })
            CrundiChip("5-hour", state.showFive, { events.onShowFive(!state.showFive) })
        }
        Segments(listOf("5h" to "5h", "day" to "Day", "week" to "Week", "month" to "Month"), state.range, events.onRange, Modifier.fillMaxWidth())
        when (val h = state.history) {
            Load.Loading -> Box(Modifier.fillMaxWidth().height(200.dp)) { LoadingState() }
            is Load.Failed -> Text(h.message, style = CrundiType.Small.copy(color = CrundiColors.Red))
            is Load.Ready -> when {
                !state.showWeek && !state.showFive -> Text("Turn on Weekly or 5-hour above to see the chart.", style = CrundiType.Small)
                h.value.isEmpty() -> Text("No usage recorded in this range yet. The chart fills in as samples are collected.", style = CrundiType.Small)
                else -> {
                    UsageChart(h.value, state.showWeek, state.showFive, state.now, Modifier.fillMaxWidth().height(200.dp))
                    Row(horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                        if (state.showWeek) Legend("Weekly", CrundiColors.Accent)
                        if (state.showFive) Legend("5-hour", CrundiColors.Sky)
                        Legend("target pace", CrundiColors.TextMuted, dashed = true)
                    }
                }
            }
        }
    }
}

@Composable
private fun Legend(label: String, color: Color, dashed: Boolean = false) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
        Canvas(Modifier.width(14.dp).height(8.dp)) {
            drawLine(color, Offset(0f, size.height / 2), Offset(size.width, size.height / 2), strokeWidth = 2.dp.toPx(),
                pathEffect = if (dashed) PathEffect.dashPathEffect(floatArrayOf(4f, 3f)) else null)
        }
        Text(label, style = CrundiType.Meta)
    }
}

/** Actual use (solid) and the even pace through each window (dashed), 0 to 100%. */
@Composable
fun UsageChart(points: List<UsagePoint>, week: Boolean, five: Boolean, now: Long, modifier: Modifier = Modifier) {
    val t0 = points.minOf { it.at }
    val t1 = maxOf(points.maxOf { it.at }, minOf(now, points.maxOf { it.at } + HOUR))
    val span = (t1 - t0).coerceAtLeast(1)
    Column(modifier) {
        Canvas(Modifier.fillMaxWidth().weight(1f)) {
            val w = size.width; val ht = size.height
            fun x(t: Long) = (t - t0).toFloat() / span * w
            fun y(p: Float) = ht - p / 100f * ht
            for (g in listOf(0, 25, 50, 75, 100)) drawLine(CrundiColors.BorderSubtle, Offset(0f, y(g.toFloat())), Offset(w, y(g.toFloat())), 1f)
            fun series(value: (UsagePoint) -> Int?, reset: (UsagePoint) -> String?, windowMs: Long, color: Color) {
                val actual = Path(); var started = false
                val target = Path(); var tStarted = false
                for (p in points) {
                    value(p)?.let { v -> val o = Offset(x(p.at), y(v.toFloat())); if (!started) { actual.moveTo(o.x, o.y); started = true } else actual.lineTo(o.x, o.y) }
                    val r = Fmt.iso(reset(p))
                    if (r > 0) {
                        val pace = ((p.at - (r - windowMs)).toFloat() / windowMs * 100f).coerceIn(0f, 100f)
                        val o = Offset(x(p.at), y(pace)); if (!tStarted) { target.moveTo(o.x, o.y); tStarted = true } else target.lineTo(o.x, o.y)
                    }
                }
                drawPath(target, color.copy(alpha = 0.55f), style = Stroke(1.5.dp.toPx(), pathEffect = PathEffect.dashPathEffect(floatArrayOf(8f, 6f))))
                drawPath(actual, color, style = Stroke(2.dp.toPx()))
            }
            if (week) series({ it.week }, { it.weekReset }, WEEK, CrundiColors.Accent)
            if (five) series({ it.five }, { it.fiveReset }, FIVE_HOURS, CrundiColors.Sky)
            if (now in t0..t1) drawLine(CrundiColors.Yellow.copy(alpha = 0.6f), Offset(x(now), 0f), Offset(x(now), ht), 1f)
        }
        Row(Modifier.fillMaxWidth().padding(top = 4.dp)) {
            val short = span <= 24 * HOUR
            val fmt = java.time.format.DateTimeFormatter.ofPattern(if (short) "HH:mm" else "d/M")
            for (i in 0..4) {
                val t = t0 + span * i / 4
                Text(fmt.format(java.time.Instant.ofEpochMilli(t).atZone(java.time.ZoneId.systemDefault())), style = CrundiType.Meta,
                    modifier = Modifier.weight(1f), textAlign = when (i) { 0 -> androidx.compose.ui.text.style.TextAlign.Start; 4 -> androidx.compose.ui.text.style.TextAlign.End; else -> androidx.compose.ui.text.style.TextAlign.Center })
            }
        }
    }
}

