package mv.obitech.crundi.ui.planning

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

/** What the run says about each step: tokens on it, visited, blocked, retired. */
internal class RunView(val run: FactoryRun?) {
    private val visited: Set<String> = run?.history?.mapNotNull { it.nodeId.ifEmpty { null } }?.toSet().orEmpty() +
        run?.tokens?.map { it.nodeId }.orEmpty() + (run?.snapshot?.nodes?.filter { it.type == "start" }?.map { it.id }.takeIf { run != null }.orEmpty())

    fun tokens(id: String) = run?.tokens?.filter { it.nodeId == id }.orEmpty()

    fun state(n: FactoryNode): NodeState {
        if (run == null) return NodeState.Idle
        val emp = n.employeeId?.let { e -> run.employees[e] }
        return when {
            tokens(n.id).isNotEmpty() -> NodeState.Active
            emp?.retired == true -> NodeState.Retired
            n.id in visited -> NodeState.Visited
            else -> NodeState.Idle
        }
    }

    fun visited(id: String) = id in visited

    /** The pill over a step with tokens: words and colour, as the desktop's. */
    fun pill(n: FactoryNode): Pair<String, Color>? {
        val t = tokens(n.id)
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
 * A factory's flow, drawn as the desktop's viewer: steps as cards where the
 * editor put them, flow edges from out handles at the bottom to the next
 * step's top, note edges dashed amber from side to side. With a [run] the
 * steps light up: active (green glow and a pill), visited, retired.
 * Pinch or the buttons to zoom; drag to pan; tap a step to select it.
 */
@Composable
fun FactoryGraph(factory: Factory, run: FactoryRun?, selected: String?, onSelect: (String?) -> Unit, modifier: Modifier = Modifier) {
    val view = remember(run) { RunView(run) }
    val nodes = factory.nodes
    if (nodes.isEmpty()) { Box(modifier.fillMaxSize(), contentAlignment = Alignment.Center) { Text("This factory has no steps yet.", style = CrundiType.Small) }; return }
    val heights = remember(factory) { mutableStateMapOf<String, Float>() }
    val minX = nodes.minOf { it.position.x }.toFloat(); val minY = nodes.minOf { it.position.y }.toFloat()
    val pad = 28f
    fun w(n: FactoryNode) = if (n.type == "start" || n.type == "end") 170f else 220f
    fun x(n: FactoryNode) = n.position.x.toFloat() - minX + pad
    fun y(n: FactoryNode) = n.position.y.toFloat() - minY + pad
    val byId = nodes.associateBy { it.id }
    val width = nodes.maxOf { x(it) + w(it) } + pad + 40f
    val height = nodes.maxOf { y(it) + (heights[it.id] ?: 80f) } + pad + 20f

    var scale by remember { mutableFloatStateOf(1f) }
    var offset by remember { mutableStateOf(Offset.Zero) }
    var box by remember { mutableStateOf(IntSize.Zero) }
    var fitted by remember(factory.id) { mutableStateOf(false) }
    val density = LocalDensity.current.density
    fun fit() {
        if (box.width == 0) return
        val s = min(box.width / (width * density), box.height / (height * density)).coerceIn(0.2f, 1.2f)
        scale = s
        offset = Offset((box.width - width * density * s) / 2f, max(0f, (box.height - height * density * s) / 2f))
    }
    LaunchedEffect(box, heights.size == nodes.size) { if (!fitted && box.width > 0 && heights.size == nodes.size) { fit(); fitted = true } }

    Box(
        modifier.fillMaxSize().clipToBounds().background(CrundiColors.BgPrimary).onSizeChanged { box = it }
            .drawBehind {
                val step = 16.dp.toPx() * scale
                if (step > 4f) {
                    var gx = offset.x % step
                    while (gx < size.width) { var gy = offset.y % step; while (gy < size.height) { drawCircle(CrundiColors.BorderSubtle, 1.dp.toPx(), Offset(gx, gy)); gy += step }; gx += step }
                }
            }
            .pointerInput(Unit) {
                detectTransformGestures { centroid, pan, zoom, _ ->
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
                        StepCard(n, factory, view, n.id == selected) { onSelect(if (selected == n.id) null else n.id) }
                    }
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
                            drawPath(path, CrundiColors.Yellow, style = Stroke(1.6f * d, pathEffect = PathEffect.dashPathEffect(floatArrayOf(5 * d, 4 * d))))
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
                        val color = when { taken -> CrundiColors.Green; loop -> CrundiColors.Sky; else -> CrundiColors.TextSecondary }
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
                        drawPath(path, color, style = Stroke((if (taken) 2.2f else 1.6f) * d))
                        // Arrowhead into the target's top.
                        val a = 6 * d
                        val head = Path().apply { moveTo(ex, ey); lineTo(ex - a, ey - a * 1.4f); lineTo(ex + a, ey - a * 1.4f); close() }
                        drawPath(head, color)
                        if (e.maxRounds > 0) {
                            // A small "max N" badge halfway.
                            drawRoundRect(CrundiColors.BgSecondary, Offset((sx + ex) / 2 - 22 * d, (sy + ey) / 2 - 8 * d), androidx.compose.ui.geometry.Size(44 * d, 16 * d), CornerRadius(8 * d))
                        }
                    }
                },
        ) { measurables, _ ->
            val placeables = measurables.map { it.layoutId as String to it.measure(Constraints()) }
            layout((width * density).toInt(), (height * density).toInt()) {
                for ((id, p) in placeables) {
                    val n = byId[id] ?: continue
                    p.place((x(n) * density).toInt(), (y(n) * density).toInt())
                }
            }
        }
        // Round-limit labels, over the edges (text cannot be drawn in drawBehind without a measurer).
        Column(Modifier.align(Alignment.BottomEnd).padding(10.dp).background(CrundiColors.BgSecondary, Dimens.Shape).border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape)) {
            CrundiIconButton("plus", { scale = (scale * 1.25f).coerceAtMost(3f) }, contentDescription = "Zoom in", touch = 40.dp)
            CrundiIconButton("minus", { scale = (scale / 1.25f).coerceAtLeast(0.15f) }, contentDescription = "Zoom out", touch = 40.dp)
            CrundiIconButton("maximize", { fit() }, contentDescription = "Fit to screen", touch = 40.dp)
        }
    }
}

