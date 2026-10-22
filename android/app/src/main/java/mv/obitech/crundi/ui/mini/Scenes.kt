package mv.obitech.crundi.ui.mini

import mv.obitech.crundi.ui.mini.P.Alpha
import mv.obitech.crundi.ui.mini.P.ArmL
import mv.obitech.crundi.ui.mini.P.ArmR
import mv.obitech.crundi.ui.mini.P.ArmSwing
import mv.obitech.crundi.ui.mini.P.ArmsBack
import mv.obitech.crundi.ui.mini.P.EyeX
import mv.obitech.crundi.ui.mini.P.EyeY
import mv.obitech.crundi.ui.mini.P.Grow
import mv.obitech.crundi.ui.mini.P.HeadRot
import mv.obitech.crundi.ui.mini.P.HeadY
import mv.obitech.crundi.ui.mini.P.Lean
import mv.obitech.crundi.ui.mini.P.LegL
import mv.obitech.crundi.ui.mini.P.LegR
import mv.obitech.crundi.ui.mini.P.Lid
import mv.obitech.crundi.ui.mini.P.Shake
import mv.obitech.crundi.ui.mini.P.Sit
import mv.obitech.crundi.ui.mini.P.Squash
import mv.obitech.crundi.ui.mini.P.Swing
import mv.obitech.crundi.ui.mini.P.Turn
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.min

/*
 * The scenes and the small moves they are made of, as crundi-mini.js has
 * them, with the same timings. Each is a suspend function on the actor: it
 * waits on the actor's own clock and stops where it stands the moment a newer
 * behaviour takes over (its coroutine is cancelled).
 */

// ─── Small moves, shared by the scenes ───

internal suspend fun Actor.knock() {
    pose(Turn to 2.1, ArmR to 0, ArmL to 0, Lean to 0)              // turn its back on us
    wait(620)
    pose(ArmR to 152); wait(300)
    repeat(3) { pose(ArmR to 118); wait(95); fx("knock", 79, 20); pose(ArmR to 156); wait(150) }
    pose(ArmR to 30, HeadRot to 13)                                  // listen
    wait(1000)
    pose(HeadRot to 0, ArmR to 0, Turn to 0); wait(560)
}

internal suspend fun Actor.shrug() {
    pose(ArmL to 48, ArmR to 48, HeadY to 2.5, HeadRot to -7, EyeY to -2); wait(620)
    pose(ArmL to 0, ArmR to 0, HeadY to 0, HeadRot to 0, EyeY to 0); wait(380)
}

internal suspend fun Actor.watch() {
    pose(ArmL to -112, HeadRot to -11, HeadY to 2, EyeX to -3.5, EyeY to 3.5, Turn to -0.25)   // wrist up, look at it
    wait(700)
    repeat(4) { pose(LegR to -22); wait(150); pose(LegR to 0); wait(170) }
    pose(Lid to 0.45); wait(260)
    pose(ArmL to 0, HeadRot to 0, HeadY to 0, EyeX to 0, EyeY to 0, Turn to 0, Lid to 0); wait(420)
}

internal suspend fun Actor.shakeHead() {
    pose(Lid to 0.55, HeadY to 1.5)
    for (i in 0 until 5) { pose(Turn to if (i % 2 == 1) 0.55 else -0.55); wait(190) }
    pose(Turn to 0, Lid to 0, HeadY to 0); wait(360)
}

internal suspend fun Actor.sigh() {
    pose(Squash to 1.07, HeadY to -1.5, ArmL to 14, ArmR to 14); wait(430)
    pose(Squash to 0.9, HeadY to 4, HeadRot to 6, Lid to 0.6, ArmL to -4, ArmR to -4)
    fx("puff", 64, 38); wait(820)
    pose(Squash to 1, HeadY to 0, HeadRot to 0, Lid to 0, ArmL to 0, ArmR to 0); wait(380)
}

internal suspend fun Actor.tapFoot() {
    if (chance(0.45)) pose(ArmsBack to 1, HeadRot to 8, EyeY to -2.5, EyeX to 2)              // hands behind its back
    else pose(ArmL to -38, ArmR to -38, HeadRot to 8, EyeY to -2.5, EyeX to 2)                // or arms folded; eyes to the ceiling
    repeat(7) { pose(LegL to -24); wait(140); pose(LegL to 0); wait(160) }
    pose(ArmsBack to 0, ArmL to 0, ArmR to 0, HeadRot to 0, EyeY to 0, EyeX to 0); wait(360)
}

/**
 * Pacing, the way people do outside a hospital room: slow, heavy lengths of
 * the same stretch, head down, hands clasped behind or wrung in front. At each
 * end a look up at the door (the conversation) in case, nothing, and round
 * again. Now and then the watch, a hand to the forehead, a long breath, or
 * sitting down for a moment and not being able to stay sat.
 */
