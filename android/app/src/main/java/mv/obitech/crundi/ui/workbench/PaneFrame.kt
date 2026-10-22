package mv.obitech.crundi.ui.workbench

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay
import mv.obitech.crundi.api.model.AgentState
import mv.obitech.crundi.ui.components.CrundiDivider
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/** What a pane's header can do. Everything defaults to nothing, so a test or a kind that lacks one just leaves it out. */
interface PaneActions {
    fun rename(v: PaneView) {}
    fun close(v: PaneView) {}
    fun full(v: PaneView, on: Boolean) {}
    fun font(v: PaneView, delta: Int) {}
    fun bubble(v: PaneView) {}
    fun plan(v: PaneView) {}
    fun split(v: PaneView, dir: Mosaic.Dir) {}
    fun move(v: PaneView, by: Int) {}
    fun refresh(v: PaneView) {}
    /** mode: resume (as it was), compact (resume, then summarise), new (a fresh conversation in the same pane). */
    fun resume(v: PaneView, mode: String = "resume") {}
    fun focus(v: PaneView) {}
    object None : PaneActions
}

/** The web's status dot (.term-status-dot): green running, yellow working, accent needs input, sky waiting, muted exited. */
fun dotColor(v: PaneView): Color = when {
    v.exited -> CrundiColors.TextMuted
    v.state == AgentState.Working -> CrundiColors.Yellow
    v.state == AgentState.NeedsInput -> CrundiColors.Accent
    v.state == AgentState.Waiting -> CrundiColors.Sky
    else -> CrundiColors.Green
}

/**
 * One pane, as the mobile web draws a workbench cell: a 1px border (accent
 * when it is the pane in use), the header, and the body below it.
 */
@Composable
fun PaneFrame(
    v: PaneView,
    actions: PaneActions,
    modifier: Modifier = Modifier,
    focused: Boolean = false,
    full: Boolean = false,
    body: @Composable ColumnScope.() -> Unit,
) {
    val shape = if (full) RoundedCornerShape(0.dp) else RoundedCornerShape(Dimens.Radius)
    Column(
        modifier
            .clip(shape)
            .background(CrundiColors.BgPrimary)
            .border(if (focused && !full) 1.5.dp else Dimens.Hairline, if (focused && !full) CrundiColors.Accent else CrundiColors.Border, shape),
    ) {
        PaneHeader(v, actions, full)
        CrundiDivider(strong = true)
        Column(Modifier.fillMaxWidth().weight(1f), content = body)
    }
}

/**
 * The pane header (headHtmlLive, headHtmlPanel, headHtmlParked): drag handle,
 * status dot, title (tap to rename), the state badge, then full screen, the
 * "more" menu and close. Close arms on the first tap and acts on the second,
 * as the web's does.
 */
@Composable
fun PaneHeader(v: PaneView, actions: PaneActions, full: Boolean = false) {
    var menu by remember { mutableStateOf(false) }
    var armed by remember { mutableStateOf(false) }
    LaunchedEffect(armed) { if (armed) { delay(3000); armed = false } }
    val live = v.kind == PaneKind.Chat || v.kind == PaneKind.Terminal
    Row(
        // Full screen has no drag handle, so the dot needs the room it gave.
        Modifier.fillMaxWidth().background(CrundiColors.BgSecondary).padding(start = if (full) 12.dp else 2.dp, end = 6.dp, top = 5.dp, bottom = 5.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        if (!full) DragHandle()
        if (live || v.kind == PaneKind.Parked) Box(Modifier.size(9.dp).clip(CircleShape).background(dotColor(v)))
        else CrundiIcon(v.kind.icon, size = 15.dp, tint = CrundiColors.AccentHover)
        // Title and badge take what is left; the buttons stay at the right edge.
        Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(
                v.title, style = CrundiType.Body.copy(fontWeight = FontWeight.SemiBold),
                maxLines = 1, overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f, fill = false).clip(Dimens.ShapeSm)
                    .then(if (live) Modifier.clickable { actions.rename(v) } else Modifier).padding(horizontal = 4.dp, vertical = 2.dp),
            )
            StateBadge(v)
            if (v.kind == PaneKind.Parked) KindTag(if (v.parkedAs == PaneKind.Chat) "chat" else "terminal")
        }
        if (v.kind != PaneKind.Pending && v.kind != PaneKind.Parked) HeadButton("maximize", if (full) "Leave full screen" else "Full screen", active = full) { actions.full(v, !full) }
        if (v.kind != PaneKind.Pending) Box {
            HeadButton("more", "More") { menu = true }
            PaneMenu(v, actions, menu) { menu = false }
        }
        if (!full) {
            if (armed) HeadButton(null, "Close", text = "Close?", danger = true) { armed = false; actions.close(v) }
            else HeadButton("x", if (v.kind == PaneKind.Chat) "Close chat" else if (v.kind == PaneKind.Terminal) "Close terminal" else "Close panel") {
                if (v.kind == PaneKind.Chat || v.kind == PaneKind.Terminal) armed = true else actions.close(v)
            }
        }
    }
}

