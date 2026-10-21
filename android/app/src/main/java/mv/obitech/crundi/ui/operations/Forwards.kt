package mv.obitech.crundi.ui.operations

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import mv.obitech.crundi.api.model.DockerForward
import mv.obitech.crundi.api.model.Forward
import mv.obitech.crundi.api.model.NewForward
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiChip
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.ListRow
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.StatusBadge
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.shell.LocalHost
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

data class ForwardsState(
    val forwards: Load<List<Forward>> = Load.Loading,
    /** What is behind each forward (from Docker), by host; empty when Docker is not here. */
    val behind: Map<String, DockerForward> = emptyMap(),
    val domain: String = "",
    /** Subdomains can be made (a domain with its certificate). */
    val subdomains: Boolean = false,
    val projects: List<String> = emptyList(),
    val busy: String? = null,
    val note: Actions.Note? = null,
)

class ForwardsEvents(
    val onAdd: (NewForward, watch: Boolean) -> Unit = { _, _ -> },
    val onRemove: (Forward) -> Unit = {},
    val onWatch: (Forward, Boolean) -> Unit = { _, _ -> },
    val onProject: (Forward, String) -> Unit = { _, _ -> },
    val onOpen: (String) -> Unit = {},
    val onCloseNote: () -> Unit = {},
    val onRetry: () -> Unit = {},
    val onBack: () -> Unit = {},
)

/** The desktop's groups, in its order, with its help words. */
private enum class Behind(val title: String, val help: String?) {
    Nothing("Nothing behind it", "No container publishes this port and nothing answers on it. Left over from something that is gone, most likely."),
    Stopped("Container stopped", "The container that publishes this port is not running."),
    Running("Running container", null),
    Program("Another program", "Something on this machine answers on the port, but not a container."),
}

private fun behindOf(d: DockerForward?): Behind? = when {
    d == null -> null
    d.container != null && d.container.running -> Behind.Running
    d.container != null -> Behind.Stopped
    d.listening -> Behind.Program
    else -> Behind.Nothing
}

@Composable
fun ForwardsRoute() {
    val app = LocalAppServices.current
    val api = app.api
    val host = LocalHost.current
    val nav = mv.obitech.crundi.ui.nav.LocalNavigator.current
    val shell by app.shell.state.collectAsStateWithLifecycle()
    val actions = rememberActions()
    val list = rememberLoad { api.forwards.list() }
    val opts = rememberLoad { runCatching { api.forwards.options() }.getOrNull() }
    val docker = rememberLoad { runCatching { api.docker.forwards() }.getOrDefault(emptyList()) }
    val reload = { list.reload(); docker.reload() }
    val o = opts.value.valueOrNull
    ForwardsScreen(
        ForwardsState(
            forwards = when (val l = list.value) { is Load.Ready -> Load.Ready(l.value.forwards); is Load.Failed -> l; Load.Loading -> Load.Loading },
            behind = docker.value.valueOrNull.orEmpty().associateBy { it.host },
            domain = list.value.valueOrNull?.domain?.ifEmpty { null } ?: o?.domain.orEmpty(),
            subdomains = o?.choices?.any { it.id == "subdomain" && it.available } == true,
            projects = shell.projects.map { it.alias },
            busy = actions.busy,
            note = actions.note,
        ),
        ForwardsEvents(
            onAdd = { f, watch ->
                actions.run("add", after = reload) {
                    val made = api.forwards.add(f)
                    if (watch) api.forwards.setWatch(made.host, true)
                    actions.note = Actions.Note("Forward ready: ${made.url}", false)
                }
            },
            onRemove = { f -> actions.run(f.host, "Removed ${f.host}", reload) { api.forwards.remove(f.host) } },
            onWatch = { f, on -> actions.run(f.host, if (on) "You will be warned when ${f.host} goes down" else "No longer watching ${f.host}", reload) { api.forwards.setWatch(f.host, on) } },
            onProject = { f, p -> actions.run(f.host, if (p.isEmpty()) "${f.host} is not linked to a project" else "${f.host} now serves $p", reload) { api.forwards.setProject(f.host, p) } },
            onOpen = host::openExternal,
            onCloseNote = actions::clear,
            onRetry = reload,
            onBack = { nav.back() },
        ),
    )
}

