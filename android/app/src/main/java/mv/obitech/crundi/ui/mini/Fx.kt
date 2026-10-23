package mv.obitech.crundi.ui.mini

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.vector.PathParser

/*
 * The small things that appear around Mini and vanish (crundi-mini.js FX):
 * knocks, dust, puffs, z's, notes, a bang, a question mark, a speech bubble,
 * rings, sweat, anger marks, sparks, confetti, a paper plane. All drawn, no
 * fonts; each plays its one short animation (the web's cm-* keyframes) and goes.
 */

internal fun svgPath(d: String): Path = PathParser().parsePathString(d).toPath()

internal val Amber = Color(0xFFFBBF24)
internal val IndigoLight = Color(0xFF818CF8)
internal val IndigoDeep = Color(0xFF4F46E5)
internal val Dark = Color(0xFF0D0D16)
private val Grey = Color(0xFF8888A8)
private val Lilac = Color(0xFFA5B4FC)

private enum class Anim { Knock, Dust, Rise, Pop, Ring, Drop, Fall, Plane }

private class FxDef(val w: Float, val h: Float, val ms: Int, val anim: Anim, val draw: DrawScope.(Color?) -> Unit)

private fun DrawScope.stroke(p: Path, c: Color, w: Float, round: Boolean = true) =
    drawPath(p, c, style = Stroke(w, cap = StrokeCap.Round, join = if (round) StrokeJoin.Round else StrokeJoin.Miter))

private val knockP by lazy { svgPath("M11 2V7M3 6L7 9M19 6L15 9") }
private val zP by lazy { svgPath("M2 2H10L2 10H10") }
private val noteP by lazy { svgPath("M4 12V3L10 1.5V10") }
private val bangP by lazy { svgPath("M4 2V10") }
private val askP by lazy { svgPath("M3 6C3 1 11 1 11 6C11 9 7 9 7 13") }
private val dotsP by lazy { svgPath("M4 1H30A3 3 0 0 1 33 4V13A3 3 0 0 1 30 16H14L9 21V16H4A3 3 0 0 1 1 13V4A3 3 0 0 1 4 1Z") }
private val sweatP by lazy { svgPath("M4 1C6 4 7 5.5 7 7.5A3 3 0 0 1 1 7.5C1 5.5 2 4 4 1Z") }
private val angerP by lazy { svgPath("M6 2V6H2M10 2V6H14M6 14V10H2M10 14V10H14") }
private val sparkP by lazy { svgPath("M7 0L8.6 5.4L14 7L8.6 8.6L7 14L5.4 8.6L0 7L5.4 5.4Z") }
private val planeP by lazy { svgPath("M1 6L17 1L11 13L8 8Z") }
private val planeLineP by lazy { svgPath("M8 8L17 1") }

private val FX: Map<String, FxDef> = mapOf(
    "knock" to FxDef(22f, 22f, 420, Anim.Knock) { stroke(knockP, Amber, 2.4f) },
    "dust" to FxDef(12f, 12f, 520, Anim.Dust) { drawCircle(Grey, 5f, Offset(6f, 6f)) },
    "puff" to FxDef(14f, 10f, 900, Anim.Rise) { drawOval(Grey.copy(alpha = 0.7f), Offset(1f, 1f), Size(12f, 8f)) },
    "z" to FxDef(12f, 12f, 1700, Anim.Rise) { stroke(zP, Lilac, 2.2f) },
    "note" to FxDef(12f, 16f, 1500, Anim.Rise) {
        stroke(noteP, Amber, 2f); drawCircle(Amber, 2.4f, Offset(2.6f, 12f)); drawCircle(Amber, 2.4f, Offset(8.6f, 10f))
    },
    "bang" to FxDef(8f, 18f, 900, Anim.Pop) { stroke(bangP, Amber, 3.4f); drawCircle(Amber, 2f, Offset(4f, 15.5f)) },
    "ask" to FxDef(14f, 20f, 1500, Anim.Pop) { stroke(askP, Amber, 3f); drawCircle(Amber, 1.9f, Offset(7f, 17.6f)) },
    "dots" to FxDef(34f, 22f, 2300, Anim.Pop) {
        drawPath(dotsP, Color(0xFF1A1A28)); drawPath(dotsP, Color(0xFF3A3A55), style = Stroke(1.2f))
        for (cx in listOf(10f, 17f, 24f)) drawCircle(Lilac, 2.1f, Offset(cx, 8.5f))
    },
    "ring" to FxDef(20f, 20f, 520, Anim.Ring) { drawCircle(IndigoLight, 8f, Offset(10f, 10f), style = Stroke(2f)) },
    "sweat" to FxDef(8f, 11f, 700, Anim.Drop) { drawPath(sweatP, Color(0xFF7DD3FC)) },
    "anger" to FxDef(16f, 16f, 900, Anim.Pop) { stroke(angerP, Color(0xFFFB7185), 2.4f) },
    "spark" to FxDef(14f, 14f, 700, Anim.Pop) { drawPath(sparkP, Amber) },
    "confetti" to FxDef(6f, 9f, 1300, Anim.Fall) { c -> drawRoundRect(c ?: IndigoLight, size = Size(6f, 9f), cornerRadius = CornerRadius(1.5f)) },
    "plane" to FxDef(18f, 14f, 1100, Anim.Plane) { drawPath(planeP, Color(0xFFE8E8F0)); drawPath(planeLineP, Grey, style = Stroke(1f)) },
)

