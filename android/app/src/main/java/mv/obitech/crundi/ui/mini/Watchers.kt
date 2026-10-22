package mv.obitech.crundi.ui.mini

import mv.obitech.crundi.ui.mini.P.ArmL
import mv.obitech.crundi.ui.mini.P.ArmR
import mv.obitech.crundi.ui.mini.P.EyeY
import mv.obitech.crundi.ui.mini.P.HeadRot
import mv.obitech.crundi.ui.mini.P.HeadY
import mv.obitech.crundi.ui.mini.P.LegL
import mv.obitech.crundi.ui.mini.P.LegR
import mv.obitech.crundi.ui.mini.P.Lid
import mv.obitech.crundi.ui.mini.P.Sit
import mv.obitech.crundi.ui.mini.P.Swing
import mv.obitech.crundi.ui.mini.P.Turn
import kotlin.math.max
import kotlin.math.min

/*
 * Watching a chat, and deciding when Mini belongs in it (crundi-mini.js
 * attach and attachParked), with the web's triggers and timings:
 *
 *   queue  a sent message not picked up for 12 s
 *   sit    a suggestion untouched for 25 s while Claude is idle
 *   nudge  90 s since Claude finished (or the last touch or keystroke) with
 *          nothing typed; then a rest of ten minutes, ended early by a new turn
 *   parked a sleeping pane, after 35 to 110 s, if it feels like it; then not
 *          again for fifteen minutes
 */

internal object MiniDelays {
    var queue = 12_000L
    var sit = 25_000L
    var nudge = 90_000L
    const val NUDGE_REST = 10 * 60_000L
}

private fun now() = System.currentTimeMillis()

/** Is this box mostly in front of the person (on a phone, panes sit side by side and only one is in view)? */
private fun MiniHost.inView(clipped: Box?, whole: Box?): Boolean {
    if (clipped == null || whole == null) return false
    if (whole.w < 40 || whole.h < 40) return false
    val w = min(clipped.r, width) - max(clipped.l, 0f)
    val h = min(clipped.b, height) - max(clipped.t, 0f)
    return w > whole.w * 0.55f && h > min(whole.h * 0.5f, 160f)
}

internal class ChatWatcher(val host: MiniHost, val id: String) {
    private var state = "idle"
    private var typed = false
    private var draft = ""
    private var queued: List<String> = emptyList()
    private var sugShown = false

    private var actor: Actor? = null
    private var scene: String? = null
    private var leaving = false
    private var dead = false
    private var sinceQueue = 0L
    private var sinceSit = 0L
    private var sinceIdle = now()
    private var lastState = ""
    // The nudge needs only an open chat and nobody typing. What stops it coming
    // round and round is a rest after each visit, ended early by a new turn.
    private var nudgeAfter = 0L
    private var coming: String? = null
    private var awayFor = 0
    private val taps = ArrayList<Long>()
    private var lastKind = -1
    private data class Sig(val n: Int, val len: Int, val lines: Int)
    private var lastSig: Sig? = null

    fun feed(state: String, draft: String, queued: List<String>, suggestion: Boolean) {
        this.state = state
        // Typing is getting on with it, as a keystroke is on the web.
        if (draft != this.draft) { this.draft = draft; userActed() }
        typed = draft.isNotBlank()
        this.queued = queued
        sugShown = suggestion
    }

    private fun box(what: String) = host.box("$id/$what")
    private fun span(): Span = box("root")?.let { Span(it.l, it.r) } ?: Span(0f, host.width)
    private fun topOf(what: String, padL: Float, padR: Float): () -> Surf? = { box(what)?.let { Surf(it.t, it.l + padL, it.r - padR) } }
    private fun badgeBox(): Box? = box("badges")
    private fun activity(): Activity? = box("activity")?.let { Activity(it.l + 8, it.r, it.cy, it.t) }
    private val env = ChatEnv(::activity, ::badgeBox)

