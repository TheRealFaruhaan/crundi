package mv.obitech.crundi.ui.mini

import kotlinx.coroutines.CancellableContinuation
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlin.coroutines.resume
import kotlin.math.abs
import kotlin.math.min
import kotlin.math.sin
import kotlin.math.sqrt

/*
 * Mini, alive: the port of app/vendor/crundi-mini.js. Its header says it best,
 * and it holds here too: every part of the body is a number on a spring, and a
 * behaviour only ever says where those numbers should go next. So whatever
 * Mini is doing when something changes, it carries on from exactly that pose.
 *
 * Everything is in dp, so the web's numbers (CSS pixels) carry over as they are.
 */

/** The parts of the body, each a spring: stiffness, damping (lower is bouncier), and where it rests. */
enum class P(val stiff: Float, val damp: Float, val rest: Float) {
    Turn(130f, 21f, 0f), Lean(150f, 22f, 0f), Squash(330f, 13f, 1f), HeadRot(160f, 19f, 0f), HeadY(170f, 20f, 0f),
    EyeX(230f, 26f, 0f), EyeY(230f, 26f, 0f), Lid(420f, 34f, 0f), ArmL(165f, 16f, 0f), ArmR(165f, 16f, 0f),
    LegL(260f, 24f, 0f), LegR(260f, 24f, 0f), Sit(95f, 17f, 0f), Swing(50f, 13f, 0f), Shake(70f, 15f, 0f),
    Alpha(60f, 15f, 1f), ArmSwing(90f, 18f, 1f), ArmsBack(110f, 19f, 0f), Grow(120f, 12f, 1f), Reach(150f, 20f, 0f),
    Chute(150f, 13f, 0f),
}

/** Something to stand on: its top, and how far along it the feet may go. */
data class Surf(val y: Float, val l: Float, val r: Float)

/** A point to point at or ring. */
data class Pt(val x: Float, val y: Float)

/** The width of the place Mini is in: where it comes in from and leaves to. */
data class Span(val l: Float, val r: Float) { val mid get() = (l + r) / 2 }

const val GROUND = 90f      // feet, in the drawing's own units
const val SIT_DROP = 12f    // how far the body sinks to rest on an edge
const val GRAVITY = 1500f   // dp per second squared

internal fun clamp(v: Float, a: Float, b: Float) = if (v < a) a else if (v > b) b else v
internal fun rnd(a: Number, b: Number): Float = a.toFloat() + (Math.random() * (b.toFloat() - a.toFloat())).toFloat()
internal fun chance(p: Double) = Math.random() < p

/**
 * What to do next, without doing the same thing over and over: every choice
 * is dealt from a shuffled pack, each once before any comes round again, and
 * never the same one twice running across a reshuffle. Packs last as long as
 * the process, so a scene that starts again does not open the way it did.
 */
internal object Packs {
    private class Pack(val q: ArrayDeque<Int>, var last: Int, val n: Int)
    private val packs = HashMap<String, Pack>()
    fun deal(key: String, n: Int): Int {
        var p = packs[key]
        if (p == null || p.q.isEmpty() || p.n != n) {
            val last = if (p != null && p.n == n) p.last else -1
            val q = (0 until n).shuffled().toMutableList()
            if (n > 1 && q.last() == last) { q[q.size - 1] = q[0]; q[0] = last }
            p = Pack(ArrayDeque(q), last, n)
            packs[key] = p
        }
        p.last = p.q.removeLast()
        return p.last
    }
}

/**
 * The body on springs, and where it is. [size] is its height in dp; the
 * drawing is 96 by 92 units. [fx] is where its little effects go.
 */
class Actor(size: Float, private val fxLayer: FxLayer? = null) {
    val k = size / 92f
    private val n = P.entries.size
    val v = FloatArray(n) { P.entries[it].rest }
    val t = FloatArray(n) { P.entries[it].rest }
    val vel = FloatArray(n)

    var x = 0f; var vx = 0f; var tx: Float? = null; var speed = 60f
    var baseY = 0f; var yOff = 0f; var vy = 0f; var air = false
    var surf: (() -> Surf?)? = null
    var clampX = true
    var phase = 0f; var walk = 0f; var time = 0f; var ant = 0f; var antV = 0f
    var nextBlink = 1.5f; var blink = 0f; var autoBlink = true
    var chuting = false
    /** Holding on to something someone else is moving: where its hands are. */
    var carry: (() -> Float)? = null
    var lastSpeed = 0f
    /** It tried everything, slept and woke: the scene is over. */
    var finished = false
    var dead = false
    /** Drawn only inside this box (the pane it is in), as the web's layer clips it. */
    var clip: (() -> Box?)? = null
    /** A tap on it; null for the one in the top bar, which takes no taps. */
    var onTap: (() -> Unit)? = null

