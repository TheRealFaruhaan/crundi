package mv.obitech.crundi.ui.shell

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
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
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.DrawerState
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.ModalDrawerSheet
import androidx.compose.material3.ModalNavigationDrawer
import androidx.compose.material3.Text
import androidx.compose.material3.rememberDrawerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import mv.obitech.crundi.R
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.CrundiDivider
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.ListRow
import mv.obitech.crundi.ui.components.SectionHeader
import mv.obitech.crundi.ui.components.StateDot
import mv.obitech.crundi.ui.components.StatusBadge
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/** The Crundi head, as in the desktop's top bar. */
@Composable
fun CrundiMark(modifier: Modifier = Modifier, size: androidx.compose.ui.unit.Dp = 24.dp) {
    Image(painterResource(R.drawable.ic_logo), contentDescription = "Crundi", modifier = modifier.size(size))
}

/**
 * The whole frame round the app, stateless so a screenshot test can draw it:
 * the project drawer (the desktop's sidebar), and, on an area's front page,
 * the top bar and the tab row. `content` is the current screen (the NavHost).
 */
@Composable
fun ShellScaffold(
    data: ShellData,
    current: Area?,
    showChrome: Boolean,
    onArea: (Area) -> Unit,
    onProject: (String) -> Unit,
    onApprovals: () -> Unit,
    modifier: Modifier = Modifier,
    drawerState: DrawerState = rememberDrawerState(DrawerValue.Closed),
    onAppSettings: () -> Unit = {},
    onPairAnother: () -> Unit = {},
    content: @Composable () -> Unit,
) {
    val scope = rememberCoroutineScope()
    ModalNavigationDrawer(
        drawerState = drawerState,
        gesturesEnabled = showChrome || drawerState.isOpen,
        scrimColor = CrundiColors.Scrim,
        drawerContent = {
            ProjectDrawer(
                data = data,
                onProject = { scope.launch { drawerState.close() }; onProject(it) },
                onAppSettings = { scope.launch { drawerState.close() }; onAppSettings() },
                onPairAnother = { scope.launch { drawerState.close() }; onPairAnother() },
            )
        },
        modifier = modifier,
    ) {
        Column(Modifier.fillMaxSize().background(CrundiColors.BgPrimary)) {
            if (showChrome) {
                ShellTopBar(data, onMenu = { scope.launch { drawerState.open() } }, onApprovals = onApprovals)
                AreaTabs(current, onArea)
            }
            Box(Modifier.fillMaxWidth().weight(1f)) { content() }
        }
    }
}

/**
 * The desktop's top bar on a phone: menu (opens the projects), the mark, "/",
 * the current project, then the waiting counter and the connection badge.
 * Usage is drawn behind it in two thin rows, as on the desktop.
 */
