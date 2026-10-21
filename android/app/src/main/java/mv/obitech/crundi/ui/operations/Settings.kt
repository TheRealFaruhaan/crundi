package mv.obitech.crundi.ui.operations

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
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
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import mv.obitech.crundi.api.model.ClaudeUpdate
import mv.obitech.crundi.api.model.NotifyChannel
import mv.obitech.crundi.api.model.ServerSettings
import mv.obitech.crundi.api.model.SettingsChange
import mv.obitech.crundi.api.model.UpdateStatus
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.Banner
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiDivider
import mv.obitech.crundi.ui.components.CrundiSwitchRow
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.ListRow
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.PanelHeader
import mv.obitech.crundi.ui.components.StatusBadge
import mv.obitech.crundi.ui.components.TopBar
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.shell.LocalHost
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/** The settings pages, each a detail route under "settings/". */
enum class SettingsPage(val route: String, val title: String, val icon: String, val blurb: String) {
    Server("settings/server", "Server and Claude Code", "server", "Version, updates, release channel, restart"),
    Notifications("settings/notifications", "Notifications", "bell", "Channels, and which events are worth telling you"),
    Prompt("settings/prompt", "System prompt", "message-square", "Added to every chat and terminal Crundi starts"),
    General("settings/general", "Usage and idle panes", "clock", "Keep the 5-hour window warm, close idle panes"),
    Config("settings/config", "Configuration", "settings", "Telegram, projects folder, port, tunnel"),
    Machine("settings/machine", "This machine", "hard-drive", "CPU, memory, disk, and space to reclaim"),
    Logs("settings/logs", "Server logs", "file-text", "What the server is saying, live"),
    Sessions("settings/sessions", "Signed-in devices", "smartphone", "Where you are signed in; sign others out"),
}

// ─── Front page ───

data class SettingsHome(val update: UpdateStatus? = null, val host: String = "", val appVersion: String = "")

@Composable
fun SettingsRoute() {
    val app = LocalAppServices.current
    val nav = LocalNavigator.current
    val host = LocalHost.current
    val shell by app.shell.state.collectAsStateWithLifecycle()
    val update = rememberLoad { app.api.system.update() }
    SettingsScreen(SettingsHome(update.value.valueOrNull, shell.serverHost, mv.obitech.crundi.BuildConfig.VERSION_NAME), onPage = { nav.to(it.route) }, onAppSettings = host::openAppSettings)
}

@Composable
fun SettingsScreen(home: SettingsHome, onPage: (SettingsPage) -> Unit = {}, onAppSettings: () -> Unit = {}) {
    Column(Modifier.fillMaxSize()) {
        PanelHeader("Settings", meta = home.host.takeIf { it.isNotEmpty() }?.let { "Server $it" })
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(vertical = 8.dp)) {
            val u = home.update?.update
            if (u != null && u.available) Banner("Version ${u.latest} is available. Open Server and Claude Code to update.", Modifier.padding(horizontal = Dimens.ScreenPad, vertical = 6.dp).fillMaxWidth(), BadgeKind.Accent, "download")
            Group("Server") {
                for (p in SettingsPage.entries) {
                    ListRow(
                        p.title, subtitle = if (p == SettingsPage.Server && u != null) "Version ${u.current}" + (if (u.available) " · ${u.latest} is available" else " · up to date") else p.blurb,
                        icon = p.icon, onClick = { onPage(p) },
                        trailing = { CrundiIcon("chevron-right", size = Dimens.IconSm, tint = CrundiColors.TextMuted) },
                    )
                }
            }
            Group("This phone") {
                ListRow(
                    "App settings", subtitle = "Notifications, bubbles and paired servers on this phone" + (if (home.appVersion.isNotEmpty()) " · app ${home.appVersion}" else ""),
                    icon = "smartphone", onClick = onAppSettings,
                    trailing = { CrundiIcon("chevron-right", size = Dimens.IconSm, tint = CrundiColors.TextMuted) },
                )
            }
        }
    }
}

/** A settings detail page: its own top bar with Back, the note, and a scrolling body. */
@Composable
fun SettingsPageFrame(page: SettingsPage, note: Actions.Note? = null, onCloseNote: () -> Unit = {}, onBack: () -> Unit = {}, actions: @Composable androidx.compose.foundation.layout.RowScope.() -> Unit = {}, body: @Composable () -> Unit) {
    Column(Modifier.fillMaxSize()) {
        TopBar(page.title, onBack = onBack, actions = actions)
        NoteBanner(note, onCloseNote)
        body()
    }
}

