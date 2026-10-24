package mv.obitech.crundi.ui.workbench

import android.os.Build
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.blur
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.LocalIndication
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.PressInteraction
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Matrix
import androidx.compose.ui.graphics.TileMode
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.graphics.graphicsLayer
import kotlinx.coroutines.launch
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.sin
import kotlin.math.tan
import mv.obitech.crundi.ui.components.motionStill
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.mini.miniActivity
import mv.obitech.crundi.ui.mini.miniTarget
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/** How a new terminal pane starts (the web's launch-terminal modes). */
enum class LaunchMode(val label: String) {
    Shell("Empty Shell"),
    Chat("Claude UI Mode"),
    ChatSkip("Claude UI Mode, Skip Permissions"),
    Terminal("Terminal"),
    TerminalSkip("Terminal, Skip Permissions"),
}

/**
 * An empty slot (emptyLeafHtml): dashed outline, round buttons to launch any
 * panel into it, and the slot's own split and remove. [open]: kinds already
 * in the workbench, left out because they can be there only once.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun EmptySlot(open: Set<PaneKind>, onLaunch: (PaneKind) -> Unit, onSplit: (Mosaic.Dir) -> Unit, onRemove: (() -> Unit)?, modifier: Modifier = Modifier) {
    Box(
        modifier.fillMaxSize().padding(3.dp).drawBehind {
            drawRoundRect(
                CrundiColors.Border, style = Stroke(2.dp.toPx(), pathEffect = PathEffect.dashPathEffect(floatArrayOf(8.dp.toPx(), 6.dp.toPx()))),
                cornerRadius = CornerRadius(Dimens.Radius.toPx()),
            )
        },
        contentAlignment = Alignment.Center,
    ) {
      // A short slot (a split) cut its buttons off: it scrolls now, and is
      // centred as before whenever it all fits.
      androidx.compose.foundation.layout.BoxWithConstraints(Modifier.fillMaxSize()) {
        val tall = maxHeight
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(14.dp, Alignment.CenterVertically),
            modifier = Modifier.fillMaxWidth().verticalScroll(androidx.compose.foundation.rememberScrollState()).heightIn(min = tall).padding(16.dp),
        ) {
            Text("Launch a new panel here", style = CrundiType.Small.copy(color = CrundiColors.TextMuted))
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(14.dp, Alignment.CenterHorizontally),
                verticalArrangement = Arrangement.spacedBy(14.dp),
                modifier = Modifier.widthIn(max = 320.dp),
            ) {
                for (k in PaneKind.addable) if (!(k.unique && k in open)) RoundLauncher(k) { onLaunch(k) }
            }
            // Wraps as whole buttons on a narrow pane (Remove went to two lines).
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                CrundiButton("Split right", { onSplit(Mosaic.Dir.Row) }, icon = "split-right")
                CrundiButton("Split down", { onSplit(Mosaic.Dir.Col) }, icon = "split-down")
                if (onRemove != null) CrundiButton("Remove", onRemove, kind = ButtonKind.Ghost, icon = "x")
            }
        }
      }
    }
}

/** One round launcher: the kind's icon in a circle, its name under it. */
@Composable
fun RoundLauncher(k: PaneKind, onClick: () -> Unit) {
    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(5.dp), modifier = Modifier.width(60.dp).clip(Dimens.ShapeSm).clickable(onClick = onClick)) {
        Box(
            Modifier.size(48.dp).clip(CircleShape).background(CrundiColors.BgTertiary).border(Dimens.Hairline, CrundiColors.Border, CircleShape),
            contentAlignment = Alignment.Center,
        ) { CrundiIcon(k.icon, size = 20.dp, tint = CrundiColors.AccentHover) }
        Text(k.label, style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, color = CrundiColors.TextSecondary), maxLines = 1)
    }
}

/**
 * A new terminal pane (the web's term-launch): Empty Shell, then Claude in UI
 * Mode or as a Terminal, each with its Skip Permissions shield beside it.
 * UI Mode is the hero (the indigo gradient), the rest are flat.
 */