    private var onArrive: (() -> Unit)? = null
    private var onLand: (() -> Unit)? = null
    private class Waiter(val at: Float, val c: CancellableContinuation<Unit>)
    private val waits = ArrayList<Waiter>()
    private var job: Job? = null

    init { v[P.Alpha.ordinal] = 0f; t[P.Alpha.ordinal] = 0f }

    operator fun FloatArray.get(p: P) = this[p.ordinal]
    operator fun FloatArray.set(p: P, value: Float) { this[p.ordinal] = value }

    /** Put every part back where it rests, at once (coming in fresh). */
    fun resetPose() {
        for (p in P.entries) { v[p] = p.rest; t[p] = p.rest; vel[p] = 0f }
    }

    fun destroy() {
        dead = true
        job?.cancel()
        waits.toList().forEach { it.c.cancel() }
        waits.clear()
    }

    /** One step of the clock: springs, walking, falling, blinking, the antenna. */
    fun step(dt: Float) {
        time += dt
        if (waits.isNotEmpty()) {
            val due = waits.filter { time >= it.at }
            waits.removeAll(due)
            due.forEach { if (it.c.isActive) it.c.resume(Unit) }
        }

        for (p in P.entries) {
            val i = p.ordinal
            vel[i] += ((t[i] - v[i]) * p.stiff - vel[i] * p.damp) * dt
            v[i] += vel[i] * dt
        }

        // Where the ground is. The ground rising (a card growing) carries it up;
        // the ground dropping by more than a step is a fall of that height.
        val sf = surf?.invoke()
        if (sf != null) {
            if (!air && sf.y - baseY > 10 && v[P.Alpha] > 0.5f) { yOff = baseY - sf.y; baseY = sf.y; air = true; vy = 0f }
            baseY += (sf.y - baseY) * min(1f, dt * 18)
            if (clampX) tx?.let { tx = clamp(it, sf.l, sf.r) }
        }

        // Walking: ease the speed in and out, stop on the mark.
        var want = 0f
        tx?.let { goal ->
            val d = goal - x
            want = clamp(d * 5, -speed, speed)
            if (abs(d) < 1.2f && abs(vx) < 14) {
                x = goal; tx = null; want = 0f
                onArrive?.let { f -> onArrive = null; f() }
            }
        }
        vx += (want - vx) * min(1f, dt * 9)
        x += vx * dt
        carry?.let { vx = 0f; x = it() }
        val sp = abs(vx)
        walk += (clamp(sp / 34, 0f, 1f) - walk) * min(1f, dt * 10)
        // About one stride to the length of a step, so the feet do not slide.
        phase += sp * dt * 0.165f / k

        if (air) {
            vy += GRAVITY * dt
            // A long way down: after a moment of plain falling, the parachute.
            if (vy > 0 && !chuting && -yOff > 2.3f * 92 * k && vy > 190) { chuting = true; t[P.Chute] = 1f; vel[P.Squash.ordinal] += 2.5f; vy *= 0.35f }
            if (chuting) {
                vy = min(vy, 62 + 26 * sin(time * 3.1f))
                x += sin(time * 2.3f) * 13 * dt
                t[P.Lean] = sin(time * 2.3f + 1.2f) * 9
            }
            yOff += vy * dt
            if (yOff >= 0 && vy > 0) {
                val hit = vy
                yOff = 0f; vy = 0f; air = false
                if (chuting) { chuting = false; t[P.Chute] = 0f; t[P.Lean] = 0f }
                vel[P.Squash.ordinal] -= clamp(hit / 95, 1.2f, 7f)      // the landing squashes it
                if (hit > 260) { fx("dust", 30, GROUND - 3, vx = -9); fx("dust", 66, GROUND - 3, vx = 9) }
                onLand?.let { f -> onLand = null; f() }
            }
        }

        if (autoBlink) {
            nextBlink -= dt
            if (nextBlink <= 0) { blink = 0.13f; nextBlink = rnd(1.8, 4.6) }
        }
        if (blink > 0) blink -= dt

        // The antenna trails behind whatever the head does.
        val drive = -vx * 0.012f - vel[P.HeadRot.ordinal] * 0.006f - vel[P.Turn.ordinal] * 0.5f + (if (air) -vy * 0.0016f else 0f)
        antV += ((drive - ant) * 210 - antV * 11) * dt
        ant += antV * dt
    }