@Composable
fun ScrollBody(content: @Composable () -> Unit) {
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(vertical = 8.dp)) { content() }
}

// ─── Server and Claude Code ───

data class ServerPageState(
    val update: Load<UpdateStatus> = Load.Loading,
    val claude: Load<ClaudeUpdate> = Load.Loading,
    val busy: String? = null,
    val note: Actions.Note? = null,
)

class ServerPageEvents(
    val onCheck: () -> Unit = {},
    val onApply: () -> Unit = {},
    val onChannel: (String) -> Unit = {},
    val onRestart: () -> Unit = {},
    val onCheckClaude: () -> Unit = {},
    val onUpdateClaude: (String) -> Unit = {},
    val onCloseNote: () -> Unit = {},
    val onBack: () -> Unit = {},
)

@Composable
fun ServerPageRoute() {
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val actions = rememberActions()
    var force by remember { mutableStateOf(false) }
    val update = rememberLoad(force) { api.system.update(force) }
    val claude = rememberLoad(force) { api.system.claude(force) }
    ServerPageScreen(
        ServerPageState(update.value, claude.value, actions.busy, actions.note),
        ServerPageEvents(
            onCheck = { if (force) { update.reload(); claude.reload() } else force = true },
            onApply = { actions.run("apply", "Updating to ${update.value.valueOrNull?.update?.latest}", update::reload) { api.system.applyUpdate() } },
            onChannel = { c -> actions.run("channel", if (c == "dev") "Dev releases on" else "Production releases", update::reload) { api.system.setUpdateChannel(c) } },
            onRestart = { actions.run("restart", "Restarting…") { api.system.restart() } },
            onCheckClaude = { if (force) claude.reload() else force = true },
            onUpdateClaude = { v ->
                val beyond = claude.value.valueOrNull?.beyondTested == true
                actions.run("claude", "Claude Code updated to $v", claude::reload) { api.system.updateClaude(v, allowBeyondTested = beyond) }
            },
            onCloseNote = actions::clear,
            onBack = { nav.back() },
        ),
    )
}

