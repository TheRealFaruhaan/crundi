package mv.obitech.crundi.ui.shell

import mv.obitech.crundi.ui.mini.miniLogo
import mv.obitech.crundi.ui.mini.miniTarget

import androidx.activity.compose.BackHandler
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.layout.positionInParent
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import mv.obitech.crundi.R
import mv.obitech.crundi.ui.components.CrundiDivider
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.ListRow
import mv.obitech.crundi.ui.components.SectionHeader
import mv.obitech.crundi.ui.components.StateDot
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens
import mv.obitech.crundi.ui.workbench.MenuItem
import mv.obitech.crundi.ui.workbench.PaneKind
import mv.obitech.crundi.ui.workbench.WbLayout

/** The Crundi mark as a still picture (the drawable), for large places. The top bar uses [RobotHead]. */
@Composable
fun CrundiMark(modifier: Modifier = Modifier, size: androidx.compose.ui.unit.Dp = 24.dp) {
    Image(painterResource(R.drawable.ic_logo), contentDescription = "Crundi", modifier = modifier.size(size))
}

/** What the shell's controls do. Everything defaults to nothing (tests). */
class ShellCallbacks(
    val onArea: (Area) -> Unit = {},
    val onProject: (String) -> Unit = {},
    val onApprovals: () -> Unit = {},
    val onUsage: () -> Unit = {},
    val onPanels: () -> Unit = {},
    val onAddProject: () -> Unit = {},
    val onAppSettings: () -> Unit = {},
    val onPairAnother: () -> Unit = {},
    /** The "+" beside the Workbench tab. */
    val onAdd: (PaneKind) -> Unit = {},
    val onFit: () -> Unit = {},
    val onToggleLayout: () -> Unit = {},
    val layout: WbLayout = WbLayout.Columns,
)

/**
 * The frame round the app, as the mobile web: the top bar, the icon tab strip
 * (on an area's front page), and the projects drawer sliding in from the left
 * under the top bar. Stateless apart from the add menu, so a screenshot test
 * can draw it; [drawerOpen] is hoisted for back handling.
 */
@Composable
fun ShellScaffold(
    data: ShellData,
    current: Area?,
    showChrome: Boolean,
    callbacks: ShellCallbacks,
    drawerOpen: Boolean,
    onDrawer: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
    addMenuOpen: Boolean = false,
    /** Drawn over the content under the top bar, like the drawer: the approvals dropdown. */
    overlay: @Composable () -> Unit = {},
    content: @Composable () -> Unit,
) {
    BackHandler(enabled = drawerOpen) { onDrawer(false) }
    // A pane full screen: the top bar and tabs slide away and the status bar
    // goes, and the pane grows into the room (the workbench keeps it alive,
    // so a chat does not load again).
    val paneFull = FullScreen.pane != null
    val view = androidx.compose.ui.platform.LocalView.current
    androidx.compose.runtime.LaunchedEffect(paneFull) {
        val window = (view.context as? android.app.Activity)?.window ?: return@LaunchedEffect
        val ctl = androidx.core.view.WindowCompat.getInsetsController(window, view)
        ctl.systemBarsBehavior = androidx.core.view.WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        if (paneFull) ctl.hide(androidx.core.view.WindowInsetsCompat.Type.statusBars()) else ctl.show(androidx.core.view.WindowInsetsCompat.Type.statusBars())
    }
    Column(modifier.fillMaxSize().background(CrundiColors.BgPrimary)) {
        androidx.compose.animation.AnimatedVisibility(showChrome && !paneFull, enter = androidx.compose.animation.expandVertically() + fadeIn(), exit = androidx.compose.animation.shrinkVertically() + fadeOut()) {
            ShellTopBar(data, onMenu = { onDrawer(!drawerOpen) }, callbacks = callbacks)
        }
        Box(Modifier.fillMaxWidth().weight(1f)) {
            Column(Modifier.fillMaxSize()) {
                androidx.compose.animation.AnimatedVisibility(showChrome && !paneFull, enter = androidx.compose.animation.expandVertically() + fadeIn(), exit = androidx.compose.animation.shrinkVertically() + fadeOut()) {
                    AreaTabs(current, data, callbacks, addMenuOpen)
                }
                Box(Modifier.fillMaxWidth().weight(1f)) { content() }
            }
            // The drawer and its dim overlay start under the top bar, as on the web.
            androidx.compose.animation.AnimatedVisibility(drawerOpen, enter = fadeIn(), exit = fadeOut()) {
                Box(Modifier.fillMaxSize().background(Color(0x66000000)).clickable(remember { MutableInteractionSource() }, null) { onDrawer(false) })
            }
            androidx.compose.animation.AnimatedVisibility(drawerOpen, enter = slideInHorizontally { -it }, exit = slideOutHorizontally { -it }) {
                ProjectDrawer(
                    data,
                    onProject = { onDrawer(false); callbacks.onProject(it) },
                    onAddProject = { onDrawer(false); callbacks.onAddProject() },
                    onAppSettings = { onDrawer(false); callbacks.onAppSettings() },
                    onPairAnother = { onDrawer(false); callbacks.onPairAnother() },
                )
            }
            overlay()
        }
    }
}