    /** Keep off the badges (they are buttons), and off the words "Working…" when they are at its level. */
    private fun clearOfBadges(fn: () -> Surf?): () -> Surf? = {
        fn()?.let { sf ->
            var l = sf.l; var r = sf.r
            val b = badgeBox(); val w = activity()
            if (b != null && b.l - 12 > l + 40) r = min(r, b.l - 12)
            if (w != null && w.t > sf.y - 60 && r - (w.r + 26) > 70) l = max(l, w.r + 26)
            Surf(sf.y, l, r)
        }
    }
    private fun nudgeSurf(): Surf? = box("send")?.let { Surf(it.t, it.l + 14, it.r - 14) }
    private fun composerTop(): Surf? = box("composer")?.let { Surf(it.t, it.l + 10, it.r - 10) }
    private fun inputTop(): Surf? = box("input")?.let { Surf(it.t, it.l + 10, it.r - 10) }
    private fun sugAt(): Pt? = box("sug")?.let { Pt(it.l + min(40f, it.w / 2), it.cy) }
    private fun aim(): Pt? = box("input")?.let { Pt(it.l + min(90f, it.w * 0.3f), it.cy) }
    private fun hasQ() = queued.isNotEmpty() && box("queue") != null
    private fun hasS() = sugShown && box("sug") != null
    private fun seen() = host.inView(box("root"), box("root+"))
    private fun mayNudge(t: Long) = t >= nudgeAfter

    private fun ensure(): Actor = actor ?: Actor(50f, host.fx).also { a ->
        a.clip = { box("root") }
        a.onTap = { tapped() }
        actor = a
        host.add(a)
    }

    /** It has left the chat: home to the top bar at the pace it left with. Nobody else has it until it is back. */
    fun gone(quiet: Boolean = false) {
        val sp = actor?.lastSpeed ?: 0f
        actor?.let { host.remove(it) }
        actor = null; scene = null; leaving = false; coming = null
        if (MiniDirector.holder !== this) return
        if (quiet) { MiniDirector.holder = null; return }
        host.after(host.logoReturn(sp)) { if (MiniDirector.holder === this && scene == null) MiniDirector.holder = null }
    }

    private fun start(name: String) {
        val h = MiniDirector.holder
        if (h != null && h !== this) return                  // it is busy in another chat
        MiniDirector.holder = this
        // First it has to get here: out of the logo and off the top bar.
        if (actor == null && coming != name) {
            coming = name; scene = "coming"
            host.after(host.logoLeave()) {
                if (dead || coming != name) return@after
                coming = null; scene = null
                if (!wanted(name)) { gone(); return@after }   // what it was coming for has gone: turn round
                start2(name)
            }
            return
        }
        start2(name)
    }

    private fun wanted(name: String) = when (name) {
        "queue" -> hasQ()
        "sit" -> hasS()
        else -> state == "idle" && !typed
    }

    private fun start2(name: String) {
        val a = ensure()
        scene = name; leaving = false
        a.autoBlink = true
        when (name) {
            "queue" -> { a.surf = clearOfBadges(topOf("queue", 12f, 12f)); a.play(host.scope) { sceneQueue(span(), env) } }
            "sit" -> { a.surf = topOf("sug", 0f, 0f); a.play(host.scope) { sceneSit(span()) } }
            else -> { a.surf = clearOfBadges(::nudgeSurf); a.play(host.scope) { sceneNudge(span(), ::aim, ::sugAt) } }
        }
    }

    /** From the seat on the suggestion to Send, without leaving: stand, look down, step off, land, walk over. */
    private fun climbDown() {
        val a = actor ?: return
        val sp = span()
        scene = "nudge"
        val held = Surf(a.baseY, sp.l - 100, sp.r + 100)
        a.surf = { held }
        a.tx = null; a.autoBlink = true
        a.play(host.scope) {
            rest()
            pose(Sit to 0, Swing to 0); wait(650)
            pose(HeadY to 5, EyeY to 4.5); wait(600)                // a long way down
            pose(HeadY to 0, EyeY to 0, ArmL to 120, ArmR to 120)
            wait(220)
            val surf = clearOfBadges(::nudgeSurf)
            fallTo { surf()?.let { Surf(it.y, sp.l - 100, sp.r + 100) } ?: held }
            pose(ArmL to 0, ArmR to 0); wait(360)
            this.surf = surf
            surf()?.let { walkTo(it.l + (it.r - it.l) * 0.5f, 60) }
            wave(2)
            nudgeLoop(::aim, 0, ::sugAt)
        }
    }