/** The two columns of dots the web draws as the drag handle. */
@Composable
private fun DragHandle() {
    Canvas(Modifier.width(10.dp).height(16.dp)) {
        val r = 1.3.dp.toPx()
        for (cx in listOf(size.width * 0.3f, size.width * 0.7f)) for (cy in listOf(0.2f, 0.5f, 0.8f)) drawCircle(CrundiColors.TextMuted, r, Offset(cx, size.height * cy))
    }
}

/** WORKING, NEEDS INPUT, WAITING (.term-agent-badge). */
@Composable
fun StateBadge(v: PaneView) {
    if (v.exited) return
    val (text, fg, bg) = when (v.state) {
        AgentState.Working -> Triple("working", CrundiColors.Yellow, CrundiColors.YellowDim)
        AgentState.NeedsInput -> Triple("needs input", CrundiColors.AccentHover, CrundiColors.AccentDim)
        AgentState.Waiting -> Triple("waiting", CrundiColors.Sky, CrundiColors.SkyDim)
        else -> return
    }
    Text(
        text.uppercase(),
        style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontSize = 10.sp, fontWeight = FontWeight.Bold, letterSpacing = 0.04.em, color = fg),
        maxLines = 1,
        modifier = Modifier.clip(RoundedCornerShape(50)).background(bg).padding(horizontal = 7.dp, vertical = 2.dp),
    )
}

/** The small "chat" / "terminal" tag (.term-kind-tag). */
@Composable
fun KindTag(text: String) {
    Text(
        text, style = CrundiType.Meta.copy(fontSize = 10.sp, color = CrundiColors.TextMuted), maxLines = 1,
        modifier = Modifier.clip(RoundedCornerShape(4.dp)).border(Dimens.Hairline, CrundiColors.Border, RoundedCornerShape(4.dp)).padding(horizontal = 5.dp, vertical = 1.dp),
    )
}

/** A header button (.term-head-btn / .term-font-btn on a phone: 34 tall, 38 wide, bordered). */
@Composable
fun HeadButton(icon: String?, description: String, text: String? = null, active: Boolean = false, danger: Boolean = false, onClick: () -> Unit) {
    val fg = when { danger -> CrundiColors.Red; active -> CrundiColors.AccentHover; else -> CrundiColors.TextSecondary }
    val border = when { danger -> CrundiColors.Red; active -> CrundiColors.Accent; else -> CrundiColors.Border }
    // No bigger than the top row's icons: a bare icon, boxed only when it
    // means something (full screen on, close armed). The boxed 38dp buttons
    // made every pane header as tall as a toolbar.
    val boxed = danger || active
    Box(
        Modifier.defaultMinSize(minWidth = 30.dp, minHeight = 28.dp).clip(Dimens.ShapeSm)
            .background(if (danger) CrundiColors.RedDim else Color.Transparent)
            .then(if (boxed) Modifier.border(Dimens.Hairline, border, Dimens.ShapeSm) else Modifier)
            .clickable(onClickLabel = description, onClick = onClick).padding(horizontal = 6.dp),
        contentAlignment = Alignment.Center,
    ) {
        if (icon != null) CrundiIcon(icon, size = 16.dp, tint = fg, contentDescription = description)
        else Text(text.orEmpty(), style = CrundiType.Small.copy(color = fg, fontWeight = FontWeight.SemiBold))
    }
}