internal suspend fun Actor.pace(turns: Int, speed: Float) {
    var sf = surfNow() ?: return
    if (sf.r - sf.l < 58) return
    var dir = if (x - sf.l > sf.r - x) -1 else 1                    // start towards the longer side
    var clasp = chance(0.65)
    fun brood(d: Int) {
        if (clasp) pose(ArmsBack to 1, ArmL to 0, ArmR to 0, Shake to 0, ArmSwing to 0.08, HeadY to 2.6, HeadRot to d * 8, EyeY to 3.6, Lid to 0.26, Lean to d * 1.5, Squash to 0.985)
        else pose(ArmsBack to 0, ArmL to -30, ArmR to -30, Shake to 0.1, ArmSwing to 0, HeadY to 2.6, HeadRot to d * 8, EyeY to 3.6, Lid to 0.26, Lean to d * 1.5, Squash to 0.985)
    }
    suspend fun lookUp() {                                           // anything? no.
        pose(Turn to 0, HeadRot to -6, HeadY to -0.5, EyeY to -4.2, Lid to -0.12, Lean to 0, Shake to 0, Squash to 1)
        wait(rnd(900, 1500))
        pose(Lid to 0.55, HeadY to 2.5, EyeY to 2)
        wait(rnd(380, 620))
    }
    for (i in 0 until turns) {
        sf = surfNow() ?: return
        val to = if (dir > 0) sf.r - 8 - rnd(0, 14) else sf.l + 8 + rnd(0, 14)
        val kind = if (i == 0) 0 else (Math.random() * 6).toInt()
        if (i > 0 && chance(0.3)) clasp = !clasp
        brood(dir)
        var going = go(to, if (kind == 3) speed * 1.4f else speed)
        when (kind) {
            1 -> {                                                   // the watch, without stopping
                wait(520)
                pose(ArmsBack to 0, Shake to 0, ArmSwing to 0.08, ArmL to -112, ArmR to 0, HeadRot to -9, HeadY to 3, EyeX to -3.2, EyeY to 3.6, Lid to 0.1)
                wait(1000)
                if (tx != null) brood(dir)
            }
            2 -> {                                                   // stop dead, a hand to the forehead
                wait(rnd(500, 900))
                if (tx != null) {
                    tx = null
                    pose(Turn to 0, ArmsBack to 0, Shake to 0, ArmR to 0, ArmL to -122, HeadY to 4, HeadRot to -5, Lid to 0.9, Lean to 0)
                    wait(1300)
                    pose(ArmL to 0, Lid to 0.3, HeadY to 2); wait(350)
                    brood(dir)
                    going = go(to, speed)
                }
            }
            4 -> {                                                   // a long breath on the way
                wait(rnd(400, 800))
                if (tx != null) { tx = null; pose(Turn to 0, Lean to 0); wait(200); sigh(); brood(dir); going = go(to, speed) }
            }
        }
        going.await()
        pose(Squash to 0.97, Lean to 0, HeadRot to 0)               // the end of the length: stop, look up, turn
        wait(rnd(260, 520))
        if (i == turns - 1 || chance(0.6)) lookUp()
        if (kind == 5) {                                             // sit; cannot stay sat
            pose(ArmsBack to 0, Shake to 0, Sit to 1, Swing to 0.25, ArmL to -118, ArmR to -118, HeadY to 5, Lid to 0.9, Turn to 0, HeadRot to 0)
            wait(rnd(2200, 3200))
            pose(Sit to 0, Swing to 0, ArmL to 0, ArmR to 0, HeadY to 0, Lid to 0.2)
            wait(650)
        }
        pose(Squash to 1)
        dir = -dir
    }
    pose(ArmsBack to 0, Shake to 0, ArmL to 0, ArmR to 0, ArmSwing to 1, HeadY to 0, HeadRot to 0, EyeY to 0, EyeX to 0, Lid to 0, Lean to 0, Turn to 0, Squash to 1)
    wait(420)
}

internal suspend fun Actor.peekDown() {
    pose(Lean to 0, HeadY to 6, HeadRot to 0, EyeY to 4.5, Squash to 0.93, ArmL to 22, ArmR to 22)   // read what is underfoot
    wait(900)
    pose(Squash to 1, HeadY to 0, EyeY to 0, ArmL to 0, ArmR to 0); wait(250)
    hop(7); hop(7)                                                    // stamp on it
    wait(300)
}

internal suspend fun Actor.scan() {
    pose(ArmR to 148, Turn to 0.85, EyeX to 2); wait(820)            // hand over the eyes, look far off
    pose(ArmR to 0, ArmL to 148, Turn to -0.85, EyeX to -2); wait(900)
    pose(ArmL to 0, Turn to 0, EyeX to 0); wait(360)
}