/** CSS ease-out, applied to each stretch between keyframes as the browser does. */
private val easeOut = CubicBezierEasing(0f, 0f, 0.58f, 1f)

/** A keyframe track: pairs of (offset, value). */
private fun kf(p: Float, vararg pts: Float): Float {
    if (p <= pts[0]) return pts[1]
    var i = 0
    while (i + 2 < pts.size) {
        val o0 = pts[i]; val v0 = pts[i + 1]; val o1 = pts[i + 2]; val v1 = pts[i + 3]
        if (p <= o1) { val f = if (o1 > o0) easeOut.transform((p - o0) / (o1 - o0)) else 1f; return v0 + (v1 - v0) * f }
        i += 2
    }
    return pts[pts.size - 1]
}

private class Fx(val def: FxDef, val cx: Float, val cy: Float, val dx: Float?, val dy: Float?, val color: Color?, val clip: Box?) {
    var born = Float.NaN
}

/** The effects on screen, in the layer's dp, on the layer's clock. */
class FxLayer {
    private val list = ArrayList<Fx>()
    /** Seconds, advanced by the frame loop. */
    var now = 0f
    var onAdd: (() -> Unit)? = null
    val isEmpty get() = list.isEmpty()

    fun add(name: String, cx: Float, cy: Float, dx: Float?, dy: Float?, color: Long?, clip: Box?) {
        val def = FX[name] ?: return
        list.add(Fx(def, cx, cy, dx, dy, color?.let { Color(it) }, clip))
        onAdd?.invoke()
    }

    fun clear() = list.clear()

    /** Draw them all and drop the finished ones. [density] turns dp into px. */
    fun draw(scope: DrawScope, density: Float) = with(scope) {
        val it = list.iterator()
        while (it.hasNext()) {
            val f = it.next()
            if (f.born.isNaN()) f.born = now
            val p = (now - f.born) * 1000f / f.def.ms
            if (p >= 1f) { it.remove(); continue }
            var o = 1f; var tx = 0f; var ty = 0f; var s = 1f; var rot = 0f
            when (f.def.anim) {
                Anim.Knock -> { o = kf(p, 0f, 0f, .25f, 1f, 1f, 0f); s = kf(p, 0f, .3f, 1f, 1.5f) }
                Anim.Dust -> { o = kf(p, 0f, .55f, 1f, 0f); tx = kf(p, 0f, 0f, 1f, f.dx ?: 0f); ty = kf(p, 0f, 0f, 1f, -9f); s = kf(p, 0f, .4f, 1f, 1.5f) }
                Anim.Rise -> { o = kf(p, 0f, 0f, .2f, 1f, 1f, 0f); tx = kf(p, 0f, 0f, 1f, f.dx ?: 6f); ty = kf(p, 0f, 4f, 1f, -26f); s = kf(p, 0f, .6f, 1f, 1.1f) }
                Anim.Pop -> { o = kf(p, 0f, 0f, .18f, 1f, .3f, 1f, .78f, 1f, 1f, 0f); s = kf(p, 0f, .2f, .18f, 1.15f, .3f, 1f, .78f, 1f, 1f, .8f) }
                Anim.Ring -> { o = kf(p, 0f, .9f, 1f, 0f); s = kf(p, 0f, .3f, 1f, 1.6f) }
                Anim.Drop -> { o = kf(p, 0f, 0f, .2f, 1f, 1f, 0f); ty = kf(p, 0f, -3f, 1f, 14f) }
                Anim.Fall -> { o = kf(p, 0f, 0f, .15f, 1f, 1f, 0f); tx = kf(p, 0f, 0f, 1f, f.dx ?: 6f); ty = kf(p, 0f, -6f, 1f, 26f); rot = kf(p, 0f, 0f, 1f, 260f) }
                Anim.Plane -> {
                    o = kf(p, 0f, 0f, .12f, 1f, 1f, 0f); tx = kf(p, 0f, 0f, 1f, f.dx ?: -60f); ty = kf(p, 0f, 0f, 1f, f.dy ?: -110f)
                    rot = kf(p, 0f, -20f, 1f, -34f); s = kf(p, 0f, .6f, 1f, 1f)
                }
            }
            val c = f.clip
            val body: DrawScope.() -> Unit = {
                translate((f.cx + tx) * density, (f.cy + ty) * density) {
                    rotate(rot, Offset.Zero) {
                        scale(s * density, s * density, Offset.Zero) {
                            translate(-f.def.w / 2, -f.def.h / 2) {
                                // Opacity by drawing into a faded layer, so overlapping strokes do not darken.
                                drawContext.canvas.saveLayer(androidx.compose.ui.geometry.Rect(-4f, -4f, f.def.w + 4, f.def.h + 4), androidx.compose.ui.graphics.Paint().apply { alpha = o.coerceIn(0f, 1f) })
                                f.def.draw(this, f.color)
                                drawContext.canvas.restore()
                            }
                        }
                    }
                }
            }
            if (c != null) clipRect(c.l * density, c.t * density, c.r * density, c.b * density) { body() } else body()
        }
    }
}
