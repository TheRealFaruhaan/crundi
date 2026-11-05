package mv.obitech.crundi.voice

import androidx.compose.foundation.Canvas
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.sp
import mv.obitech.crundi.R
import mv.obitech.crundi.ui.components.DotState
import mv.obitech.crundi.ui.shell.FactoryMark
import mv.obitech.crundi.ui.shell.ProjectSummary
import mv.obitech.crundi.ui.theme.CrundiColors
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sin

/** A project as a cell: its state's colour, and its service as a dot inside. */
data class Cell(val name: String, val kind: CellKind, val service: Boolean? = null)

enum class CellKind(val color: Color) {
    Working(CrundiColors.Yellow), Waiting(CrundiColors.Sky), Needs(CrundiColors.AccentHover),
    Error(CrundiColors.Red), Running(CrundiColors.Green), Idle(CrundiColors.TextMuted);

    val active: Boolean get() = this == Working || this == Waiting || this == Needs
}

/** The live project list as cells: the busy ones first, at most [max]. */
fun cellsOf(projects: List<ProjectSummary>, max: Int = 12): List<Cell> {
    val cells = projects.map { p ->
        val kind = when {
            p.state == DotState.NeedsInput || p.factory == FactoryMark.Needs -> CellKind.Needs
            p.state == DotState.Working -> CellKind.Working
            p.state == DotState.Waiting -> CellKind.Waiting
            p.state == DotState.Error -> CellKind.Error
            p.liveService || p.state == DotState.Running || p.factory == FactoryMark.Running -> CellKind.Running
            else -> CellKind.Idle
        }
        Cell(p.name, kind, if (p.hasService) p.liveService else null)
    }
    if (cells.size <= max) return cells
    val rank = { c: Cell -> when (c.kind) { CellKind.Needs, CellKind.Working, CellKind.Waiting -> 0; CellKind.Error -> 1; CellKind.Running -> 2; CellKind.Idle -> 3 } }
    val keep = cells.withIndex().sortedBy { rank(it.value) }.take(max).map { it.index }.toSet()
    return cells.filterIndexed { i, _ -> i in keep }
}

/** What the usage arcs show, 0..1 each; null leaves an arc out. */
data class UsageArcs(val week: Float?, val fiveHour: Float?, val api: Float?, val apiLabel: String = "")

/**
 * The voice screen's field: the Crundi head glowing at the centre, voice
 * lines rippling round it (Master's voice while listening, Crundi's while it
 * speaks), the usage arcs, and every project as a living cell joined to the
 * centre by a tendril, its colour its real state. Sparks run down the
 * tendrils of projects where Claude is busy or waiting.
 *
 * [mic] and [voice] are read each frame, not as state.
 */
