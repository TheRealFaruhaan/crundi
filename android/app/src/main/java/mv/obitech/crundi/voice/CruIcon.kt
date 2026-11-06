package mv.obitech.crundi.voice

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.sin

/**
 * Cru's mark: the voice screen's contours (the same formula as [VoiceField],
 * frozen at one moment of speech), fourteen hairlines round a dark centre,
 * sky to violet to pink. The web draws the same (crundi-voice.js, cruIcon).
 */
@Composable
fun CruIcon(size: Dp = 24.dp, modifier: Modifier = Modifier) {
    val rings = remember { cruRings() }
    Canvas(modifier.size(size)) {
        val k = this.size.minDimension / 24f
        val brush = Brush.linearGradient(
            0f to Color(0xFF38BDF8), 0.45f to Color(0xFF6366F1), 0.75f to Color(0xFFA78BFA), 1f to Color(0xFFE879F9),
            start = Offset.Zero, end = Offset(24f, 24f),
        )
        scale(k, k, pivot = Offset.Zero) {
            for ((path, f) in rings) drawPath(path, brush, alpha = max(0.15f, 1f - f * 0.7f), style = Stroke(0.32f))
            drawCircle(Color(0xFF05050A), 2.6f, Offset(12f, 12f))
            drawCircle(brush, 2.6f, Offset(12f, 12f), style = Stroke(0.58f))
        }
    }
}

/** The rings in a 24 x 24 box, each with how far out it is (0..1). */
private fun cruRings(): List<Pair<Path, Float>> {
    val lines = 14; val disc = 2.6f; val gap = 0.5f; val swell = 2.4f; val seed = 1.9f; val points = 96
    return (0 until lines).map { i ->
        val f = (i + 1f) / lines
        val base = disc + 0.5f + i * gap
        val amp = swell * f * f + 0.35f * f
        val p = Path()
        for (s in 0..points) {
            val an = s / points.toFloat() * (2 * PI).toFloat()
            val frac = abs(s / points.toFloat() * 2f - 1f)
            val v = 0.5f + 0.5f * sin((1 - frac) * 7f + seed) * cos((1 - frac) * 3.1f)
            val sh = 0.55f * sin(an * 3f + seed + f * 1.2f) + 0.30f * sin(an * 5f - seed * 0.7f + f * 2.1f) + 0.15f * sin(an * 2f + seed * 1.3f)
            val rr = base + amp * (0.35f + 0.65f * v) * (0.6f + 0.4f * sh) + amp * 0.5f * sh
            val x = 12f + cos(an - (PI / 2).toFloat()) * rr; val y = 12f + sin(an - (PI / 2).toFloat()) * rr
            if (s == 0) p.moveTo(x, y) else p.lineTo(x, y)
        }
        p.close()
        p to f
    }
}