/**
 * The "more" menu (headTools folded on a phone): text size A- / reset / A+,
 * the plan pill and "chat" tag for a chat, Bubble, move, split right / down,
 * and Refresh for a panel.
 */
@Composable
fun PaneMenu(v: PaneView, actions: PaneActions, open: Boolean, onDismiss: () -> Unit) {
    DropdownMenu(
        expanded = open, onDismissRequest = onDismiss,
        containerColor = CrundiColors.BgSecondary,
        border = androidx.compose.foundation.BorderStroke(Dimens.Hairline, CrundiColors.Border),
        shape = Dimens.Shape,
    ) {
        PaneMenuBody(v, actions, onDismiss)
    }
}

/** The menu's content, also drawn by itself in screenshot tests. */
@Composable
fun PaneMenuBody(v: PaneView, actions: PaneActions, onDismiss: () -> Unit = {}) {
    Column(Modifier.widthIn(min = 220.dp).padding(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        val live = v.kind == PaneKind.Chat || v.kind == PaneKind.Terminal
        // The browser too: its text grows and shrinks as the chat's does.
        if (live || v.kind == PaneKind.Notes || v.kind == PaneKind.Browser) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(horizontal = 4.dp, vertical = 2.dp)) {
                Text("Text size", style = CrundiType.Small, modifier = Modifier.weight(1f))
                HeadButton(null, "Smaller text", text = "A-") { actions.font(v, -1) }
                HeadButton("rotate-ccw", "Reset text size") { actions.font(v, 0) }
                HeadButton(null, "Larger text", text = "A+") { actions.font(v, 1) }
            }
        }
        if (v.kind == PaneKind.Chat) {
            val p = v.pane
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp), modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp)) {
                planTag(p?.planStatus.orEmpty(), p?.permissionMode.orEmpty())?.let { (label, color) ->
                    Text(label, style = CrundiType.Meta.copy(color = color), modifier = Modifier.clip(RoundedCornerShape(4.dp)).border(Dimens.Hairline, color, RoundedCornerShape(4.dp)).clickable { onDismiss(); actions.plan(v) }.padding(horizontal = 6.dp, vertical = 1.dp))
                }
                KindTag("chat")
            }
            MenuItem("layers", "Bubble") { onDismiss(); actions.bubble(v) }
        }
        if (v.kind != PaneKind.Parked) {
            MenuItem("chevron-left", "Move left") { onDismiss(); actions.move(v, -1) }
            MenuItem("chevron-right", "Move right") { onDismiss(); actions.move(v, 1) }
            MenuItem("split-right", "Split right") { onDismiss(); actions.split(v, Mosaic.Dir.Row) }
            MenuItem("split-down", "Split down") { onDismiss(); actions.split(v, Mosaic.Dir.Col) }
        }
        if (!live && v.kind != PaneKind.Parked) MenuItem("refresh", "Refresh") { onDismiss(); actions.refresh(v) }
    }
}

/** The plan pill's words and colour (planTagFor), or null. */
fun planTag(planStatus: String, permissionMode: String): Pair<String, Color>? = when {
    planStatus == "proposed" -> "plan" to CrundiColors.Yellow
    planStatus == "executing" -> "plan" to CrundiColors.Green
    planStatus == "rejected" -> "plan" to CrundiColors.Red
    permissionMode == "plan" -> "planning" to CrundiColors.Sky
    else -> null
}

@Composable
fun MenuItem(icon: String, label: String, danger: Boolean = false, enabled: Boolean = true, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clip(Dimens.ShapeSm).clickable(enabled = enabled, onClick = onClick).padding(horizontal = 10.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        val c = when { !enabled -> CrundiColors.TextMuted; danger -> CrundiColors.Red; else -> CrundiColors.TextPrimary }
        CrundiIcon(icon, size = 16.dp, tint = if (danger) CrundiColors.Red else CrundiColors.AccentHover)
        Text(label, style = CrundiType.Body.copy(color = c))
    }
}
