package mv.obitech.crundi.ui.mini

import androidx.compose.foundation.ScrollState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.ui.platform.LocalContext
import mv.obitech.crundi.ui.mini.P.Alpha
import mv.obitech.crundi.ui.mini.P.ArmL
import mv.obitech.crundi.ui.mini.P.ArmR
import mv.obitech.crundi.ui.mini.P.ArmSwing
import mv.obitech.crundi.ui.mini.P.EyeX
import mv.obitech.crundi.ui.mini.P.EyeY
import mv.obitech.crundi.ui.mini.P.Grow
import mv.obitech.crundi.ui.mini.P.HeadRot
import mv.obitech.crundi.ui.mini.P.HeadY
import mv.obitech.crundi.ui.mini.P.Lean
import mv.obitech.crundi.ui.mini.P.LegL
import mv.obitech.crundi.ui.mini.P.LegR
import mv.obitech.crundi.ui.mini.P.Lid
import mv.obitech.crundi.ui.mini.P.Reach
import mv.obitech.crundi.ui.mini.P.Shake
import mv.obitech.crundi.ui.mini.P.Sit
import mv.obitech.crundi.ui.mini.P.Squash
import mv.obitech.crundi.ui.mini.P.Swing
import mv.obitech.crundi.ui.mini.P.Turn
import kotlin.math.abs
import kotlin.math.min

/*
 * A life in the top bar (crundi-mini.js, "A life in the top bar"). Mini does
 * not only leave the top bar for a chat. Things up there bother it too, and it
 * deals with them at the size it is as a logo:
 *
 *   five    the five-hour limit is over 90%. Panic. It runs to the end of the
 *           bar and tries to drag it back, or gets round the far side and
 *           shoves, with its hands or with its back. The bar does not move. It
 *           wears itself out and trudges home.
 *   week    the weekly limit is over 90%. That bar is in the top half, out of
 *           reach, so it jumps for it. And jumps. And cannot.
 *   notify  something is waiting for the person (the approvals counter is up).
 *           Some of the time it walks over and points at it.
 *   party   a limit has reset. It jumps for joy, runs a lap of honour, dances
 *           or spins; more of it for the weekly one.
 *   update  there is an update (the dot on Settings). It jumps down onto the
 *           tabs, walks along to Settings and points at it; if Settings is off
 *           the side it hauls the row of tabs along until it is there. Once
 *           when the update turns up, then seldom.
 *
 * If a chat needs it while it is out on one of these, it drops what it is
 * doing and walks off the screen to go there.
 */

private val PARTY = listOf(0xFF818CF8, 0xFFFBBF24, 0xFF34D399, 0xFFFB7185, 0xFF38BDF8)
private const val UPDATE_EVERY = 4 * 3_600_000L          // a reminder, not a nag

/** What the top bar shows, as Mini needs it. */
internal class TopRead(val five: Bar?, val week: Bar?, val update: Boolean, val tabs: Box?, val settings: Box?, val bell: Box?, val bells: Int)
/** A usage bar: where its fill ends, how full it is (percent), and the row it runs along. */
internal class Bar(val x: Float, val pct: Float, val top: Float, val bottom: Float, val left: Float, val right: Float)

internal class TopLife(private val host: MiniHost) {
    // Fed by the top bar.
    var fivePct: Float? = null
    var weekPct: Float? = null
    var update = false
    var approvals = 0

    var busy = false
    private var cutFn: (() -> Unit)? = null
    private val next = HashMap<String, Long>()
    private val seen = HashMap<String, Boolean>()
    private var prev: MutableMap<String, Float>? = null
    private var partyFor = "five"
    private var partyAt = 0L
    private var bells = 0
    private var bellDue = false
    private var lastTick = 0L

    /** A chat wants it: leave from wherever it is, by the nearest edge. */
    fun cut() { cutFn?.invoke() }

    fun pause() { cutFn = null; busy = false }

    private fun now() = System.currentTimeMillis()

    private fun bar(key: String, pct: Float?): Bar? {
        val row = host.box("top/$key") ?: return null
        val p = pct ?: return null
        if (row.w < 40 || row.h < 4) return null
        return Bar(row.l + row.w * (p / 100f).coerceIn(0f, 1f), p, row.t, row.b, row.l, row.r)
    }

