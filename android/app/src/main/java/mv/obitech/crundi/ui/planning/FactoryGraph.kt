package mv.obitech.crundi.ui.planning

import androidx.compose.animation.core.animateFloat
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.ui.composed
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.onLongClick
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.layoutId
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import mv.obitech.crundi.api.model.Factory
import mv.obitech.crundi.api.model.FactoryEdge
import mv.obitech.crundi.api.model.FactoryNode
import mv.obitech.crundi.api.model.FactoryRun
import mv.obitech.crundi.api.model.Position
import androidx.compose.foundation.gestures.detectTapGestures
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens
import kotlin.math.max
import kotlin.math.min

/** How each kind of step looks: icon, head colour, kind label. As tools/factory-flow/src/nodes.jsx. */
internal data class NodeLook(val icon: String, val head: Color, val kind: String)

internal fun lookOf(type: String) = when (type) {
    "start" -> NodeLook("play", CrundiColors.Green, "Start")
    "manager" -> NodeLook("crown", CrundiColors.Yellow, "Manager")
    "employee" -> NodeLook("user", CrundiColors.TextSecondary, "Step")
    "decision" -> NodeLook("git-branch", CrundiColors.Sky, "Decision")
    "join" -> NodeLook("git-merge", CrundiColors.Sky, "Join")
    "ask" -> NodeLook("help-circle", CrundiColors.Ask, "Ask me")
    "wait" -> NodeLook("clock", CrundiColors.WaitLight, "Wait")
    "end" -> NodeLook("flag", CrundiColors.Green, "End")
    else -> NodeLook("circle", CrundiColors.TextSecondary, type)
}

/** A step's state in a run. */
internal enum class NodeState { Idle, Visited, Active, Retired }

/** Who is at a step this second and what they are doing (the live graph, as the web's node.data.doing). */
internal data class Doing(val who: String, val text: String, val state: String, val chatId: String, val job: String? = null)

/**
 * What the run says about each step: tokens on it, visited, blocked, retired.
 * [live] (the workbench pane's live graph) adds who is at work there and
 * what they are doing, and draws a job the Manager handed out on that
 * person's own step, as the web's viewer does.
 */
internal class RunView(val run: FactoryRun?, val live: Boolean = false) {
    private val visited: Set<String> = run?.history?.mapNotNull { it.nodeId.ifEmpty { null } }?.toSet().orEmpty() +
        run?.tokens?.map { it.nodeId }.orEmpty() + (run?.snapshot?.nodes?.filter { it.type == "start" }?.map { it.id }.takeIf { run != null }.orEmpty())

    fun tokens(id: String) = run?.tokens?.filter { it.nodeId == id }.orEmpty()

    /** The run is moving (not crashed, paused, stopped or finished): there is live activity to show. */
    private val moving = live && run != null && run.status !in setOf("crashed", "paused", "stopped", "finished")

    /** Jobs the Manager handed out, each drawn on the first step of its person (nodeId of that step to the token). */
    private val jobs: Map<String, mv.obitech.crundi.api.model.RunToken> = if (!moving) emptyMap() else buildMap {
        for (t in run!!.tokens.filter { it.job != null && it.status == "work" }) {
            val at = run.snapshot?.nodes?.firstOrNull { it.type == "employee" && it.employeeId == t.employeeId && tokens(it.id).none { k -> k.status == "work" || k.status == "answer" } } ?: continue
            if (!containsKey(at.id)) put(at.id, t)
        }
    }

    /** Who is here now and what they are doing, from their chat; null when nobody is at work here. */
    fun doing(n: FactoryNode): Doing? {
        if (!moving) return null
        val r = run!!
        val tok = tokens(n.id).firstOrNull { it.status == "work" || it.status == "answer" }
        if (tok != null) {
            val e = r.employees[tok.employeeId]
            val chat = e?.chatId?.ifEmpty { null } ?: if (n.type == "manager") r.managerChatId else ""
            val who = tok.employee.ifEmpty { e?.name ?: r.snapshot?.roster?.firstOrNull { it.id == tok.employeeId }?.name ?: if (n.type == "manager") "Manager" else "" }
            return Doing(who, e?.activity?.text?.ifEmpty { null } ?: if (tok.status == "answer") "Deciding" else "Starting", e?.activity?.state.orEmpty(), chat)
        }
        val job = jobs[n.id] ?: return null
        val e = r.employees[job.employeeId]
        return Doing(job.employee.ifEmpty { e?.name.orEmpty() }, "Job: " + (e?.activity?.text?.ifEmpty { null } ?: job.job.orEmpty().lineSequence().firstOrNull().orEmpty().take(60)), e?.activity?.state.orEmpty(), e?.chatId.orEmpty(), job.id)
    }

