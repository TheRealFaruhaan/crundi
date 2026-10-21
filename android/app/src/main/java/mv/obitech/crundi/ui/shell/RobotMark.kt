package mv.obitech.crundi.ui.shell

import android.provider.Settings
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalInspectionMode
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
}

/**
 * The Crundi head (headSvg in src/brand.js), drawn natively so it can move as
 * the web's does: 96 by 96, the indigo gradient head with its eyes cut out,
 * the amber antenna and tip. Still for someone who has turned animations off
 * (and in screenshot tests).
 */
@Composable
fun RobotHead(modifier: Modifier = Modifier, size: Dp = 24.dp, motion: MarkMotion = MarkMotion.Idle) {
    val ctx = LocalContext.current
    val still = LocalInspectionMode.current ||
        runCatching { Settings.Global.getFloat(ctx.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f }.getOrDefault(false)
    if (still) { Canvas(modifier.size(size)) { drawHead(1f, 1f) }; return }

    // cb-blink: open until 40%, shut at 44%, open again at 48%.
    val period = if (motion == MarkMotion.Load) 3200 else 5000
    val loop = rememberInfiniteTransition(label = "mark")
    val eye by loop.animateFloat(
        1f, 1f,
        infiniteRepeatable(keyframes {
            durationMillis = period
            1f at (period * 0.40f).toInt()
            0.1f at (period * 0.44f).toInt()
            1f at (period * 0.48f).toInt()
        }),
        label = "blink",
    )
    val tip by loop.animateFloat(
        1f, if (motion == MarkMotion.Load) 0.7f else 1f,
        infiniteRepeatable(tween(600, easing = LinearEasing), RepeatMode.Reverse),
        label = "pulse",
    )
    // cb-intro: one blink 0.5 s in, over 1.4 s, before the idle blinking.
    val intro = remember { Animatable(1f) }
    LaunchedEffect(motion) {
        if (motion == MarkMotion.Intro) intro.animateTo(1f, keyframes {
            durationMillis = 1900
            1f at 500 + (1400 * 0.40f).toInt()
            0.1f at 500 + (1400 * 0.44f).toInt()
            1f at 500 + (1400 * 0.48f).toInt()
        })
    }
    Canvas(modifier.size(size).graphicsLayer(compositingStrategy = CompositingStrategy.Offscreen)) {
        drawHead(minOf(eye, intro.value), tip)
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
