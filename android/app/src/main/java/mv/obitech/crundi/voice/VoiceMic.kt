package mv.obitech.crundi.voice

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.animation.core.animateFloat
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiColors
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.sign
import kotlin.math.sin
import kotlin.random.Random

/**
 * The mic button as liquid. Quiet, it is a glossy sphere of layered liquid
 * (cyan, indigo, violet, blended). When the owner talks it spreads into a water
 * surface whose top edge is the voice itself (its loudness of the last
 * moments, newest in the middle), with droplets flying off the peaks and
 * ripples running over it; when they stop, surface tension pulls it back
 * into the sphere, with a wobble.
 *
 * [mic] is read every frame (The owner\'s level, 0..1). It draws wider than its
 * touch area while spread, on purpose.
 */
@Composable
fun FluidMic(phase: Phase, accent: Color, mic: () -> Float, icon: String?, label: String, onClick: () -> Unit, modifier: Modifier = Modifier, tone: MicTone = MicTone.Ready) {
    // The state in its colours (the owner, 11 Nov: no pill on the top bar), eased from one to the next.
    val cols = tone.colors(phase)
    val c0 by androidx.compose.animation.animateColorAsState(cols[0], androidx.compose.animation.core.tween(450), label = "mic0")
    val c1 by androidx.compose.animation.animateColorAsState(cols[1], androidx.compose.animation.core.tween(450), label = "mic1")
    val c2 by androidx.compose.animation.animateColorAsState(cols[2], androidx.compose.animation.core.tween(450), label = "mic2")
    // Waking or reconnecting: a slow fade in and out; asleep: dim.
    val throb = androidx.compose.animation.core.rememberInfiniteTransition(label = "micThrob").animateFloat(0.45f, 1f, androidx.compose.animation.core.infiniteRepeatable(androidx.compose.animation.core.tween(1100), androidx.compose.animation.core.RepeatMode.Reverse), label = "throb")
    val dim = when { tone == MicTone.Offline || phase == Phase.Starting -> throb.value; phase == Phase.Ended -> 0.45f; else -> 1f }
    var frame by remember { mutableLongStateOf(0L) }
    val st = remember { FloatArray(4) }                  // morph (0 sphere .. 1 water), its velocity, level, peak
    val hist = remember { FloatArray(HIST) }
    val drops = remember { ArrayList<Drop>() }
    val ripples = remember { ArrayList<Pair<Float, Long>>() }  // (x offset as a share of the width, born)
    val clock = remember { LongArray(3) }                 // last frame, talking until, last drop

    LaunchedEffect(phase) {
        while (true) withFrameNanos { now ->
            val dt = if (clock[0] == 0L) 0.016f else ((now - clock[0]) / 1e9f).coerceIn(0.001f, 0.05f)
            clock[0] = now
            val raw = if (phase == Phase.Listening) mic().coerceIn(0f, 1f) else 0f
            st[3] = max(st[3] * 0.995f, max(0.1f, raw))
            val level = (raw / st[3]).coerceIn(0f, 1f)
            st[2] += (level - st[2]) * (if (level > st[2]) 0.45f else 0.12f)
            System.arraycopy(hist, 0, hist, 1, HIST - 1)
            hist[0] = st[2]
            if (level > 0.22f) clock[1] = now + 450_000_000L
            val talking = phase == Phase.Listening && now < clock[1]
            // Spread out while talking; surface tension pulls it back, with a wobble.
            val target = if (talking) 1f else 0f
            val k = if (talking) 28f else 42f
            val damp = if (talking) 9f else 5.5f
            st[1] += ((target - st[0]) * k - st[1] * damp) * dt
            st[0] += st[1] * dt
            if (talking && st[2] > 0.55f && now - clock[2] > 140_000_000L) {
                drops += Drop((Random.nextFloat() - 0.5f) * 3f, (Random.nextFloat() - 0.5f) * 60f, -(90f + st[2] * 120f), now)
                clock[2] = now
            }
            if (talking && st[2] > 0.3f && Random.nextFloat() < 0.08f) ripples += ((Random.nextFloat() - 0.5f) * 2.6f) to now
            drops.removeAll { now - it.born > 900_000_000L }
            ripples.removeAll { now - it.second > 900_000_000L }
            frame = now
        }
    }

    Box(
        modifier.size(112.dp)
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, role = Role.Button, onClickLabel = label, onClick = onClick)
            .semantics { contentDescription = label },
        contentAlignment = Alignment.Center,
    ) {
        Canvas(Modifier.fillMaxSize()) {
            val t = frame / 1e9f
            val c = center
            val r = 30.dp.toPx()
            val m = st[0]
            val lv = st[2]
            val px = density   // the preview was drawn in CSS pixels: dp here

            drawCircle(Brush.radialGradient(listOf(c0.copy(alpha = (0.18f + 0.2f * lv) * dim), Color.Transparent), c, r * 3f), r * 3f, c)

            // Ripples on the surface.
            for ((x, born) in ripples) {
                val a = ((frame - born) / 9e8f).coerceIn(0f, 1f)
                val w = (6f + 40f * a) * px
                drawOval(lerp(CrundiColors.Sky, Color.White, 0.3f).copy(alpha = (0.5f * (1f - a) * m).coerceIn(0f, 1f)), Offset(c.x + x * r - w, c.y - r * (1f - 0.62f * m) * 0.35f - w * 0.22f), Size(w * 2, w * 0.44f), style = Stroke(1.2f * px))
            }

            // The liquid: three translucent layers, blended so they glow where they overlap.
            val layers = listOf(c0, c1, c2).map { it to lerp(it, Color.Black, 0.55f) }
            layers.forEachIndexed { l, (top, deep) ->
                val path = shape(c, r, m, t + l * 0.7f, l * 1.3f, hist)
                drawPath(path, Brush.linearGradient(listOf(top, deep), Offset(c.x - r * 2, c.y - r), Offset(c.x + r * 2, c.y + r)), alpha = 0.55f * dim, blendMode = BlendMode.Plus)
            }
            drawPath(shape(c, r, m, t, 0f, hist), Color.White.copy(alpha = 0.22f), style = Stroke(1f * px))

            // The highlight slides with the liquid.
            val hw = r * (0.34f + 0.5f * m); val hh = r * (0.13f - 0.06f * m)
            drawOval(Color.White.copy(alpha = 0.22f * (1f - 0.5f * m)), Offset(c.x - r * 0.25f * (1f - m) - r * 0.6f * m - hw, c.y - r * 0.55f * (1f - m) - r * 0.38f * m - hh), Size(hw * 2, hh * 2))

            // Droplets thrown off the peaks.
            for (d in drops) {
                val a = (frame - d.born) / 1e9f
                val x = c.x + d.x * r + d.vx * a * px
                val y = c.y - r * 0.4f + (d.vy * a + 260f * a * a) * px
                drawCircle(lerp(CrundiColors.Sky, Color.White, 0.3f).copy(alpha = (0.8f * (1f - a)).coerceIn(0f, 1f)), 2.4f * px * (1f - a * 0.5f), Offset(x, y))
            }
        }
        if (icon != null) CrundiIcon(icon, size = 24.dp, tint = Color.White)
    }
}