    /**
     * Where the work is, for the camera to follow: steps with someone working,
     * deciding, asking, waiting or joining there, and the steps a job is drawn on.
     */
    fun activeIds(): List<String> {
        if (run == null || !live) return emptyList()
        val at = run.tokens.filter { it.nodeId.isNotEmpty() && it.job == null && it.status in setOf("work", "answer", "ask", "wait", "join") }.map { it.nodeId }
        return (at + jobs.keys).distinct().sorted()
    }

    fun state(n: FactoryNode): NodeState {
        if (run == null) return NodeState.Idle
        val emp = n.employeeId?.let { e -> run.employees[e] }
        return when {
            tokens(n.id).isNotEmpty() || n.id in jobs -> NodeState.Active
            emp?.retired == true -> NodeState.Retired
            n.id in visited -> NodeState.Visited
            else -> NodeState.Idle
        }
    }

    fun visited(id: String) = id in visited

    /** The pill over a step with tokens: words and colour, as the desktop's. */
    fun pill(n: FactoryNode): Pair<String, Color>? {
        val t = tokens(n.id).ifEmpty { listOfNotNull(jobs[n.id]) }
        if (t.isEmpty()) return null
        val first = t.first()
        val blocked = t.any { it.blocked } || run?.permissions?.any { p -> !p.sent && p.employee.isNotEmpty() && p.employee == first.employee } == true
        val (w, c) = when {
            blocked -> "needs permission" to CrundiColors.Yellow
            first.status == "ask" -> "waiting on you" to CrundiColors.Ask
            first.status == "wait" -> "waiting" to CrundiColors.Wait
            first.status == "join" -> "waiting" to CrundiColors.TextSecondary
            first.status == "answer" -> "deciding" to CrundiColors.Green
            first.status == "finished" -> "finished" to CrundiColors.Green
            else -> "working" to CrundiColors.Green
        }
        return (if (t.size > 1) "$w ×${t.size}" else w) to c
    }

    /** The last thing said at this step (the newest handoff there). */
    fun lastSummary(id: String) = run?.history?.lastOrNull { it.nodeId == id && it.summary.isNotEmpty() }?.summary
}

/** The out handles of a step, in order, with their labels. */
internal fun handlesOf(n: FactoryNode, edges: List<FactoryEdge>): List<Pair<String, String>> {
    val used = edges.filter { it.source == n.id && it.kind != "note" }.map { it.sourceHandle }
    val base = when (n.type) {
        "manager" -> listOf("out" to "", "finish" to "Finish")
        "decision" -> when (n.decision?.kind) {
            "yesno", null -> listOf("yes" to "Yes", "no" to "No")
            else -> n.decision?.options.orEmpty().map { it.id to it.label }
        }
        "ask" -> n.ask?.replies?.map { it.id to it.label }.orEmpty()
        "wait" -> if (n.wait?.kind in setOf("command", "signal")) listOf("done" to "Done", "timeout" to "Timed out") else listOf("out" to "")
        "end" -> emptyList()
        else -> listOf("out" to "")
    }
    val extra = used.filter { u -> base.none { it.first == u } }.distinct().map { it to if (it == "none") "None" else it }
    return base + extra
}

internal fun nodeBody(n: FactoryNode, f: Factory): String = when (n.type) {
    "start" -> "The task comes in"
    "manager" -> "Leads the team, decides the end"
    "employee" -> f.roster.firstOrNull { it.id == n.employeeId }?.let { m ->
        listOf(m.name, m.model.ifEmpty { "default model" }, if (m.memory == "fresh") "fresh each time" else "keeps memory").joinToString(" · ")
    } ?: "Nobody assigned"
    "decision" -> n.decision?.let { d ->
        listOf(d.question.ifEmpty { null }, when (d.kind) { "yesno" -> "Yes / no"; "many" -> "Pick any"; else -> "Pick one" }).filterNotNull().joinToString(" — ")
    } ?: "Yes / no"
    "join" -> "Waits for the branches"
    "ask" -> n.ask?.prompt.orEmpty()
    "wait" -> n.wait?.let { w ->
        when (w.kind) {
            "until" -> "Until ${w.at}"
            "command" -> "Until `${w.command}` succeeds · ${w.timeout.toInt()} min max"
            "signal" -> "For a signal: ${w.what} · ${w.timeout.toInt()} min max"
            else -> "For ${w.minutes.toInt()} min"
        }
    } ?: ""
    "end" -> "The result, presented by the Manager"
    else -> ""
}

/**
 * The editor's hold on the graph ([FactoryGraph] edit). A tap on a step
 * selects it (or, while connecting, connects to it); a long press and drag
 * moves it; a long press without moving starts connecting from it. Each
 * connection has a dot halfway along to tap it. Every callback is the
 * editor's; the graph only reports.
 */