    private fun read() = TopRead(
        five = bar("five", fivePct), week = bar("week", weekPct),
        update = update,
        tabs = host.box("top/tabs"),
        // The whole button, also when it is scrolled out of sight (it is there to be hauled into view).
        settings = host.box("top/tab-settings+"),
        bell = if (approvals > 0) host.box("top/bell") else null,
        bells = approvals,
    )

    /** What it last saw of each limit, kept across restarts so a reset while the app was shut is noticed. */
    private fun prevPct(): MutableMap<String, Float> {
        prev?.let { return it }
        val m = HashMap<String, Float>()
        host.prefs?.let { p -> for (k in listOf("five", "week")) if (p.contains("pct_$k")) m[k] = p.getFloat("pct_$k", 0f) }
        prev = m
        return m
    }

    fun tick() {
        val t = now()
        if (t - lastTick < 3000) return                         // the web looks every three seconds
        lastTick = t
        if (MiniDirector.holder != null || MiniDirector.logoAway || busy) return
        if (host.box("logo") == null) return
        val r = read()
        val ready = ArrayList<Pair<String, LongRange>>()
        fun due(kind: String, on: Boolean, first: LongRange, again: LongRange) {
            if (!on) { seen[kind] = false; return }
            if (seen[kind] != true) { seen[kind] = true; if ((next[kind] ?: 0) < t + first.first) next[kind] = t + rnd(first.first, first.last).toLong() }
            if (t >= (next[kind] ?: 0)) ready.add(kind to again)
        }
        // A limit that was well up and is suddenly near nothing has reset.
        val pv = prevPct()
        for ((kk, b) in listOf("five" to r.five, "week" to r.week)) {
            val nowPct = b?.pct ?: continue
            val was = pv[kk]
            if (was != null && was >= 35 && was - nowPct >= 30) { partyFor = kk; partyAt = t + rnd(3000, 9000).toLong() }
            if (was == null || abs(was - nowPct) >= 1) { pv[kk] = nowPct; host.prefs?.edit()?.putFloat("pct_$kk", nowPct)?.apply() }
        }
        if (partyAt != 0L && t >= partyAt) { if (outing("party", r)) partyAt = 0; return }
        due("five", (r.five?.pct ?: 0f) >= 90, 6_000L..20_000L, 7 * 60_000L..16 * 60_000L)
        due("week", (r.week?.pct ?: 0f) >= 90, 15_000L..45_000L, 12 * 60_000L..25 * 60_000L)
        // The update reminder is remembered across restarts, so opening the app again does not bring it straight back.
        val last = host.prefs?.getLong("update_at", 0L) ?: 0L
        if (r.update && r.tabs != null && r.settings != null && t - last >= UPDATE_EVERY) due("update", true, 20_000L..45_000L, UPDATE_EVERY..(UPDATE_EVERY * 3 / 2))
        else seen["update"] = false
        // A notification: only sometimes, and only when one has just arrived.
        val nb = if (r.bell != null) maxOf(1, r.bells) else 0
        if (nb > bells && t >= (next["notify"] ?: 0)) {
            if (chance(0.55)) { seen["notify"] = true; next["notify"] = t + rnd(8000, 30000).toLong(); bellDue = true }
            else next["notify"] = t + rnd(10 * 60_000, 20 * 60_000).toLong()      // not this time
        }
        bells = nb
        if (bellDue && r.bell == null) bellDue = false
        if (bellDue && t >= (next["notify"] ?: 0)) ready.add("notify" to 20 * 60_000L..40 * 60_000L)
        if (ready.isEmpty()) return
        val go = ready.random()
        if (outing(go.first, r)) {
            next[go.first] = t + rnd(go.second.first, go.second.last).toLong()
            if (go.first == "notify") bellDue = false
            if (go.first == "update") host.prefs?.edit()?.putLong("update_at", t)?.apply()
        }
    }