// ─── Top bar ───

/**
 * The mobile web's top bar: menu, the animated head, "/", the project's icon
 * and name, then the usage badge ("2m ago", opens the history chart), Panels
 * with its count, the approvals shield with its yellow count, and the
 * connection as a coloured dot. Usage is drawn behind it in two rows.
 */
@Composable
fun ShellTopBar(data: ShellData, onMenu: () -> Unit, callbacks: ShellCallbacks, modifier: Modifier = Modifier) {
    // Mini's life up here (ui.mini Top.kt): the limits, what is waiting, an update.
    mv.obitech.crundi.ui.mini.MiniTop(data.usage?.fiveHourUsed, data.usage?.weekUsed, data.approvals, data.tabBadges[Area.Settings.key] == -1)
    Column(modifier.fillMaxWidth().background(CrundiColors.BgSecondary)) {
        Box(Modifier.fillMaxWidth().height(Dimens.TopBarHeight)) {
            data.usage?.let { UsageBackdrop(it, Modifier.clickable(onClick = callbacks.onUsage)) }
            TopBarRow(
                Modifier.fillMaxSize().padding(start = 2.dp, end = 10.dp),
                left = {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        CrundiIconButton("menu", onMenu, contentDescription = "Projects", tint = CrundiColors.TextPrimary, size = 22.dp)
                        // Mini (ui.mini) comes out of this head and goes back into it; while it is out the spot is empty.
                        RobotHead(Modifier.miniLogo(), size = 24.dp, motion = when {
                            data.connection != Connection.Connected -> MarkMotion.Load
                            data.working -> MarkMotion.Working
                            else -> MarkMotion.Intro
                        })
                        Text("/", style = CrundiType.Body.copy(color = CrundiColors.TextMuted))
                        // The project's icon, or its initials on its own colour (the web's mark).
                        data.project?.let { mv.obitech.crundi.ui.chats.MarkTile(mv.obitech.crundi.ui.chats.projectMark(it.name), it.icon, 18.dp) }
                    }
                },
                name = {
                    Text(
                        data.project?.name ?: "No project",
                        style = CrundiType.Body.copy(color = CrundiColors.TextSecondary),
                        maxLines = 1, overflow = TextOverflow.Ellipsis, softWrap = false,
                        modifier = Modifier.clickable(onClick = onMenu),
                    )
                },
                right = { compact -> TopBarRight(data, callbacks, compact) },
            )
        }
        CrundiDivider(strong = true)
    }
}