@Composable
fun ServerPageScreen(state: ServerPageState, events: ServerPageEvents = ServerPageEvents()) {
    var confirmApply by remember { mutableStateOf(false) }
    var confirmRestart by remember { mutableStateOf(false) }
    var confirmDev by remember { mutableStateOf(false) }
    var confirmClaude by remember { mutableStateOf(false) }
    SettingsPageFrame(SettingsPage.Server, state.note, events.onCloseNote, events.onBack) {
        ScrollBody {
            Group("Server") {
                when (val l = state.update) {
                    Load.Loading -> Text("Checking for updates…", style = CrundiType.Small)
                    is Load.Failed -> Text("Could not check for updates: ${l.message}", style = CrundiType.Small.copy(color = CrundiColors.Red))
                    is Load.Ready -> {
                        val u = l.value.update
                        Text("Version ${u.current}", style = CrundiType.Heading)
                        Text(
                            when {
                                u.applying -> "Updating… the server will restart on its own."
                                u.available && u.channel == "dev" -> if (u.latestKind == "dev") "Dev build ${u.latest} is available." else "Production release ${u.latest} is available."
                                u.available -> "Version ${u.latest} is available."
                                u.error.isNotEmpty() -> "Last check failed: ${u.error}"
                                else -> "Up to date · checked ${Fmt.ago(u.checkedAt)}."
                            },
                            style = CrundiType.Small.copy(color = if (u.available) CrundiColors.AccentHover else CrundiColors.TextSecondary),
                        )
                        if (u.blocker.isNotEmpty()) Text(u.blocker, style = CrundiType.Small.copy(color = CrundiColors.Yellow))
                        @OptIn(ExperimentalLayoutApi::class)
                        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            CrundiButton(if (u.available) "Check" else "Check now", events.onCheck, icon = "refresh")
                            if (u.available) CrundiButton(if (state.busy == "apply" || u.applying) "Working…" else "Update to ${u.latest}", { confirmApply = true },
                                kind = ButtonKind.Primary, icon = "download", enabled = state.busy != "apply" && !u.applying && u.blocker.isEmpty())
                        }
                        CrundiDivider()
                        Text("Release channel", style = CrundiType.Body)
                        Segments(listOf("production" to "Production", "dev" to "Dev"), u.channel, { c -> if (c == "dev" && u.channel != "dev") confirmDev = true else if (c != u.channel) events.onChannel(c) }, Modifier.fillMaxWidth())
                        Text("Dev gets preview builds early. A newer production release is offered either way.", style = CrundiType.Small)
                        if (u.channel == "production" && u.currentIsDev) Text("This server runs a dev build. Production updates resume once a production release is newer than ${u.current}.", style = CrundiType.Small.copy(color = CrundiColors.Yellow))
                        if (l.value.canRestart) {
                            CrundiDivider()
                            Text("Settings like the Telegram token and TLS only take effect after a restart.", style = CrundiType.Small)
                            CrundiButton("Restart server", { confirmRestart = true }, kind = ButtonKind.Danger, icon = "rotate-ccw", enabled = state.busy != "restart")
                        }
                    }
                }
            }
            Group("Claude Code") {
                when (val l = state.claude) {
                    Load.Loading -> Text("Checking…", style = CrundiType.Small)
                    is Load.Failed -> Text("Last check failed: ${l.message}", style = CrundiType.Small.copy(color = CrundiColors.Red))
                    is Load.Ready -> {
                        val c = l.value
                        Text("Version ${c.installed.ifEmpty { "—" }}", style = CrundiType.Heading)
                        Text(
                            when {
                                c.applying || state.busy == "claude" -> "Installing… this can take a minute."
                                c.installed.isEmpty() -> "Not installed, or not on the server's PATH."
                                c.error.isNotEmpty() -> "Last check failed: ${c.error}"
                                c.available -> "Version ${c.latest} is available."
                                else -> "Up to date."
                            },
                            style = CrundiType.Small,
                        )
                        if (c.tested.isNotEmpty()) Text(
                            "This Crundi build was tested with ${c.tested}." + (if (c.available && c.beyondTested) " ${c.latest} is newer than that: it will ask first." else ""),
                            style = CrundiType.Small,
                        )
                        if (c.needsSudo) Text("Installed system-wide, so updating uses sudo, which is also why Claude Code cannot update itself here.", style = CrundiType.Small)
                        if (c.blocker.isNotEmpty()) Text(c.blocker, style = CrundiType.Small.copy(color = CrundiColors.Yellow))
                        @OptIn(ExperimentalLayoutApi::class)
                        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            CrundiButton(if (c.available) "Check" else "Check now", events.onCheckClaude, icon = "refresh")
                            if (c.available && c.canUpdate) CrundiButton("Update to ${c.latest}", { if (c.beyondTested) confirmClaude = true else events.onUpdateClaude(c.latest) },
                                kind = if (c.beyondTested) ButtonKind.Danger else ButtonKind.Primary, icon = "download", enabled = state.busy != "claude" && !c.applying)
                        }
                    }
                }
            }
        }
    }
    val latest = state.update.valueOrNull?.update?.latest.orEmpty()
    if (confirmApply) CrundiDialog("Update the server now?", { confirmApply = false },
        message = "This restarts Crundi when it finishes, which ends every running chat and terminal on it.",
        confirm = "Update to $latest", onConfirm = { confirmApply = false; events.onApply() })
    if (confirmRestart) CrundiDialog("Restart the server?", { confirmRestart = false },
        message = "This ends every running chat and terminal on it. It comes back on its own in a few seconds.",
        confirm = "Restart server", danger = true, onConfirm = { confirmRestart = false; events.onRestart() })
    if (confirmDev) CrundiDialog("Switch this server to dev releases?", { confirmDev = false },
        message = "Dev builds are previews of what is coming and may be less stable.\n\nYou cannot go back to production until a production release newer than your dev build is published: Crundi never installs an older version. A newer production release is always offered on the dev channel too.",
        confirm = "Use dev releases", onConfirm = { confirmDev = false; events.onChannel("dev") })
    val c = state.claude.valueOrNull
    if (confirmClaude && c != null) CrundiDialog("Install Claude Code ${c.latest}?", { confirmClaude = false },
        message = "Claude Code ${c.latest} is newer than the version this Crundi build was tested with.\n\nClaude Code is what every chat and terminal runs on, so a problem there affects all of them.",
        confirm = "Install anyway", danger = true, onConfirm = { confirmClaude = false; events.onUpdateClaude(c.latest) })
}

// ─── Notifications ───

