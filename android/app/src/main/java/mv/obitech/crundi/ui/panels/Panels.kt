package mv.obitech.crundi.ui.panels

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import kotlinx.coroutines.launch
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.model.Widget
import mv.obitech.crundi.api.model.WidgetVersion
import mv.obitech.crundi.api.rememberServerImage
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiDivider
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.PanelHeader
import mv.obitech.crundi.ui.components.SectionHeader
import mv.obitech.crundi.ui.components.StatusBadge
import mv.obitech.crundi.ui.components.errorText
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.operations.Actions
import mv.obitech.crundi.ui.operations.Fmt
import mv.obitech.crundi.ui.operations.Load
import mv.obitech.crundi.ui.operations.NoteBanner
import mv.obitech.crundi.ui.operations.rememberActions
import mv.obitech.crundi.ui.operations.rememberLoad
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens
import mv.obitech.crundi.ui.workbench.WbCell

/*
 * Owned by ui.operations. Panels (the web's widgets): the Panels tab with its
 * sideways list, a Panel pane in the workbench, a chat's panel dock, and the
 * top bar's chip popover. The frames are PanelFrame (the WebView exception,
 * see PanelFrame.kt); everything around them is native.
 */

// ─────────────────────────── Panels tab ───────────────────────────

/** What the selected panel's stage shows: the panel, Claude's check renders, or its versions. */
enum class PanelPane { Panel, Checks, History }

data class PanelsState(
    val project: String,
    val list: Load<List<Widget>>,
    val selected: String? = null,
    val pane: PanelPane = PanelPane.Panel,
    val chipValues: Map<String, String> = emptyMap(),
    val versions: Load<List<WidgetVersion>> = Load.Loading,
    val shots: Load<List<PanelShot>> = Load.Loading,
    val shotsAt: Long = 0,
    val reloadKey: Int = 0,
    val busy: String? = null,
    val note: Actions.Note? = null,
    val now: Long = System.currentTimeMillis(),
) {
    val sorted get() = sortForTab(list.valueOrNull.orEmpty())
    val current get() = sorted.firstOrNull { it.id == selected } ?: sorted.firstOrNull()
}

class PanelsEvents(
    val onSelect: (String) -> Unit = {},
    val onPane: (PanelPane) -> Unit = {},
    val onSlot: (Widget, String) -> Unit = { _, _ -> },
    val onPin: (Widget) -> Unit = {},
    val onGlobal: (Widget, Boolean) -> Unit = { _, _ -> },
    val onReload: () -> Unit = {},
    val onClose: (Widget) -> Unit = {},
    val onReopen: (Widget) -> Unit = {},
    val onDelete: (Widget) -> Unit = {},
    val onRestore: (Widget, Int) -> Unit = { _, _ -> },
    val onRetry: () -> Unit = {},
    val onCloseNote: () -> Unit = {},
    val shotPath: (Widget, PanelShot) -> String? = { _, _ -> null },
)