class GraphEdit(
    val selectedEdge: String? = null,
    /** Connecting from this step's output (step id to output): steps it may not lead to are dimmed. */
    val connectFrom: Pair<String, String>? = null,
    val canConnectTo: (String) -> Boolean = { true },
    val onMove: (String, Position) -> Unit = { _, _ -> },
    val onLongPress: (String) -> Unit = {},
    val onEdge: (String) -> Unit = {},
    val onBackground: () -> Unit = {},
    /** The middle of what is on screen, in factory coordinates: where Add puts a step. */
    val onViewCentre: (Position) -> Unit = {},
    /** Centre the camera on this step whenever [focusKey] changes (going to an issue). */
    val focusNode: String? = null,
    val focusKey: Int = 0,
)

/**
 * A factory's flow, drawn as the desktop's viewer: steps as cards where the
 * editor put them, flow edges from out handles at the bottom to the next
 * step's top, note edges dashed amber from side to side. With a [run] the
 * steps light up: active (green glow and a pill), visited, retired.
 * Pinch or the buttons to zoom; drag to pan; tap a step to select it.
 */
@Composable
fun FactoryGraph(
    factory: Factory, run: FactoryRun?, selected: String?, onSelect: (String?) -> Unit, modifier: Modifier = Modifier,
    /** The live graph (the workbench pane): each active step shows who is there and what they are doing. */
    live: Boolean = false,
    /** Keep the camera on the steps where people are working, moving with them (the pane's Follow). */
    follow: Boolean = false,
    /** Panned, zoomed or moved by hand: the pane stops following. */
    onManualMove: () -> Unit = {},
    /** The factory editor: move, connect, select connections ([GraphEdit]). */
    edit: GraphEdit? = null,
) {
    val view = remember(run, live) { RunView(run, live) }
    val nodes = factory.nodes
    if (nodes.isEmpty()) { Box(modifier.fillMaxSize(), contentAlignment = Alignment.Center) { Text("This factory has no steps yet.", style = CrundiType.Small) }; return }
    // Kept by factory, not by version: an edit must not forget how tall the steps are.
    val heights = remember(factory.id) { mutableStateMapOf<String, Float>() }
    val curMinX = nodes.minOf { it.position.x }.toFloat(); val curMinY = nodes.minOf { it.position.y }.toFloat()
    // While editing the origin stays where it was when the factory opened, so
    // moving or adding a step never shifts everything else on screen.
    val origin = remember(factory.id, edit != null) { Offset(curMinX, curMinY) }
    val minX = if (edit != null) origin.x else curMinX
    val minY = if (edit != null) origin.y else curMinY
    val pad = 28f
    /** The step being dragged, and how far (dp). */
    var drag by remember { mutableStateOf<Pair<String, Offset>?>(null) }
    fun w(n: FactoryNode) = if (n.type == "start" || n.type == "end") 170f else 220f
    fun x(n: FactoryNode) = n.position.x.toFloat() - minX + pad + (drag?.takeIf { it.first == n.id }?.second?.x ?: 0f)
    fun y(n: FactoryNode) = n.position.y.toFloat() - minY + pad + (drag?.takeIf { it.first == n.id }?.second?.y ?: 0f)
    val byId = nodes.associateBy { it.id }
    val width = nodes.maxOf { x(it) + w(it) } + pad + 40f
    val height = nodes.maxOf { y(it) + (heights[it.id] ?: 80f) } + pad + 20f
    // Left of and above the origin (only while editing): 0 in the viewer.
    val left0 = min(0f, nodes.minOf { x(it) } - pad)
    val top0 = min(0f, nodes.minOf { y(it) } - pad)

    var scale by remember { mutableFloatStateOf(1f) }
    var offset by remember { mutableStateOf(Offset.Zero) }
    var box by remember { mutableStateOf(IntSize.Zero) }
    var fitted by remember(factory.id) { mutableStateOf(false) }
    val density = LocalDensity.current.density
    fun fit() {
        if (box.width == 0) return
        val cw = width - left0; val ch = height - top0
        val s = min(box.width / (cw * density), box.height / (ch * density)).coerceIn(0.2f, 1.2f)
        scale = s
        offset = Offset((box.width - cw * density * s) / 2f - left0 * density * s, max(0f, (box.height - ch * density * s) / 2f) - top0 * density * s)
    }
    /**
     * Readable, not everything: fitting a whole factory onto a phone made step
     * titles a few pixels high. Opens at full size centred on the step at work
     * (or the last one reached, or the start); the fit button is the overview.
     */
    fun focus() {
        if (box.width == 0) return
        val at = nodes.firstOrNull { view.tokens(it.id).isNotEmpty() }
            ?: run?.history?.lastOrNull { it.nodeId.isNotEmpty() }?.let { h -> byId[h.nodeId] }
            ?: nodes.firstOrNull { it.type == "start" } ?: nodes.first()
        val s = 1f
        val cx = (x(at) + w(at) / 2) * density * s
        val cy = (y(at) + (heights[at.id] ?: 80f) / 2) * density * s
        scale = s
        offset = Offset(box.width / 2f - cx, box.height * 0.4f - cy)
    }
    val measured = nodes.all { it.id in heights }
    LaunchedEffect(box, measured) { if (!follow && !fitted && box.width > 0 && measured) { focus(); fitted = true } }

    // The editor: where the middle of the screen is, and going to a step.
    if (edit != null) {
        val report by androidx.compose.runtime.rememberUpdatedState(edit.onViewCentre)
        LaunchedEffect(scale, offset, box) {
            if (box.width == 0) return@LaunchedEffect
            val cx = (box.width / 2f - offset.x) / (scale * density) - pad + minX
            val cy = (box.height / 2f - offset.y) / (scale * density) - pad + minY
            report(Position(cx.toDouble(), cy.toDouble()))
        }
        // Again when the canvas changes size (the sheet opening): the step stays in view.
        LaunchedEffect(edit.focusKey, box) {
            val at = edit.focusNode?.let { byId[it] } ?: return@LaunchedEffect
            if (box.width == 0 || edit.focusKey == 0) return@LaunchedEffect
            val s = max(scale, 0.9f)
            scale = s
            offset = Offset(box.width / 2f - (x(at) + w(at) / 2) * density * s, box.height * 0.35f - (y(at) + (heights[at.id] ?: 80f) / 2) * density * s)
        }
    }

    /**
     * Follow the work, as the web's fitView on the active steps (padding 0.45,
     * at most 0.95): zoom to where people are working and move with it. On a
     * phone a wide spread of parallel steps would be fitted into unreadably
     * small cards, so it never goes below [FOLLOW_MIN_SCALE]; it centres on
     * them then, and the working-now strip names everyone. With nobody at a
     * step, it goes to the step last reached, as opening the graph does.
     */
    val activeKey = view.activeIds().joinToString(",")
    val still = androidx.compose.ui.platform.LocalInspectionMode.current
    LaunchedEffect(follow, activeKey, box, measured) {
        if (!follow || box.width == 0 || !measured) return@LaunchedEffect
        val ids = activeKey.split(',').filter { it.isNotEmpty() }.mapNotNull { byId[it] }
        val (s1, o1) = if (ids.isEmpty()) {
            focus(); scale to offset
        } else {
            val l = ids.minOf { x(it) }; val t = ids.minOf { y(it) }
            val r = ids.maxOf { x(it) + w(it) }; val b = ids.maxOf { y(it) + (heights[it.id] ?: 80f) }
            val s = min(box.width / ((r - l) * density * 1.45f), box.height / ((b - t) * density * 1.45f)).coerceIn(FOLLOW_MIN_SCALE, 0.95f)
            s to Offset(box.width / 2f - (l + r) / 2f * density * s, box.height / 2f - (t + b) / 2f * density * s)
        }
        if (still || !fitted) { scale = s1; offset = o1; fitted = true; return@LaunchedEffect }
        val s0 = scale; val o0 = offset
        androidx.compose.animation.core.animate(0f, 1f, animationSpec = androidx.compose.animation.core.tween(450)) { f, _ ->
            scale = s0 + (s1 - s0) * f
            offset = o0 + (o1 - o0) * f
        }
    }

    val editBackground = androidx.compose.runtime.rememberUpdatedState(edit?.onBackground ?: {})
    val editMove = androidx.compose.runtime.rememberUpdatedState<(String, Position) -> Unit>(edit?.onMove ?: { _, _ -> })
    Box(
        modifier.fillMaxSize().clipToBounds().background(CrundiColors.BgPrimary).onSizeChanged { box = it }
            .drawBehind {
                val step = 16.dp.toPx() * scale
                if (step > 4f) {
                    var gx = offset.x % step
                    while (gx < size.width) { var gy = offset.y % step; while (gy < size.height) { drawCircle(CrundiColors.BorderSubtle, 1.dp.toPx(), Offset(gx, gy)); gy += step }; gx += step }
                }
            }
            .then(if (edit != null) Modifier.pointerInput(Unit) { detectTapGestures(onTap = { editBackground.value() }) } else Modifier)
            .pointerInput(Unit) {
                detectTransformGestures { centroid, pan, zoom, _ ->
                    onManualMove()
                    val ns = (scale * zoom).coerceIn(0.15f, 3f)
                    offset = (offset - centroid) * (ns / scale) + centroid + pan
                    scale = ns
                }
            },
    ) {
        Layout(
            content = {
                for (n in nodes) {
                    Box(Modifier.layoutId(n.id).width(w(n).dp).onSizeChanged { heights[n.id] = it.height / density }) {
                        if (edit == null) StepCard(n, factory, view, n.id == selected) { onSelect(if (selected == n.id) null else n.id) }
                        else {
                            val c = edit.connectFrom
                            StepCard(
                                n, factory, view, n.id == selected || c?.first == n.id, onClick = { onSelect(n.id) },
                                dim = c != null && c.first != n.id && !edit.canConnectTo(n.id),
                                gestures = Modifier.editGestures(
                                    n.id, onTap = { onSelect(n.id) }, onLongPress = { edit.onLongPress(n.id) },
                                    onDrag = { d -> drag = n.id to d },
                                    onDragEnd = { d ->
                                        drag = null
                                        editMove.value(n.id, Position(n.position.x + d.x, n.position.y + d.y))
                                    },
                                ),
                            )
                        }
                    }
                }
                // A dot halfway along each connection, to tap it (the editor only).
                if (edit != null) for (e in factory.edges) {
                    if (byId[e.source] == null || byId[e.target] == null) continue
                    Box(Modifier.layoutId("e:" + e.id)) { EdgeDot(e, e.id == edit.selectedEdge) { edit.onEdge(e.id) } }
                }
            },
            modifier = Modifier
                .wrapContentSize(Alignment.TopStart, unbounded = true)
                .graphicsLayer { translationX = offset.x; translationY = offset.y; scaleX = scale; scaleY = scale; transformOrigin = TransformOrigin(0f, 0f) }
                .drawBehind {
                    val d = density
                    for (e in factory.edges) {
                        val s = byId[e.source] ?: continue
                        val t = byId[e.target] ?: continue
                        val taken = run != null && view.visited(s.id) && view.visited(t.id)
                        if (e.kind == "note") {
                            val sx = (x(s) + w(s)) * d; val sy = (y(s) + (heights[s.id] ?: 80f) / 2) * d
                            val ex = x(t) * d; val ey = (y(t) + (heights[t.id] ?: 80f) / 2) * d
                            val path = Path().apply { moveTo(sx, sy); cubicTo(sx + 60 * d, sy, ex - 60 * d, ey, ex, ey) }
                            drawPath(path, if (edit?.selectedEdge == e.id) CrundiColors.Accent else CrundiColors.Yellow, style = Stroke((if (edit?.selectedEdge == e.id) 2.2f else 1.6f) * d, pathEffect = PathEffect.dashPathEffect(floatArrayOf(5 * d, 4 * d))))
                            continue
                        }
                        val hs = handlesOf(s, factory.edges)
                        val hi = hs.indexOfFirst { it.first == e.sourceHandle }.coerceAtLeast(0)
                        val slots = max(1, hs.size)
                        val sx = (x(s) + w(s) * (hi + 0.5f) / slots) * d
                        val sy = (y(s) + (heights[s.id] ?: 80f)) * d
                        val ex = (x(t) + w(t) / 2) * d
                        val ey = y(t) * d
                        val loop = e.maxRounds > 0 || ey <= sy
                        val picked = edit?.selectedEdge == e.id
                        val color = when { picked -> CrundiColors.Accent; taken -> CrundiColors.Green; loop -> CrundiColors.Sky; else -> CrundiColors.TextSecondary }
                        val path = Path()
                        if (ey > sy + 10 * d) {
                            val dy = (ey - sy) * 0.45f
                            path.moveTo(sx, sy); path.cubicTo(sx, sy + dy, ex, ey - dy, ex, ey)
                        } else {
                            // Back up the flow: round the right-hand side.
                            val lane = max(x(s) + w(s), x(t) + w(t)) * d + 30 * d
                            path.moveTo(sx, sy); path.cubicTo(sx, sy + 40 * d, lane, sy + 40 * d, lane, (sy + ey) / 2)
                            path.cubicTo(lane, ey - 40 * d, ex, ey - 40 * d, ex, ey)
                        }
                        drawPath(path, color, style = Stroke((if (taken || picked) 2.2f else 1.6f) * d))
                        // Arrowhead into the target's top.
                        val a = 6 * d
                        val head = Path().apply { moveTo(ex, ey); lineTo(ex - a, ey - a * 1.4f); lineTo(ex + a, ey - a * 1.4f); close() }
                        drawPath(head, color)
                        if (e.maxRounds > 0 && edit == null) {
                            // A small "max N" badge halfway.
                            drawRoundRect(CrundiColors.BgSecondary, Offset((sx + ex) / 2 - 22 * d, (sy + ey) / 2 - 8 * d), androidx.compose.ui.geometry.Size(44 * d, 16 * d), CornerRadius(8 * d))
                        }
                    }
                },
        ) { measurables, _ ->
            val placeables = measurables.map { it.layoutId as String to it.measure(Constraints()) }
            val hts = placeables.filter { byId.containsKey(it.first) }.associate { it.first to it.second.height / density }
            layout((width * density).toInt(), (height * density).toInt()) {
                for ((id, p) in placeables) {
                    if (id.startsWith("e:")) {
                        val e = factory.edges.firstOrNull { it.id == id.substring(2) } ?: continue
                        val mid = edgeMid(e, byId, hts, factory.edges, ::x, ::y, ::w) ?: continue
                        p.place((mid.x * density).toInt() - p.width / 2, (mid.y * density).toInt() - p.height / 2)
                        continue
                    }
                    val n = byId[id] ?: continue
                    p.place((x(n) * density).toInt(), (y(n) * density).toInt())
                }
            }
        }
        // Round-limit labels, over the edges (text cannot be drawn in drawBehind without a measurer).
        Column(Modifier.align(Alignment.BottomEnd).padding(10.dp).background(CrundiColors.BgSecondary, Dimens.Shape).border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape)) {
            CrundiIconButton("plus", { onManualMove(); scale = (scale * 1.25f).coerceAtMost(3f) }, contentDescription = "Zoom in", touch = 40.dp)
            CrundiIconButton("minus", { onManualMove(); scale = (scale / 1.25f).coerceAtLeast(0.15f) }, contentDescription = "Zoom out", touch = 40.dp)
            CrundiIconButton("maximize", { onManualMove(); fit() }, contentDescription = "Fit to screen", touch = 40.dp)
            CrundiIconButton("pin", { onManualMove(); focus() }, contentDescription = if (run != null) "Go to the current step" else "Go to the start", touch = 40.dp)
        }
    }
}

