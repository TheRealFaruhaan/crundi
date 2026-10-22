package mv.obitech.crundi.ui.mini

import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Paint
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.drawscope.withTransform
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sin
import kotlin.math.sqrt

/*
 * The drawing (crundi-mini.js markup and draw): the logo's head, a body, two
 * arms and two legs, in a 96 by 92 box whose feet are at y 90. Each part is
 * drawn under the same transforms the web gives its SVG group, nested in the
 * same order, so a pose looks the same on both.
 */

// The gradient runs across the drawing's own units, as the SVG's userSpaceOnUse one does.
private val bodyBrush = Brush.linearGradient(listOf(IndigoLight, IndigoDeep), Offset(14f, 10f), Offset(82f, 84f))
private val chuteLines by lazy { svgPath("M12 -8L33 56M36 -6L40 54M60 -6L56 54M84 -8L63 56") }
private val chuteCanopy by lazy { svgPath("M6 -8Q48 -66 90 -8Q78 -17 69 -8Q58 -17 48 -8Q38 -17 27 -8Q18 -17 6 -8Z") }
private val chutePanels by lazy { svgPath("M27 -8Q30 -44 48 -51Q38 -30 48 -8Q38 -17 27 -8ZM69 -8Q66 -44 48 -51Q58 -30 48 -8Q58 -17 69 -8Z") }
private val backLines by lazy { svgPath("M41 62H55M41 67H55M41 72H55") }
private val headClip by lazy { Path().apply { addRoundRect(RoundRect(23f, 12f, 73f, 49f, CornerRadius(15f))) } }

/** Grow a part from a point: nothing when [f] is 1, as the web skips the transform then. */
private inline fun DrawScope.grown(px: Float, py: Float, f: Float, block: DrawScope.() -> Unit) {
    if (f == 1f) block() else scale(max(0.0001f, f), max(0.0001f, f), Offset(px, py), block)
}