internal suspend fun Actor.paperPlane() {
    pose(ArmR to 70, Turn to 0.4, Lean to -5); wait(330)             // wind up
    pose(ArmR to 160, Lean to 6)
    fx("plane", 76, 30, vx = rnd(-70, -20), vy = -120)
    wait(520)
    pose(ArmR to 0, Lean to 0, Turn to 0, HeadRot to -9, EyeY to -4); wait(900)   // watch it go
    pose(HeadRot to 0, EyeY to 0); wait(260)
}

internal suspend fun Actor.wave(n: Int = 4) {
    pose(ArmR to 150, HeadRot to 6); wait(260)
    repeat(n) { pose(ArmR to 126); wait(130); pose(ArmR to 164); wait(130) }
    pose(ArmR to 0, HeadRot to 0); wait(340)
}

internal suspend fun Actor.enter(from: Float, to: Float, speed: Number) {
    clampX = false
    x = from; vx = 0f; baseY = surfNow()?.y ?: baseY; yOff = 0f; air = false
    resetPose()
    v[Alpha.ordinal] = 0f; pose(Alpha to 1)
    walkTo(to, speed)
    clampX = true
}

/** Leave in a hurry: arms in the air, flapping, dust at the heels. */
internal suspend fun Actor.runOff(span: Span) {
    val to = if (x < span.mid) span.l - 50 else span.r + 50
    clampX = false
    pose(ArmL to 162, ArmR to 162, Shake to 1, Lid to -0.3, Sit to 0, Swing to 0, HeadY to -1, Lean to 0, HeadRot to 0, EyeX to 0, EyeY to 0, Squash to 1, LegL to 0, LegR to 0)
    val done = go(to, 215)
    var i = 0
    while (i < 12 && tx != null) {
        fx("dust", if (vx > 0) 30 else 66, GROUND - 3, vx = if (vx > 0) -12 else 12)
        wait(105); i++
    }
    done.await()
    pose(Alpha to 0); wait(200)
}

// ─── A message waiting to be read ───

/** What else is in the chat: the "Working…" line and the badges in the corner. */
internal class ChatEnv(val activity: () -> Activity?, val badge: () -> Box?)
internal data class Activity(val x: Float, val r: Float, val y: Float, val t: Float)

internal suspend fun Actor.sceneQueue(span: Span, env: ChatEnv) {
    val sf = surfNow() ?: return
    // Come in on the side that does not mean walking across the badges.
    val side = if (env.badge() != null) -1 else if (chance(0.5)) -1 else 1
    val spot = sf.l + (sf.r - sf.l) * rnd(0.55, 0.72)
    enter(if (side < 0) span.l - 40 else span.r + 40, spot, 62)
    wait(380)
    when (Packs.deal("queue-open", 4)) {                              // it does not always open the same way
        0 -> { pose(EyeY to -3, HeadRot to -6); wait(520); pose(EyeY to 0, HeadRot to 0); knock(); shrug() }
        1 -> { watch(); shakeHead() }
        2 -> { peekDown(); knock() }
        else -> { scan(); sigh() }
    }
    queueLoop(env)
}

/** "Working…" is right there. Go over, look up at it, point, tap the watch. */
private suspend fun Actor.nagWorking(env: ChatEnv) {
    val sf = surfNow() ?: return
    val w = env.activity() ?: return
    walkTo(clamp(w.r + 30, sf.l, sf.r - 4), 70)
    val left = w.x < x
    val arm = if (left) ArmL else ArmR
    val o = mutableMapOf<P, Number>(Turn to (if (left) -0.6 else 0.6), EyeY to -4, HeadRot to (if (left) -9 else 9))
    pose(o); wait(700)
    val up = if (w.y < baseY - 46 * k) 128 else 96                  // above its shoulder, or level with it
    o[arm] = up; pose(o); wait(260)
    repeat(3) {
        o[arm] = up - 14; pose(o); wait(140)
        env.activity()?.let { fxAt("ring", it.x, it.y) }
        o[arm] = up + 6; pose(o); wait(170)
    }
    pose(Turn to 0, EyeY to 0, HeadRot to 0, arm to 0); wait(300)
    watch()                                                           // and how long has it been
}

/** A background command is running in the corner. Go and see what it is. */
private suspend fun Actor.checkBadge(env: ChatEnv) {
    val sf = surfNow() ?: return
    env.badge() ?: return
    walkTo(sf.r - 2, 60)
    pose(Turn to 0.75, Lean to 7, HeadRot to 8, EyeY to 2); wait(900)  // lean in and read it
    pose(ArmR to 96)
    repeat(2) { pose(ArmR to 84); wait(130); env.badge()?.let { fxAt("ring", it.l + 6, it.cy) }; pose(ArmR to 100); wait(170) }
    pose(ArmR to 0, Lean to 0, HeadRot to 0, EyeY to 0, Turn to 0); wait(300)
    shrug()                                                           // no idea either
}