@Composable
fun PanelsRoute() {
    val api = LocalAppServices.current.api
    val project = currentPanelProject()
    val list = rememberPanelList(project)
    val actions = rememberActions()
    var selected by remember(project) { mutableStateOf<String?>(null) }
    var pane by remember(project) { mutableStateOf(PanelPane.Panel) }
    var reloadKey by remember { mutableIntStateOf(0) }
    val widgets = list.value.valueOrNull.orEmpty()
    val chipValues = rememberChipValues(project, widgets)
    val current = sortForTab(widgets).let { s -> s.firstOrNull { it.id == selected } ?: s.firstOrNull() }
    val versions = rememberLoad(api, project, current?.id, pane == PanelPane.History) {
        if (current != null && pane == PanelPane.History) api?.widgets?.versions(project, current.id).orEmpty() else emptyList()
    }
    val renders = rememberLoad(api, project, current?.id, pane == PanelPane.Checks) {
        if (current != null && pane == PanelPane.Checks) parseShots(api?.widgets?.renders(project, current.id)) else emptyList<PanelShot>() to 0L
    }
    PanelsScreen(
        PanelsState(
            project, list.value, current?.id, pane, chipValues,
            versions.value,
            when (val r = renders.value) { is Load.Ready -> Load.Ready(r.value.first); is Load.Failed -> r; Load.Loading -> Load.Loading },
            renders.value.valueOrNull?.second ?: 0L,
            reloadKey, actions.busy, actions.note,
        ),
        PanelsEvents(
            onSelect = { selected = it; pane = PanelPane.Panel },
            onPane = { pane = if (pane == it) PanelPane.Panel else it },
            onSlot = { w, s -> actions.run(w.id, "Moved to: " + (Widget.SLOT_LABELS[s] ?: s), list::reload) { api!!.widgets.update(project, w.id, slot = s) } },
            onPin = { w -> actions.run(w.id, after = list::reload) { api!!.widgets.update(project, w.id, pinned = w.lifecycle != "pinned") } },
            onGlobal = { w, on ->
                actions.run(w.id, if (on) "Now global: shown in every project" else "Back in this project only", list::reload) { api!!.widgets.setGlobal(project, w.id, on) }
            },
            onReload = { pane = PanelPane.Panel; reloadKey++ },
            onClose = { w -> actions.run(w.id, "Closed ${w.title}", list::reload) { api!!.widgets.update(project, w.id, closed = true) } },
            onReopen = { w -> actions.run(w.id, "Reopened ${w.title}", list::reload) { api!!.widgets.update(project, w.id, closed = false) } },
            onDelete = { w -> actions.run(w.id, "Deleted ${w.title}", { selected = null; list.reload() }) { api!!.widgets.delete(project, w.id) } },
            onRestore = { w, v -> actions.run(w.id, "Restored v$v", { pane = PanelPane.Panel; reloadKey++ }) { api!!.widgets.rollback(project, w.id, v) } },
            onRetry = list::reload,
            onCloseNote = actions::clear,
            shotPath = { w, s -> api?.widgets?.renderImagePath(project, w.id, s.name) },
        ),
    )
}

@Composable
fun PanelsScreen(state: PanelsState, events: PanelsEvents = PanelsEvents()) {
    val all = state.sorted
    Column(Modifier.fillMaxSize()) {
        PanelHeader(
            "Panels",
            meta = when {
                state.list !is Load.Ready -> null
                all.isEmpty() -> if (state.project.isEmpty()) "global panels" else null
                else -> "${all.size} panel${if (all.size == 1) "" else "s"} · ${all.count { it.open }} open" + if (state.project.isEmpty()) " · global" else ""
            },
        )
        NoteBanner(state.note, events.onCloseNote)
        when (val l = state.list) {
            Load.Loading -> LoadingState(text = "Loading panels…")
            is Load.Failed -> ErrorState(l.message, onRetry = events.onRetry)
            is Load.Ready -> if (all.isEmpty()) {
                if (state.project.isEmpty()) EmptyState("panel", "No global panels yet", body = "Pick a project to see its panels. Any of them can be made global from here, to show in every project.")
                else EmptyState("panel", "No panels yet", body = "Claude builds these when something is better seen than read: progress on a long task, a table from a database, a build board. Ask for one, or let it decide.")
            } else {
                val chips = all.filter { it.isChip }
                if (chips.isNotEmpty()) ChipStrip(chips, state.chipValues, onOpen = { events.onSelect(it.id) })
                TabList(all, state.current?.id, events.onSelect)
                CrundiDivider(strong = true)
                state.current?.let { w -> SelectedPanel(w, state, events) }
            }
        }
    }
}

/** The top bar's chips, with their values, as a row (tap: show that panel). */
@Composable
private fun ChipStrip(chips: List<Widget>, values: Map<String, String>, onOpen: (Widget) -> Unit) {
    Column(Modifier.fillMaxWidth()) {
        SectionHeader("In the top bar")
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = Dimens.ScreenPad).padding(bottom = 8.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            for (w in chips) PanelChip(w, values[w.id], onClick = { onOpen(w) })
        }
    }
}