    // ─── What a behaviour is given to work with ───

    /**
     * Start a behaviour, dropping whatever it was doing: the old one is
     * cancelled where it stands, and the body carries on from that pose.
     */
    fun play(scope: CoroutineScope, block: suspend Actor.() -> Unit): Job {
        job?.cancel()
        onArrive = null; onLand = null
        val j = scope.launch {
            try { block() } catch (e: CancellationException) { throw e } catch (e: Exception) { android.util.Log.w("mini", "behaviour failed", e) }
        }
        job = j
        return j
    }

    suspend fun wait(ms: Number) {
        if (dead) throw CancellationException("gone")
        suspendCancellableCoroutine { c ->
            val w = Waiter(time + ms.toFloat() / 1000f, c)
            waits.add(w)
            c.invokeOnCancellation { waits.remove(w) }
        }
    }

    fun pose(vararg o: Pair<P, Number>) { for ((p, value) in o) t[p] = value.toFloat() }
    fun pose(o: Map<P, Number>) { for ((p, value) in o) t[p] = value.toFloat() }

    /** Everything back to rest, except what is kept and what is not a pose (alpha, sit, grow, chute). */
    fun rest(vararg keep: P) {
        for (p in P.entries) if (p != P.Alpha && p != P.Sit && p != P.Grow && p != P.Chute && p !in keep) t[p] = p.rest
    }

    /** Set off for [to]; the answer completes on arrival. Nothing walks sitting down. */
    fun go(to: Float, speed: Number = 55): CompletableDeferred<Unit> {
        t[P.Sit] = 0f; t[P.Swing] = 0f
        this.speed = speed.toFloat(); tx = to; lastSpeed = this.speed
        t[P.Turn] = if (to > x) 0.8f else -0.8f
        val d = CompletableDeferred<Unit>()
        onArrive = { t[P.Turn] = 0f; d.complete(Unit) }
        return d
    }

    suspend fun walkTo(to: Float, speed: Number = 55) = go(to, speed).await()

    private fun landing(): CompletableDeferred<Unit> {
        val d = CompletableDeferred<Unit>()
        onLand = { d.complete(Unit) }
        return d
    }

    suspend fun hop(h: Number = 12) {
        air = true; vy = -sqrt(2 * GRAVITY * h.toFloat())
        vel[P.Squash.ordinal] += 2.2f
        landing().await()
    }

    /** The ground has moved somewhere lower: keep the body where it is and let it drop. */
    suspend fun fallTo(to: () -> Surf?) {
        val was = baseY + yOff
        val sf = to()
        surf = to; baseY = sf?.y ?: was; yOff = min(0f, was - baseY); air = true; vy = maxOf(vy, 0f)
        landing().await()
    }

    /** The ground has moved somewhere higher: jump up onto it. */
    suspend fun jumpTo(to: () -> Surf?) {
        val was = baseY + yOff
        val sf = to()
        surf = to; baseY = sf?.y ?: was; yOff = maxOf(0f, was - baseY); air = true
        vy = -sqrt(2 * GRAVITY * (yOff + 9)); vel[P.Squash.ordinal] += 2.4f
        landing().await()
    }

    /** Wait for the feet to touch down, if it is in the air. */
    suspend fun landed() { if (air) landing().await() }

    fun surfNow(): Surf? = surf?.invoke()

    /**
     * A small effect at a place on the body, in the drawing's units (48 is
     * its middle, [GROUND] its feet). [vx]/[vy] are the effect's own drift.
     */
    fun fx(name: String, dx: Number, dy: Number, vx: Number? = null, vy: Number? = null, color: Long? = null) {
        if (dead) return
        val l = fxLayer ?: return
        l.add(name, x + (dx.toFloat() - 48f) * k, baseY + yOff - (GROUND - dy.toFloat()) * k, vx?.toFloat(), vy?.toFloat(), color, clip?.invoke())
    }

    /** The same, at a place in the layer rather than on the body. */
    fun fxAt(name: String, px: Float, py: Float) {
        fx(name, 48 + (px - x) / k, GROUND + (py - baseY - yOff) / k)
    }

    /** Where it is on screen, for taps: the drawing's box. */
    fun bounds(): Box {
        val top = baseY + yOff - GROUND * k
        return Box(x - 48 * k, x + 48 * k, top, top + 92 * k)
    }
}

/** A box in the layer's dp: left, right, top, bottom. */
data class Box(val l: Float, val r: Float, val t: Float, val b: Float) {
    val w get() = r - l
    val h get() = b - t
    val cx get() = (l + r) / 2
    val cy get() = (t + b) / 2
}