    private fun outing(kind: String, r: TopRead): Boolean {
        if (MiniDirector.holder != null || MiniDirector.logoAway || busy) return false
        val st = host.stage() ?: return false
        val a = st.actor
        var cut = false
        MiniDirector.holder = this; busy = true; MiniDirector.logoAway = true
        a.x = st.homeX; a.v[Grow.ordinal] = 0f; a.t[Grow.ordinal] = 0f; a.v[Alpha.ordinal] = 1f; a.t[Alpha.ordinal] = 1f
        fun free() { host.remove(a); busy = false; cutFn = null; if (MiniDirector.holder === this) MiniDirector.holder = null }
        // The logo stays out; the chat that called it brings it home.
        cutFn = {
            if (!cut) {
                cut = true
                host.after(a.play(host.scope) {
                    rest(); pose(Sit to 0, Swing to 0, Grow to 1)
                    carry = null
                    landed()
                    pose(Lid to -0.3); fx("bang", 48, -14); wait(300)          // oh. needed elsewhere
                    pose(Lid to 0)
                    clampX = false
                    walkTo(if (x < host.width / 2) -46f else host.width + 46, 120)
                }) { free() }
            }
        }
        val job = a.play(host.scope) {
            wait(260)
            pose(Grow to 1); wait(950)
            val mood = when (kind) {
                "five" -> r.five?.let { sceneFive(it, host.width) } ?: "calm"
                "week" -> r.week?.let { sceneWeek(it, host.width) } ?: "calm"
                "update" -> sceneUpdate(st, host)
                "notify" -> sceneNotify(host)
                else -> sceneParty(st, host.width, partyFor == "week")
            }
            // Home: slowly if it is worn out, head down.
            rest()
            val tired = mood == "tired"
            if (tired) pose(HeadY to 2.6, EyeY to 3.4, Lid to 0.3, ArmSwing to 0.3)
            walkTo(st.homeX, if (tired) 30 else 62)
            pose(HeadY to 0, EyeY to 0, Lid to 0, ArmSwing to 1, Turn to 0)
            wait(300)
            if (tired) sigh()
            pose(Grow to 0, Squash to 1, HeadRot to 0, Lean to 0)
            wait(820)
        }
        host.after(job) { if (!cut) { MiniDirector.logoAway = false; free() } }
        return true
    }
}

// ─── The scenes up there ───

/** Straining at something that will not give: feet slipping, breath going. */
private suspend fun Actor.strain(lean: Float, ms: Float, dustDx: Float) {
    val end = time + ms / 1000f
    var i = 0
    while (time < end) {
        val odd = i % 2 == 1
        pose(LegL to (if (odd) 26 else -22), LegR to (if (odd) -22 else 26), Lean to lean * (if (odd) 1f else 0.72f), Squash to (if (odd) 0.93 else 0.97), Lid to 0.86, HeadY to 2)
        if (i % 3 == 0) fx("dust", 48 + dustDx, GROUND - 3, vx = if (dustDx > 0) 9 else -9)
        if (i % 4 == 1) fx("sweat", 48 + dustDx * 0.5f, 10)
        wait(150); i++
    }
    pose(LegL to 0, LegR to 0)
}

/** Spent. */
private suspend fun Actor.worn() {
    pose(Reach to 0, ArmL to 0, ArmR to 0, Lean to 0, Squash to 0.9, HeadY to 5, Lid to 0.8, Turn to 0, HeadRot to 0)
    fx("puff", 64, 34); wait(700)
    fx("puff", 30, 34); wait(600)
    if (chance(0.6)) { pose(Sit to 1, Swing to 0, ArmL to 20, ArmR to 20, Squash to 1); wait(rnd(1500, 2400)); pose(Sit to 0); wait(600) }
    pose(Squash to 1, HeadY to 0, Lid to 0.2)
    pose(ArmR to -118); wait(700); pose(ArmR to 0); wait(250)        // wipe its brow
}