/** The right of the top bar: the usage badge ("2m ago", or its icon alone when [compact]), Panels, approvals, the connection dot. */
@Composable
private fun TopBarRight(data: ShellData, callbacks: ShellCallbacks, compact: Boolean) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        data.usage?.takeIf { it.fetchedAt > 0 }?.let { u ->
            Row(
                Modifier.clip(RoundedCornerShape(12.dp)).background(Color(0x26949FB8)).clickable(onClickLabel = "Usage, updated " + fmtAgo(u.fetchedAt), onClick = callbacks.onUsage)
                    .padding(horizontal = if (compact) 6.dp else 9.dp, vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp),
            ) {
                CrundiIcon("bar-chart", size = 13.dp, tint = CrundiColors.TextSecondary)
                if (!compact) Text(fmtAgo(u.fetchedAt), style = CrundiType.Badge.copy(fontSize = 11.sp), maxLines = 1)
            }
        }
        if (data.panels > 0) TopBarAct("panel", "Panels", data.panels.toString(), false, callbacks.onPanels)
        if (data.approvals > 0) TopBarAct("shield-alert", "Things waiting on you", data.approvals.toString(), true, callbacks.onApprovals, Modifier.miniTarget("top", "bell"))
        ConnectionDot(data.connection)
    }
}

/**
 * The top bar's row, laid out as the web's mobile bar is: the left group, the
 * project name, the right group at the end. The name keeps 56dp (the web's
 * min-width), or all of itself when that is shorter, and ellipsises only past
 * that; when the right group would squeeze it below that, the right group's
 * compact form (the usage badge as its icon alone) is used instead.
 */
@Composable
private fun TopBarRow(modifier: Modifier, left: @Composable () -> Unit, name: @Composable () -> Unit, right: @Composable (Boolean) -> Unit) {
    Layout(
        contents = listOf(left, name, { right(false) }, { right(true) }),
        modifier = modifier,
    ) { (l, n, rFull, rCompact), c ->
        val gap = 8.dp.roundToPx()
        val loose = Constraints(maxHeight = c.maxHeight)
        val lp = l.first().measure(loose)
        val full = rFull.first().measure(loose)
        val compact = rCompact.first().measure(loose)
        val nameWant = n.first().maxIntrinsicWidth(c.maxHeight)
        val need = minOf(nameWant, 56.dp.roundToPx())
        val roomFor = { r: androidx.compose.ui.layout.Placeable -> c.maxWidth - lp.width - gap - gap - r.width }
        val rp = if (roomFor(full) >= need) full else compact
        val nameW = minOf(nameWant, roomFor(rp)).coerceAtLeast(minOf(need, maxOf(0, c.maxWidth - lp.width - gap)))
        val np = n.first().measure(Constraints(minWidth = 0, maxWidth = nameW.coerceAtLeast(0), maxHeight = c.maxHeight))
        layout(c.maxWidth, c.maxHeight) {
            val cy = { h: Int -> (c.maxHeight - h) / 2 }
            lp.place(0, cy(lp.height))
            np.place(lp.width + gap, cy(np.height))
            rp.place(c.maxWidth - rp.width, cy(rp.height))
        }
    }
}

/** A project's own icon (from the server), small and rounded. */
@Composable
fun ProjectIcon(path: String, size: androidx.compose.ui.unit.Dp) {
    val img = mv.obitech.crundi.api.rememberServerImage(path, 96)
    if (img != null) Image(img, contentDescription = null, modifier = Modifier.size(size).clip(RoundedCornerShape(5.dp)))
}

/** .topbar-act: a small bordered button with an icon and a count (approvals: yellow). */
@Composable
private fun TopBarAct(icon: String, description: String, count: String, warn: Boolean, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val c = if (warn) CrundiColors.Yellow else CrundiColors.TextSecondary
    Row(
        modifier.clip(Dimens.ShapeSm).border(Dimens.Hairline, if (warn) CrundiColors.Yellow else CrundiColors.Border, Dimens.ShapeSm)
            .clickable(onClickLabel = description, onClick = onClick).padding(horizontal = 6.dp, vertical = 5.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        CrundiIcon(icon, size = 15.dp, tint = c, contentDescription = description)
        Text(
            count, style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontSize = 10.sp, fontWeight = FontWeight.Bold, color = if (warn) Color(0xFF1A1A1A) else CrundiColors.TextPrimary),
            modifier = Modifier.clip(RoundedCornerShape(99.dp)).background(if (warn) CrundiColors.Yellow else CrundiColors.BgTertiary).padding(horizontal = 5.dp),
        )
    }
}