    private fun leave(how: String) {
        val a = actor ?: return
        if (leaving) return
        leaving = true
        val was = scene
        // Hold the ground where it last was: what it stood on is no longer there to measure.
        val sp = span()
        val held = Surf(a.baseY, sp.l - 100, sp.r + 100)
        val job = if (how == "fall") {
            val ground: () -> Surf? = if (was == "sit") ::composerTop else ::inputTop
            a.surf = { held }
            a.play(host.scope) { sceneFall(span(), { ground() ?: held }) }
        } else {
            val f = a.surf
            a.surf = { f?.invoke() ?: held }
            a.play(host.scope) { sceneBye(span()) }
        }
        host.after(job) { if (actor === a && leaving) gone() }
    }

    /** A touch or a keystroke in the chat (not on Mini): the person is getting on with things. */
    fun userActed() {
        sinceIdle = now()
        if (scene == "nudge") leave("bye")
    }

    /** Back to what it was doing, picking up somewhere in the middle; on its feet first unless it was sitting. */
    private suspend fun Actor.resumeScene(was: String?) {
        if (was != "sit" && t[Sit.ordinal] > 0.5f) { pose(Sit to 0, Swing to 0, ArmL to 0, ArmR to 0); wait(650) }
        when (was) {
            "queue" -> queueLoop(env)
            "sit" -> { if (t[Sit.ordinal] < 0.5f) takeSeat(); pose(Sit to 1, Swing to 1, ArmL to 10, ArmR to 10); sitLoop() }
            else -> nudgeLoop(::aim, 1 + (Math.random() * 7).toInt(), ::sugAt)
        }
    }

    private fun tapped() {
        val a = actor ?: return
        if (leaving || scene == null || scene == "coming") return
        val was = scene; val at = now(); val sp = span()
        taps.removeAll { at - it >= 16_000 }; taps.add(at)
        if (taps.size >= 4) {                                   // it has had enough of you
            taps.clear(); leaving = true
            val f = a.surf; val held = Surf(a.baseY, sp.l - 100, sp.r + 100)
            a.surf = { f?.invoke() ?: held }
            host.after(a.play(host.scope) { stormOff(sp) }) {
                if (actor === a && leaving) { gone(); sinceQueue = 0; sinceSit = 0; sinceIdle = now(); nudgeAfter = now() + MiniDelays.NUDGE_REST }
            }
            return
        }
        val asleep = a.t[Lid.ordinal] > 0.8f; val seated = a.t[Sit.ordinal] > 0.5f
        val list = if (seated) ANNOYED_SEATED else ANNOYED
        var k = (Math.random() * list.size).toInt()
        if (k == lastKind) k = (k + 1) % list.size
        lastKind = k
        a.tx = null; a.autoBlink = true
        a.play(host.scope) {
            rest(Lid)
            if (asleep) grumpyWake() else list[k](this, sp)
            // However it took it, it gets to its feet: nobody stays sitting next to whoever just poked them.
            if (t[Sit.ordinal] > 0.5f) {
                pose(Sit to 0, Swing to 0, Lid to 0.4, ArmL to -34, ArmR to -34, Turn to 0, HeadRot to 0)
                wait(700)
                tapFoot()
            }
            rest()
            wait(500)
            resumeScene(was)
        }
    }

    private fun queueSig(): Sig = Sig(queued.size, queued.sumOf { it.length }, queued.sumOf { if (it.isEmpty()) 0 else it.split('\n').size })

