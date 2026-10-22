package mv.obitech.crundi.ui.mini

import android.provider.Settings
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalInspectionMode
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlin.math.min
import kotlin.math.roundToInt

/*
 * Where Mini lives. [MiniStage] wraps a window's content and lays one layer
 * over all of it; the layer draws Mini and its effects and takes a touch only
 * on Mini itself (a small box that follows it), so everything else gets its
 * touches as if the layer were not there.
 *
 * The screens tell it where things are: each place Mini may stand on or point
 * at registers its box (Modifier.miniTarget) under its chat's id, and a chat
 * says what state it is in (MiniChat). The watchers (Watchers.kt) read those
 * to decide when Mini belongs in a chat, as crundi-mini.js's attach() does.
 */

/** The window's Mini layer, for the hooks below; null where Mini does not appear (reduced motion, previews). */
val LocalMiniHost = staticCompositionLocalOf<MiniHost?> { null }

/**
 * There is one Mini in the app. Whichever chat needs it first has it
 * ([holder]); the others wait until it has gone home. While it is out the
 * top bar's logo is empty ([logoAway]).
 */
object MiniDirector {
    internal var holder: Any? = null
    var logoAway by mutableStateOf(false)
        internal set
}

class MiniHost {
    /** Boxes in root pixels, keyed "id/what". `unclipped` keeps the chat's whole box too, for in-view checks. */
    internal val rects = HashMap<String, androidx.compose.ui.geometry.Rect>()
    internal var origin = Offset.Zero
    internal var density = 1f
    /** The layer's size, in dp. */
    internal var width = 0f
    internal var height = 0f
    internal val actors = ArrayList<Actor>()
    internal val tappable = mutableStateListOf<Actor>()
    internal val fx = FxLayer()
    internal var frame by mutableLongStateOf(0L)
    internal lateinit var scope: CoroutineScope
    internal val chats = LinkedHashMap<String, ChatWatcher>()
    internal val parked = LinkedHashMap<String, ParkedWatcher>()
    internal var running = true
    internal val wake = Channel<Unit>(Channel.CONFLATED)
    internal var logoChain: Job? = null
    /** Mini's life in the top bar (Top.kt): fed by the top bar, ticked with the watchers. */
    internal val top = TopLife(this)
    /** The tab strip's scroll, so Mini can haul the tabs along (the update outing). */
    internal var tabScroll: androidx.compose.foundation.ScrollState? = null
    internal var prefs: android.content.SharedPreferences? = null

    init { fx.onAdd = { wake.trySend(Unit) } }

    /** A registered box, in the layer's dp, or null when it is not on screen. */
    internal fun box(key: String): Box? {
        val r = rects[key] ?: return null
        if (r.width < 0.5f || r.height < 0.5f) return null
        return Box((r.left - origin.x) / density, (r.right - origin.x) / density, (r.top - origin.y) / density, (r.bottom - origin.y) / density)
    }

    internal fun add(a: Actor) {
        actors.add(a)
        if (a.onTap != null) tappable.add(a)
        wake.trySend(Unit)
    }

    internal fun remove(a: Actor) {
        a.destroy()
        actors.remove(a)
        tappable.remove(a)
    }

    /** Run [block] once [job] has finished, however it finished (the web's promise.then). */
    internal fun after(job: Job, block: () -> Unit) { scope.launch { job.join(); block() } }

    // ─── It is the logo ───
    // Mini in a chat is the head in the top bar, gone for a walk. Before it
    // turns up anywhere the logo grows a body and walks off the edge of the
    // screen; when it has left the chat it comes back along the top bar at the
    // pace it left with and folds away again.

    internal class Stage(val actor: Actor, val homeX: Float, val homeY: Float, val edge: Float, val homeSurf: () -> Surf?)