/** The desktop's events, grouped, with their default. */
data class NotifyEvent(val group: String, val key: String, val label: String, val default: String)

val NOTIFY_EVENTS = listOf(
    NotifyEvent("Agent", "finished", "When agent finishes", "away"),
    NotifyEvent("Agent", "needsInput", "When agent needs input", "away"),
    NotifyEvent("Kanban", "kanbanTask", "Main task status change", "never"),
    NotifyEvent("Kanban", "kanbanSubtask", "Sub task status change", "never"),
    NotifyEvent("Schedule", "scheduleRun", "When a schedule executes", "away"),
    NotifyEvent("Schedule", "scheduledChat", "Result of a scheduled chat", "always"),
    NotifyEvent("Services", "serviceDown", "When a service crashes or stops", "always"),
    NotifyEvent("Services", "serviceUp", "When a service starts", "away"),
    NotifyEvent("Forwards", "forwardDown", "When a watched forward goes down", "always"),
    NotifyEvent("Forwards", "forwardUp", "When a watched forward is back", "away"),
    NotifyEvent("Mindmap", "mindmapAdd", "When a node is added", "never"),
    NotifyEvent("Mindmap", "mindmapDelete", "When a node is deleted", "never"),
    NotifyEvent("MCP Browser", "browserLaunch", "When a browser launches", "never"),
    NotifyEvent("MCP Browser", "browserStop", "When a browser stops", "never"),
    NotifyEvent("Security", "secretRequest", "Secret access requested", "always"),
    NotifyEvent("Collaborators", "collabPermission", "When a collaborator's Claude asks for permission", "away"),
    NotifyEvent("Updates", "updateAvailable", "When a new Claude Code version is out", "away"),
    NotifyEvent("Usage", "limitReset", "When a usage limit resets", "always"),
)

data class NotificationsState(
    val channels: Load<List<NotifyChannel>> = Load.Loading,
    val prefs: Load<Map<String, String>> = Load.Loading,
    val note: Actions.Note? = null,
)

@Composable
fun NotificationsRoute() {
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val actions = rememberActions()
    val channels = rememberLoad { api.system.channels() }
    val settings = rememberLoad { api.system.settings() }
    var local by remember { mutableStateOf<Map<String, String>?>(null) }
    val prefs: Load<Map<String, String>> = local?.let { Load.Ready(it) } ?: when (val s = settings.value) {
        is Load.Ready -> Load.Ready(s.value.notifyPrefs); is Load.Failed -> s; Load.Loading -> Load.Loading
    }
    NotificationsScreen(
        NotificationsState(channels.value, prefs, actions.note),
        onChannel = { ch, on -> actions.run(ch.id, after = channels::reload) { api.system.setChannel(ch.id, on) } },
        onPref = { key, v ->
            val next = (prefs.valueOrNull.orEmpty()) + (key to v)
            local = next
            actions.run("prefs") { api.system.saveSettings(SettingsChange(notifyPrefs = next)) }
        },
        onCloseNote = actions::clear,
        onBack = { nav.back() },
    )
}

@Composable
fun NotificationsScreen(state: NotificationsState, onChannel: (NotifyChannel, Boolean) -> Unit = { _, _ -> }, onPref: (String, String) -> Unit = { _, _ -> }, onCloseNote: () -> Unit = {}, onBack: () -> Unit = {}) {
    SettingsPageFrame(SettingsPage.Notifications, state.note, onCloseNote, onBack) {
        ScrollBody {
            Group("Notification channels", detail = "Which of these get told. Turn on as many as you like, or none at all. The list below decides which events are worth telling you about.") {
                when (val l = state.channels) {
                    Load.Loading -> Text("Loading…", style = CrundiType.Small)
                    is Load.Failed -> Text(l.message, style = CrundiType.Small.copy(color = CrundiColors.Red))
                    is Load.Ready -> for (ch in l.value) CrundiSwitchRow(
                        ch.label, ch.enabled && ch.available, { if (ch.available) onChannel(ch, it) },
                        detail = if (ch.available) ch.describe else ch.unavailableReason.ifEmpty { "Not available here" },
                        modifier = Modifier.padding(horizontal = 0.dp),
                    )
                }
            }
            Text(
                "Always sends every time; When away only while you are not looking at Crundi; Never is off.",
                style = CrundiType.Small, modifier = Modifier.padding(horizontal = Dimens.ScreenPad, vertical = 4.dp),
            )
            when (val l = state.prefs) {
                Load.Loading -> LoadingState()
                is Load.Failed -> ErrorState(l.message)
                is Load.Ready -> for ((group, events) in NOTIFY_EVENTS.groupBy { it.group }) Group(group) {
                    for ((i, e) in events.withIndex()) {
                        if (i > 0) CrundiDivider()
                        Text(e.label, style = CrundiType.Body)
                        Segments(listOf("always" to "Always", "away" to "When away", "never" to "Never"), l.value[e.key] ?: e.default, { onPref(e.key, it) }, Modifier.fillMaxWidth())
                    }
                }
            }
        }
    }
}