/** Halfway along a connection, in graph dp: where its dot goes (the curves' midpoints, as drawn). */
private fun edgeMid(
    e: FactoryEdge, byId: Map<String, FactoryNode>, hts: Map<String, Float>, edges: List<FactoryEdge>,
    x: (FactoryNode) -> Float, y: (FactoryNode) -> Float, w: (FactoryNode) -> Float,
): Offset? {
    val s = byId[e.source] ?: return null
    val t = byId[e.target] ?: return null
    val hs = hts[s.id] ?: 80f; val ht = hts[t.id] ?: 80f
    if (e.kind == "note") return Offset((x(s) + w(s) + x(t)) / 2, (y(s) + hs / 2 + y(t) + ht / 2) / 2)
    val handles = handlesOf(s, edges)
    val hi = handles.indexOfFirst { it.first == e.sourceHandle }.coerceAtLeast(0)
    val sx = x(s) + w(s) * (hi + 0.5f) / max(1, handles.size)
    val sy = y(s) + hs
    val ex = x(t) + w(t) / 2; val ey = y(t)
    return if (ey > sy + 10) Offset((sx + ex) / 2, (sy + ey) / 2) else Offset(max(x(s) + w(s), x(t) + w(t)) + 30, (sy + ey) / 2)
}

/** A connection's dot: its round limit when it has one, accent when selected. */
@Composable
private fun EdgeDot(e: FactoryEdge, selected: Boolean, onClick: () -> Unit) {
    val c = when { selected -> CrundiColors.Accent; e.kind == "note" -> CrundiColors.Yellow; e.maxRounds > 0 -> CrundiColors.Sky; else -> CrundiColors.TextSecondary }
    Box(
        Modifier.size(width = if (e.maxRounds > 0) 52.dp else 30.dp, height = 30.dp).clip(RoundedCornerShape(15.dp)).clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        if (e.maxRounds > 0) Text(
            "max ${e.maxRounds}", style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontSize = 10.sp, fontWeight = FontWeight.SemiBold, color = c),
            modifier = Modifier.clip(RoundedCornerShape(8.dp)).background(CrundiColors.BgSecondary).border(Dimens.Hairline, c, RoundedCornerShape(8.dp)).padding(horizontal = 6.dp, vertical = 1.dp),
        )
        else Box(Modifier.size(if (selected) 12.dp else 10.dp).clip(RoundedCornerShape(6.dp)).background(CrundiColors.BgSecondary).border(1.5.dp, c, RoundedCornerShape(6.dp)))
    }
}