internal suspend fun Actor.queueLoop(env: ChatEnv) {
    val acts: List<suspend () -> Unit> = listOf(
        { pace(6, 33f) }, { watch() }, { shakeHead() }, { tapFoot() }, { knock() }, { peekDown() },
        { sigh() }, { scan() }, { paperPlane() }, { nagWorking(env) }, { checkBadge(env) },
    )
    while (true) {
        var i = Packs.deal("queue", acts.size)
        // Only what is actually on screen: otherwise deal again.
        if (i == 9 && env.activity() == null) i = Packs.deal("queue", acts.size)
        if (i == 10 && env.badge() == null) i = Packs.deal("queue", acts.size)
        if ((i == 9 && env.activity() == null) || (i == 10 && env.badge() == null)) i = Packs.deal("queue-plain", 9)
        acts[i]()
        rest()
        wait(rnd(500, 1300))
    }
}

/** Another message has landed on the pile under its feet. */
internal suspend fun Actor.another(count: Int) {
    tx = null
    pose(Lid to -0.35, HeadY to -2, ArmL to 30, ArmR to 30); fx("bang", 48, -12)    // what was that
    wait(360)
    pose(Lid to 0, HeadY to 6, EyeY to 4.5, Squash to 0.93, ArmL to 22, ArmR to 22)  // look down at it
    wait(820)
    pose(HeadY to 0, EyeY to 0, Squash to 1, ArmL to 0, ArmR to 0); wait(200)
    if (count >= 3) {                                                 // this is getting silly
        pose(ArmL to 150, ArmR to 150, HeadRot to -8, EyeY to -4); wait(760)
        pose(ArmL to 0, ArmR to 0, HeadRot to 0, EyeY to 0)
        sigh()
    } else {
        pose(ArmL to -118, HeadY to 3, Lid to 0.9, HeadRot to -6); wait(900)          // hand over its face
        pose(ArmL to 0, HeadY to 0, Lid to 0, HeadRot to 0); wait(300)
        shakeHead()
    }
    knock()                                                           // and knock again, harder
}

// ─── A suggested reply is waiting: sit on its corner ───

internal suspend fun Actor.sceneSit(span: Span) {
    val sf = surfNow() ?: return
    enter(span.r + 40, min(sf.r - 8, sf.l + 60), 70)
    takeSeat()
    sitLoop()
}

internal suspend fun Actor.takeSeat() {
    val sf = surfNow() ?: return
    walkTo(sf.l + 15, 46)
    wait(200)
    pose(EyeY to 4, HeadY to 3); wait(520)                           // is it safe to sit on?
    pose(EyeY to 0, HeadY to 0)
    hop(8)
    pose(Sit to 1, Swing to 1, ArmL to 10, ArmR to 10)
    wait(1300)
}

internal suspend fun Actor.sitLoop() {
    while (true) {
        when (Packs.deal("sit", 6)) {
            0 -> { pose(Turn to 0.7, EyeY to 3, HeadRot to 7); wait(1200); pose(Turn to 0, EyeY to 0, HeadRot to 0) }   // look at the suggestion beside it
            1 -> {                                                    // pat it: this one
                pose(ArmR to 62)
                repeat(3) { pose(ArmR to 40, Turn to 0.5, EyeY to 3); wait(170); fx("ring", 84, 78); pose(ArmR to 62); wait(210) }
                pose(ArmR to 10, Turn to 0, EyeY to 0)
            }
            2 -> {                                                    // lean back and hum
                pose(ArmL to 34, ArmR to 34, HeadRot to -9, Lean to -5, Lid to 0.5)
                for (j in 0 until 3) { fx("note", 70 + j * 4, 6, vx = 8 + j * 5); wait(760) }
                pose(ArmL to 10, ArmR to 10, HeadRot to 0, Lean to 0, Lid to 0)
            }
            3 -> { wave(3); pose(ArmR to 10) }
            4 -> { pose(Swing to 2.1); wait(1300); pose(Swing to 1) }  // kick
            else -> { pose(HeadRot to -10, EyeY to -3.5, EyeX to -2); wait(1500); pose(HeadRot to 0, EyeY to 0, EyeX to 0) }   // stare at the ceiling
        }
        wait(rnd(1400, 3000))
    }
}

