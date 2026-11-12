package mv.obitech.crundi.voice

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.vector.PathNode
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlin.math.PI
import kotlin.math.cos

/**
 * Cru's mark (the owner's sketch, 9 Nov): three bars centred on one line like
 * a sound wave, short, medium and tall, the tall one running into a joined
 * handwritten "Cru", all one stroke, with a dark layer behind and a light
 * edge on top for depth. The web draws the same (crundi-voice.js, cruIcon):
 * viewBox "6 0 150 90", so [height] sets the size and the width is
 * height × 150 / 90. [live]: Cru listens, thinks or talks, and the two short
 * bars pulse (scaleY 1 to 0.45 every 0.9 s, the second 0.15 s later).
 */
@Composable
fun CruIcon(height: Dp = 24.dp, modifier: Modifier = Modifier, live: Boolean = false) {
    val word = remember { CruMark.wordPath() }
    val t = if (live) rememberInfiniteTransition(label = "cru").animateFloat(0f, 1f, infiniteRepeatable(tween(CruMark.PULSE_MS, easing = LinearEasing)), label = "cruT") else null
    Canvas(modifier.size(height * CruMark.WIDTH / CruMark.HEIGHT, height)) {
        val phase = t?.value
        val b1 = if (phase == null) 1f else CruMark.pulse(phase)
        val b2 = if (phase == null) 1f else CruMark.pulse(phase - CruMark.SECOND_DELAY_MS.toFloat() / CruMark.PULSE_MS)
        val k = size.height / CruMark.HEIGHT
        scale(k, k, pivot = Offset.Zero) {
            translate(-CruMark.VIEW_X, 0f) {
                translate(1.6f, 2.4f) { lines(word, b1, b2, CruMark.dark, CruMark.STROKE) }
                lines(word, b1, b2, CruMark.main, CruMark.STROKE)
                translate(-1.2f, -1.4f) { lines(word, b1, b2, SolidWhite, 2.4f, alpha = 0.28f) }
            }
        }
    }
}

private val SolidWhite: Brush = androidx.compose.ui.graphics.SolidColor(Color.White)

/** The bars (the two short ones scaled about their own centres) and the word, in one brush. */
private fun DrawScope.lines(word: Path, b1: Float, b2: Float, brush: Brush, width: Float, alpha: Float = 1f) {
    val st = Stroke(width, cap = StrokeCap.Round, join = StrokeJoin.Round)
    for ((x, half, s) in listOf(Triple(15f, 10f, b1), Triple(28f, 20f, b2), Triple(41f, 35f, 1f))) {
        drawLine(brush, Offset(x, CruMark.MID - half * s), Offset(x, CruMark.MID + half * s), width, StrokeCap.Round, alpha = alpha)
    }
    drawPath(word, brush, alpha = alpha, style = st)
}

/** The mark's geometry, with no drawing in it (CruIconTest). */
object CruMark {
    /** The joined "Cru", from the tall bar's foot (crundi-voice.js CRU_WORD). */
    const val CRU_WORD = "M76.6 24C70.2 12 54.2 14 51 38C48.6 60 57.4 74 68.6 72C76.6 71 81.4 64 84.6 58C87.8 51 90.2 46 92.6 42L92.6 72L92.6 58C95.8 48 100.6 42 107 42C105.4 56 105.4 71 114.2 72C123 73 127.8 60 129.4 42C127.8 58 127.8 72 135.8 72C140.6 72 143.8 67 146.2 62"
    const val VIEW_X = 6f
    const val WIDTH = 150f
    const val HEIGHT = 90f
    /** The bars' shared centre line. */
    const val MID = 45f
    const val STROKE = 9f
    const val PULSE_MS = 900
    const val SECOND_DELAY_MS = 150

    /** The word's commands, parsed. */
    fun wordNodes(): List<PathNode> = PathParser().parsePathString(CRU_WORD).toNodes()

    fun wordPath(): Path = PathParser().parsePathString(CRU_WORD).toPath()

    /** A short bar's height at [phase] (0..1 of a pulse; any value, wrapped): 1 at the ends, 0.45 half way, eased. */
    fun pulse(phase: Float): Float {
        val p = ((phase % 1f) + 1f) % 1f
        val eased = (1f - cos(p * 2f * PI.toFloat())) / 2f
        return 1f - 0.55f * eased
    }

    private fun grad(vararg stops: Pair<Float, Color>) = Brush.linearGradient(*stops, start = Offset(8f, 90f), end = Offset(155f, 0f))
    val main: Brush = grad(0f to Color(0xFF38BDF8), 0.4f to Color(0xFF6366F1), 0.72f to Color(0xFFA78BFA), 1f to Color(0xFFE879F9))
    val dark: Brush = grad(0f to Color(0xFF0C4A6E), 0.45f to Color(0xFF312E81), 1f to Color(0xFF701A75))
}