/** Draw [a] on this canvas; the layer is in dp, so [density] turns its numbers into pixels. */
fun DrawScope.drawActor(a: Actor, density: Float) {
    val v = a.v
    fun g(p: P) = v[p.ordinal]
    val k = a.k
    val alpha = clamp(g(P.Alpha), 0f, 1f)
    if (alpha <= 0.002f) return

    val gait = sin(a.phase); val w = a.walk
    val run = clamp((abs(a.vx) - 90) / 80, 0f, 1f)
    val sit = clamp(g(P.Sit), 0f, 1f)
    val sw = g(P.Swing); val tm = a.time
    val shake = g(P.Shake) * sin(tm * 34) * 15

    // A step: the leg swings from the hip, the foot going forward is picked up,
    // the body rises twice a stride and rolls a little, the head follows late.
    val amp = (30 + run * 14) * w; val cosg = cos(a.phase)
    val legL = g(P.LegL) + gait * amp + sin(tm * 5.2f) * 24 * sw
    val legR = g(P.LegR) - gait * amp + sin(tm * 5.2f + 2.5f) * 24 * sw
    // The foot to lift is the one travelling forwards, which depends on the way it goes.
    val way = clamp(a.vx / 18, -1f, 1f)
    val liftL = max(0f, -way * cosg) * 3.4f * w; val liftR = max(0f, way * cosg) * 3.4f * w
    val swingA = 24 * w * clamp(g(P.ArmSwing), 0f, 1.4f) * (1 - clamp(g(P.Shake), 0f, 1f))
    var armL = g(P.ArmL) - gait * swingA + shake
    var armR = g(P.ArmR) + gait * swingA - shake
    // Hanging from the lines: both hands up.
    val ch = clamp(g(P.Chute), 0f, 1.15f); val chm = min(1f, ch)
    armL = armL * (1 - chm) + 152 * chm; armR = armR * (1 - chm) + 152 * chm
    val rise = 0.5f - 0.5f * cos(a.phase * 2)
    val bob = -rise * (2.3f + run * 2) * w + (if (a.air) 0f else sin(tm * 2.1f) * 0.5f)
    val roll = gait * 1.7f * w
    val headLag = sin(a.phase * 2 - 0.9f) * 0.9f * w
    val lean = g(P.Lean) + clamp(a.vx / 22, -11f, 11f) + roll
    val sq = clamp(g(P.Squash), 0.55f, 1.4f); val sx = 1 / sqrt(sq)
    val drop = sit * SIT_DROP
    // Growing out of the logo: the body first, then the arms, then the legs.
    val gr = g(P.Grow)
    var gB = clamp(gr * 2.2f, 0f, 1.12f); var gA = clamp(gr * 2.2f - 0.6f, 0f, 1.12f); var gL = clamp(gr * 2.2f - 1.2f, 0f, 1.12f)
    if (gr > 0.985f && gr < 1.015f) { gB = 1f; gA = 1f; gL = 1f }
    // Hands behind the back, and both hands reaching to one side.
    val ab = clamp(g(P.ArmsBack), 0f, 1f); val behind = ab > 0.1f
    val reach = g(P.Reach)
    val rcA = clamp(abs(reach), 0f, 1f); val rcX = 48 + (if (reach < 0) -9 else 9)
    val rLx = rcA * (rcX - 23.5f); val rRx = rcA * (rcX - 72.5f); val rRy = rcA * 5
    val turn = g(P.Turn); val tc = clamp(turn, -1f, 1f)
    val lid = clamp(g(P.Lid) + (if (a.blink > 0) 1f else 0f), -0.35f, 0.94f); val open = 1 - lid
    val nearL = 1 - clamp(-tc, 0f, 1f) * 0.4f; val nearR = 1 - clamp(tc, 0f, 1f) * 0.4f
    val backOp = clamp((abs(turn) - 1.25f) / 0.5f, 0f, 1f)
    val lightOp = 0.6f * (1 - clamp((abs(turn) - 1.1f) / 0.4f, 0f, 1f))
    val antC = clamp(a.ant, -1.3f, 1.3f); val ax = 48 + antC * 7; val ay = 5 + abs(antC) * 1.6f
    val lift = clamp(-a.yOff / 40, 0f, 1f)
    val shadowOp = 0.32f * (1 - sit) * (1 - lift * 0.7f) * clamp(gL, 0f, 1f)

    val u = k * density
    val clip = a.clip?.invoke()
    val body: DrawScope.() -> Unit = {
        translate((a.x - 48 * k) * density, (a.baseY + a.yOff - GROUND * k) * density) {
            scale(u, u, Offset.Zero) {
                // One faded layer for the whole robot, so its parts do not show through each other.
                drawContext.canvas.saveLayer(Rect(-20f, -80f, 116f, 100f), Paint().apply { this.alpha = alpha })

                withTransform({ translate(0f, -a.yOff / k); scale(1 - lift * 0.4f, 1 - lift * 0.4f, Offset(48f, 90f)) }) {
                    drawOval(Color.Black.copy(alpha = shadowOp), Offset(27f, 86.8f), Size(42f, 6.4f))
                }
                withTransform({ translate(0f, drop); rotate(lean, Offset(48f, GROUND)); scale(sx, sq, Offset(48f, GROUND)) }) {
                    if (ch > 0.001f) {
                        val s = max(0.01f, ch); val op = clamp(ch * 3, 0f, 1f)
                        scale(s, s, Offset(48f, 54f)) {
                            drawPath(chuteLines, Color(0xFFC7C9E8).copy(alpha = op), style = Stroke(1.4f))
                            drawPath(chuteCanopy, Amber.copy(alpha = op))
                            drawPath(chutePanels, IndigoLight.copy(alpha = op))
                        }
                    }
                    fun leg(x0: Float, lift0: Float, ang: Float, px: Float) {
                        withTransform({ translate(0f, -lift0); rotate(ang, Offset(px, 78f)) }) {
                            grown(px, 78f, gL) { drawRoundRect(Amber, Offset(x0, 77f), Size(9f, 13f), CornerRadius(4.2f)) }
                        }
                    }
                    leg(36f, liftL, legL, 40.5f)
                    leg(51f, liftR, legR, 55.5f)
                    translate(0f, bob) {
                        fun arms() {
                            withTransform({ translate(ab * 7.5f + rLx, ab * 2.5f); rotate(armL - ab * 24, Offset(23.5f, 58f)) }) {
                                grown(23.5f, 58f, gA) { drawRoundRect(Amber, Offset(20f, 56f), Size(7f, 16f), CornerRadius(3.5f)) }
                            }
                            withTransform({ translate(-ab * 7.5f + rRx, ab * 2.5f + rRy); rotate(-(armR - ab * 24), Offset(72.5f, 58f)) }) {
                                grown(72.5f, 58f, gA) { drawRoundRect(Amber, Offset(69f, 56f), Size(7f, 16f), CornerRadius(3.5f)) }
                            }
                        }
                        // Behind the back the arms are drawn before the body, so only the elbows show.
                        if (behind) arms()
                        grown(48f, 51f, gB) {
                            drawRoundRect(bodyBrush, Offset(32f, 54f), Size(32f, 24f), CornerRadius(9f))
                            drawCircle(Dark.copy(alpha = lightOp), 4.8f, Offset(48f, 66f))
                            if (backOp > 0) drawPath(backLines, Dark.copy(alpha = 0.4f * backOp), style = Stroke(2.2f, cap = StrokeCap.Round))
                        }
                        withTransform({ translate(tc * 2.6f, g(P.HeadY) + headLag); rotate(g(P.HeadRot), Offset(48f, 50f)) }) {
                            val ant = Path().apply { moveTo(48f, 13f); quadraticTo(48f, 9f, ax, ay + 1) }
                            drawPath(ant, Amber, style = Stroke(4f, cap = StrokeCap.Round))
                            drawCircle(Amber, 4.5f, Offset(ax, ay))
                            drawRoundRect(bodyBrush, Offset(23f, 12f), Size(50f, 37f), CornerRadius(15f))
                            clipPath(headClip) {
                                translate(turn * 15 + g(P.EyeX), g(P.EyeY)) {
                                    scale(nearL, open, Offset(40.25f, 30.5f)) { drawRoundRect(Dark, Offset(36.5f, 24f), Size(7.5f, 13f), CornerRadius(3.7f)) }
                                    scale(nearR, open, Offset(55.75f, 30.5f)) { drawRoundRect(Dark, Offset(52f, 24f), Size(7.5f, 13f), CornerRadius(3.7f)) }
                                }
                            }
                        }
                        if (!behind) arms()
                    }
                }
                drawContext.canvas.restore()
            }
        }
    }
    if (clip != null) clipRect(clip.l * density, clip.t * density, clip.r * density, clip.b * density) { body() } else body()
}