@Composable
fun ShellTopBar(data: ShellData, onMenu: () -> Unit, onApprovals: () -> Unit, modifier: Modifier = Modifier) {
    Column(modifier.fillMaxWidth().background(CrundiColors.BgSecondary)) {
        Box(Modifier.fillMaxWidth().height(Dimens.TopBarHeight)) {
            data.usage?.let { UsageBackdrop(it) }
            Row(Modifier.fillMaxSize().padding(start = 2.dp, end = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                CrundiIconButton("menu", onMenu, contentDescription = "Projects", tint = CrundiColors.TextPrimary, size = 20.dp)
                CrundiMark(size = 24.dp)
                Text("/", style = CrundiType.Body.copy(color = CrundiColors.TextMuted), modifier = Modifier.padding(horizontal = 9.dp))
                Text(
                    data.project?.name ?: "No project",
                    style = CrundiType.Body.copy(color = CrundiColors.TextSecondary),
                    maxLines = 1, overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f).clickable(onClick = onMenu),
                )
                if (data.approvals > 0) {
                    Row(
                        Modifier.clip(Dimens.ShapeSm).background(CrundiColors.AccentDim).clickable(onClick = onApprovals).padding(horizontal = 8.dp, vertical = 5.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(5.dp),
                    ) {
                        CrundiIcon("bell", size = 15.dp, tint = CrundiColors.AccentHover)
                        Text(data.approvals.toString(), style = CrundiType.Badge.copy(color = CrundiColors.AccentHover))
                    }
                    Spacer(Modifier.width(8.dp))
                }
                StatusBadge(
                    data.connection.label,
                    when (data.connection) {
                        Connection.Connected -> BadgeKind.Good
                        Connection.Reconnecting -> BadgeKind.Warn
                        else -> BadgeKind.Bad
                    },
                )
            }
        }
        CrundiDivider(strong = true)
    }
}

/** The two usage rows behind the top bar (weekly on top, 5-hour below). */
@Composable
private fun UsageBackdrop(u: UsageGlance) {
    Column(Modifier.fillMaxSize()) {
        for ((used, elapsed) in listOf(u.weekUsed to u.weekElapsed, u.fiveHourUsed to u.fiveHourElapsed)) {
            Box(
                Modifier.fillMaxWidth().weight(1f).drawBehind {
                    drawRect(Color(0x297DAAFF), size = Size(size.width * elapsed.coerceIn(0f, 1f), size.height))
                    val c = when {
                        used >= 0.9f -> CrundiColors.Red.copy(alpha = 0.28f)
                        used > elapsed + 0.1f -> CrundiColors.Yellow.copy(alpha = 0.26f)
                        else -> CrundiColors.Accent.copy(alpha = 0.22f)
                    }
                    drawRect(c, size = Size(size.width * used.coerceIn(0f, 1f), size.height))
                },
            )
        }
    }
}

/**
 * The desktop's tab bar: scrolls sideways, icons only on a phone as on the
 * desktop's narrow layout, with the current tab's label shown beside its icon
 * and the accent gradient line under it.
 */
@Composable
fun AreaTabs(current: Area?, onArea: (Area) -> Unit, modifier: Modifier = Modifier) {
    Column(modifier.fillMaxWidth().background(CrundiColors.BgSecondary)) {
        Row(
            Modifier.fillMaxWidth().height(Dimens.TabBarHeight).horizontalScroll(rememberScrollState()).padding(horizontal = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            for (a in Area.entries) {
                val on = a == current
                val fg = if (on) CrundiColors.AccentHover else CrundiColors.TextMuted
                Row(
                    Modifier
                        .fillMaxHeight()
                        .clickable { onArea(a) }
                        .drawBehind {
                            if (on) {
                                val inset = 10.dp.toPx()
                                drawRoundRect(
                                    brush = CrundiColors.AccentGrad,
                                    topLeft = Offset(inset, size.height - 2.dp.toPx()),
                                    size = Size(size.width - 2 * inset, 2.dp.toPx()),
                                    cornerRadius = CornerRadius(2.dp.toPx()),
                                )
                            }
                        }
                        .padding(horizontal = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    CrundiIcon(a.icon, size = 17.dp, tint = fg, contentDescription = a.label)
                    if (on) Text(a.label, style = CrundiType.Tab.copy(color = fg), maxLines = 1)
                }
            }
        }
        CrundiDivider(strong = true)
    }
}

/** The desktop's sidebar as a drawer: projects to switch between, and the phone's own things. */
@Composable
fun ProjectDrawer(
    data: ShellData,
    onProject: (String) -> Unit,
    onAppSettings: () -> Unit,
    onPairAnother: () -> Unit,
    modifier: Modifier = Modifier,
) {
    ModalDrawerSheet(
        modifier = modifier.width(288.dp),
        drawerContainerColor = CrundiColors.BgSecondary,
        drawerContentColor = CrundiColors.TextPrimary,
        drawerShape = RoundedCornerShape(topEnd = Dimens.Radius, bottomEnd = Dimens.Radius),
    ) {
        Row(Modifier.fillMaxWidth().height(Dimens.TopBarHeight).padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
            CrundiMark(size = 24.dp)
            Text("crundi", style = CrundiType.Wordmark, modifier = Modifier.padding(start = 9.dp))
            Spacer(Modifier.weight(1f))
            Text(data.serverHost, style = CrundiType.Meta, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(start = 12.dp))
        }
        CrundiDivider(strong = true)
        SectionHeader("Projects")
        LazyColumn(Modifier.weight(1f).padding(horizontal = 8.dp)) {
            items(data.projects, key = { it.alias }) { p ->
                ListRow(
                    title = p.name,
                    active = p.alias == data.currentProject,
                    leading = { StateDot(p.state) },
                    onClick = { onProject(p.alias) },
                )
            }
            if (data.projects.isEmpty()) item {
                Text("No projects yet. Add one on the desktop, or ask Claude in a chat.", style = CrundiType.Small, modifier = Modifier.padding(12.dp))
            }
        }
        CrundiDivider()
        Column(Modifier.padding(8.dp)) {
            ListRow("App settings", icon = "smartphone", onClick = onAppSettings)
            ListRow("Connect to another server", icon = "link", onClick = onPairAnother)
        }
    }
}