/** The seat has gone. Hang for a moment, fall, get up, leave. */
internal suspend fun Actor.sceneFall(span: Span, ground: () -> Surf?) {
    tx = null
    pose(Lid to -0.35, ArmL to 150, ArmR to 150, Swing to 0, HeadY to -2, Turn to 0, HeadRot to 0, Lean to 0)
    fx("bang", 48, -12)
    wait(330)                                                         // the cartoon pause before gravity notices
    pose(Sit to 0, Shake to 0.8, LegL to 24, LegR to -24)
    fallTo(ground)
    pose(Shake to 0, ArmL to 62, ArmR to 62, Lid to 0.9, HeadY to 5, LegL to 0, LegR to 0)
    wait(520)
    for (i in 0 until 4) { pose(HeadRot to if (i % 2 == 1) 9 else -9); wait(150) }   // shake it off
    pose(HeadRot to 0, Lid to 0, HeadY to 0, ArmL to 0, ArmR to 0); wait(260)
    pose(EyeY to -4.5, HeadRot to -8); wait(620)                     // look up at where the seat was
    pose(EyeY to 0, HeadRot to 0, Lid to -0.3)
    fx("sweat", 70, 14); wait(260)
    runOff(span)
}

// ─── Nothing typed for a long time: stand by Send and make a case for the box ───

/**
 * The phone layout of the web's nudge (the composer is always the narrow one
 * here): it stands on Send and points at the message box.
 */
internal suspend fun Actor.sceneNudge(span: Span, aim: () -> Pt?, sugAt: () -> Pt?) {
    val sf = surfNow() ?: return
    enter(span.r + 40, sf.l + (sf.r - sf.l) * 0.5f, 66)
    wait(300)
    wave(3)
    nudgeLoop(aim, 0, sugAt)
}

internal suspend fun Actor.nudgeLoop(aim: () -> Pt?, from: Int, sugAt: () -> Pt?) {
    data class Aim(val left: Boolean, val ang: Float, val dy: Float)
    fun pointAt(where: () -> Pt?): Aim? {
        val t = where() ?: return null
        val dx = t.x - x; val dy = t.y - (baseY - 34 * k)
        val ang = Math.toDegrees(atan2(abs(dx), dy).toDouble()).toFloat()
        return Aim(dx < 0, clamp(ang, 18f, 172f), dy)
    }
    suspend fun point() {
        val p = pointAt(aim) ?: return
        val arm = if (p.left) ArmL else ArmR
        val o = mutableMapOf<P, Number>(arm to p.ang, Turn to (if (p.left) -0.55 else 0.55), EyeY to (if (p.dy < 0) -3.5 else 3.5), HeadRot to (if (p.left) -6 else 6))
        pose(o); wait(520)
        repeat(3) { o[arm] = p.ang - 16; pose(o); wait(150); o[arm] = p.ang + 4; pose(o); wait(170) }
        pose(Turn to 0, EyeY to 0, HeadRot to 0); wait(520)          // look back at you, still pointing
        pose(arm to 0); wait(320)
    }
    suspend fun typing() {
        pose(ArmL to -52, ArmR to -52, HeadY to 2.5, EyeY to 3.5)
        for (i in 0 until 9) { if (i % 2 == 1) pose(ArmL to -66, ArmR to -40) else pose(ArmL to -40, ArmR to -66); wait(110) }
        pose(ArmL to 0, ArmR to 0, HeadY to 0, EyeY to 0); wait(260)
        fx("dots", 48, -18)
        wait(1500)
    }
    suspend fun jacks() {
        repeat(3) { pose(ArmL to 150, ArmR to 150, LegL to 16, LegR to -16); hop(11); pose(ArmL to 10, ArmR to 10, LegL to 0, LegR to 0); wait(170) }
        pose(ArmL to 0, ArmR to 0)
    }
    suspend fun ask() { pose(HeadRot to 11, ArmL to 44, ArmR to 44, EyeY to -1); fx("ask", 48, -16); wait(1500); pose(HeadRot to 0, ArmL to 0, ArmR to 0, EyeY to 0); wait(300) }
    suspend fun sitAwhile() {
        hop(6)
        pose(Sit to 1, Swing to 1, ArmL to 10, ArmR to 10); wait(4200)
        pose(Sit to 0, Swing to 0, ArmL to 0, ArmR to 0); wait(620)
    }
    // There is a suggested reply right there: or just tap that.
    suspend fun orThat() {
        if (sugAt() == null) return point()
        val p = pointAt(sugAt) ?: return
        val arm = if (p.left) ArmL else ArmR
        val o = mutableMapOf<P, Number>(arm to p.ang, Turn to (if (p.left) -0.55 else 0.55), EyeY to -3.5, HeadRot to (if (p.left) -7 else 7))
        pose(o); wait(520)
        repeat(3) { o[arm] = p.ang - 14; pose(o); wait(150); sugAt()?.let { fxAt("ring", it.x, it.y) }; o[arm] = p.ang + 4; pose(o); wait(180) }
        pose(Turn to 0, EyeY to 0, HeadRot to 0); wait(600)          // look at you: well, that one then
        pose(arm to 0); wait(320)
    }
    val acts: List<suspend () -> Unit> = listOf(
        { point() }, { typing() }, { orThat() }, { ask() }, { jacks() }, { point() }, { pace(2, 36f) },
        { sitAwhile() }, { watch() }, { orThat() }, { sigh() },
    )
    // The first thing is always the point of it; after that a different run each time, and not all of them.
    val turns = if (from >= 99) 0 else 6 + (Math.random() * 3).toInt()
    for (n in 0 until turns) {
        if (n == 0 && from == 0) acts[0]() else acts[Packs.deal("nudge", acts.size)]()
        rest()
        wait(rnd(900, 2000) + n * 220)
    }
    // It has tried everything. Sit down and nod off until someone types.
    if (t[Sit.ordinal] < 0.5f) hop(5)
    pose(Sit to 1, Swing to 0.5, ArmL to 8, ArmR to 8); wait(1700)
    pose(Lid to 0.5, HeadY to 2); wait(900)
    pose(Lid to 0.1, HeadY to 0); wait(500)
    autoBlink = false
    pose(Lid to 0.94, HeadRot to 13, HeadY to 4, Swing to 0)
    // Not for ever: while it sleeps nobody else can have it and the logo's
    // place stays empty. After a couple of minutes it wakes, stretches, goes home.
    repeat(80 + (Math.random() * 40).toInt()) { fx("z", 66, 2, vx = 10); wait(1500) }
    autoBlink = true
    pose(Lid to 0.5, HeadRot to 0, HeadY to 0); wait(600)
    pose(Lid to 0.75, ArmL to 152, ArmR to 152, HeadY to -2, Sit to 0); wait(1100)   // a stretch and a yawn
    pose(Lid to 0, ArmL to 0, ArmR to 0, HeadY to 0); wait(400)
    finished = true
}