/** One chip as the web's .wg-chip: panel icon, label, value, "approve" when it waits. */
@Composable
fun PanelChip(w: Widget, value: String?, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val shape = RoundedCornerShape(99.dp)
    Row(
        modifier.widthIn(max = 240.dp).height(32.dp).clip(shape).background(CrundiColors.BgTertiary).border(Dimens.Hairline, CrundiColors.Border, shape)
            .clickable(onClick = onClick).padding(horizontal = 11.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        CrundiIcon("panel", size = 13.dp, tint = CrundiColors.AccentHover)
        Text(w.chip?.label?.ifEmpty { null } ?: w.title, style = CrundiType.Small, maxLines = 1)
        if (!value.isNullOrEmpty()) Text(value, style = CrundiType.Small.copy(color = CrundiColors.TextPrimary), maxLines = 1, overflow = TextOverflow.Ellipsis)
        if (w.needsGrant) MiniBadge("approve", BadgeKind.Warn)
    }
}

/** The web's small uppercase badge (wg-badge): global, approve, error. */
@Composable
fun MiniBadge(text: String, kind: BadgeKind = BadgeKind.Neutral) {
    val shape = RoundedCornerShape(99.dp)
    Text(
        text.uppercase(),
        style = CrundiType.Section.copy(color = kind.fg, fontSize = CrundiType.Meta.fontSize, letterSpacing = CrundiType.Meta.letterSpacing),
        maxLines = 1,
        modifier = Modifier.clip(shape).background(if (kind == BadgeKind.Neutral) CrundiColors.BgPrimary else kind.bg)
            .border(Dimens.Hairline, if (kind == BadgeKind.Neutral) CrundiColors.Border else CrundiColors.BgPrimary.copy(alpha = 0f), shape)
            .padding(horizontal = 6.dp, vertical = 1.dp),
    )
}

/** The sideways list of panels (the web's .wg-tab-list, laid out in a row on a phone). */
@Composable
private fun TabList(all: List<Widget>, selected: String?, onSelect: (String) -> Unit) {
    LazyRow(
        Modifier.fillMaxWidth().background(CrundiColors.BgSecondary),
        contentPadding = PaddingValues(horizontal = Dimens.ScreenPad, vertical = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        items(all, key = { it.id }) { w ->
            val sel = w.id == selected
            Row(
                Modifier.heightIn(min = 40.dp).widthIn(max = 220.dp).clip(Dimens.ShapeSm)
                    .background(if (sel) CrundiColors.BgTertiary else CrundiColors.BgSecondary)
                    .border(Dimens.Hairline, if (sel) CrundiColors.Border else CrundiColors.BgSecondary, Dimens.ShapeSm)
                    .clickable { onSelect(w.id) }.padding(horizontal = 10.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                CrundiIcon("panel", size = Dimens.IconSm, tint = CrundiColors.AccentHover)
                Text(w.title.ifEmpty { w.id }, style = CrundiType.Small.copy(color = if (w.open) CrundiColors.TextPrimary else CrundiColors.TextMuted), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
                if (w.global) MiniBadge("global")
                when {
                    w.needsGrant -> MiniBadge("approve", BadgeKind.Warn)
                    w.faults > 0 -> MiniBadge("error", BadgeKind.Bad)
                    else -> Text(statusWord(w), style = CrundiType.Meta.copy(color = CrundiColors.TextMuted))
                }
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun SelectedPanel(w: Widget, state: PanelsState, events: PanelsEvents) {
    var picking by remember(w.id) { mutableStateOf(false) }
    var deleting by remember(w.id) { mutableStateOf(false) }
    val busy = state.busy == w.id
    Column(Modifier.fillMaxSize()) {
        // The web's .wg-tab-bar: title, where it sits, then its tools.
        Column(Modifier.fillMaxWidth().background(CrundiColors.BgSecondary).padding(horizontal = Dimens.ScreenPad, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(w.title.ifEmpty { w.id }, style = CrundiType.Heading, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                if (w.global && w.home.isNotEmpty() && w.home != panelKey(state.project)) MiniBadge("global · from ${w.home}")
            }
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                if (w.open) Tool(Widget.SLOT_LABELS[w.slot] ?: w.slot, "chevron-down", enabled = !busy) { picking = true }
                Tool(if (w.lifecycle == "pinned") "Pinned" else "Pin", "pin", on = w.lifecycle == "pinned", enabled = !busy) { events.onPin(w) }
                when {
                    !w.global -> Tool("Make global", "globe", enabled = !busy) { events.onGlobal(w, true) }
                    w.home.isEmpty() || w.home == panelKey(state.project) -> Tool("Global", "globe", on = true, enabled = !busy) { events.onGlobal(w, false) }
                }
                Tool("Reload", "refresh") { events.onReload() }
                Tool("Checks", "eye", on = state.pane == PanelPane.Checks) { events.onPane(PanelPane.Checks) }
                Tool("History", "rotate-ccw", on = state.pane == PanelPane.History) { events.onPane(PanelPane.History) }
                if (w.open) Tool("Close", "x", enabled = !busy) { events.onClose(w) } else Tool("Reopen", "play", on = true, enabled = !busy) { events.onReopen(w) }
                Tool("Delete", "trash", danger = true, enabled = !busy) { deleting = true }
            }
        }
        CrundiDivider()
        Box(Modifier.weight(1f).fillMaxWidth()) {
            when (state.pane) {
                PanelPane.Checks -> ChecksPane(w, state, events)
                PanelPane.History -> HistoryPane(w, state, events)
                PanelPane.Panel -> if (!w.open) EmptyState("panel", "This panel is closed.", body = "Reopen it to see it here and wherever it was placed.")
                else PanelFrame(state.project, w.id, "tab", Modifier.fillMaxSize(), reloadKey = state.reloadKey)
            }
        }
    }
    if (picking) SlotPicker(w, onPick = { picking = false; if (it != w.slot) events.onSlot(w, it) }, onDismiss = { picking = false })
    if (deleting) CrundiDialog(
        "Delete panel", onDismiss = { deleting = false },
        message = "Delete “${w.title}” and its source files? This cannot be undone.",
        confirm = "Delete panel", onConfirm = { deleting = false; events.onDelete(w) }, danger = true,
    )
}

/** A small tool button in the panel's bar (the web's .wg-btn). */
@Composable
private fun Tool(text: String, icon: String, on: Boolean = false, danger: Boolean = false, enabled: Boolean = true, onClick: () -> Unit) {
    val fg = when { danger -> CrundiColors.Red; on -> CrundiColors.AccentHover; else -> CrundiColors.TextSecondary }
    val bg = when { danger -> CrundiColors.RedDim; on -> CrundiColors.AccentDim; else -> CrundiColors.BgTertiary }
    Row(
        Modifier.height(34.dp).clip(Dimens.ShapeSm).background(bg)
            .border(Dimens.Hairline, if (danger || on) bg else CrundiColors.Border, Dimens.ShapeSm)
            .clickable(enabled = enabled, onClick = onClick).padding(horizontal = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        CrundiIcon(icon, size = 14.dp, tint = fg)
        Text(text, style = CrundiType.Small.copy(color = fg), maxLines = 1)
    }
}

/** "Where this panel sits": the slots it may move to. */
@Composable
fun SlotPicker(w: Widget, onPick: (String) -> Unit, onDismiss: () -> Unit) {
    CrundiDialog("Where this panel sits", onDismiss = onDismiss) { SlotOptions(w, onPick) }
}

/** The rows of the slot picker (also drawn in place by the screenshots). */
@Composable
fun SlotOptions(w: Widget, onPick: (String) -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        for ((slot, label) in movableSlots(w)) Row(
            Modifier.fillMaxWidth().heightIn(min = Dimens.Touch).clip(Dimens.ShapeSm)
                .background(if (slot == w.slot) CrundiColors.AccentDim else CrundiColors.BgSecondary)
                .clickable { onPick(slot) }.padding(horizontal = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Text(label, style = CrundiType.Body.copy(color = if (slot == w.slot) CrundiColors.AccentHover else CrundiColors.TextPrimary), modifier = Modifier.weight(1f))
            if (slot == w.slot) CrundiIcon("check", size = Dimens.IconSm, tint = CrundiColors.AccentHover)
        }
    }
}

@Composable
private fun ChecksPane(w: Widget, state: PanelsState, events: PanelsEvents) {
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(Dimens.ScreenPad), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text("What Claude saw when it last checked this panel.", style = CrundiType.Body.copy(color = CrundiColors.TextPrimary))
        when (val s = state.shots) {
            Load.Loading -> LoadingState(Modifier.height(120.dp))
            is Load.Failed -> ErrorState(s.message)
            is Load.Ready -> if (s.value.isEmpty()) Text("No check has been run yet. Claude renders a panel with widget_render before calling it done.", style = CrundiType.Small.copy(color = CrundiColors.TextMuted))
            else for (shot in s.value) Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                val img = rememberServerImage(events.shotPath(w, shot)?.let { it + "&t=" + state.shotsAt }, 1200)
                Box(Modifier.fillMaxWidth().heightIn(min = 120.dp).clip(Dimens.ShapeSm).background(CrundiColors.BgSecondary).border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm), contentAlignment = Alignment.Center) {
                    if (img != null) Image(img, contentDescription = "Render: ${shot.frame}", contentScale = ContentScale.FillWidth, modifier = Modifier.fillMaxWidth())
                    else CrundiIcon("image", size = 22.dp, tint = CrundiColors.TextMuted)
                }
                Text(shot.label, style = CrundiType.Meta)
            }
        }
    }
}

@Composable
private fun HistoryPane(w: Widget, state: PanelsState, events: PanelsEvents) {
    when (val v = state.versions) {
        Load.Loading -> LoadingState()
        is Load.Failed -> ErrorState(v.message)
        is Load.Ready -> if (v.value.isEmpty()) EmptyState("rotate-ccw", "No versions yet", body = "Each time Claude saves this panel, the earlier version is kept here.")
        else LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(vertical = 8.dp)) {
            items(v.value.size) { i ->
                val ver = v.value[i]
                Row(Modifier.fillMaxWidth().heightIn(min = Dimens.Touch).padding(horizontal = Dimens.ScreenPad), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text("v${ver.version}", style = CrundiType.Heading)
                    Text(Fmt.date(ver.at), style = CrundiType.Meta, modifier = Modifier.weight(1f))
                    if (i == 0) StatusBadge("current", BadgeKind.Accent)
                    else Tool("Restore", "rotate-ccw", enabled = state.busy != w.id) { events.onRestore(w, ver.version) }
                }
                CrundiDivider()
            }
        }
    }
}

// ─────────────────────────── Panel pane ───────────────────────────

/** Which panel a Panel pane shows, kept on the phone by the pane's key (the cell holds none until one is picked). */
private fun paneChoice(ctx: android.content.Context) = ctx.getSharedPreferences("crundi-panel-panes", android.content.Context.MODE_PRIVATE)

/** A Panel pane. [cell].data: widgetId (empty: the picker). */
@Composable
fun PanelPane(cell: WbCell?) {
    val ctx = LocalContext.current
    val api = LocalAppServices.current.api
    val project = currentPanelProject()
    val scope = rememberCoroutineScope()
    val key = cell?.key.orEmpty()
    val workbench = mv.obitech.crundi.ui.workbench.LocalWorkbench.current
    var chosen by remember(key) { mutableStateOf(cell?.data?.get("widgetId")?.ifEmpty { null } ?: paneChoice(ctx).getString(key, null)) }
    val list = rememberPanelList(project)
    val id = chosen
    if (id != null && list.value.valueOrNull?.let { l -> l.none { it.id == id } } != true) {
        PanelFrame(project, id, "cell", Modifier.fillMaxSize())
        return
    }
    var error by remember { mutableStateOf<String?>(null) }
    PanelPicker(
        list.value, project, error,
        onPick = { w ->
            // As the web: the panel is placed in the workbench (opened if it was
            // closed) and this chooser's place goes to its own pane. Without a
            // workbench (a stand-alone screen) it simply shows here.
            val wb = workbench
            scope.launch {
                try { api?.widgets?.update(project, w.id, slot = "cell", closed = false) } catch (e: ApiException) { error = "Could not open the panel: " + errorText(e); return@launch }
                if (wb != null && key.isNotEmpty()) wb.swapPicker(key, w.id)
                else { chosen = w.id; if (key.isNotEmpty()) paneChoice(ctx).edit().putString(key, w.id).apply() }
            }
        },
        onRetry = list::reload,
    )
}

/** "Choose a panel to open here", as the web's picker. */
@Composable
fun PanelPicker(list: Load<List<Widget>>, project: String, error: String? = null, onPick: (Widget) -> Unit = {}, onRetry: () -> Unit = {}) {
    when (list) {
        Load.Loading -> LoadingState(text = "Loading panels…")
        is Load.Failed -> ErrorState(list.message, onRetry = onRetry)
        is Load.Ready -> if (list.value.isEmpty()) EmptyState(
            "panel", if (project.isEmpty()) "No global panels yet" else "No panels in this project yet",
            body = "Ask Claude for one in a chat: “make a panel showing…”, or “show me a live panel of this task”.",
        ) else LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(Dimens.ScreenPad), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            item { Text("Choose a panel to open here", style = CrundiType.Section, modifier = Modifier.padding(bottom = 6.dp)) }
            if (error != null) item { Text(error, style = CrundiType.Small.copy(color = CrundiColors.Red)) }
            items(list.value.sortedByDescending { it.updatedMs() }, key = { it.id }) { w ->
                Row(
                    Modifier.fillMaxWidth().heightIn(min = Dimens.Touch).clip(Dimens.ShapeSm).background(CrundiColors.BgSecondary)
                        .border(Dimens.Hairline, CrundiColors.BorderSubtle, Dimens.ShapeSm).clickable { onPick(w) }.padding(horizontal = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    CrundiIcon("panel", size = Dimens.IconSm, tint = CrundiColors.AccentHover)
                    Text(w.title.ifEmpty { w.id }, style = CrundiType.Body, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                    if (w.needsGrant) MiniBadge("approve", BadgeKind.Warn)
                    Text(pickerWord(w), style = CrundiType.Meta.copy(color = CrundiColors.TextMuted))
                }
            }
        }
    }
}

// ─────────────────────────── Chat dock ───────────────────────────

/**
 * A chat with its docked panels above the conversation, as the web's
 * syncDock on a phone: the dock takes at most 42% of the chat's height.
 * For ui.chats to wrap its chat in: WithPanelDock(chatId) { ChatContent() }.
 */
@Composable
fun WithPanelDock(chatId: String, modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    val project = currentPanelProject()
    val list = rememberPanelList(project)
    val docked = list.value.valueOrNull.orEmpty().filter { it.chat == chatId && it.open && it.slot == "dock" }
    androidx.compose.foundation.layout.BoxWithConstraints(modifier.fillMaxSize()) {
        val maxDock = maxHeight * 0.42f
        Column(Modifier.fillMaxSize()) {
            if (docked.isNotEmpty()) PanelDock(project, docked, maxDock)
            Box(Modifier.weight(1f).fillMaxWidth()) { content() }
        }
    }
}

/** The dock itself: one item per panel, each with its header (title, approve, collapse, more, close). */
@Composable
fun PanelDock(project: String, docked: List<Widget>, maxHeight: androidx.compose.ui.unit.Dp, modifier: Modifier = Modifier) {
    val api = LocalAppServices.current.api
    val scope = rememberCoroutineScope()
    Column(modifier.fillMaxWidth().heightIn(max = maxHeight).background(CrundiColors.BgPrimary).verticalScroll(rememberScrollState())) {
        for (w in docked) {
            var collapsed by remember(w.id, w.collapsed) { mutableStateOf(w.collapsed) }
            var picking by remember(w.id) { mutableStateOf(false) }
            fun patch(block: suspend () -> Unit) = scope.launch { try { block() } catch (_: ApiException) { } }
            DockHead(
                w, collapsed,
                onToggle = { collapsed = !collapsed; val c = collapsed; patch { api?.widgets?.update(project, w.id, collapsed = c) } },
                onMore = { picking = true },
                onClose = { patch { api?.widgets?.update(project, w.id, closed = true) } },
            )
            if (!collapsed) PanelFrame(project, w.id, "dock", Modifier.fillMaxWidth(), autoHeight = true, maxHeight = (maxHeight - 30.dp).coerceAtLeast(120.dp))
            CrundiDivider()
            if (picking) SlotPicker(w, onPick = { s -> picking = false; if (s != w.slot) patch { api?.widgets?.update(project, w.id, slot = s) } }, onDismiss = { picking = false })
        }
    }
}

/** A dock item's header (the web's .wg-dock-head). */
@Composable
fun DockHead(w: Widget, collapsed: Boolean, onToggle: () -> Unit = {}, onMore: () -> Unit = {}, onClose: () -> Unit = {}) {
    Row(
        Modifier.fillMaxWidth().height(36.dp).background(CrundiColors.BgSecondary).padding(start = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(Modifier.weight(1f).clickable(onClick = onToggle), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            CrundiIcon("panel", size = 13.dp, tint = CrundiColors.AccentHover)
            Text(w.title.uppercase(), style = CrundiType.Section, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
            if (w.needsGrant) MiniBadge("approve", BadgeKind.Warn)
        }
        CrundiIconButton(if (collapsed) "chevron-down" else "chevron-up", onToggle, contentDescription = if (collapsed) "Expand" else "Collapse", size = 14.dp, touch = 36.dp)
        CrundiIconButton("more", onMore, contentDescription = "Where this panel sits", size = 14.dp, touch = 36.dp)
        CrundiIconButton("x", onClose, contentDescription = "Close panel", size = 14.dp, touch = 36.dp)
    }
}

// ─────────────────────────── Top bar chips ───────────────────────────

/**
 * The top bar's Panels button, opened: the chips with their values, and the
 * chosen chip's panel in a popover (the web's wg-chips list and openPop).
 * For the shell: show it from ShellCallbacks.onPanels.
 */
@Composable
fun PanelChipsPopover(onDismiss: () -> Unit, onOpenTab: () -> Unit = {}) {
    val api = LocalAppServices.current.api
    val project = currentPanelProject()
    val list = rememberPanelList(project)
    val chips = list.value.valueOrNull.orEmpty().filter { it.isChip }
    val values = rememberChipValues(project, chips)
    var open by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    Dialog(onDismissRequest = onDismiss, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        val w = chips.firstOrNull { it.id == open }
        Box(Modifier.fillMaxSize().clickable(onClick = onDismiss), contentAlignment = Alignment.TopCenter) {
            ChipsPopoverCard(
                chips, values, w, project,
                onChip = { open = it.id },
                onBack = { open = null },
                onClose = onDismiss,
                onOpenTab = { onDismiss(); onOpenTab() },
                onSlot = { cw, s -> scope.launch { try { api?.widgets?.update(project, cw.id, slot = s); open = null; list.reload() } catch (_: ApiException) { } } },
                loading = list.value is Load.Loading,
            )
        }
    }
}

/** The popover's card: the list of chips, or one chip's panel. Stateless, for screenshots. */
@Composable
fun ChipsPopoverCard(
    chips: List<Widget>, values: Map<String, String>, open: Widget?, project: String,
    onChip: (Widget) -> Unit = {}, onBack: () -> Unit = {}, onClose: () -> Unit = {}, onOpenTab: () -> Unit = {},
    onSlot: (Widget, String) -> Unit = { _, _ -> }, loading: Boolean = false,
) {
    var picking by remember(open?.id) { mutableStateOf(false) }
    Column(
        Modifier.padding(top = Dimens.TopBarHeight + 8.dp, start = 12.dp, end = 12.dp).fillMaxWidth()
            .heightIn(max = if (open != null) 560.dp else 420.dp)
            .clip(Dimens.Shape).background(CrundiColors.BgPrimary).border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape)
            .clickable(enabled = false) {},
    ) {
        Row(Modifier.fillMaxWidth().height(40.dp).background(CrundiColors.BgSecondary).padding(start = 6.dp), verticalAlignment = Alignment.CenterVertically) {
            if (open != null) CrundiIconButton("chevron-left", onBack, contentDescription = "Back to the chips", size = 16.dp, touch = 36.dp)
            else Spacer(Modifier.size(6.dp))
            CrundiIcon("panel", size = 14.dp, tint = CrundiColors.AccentHover)
            Text((open?.title ?: "Panels").uppercase(), style = CrundiType.Section, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f).padding(start = 6.dp))
            if (open != null) CrundiIconButton("more", { picking = true }, contentDescription = "Where this panel sits", size = 14.dp, touch = 36.dp)
            CrundiIconButton("x", onClose, contentDescription = "Close", size = 14.dp, touch = 36.dp)
        }
        CrundiDivider()
        if (open != null) PanelFrame(project, open.id, "chip", Modifier.fillMaxWidth().height(500.dp))
        else if (loading) LoadingState(Modifier.height(120.dp))
        else Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            if (chips.isEmpty()) Text("No panels in the top bar. Claude puts small live values here, like a test count or a build status.", style = CrundiType.Small)
            for (c in chips) PanelChip(c, values[c.id], onClick = { onChip(c) }, modifier = Modifier.fillMaxWidth())
            Row(Modifier.fillMaxWidth().clip(Dimens.ShapeSm).clickable(onClick = onOpenTab).padding(vertical = 8.dp, horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                CrundiIcon("layers", size = Dimens.IconSm, tint = CrundiColors.AccentHover)
                Text("All panels", style = CrundiType.Small.copy(color = CrundiColors.AccentHover))
            }
        }
    }
    if (picking && open != null) SlotPicker(open, onPick = { picking = false; if (it != open.slot) onSlot(open, it) }, onDismiss = { picking = false })
}