/**
 * A step's touches in the editor: tap; long press and drag to move (the
 * distance in dp, as it goes and at the end); long press and let go to
 * connect from it. Panning the canvas takes a touch that moves before the
 * long press.
 */
private fun Modifier.editGestures(key: String, onTap: () -> Unit, onLongPress: () -> Unit, onDrag: (Offset) -> Unit, onDragEnd: (Offset) -> Unit): Modifier = composed {
    val tap by androidx.compose.runtime.rememberUpdatedState(onTap)
    val long by androidx.compose.runtime.rememberUpdatedState(onLongPress)
    val dragTo by androidx.compose.runtime.rememberUpdatedState(onDrag)
    val dragEnd by androidx.compose.runtime.rememberUpdatedState(onDragEnd)
    val haptics = androidx.compose.ui.platform.LocalHapticFeedback.current
    pointerInput(key) {
        awaitEachGesture {
            val down = awaitFirstDown()
            var up: androidx.compose.ui.input.pointer.PointerInputChange? = null
            val r = withTimeoutOrNull(viewConfiguration.longPressTimeoutMillis) { up = waitForUpOrCancellation(); if (up != null) 1 else 2 } ?: 0
            if (r == 1) { up?.consume(); tap(); return@awaitEachGesture }
            if (r == 2) return@awaitEachGesture
            haptics.performHapticFeedback(androidx.compose.ui.hapticfeedback.HapticFeedbackType.LongPress)
            var moved = Offset.Zero
            var dragging = false
            while (true) {
                val ev = awaitPointerEvent()
                val c = ev.changes.firstOrNull { it.id == down.id } ?: break
                if (!c.pressed) { c.consume(); break }
                val d = c.positionChange()
                if (d != Offset.Zero) {
                    moved += d
                    if (!dragging && moved.getDistance() > viewConfiguration.touchSlop) dragging = true
                    if (dragging) dragTo(moved / density)
                    c.consume()
                }
            }
            if (dragging) dragEnd(moved / density) else long()
        }
    }.semantics { onClick { tap(); true }; onLongClick { long(); true } }
}