@Composable
fun TerminalLauncher(onPick: (LaunchMode) -> Unit, busy: Boolean = false, error: String? = null) {
    Box(Modifier.fillMaxSize().padding(16.dp), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.widthIn(max = 300.dp).fillMaxWidth()) {
            CrundiIcon("terminal", size = 40.dp, tint = CrundiColors.TextMuted.copy(alpha = 0.6f))
            LaunchButton("Empty Shell", null, Modifier.fillMaxWidth(), style = LaunchStyle.Shell, enabled = !busy) { onPick(LaunchMode.Shell) }
            Text("Claude", style = CrundiType.Section, modifier = Modifier.padding(top = 6.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                LaunchButton("UI Mode", null, Modifier.weight(1f), style = LaunchStyle.Hero, enabled = !busy) { onPick(LaunchMode.Chat) }
                LaunchButton(null, "shield-alert", Modifier.width(52.dp), style = LaunchStyle.Skip, enabled = !busy, description = "UI Mode, Skip Permissions") { onPick(LaunchMode.ChatSkip) }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                LaunchButton("Terminal", "terminal", Modifier.weight(1f), style = LaunchStyle.Normal, enabled = !busy) { onPick(LaunchMode.Terminal) }
                LaunchButton(null, "shield-alert", Modifier.width(52.dp), style = LaunchStyle.Skip, enabled = !busy, description = "Terminal, Skip Permissions") { onPick(LaunchMode.TerminalSkip) }
            }
            if (error != null) Text(error, style = CrundiType.Small.copy(color = CrundiColors.Red), textAlign = TextAlign.Center)
        }
    }
}

private enum class LaunchStyle { Shell, Hero, Normal, Skip }

@Composable
private fun LaunchButton(text: String?, icon: String?, modifier: Modifier, style: LaunchStyle, enabled: Boolean, description: String? = null, onClick: () -> Unit) {
    val shape = Dimens.ShapeSm
    val (bg, border, fg) = when (style) {
        LaunchStyle.Shell -> Triple(Brush.linearGradient(listOf(Color.Transparent, Color.Transparent)), CrundiColors.Border, CrundiColors.TextSecondary)
        LaunchStyle.Hero -> Triple(null, CrundiColors.AccentHover, Color.White)
        LaunchStyle.Normal -> Triple(Brush.linearGradient(listOf(CrundiColors.Accent.copy(alpha = 0.10f), CrundiColors.Accent.copy(alpha = 0.10f))), CrundiColors.Accent.copy(alpha = 0.55f), CrundiColors.TextPrimary)
        LaunchStyle.Skip -> Triple(Brush.linearGradient(listOf(CrundiColors.BgTertiary, CrundiColors.BgTertiary)), CrundiColors.Border, CrundiColors.Yellow)
    }
    val hero = style == LaunchStyle.Hero
    val interaction = remember { MutableInteractionSource() }
    val motion = if (hero) rememberHeroMotion(interaction) else null
    Row(
        modifier.height(44.dp).clip(shape)
            .then(if (motion != null) Modifier.heroSurface(motion) else Modifier.background(bg!!))
            .border(Dimens.Hairline, border, shape)
            .clickable(interactionSource = interaction, indication = LocalIndication.current, enabled = enabled, onClickLabel = description ?: text, onClick = onClick).padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
    ) {
        if (icon != null) CrundiIcon(icon, size = 16.dp, tint = fg, contentDescription = description)
        if (motion != null) Box(Modifier.width(7.dp).height(15.dp).graphicsLayer { alpha = motion.caretAlpha() }.background(Color.White))
        if (text != null) Text(text, style = CrundiType.Button.copy(color = fg, fontWeight = if (hero) FontWeight.Bold else FontWeight.Medium))
    }
}