/** The connection badge collapsed to its dot, as on a phone. */
@Composable
fun ConnectionDot(c: Connection) {
    val (fg, bg) = when (c) {
        Connection.Connected -> CrundiColors.Green to CrundiColors.GreenDim
        Connection.Reconnecting -> CrundiColors.Yellow to CrundiColors.YellowDim
        else -> CrundiColors.Red to CrundiColors.RedDim
    }
    Box(Modifier.size(22.dp).clip(CircleShape).background(bg), contentAlignment = Alignment.Center) {
        Box(Modifier.size(10.dp).clip(CircleShape).background(fg))
    }
}

/**
 * The usage rows behind the top bar, as the desktop draws them: weekly on top,
 * five-hour below; in each, time gone by (cool) under usage (accent, yellow
 * when ahead of the clock, red near the limit), and their labels at the bars'
 * edges ("wk 61%", the clock and "2h 1m").
 */
@Composable
private fun UsageBackdrop(u: UsageGlance, modifier: Modifier = Modifier) {
    Box(modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize()) {
            UsageBar(u.weekUsed, u.weekElapsed, Modifier.weight(1f).miniTarget("top", "week"))
            Box(Modifier.fillMaxWidth().height(1.dp).background(Color(0x0DFFFFFF)))
            UsageBar(u.fiveHourUsed, u.fiveHourElapsed, Modifier.weight(1f).miniTarget("top", "five"))
        }
        // The labels sit in the 10dp bands along the bar's top and bottom
        // edges. Every chip, the name and the buttons are at most 26dp tall
        // and centred, so nothing in the bar can reach those bands: the
        // labels never cover a chip or the project name.
        UsageLabels("wk", u.weekUsed, u.weekElapsed, u.weekResetsAt, Modifier.align(Alignment.TopStart).padding(top = 0.5.dp))
        UsageLabels("5h", u.fiveHourUsed, u.fiveHourElapsed, u.fiveHourResetsAt, Modifier.align(Alignment.BottomStart).padding(bottom = 0.5.dp))
    }
}

/** One usage row's bars: time gone by (cool) under usage (accent, yellow when ahead of the clock, red near the limit). */
@Composable
private fun UsageBar(used: Float, elapsed: Float, modifier: Modifier) {
    val usedC = used.coerceIn(0f, 1f); val timeC = elapsed.coerceIn(0f, 1f)
    Box(modifier.fillMaxWidth().drawBehind {
        drawRect(Color(0x297DAAFF), size = Size(size.width * timeC, size.height))
        val fill = when {
            usedC >= 0.9f -> CrundiColors.Red.copy(alpha = 0.30f)
            usedC > timeC + 0.1f -> CrundiColors.Yellow.copy(alpha = 0.26f)
            else -> CrundiColors.Accent.copy(alpha = 0.22f)
        }
        drawRect(fill, size = Size(size.width * usedC, size.height))
    })
}

/**
 * A row's two labels in its band: "wk 61%" at the usage bar's edge and the
 * clock with the time to reset at the time bar's edge (placeUsageLabel: past
 * the middle just inside the end, otherwise just after it). When the two
 * would touch, the reset time moves to the far side of the usage label.
 */