@Composable
private fun StepCard(n: FactoryNode, f: Factory, view: RunView, selected: Boolean, dim: Boolean = false, gestures: Modifier? = null, onClick: () -> Unit) {
    val look = lookOf(n.type)
    val state = view.state(n)
    val shape = RoundedCornerShape(10.dp)
    val border = when {
        selected -> CrundiColors.Accent
        state == NodeState.Active -> CrundiColors.Green
        state == NodeState.Visited -> CrundiColors.Green.copy(alpha = 0.45f)
        else -> CrundiColors.Border
    }
    val pill = view.pill(n)
    Box(Modifier.padding(top = if (pill != null) 0.dp else 0.dp)) {
        Column(
            Modifier.fillMaxWidth()
                .alpha(if (dim) 0.35f else if (state == NodeState.Retired) 0.6f else 1f)
                .drawBehind {
                    val ring = when { selected -> CrundiColors.AccentDim; state == NodeState.Active -> CrundiColors.GreenDim; else -> null }
                    if (ring != null) {
                        val r = 3.dp.toPx()
                        drawRoundRect(ring, Offset(-r, -r), androidx.compose.ui.geometry.Size(size.width + 2 * r, size.height + 2 * r), CornerRadius(13.dp.toPx()))
                    }
                }
                .clip(shape).background(CrundiColors.BgCard)
                .then(if (state == NodeState.Retired) Modifier.dashedBorder(border, 10.dp) else Modifier.border(if (selected || state == NodeState.Active) 1.5.dp else Dimens.Hairline, border, shape))
                .then(gestures ?: Modifier.clickable(onClick = onClick)),
        ) {
            Column(Modifier.padding(horizontal = 11.dp, vertical = 9.dp), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    CrundiIcon(look.icon, size = 14.dp, tint = look.head)
                    Text(look.kind.uppercase(), style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontSize = 10.5.sp, fontWeight = FontWeight.SemiBold, color = look.head, letterSpacing = 0.5.sp))
                }
                Text(n.label.ifEmpty { look.kind }, style = CrundiType.Body.copy(fontSize = 13.5.sp, fontWeight = FontWeight.SemiBold, lineHeight = 17.sp), maxLines = 2, overflow = TextOverflow.Ellipsis)
                val doing = view.doing(n)
                if (doing != null) Row(Modifier.padding(top = 2.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Pulse(doing.state == "working", Modifier.padding(top = 4.dp))
                    Text(
                        androidx.compose.ui.text.buildAnnotatedString {
                            if (doing.who.isNotEmpty()) { pushStyle(androidx.compose.ui.text.SpanStyle(fontWeight = FontWeight.SemiBold)); append(doing.who); pop(); append(" ") }
                            append(doing.text)
                        },
                        style = CrundiType.Small.copy(fontSize = 11.5.sp, lineHeight = 15.sp, color = CrundiColors.TextPrimary), maxLines = 2, overflow = TextOverflow.Ellipsis,
                    )
                } else {
                    val body = (if (state == NodeState.Active) view.lastSummary(n.id)?.take(110) else null) ?: nodeBody(n, f)
                    if (body.isNotEmpty()) Text(body, style = CrundiType.Small.copy(fontSize = 11.5.sp, lineHeight = 15.sp, color = CrundiColors.TextMuted), maxLines = 2, overflow = TextOverflow.Ellipsis)
                }
                val skills = f.roster.firstOrNull { it.id == n.employeeId }?.skills.orEmpty()
                if (skills.isNotEmpty()) Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    for (s in skills.take(3)) Text("/$s", style = CrundiType.Meta.copy(fontSize = 10.sp, color = CrundiColors.Green), modifier = Modifier.clip(RoundedCornerShape(6.dp)).background(CrundiColors.GreenDim).padding(horizontal = 5.dp, vertical = 1.dp))
                }
            }
            val hs = handlesOf(n, f.edges)
            if (hs.size > 1 || hs.any { it.second.isNotEmpty() }) {
                Box(Modifier.fillMaxWidth().background(CrundiColors.BorderSubtle).padding(top = 1.dp))
                Row(Modifier.fillMaxWidth().padding(vertical = 5.dp)) {
                    for ((k, label) in hs) Text(
                        label, style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontSize = 10.5.sp, color = if (k == "finish") CrundiColors.Green else CrundiColors.TextSecondary),
                        textAlign = TextAlign.Center, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f),
                    )
                }
            }
        }
        if (pill != null) Text(
            pill.first, style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontSize = 10.sp, fontWeight = FontWeight.Bold, color = CrundiColors.BgPrimary),
            modifier = Modifier.offset(x = (-6).dp, y = (-10).dp).clip(RoundedCornerShape(10.dp)).background(pill.second).padding(horizontal = 7.dp, vertical = 2.dp),
        )
    }
}