    internal fun stage(): Stage? {
        val r = box("logo") ?: return null
        if (r.w < 8) return null
        val k = (r.w * 74 / 96) / 50                       // so its head is exactly the size of the logo's
        val a = Actor(92 * k, fx)
        val homeX = r.l + r.w * 11 / 96 + 25 * k
        val homeY = r.t + r.h * 26 / 96 + 78 * k
        a.clampX = false
        val homeSurf = { Surf(homeY, -200f, width + 200) }
        a.surf = homeSurf
        a.baseY = homeY
        val edge = if (homeX < width / 2) -46f else width + 46
        add(a)
        return Stage(a, homeX, homeY, edge, homeSurf)
    }

    internal fun logoLeave(): Job {
        val prev = logoChain
        val j = scope.launch {
            prev?.join()
            if (MiniDirector.logoAway) return@launch
            val st = stage() ?: return@launch
            val a = st.actor
            a.x = st.homeX; a.v[P.Grow.ordinal] = 0f; a.t[P.Grow.ordinal] = 0f; a.v[P.Alpha.ordinal] = 1f; a.t[P.Alpha.ordinal] = 1f
            MiniDirector.logoAway = true
            try { a.leaveLogo(st.edge) } finally { remove(a) }
        }
        logoChain = j
        return j
    }

    internal fun logoReturn(speed: Float): Job {
        val prev = logoChain
        val j = scope.launch {
            prev?.join()
            if (!MiniDirector.logoAway) return@launch
            val st = stage()
            if (st == null) { MiniDirector.logoAway = false; return@launch }
            val a = st.actor
            a.x = st.edge; a.v[P.Alpha.ordinal] = 1f; a.t[P.Alpha.ordinal] = 1f
            try { a.returnToLogo(st.homeX, if (speed > 0) speed else 62f) } finally { MiniDirector.logoAway = false; remove(a) }
        }
        logoChain = j
        return j
    }

    /** The window went to the background: Mini goes home at once, and nobody holds it. */
    internal fun pause() {
        running = false
        top.pause()
        chats.values.forEach { it.gone(quiet = true) }
        parked.values.forEach { it.gone(quiet = true) }
        logoChain?.cancel(); logoChain = null
        actors.toList().forEach { remove(it) }
        fx.clear()
        if (MiniDirector.logoAway) MiniDirector.logoAway = false
        val h = MiniDirector.holder
        if (h is ChatWatcher && h.host === this || h is ParkedWatcher && h.host === this || h === top) MiniDirector.holder = null
    }

    internal fun tick() {
        if (!running) return
        top.tick()
        chats.values.toList().forEach { it.tick() }
        parked.values.toList().forEach { it.tick() }
    }
}