@Composable
fun VoiceField(
    phase: Phase,
    a: Color,
    b: Color,
    mic: () -> Float,
    voice: () -> Float,
    cells: List<Cell>,
    usage: UsageArcs,
    modifier: Modifier = Modifier,
) {
    val logo = painterResource(R.drawable.ic_logo)
    val measurer = rememberTextMeasurer()
    val labelStyle = remember { TextStyle(fontSize = 10.5.sp, fontWeight = FontWeight.Medium) }
    val monoStyle = remember { TextStyle(fontSize = 10.sp, fontFamily = FontFamily.Monospace) }
    val labels = remember { HashMap<String, TextLayoutResult>() }
    val smooth = remember { FloatArray(1) }
    var frame by remember { mutableLongStateOf(0L) }

    LaunchedEffect(phase) {
        while (true) {
            withFrameNanos { now ->
                val t = now / 1e9f
                val target = when (phase) {
                    Phase.Listening -> mic()
                    Phase.Speaking -> voice()
                    Phase.Thinking -> 0.08f + 0.05f * sin(t * 3f)
                    Phase.Starting -> 0.05f
                    Phase.Ended -> 0.02f
                }.coerceIn(0f, 1f)
                smooth[0] += (target - smooth[0]) * (if (target > smooth[0]) 0.3f else 0.1f)
                frame = now
            }
        }
    }

    Canvas(modifier) {
        val t = frame / 1e9f
        val lvl = smooth[0]
        val c = Offset(size.width / 2f, size.height / 2f)
        // The head's radius: 38dp, smaller on a small field.
        val L = 38f * density * min(1f, min(size.width, size.height * 0.8f) / (390f * density) * 1.05f)

        // Light from the centre.
        drawRect(Brush.radialGradient(listOf(a.copy(alpha = 0.16f + 0.12f * lvl), Color.Transparent), c, max(size.width, size.height) * 0.75f))

        // ─── cells: positions on an ellipse round the centre, slowly drifting ───
        val n = cells.size
        val rx = size.width / 2f - 46f * density
        val ry = size.height / 2f - 34f * density
        class Placed(val cell: Cell, val i: Int, val x: Float, val y: Float, val r: Float, val ang: Float)
        val placed = cells.mapIndexed { i, cell ->
            val ang = (-PI / 2).toFloat() + i.toFloat() / max(1, n) * (2 * PI).toFloat() + t * 0.025f + 0.06f * sin(t * 0.4f + i)
            val ex = rx + 6f * density * sin(t * 0.5f + i * 1.7f)
            val ey = ry + 6f * density * cos(t * 0.45f + i)
            val r = ((if (cell.kind == CellKind.Idle) 13f else 17f) + (if (cell.service != null) 1.5f else 0f)) * density
            Placed(cell, i, c.x + cos(ang) * ex, c.y + sin(ang) * ey, r, ang)
        }

        // Strands between neighbours.
        for (k in placed.indices) {
            if (placed.size < 3) break
            val p = placed[k]; val q = placed[(k + 1) % placed.size]
            val mid = Offset((p.x + q.x) / 2f, (p.y + q.y) / 2f)
            val ctl = Offset(mid.x + (c.x - mid.x) * 0.25f, mid.y + (c.y - mid.y) * 0.25f)
            val path = Path().apply { moveTo(p.x, p.y); quad(p.x, p.y, ctl.x, ctl.y, q.x, q.y) }
            drawPath(path, CrundiColors.AccentHover.copy(alpha = 0.10f), style = Stroke(1f * density))
        }

        // Tendrils from the centre's membrane to each cell, sparks on busy ones.
        val r0 = L * 2.85f
        for (p in placed) {
            val dx = cos(p.ang); val dy = sin(p.ang); val px = -dy; val py = dx
            val dist = hypot(p.x - c.x, p.y - c.y)
            val p0 = Offset(c.x + dx * r0, c.y + dy * r0)
            val p3 = Offset(c.x + dx * (dist - p.r), c.y + dy * (dist - p.r))
            if (dist - p.r <= r0) continue
            val w = 14f * density * sin(t * 0.8f + p.i)
            val reach = 28f * density
            val p1 = Offset(p0.x + dx * reach + px * w, p0.y + dy * reach + py * w)
            val p2 = Offset(p3.x - dx * reach - px * w, p3.y - dy * reach - py * w)
            val col = p.cell.kind.color
            val idle = p.cell.kind == CellKind.Idle
            val path = Path().apply { moveTo(p0.x, p0.y); cubicTo(p1.x, p1.y, p2.x, p2.y, p3.x, p3.y) }
            drawPath(path, col.copy(alpha = if (idle) 0.12f else 0.22f), style = Stroke(4f * density, cap = StrokeCap.Round))
            drawPath(path, col.copy(alpha = if (idle) 0.25f else 0.55f), style = Stroke(1.2f * density, cap = StrokeCap.Round))
            if (p.cell.kind.active) for (j in 0 until 3) {
                val s = ((t * 0.45f + j / 3f + p.i * 0.13f) % 1f)
                val q = bez(p3, p2, p1, p0, s)
                drawCircle(col.copy(alpha = 0.9f * (1f - s * 0.6f)), 2.2f * density, q)
            }
        }

        // The cells.
        for (p in placed) {
            val col = p.cell.kind.color
            val idle = p.cell.kind == CellKind.Idle
            val pulse = when (p.cell.kind) { CellKind.Working -> 1f + 0.08f * sin(t * 4f + p.i); CellKind.Error -> 1f + 0.05f * sin(t * 8f); else -> 1f }
            val r = p.r * pulse
            val at = Offset(p.x, p.y)
            if (!idle) drawCircle(col.copy(alpha = 0.10f), r * 1.9f, at)
            val membrane = blob(p.x, p.y, r, t, p.i * 2.1f, if (idle) 0.05f else 0.09f)
            drawPath(membrane, Brush.radialGradient(listOf(col.copy(alpha = if (idle) 0.22f else 0.45f), col.copy(alpha = 0.05f)), Offset(p.x - r * 0.3f, p.y - r * 0.3f), r * 1.2f))
            drawPath(membrane, col.copy(alpha = if (idle) 0.45f else 0.9f), style = Stroke(1.4f * density))
            drawCircle(col.copy(alpha = 0.95f), r * 0.28f, Offset(p.x + sin(t * 0.7f + p.i) * 2f * density, p.y + cos(t * 0.6f + p.i) * 2f * density))
            p.cell.service?.let { up ->
                val aa = t * 1.2f + p.i
                drawCircle(if (up) CrundiColors.Green else CrundiColors.Red, 2.2f * density, Offset(p.x + cos(aa) * r * 0.62f, p.y + sin(aa) * r * 0.62f))
            }
            val name = if (p.cell.name.length > 14) p.cell.name.take(13) + "…" else p.cell.name
            val lay = labels.getOrPut(name + idle) { measurer.measure(name, labelStyle.copy(color = if (idle) CrundiColors.TextMuted else Color(0xFFB8B8D0))) }
            drawText(lay, topLeft = Offset(p.x - lay.size.width / 2f, p.y + r + 3f * density))
        }

        // ─── usage arcs round the centre ───
        val arcs = listOfNotNull(
            usage.week?.let { Triple(L * 2.05f, it, CrundiColors.AccentHover to "wk ${(it * 100).toInt()}%") },
            usage.fiveHour?.let { Triple(L * 2.29f, it, CrundiColors.Sky to "5h ${(it * 100).toInt()}%") },
            usage.api?.let { Triple(L * 2.53f, it, CrundiColors.Ask to usage.apiLabel) },
        )
        for ((rad, v, look) in arcs) {
            val (col, label) = look
            val tl = Offset(c.x - rad, c.y - rad); val sz = Size(rad * 2, rad * 2)
            drawArc(CrundiColors.Border.copy(alpha = 0.9f), -90f, 270f, false, tl, sz, style = Stroke(3f * density, cap = StrokeCap.Round))
            drawArc(col.copy(alpha = 0.35f), -90f, max(2f, 270f * v.coerceIn(0f, 1f)), false, tl, sz, style = Stroke(7f * density, cap = StrokeCap.Round))
            drawArc(col, -90f, max(2f, 270f * v.coerceIn(0f, 1f)), false, tl, sz, style = Stroke(3f * density, cap = StrokeCap.Round))
            val lay = labels.getOrPut("u:$label") { measurer.measure(label, monoStyle.copy(color = col.copy(alpha = 0.85f))) }
            drawText(lay, topLeft = Offset(c.x - 6f * density - lay.size.width, c.y - rad - lay.size.height / 2f))
        }

        // ─── voice lines round the head ───
        for (k in 0 until 5) {
            val base = L + (8f + k * 6f + (t * 14f) % 6f) * density
            val amp = (2f + lvl * 12f) * (1f - k * 0.16f) * density
            val m = 5 + k
            val path = Path()
            for (s in 0..96) {
                val an = s / 96f * (2 * PI).toFloat()
                val rr = base + amp * sin(an * m + t * (3 + k) + k) * sin(an * 2 + t * 1.7f)
                val x = c.x + cos(an) * rr; val y = c.y + sin(an) * rr
                if (s == 0) path.moveTo(x, y) else path.lineTo(x, y)
            }
            drawPath(path, (if (k % 2 == 1) b else a).copy(alpha = ((0.65f - k * 0.11f) * (0.35f + lvl * 1.1f)).coerceIn(0f, 1f)), style = Stroke(1.4f * density))
        }

        // Thinking: a comet round the head.
        if (phase == Phase.Thinking) rotate(t * 170f, c) {
            for (s in 0 until 20) drawArc(a.copy(alpha = s / 20f), s * 6.9f, 7f, false, Offset(c.x - L - 20f * density, c.y - L - 20f * density), Size((L + 20f * density) * 2, (L + 20f * density) * 2), style = Stroke(2.4f * density, cap = StrokeCap.Round))
        }

        // The glow and the head.
        drawCircle(Brush.radialGradient(listOf(a.copy(alpha = (0.55f + 0.35f * lvl).coerceAtMost(1f)), b.copy(alpha = 0.18f), Color.Transparent), c, L * 1.9f), L * 1.9f, c)
        val sz = L * 2f * (1f + 0.07f * lvl)
        translate(c.x - sz / 2f, c.y - sz / 2f - 2f * density) { with(logo) { draw(Size(sz, sz)) } }
    }
}