/** Below this the follow camera makes step cards too small to read on a phone. */
private const val FOLLOW_MIN_SCALE = 0.65f

/**
 * The live dot beside someone at work: green and breathing while their turn
 * runs, amber when they wait on a permission, grey otherwise. Still under
 * LocalInspectionMode (screenshots never go idle with an endless animation).
 */
@Composable
internal fun Pulse(busy: Boolean, modifier: Modifier = Modifier, blocked: Boolean = false) {
    val color = when { blocked -> CrundiColors.Yellow; busy -> CrundiColors.Green; else -> CrundiColors.TextSecondary }
    val still = androidx.compose.ui.platform.LocalInspectionMode.current
    val ring = if (busy && !blocked && !still) {
        val t = androidx.compose.animation.core.rememberInfiniteTransition(label = "pulse")
        t.animateFloat(0f, 1f, androidx.compose.animation.core.infiniteRepeatable(androidx.compose.animation.core.tween(1400)), label = "ring").value
    } else 0f
    Box(
        modifier.size(8.dp).drawBehind {
            if (ring > 0f) {
                val k = if (ring < 0.5f) ring * 2 else (1 - ring) * 2
                drawCircle(CrundiColors.Green.copy(alpha = 0.55f * (1 - k)), radius = size.minDimension / 2 + 5.dp.toPx() * k)
            }
            drawCircle(color)
        },
    )
}