/** Is motion turned off on this phone (or is this a preview)? Then Mini never appears. */
@Composable
private fun miniStill(): Boolean {
    val ctx = LocalContext.current
    return LocalInspectionMode.current ||
        runCatching { Settings.Global.getFloat(ctx.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f }.getOrDefault(false)
}

/** A window's content with Mini's layer over it. */
@Composable
fun MiniStage(modifier: Modifier = Modifier, content: @Composable BoxScope.() -> Unit) {
    if (miniStill()) { Box(modifier) { content() }; return }
    val host = remember { MiniHost() }
    Box(modifier) {
        CompositionLocalProvider(LocalMiniHost provides host) { content() }
        MiniLayer(host)
    }
}

@Composable
private fun BoxScope.MiniLayer(host: MiniHost) {
    host.scope = rememberCoroutineScope()
    host.density = LocalDensity.current.density
    val life = LocalLifecycleOwner.current.lifecycle
    DisposableEffect(host, life) {
        val obs = LifecycleEventObserver { _, e ->
            if (e == Lifecycle.Event.ON_STOP) host.pause()
            if (e == Lifecycle.Event.ON_START) host.running = true
        }
        life.addObserver(obs)
        onDispose { life.removeObserver(obs); host.pause() }
    }
    // One clock for every actor and effect, only while there is something to move.
    LaunchedEffect(host) {
        var last = 0L
        while (isActive) {
            if (host.actors.isEmpty() && host.fx.isEmpty) { host.wake.receive(); last = 0L; continue }
            withFrameNanos { now ->
                val dt = if (last == 0L) 0.016f else min(0.05f, (now - last) / 1e9f)
                last = now
                host.fx.now += dt
                for (a in host.actors.toList().asReversed()) a.step(dt)
                host.frame++
            }
        }
    }
    // The watchers look at their chats a few times a second, as the web's do.
    LaunchedEffect(host) { while (isActive) { delay(400); host.tick() } }

    Canvas(Modifier.matchParentSize().onGloballyPositioned {
        host.origin = it.positionInRoot()
        host.width = it.size.width / host.density
        host.height = it.size.height / host.density
    }) {
        host.frame                                    // redraw every frame
        for (a in host.actors) drawActor(a, host.density)
        host.fx.draw(this, host.density)
    }
    // Only the robot itself takes a tap.
    for (a in host.tappable) {
        val size = (96 * a.k).dp
        Box(
            Modifier
                .offset { host.frame; val b = a.bounds(); IntOffset((b.l * host.density).roundToInt(), (b.t * host.density).roundToInt()) }
                .size(size, (92 * a.k).dp)
                .pointerInput(a) { detectTapGestures { if (a.v[P.Alpha.ordinal] > 0.3f) a.onTap?.invoke() } },
        )
    }
}

// ─── Hooks for the screens ───

/** Register this element's box as [what] of [id] (a chat or a parked pane), for Mini to stand on or point at. */
fun Modifier.miniTarget(id: String?, what: String): Modifier = if (id == null) this else composed {
    val host = LocalMiniHost.current ?: return@composed this
    val key = "$id/$what"
    DisposableEffect(host, key) { onDispose { host.rects.remove(key); host.rects.remove("$key+") } }
    onGloballyPositioned {
        host.rects[key] = it.boundsInRoot()
        // The whole box, not cut to what is visible, to tell how much of it is on screen.
        val p = it.positionInRoot()
        host.rects["$key+"] = androidx.compose.ui.geometry.Rect(p.x, p.y, p.x + it.size.width, p.y + it.size.height)
    }
}

/** Any touch inside here, other than on Mini, is the person getting on with things in [id]. */
fun Modifier.miniActivity(id: String?): Modifier = if (id == null) this else composed {
    val host = LocalMiniHost.current ?: return@composed this
    pointerInput(host, id) {
        awaitPointerEventScope {
            while (true) {
                val e = awaitPointerEvent(PointerEventPass.Initial)
                if (e.type == PointerEventType.Press) { host.chats[id]?.userActed(); host.parked[id]?.acted() }
            }
        }
    }
}

/** The top bar's head: where Mini comes from and goes back to. Empty while Mini is out. */
fun Modifier.miniLogo(): Modifier = composed {
    val host = LocalMiniHost.current ?: return@composed this
    DisposableEffect(host) { onDispose { host.rects.remove("logo") } }
    onGloballyPositioned { host.rects["logo"] = it.boundsInRoot() }
        .graphicsLayer { alpha = if (MiniDirector.logoAway) 0f else 1f }
}

/**
 * A chat says how it is: its state ("idle", "working", ...), what is in the
 * message box, and what is queued (each queued message's text). Mini watches
 * it from here on, until it leaves the screen.
 */
@Composable
fun MiniChat(id: String, state: String, draft: String, queued: List<String>, suggestion: Boolean) {
    val host = LocalMiniHost.current ?: return
    DisposableEffect(host, id) {
        val w = ChatWatcher(host, id)
        host.chats[id] = w
        onDispose { if (host.chats[id] === w) host.chats.remove(id); w.destroy() }
    }
    SideEffect { host.chats[id]?.feed(state, draft, queued, suggestion) }
}

/**
 * A parked pane says whether its Resume can be pressed ([ready]) and whether
 * the card offers "Compact and resume" ([long]).
 */
@Composable
fun MiniParked(id: String?, ready: Boolean, long: Boolean) {
    val host = LocalMiniHost.current ?: return
    if (id == null) return
    DisposableEffect(host, id) {
        val w = ParkedWatcher(host, id)
        host.parked[id] = w
        onDispose { if (host.parked[id] === w) host.parked.remove(id); w.destroy() }
    }
    SideEffect { host.parked[id]?.let { it.ready = ready; it.long = long } }
}