@Composable
fun ForwardsScreen(state: ForwardsState, events: ForwardsEvents = ForwardsEvents(), adding: Boolean = false) {
    var add by remember { mutableStateOf(adding) }
    Column(Modifier.fillMaxSize()) {
        val all = state.forwards.valueOrNull
        val orphans = all?.count { behindOf(state.behind[it.host]) == Behind.Nothing } ?: 0
        // Reached from Info and Docker, not a tab: a detail screen with a back arrow.
        mv.obitech.crundi.ui.components.TopBar(
            "Forwards",
            onBack = events.onBack,
            subtitle = all?.let { "${it.size} forward${if (it.size == 1) "" else "s"}" + (if (orphans > 0) ", $orphans with nothing behind them" else "") + (if (state.domain.isNotEmpty()) " · ${state.domain}" else "") },
        ) { CrundiButton("Add forward", { add = true }, kind = ButtonKind.Primary, icon = "plus") }
        NoteBanner(state.note, events.onCloseNote)
        when (val l = state.forwards) {
            Load.Loading -> LoadingState(text = "Loading forwards")
            is Load.Failed -> ErrorState(l.message, onRetry = events.onRetry)
            is Load.Ready -> if (l.value.isEmpty()) EmptyState(
                "globe", "No forwards",
                body = "A forward reaches a port on this machine through Crundi, by name: for a container, or for anything else listening here.",
                action = "Add forward", onAction = { add = true },
            ) else LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(Dimens.ScreenPad), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                val grouped = l.value.groupBy { behindOf(state.behind[it.host]) }
                val order = listOf<Behind?>(Behind.Nothing, Behind.Stopped, Behind.Running, Behind.Program, null)
                for (g in order) {
                    val rows = grouped[g] ?: continue
                    if (g != null) item(key = "h-${g.name}") {
                        Column(Modifier.padding(top = 6.dp)) {
                            Text("${g.title.uppercase()} · ${rows.size}", style = CrundiType.Section)
                            if (g.help != null) Text(g.help, style = CrundiType.Small)
                        }
                    }
                    items(rows, key = { it.host }) { f -> ForwardCard(f, state.behind[f.host], state, events) }
                }
            }
        }
    }
    if (add) AddForwardDialog(state.domain, state.subdomains, onDismiss = { add = false }, onCreate = { f, w -> add = false; events.onAdd(f, w) })
}

@Composable
private fun ForwardCard(f: Forward, d: DockerForward?, state: ForwardsState, events: ForwardsEvents) {
    var confirm by remember { mutableStateOf(false) }
    var pickProject by remember { mutableStateOf(false) }
    val busy = state.busy == f.host
    Column(
        Modifier.fillMaxWidth().clip(Dimens.Shape).background(CrundiColors.BgCard).border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape).padding(horizontal = 14.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiIcon("globe", size = Dimens.Icon, tint = CrundiColors.TextSecondary)
            Text(f.host, style = CrundiType.Heading, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (f.public) StatusBadge("public", BadgeKind.Warn)
            if (f.watch) StatusBadge("watched", BadgeKind.Good)
        }
        Text(f.url.ifEmpty { f.host }, style = CrundiType.Meta.copy(color = CrundiColors.AccentHover, fontSize = 12.sp), maxLines = 1, overflow = TextOverflow.Ellipsis,
            modifier = Modifier.clickable { if (f.url.isNotEmpty()) events.onOpen(f.url) })
        Text(
            listOfNotNull("port ${f.port}", f.mode, f.project.ifEmpty { null }, f.description.ifEmpty { null }).joinToString(" · "),
            style = CrundiType.Meta, maxLines = 2, overflow = TextOverflow.Ellipsis,
        )
        d?.container?.let { c ->
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                CrundiIcon("docker", size = 13.dp, tint = CrundiColors.TextMuted)
                StatusBadge(c.name + if (c.health == "unhealthy") " · unhealthy" else "", if (c.running && c.health != "unhealthy") BadgeKind.Good else BadgeKind.Bad)
            }
        }
        @OptIn(ExperimentalLayoutApi::class)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiButton("Open", { events.onOpen(f.url) }, icon = "external-link", enabled = f.url.isNotEmpty())
            CrundiButton(if (f.watch) "Stop warning" else "Warn me", { events.onWatch(f, !f.watch) }, kind = if (f.watch) ButtonKind.Primary else ButtonKind.Secondary, icon = "bell", enabled = !busy)
            CrundiButton(f.project.ifEmpty { "No project" }, { pickProject = true }, icon = "folder", enabled = !busy)
            CrundiButton("Remove", { confirm = true }, kind = ButtonKind.Danger, icon = "trash", enabled = !busy)
        }
    }
    if (confirm) CrundiDialog(
        "Remove ${f.host}?", onDismiss = { confirm = false },
        message = "${f.url.ifEmpty { f.host }} stops answering. Whatever is on port ${f.port} keeps running.",
        confirm = "Remove forward", danger = true, onConfirm = { confirm = false; events.onRemove(f) },
    )
    if (pickProject) CrundiDialog(
        "Which project does ${f.host} serve?", onDismiss = { pickProject = false },
        message = "Outside collaborators on that project can open it.",
    ) {
        Column(Modifier.heightIn(max = 360.dp).verticalScroll(rememberScrollState())) {
            ListRow("No project", icon = "minus", active = f.project.isEmpty(), onClick = { pickProject = false; events.onProject(f, "") })
            for (p in state.projects) ListRow(p, icon = "folder", active = p == f.project, onClick = { pickProject = false; events.onProject(f, p) })
        }
    }
}