// ─── Being tapped ───

private fun Actor.steam() { fx("puff", 22, 16, vx = -10); fx("puff", 74, 16, vx = 10) }

/** Annoyed, on its feet: a different way each time. */
internal val ANNOYED: List<suspend Actor.(Span) -> Unit> = listOf(
    { // stamp
        pose(Lid to 0.5, ArmL to -34, ArmR to -34, HeadY to 1); fx("anger", 76, 6)
        wait(260)
        hop(6); hop(6); hop(6)
        steam(); wait(700)
    },
    { // shoo
        pose(Lid to 0.45, Turn to 0.35, HeadRot to 5)
        repeat(4) { pose(ArmR to 112); wait(120); pose(ArmR to 66); wait(130) }
        pose(ArmR to 0); wait(360)
    },
    { // turn its back
        fx("anger", 76, 6)
        pose(Turn to 2.1, ArmL to -36, ArmR to -36); wait(700)
        repeat(4) { pose(LegL to -22); wait(140); pose(LegL to 0); wait(160) }
        pose(Turn to 1.05, HeadRot to 6); wait(820)                    // one eye over the shoulder: still there?
        pose(Turn to 2.1, HeadRot to 0); wait(620)
        pose(Turn to 0, ArmL to 0, ArmR to 0); wait(460)
    },
    { // shake a fist
        pose(Lid to 0.5, Lean to 5, ArmR to 158, Shake to 0.55, HeadRot to 4); fx("anger", 20, 8)
        wait(1100)
        pose(Shake to 0, ArmR to 0, Lean to 0, HeadRot to 0); wait(360)
    },
    { // step out of reach
        val sf = surfNow()
        if (sf != null) {
            pose(Lid to -0.35); fx("bang", 48, -12); wait(200)
            val room = if (x - sf.l > sf.r - x) -1 else 1
            val to = clamp(x + room * rnd(46, 80), sf.l + 6, sf.r - 6)
            pose(Lid to 0)
            walkTo(to, 150)
            pose(Turn to -room * 0.7, Lid to 0.5, ArmL to -34, ArmR to -34); wait(1100)   // and glare back
            shakeHead()
        }
    },
    { span -> // leave the pane and peer back in
        val sf = surfNow()
        if (sf != null) {
            val home = x; val right = x > span.mid; val edge = if (right) span.r else span.l
            pose(Lid to 0.5); fx("anger", 76, 6); wait(260)
            clampX = false
            walkTo(if (right) span.r + 46 else span.l - 46, 170)          // right out of the pane
            wait(rnd(1300, 2400))
            pose(Lid to 0)
            walkTo(edge + if (right) 9 else -9, 26)                      // one eye back round the corner
            pose(Turn to (if (right) -0.6 else 0.6), HeadRot to (if (right) -8 else 8))
            wait(1500)
            pose(HeadRot to 0)
            walkTo(clamp(home, sf.l + 6, sf.r - 6), 50)
            clampX = true
            wait(260)
        }
    },
)