private suspend fun Actor.sceneFive(r: Bar, w: Float): String {
    val end = r.x
    pose(Turn to (if (end > x) 0.8 else -0.8), EyeX to 0); wait(500)  // it has seen the number
    pose(Lid to -0.35, HeadY to -2, ArmL to 150, ArmR to 150, Shake to 0.9); fx("bang", 48, -14)
    wait(620)
    val roomRight = min(w, r.right) - end
    val far = roomRight > 44 * k + 8                                  // is there room to get round the other side
    val ways = if (far) listOf("pull", "push", "back", "charge", "plead") else listOf("pull", "plead")
    val rounds = 1 + (Math.random() * 2).toInt()
    fun cx(v: Float) = clamp(v, 12f, w - 12)
    for (n in 0 until rounds) {
        val way = ways[Packs.deal(if (far) "five-far" else "five-near", ways.size)]
        val heaves = 1 + (Math.random() * 3).toInt()
        when (way) {
            "pull" -> {                                                // hands on the end, leaning right back
                walkTo(cx(end - 5 * k), 150)
                pose(Shake to 0, Lid to 0, Turn to 0.8, Reach to 1, ArmR to 100, ArmL to -92, HeadY to 0); wait(360)   // take hold
                repeat(heaves) {
                    pose(Lean to -34, Squash to 0.94, ArmR to 124, ArmL to -114, HeadRot to -8)
                    strain(-34f, rnd(800, 1700), 14f)
                    pose(Lean to -12, Squash to 1, Lid to 0.3, ArmR to 104, ArmL to -96, HeadRot to 0); wait(rnd(200, 420))
                }
            }
            "push" -> {                                                // round the far side, hands flat on it, shove
                walkTo(cx(end + 33 * k), 150)
                pose(Shake to 0, Lid to 0, Turn to -0.8, Reach to -1, ArmL to 90, ArmR to -80, HeadY to 0); wait(360)
                repeat(heaves) { strain(-19f, rnd(700, 1600), 14f); pose(Lean to -5, Squash to 1, Lid to 0.3); wait(rnd(200, 420)) }
            }
            "back" -> {                                                // its back against the end, feet out in front
                walkTo(cx(end + 23 * k), 150)
                pose(Shake to 0, Lid to 0, Turn to 0.8, ArmL to 26, ArmR to 26, HeadRot to -9, HeadY to 0); wait(360)
                repeat(heaves) { strain(-17f, rnd(800, 1700), 14f); pose(Lean to -6, Squash to 1, Lid to 0.3); wait(rnd(200, 420)) }
            }
            "charge" -> {                                              // a run-up. this will do it.
                walkTo(cx(end + 30 * k + rnd(46, 70)), 120)
                pose(Shake to 0, Lid to 0.5, Turn to -0.8, Lean to -6, HeadY to 1)
                repeat(3) { pose(LegR to 34); wait(150); fx("dust", 66, GROUND - 3, vx = 10); pose(LegR to 0); wait(170) }   // paw the ground
                pose(Lean to -12, ArmL to 40, ArmR to 40)
                walkTo(cx(end + 20 * k), 230)
                fxAt("ring", end, baseY - 12 * k); fx("bang", 48, -14)
                vx = 150f; vel[Squash.ordinal] -= 3f                   // and straight back off it
                pose(Lid to 0.94, Lean to 14, ArmL to 150, ArmR to 150, Turn to 0)
                hop(7)
                pose(Sit to 1, Swing to 0, Lean to 0, ArmL to 30, ArmR to 30, HeadY to 4)
                for (d in 0 until 5) { pose(HeadRot to if (d % 2 == 1) 10 else -10); wait(170) }   // stars
                pose(HeadRot to 0, Lid to 0.3, Sit to 0, HeadY to 0, ArmL to 0, ArmR to 0); wait(650)
            }
            else -> {                                                  // ask it nicely
                walkTo(cx(end + (if (far) 30 else -30) * k), 110)
                val fd = if (far) -1 else 1
                pose(Shake to 0, Lid to 0, Turn to fd * 0.8, Squash to 0.86, HeadY to 3, Reach to fd, ArmL to (if (fd < 0) 62 else -52), ArmR to (if (fd < 0) -52 else 62)); wait(1300)   // on its knees to it
                pose(Turn to 0, EyeY to -3, HeadRot to 6); wait(900)  // then to you
                pose(Turn to fd * 0.8, EyeY to 0, HeadRot to 0); wait(900)
                pose(Squash to 1, HeadY to 0, Reach to 0, ArmL to 0, ArmR to 0); wait(350)
            }
        }
        if (way != "plead" || chance(0.4)) worn() else { pose(Turn to 0); sigh() }
        if (n < rounds - 1) { pose(Turn to (if (end > x) 0.8 else -0.8), Lid to 0.45); wait(700) }   // right. something else.
    }
    val after = Packs.deal("five-after", 3) / 3f
    if (after < 0.34f) {                                               // kick it, regret it
        pose(Turn to (if (end > x) 0.7 else -0.7), Lid to 0.5); wait(300)
        pose(LegR to (if (end > x) -70 else 70)); wait(170); fxAt("ring", end, baseY - 8 * k)
        pose(LegR to 0, Lid to -0.35); fx("bang", 48, -14)
        repeat(4) { hop(5) }
    } else if (after < 0.67f) { pose(Turn to 0); shrug() }             // to you: I tried
    else { pose(Turn to (if (end > x) 0.7 else -0.7)); shakeHead() }
    return "tired"
}