@Composable
private fun UsageLabels(name: String, used: Float, elapsed: Float, resetsAt: Long, modifier: Modifier) {
    val label = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontSize = 8.5.sp, fontWeight = FontWeight.Bold, lineHeight = 10.sp)
    Layout(
        content = {
            Text("$name ${(used * 100).toInt()}%", style = label.copy(color = CrundiColors.TextPrimary.copy(alpha = 0.9f)), maxLines = 1)
            if (resetsAt > 0) Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(2.dp)) {
                CrundiIcon("clock", size = 8.dp, tint = CrundiColors.TextSecondary.copy(alpha = 0.7f))
                Text(fmtRemaining(resetsAt), style = label.copy(color = CrundiColors.TextSecondary.copy(alpha = 0.7f)), maxLines = 1)
            }
        },
        modifier = modifier.fillMaxWidth(),
    ) { m, c ->
        val gap = 6.dp.roundToPx()
        val w = c.maxWidth
        fun at(pct: Float, width: Int): Int {
            val edge = (w * pct.coerceIn(0f, 1f)).toInt()
            return (if (pct > 0.5f) edge - gap - width else edge + gap).coerceIn(0, (w - width).coerceAtLeast(0))
        }
        val up = m[0].measure(Constraints())
        val tp = m.getOrNull(1)?.measure(Constraints())
        val ux = at(used, up.width)
        var tx = tp?.let { at(elapsed, it.width) } ?: 0
        if (tp != null && tx < ux + up.width + gap && tx + tp.width + gap > ux) {
            // Overlap: put the reset time after the usage label, or before it if there is no room after.
            tx = if (ux + up.width + gap + tp.width <= w) ux + up.width + gap else (ux - gap - tp.width).coerceAtLeast(0)
        }
        val h = maxOf(up.height, tp?.height ?: 0)
        layout(w, h) {
            up.place(ux, (h - up.height) / 2)
            tp?.place(tx, (h - tp.height) / 2)
        }
    }
}

fun fmtAgo(ms: Long, now: Long = System.currentTimeMillis()): String {
    val s = ((now - ms) / 1000).coerceAtLeast(0)
    if (s < 60) return "just now"
    val m = s / 60; if (m < 60) return "${m}m ago"
    val h = m / 60; if (h < 24) return "${h}h ago"
    return "${h / 24}d ago"
}

fun fmtRemaining(resetsAt: Long, now: Long = System.currentTimeMillis()): String {
    if (resetsAt <= 0) return "—"
    val ms = (resetsAt - now).coerceAtLeast(0)
    val d = ms / 86_400_000; val h = ms % 86_400_000 / 3_600_000; val m = ms % 3_600_000 / 60_000
    return when { d > 0 -> "${d}d ${h}h"; h > 0 -> "${h}h ${m}m"; else -> "${m}m" }
}

// ─── Tab strip ───

/**
 * The tab strip: icons only, scrolling sideways, the current one in accent
 * with the gradient line under it and brought into view; "+" beside the
 * Workbench opens the add menu; red counts on Factories and Secrets, a green
 * dot on Settings for an update. With no project, only the global tabs.
 */