/** Annoyed while sitting on a suggestion. */
internal val ANNOYED_SEATED: List<suspend Actor.(Span) -> Unit> = listOf(
    { pose(Lid to 0.5, Swing to 2.4, ArmL to -30, ArmR to -30); fx("anger", 76, 6); wait(1500); pose(Lid to 0, Swing to 1, ArmL to 10, ArmR to 10) },
    {
        pose(Lid to 0.45, Turn to 0.35)
        repeat(4) { pose(ArmR to 112); wait(120); pose(ArmR to 66); wait(130) }
        pose(ArmR to 10, Lid to 0, Turn to 0); wait(300)
    },
    { pose(Turn to 1.7, ArmL to -34, ArmR to -34, Swing to 0.2); wait(1500); pose(Turn to 1.0); wait(700); pose(Turn to 0, ArmL to 10, ArmR to 10, Swing to 1); wait(400) },
    { // hop down, glare, sit again
        val sf = surfNow()
        if (sf != null) {
            pose(Sit to 0, Swing to 0, Lid to 0.5); fx("anger", 76, 6); wait(380)
            walkTo(clamp(x + 56, sf.l + 6, sf.r - 6), 110)
            pose(Turn to -0.7, ArmL to -34, ArmR to -34); wait(1000)
            pose(Turn to 0, ArmL to 0, ArmR to 0, Lid to 0)
            takeSeat()
        }
    },
)

/** Prodded awake. */
internal suspend fun Actor.grumpyWake() {
    pose(Lid to -0.35, HeadRot to 0, HeadY to -2); fx("bang", 48, -12); wait(420)
    pose(Lid to 0.5, HeadY to 0); fx("anger", 76, 6); wait(500)
    shakeHead()
}

/** Enough. Shake a fist, stamp off. */
internal suspend fun Actor.stormOff(span: Span) {
    tx = null; autoBlink = true
    pose(Sit to 0, Swing to 0, Lid to 0.5, Turn to 0, HeadRot to 0, HeadY to 0, ArmL to 0, ArmR to 158, Shake to 0.6, Lean to 5)
    fx("anger", 76, 6); fx("anger", 22, 10)
    wait(900)
    pose(Shake to 0, ArmR to -34, ArmL to -34, Lean to 0)
    steam()
    clampX = false
    walkTo(if (x < span.mid) span.l - 50 else span.r + 50, 105)
    pose(Alpha to 0); wait(200)
}

/** Someone got on with it: wake if asleep, salute, go. */
internal suspend fun Actor.sceneBye(span: Span) {
    tx = null; autoBlink = true
    if (t[Lid.ordinal] > 0.8f) { pose(Lid to -0.35, HeadRot to 0, HeadY to -2); fx("bang", 48, -12); wait(360) }
    pose(Sit to 0, Swing to 0, Lid to 0, HeadRot to 0, HeadY to 0, EyeX to 0, EyeY to 0, Turn to 0, ArmL to 0, ArmR to 0, LegL to 0, LegR to 0)
    wait(260)
    pose(ArmR to 148, HeadRot to 5); wait(300)                         // salute
    pose(ArmR to 0, HeadRot to 0)
    hop(10)
    clampX = false
    walkTo(if (x < span.mid) span.l - 50 else span.r + 50, 150)
    pose(Alpha to 0); wait(200)
}

// ─── A sleeping chat ───

/** Where the parked card's buttons are. */
internal class ParkedGet(val resume: () -> Pt?, val compact: () -> Pt?, val compactSurf: () -> Surf?)

/**
 * When it feels like it: comes and stands on Resume and shows it to you. If
 * the card warns that the conversation is a long one, that worries it, and it
 * climbs down to "Compact and resume" and makes a case for that instead.
 */