@Composable
private fun StepCard(n: FactoryNode, f: Factory, view: RunView, selected: Boolean, onClick: () -> Unit) {
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
                .alpha(if (state == NodeState.Retired) 0.6f else 1f)
                .drawBehind {
                    val ring = when { selected -> CrundiColors.AccentDim; state == NodeState.Active -> CrundiColors.GreenDim; else -> null }
                    if (ring != null) {
                        val r = 3.dp.toPx()
                        drawRoundRect(ring, Offset(-r, -r), androidx.compose.ui.geometry.Size(size.width + 2 * r, size.height + 2 * r), CornerRadius(13.dp.toPx()))
                    }
                }
                .clip(shape).background(CrundiColors.BgCard)
                .then(if (state == NodeState.Retired) Modifier.dashedBorder(border, 10.dp) else Modifier.border(if (selected || state == NodeState.Active) 1.5.dp else Dimens.Hairline, border, shape))
                .clickable(onClick = onClick),
        ) {
            Column(Modifier.padding(horizontal = 11.dp, vertical = 9.dp), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    CrundiIcon(look.icon, size = 14.dp, tint = look.head)
                    Text(look.kind.uppercase(), style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontSize = 10.5.sp, fontWeight = FontWeight.SemiBold, color = look.head, letterSpacing = 0.5.sp))
                }
                Text(n.label.ifEmpty { look.kind }, style = CrundiType.Body.copy(fontSize = 13.5.sp, fontWeight = FontWeight.SemiBold, lineHeight = 17.sp), maxLines = 2, overflow = TextOverflow.Ellipsis)
                val body = (if (state == NodeState.Active) view.lastSummary(n.id)?.take(110) else null) ?: nodeBody(n, f)
                if (body.isNotEmpty()) Text(body, style = CrundiType.Small.copy(fontSize = 11.5.sp, lineHeight = 15.sp, color = CrundiColors.TextMuted), maxLines = 2, overflow = TextOverflow.Ellipsis)
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