@Composable
fun AreaTabs(current: Area?, data: ShellData, callbacks: ShellCallbacks, addMenuOpen: Boolean = false, modifier: Modifier = Modifier) {
    val scroll = rememberScrollState()
    val spans = remember { mutableStateMapOf<Area, IntArray>() }
    var viewport by remember { mutableIntStateOf(0) }
    var placed by remember { mutableStateOf(false) }
    val span = current?.let { spans[it] }
    LaunchedEffect(current, span?.get(0), span?.get(1), viewport) {
        if (span == null || viewport == 0) return@LaunchedEffect
        val target = (span[0] + span[1] / 2 - viewport / 2).coerceIn(0, scroll.maxValue)
        if (placed) scroll.animateScrollTo(target) else { scroll.scrollTo(target); placed = true }
    }
    var menu by remember(addMenuOpen) { mutableStateOf(addMenuOpen) }
    val hasProject = data.currentProject != null
    // Mini may haul the tabs along to show Settings (ui.mini Top.kt, the update outing).
    mv.obitech.crundi.ui.mini.MiniTabScroll(scroll)
    Column(modifier.fillMaxWidth().background(CrundiColors.BgSecondary)) {
        Row(
            Modifier.fillMaxWidth().height(Dimens.TabBarHeight).onSizeChanged { viewport = it.width }.miniTarget("top", "tabs").horizontalScroll(scroll).padding(horizontal = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            for (a in Area.tabs(hasProject)) {
                val on = a == current
                val fg = if (on) CrundiColors.AccentHover else CrundiColors.TextMuted
                Box(
                    Modifier.fillMaxHeight()
                        .onGloballyPositioned { c -> spans[a] = intArrayOf(c.positionInParent().x.toInt(), c.size.width) }
                        .miniTarget("top", "tab-" + a.key)
                        .clickable(onClickLabel = a.label) { callbacks.onArea(a) }
                        .drawBehind {
                            if (on) {
                                val inset = 10.dp.toPx()
                                drawRoundRect(CrundiColors.AccentGrad, Offset(inset, size.height - 2.dp.toPx()), Size(size.width - 2 * inset, 2.dp.toPx()), CornerRadius(2.dp.toPx()))
                            }
                        }
                        .padding(horizontal = 13.dp),
                    contentAlignment = Alignment.Center,
                ) {
                    CrundiIcon(a.icon, size = 18.dp, tint = fg, contentDescription = a.label)
                    val badge = data.tabBadges[a.key]
                    if (badge != null && badge != 0) {
                        if (badge < 0) Box(Modifier.align(Alignment.Center).offset(x = 9.dp, y = (-9).dp).size(8.dp).clip(CircleShape).background(CrundiColors.Green))
                        else Text(
                            if (badge > 99) "99+" else badge.toString(),
                            style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontSize = 9.sp, fontWeight = FontWeight.Bold, color = Color.White, lineHeight = 12.sp),
                            modifier = Modifier.align(Alignment.Center).offset(x = 11.dp, y = (-9).dp).clip(RoundedCornerShape(8.dp)).background(CrundiColors.Red).padding(horizontal = 4.dp),
                        )
                    }
                }
                if (a == Area.Workbench) Box {
                    Box(
                        Modifier.padding(horizontal = 4.dp).size(34.dp).clip(Dimens.ShapeSm).background(CrundiColors.BgTertiary)
                            .border(Dimens.Hairline, if (menu) CrundiColors.Accent else CrundiColors.Border, Dimens.ShapeSm)
                            .clickable(onClickLabel = "Add to the workbench") { menu = true },
                        contentAlignment = Alignment.Center,
                    ) { CrundiIcon("plus", size = 18.dp, tint = CrundiColors.TextSecondary, contentDescription = "Add to the workbench") }
                    DropdownMenu(
                        menu, { menu = false },
                        containerColor = CrundiColors.BgSecondary, shape = Dimens.Shape,
                        border = androidx.compose.foundation.BorderStroke(Dimens.Hairline, CrundiColors.Border),
                    ) { AddMenuBody(callbacks) { menu = false } }
                }
            }
        }
        CrundiDivider(strong = true)
    }
}

/** The "+" menu by the Workbench tab (#wb-add-menu): every kind of pane, Fit to screen, and the layout (columns or stack). */
@Composable
fun AddMenuBody(callbacks: ShellCallbacks, onDone: () -> Unit = {}) {
    Column(Modifier.widthIn(min = 200.dp).padding(4.dp)) {
        for (k in PaneKind.addable) MenuItem(k.icon, k.label) { onDone(); callbacks.onAdd(k) }
        CrundiDivider(Modifier.padding(vertical = 4.dp))
        MenuItem("maximize", "Fit to screen") { onDone(); callbacks.onFit() }
        MenuItem(
            if (callbacks.layout == WbLayout.Columns) "rows" else "columns",
            if (callbacks.layout == WbLayout.Columns) "Layout: stack the panes" else "Layout: swipe between columns",
        ) { onDone(); callbacks.onToggleLayout() }
    }
}