internal suspend fun Actor.sceneParked(span: Span, get: ParkedGet) {
    val sf = surfNow() ?: return
    enter(if (chance(0.5)) span.l - 40 else span.r + 40, (sf.l + sf.r) / 2, 62)
    wait(300)
    pose(EyeX to -3, EyeY to -2); wait(600); pose(EyeX to 3); wait(600)   // dark in here
    pose(EyeX to 0, EyeY to 0)
    if (chance(0.5)) { pose(Lid to 0.7, ArmL to 150, ArmR to 150, HeadY to -2); wait(900); pose(Lid to 0, ArmL to 0, ArmR to 0, HeadY to 0) }   // a yawn: it is catching
    suspend fun press(where: () -> Pt?) {                               // this one. press this.
        where() ?: return
        pose(HeadY to 4, EyeY to 4.5, ArmL to 26, ArmR to 26); wait(500)
        repeat(2) { hop(6); where()?.let { fxAt("ring", it.x, it.y) } }
        pose(HeadY to 0, EyeY to 0, ArmL to 0, ArmR to 0); wait(300)
        val arm = if (chance(0.5)) ArmL else ArmR
        pose(arm to 24); wait(250)
        repeat(3) { pose(arm to 14); wait(140); pose(arm to 30); wait(160) }
        pose(arm to 0); wait(500)
    }
    press(get.resume)
    // A long conversation: that warning is right under its feet.
    if (get.compact() != null) {
        pose(HeadY to 6, EyeY to 4.5, Squash to 0.93); wait(1100)      // read it
        pose(HeadY to -2, EyeY to 0, Squash to 1, Lid to -0.35); fx("bang", 48, -12); wait(500)
        fx("sweat", 72, 14)
        pose(ArmL to -118, ArmR to -118, Lid to 0.3, HeadY to 3); wait(900)   // hands to its face
        pose(ArmL to 0, ArmR to 0, HeadY to 0, Lid to 0)
        pace(2, 40f)
        val cs = get.compactSurf()
        if (cs != null) {
            pose(HeadY to 5, EyeY to 4.5); wait(500)
            pose(HeadY to 0, EyeY to 0, ArmL to 120, ArmR to 120); wait(200)
            val held = Surf(cs.y, span.l - 100, span.r + 100)
            fallTo { get.compactSurf()?.let { Surf(it.y, span.l - 100, span.r + 100) } ?: held }
            surf = get.compactSurf
            pose(ArmL to 0, ArmR to 0); wait(300)
            get.compactSurf()?.let { walkTo((it.l + it.r) / 2, 50) }
            while (true) {
                press(get.compact)
                pose(ArmL to -60, ArmR to -60, HeadRot to 7, EyeY to -2); wait(1300)   // please
                pose(ArmL to 0, ArmR to 0, HeadRot to 0, EyeY to 0)
                wait(rnd(2500, 5000))
                if (chance(0.4)) watch()
            }
        }
    }
    // Nothing to worry about: keep it company, and in the end doze off on the button.
    for (n in 0 until 3) {
        wait(rnd(2200, 4200))
        if (n == 1) wave(3) else press(get.resume)
    }
    hop(5)
    pose(Sit to 1, Swing to 0.6, ArmL to 8, ArmR to 8); wait(2600)
    pose(Lid to 0.5, HeadY to 2); wait(900)
    autoBlink = false
    pose(Lid to 0.94, HeadRot to 13, HeadY to 4, Swing to 0)
    repeat(60 + (Math.random() * 40).toInt()) { fx("z", 66, 2, vx = 10); wait(1500) }
    autoBlink = true
    pose(Lid to 0.5, HeadRot to 0, HeadY to 0, Sit to 0); wait(700)  // enough of that: home
    pose(Lid to 0); wait(300)
    finished = true
}

// ─── The top bar ───

/** Out of the logo: look round, grow a body, stretch, walk off the edge. */
internal suspend fun Actor.leaveLogo(edge: Float) {
    wait(260)
    pose(EyeX to -3); wait(300); pose(EyeX to 3); wait(300); pose(EyeX to 0)   // is anyone looking
    wait(160)
    pose(Grow to 1); wait(950)                                         // body, arms, legs
    pose(ArmL to 150, ArmR to 150, HeadY to -1.5); wait(380)           // a stretch
    pose(ArmL to 0, ArmR to 0, HeadY to 0); wait(260)
    walkTo(edge, 62)
}

/** Back along the top bar at the pace it left with, and fold away into the logo. */
internal suspend fun Actor.returnToLogo(homeX: Float, speed: Float) {
    val sp = clamp(speed, 40f, 230f)
    if (sp > 170) pose(ArmL to 162, ArmR to 162, Shake to 1, Lid to -0.3)           // it left running: it comes back running
    else if (sp > 95 && sp < 125) pose(Lid to 0.5, ArmL to -34, ArmR to -34)       // it stormed off: still cross
    walkTo(homeX, sp)
    pose(Shake to 0, ArmL to 0, ArmR to 0, Lid to 0, Turn to 0)
    wait(if (sp > 170) 420 else 240)
    if (sp > 170) sigh()                                              // out of breath
    pose(EyeX to -3); wait(240); pose(EyeX to 3); wait(240); pose(EyeX to 0)
    pose(Grow to 0, Squash to 1, HeadY to 0, HeadRot to 0, Lean to 0) // fold away
    wait(820)
}
