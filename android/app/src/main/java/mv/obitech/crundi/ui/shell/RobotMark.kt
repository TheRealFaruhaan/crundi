package mv.obitech.crundi.ui.shell

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.keyframes
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import kotlinx.coroutines.delay
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/** How the mark moves (src/brand.js BRAND_CSS): .cb-idle, .cb-intro, .cb-load. */
enum class MarkMotion {
    /** Barely alive: a blink every five seconds. The top bar's. */
    Idle,
    /** Plays once when it first shows: a blink half a second in, then idle. */
    Intro,
    /** While loading: the light at the tip pulses and it blinks faster. */
    Load,
    /** A chat is working: it nods, the tip pulses, it blinks more often (the web's cb-load, on the head). */
    Working,
}

/**
 * The Crundi head (headSvg in src/brand.js), drawn natively so it can move as
 * the web's does: 96 by 96, the indigo gradient head with its eyes cut out,
 * the amber antenna and tip. Still for someone who has turned animations off
 * (and in screenshot tests).
 */
@Composable
fun RobotHead(modifier: Modifier = Modifier, size: Dp = 24.dp, motion: MarkMotion = MarkMotion.Idle) {
    val still = mv.obitech.crundi.ui.components.motionStill()
    if (still) { Canvas(modifier.size(size)) { drawHead(1f, 1f) }; return }

    // Driven step by step rather than as an infinite transition from 1 to 1:
    // that one never ran, and the head sat still. cb-blink: open, shut for a
    // moment at 44% of the period, open again.
    val busy = motion == MarkMotion.Load || motion == MarkMotion.Working
    val eye = remember { Animatable(1f) }
    LaunchedEffect(motion) {
        if (motion == MarkMotion.Intro) delay(500)
        while (true) {
            val period = if (busy) 3200L else 5000L
            if (motion != MarkMotion.Intro) delay((period * 0.40f).toLong())
            eye.animateTo(0.1f, tween((period * 0.04f).toInt()))
            eye.animateTo(1f, tween((period * 0.04f).toInt()))
            delay(if (motion == MarkMotion.Intro) 1900L else (period * 0.52f).toLong())
            if (motion == MarkMotion.Intro) break
        }
        // After the intro's blink, the idle blinking.
        while (true) {
            delay(2000)
            eye.animateTo(0.1f, tween(200)); eye.animateTo(1f, tween(200))
            delay(2600)
        }
    }
    val loop = rememberInfiniteTransition(label = "mark")
    // cb-pulse 1.2s and cb-nod 1.2s while busy.
    val tip by loop.animateFloat(1f, 0.7f, infiniteRepeatable(tween(600, easing = LinearEasing), RepeatMode.Reverse), label = "pulse")
    val nod by loop.animateFloat(0f, -3f, infiniteRepeatable(tween(600), RepeatMode.Reverse), label = "nod")
    val unit = with(androidx.compose.ui.platform.LocalDensity.current) { size.toPx() } / 96f
    Canvas(
        modifier.size(size).graphicsLayer {
            compositingStrategy = CompositingStrategy.Offscreen
            // cb-nod: up 3 units of the 96-unit drawing and back.
            translationY = if (motion == MarkMotion.Working) nod * unit else 0f
        },
    ) {
        drawHead(eye.value, if (busy) tip else 1f)
    }
}

private val indigoLight = Color(0xFF818CF8)
private val indigoDeep = Color(0xFF4F46E5)
private val amber = Color(0xFFFBBF24)

private fun DrawScope.drawHead(eyeScale: Float, tipScale: Float) {
    val u = size.minDimension / 96f
    fun p(x: Float, y: Float) = Offset(x * u, y * u)
    // Antenna and tip.
    drawLine(amber, p(48f, 27f), p(48f, 17f), strokeWidth = 5.5f * u, cap = StrokeCap.Round)
    drawCircle(amber, 6.5f * u * tipScale, p(48f, 13f))
    // The head, indigo light to deep along the diagonal.
    drawRoundRect(
        Brush.linearGradient(listOf(indigoLight, indigoDeep), p(14f, 14f), p(82f, 82f)),
        topLeft = p(11f, 26f), size = Size(74f * u, 57f * u), cornerRadius = CornerRadius(23f * u),
    )
    // The eyes are holes (a mask on the web), squeezed about their own middle to blink.
    for (x in listOf(32f, 53.5f)) {
        val h = 18f * eyeScale
        drawRoundRect(
            Color.Black, topLeft = p(x, 43f + (18f - h) / 2), size = Size(10.5f * u, h * u),
            cornerRadius = CornerRadius(5.2f * u, 5.2f * u * eyeScale.coerceAtLeast(0.2f)), blendMode = BlendMode.Clear,
        )
    }
}