/** The add-forward form, inside a dialog (and drawn on its own in screenshots). */
@Composable
fun AddForwardForm(domain: String, subdomains: Boolean, onCancel: () -> Unit, onCreate: (NewForward, Boolean) -> Unit, initialPort: Int? = null, initialName: String = "") {
    var port by remember { mutableStateOf(initialPort?.toString() ?: "") }
    var name by remember { mutableStateOf(initialName) }
    var mode by remember { mutableStateOf(if (subdomains) "subdomain" else "path") }
    var public by remember { mutableStateOf(false) }
    var watch by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text(
            "Forwards 127.0.0.1 on the port you give: a container's published port, a dev server, anything listening on this machine. Private unless Public is ticked: only people signed in to Crundi can open it.",
            style = CrundiType.Small,
        )
        CrundiTextField(port, { t -> port = t.filter(Char::isDigit).take(5) }, placeholder = "Port", mono = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number))
        CrundiTextField(name, { name = it.take(32) }, placeholder = "name", mono = true)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            if (subdomains) CrundiChip("${name.ifEmpty { "name" }}.$domain", selected = mode == "subdomain", onClick = { mode = "subdomain" })
            CrundiChip("/tunnel/${name.ifEmpty { "name" }}/", selected = mode == "path", onClick = { mode = "path" })
        }
        CheckRow("Public", public) { public = it }
        CheckRow("Warn if it goes down", watch) { watch = it }
        error?.let { Text(it, style = CrundiType.Small.copy(color = CrundiColors.Red)) }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp, Alignment.End)) {
            CrundiButton("Cancel", onCancel)
            CrundiButton("Create", {
                val p = port.toIntOrNull()
                val n = name.trim().lowercase()
                error = when {
                    p == null || p !in 1..65535 -> "Give the port to forward, 1 to 65535"
                    n.isNotEmpty() && !Regex("^[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?$").matches(n) -> "Lowercase letters, digits and hyphens only, not starting or ending with one."
                    else -> null
                }
                if (error == null) onCreate(NewForward(port = p!!, name = n.ifEmpty { null }, mode = mode, public = public), watch)
            }, kind = ButtonKind.Primary, icon = "check")
        }
    }
}

@Composable
private fun AddForwardDialog(domain: String, subdomains: Boolean, onDismiss: () -> Unit, onCreate: (NewForward, Boolean) -> Unit) {
    CrundiDialog("Add forward", onDismiss = onDismiss, dismiss = null) { AddForwardForm(domain, subdomains, onDismiss, onCreate) }
}