private suspend fun Actor.sceneWeek(r: Bar, w: Float): String {
    val end = r.x
    pose(EyeY to -4.5, HeadRot to -7); wait(700)                       // up there
    pose(Lid to -0.3); fx("bang", 48, -14); wait(420)
    pose(Lid to 0, EyeY to 0, HeadRot to 0)
    walkTo(clamp(end - 6 * k, 12f, w - 12), 120)
    val tries = 3 + (Math.random() * 3).toInt()
    for (i in 0 until tries) {
        pose(Squash to 0.86, ArmL to 30, ArmR to 30, EyeY to -4.5); wait(220)   // crouch
        pose(Squash to 1, ArmL to 170, ArmR to 170)
        hop(7 + i * 1.2f)                                              // not high enough. never high enough.
        pose(ArmL to 0, ArmR to 0); wait(rnd(160, 320))
    }
    val after = Packs.deal("week-after", 3) * 0.34f
    if (after < 0.4f) {                                                // on tiptoe, trembling
        pose(Squash to 1.09, ArmR to 172, HeadY to -3, Shake to 0.3, EyeY to -4.5); wait(1500)
        pose(Squash to 1, ArmR to 0, HeadY to 0, Shake to 0, EyeY to 0)
    } else if (after < 0.7f) {                                         // shake a fist at it
        pose(ArmR to 158, Shake to 0.55, Lid to 0.5, EyeY to -4); fx("anger", 76, 6); wait(1100)
        pose(ArmR to 0, Shake to 0, Lid to 0, EyeY to 0)
    }
    worn()
    return "tired"
}