    fun tick() {
        if (dead) return
        val t = now()
        val st = state
        if (st != lastState) {
            if (st == "working") nudgeAfter = 0
            if (st == "idle") sinceIdle = t
            lastState = st
        }
        val hasQ = hasQ(); val hasS = hasS()
        val seen = seen()
        // Only in a pane in front of the person: swiped away, it goes home after a few seconds.
        if (scene != null && scene != "coming" && !leaving) { awayFor = if (seen) 0 else awayFor + 400; if (awayFor > 5000) { awayFor = 0; gone(); return } }
        // It tried everything, slept, woke: time to go.
        if (scene == "nudge" && !leaving && actor?.finished == true) { leave("bye"); return }
        sinceQueue = if (hasQ) (if (sinceQueue != 0L) sinceQueue else t) else 0
        sinceSit = if (hasS) (if (sinceSit != 0L) sinceSit else t) else 0

        val a = actor
        if (scene == "queue" && !hasQ) leave("fall")
        else if (scene == "sit" && !hasS) {
            // Its seat was taken away, not the reason to be here: back before long to make the case for the box.
            if (!leaving && st == "idle") sinceIdle = min(sinceIdle, t - MiniDelays.nudge + min(MiniDelays.nudge, 30_000L))
            leave("fall")
        }
        // A suggestion being there does not send the nudge away: it points at it.
        else if (scene == "nudge" && (st != "idle" || typed || hasQ)) leave("bye")
        // Sat on the suggestion long enough and still nothing typed: get down and stand by Send.
        else if (scene == "sit" && !leaving && a != null && !a.air && st == "idle" && mayNudge(t) && !typed && t - sinceIdle >= MiniDelays.nudge && nudgeSurf() != null) {
            nudgeAfter = t + MiniDelays.NUDGE_REST
            climbDown()
        }
        // More was queued while it stood there: it notices.
        val sig = queueSig()
        val ls = lastSig
        if (scene == "queue" && !leaving && a != null && ls != null && sig.n > 0 && (sig.n > ls.n || sig.len > ls.len + 2) && !a.air) {
            val cnt = sig.lines
            a.autoBlink = true
            a.play(host.scope) { rest(); another(cnt); rest(); wait(500); queueLoop(env) }
        }
        lastSig = sig
        if (scene != null) return
        val h = MiniDirector.holder
        if (h != null && h !== this) return
        if (!seen) return

        if (hasQ && t - sinceQueue >= MiniDelays.queue) start("queue")
        else if (hasS && st == "idle" && t - sinceSit >= MiniDelays.sit) start("sit")
        else if (!hasQ && st == "idle" && mayNudge(t) && !typed && t - sinceIdle >= MiniDelays.nudge && nudgeSurf() != null) {
            nudgeAfter = t + MiniDelays.NUDGE_REST                 // then leave them be for a while
            start("nudge")
        }
    }

    fun destroy() {
        dead = true
        gone()
        if (MiniDirector.holder === this) MiniDirector.holder = null
    }
}

/**
 * A sleeping chat (a parked pane): none of the usual scenes belong there.
 * When it feels like it Mini stands on Resume and shows it; for a long
 * conversation it frets and points at "Compact and resume".
 */
internal class ParkedWatcher(val host: MiniHost, val id: String) {
    var ready = true
    var long = false
    private var actor: Actor? = null
    private var scene: String? = null
    private var leaving = false
    private var dead = false
    private var wantAt = now() + rnd(35_000, 110_000).toLong()       // if it feels like it, and not at once
    private var handing = false
    private var offFor = 0

    private fun box(what: String) = host.box("$id/$what")
    private fun span(): Span = box("pk-veil")?.let { Span(it.l, it.r) } ?: Span(0f, host.width)
    private fun mid(what: String): () -> Pt? = { box(what)?.let { Pt(it.cx, it.cy) } }
    private fun topOf(what: String): () -> Surf? = { box(what)?.let { Surf(it.t, it.l + 6, it.r - 6) } }
    private val get = ParkedGet(mid("pk-resume"), mid("pk-compact"), topOf("pk-compact"))
    private fun ok() = ready && box("pk-resume") != null

    fun gone(quiet: Boolean = false) {
        val sp = actor?.lastSpeed ?: 0f
        actor?.let { host.remove(it) }
        actor = null; scene = null; leaving = false
        if (MiniDirector.holder !== this || handing) return
        if (quiet) { MiniDirector.holder = null; return }
        host.after(host.logoReturn(sp)) { if (MiniDirector.holder === this && scene == null) MiniDirector.holder = null }
    }