/*
 * UI Mode, the hero action, moves as on the web (.term-launch .btn-chat):
 * - chatDrift: the indigo gradient (110deg, #4338ca #6366f1 #a5b4fc #6366f1
 *   #4338ca over 260% of the width) drifts to the right without end. The web
 *   moves it 4.16 widths in 9 s and jumps back; here it moves the same speed
 *   over a whole number of repeats, so the loop has no seam.
 * - chatSheen: a white band (45% of the width, skewed -22deg) sweeps across
 *   from -70% to 125% in 0.75 s, ease-out; on press, the touch hover.
 * - chatBlink: the block caret is 0.95 for 55% of 1.15 s, then 0.15.
 * With animations off (motionStill: the phone's reduced motion, or a
 * screenshot) nothing moves: the gradient rests at 35%, the caret lit, as the
 * web's prefers-reduced-motion.
 */
private val HeroStops = listOf(Color(0xFF4338CA), Color(0xFF6366F1), Color(0xFFA5B4FC), Color(0xFF6366F1), Color(0xFF4338CA))
private val HeroAt = listOf(0f, 0.26f, 0.5f, 0.74f, 1f)
private const val DriftMs = 9000
private const val BlinkMs = 1150

/** The hero's moving parts, read while drawing so they never recompose the button. */
private class HeroMotion(val still: Boolean, val drift: () -> Float, val blink: () -> Float, val sheen: () -> Float?) {
    fun caretAlpha(): Float = if (still) 0.95f else if (blink() < 0.555f) 0.95f else 0.15f
}

@Composable
private fun rememberHeroMotion(interaction: MutableInteractionSource): HeroMotion {
    val still = motionStill()
    if (still) return remember { HeroMotion(true, { 0f }, { 0f }, { null }) }
    val t = rememberInfiniteTransition(label = "hero")
    // The web: 9 s for 1.6 repeats of the gradient. Two repeats here, at the same speed.
    val drift = t.animateFloat(0f, 1f, infiniteRepeatable(tween((DriftMs * 2 / 1.6f).toInt(), easing = LinearEasing)), label = "drift")
    val blink = t.animateFloat(0f, 1f, infiniteRepeatable(tween(BlinkMs, easing = LinearEasing)), label = "blink")
    val sheen = remember { Animatable(0f) }
    var sweeping by remember { mutableStateOf(false) }
    LaunchedEffect(interaction) {
        interaction.interactions.collect { i ->
            if (i is PressInteraction.Press) launch {
                sweeping = true
                sheen.snapTo(0f)
                sheen.animateTo(1f, tween(750, easing = CubicBezierEasing(0f, 0f, 0.58f, 1f))) // CSS ease-out
                sweeping = false
            }
        }
    }
    return remember(drift, blink, sheen) { HeroMotion(false, { drift.value }, { blink.value }, { if (sweeping) sheen.value else null }) }
}

private fun Modifier.heroSurface(m: HeroMotion): Modifier = drawWithContent {
    val w = size.width
    val h = size.height
    // The CSS gradient line at 110deg over an image 2.6 widths wide.
    val rad = Math.toRadians(110.0)
    val dx = sin(rad).toFloat()
    val dy = (-cos(rad)).toFloat()
    val imgW = 2.6f * w
    val len = imgW * abs(dx) + h * abs(dy)
    val period = len / dx // one repeat of the gradient, measured along x
    // At rest: background-position 35%, i.e. the image 0.56 widths to the left.
    val shift = if (m.still) -0.56f * w else -0.56f * w + m.drift() * 2f * period
    val cx = imgW / 2f + shift
    val cy = h / 2f
    drawRect(Brush.linearGradient(
        colorStops = HeroAt.zip(HeroStops).toTypedArray(),
        start = Offset(cx - dx * len / 2f, cy - dy * len / 2f),
        end = Offset(cx + dx * len / 2f, cy + dy * len / 2f),
        tileMode = TileMode.Repeated,
    ))
    drawContent()
    val p = m.sheen() ?: return@drawWithContent
    val left = w * (-0.70f + p * (1.25f + 0.70f))
    val bw = 0.45f * w
    withTransform({
        translate(0f, cy)
        transform(Matrix().apply { values[Matrix.SkewX] = tan(Math.toRadians(-22.0)).toFloat() })
        translate(0f, -cy)
    }) {
        drawRect(
            Brush.horizontalGradient(listOf(Color.Transparent, Color.White.copy(alpha = 0.42f), Color.Transparent), startX = left, endX = left + bw),
            topLeft = Offset(left, -0.6f * h), size = Size(bw, 2.2f * h),
        )
    }
}