private suspend fun Actor.sceneUpdate(st: MiniHost.Stage, host: MiniHost): String {
    val w = host.width; val d = host.density
    val scroll: ScrollState? = host.tabScroll
    fun tabs() = host.box("top/tabs")
    fun btn() = host.box("top/tab-settings+")
    if (tabs() == null || btn() == null) return "calm"
    fun floor(): Surf? = tabs()?.let { Surf(it.b - 1, -200f, w + 200) }
    fun seen(): Boolean { val b = btn() ?: return true; val t = tabs() ?: return true; return b.l >= t.l - 2 && b.r <= min(t.r, w) + 2 }
    pose(EyeY to 4, HeadY to 3); wait(600)                             // what is that down there
    pose(EyeY to 0, HeadY to 0, ArmL to 120, ArmR to 120); wait(200)
    fallTo(::floor)
    pose(ArmL to 0, ArmR to 0); wait(380)

    // It was holding the tabs and the person dragged them: it goes where they go, hanging on.
    suspend fun carried(gripScroll: Int, sc: ScrollState): String {
        val gripX = x
        var prevS = sc.value; var still = 0; var v = 0f
        tx = null
        carry = { gripX - (sc.value - gripScroll) / d }
        pose(Lid to -0.35, Reach to 1, ArmR to 90, ArmL to -80, Turn to 0.75, Shake to 0.35, Squash to 1, HeadY to -1); fx("bang", 48, -14)
        try {
            while (still < 6) {
                wait(70)
                val cur = sc.value; val dd = (cur - prevS) / d
                if (dd != 0f) { v = -dd / 0.07f; still = 0; pose(Lean to clamp(dd * 2.2f, -34f, 34f), LegL to clamp(dd * 3, -50f, 50f), LegR to clamp(dd * 3 + 14, -50f, 50f)) }
                else still++
                prevS = cur
                if (x < -30 || x > w + 30) return "flung"
                // The tabs hit the end of their travel while it was still going fast.
                if (dd == 0f && abs(v) > 260) {
                    carry = null
                    pose(ArmL to 160, ArmR to 160, Shake to 1, Lean to (if (v > 0) 30 else -30))
                    walkTo(if (v > 0) w + 60 else -60f, clamp(abs(v), 300f, 700f))
                    return "flung"
                }
                if (still > 2) v = 0f
            }
        } finally { carry = null }
        return "dropped"
    }
    // Thrown off the screen: back along the top bar, not pleased, has a word, goes home.
    suspend fun stompBack(): String {
        val from = if (x < w / 2) -46f else w + 46
        rest(); pose(Alpha to 0); wait(rnd(1100, 1900))
        surf = st.homeSurf; baseY = st.homeY; yOff = 0f; air = false; x = from; vx = 0f
        // Half the time it comes back cross; otherwise dizzy, and half of those it faints on the way.
        if (chance(0.5)) {
            pose(Alpha to 1, Lid to 0.5, ArmL to -34, ArmR to -34)
            val going = go(st.homeX, 46); var n = 0
            while (tx != null) { if (n % 6 == 0) fx("anger", 76, 4); n++; wait(160) }
            going.await()
            pose(Turn to 0); wait(500)                                 // at you
            when (Packs.deal("flung-cross", 3)) {
                0 -> { pose(ArmL to 0, ArmR to 158, Shake to 0.55, Lean to 4); wait(1000); pose(Shake to 0, ArmR to 0, Lean to 0) }
                1 -> shakeHead()
                else -> { fx("puff", 22, 16, vx = -10); fx("puff", 74, 16, vx = 10); hop(5); hop(5) }
            }
            pose(ArmL to 0, ArmR to 0, Lid to 0); wait(300)
            return "calm"
        }
        val faint = chance(0.5)
        val stopAt = if (faint) from + (st.homeX - from) * rnd(0.35, 0.7) else st.homeX
        suspend fun stagger(to: Float) {                               // not in a straight line
            var g = go(to, 24); var i = 0
            while (tx != null) {
                val odd = i % 2 == 1
                pose(Lean to (if (odd) 13 else -13), HeadRot to (if (odd) -11 else 11), Lid to 0.62, ArmL to (if (odd) 40 else 8), ArmR to (if (odd) 8 else 40), EyeX to (if (odd) 2.5 else -2.5))
                if (i % 2 == 0) fx("spark", if (i % 4 != 0) 70 else 26, -8)
                if (i % 5 == 4 && tx != null) { val keep = tx!!; tx = null; wait(rnd(350, 650)); if (tx == null) g = go(keep, 24) }   // which way was it
                wait(260); i++
            }
            g.await()
            pose(Lean to 0, HeadRot to 0, ArmL to 0, ArmR to 0, EyeX to 0)
        }
        pose(Alpha to 1, Lid to 0.62)
        stagger(stopAt)
        if (faint) {
            pose(Turn to 0, Lid to 0.3, HeadRot to 9); wait(500)       // oh
            pose(Lid to 0.94, Lean to 22, ArmL to 30, ArmR to 30); wait(260)
            pose(Lean to 88, Squash to 0.96)                           // and over it goes
            vel[Squash.ordinal] -= 1.5f; fx("dust", 78, GROUND - 3, vx = 9)
            wait(700)
            val out = rnd(3200, 5600); val t0 = time
            while ((time - t0) * 1000 < out) { fx("spark", rnd(60, 86), rnd(50, 70)); wait(700) }
            pose(Lid to 0.6); wait(350); pose(Lid to 0.94); wait(300)  // coming round
            pose(Lid to 0.4, Lean to 30); wait(500)
            pose(Lean to 0, Squash to 1, Sit to 1, ArmL to 20, ArmR to 20); wait(700)   // sitting up
            shakeHead()
            pose(Sit to 0, Lid to 0.2); wait(600)
            pose(ArmR to -118); wait(700); pose(ArmR to 0); wait(250) // a hand to its head
            walkTo(st.homeX, 34)
        } else shakeHead()                                             // clear it
        pose(Turn to 0, Lid to 0, ArmL to 0, ArmR to 0); wait(300)
        return "tired"
    }

    // Settings is off the side: haul the tabs along until it is not.
    var n = 0
    while (n < 9 && !seen() && scroll != null) {
        val t = tabs() ?: break
        val from = clamp(min(t.r, w) - 26, 30f, w - 14)
        walkTo(from, 70)
        pose(Turn to 0.75, Reach to 1, ArmR to 90, ArmL to -80); wait(300)   // get a grip on it
        val x0 = x; val base = scroll.value
        val going = go(from - rnd(42, 58), 13)
        this.t[Turn.ordinal] = 0.75f                                   // backwards, leaning into it
        var i = 0; var set = scroll.value; var fate: String? = null
        while (tx != null) {
            // Somebody else has hold of the tabs: the person is dragging them.
            if (abs(scroll.value - set) > 2) { fate = carried(set, scroll); break }
            val want = base + ((x0 - x) * d).toInt()
            scroll.dispatchRawDelta((want - scroll.value).toFloat())
            set = scroll.value                                         // what it actually became (it stops at the end)
            pose(Lean to (if (i % 2 == 1) -19 else -13), Squash to (if (i % 2 == 1) 0.93 else 0.97), Lid to 0.85, HeadY to 2)
            if (i % 5 == 0) fx("sweat", 40, 10)
            if (i % 4 == 0) fx("dust", 62, GROUND - 3, vx = 9)
            wait(120); i++
        }
        if (fate == "flung") return stompBack()
        if (fate == null) going.await()
        pose(Reach to 0, Lean to 0, Squash to 1, Lid to 0.3, ArmL to 0, ArmR to 0, HeadY to 0, Turn to 0, Shake to 0, LegL to 0, LegR to 0); wait(260)
        n++
        if (fate == "dropped") { shakeHead(); continue }               // let go of. dizzy. where was it
        if (!seen()) { fx("puff", 64, 34); pose(Squash to 0.92, HeadY to 4, Lid to 0.7); wait(rnd(600, 1000)); pose(Squash to 1, HeadY to 0, Lid to 0) }
    }
    val b = btn() ?: return "calm"
    val side = if (b.cx - 22 * k > 12) -1 else 1
    walkTo(clamp(b.cx + side * 22 * k, 10f, w - 10), 66)
    val arm = if (side < 0) ArmR else ArmL
    val o = mutableMapOf<P, Number>(Turn to (if (side < 0) 0.6 else -0.6), EyeY to -1)
    for (rep in 0 until 2) {
        o[arm] = 104; pose(o); wait(320)
        repeat(3) { o[arm] = 92; pose(o); wait(130); btn()?.let { fxAt("ring", it.cx, it.cy) }; o[arm] = 110; pose(o); wait(180) }
        pose(Turn to 0, EyeY to 0); wait(700)                          // look at you. this. here.
        if (rep == 0) { hop(6); hop(6) }
    }
    pose(arm to 0); wait(300)
    jumpTo(st.homeSurf)                                                // and back upstairs
    wait(260)
    return "calm"
}