    /**
     * Resumed. The card it stood on is taken away, so it cannot leave from
     * there: it would just vanish. It is picked up where it was, glad about it,
     * comes down onto the message box of the chat that woke (by parachute from
     * up there) and runs off properly.
     */
    private fun handOver() {
        val old = actor
        if (handing || old == null) { gone(); return }
        handing = true
        val a = Actor(50f, host.fx)
        for (i in old.v.indices) { a.v[i] = old.v[i]; a.t[i] = old.t[i] }
        a.x = old.x; a.baseY = old.baseY + old.yOff; a.clampX = false
        val floor = Surf(a.baseY, -200f, host.width + 200)
        a.surf = { floor }
        a.v[P.Alpha.ordinal] = 1f; a.t[P.Alpha.ordinal] = 1f; a.autoBlink = true
        host.remove(old); actor = null; scene = null; leaving = false
        host.add(a)
        // The chat that has just woken has the same id; its composer, else the bottom of the screen.
        fun ground(): Surf {
            val y = host.box("$id/composer")?.t ?: (host.height - 6)
            return Surf(max(y, floor.y), -200f, host.width + 200)
        }
        val job = a.play(host.scope) {
            rest(); pose(Sit to 0, Swing to 0, Lid to -0.35, HeadY to -2, ArmL to 150, ArmR to 150); fx("bang", 48, -12)   // awake! and no floor
            wait(380)
            pose(LegL to 22, LegR to -22)
            fallTo(::ground)
            pose(LegL to 0, LegR to 0, Lid to 0.5, ArmL to 160, ArmR to 160, HeadY to 0)
            wait(260)
            hop(11); fx("spark", 20, 2); hop(11); fx("spark", 76, 2)
            pose(ArmL to 0, ArmR to 0, Lid to 0); wait(200)
            runOff(Span(0f, host.width))
        }
        host.after(job) {
            host.remove(a)
            handing = false
            host.after(host.logoReturn(215f)) { if (MiniDirector.holder === this) MiniDirector.holder = null }
        }
    }

    private fun leave() {
        val a = actor ?: return
        if (leaving) return
        leaving = true
        val sp = span()
        val held = Surf(a.baseY, sp.l - 100, sp.r + 100)
        a.surf = { held }
        host.after(a.play(host.scope) { sceneBye(span()) }) { if (actor === a) gone() }
    }

    fun acted() {
        wantAt = now() + rnd(60_000, 180_000).toLong()
        if (scene == "parked") leave()
    }

    private fun tapped() {
        val a = actor ?: return
        if (leaving || scene != "parked") return
        val sp = span()
        val f = listOf(ANNOYED[0], ANNOYED[1], ANNOYED[3]).random()
        a.tx = null; a.autoBlink = true
        host.after(a.play(host.scope) { rest(); pose(Sit to 0, Swing to 0); wait(300); f(this, sp); rest(); wait(600); leaving = true; sceneBye(sp) }) {
            if (actor === a) gone()
        }
    }

    fun tick() {
        if (dead) return
        val ok = ok()
        if (scene == "parked" && !leaving && !ok) { handOver(); return }
        val seen = host.inView(box("pk-veil"), box("pk-veil+"))
        if (scene == "parked" && !leaving && actor?.finished == true) { leave(); return }
        if (scene == "parked" && !leaving) { offFor = if (seen) 0 else offFor + 400; if (offFor > 5000) { offFor = 0; gone(); return } }
        if (scene != null || !ok || !seen || MiniDirector.holder != null || now() < wantAt) return
        MiniDirector.holder = this; scene = "coming"
        host.after(host.logoLeave()) {
            if (dead) return@after
            if (!ok()) { scene = null; gone(); return@after }
            val a = Actor(50f, host.fx)
            a.clip = { box("pk-veil") }
            a.onTap = { tapped() }
            actor = a
            host.add(a)
            scene = "parked"; leaving = false
            a.surf = topOf("pk-resume")
            a.play(host.scope) { sceneParked(span(), get) }
            wantAt = now() + 15 * 60_000L                               // not again for a good while
        }
    }

    fun destroy() {
        dead = true
        // Being taken away while it stands there is what resuming looks like from in here.
        if (actor != null && scene == "parked" && !leaving) handOver()
        else { gone(); if (MiniDirector.holder === this && !handing) MiniDirector.holder = null }
    }
}