/**
 * A parked pane (closed when Crundi restarted, or put away): a blurred copy
 * of what it showed, and Resume over it (.parked-cell .pk-veil). [preview] is
 * the copy; until a kind supplies one, a still outline of lines stands in.
 */
@Composable
fun ParkedBody(v: PaneView, onResume: (mode: String) -> Unit, busy: Boolean = false, busyMode: String = "resume", preview: @Composable () -> Unit = { GhostLines() }) {
    val p = v.pane
    val isChat = v.parkedAs == PaneKind.Chat
    // The web's parkedChoicesHtml: whenever there is a conversation to leave
    // behind, "New session" (the pane and its place, a fresh conversation);
    // for a long chat, the token warning and "Compact and resume". A plain
    // shell has nothing to resume, so Resume already starts it fresh.
    val choices = p != null && !p.shellOnly && p.hasSession
    val heavy = choices && p!!.heavy
    // Mini (ui.mini) may come and stand on Resume, or fret over a long conversation.
    val miniId = p?.id
    mv.obitech.crundi.ui.mini.MiniParked(miniId, ready = !busy, long = isChat && heavy)
    Box(Modifier.fillMaxSize().miniTarget(miniId, "pk-veil").miniActivity(miniId)) {
        Box(Modifier.fillMaxSize().then(if (Build.VERSION.SDK_INT >= 31) Modifier.blur(6.dp) else Modifier)) { preview() }
        Box(Modifier.fillMaxSize().background(CrundiColors.BgPrimary.copy(alpha = 0.72f)), contentAlignment = Alignment.Center) {
            Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.widthIn(max = 280.dp).padding(16.dp)) {
                Text(
                    if (v.pane?.reason == "restart") "Closed when Crundi restarted" else "Put away to resume later",
                    style = CrundiType.Heading, textAlign = TextAlign.Center,
                )
                Text(
                    if (v.parkedAs == PaneKind.Chat) "The conversation is kept. Resume starts its Claude again where it left off." else "Resume opens the terminal again.",
                    style = CrundiType.Small, textAlign = TextAlign.Center,
                )
                if (heavy) Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                    CrundiIcon("shield-alert", size = 14.dp, tint = CrundiColors.Yellow)
                    Text("%.1fk tokens, a long conversation".format(p!!.tokens / 1000.0), style = CrundiType.Small.copy(color = CrundiColors.Yellow))
                }
                CrundiButton(if (busy && busyMode == "resume") "Resuming…" else "Resume", { onResume("resume") }, Modifier.miniTarget(miniId, "pk-resume"), kind = ButtonKind.Primary, icon = "play", enabled = !busy)
                if (isChat && heavy) CrundiButton(if (busy && busyMode == "compact") "Compacting…" else "Compact and resume", { onResume("compact") }, Modifier.miniTarget(miniId, "pk-compact"), enabled = !busy)
                if (choices) CrundiButton(if (busy && busyMode == "new") "Starting…" else "New session", { onResume("new") }, kind = ButtonKind.Ghost, icon = "plus", enabled = !busy)
                if (choices) Text("New session starts a fresh conversation here. The old one is kept and can be resumed from the session list.", style = CrundiType.Meta.copy(color = CrundiColors.TextMuted), textAlign = TextAlign.Center)
            }
        }
    }
}

/** Grey lines where text was: what a parked pane shows when there is no copy of it. */
@Composable
fun GhostLines() {
    Column(Modifier.fillMaxSize().padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        for (w in listOf(0.9f, 0.7f, 0.85f, 0.5f, 0.95f, 0.6f, 0.8f, 0.4f, 0.75f, 0.9f)) {
            Box(Modifier.fillMaxWidth(w).height(10.dp).clip(RoundedCornerShape(5.dp)).background(CrundiColors.BgTertiary))
        }
    }
}
