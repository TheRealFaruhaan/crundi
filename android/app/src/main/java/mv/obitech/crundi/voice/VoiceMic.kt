package mv.obitech.crundi.voice

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
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
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.sin

/**
 * The mic button as a drop of liquid. While Master talks it flattens like
 * water under the voice, ripples running round its surface and rings
 * spreading out; when they stop, a spring pulls it back into a circle,
 * overshooting a little as liquid does. While Crundi speaks or thinks it is
 * a calm, softly moving circle.
 *
 * [mic] is read every frame (Master's level, 0..1).
 */
@Composable
fun FluidMic(phase: Phase, accent: Color, mic: () -> Float, icon: String, label: String, onClick: () -> Unit, modifier: Modifier = Modifier) {
    var frame by remember { mutableLongStateOf(0L) }
    // The drop's state: how flat (spring), its speed, the smoothed level, and the ripples.
    val st = remember { FloatArray(4) }                  // flat, velocity, level, peak
    val rings = remember { ArrayDeque<Pair<Long, Float>>() } // (born, strength)
    val lastRing = remember { longArrayOf(0L) }
    val lastT = remember { longArrayOf(0L) }

    LaunchedEffect(phase) {
        while (true) withFrameNanos { now ->
            val dt = if (lastT[0] == 0L) 0.016f else ((now - lastT[0]) / 1e9f).coerceIn(0.001f, 0.05f)
            lastT[0] = now
            val raw = if (phase == Phase.Listening) mic().coerceIn(0f, 1f) else 0f
            st[3] = max(st[3] * 0.995f, max(0.1f, raw))
            val level = (raw / st[3]).coerceIn(0f, 1f)
            st[2] += (level - st[2]) * (if (level > st[2]) 0.4f else 0.12f)
            // A damped spring towards flatness = loudness: liquid, with a little overshoot.
            val target = st[2]
            val k = 70f; val damp = 7f
            st[1] += ((target - st[0]) * k - st[1] * damp) * dt
            st[0] += st[1] * dt
            // A ripple ring every so often while Master is talking.
            if (st[2] > 0.25f && now - lastRing[0] > 260_000_000L) { rings.addLast(now to st[2]); lastRing[0] = now }
            while (rings.isNotEmpty() && now - rings.first().first > 1_100_000_000L) rings.removeFirst()
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
            val r = 32.dp.toPx()
            val flat = st[0]
            // Rings spreading out over the "water".
            for ((born, strength) in rings) {
                val age = ((frame - born) / 1.1e9f).coerceIn(0f, 1f)
                val rr = r * (1.05f + 0.75f * age)
                drawOval(
                    accent.copy(alpha = (0.45f * strength * (1f - age)).coerceIn(0f, 1f)),
                    topLeft = Offset(c.x - rr * (1f + 0.35f * flat), c.y - rr * (1f - 0.35f * flat)),
                    size = androidx.compose.ui.geometry.Size(rr * 2 * (1f + 0.35f * flat), rr * 2 * (1f - 0.35f * flat)),
                    style = Stroke(1.6.dp.toPx() * (1f - age * 0.6f)),
                )
            }
            if (phase == Phase.Listening && rings.isEmpty()) {
                // Waiting for a voice: a slow breath round the drop.
                val p = (t % 1.6f) / 1.6f
                drawCircle(accent.copy(alpha = 0.3f * (1f - p)), r * (1.02f + 0.3f * p), c, style = Stroke(1.5.dp.toPx()))
            }
            // The drop: squashed wider and lower as it flattens, its surface rippling.
            val path = Path()
            val n = 72
            for (i in 0..n) {
                val a = i / n.toFloat() * (2 * PI).toFloat()
                val surface = r * (0.07f * flat * sin(a * 7f + t * 11f) + 0.035f * (0.3f + flat) * sin(a * 3f - t * 5f) + 0.012f * sin(a * 4f + t * 2.2f))
                val rx = (r + surface) * (1f + 0.34f * flat)
                val ry = (r + surface) * (1f - 0.40f * flat.coerceAtLeast(-0.3f))
                val x = c.x + cos(a) * rx
                val y = c.y + sin(a) * ry + r * 0.18f * flat     // settles downward as it flattens
                if (i == 0) path.moveTo(x, y) else path.lineTo(x, y)
            }
            path.close()
            drawPath(path, Brush.linearGradient(listOf(lerp(accent, Color.White, 0.15f), accent, lerp(accent, CrundiColors.AccentDeep, 0.6f)), Offset(c.x - r, c.y - r), Offset(c.x + r, c.y + r)))
            drawPath(path, Color.White.copy(alpha = 0.18f), style = Stroke(1.dp.toPx()))
            // A highlight, as on a drop.
            drawOval(Color.White.copy(alpha = 0.16f * (1f - 0.6f * flat.coerceIn(0f, 1f))), topLeft = Offset(c.x - r * 0.55f, c.y - r * (0.72f - 0.3f * flat)), size = androidx.compose.ui.geometry.Size(r * 0.7f, r * 0.32f))
        }
        CrundiIcon(icon, size = 26.dp, tint = Color.White)
    }
}