/** A cell's membrane: a circle that breathes, smoothed through its midpoints. */
private fun blob(x: Float, y: Float, r: Float, t: Float, seed: Float, wob: Float): Path {
    val n = 26
    val xs = FloatArray(n); val ys = FloatArray(n)
    for (k in 0 until n) {
        val a = k / n.toFloat() * (2 * PI).toFloat()
        val rr = r * (1 + wob * (0.6f * sin(3 * a + t * 1.3f + seed) + 0.4f * sin(5 * a - t * 0.9f + seed * 2)))
        xs[k] = x + cos(a) * rr; ys[k] = y + sin(a) * rr
    }
    return Path().apply {
        var fx = (xs[n - 1] + xs[0]) / 2f; var fy = (ys[n - 1] + ys[0]) / 2f
        moveTo(fx, fy)
        for (k in 0 until n) {
            val j = (k + 1) % n
            val tx = (xs[k] + xs[j]) / 2f; val ty = (ys[k] + ys[j]) / 2f
            quad(fx, fy, xs[k], ys[k], tx, ty)
            fx = tx; fy = ty
        }
        close()
    }
}

/** A quadratic curve from (x0, y0) as the cubic it equals. */
private fun Path.quad(x0: Float, y0: Float, qx: Float, qy: Float, x: Float, y: Float) =
    cubicTo(x0 + 2f / 3f * (qx - x0), y0 + 2f / 3f * (qy - y0), x + 2f / 3f * (qx - x), y + 2f / 3f * (qy - y), x, y)

private fun bez(p0: Offset, p1: Offset, p2: Offset, p3: Offset, s: Float): Offset {
    val v = 1 - s
    return Offset(
        v * v * v * p0.x + 3 * v * v * s * p1.x + 3 * v * s * s * p2.x + s * s * s * p3.x,
        v * v * v * p0.y + 3 * v * v * s * p1.y + 3 * v * s * s * p2.y + s * s * s * p3.y,
    )
}