// ─── System prompt ───

data class PromptState(
    val settings: Load<ServerSettings> = Load.Loading,
    val project: String? = null,
    val projectPrompt: String = "",
    val busy: String? = null,
    val note: Actions.Note? = null,
)

@Composable
fun PromptRoute() {
    val app = LocalAppServices.current
    val api = app.api
    val nav = LocalNavigator.current
    val shell by app.shell.state.collectAsStateWithLifecycle()
    val actions = rememberActions()
    val settings = rememberLoad { api.system.settings() }
    val projects = rememberLoad { api.projects.list() }
    val project = shell.currentProject
    PromptScreen(
        PromptState(settings.value, project, projects.value.valueOrNull?.firstOrNull { it.alias == project }?.systemPrompt.orEmpty(), actions.busy, actions.note),
        onSave = { all, mine ->
            actions.run("save", "System prompt saved", after = { settings.reload(); projects.reload() }) {
                api.system.saveSettings(SettingsChange(systemPrompt = all))
                if (project != null && mine != null) api.projects.setSystemPrompt(project, mine)
            }
        },
        onCloseNote = actions::clear,
        onBack = { nav.back() },
    )
}

@Composable
fun PromptScreen(state: PromptState, onSave: (String, String?) -> Unit = { _, _ -> }, onCloseNote: () -> Unit = {}, onBack: () -> Unit = {}) {
    SettingsPageFrame(SettingsPage.Prompt, state.note, onCloseNote, onBack) {
        when (val l = state.settings) {
            Load.Loading -> LoadingState()
            is Load.Failed -> ErrorState(l.message)
            is Load.Ready -> {
                var all by remember(l.value.systemPrompt) { mutableStateOf(l.value.systemPrompt) }
                var mine by remember(state.projectPrompt) { mutableStateOf(state.projectPrompt) }
                val max = l.value.systemPromptMax
                ScrollBody {
                    Text(
                        "Added to every chat and terminal Crundi starts, after its own description of this machine and the tools available here. Scheduled jobs get only that description, so an edit here cannot change what an unattended job does.",
                        style = CrundiType.Small, modifier = Modifier.padding(horizontal = Dimens.ScreenPad, vertical = 6.dp),
                    )
                    Group("Everywhere", detail = "Applies to every project.") {
                        CrundiTextField(all, { all = it.take(max) }, placeholder = "e.g. Prefer small commits. Ask before installing anything globally.", singleLine = false, minLines = 4, maxLines = 12)
                        Text("${all.length} / $max", style = CrundiType.Meta)
                    }
                    if (state.project != null) Group("Just ${state.project}", detail = "Read last, so it wins where it disagrees with the box above.") {
                        CrundiTextField(mine, { mine = it.take(max) }, placeholder = "e.g. This repo deploys on merge to main. Never push directly.", singleLine = false, minLines = 4, maxLines = 12)
                    } else Text("Open a project to give it its own additional prompt.", style = CrundiType.Small, modifier = Modifier.padding(horizontal = Dimens.ScreenPad, vertical = 6.dp))
                    Text(
                        "Takes effect for new conversations. A chat already open keeps the wording it started with.",
                        style = CrundiType.Small, modifier = Modifier.padding(horizontal = Dimens.ScreenPad, vertical = 6.dp),
                    )
                    Row(Modifier.fillMaxWidth().padding(horizontal = Dimens.ScreenPad, vertical = 8.dp), horizontalArrangement = Arrangement.End) {
                        CrundiButton("Save prompts", { onSave(all, if (state.project != null) mine else null) }, kind = ButtonKind.Primary, icon = "check", enabled = state.busy != "save")
                    }
                }
            }
        }
    }
}

// ─── Usage warm-up and idle panes ───