// ─── Projects drawer ───

/**
 * The sidebar as a drawer (280dp, from the left, under the top bar): each
 * project with its icon or initials, state dot, name, the services
 * heartbeat, the schedule clock and the factory mark; then "Add project",
 * "App settings" and "Connect to another server".
 */
@Composable
fun ProjectDrawer(
    data: ShellData,
    onProject: (String) -> Unit,
    onAddProject: () -> Unit,
    onAppSettings: () -> Unit,
    onPairAnother: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier.width(280.dp).fillMaxHeight().background(CrundiColors.BgSecondary)
            .drawBehind { drawLine(CrundiColors.Border, Offset(size.width, 0f), Offset(size.width, size.height), 1.dp.toPx()) },
    ) {
        SectionHeader("Projects", trailing = { Text(data.serverHost, style = CrundiType.Meta, maxLines = 1, overflow = TextOverflow.Ellipsis) })
        CrundiDivider()
        LazyColumn(Modifier.weight(1f).padding(horizontal = 8.dp, vertical = 4.dp)) {
            items(data.projects, key = { it.alias }) { p -> ProjectRow(p, p.alias == data.currentProject) { onProject(p.alias) } }
            if (data.projects.isEmpty()) item {
                Text("No projects yet. Add one below, or ask Claude in a chat.", style = CrundiType.Small, modifier = Modifier.padding(12.dp))
            }
        }
        Column(Modifier.drawBehind { drawLine(CrundiColors.BorderSubtle, Offset.Zero, Offset(size.width, 0f), 1.dp.toPx()) }.padding(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            DashedButton("smartphone", "App settings", onAppSettings)
            DashedButton("plus", "Add project", onAddProject)
            DashedButton("link", "Connect to another server", onPairAnother)
        }
    }
}

@Composable
private fun ProjectRow(p: ProjectSummary, active: Boolean, onClick: () -> Unit) {
    ListRow(
        title = p.name,
        active = active,
        leading = {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                mv.obitech.crundi.ui.chats.MarkTile(mv.obitech.crundi.ui.chats.projectMark(p.name), p.icon, 22.dp)
                StateDot(p.state)
            }
        },
        trailing = {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                if (p.hasService) CrundiIcon("heartbeat", size = 16.dp, tint = if (p.liveService) CrundiColors.Green else CrundiColors.TextMuted, contentDescription = if (p.liveService) "A service is running" else "Services stopped")
                if (p.scheduled) CrundiIcon("clock", size = 14.dp, tint = if (active) CrundiColors.AccentHover else CrundiColors.TextMuted, contentDescription = "Scheduled")
                p.factory?.let { CrundiIcon("factory", size = 14.dp, tint = if (it == FactoryMark.Needs) CrundiColors.Yellow else CrundiColors.Green, contentDescription = "Factory run") }
            }
        },
        onClick = onClick,
    )
}

/** .sidebar-footer button: dashed, muted, full width. */
@Composable
private fun DashedButton(icon: String, label: String, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clip(Dimens.ShapeSm).drawBehind {
            drawRoundRect(CrundiColors.Border, style = Stroke(1.dp.toPx(), pathEffect = PathEffect.dashPathEffect(floatArrayOf(5.dp.toPx(), 4.dp.toPx()))), cornerRadius = CornerRadius(Dimens.RadiusSm.toPx()))
        }.clickable(onClick = onClick).padding(10.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
    ) {
        CrundiIcon(icon, size = 15.dp, tint = CrundiColors.TextMuted)
        Text(label, style = CrundiType.Small.copy(color = CrundiColors.TextMuted))
    }
}

/** Which workbench pane is full screen, if any: the shell hides its chrome for it. */
object FullScreen {
    var pane by androidx.compose.runtime.mutableStateOf<String?>(null)
}