/** Something is waiting for the person: the approvals counter in the top bar is up. */
private suspend fun Actor.sceneNotify(host: MiniHost): String {
    val w = host.width
    fun at(): Box? = host.box("top/bell")
    val t = at() ?: return "calm"
    pose(Turn to (if (t.cx > x) 0.8 else -0.8), Lid to -0.2); wait(650)   // oh, that is new
    val side = if (t.l - 24 * k > 12) -1 else 1
    walkTo(clamp((if (side < 0) t.l else t.r) + side * 20 * k, 10f, w - 10), 96)
    val arm = if (side < 0) ArmR else ArmL
    val o = mutableMapOf<P, Number>(Turn to (if (side < 0) 0.6 else -0.6))
    val how = Math.random()
    for (n in 0 until 2) {
        o[arm] = 100; pose(o); wait(300)
        repeat(3) { o[arm] = 88; pose(o); wait(130); at()?.let { fxAt("ring", it.cx, it.cy) }; o[arm] = 106; pose(o); wait(170) }
        pose(Turn to 0); wait(650)                                     // you. look.
        if (n == 0) {
            if (how < 0.35) { hop(6); hop(6) }
            else if (how < 0.7) { pose(ArmL to -118, ArmR to -118, Squash to 1.06, HeadY to -2); wait(700); pose(ArmL to 0, ArmR to 0, Squash to 1, HeadY to 0) }   // hands round its mouth: oi
            else wave(3)
            o[Turn] = if (side < 0) 0.6 else -0.6
        }
    }
    pose(arm to 0); wait(300)
    return "calm"
}