private const val HIST = 64

/**
 * What the blob's colours say. Ready: by phase (listening sky blue, thinking
 * violet, speaking green, waking indigo, asleep grey). Offline: the live
 * stream is lost and the phone's own recogniser stands in, or it is
 * reconnecting (amber, fading in and out). Problem: the microphone is off or
 * Cru is unavailable (red).
 */
enum class MicTone {
    Ready, Offline, Problem;

    fun colors(phase: Phase): List<Color> = when (this) {
        Problem -> listOf(CrundiColors.Red, Color(0xFFF87171), CrundiColors.Red)
        Offline -> listOf(CrundiColors.Yellow, Color(0xFFFB923C), CrundiColors.Yellow)
        Ready -> when (phase) {
            Phase.Listening -> listOf(CrundiColors.Sky, CrundiColors.Accent, CrundiColors.Ask)
            Phase.Thinking -> listOf(CrundiColors.Ask, CrundiColors.AccentDeep, CrundiColors.Accent)
            Phase.Speaking -> listOf(CrundiColors.Green, CrundiColors.Sky, CrundiColors.Green)
            Phase.Starting -> listOf(CrundiColors.Accent, CrundiColors.AccentDeep, CrundiColors.Sky)
            Phase.Ended -> listOf(CrundiColors.TextMuted, CrundiColors.BgHover, CrundiColors.TextMuted)
        }
    }

    companion object {
        fun of(ui: VoiceUi, phase: Phase): MicTone = when {
            ui.needsMic || ui.blocked.isNotEmpty() -> Problem
            ui.state.offline -> Offline
            else -> Ready
        }
    }
}

private class Drop(val x: Float, val vx: Float, val vy: Float, val born: Long)

/**
 * The liquid's outline: a wobbling sphere at [m] 0, a water surface at 1
 * (wide and low, its top edge the voice from [hist], newest in the middle),
 * each point moving straight between the two.
 */
private fun shape(c: Offset, r: Float, m: Float, t: Float, layer: Float, hist: FloatArray): Path {
    val path = Path()
    val n = 120
    val halfW = r * (1f + 1.9f * m)
    val band = r * (1f - 0.62f * m)
    for (i in 0..n) {
        val a = i / n.toFloat() * (2 * PI).toFloat()
        val wob = r * (0.03f * sin(a * 3 + t * 2 + layer) + 0.02f * sin(a * 5 - t * 3 + layer * 2)) * (1 - m) + r * 0.06f * (1 - m) * sin(a * 2 + t * 1.3f)
        val sx = cos(a) * (r + wob)
        val sy = sin(a) * (r + wob)
        val u = cos(a)
        val k = ((u * 0.5f + 0.5f) * 63).toInt()
        val wave = hist[min(HIST - 1, abs(k - 32))] * r * 0.9f
        val surf = -band * 0.35f - wave * (0.6f + 0.4f * sin(u * 9 + t * 8 + layer)) - r * 0.05f * sin(u * 14 - t * 10 + layer)
        val bx = u * halfW
        val by = if (sin(a) < 0) surf * (1 - abs(u).pow(6)) else band * 0.55f * (1 - abs(u).pow(4)) * (if (sin(a) == 0f) 1f else sign(sin(a)))
        val x = c.x + sx + (bx - sx) * m
        val y = c.y + sy + (by - sy) * m
        if (i == 0) path.moveTo(x, y) else path.lineTo(x, y)
    }
    path.close()
    return path
}