data class GeneralState(val settings: Load<ServerSettings> = Load.Loading, val busy: String? = null, val note: Actions.Note? = null)

@Composable
fun GeneralRoute() {
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val actions = rememberActions()
    val settings = rememberLoad { api.system.settings() }
    GeneralScreen(
        GeneralState(settings.value, actions.busy, actions.note),
        onWarmup = { on ->
            actions.run("warm", if (on) "Crundi will keep the 5-hour window warm" else "Window warm-up off", settings::reload) { api.system.saveSettings(SettingsChange(limitWarmup = on)) }
        },
        onAutoPark = { m ->
            actions.run("park", if (m == 0) "Idle panes stay open" else "Idle panes close after $m minutes", settings::reload) { api.system.saveSettings(SettingsChange(autoParkMinutes = m)) }
        },
        onCloseNote = actions::clear,
        onBack = { nav.back() },
    )
}

@Composable
fun GeneralScreen(state: GeneralState, onWarmup: (Boolean) -> Unit = {}, onAutoPark: (Int) -> Unit = {}, onCloseNote: () -> Unit = {}, onBack: () -> Unit = {}) {
    var confirmOff by remember { mutableStateOf(false) }
    SettingsPageFrame(SettingsPage.General, state.note, onCloseNote, onBack) {
        when (val l = state.settings) {
            Load.Loading -> LoadingState()
            is Load.Failed -> ErrorState(l.message)
            is Load.Ready -> {
                val s = l.value
                val warm = s.limitWarmup
                val on = (warm?.get("enabled") as? JsonPrimitive)?.booleanOrNull == true
                ScrollBody {
                    Group("Usage") {
                        CrundiSwitchRow("Keep the 5-hour window warm", on, { if (!it && s.queuedLimitReset > 0) confirmOff = true else onWarmup(it) }, modifier = Modifier.padding(horizontal = 0.dp))
                        Text(
                            "Claude's 5-hour allowance is a rolling window that starts on your first message, so beginning real work also starts the clock, and it can run out mid-session. When on, Crundi checks about once a minute and, if the window has lapsed while you were idle, sends a one-word message to open the next one. Never while a session is working, and never if your own work already opened a window.",
                            style = CrundiType.Small,
                        )
                        val last = (warm?.get("lastWarmResult") as? JsonPrimitive)?.contentOrNull.orEmpty()
                        val lastAt = Fmt.iso((warm?.get("lastWarmAt") as? JsonPrimitive)?.contentOrNull)
                        if (last.isNotEmpty()) Text("Last: $last · ${Fmt.date(lastAt)}", style = CrundiType.Meta)
                        val reset = Fmt.iso((warm?.get("lastKnownReset") as? JsonPrimitive)?.contentOrNull)
                        if (reset > 0) Text(if (reset > System.currentTimeMillis()) "Current window resets ${Fmt.date(reset)}" else "No window open · last one ended ${Fmt.date(reset)}", style = CrundiType.Meta)
                    }
                    Group("Idle panes") {
                        var text by remember(s.autoParkMinutes) { mutableStateOf(s.autoParkMinutes.toString()) }
                        var error by remember { mutableStateOf<String?>(null) }
                        Text("Close idle panes after (minutes)", style = CrundiType.Body)
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            CrundiTextField(text, { t -> text = t.filter(Char::isDigit).take(5); error = null }, Modifier.weight(1f), mono = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number))
                            CrundiButton("Save", {
                                val m = text.toIntOrNull() ?: -1
                                if (m != 0 && m < s.autoParkMin) error = "Use 0 to turn it off, or ${s.autoParkMin} minutes or more" else onAutoPark(m)
                            }, kind = ButtonKind.Primary, enabled = state.busy != "park")
                        }
                        error?.let { Text(it, style = CrundiType.Small.copy(color = CrundiColors.Red)) }
                        Text(
                            "A chat or Claude terminal with nothing going on closes once you have also been away this long. It keeps its place, shows what it was, and Resume picks the conversation back up. 0 turns it off; otherwise at least ${s.autoParkMin}. Shells are never closed for being idle.",
                            style = CrundiType.Small,
                        )
                    }
                }
                val n = s.queuedLimitReset
                if (confirmOff) CrundiDialog("Turn off window warm-up?", { confirmOff = false },
                    message = (if (n == 1) "1 scheduled message is" else "$n scheduled messages are") + " waiting to send when the limit resets. Warm-up is what notices the reset, so turning it off clears " + (if (n == 1) "it." else "them."),
                    confirm = "Turn off", danger = true, onConfirm = { confirmOff = false; onWarmup(false) })
            }
        }
    }
}