/** The desktop's run-status chip: label and colour, with waitingOn taking over while running. */
fun runStatus(status: String, waitingOn: String): Pair<String, Color> {
    if (status == "running" && waitingOn.isNotEmpty()) return when (waitingOn) {
        "usage" -> "Paused: usage limit"; "paused" -> "Paused"; "cap" -> "Reached its limit"; "ask" -> "A question for you"
        "manager" -> "The Manager is talking to you"; else -> waitingOn
    } to CrundiColors.Yellow
    return when (status) {
        "starting" -> "Starting" to CrundiColors.TextSecondary
        "running" -> "Running" to CrundiColors.Green
        "waiting" -> "Waiting on you" to CrundiColors.Yellow
        "paused" -> "Paused" to CrundiColors.Yellow
        "finishing" -> "Done: your decision" to CrundiColors.Sky
        "finished" -> "Finished" to CrundiColors.Sky
        "crashed" -> "Crashed" to CrundiColors.Red
        "stopped" -> "Stopped" to CrundiColors.Red
        else -> status.replaceFirstChar { it.uppercase() } to CrundiColors.TextSecondary
    }
}

/** A coloured outline chip: the colour on the text, the border at 40%. */
@Composable
fun OutlineChip(text: String, color: Color, modifier: Modifier = Modifier) {
    Text(
        text, style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontSize = 11.5.sp, fontWeight = FontWeight.SemiBold, color = color), maxLines = 1,
        modifier = modifier.clip(RoundedCornerShape(10.dp)).border(Dimens.Hairline, color.copy(alpha = 0.4f), RoundedCornerShape(10.dp)).padding(horizontal = 8.dp, vertical = 2.dp),
    )
}