/** A limit has reset. Everything it was worried about is gone. */
private suspend fun Actor.sceneParty(st: MiniHost.Stage, w: Float, big: Boolean): String {
    fun throwUp() { repeat(4) { fx("confetti", rnd(10, 86), rnd(-26, -4), vx = rnd(-12, 12), color = PARTY.random()) } }
    pose(EyeY to -3, Lid to -0.3); wait(600)                           // it looks. it looks again.
    pose(EyeY to 0, Lid to 0); wait(200)
    pose(EyeY to -3, Lid to -0.35); fx("bang", 48, -14); wait(500)
    pose(EyeY to 0)
    val acts: List<suspend () -> Unit> = listOf(
        {                                                              // up and down on the spot
            val n = 4 + (Math.random() * 3).toInt()
            for (i in 0 until n) { pose(ArmL to 162, ArmR to 162, Lid to 0.5); fx("spark", if (i % 2 == 1) 78 else 18, 4); if (i % 2 == 0) throwUp(); hop(rnd(8, 13)); pose(ArmL to 120, ArmR to 120); wait(90) }
            pose(ArmL to 0, ArmR to 0, Lid to 0)
        },
        {                                                              // a lap of honour, arms in the air
            val far = clamp(x + rnd(0.3, 0.55) * w, 20f, w - 20)
            pose(ArmL to 162, ArmR to 162, Shake to 0.9, Lid to 0.5)
            for (i in 0 until 2) {
                val g = go(if (i % 2 == 1) clamp(st.homeX + 30, 20f, w - 20) else far, 185)
                while (tx != null) { fx("spark", 48, -6); wait(170) }
                g.await(); throwUp()
            }
            pose(Shake to 0, ArmL to 0, ArmR to 0, Lid to 0, Turn to 0)
        },
        {                                                              // a little dance
            for (i in 0 until 8) {
                if (i % 2 == 1) pose(ArmL to 150, ArmR to 24, Lean to 7, HeadRot to 8, LegL to 0, LegR to -18, Lid to 0.5)
                else pose(ArmL to 24, ArmR to 150, Lean to -7, HeadRot to -8, LegL to 18, LegR to 0, Lid to 0.5)
                if (i % 2 == 1) fx("note", 74, 2, vx = 10)
                wait(230)
            }
            pose(ArmL to 0, ArmR to 0, Lean to 0, HeadRot to 0, LegL to 0, LegR to 0, Lid to 0)
        },
        {                                                              // round and round, then a pump of the fist
            repeat(2) { pose(Turn to 2.1, ArmL to 60, ArmR to 60); wait(260); pose(Turn to 0); wait(260) }
            pose(Squash to 0.85, ArmR to 30); wait(200)
            pose(Squash to 1, ArmR to 168, ArmL to 0); throwUp(); hop(12)
            pose(ArmR to 0)
        },
    )
    val count = if (big) 3 else 1 + (Math.random() * 2).toInt()
    repeat(count) { acts[Packs.deal("party", acts.size)](); wait(rnd(250, 500)) }
    pose(Turn to 0); wave(3)                                           // to you: did you see that
    return "calm"
}

// ─── Hooks for the top bar ───

/** The top bar tells Mini what it shows: the usage (0..1), the approvals count, and whether an update is waiting. */
@Composable
fun MiniTop(fiveUsed: Float?, weekUsed: Float?, approvals: Int, update: Boolean) {
    val host = LocalMiniHost.current ?: return
    val ctx = LocalContext.current
    SideEffect {
        if (host.prefs == null) host.prefs = ctx.getSharedPreferences("crundi_mini", android.content.Context.MODE_PRIVATE)
        host.top.fivePct = fiveUsed?.let { it * 100 }
        host.top.weekPct = weekUsed?.let { it * 100 }
        host.top.approvals = approvals
        host.top.update = update
    }
}

/** The tab strip's scroll, so Mini can haul the tabs along to Settings. */
@Composable
fun MiniTabScroll(scroll: ScrollState) {
    val host = LocalMiniHost.current ?: return
    DisposableEffect(host, scroll) {
        host.tabScroll = scroll
        onDispose { if (host.tabScroll === scroll) host.tabScroll = null }
    }
}