// ─── Configuration (the server's .env) ───

data class EnvField(val key: String, val label: String, val placeholder: String, val hint: String, val secret: Boolean = false, val required: Boolean = false)

val ENV_FIELDS = listOf(
    EnvField("TELEGRAM_BOT_TOKEN", "Telegram bot token", "123456:ABC-DEF...", "From @BotFather. Used for login and notifications.", secret = true),
    EnvField("ALLOWED_USERNAME", "Allowed username", "your-telegram-username", "Only this Telegram user can access Crundi."),
    EnvField("PROJECTS_DIR", "Projects directory", "/home/you/projects", "Single mode: subfolders are found by themselves. Leave empty for multi mode (add projects one by one)."),
    EnvField("WEB_PORT", "Web port", "0 (auto)", "Port for the web UI. 0 picks one."),
    EnvField("CLOUDFLARE_TUNNEL_TOKEN", "Cloudflare tunnel token", "(optional)", "For a lasting custom domain. Leave empty for a random quick tunnel.", secret = true),
    EnvField("CLOUDFLARE_TUNNEL_URL", "Cloudflare tunnel URL", "https://crundi.example.com", "Your public tunnel URL (used with the named tunnel token above)."),
    EnvField("DATA_DIR", "Data directory", "(default: platform app dir)", "Where Crundi keeps its data. Leave empty for the default."),
)

data class ConfigState(val settings: Load<ServerSettings> = Load.Loading, val restartRequired: Boolean = false, val busy: String? = null, val note: Actions.Note? = null)

@Composable
fun ConfigRoute() {
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val actions = rememberActions()
    val settings = rememberLoad { api.system.settings() }
    var restart by remember { mutableStateOf(false) }
    ConfigScreen(
        ConfigState(settings.value, restart, actions.busy, actions.note),
        onSave = { env ->
            actions.run("save", "Settings saved. Restart required for changes to take effect.", settings::reload) {
                restart = api.system.saveSettings(SettingsChange(env = env)).restartRequired
            }
        },
        onRestartPage = { nav.to(SettingsPage.Server.route) },
        onCloseNote = actions::clear,
        onBack = { nav.back() },
    )
}

@Composable
fun ConfigScreen(state: ConfigState, onSave: (Map<String, String>) -> Unit = {}, onRestartPage: () -> Unit = {}, onCloseNote: () -> Unit = {}, onBack: () -> Unit = {}) {
    SettingsPageFrame(SettingsPage.Config, state.note, onCloseNote, onBack) {
        when (val l = state.settings) {
            Load.Loading -> LoadingState()
            is Load.Failed -> ErrorState(l.message)
            is Load.Ready -> {
                val values = remember(l.value) { androidx.compose.runtime.mutableStateMapOf<String, String>().apply { putAll(l.value.settings) } }
                ScrollBody {
                    if (l.value.envPath.isNotEmpty()) Text("Changes are saved to ${l.value.envPath}", style = CrundiType.Meta, modifier = Modifier.padding(horizontal = Dimens.ScreenPad, vertical = 6.dp))
                    if (state.restartRequired) Banner("Saved: restart the server for it to take effect.", Modifier.padding(horizontal = Dimens.ScreenPad, vertical = 6.dp).fillMaxWidth(), BadgeKind.Warn, "alert-triangle")
                    Group("Server configuration") {
                        for ((i, f) in ENV_FIELDS.withIndex()) {
                            if (i > 0) CrundiDivider()
                            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                Text(f.label, style = CrundiType.Body)
                                if (f.required) StatusBadge("required")
                            }
                            CrundiTextField(values[f.key].orEmpty(), { values[f.key] = it }, placeholder = f.placeholder, mono = true, password = f.secret)
                            Text(f.hint, style = CrundiType.Small)
                        }
                    }
                    Row(Modifier.fillMaxWidth().padding(horizontal = Dimens.ScreenPad, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End)) {
                        if (state.restartRequired) CrundiButton("Restart server…", onRestartPage, icon = "rotate-ccw")
                        CrundiButton("Save", { onSave(values.toMap()) }, kind = ButtonKind.Primary, icon = "check", enabled = state.busy != "save")
                    }
                }
            }
        }
    }
}
