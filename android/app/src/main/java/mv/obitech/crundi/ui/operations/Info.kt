package mv.obitech.crundi.ui.operations

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.delay
import mv.obitech.crundi.api.model.CollabInvite
import mv.obitech.crundi.api.model.Collaborators
import mv.obitech.crundi.api.model.MaintenanceTask
import mv.obitech.crundi.api.model.Project
import mv.obitech.crundi.api.model.ServerLog
import mv.obitech.crundi.api.model.Stats
import mv.obitech.crundi.api.model.Tunnels
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiChip
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiDivider
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.DialogCard
import mv.obitech.crundi.ui.components.PanelHeader
import mv.obitech.crundi.ui.components.StatusBadge
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.shell.Area
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.shell.LocalHost
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/*
 * The Info tab, as the web's renderInfo: the project and its status (with a
 * project), this machine, reclaim disk space, outside collaborators, the
 * tunnel, server logs; and Forwards, which the phone reaches from here. Each
 * section loads and fails on its own, so one that cannot be read never hides
 * the rest.
 */

/** The project block: what the web shows under Project and Status. */
data class InfoProject(
    val project: Project,
    /** A Claude terminal or chat is open in it (not parked). */
    val active: Boolean,
    val services: Int,
    val running: Int,
)

/** The passcode, shown once after creating access or asking for a new one. */
data class PasscodeShown(val name: String, val passcode: String, val projects: List<String> = emptyList(), val endsAt: Long = 0, val title: String? = null)

data class InfoState(
    val project: InfoProject? = null,
    val stats: Load<Stats> = Load.Loading,
    val tasks: Load<List<MaintenanceTask>> = Load.Loading,
    val collab: Load<Collaborators> = Load.Loading,
    val tunnel: Load<Tunnels> = Load.Loading,
    val logs: Load<List<ServerLog>> = Load.Loading,
    val forwards: Int? = null,
    /** Every project, for the invite form. */
    val projects: List<Project> = emptyList(),
    /** Where a collaborator signs in (this server). */
    val signInUrl: String = "",
    val passcode: PasscodeShown? = null,
    val busy: String? = null,
    val note: Actions.Note? = null,
    val now: Long = System.currentTimeMillis(),
    /** Open the invite form (screenshots). */
    val inviting: Boolean = false,
)

/** A new invitation, as the web's invite form sends it. */
data class CollabRequest(val name: String, val telegram: String, val projects: List<String>, val hours: Int, val tokenLimit: Long)

class InfoEvents(
    val onRunTask: (MaintenanceTask) -> Unit = {},
    val onInvite: (CollabRequest) -> Unit = {},
    val onSandboxSetup: () -> Unit = {},
    val onReissue: (CollabInvite) -> Unit = {},
    val onRevoke: (CollabInvite) -> Unit = {},
    val onRestore: (CollabInvite) -> Unit = {},
    val onRemove: (CollabInvite) -> Unit = {},
    val onClosePasscode: () -> Unit = {},
    val onCopy: (String) -> Unit = {},
    val onRetryCollab: () -> Unit = {},
    val onOpenUrl: (String) -> Unit = {},
    val onLogs: () -> Unit = {},
    val onForwards: () -> Unit = {},
    val onCloseNote: () -> Unit = {},
)

// ─── The web's helpers (collabTimeLeft, fmtTok, parseTok) ───

object CollabFmt {
    /** "3 days left", "5 hours left", "12 minutes left", "expired". */
    fun timeLeft(expiresAt: Long, now: Long): String {
        val left = expiresAt - now
        if (left <= 0) return "expired"
        val d = left / 86_400_000
        if (d >= 1) return "$d ${if (d == 1L) "day" else "days"} left"
        val h = left / 3_600_000
        if (h >= 1) return "$h ${if (h == 1L) "hour" else "hours"} left"
        val m = maxOf(1L, Math.round(left / 60_000.0))
        return "$m ${if (m == 1L) "minute" else "minutes"} left"
    }

    /** 2400000 -> "2.4M", 412000 -> "412k". */
    fun tokens(n: Long): String = when {
        n >= 1_000_000 -> (Math.round(n / 100_000.0) / 10.0).let { if (it == Math.floor(it)) it.toLong().toString() else it.toString() } + "M"
        n >= 1_000 -> "${Math.round(n / 1000.0)}k"
        else -> n.toString()
    }

    /** "2M", "500k", "1,500,000" -> tokens; null when it is not a number. */
    fun parseTokens(raw: String): Long? {
        val m = Regex("^(\\d+(?:\\.\\d+)?)\\s*([km]?)$").find(raw.trim().lowercase().replace(",", "")) ?: return null
        val n = m.groupValues[1].toDouble() * when (m.groupValues[2]) { "m" -> 1e6; "k" -> 1e3; else -> 1.0 }
        return Math.round(n)
    }

    /** One person per Telegram name, or per name for passcode-only access, as the web groups them. */
    fun people(list: List<CollabInvite>): List<List<CollabInvite>> =
        list.groupBy { if (it.telegram.isNotEmpty()) "tg:" + it.telegram else "pc:" + it.name.lowercase() }.values.toList()
}

// ─── Route ───

@Composable
fun InfoRoute() {
    val app = LocalAppServices.current
    val api = app.api
    val nav = LocalNavigator.current
    val host = LocalHost.current
    val clip = LocalClipboardManager.current
    val shell by app.shell.state.collectAsStateWithLifecycle()
    val live by api.events.state.collectAsStateWithLifecycle(initialValue = null)
    val actions = rememberActions()
    val alias = shell.currentProject
    val stats = rememberLoad { api.system.stats() }
    val tasks = rememberLoad { api.system.maintenance() }
    val collab = rememberLoad { api.collaborators.list() }
    val tunnel = rememberLoad { api.services.tunnels() }
    val forwards = rememberLoad { api.forwards.list() }
    val backlog = rememberLoad { api.system.logs() }
    val liveLogs = remember { mutableStateListOf<ServerLog>() }
    var passcode by remember { mutableStateOf<PasscodeShown?>(null) }
    // The desktop polls the machine every 2 s while the tab is open.
    LaunchedEffect(Unit) { while (true) { delay(2000); stats.reload() } }
    LaunchedEffect(Unit) { api.serverLogs().collect { liveLogs += it; if (liveLogs.size > 500) liveLogs.removeRange(0, liveLogs.size - 500) } }

    val st = live
    val project = alias?.let { a -> st?.projects?.firstOrNull { it.alias.equals(a, true) } }?.let { p ->
        val svcs = st!!.servicesOf(p.alias)
        InfoProject(p, st.terminals.any { it.project.equals(p.alias, true) && !it.isParked }, svcs.size, svcs.count { it.status == "running" })
    }
    val server = shell.serverHost.let { if (it.isEmpty() || it.startsWith("http")) it else "https://$it" }
    InfoScreen(
        InfoState(
            project = project,
            stats = stats.value,
            tasks = tasks.value,
            collab = collab.value,
            tunnel = tunnel.value,
            logs = when (val b = backlog.value) { is Load.Ready -> Load.Ready(b.value + liveLogs); else -> b },
            forwards = forwards.value.valueOrNull?.forwards?.size,
            projects = st?.projects.orEmpty(),
            signInUrl = server,
            passcode = passcode,
            busy = actions.busy,
            note = actions.note,
        ),
        InfoEvents(
            onRunTask = { t -> actions.run(t.id, after = tasks::reload) { val freed = api.system.runMaintenance(t.id); actions.note = Actions.Note(if (freed > 0) "Freed ${Fmt.bytes(freed)}" else "Done.", false) } },
            onInvite = { r ->
                actions.run("invite", after = collab::reload) {
                    val made = api.collaborators.create(r.name, r.projects, hours = r.hours, telegram = r.telegram.ifEmpty { null }, tokenLimit = r.tokenLimit.takeIf { it > 0 })
                    if (made.passcode.isNotEmpty()) passcode = PasscodeShown(r.name, made.passcode, made.collaborators.map { it.project }, made.collaborators.firstOrNull()?.expiresAt ?: 0)
                    else actions.note = Actions.Note("Added to their existing access. Their passcode is unchanged.", false)
                }
            },
            onSandboxSetup = { actions.run("sandbox", "Sandbox ready", collab::reload) { api.collaborators.setupSandbox() } },
            onReissue = { c ->
                actions.run(c.id, after = collab::reload) {
                    val code = api.collaborators.reissue(c.id)
                    passcode = PasscodeShown(c.name, code, title = "New passcode for ${c.name}")
                }
            },
            onRevoke = { c -> actions.run(c.id, "Access for ${c.name} to ${c.project} revoked", collab::reload) { api.collaborators.revoke(c.id) } },
            onRestore = { c -> actions.run(c.id, "Access for ${c.name} to ${c.project} restored", collab::reload) { api.collaborators.update(c.id, mapOf("revoked" to false)) } },
            onRemove = { c -> actions.run(c.id, "Removed ${c.name} from ${c.project}", collab::reload) { api.collaborators.remove(c.id) } },
            onClosePasscode = { passcode = null },
            onCopy = { t -> clip.setText(AnnotatedString(t)); actions.note = Actions.Note("Copied", false) },
            onRetryCollab = collab::reload,
            onOpenUrl = host::openExternal,
            onLogs = { nav.to(SettingsPage.Logs.route) },
            onForwards = { nav.to(Area.Forwards.key) },
            onCloseNote = actions::clear,
        ),
    )
}

// ─── Screen ───

@Composable
fun InfoScreen(state: InfoState, events: InfoEvents = InfoEvents()) {
    var inviting by remember { mutableStateOf(state.inviting) }
    Column(Modifier.fillMaxSize()) {
        PanelHeader("Info", meta = state.project?.project?.label ?: "This server")
        NoteBanner(state.note, events.onCloseNote)
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(vertical = 4.dp)) {
            state.project?.let { p ->
                Group("Project") {
                    KeyValue("Alias", p.project.alias)
                    KeyValue("Name", p.project.label)
                    KeyValue("Path", p.project.path.ifEmpty { "—" })
                }
                Group("Status") {
                    KeyValue("Claude Terminal", if (p.active) "Active" else "Not started", valueColor = if (p.active) CrundiColors.Green else CrundiColors.TextPrimary)
                    KeyValue("Services", "${p.services} registered, ${p.running} running")
                }
            }
            MachineGroup(state.stats)
            ReclaimGroup(state.tasks, state.busy, events.onRunTask)
            CollaboratorsGroup(state, events, onInvite = { inviting = true })
            TunnelGroup(state.tunnel, events.onOpenUrl)
            Group("Forwards") {
                Text(
                    "A forward reaches a port on this machine through Crundi, by name: for a container, or for anything else listening here.",
                    style = CrundiType.Small,
                )
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(state.forwards?.let { "$it forward${if (it == 1) "" else "s"}" } ?: "—", style = CrundiType.Body, modifier = Modifier.weight(1f))
                    CrundiButton("Open forwards", events.onForwards, icon = "network")
                }
            }
            LogsGroup(state.logs, events)
        }
    }
    if (inviting) CrundiDialog("Invite someone", { inviting = false }, dismiss = null) {
        InviteForm(state.projects, state.project?.project?.alias, sandboxOk = state.collab.valueOrNull?.sandbox?.ok == true, now = state.now,
            onCancel = { inviting = false }, onCreate = { inviting = false; events.onInvite(it) })
    }
    state.passcode?.let { PasscodeDialog(it, state.signInUrl, events) }
}

@Composable
private fun TunnelGroup(load: Load<Tunnels>, onOpen: (String) -> Unit) {
    Group("Tunnel") {
        when (load) {
            Load.Loading -> Text("Loading…", style = CrundiType.Small)
            is Load.Failed -> Text(load.message, style = CrundiType.Small.copy(color = CrundiColors.Red))
            is Load.Ready -> {
                val t = load.value
                val active = t.tunnels.values.count { it.status == "active" }
                val url = t.tunnelUrl?.ifEmpty { null }
                Row(Modifier.fillMaxWidth().padding(vertical = 3.dp)) {
                    Text("URL", style = CrundiType.Small, modifier = Modifier.weight(0.38f))
                    Text(
                        url ?: "—",
                        style = CrundiType.Code.copy(fontSize = CrundiType.Small.fontSize, color = if (url != null) CrundiColors.Accent else CrundiColors.TextPrimary),
                        modifier = Modifier.weight(0.62f).let { m -> if (url != null) m.clickable { onOpen(url) } else m },
                    )
                }
                KeyValue("Status", if (active > 0) "Connected" else if (t.tunnels.isNotEmpty()) "Connecting..." else "Not configured")
                if (t.named && t.target.isNotEmpty()) KeyValue("Cloudflare service", "HTTP → ${t.target}")
            }
        }
    }
}

@Composable
private fun LogsGroup(load: Load<List<ServerLog>>, events: InfoEvents) {
    Column(Modifier.fillMaxWidth().padding(horizontal = Dimens.ScreenPad, vertical = 8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("SERVER LOGS", style = CrundiType.Section, modifier = Modifier.weight(1f))
            CrundiIconButton("copy", { load.valueOrNull?.let { l -> events.onCopy(l.joinToString("\n") { it.text }) } }, contentDescription = "Copy logs", size = Dimens.IconSm, touch = 36.dp)
            CrundiIconButton("maximize", events.onLogs, contentDescription = "Open server logs", size = Dimens.IconSm, touch = 36.dp)
        }
        Column(
            Modifier.fillMaxWidth().heightIn(max = 300.dp).clip(Dimens.Shape).background(CrundiColors.BgPrimary)
                .border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape).clickable(onClick = events.onLogs).padding(8.dp),
        ) {
            when (load) {
                Load.Loading -> Text("(loading...)", style = CrundiType.Code.copy(fontSize = 11.5.sp, color = CrundiColors.TextMuted))
                is Load.Failed -> Text(load.message, style = CrundiType.Small.copy(color = CrundiColors.Red))
                is Load.Ready -> if (load.value.isEmpty()) Text("No log lines yet.", style = CrundiType.Small)
                else for (line in load.value.takeLast(14)) Text(
                    line.text,
                    maxLines = 2,
                    style = CrundiType.Code.copy(fontSize = 11.5.sp, lineHeight = 16.sp, color = when (line.level) {
                        "error" -> CrundiColors.Red
                        "warn" -> CrundiColors.Yellow
                        else -> CrundiColors.TextMuted
                    }),
                )
            }
        }
    }
}

// ─── Outside collaborators ───

@Composable
private fun CollaboratorsGroup(state: InfoState, events: InfoEvents, onInvite: () -> Unit) {
    Group("Outside collaborators") {
        when (val l = state.collab) {
            Load.Loading -> Text("Loading…", style = CrundiType.Small)
            is Load.Failed -> {
                Text("Could not load collaborators. ${l.message}", style = CrundiType.Small.copy(color = CrundiColors.Red))
                CrundiButton("Try again", events.onRetryCollab, icon = "refresh")
            }
            is Load.Ready -> {
                val d = l.value
                SandboxNote(d, state.busy == "sandbox", events.onSandboxSetup)
                val people = CollabFmt.people(d.collaborators)
                if (people.isEmpty()) Text("Nobody outside has access right now.", style = CrundiType.Small)
                for (rows in people) { CrundiDivider(); Person(rows, state, events) }
                CrundiDivider()
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text("They see only the projects you pick, and nothing else on this machine.", style = CrundiType.Small, modifier = Modifier.weight(1f))
                    CrundiButton("Invite someone", onInvite, kind = ButtonKind.Primary, icon = "plus", enabled = d.sandbox.ok && state.busy != "invite")
                }
            }
        }
    }
}

@Composable
private fun SandboxNote(d: Collaborators, busy: Boolean, onSetup: () -> Unit) {
    val sb = d.sandbox
    val ok = sb.ok
    Column(
        Modifier.fillMaxWidth().clip(Dimens.ShapeSm)
            .background(if (ok) CrundiColors.GreenDim else CrundiColors.YellowDim)
            .padding(10.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        if (ok) Text("Sandbox ready. Collaborators’ shells are confined to their own worktree.", style = CrundiType.Small.copy(color = CrundiColors.Green))
        else {
            Text("Sandbox not set up. Invites are disabled and collaborator chats will not start until it is.", style = CrundiType.Small.copy(color = CrundiColors.Yellow, fontWeight = FontWeight.SemiBold))
            for (p in sb.problems.ifEmpty { listOf("Could not check the sandbox") }) Text("• $p", style = CrundiType.Small)
            if (sb.canAutoFix) CrundiButton(if (busy) "Setting up…" else "Set up sandbox", onSetup, enabled = !busy, icon = "shield")
            else Text("On the server run: sudo apt-get install -y bubblewrap socat", style = CrundiType.Code.copy(fontSize = CrundiType.Small.fontSize))
        }
    }
}

@Composable
private fun Person(rows: List<CollabInvite>, state: InfoState, events: InfoEvents) {
    val first = rows.first()
    val used = rows.sumOf { it.tokensUsed }
    val limit = rows.maxOf { it.tokenLimit }
    val over = limit > 0 && used >= limit
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiIcon("user", size = Dimens.IconSm, tint = CrundiColors.TextSecondary)
            Column(Modifier.weight(1f)) {
                Text(first.name, style = CrundiType.Body.copy(fontWeight = FontWeight.SemiBold))
                Text(if (first.telegram.isNotEmpty()) "@${first.telegram}" else "passcode only", style = CrundiType.Meta)
            }
            CrundiButton("New passcode", { events.onReissue(first) }, enabled = state.busy != first.id)
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(
                "${CollabFmt.tokens(used)} tokens" + (if (limit > 0) " of ${CollabFmt.tokens(limit)}" else " · no limit") + (if (over) " · limit reached" else ""),
                style = CrundiType.Meta.copy(color = if (over) CrundiColors.Red else CrundiColors.TextMuted),
            )
            if (limit > 0) Meter((used.toFloat() / limit).coerceIn(0f, 1f), Modifier.weight(1f), color = if (over) CrundiColors.Red else CrundiColors.Accent)
        }
        val sites = rows.flatMap { it.allowedDomains }.distinct().sorted()
        if (sites.isNotEmpty()) Text("Allowed sites: " + sites.joinToString(", "), style = CrundiType.Meta)
        for (r in rows) InviteRow(r, state, events)
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun InviteRow(r: CollabInvite, state: InfoState, events: InfoEvents) {
    var confirm by remember { mutableStateOf<String?>(null) }
    val gone = r.expired || r.revoked
    val busy = state.busy == r.id
    Column(
        Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgTertiary).padding(10.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiIcon("folder", size = Dimens.IconSm, tint = if (gone) CrundiColors.TextMuted else CrundiColors.TextSecondary)
            Column(Modifier.weight(1f)) {
                Text(r.project, style = CrundiType.Body.copy(color = if (gone) CrundiColors.TextMuted else CrundiColors.TextPrimary))
                if (r.branch.isNotEmpty()) Text(r.branch, style = CrundiType.Code.copy(fontSize = CrundiType.Meta.fontSize, color = CrundiColors.TextMuted))
            }
            StatusBadge(
                if (r.revoked) "revoked" else CollabFmt.timeLeft(r.expiresAt, state.now),
                if (r.revoked || r.expired) BadgeKind.Bad else BadgeKind.Neutral,
            )
        }
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            if (r.revoked) CrundiButton("Restore", { events.onRestore(r) }, enabled = !busy, icon = "rotate-ccw")
            else CrundiButton("Revoke", { confirm = "revoke" }, enabled = !busy, icon = "ban")
            CrundiButton("Remove", { confirm = "remove" }, kind = ButtonKind.Danger, enabled = !busy, icon = "trash")
        }
    }
    when (confirm) {
        "revoke" -> CrundiDialog("Revoke access?", { confirm = null }, message = "${r.name} loses access to ${r.project} now, and their chats there close. You can restore it later.",
            confirm = "Revoke", danger = true, onConfirm = { confirm = null; events.onRevoke(r) })
        "remove" -> CrundiDialog("Remove ${r.name} from ${r.project}?", { confirm = null }, message = "This ends their access and deletes their worktree. Anything they did not merge is lost.",
            confirm = "Remove", danger = true, onConfirm = { confirm = null; events.onRemove(r) })
    }
}

/** The web's invite form: every field labelled, the end of access said as a date. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun InviteForm(projects: List<Project>, current: String?, sandboxOk: Boolean, now: Long, onCancel: () -> Unit, onCreate: (CollabRequest) -> Unit) {
    var name by remember { mutableStateOf("") }
    var telegram by remember { mutableStateOf("") }
    var picked by remember { mutableStateOf(setOfNotNull(current)) }
    var amount by remember { mutableStateOf("7") }
    var hoursUnit by remember { mutableStateOf(false) }
    var limit by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Label("Their name")
        CrundiTextField(name, { name = it.take(40) }, placeholder = "e.g. Sam")
        Hint("They type this to sign in.")
        Label("Telegram username", "optional")
        CrundiTextField(telegram, { telegram = it.removePrefix("@").take(40) }, placeholder = "without the @")
        Hint("Lets them use the Telegram button instead of a passcode.")
        Label("Projects", "pick one or more")
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            for (p in projects) CrundiChip(p.label, selected = p.alias in picked, onClick = { picked = if (p.alias in picked) picked - p.alias else picked + p.alias }, icon = "folder")
        }
        Hint("Each gets its own git worktree and branch.")
        Label("Access lasts")
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiTextField(amount, { amount = it.filter(Char::isDigit).take(4) }, Modifier.width(90.dp), mono = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number))
            CrundiChip("days", selected = !hoursUnit, onClick = { hoursUnit = false })
            CrundiChip("hours", selected = hoursUnit, onClick = { hoursUnit = true })
        }
        val n = amount.toIntOrNull() ?: 0
        val hours = if (hoursUnit) n else n * 24
        Hint(when {
            n < 1 -> "Enter how long, at least 1 ${if (hoursUnit) "hour" else "day"}."
            hours > 8760 -> "That is over a year. The most is 365 days."
            else -> "Ends ${endsText(now + hours * 3_600_000L, hours < 48)}. You can extend or revoke it at any time."
        })
        Label("Token limit", "optional")
        CrundiTextField(limit, { limit = it.take(16) }, placeholder = "e.g. 2M or 500k, blank for no limit", mono = true)
        Hint("Counted across all their projects. Their chat stops the moment it is reached, and you can change it later.")
        if (!sandboxOk) Text("Set up the sandbox first.", style = CrundiType.Small.copy(color = CrundiColors.Yellow))
        error?.let { Text(it, style = CrundiType.Small.copy(color = CrundiColors.Red)) }
        Row(Modifier.fillMaxWidth().padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(12.dp, Alignment.End)) {
            CrundiButton("Cancel", onCancel)
            CrundiButton("Create access", {
                val tok = if (limit.isBlank()) 0L else CollabFmt.parseTokens(limit)
                error = when {
                    name.isBlank() -> "Give them a name"
                    picked.isEmpty() -> "Pick at least one project"
                    n < 1 || hours > 8760 -> "Access lasts: at least 1 hour, at most 365 days"
                    tok == null -> "Token limit: try something like 2M or 500k"
                    else -> null
                }
                if (error == null) onCreate(CollabRequest(name.trim(), telegram.trim(), projects.map { it.alias }.filter { it in picked }, hours, tok ?: 0))
            }, kind = ButtonKind.Primary, icon = "check", enabled = sandboxOk)
        }
    }
}

private fun endsText(at: Long, withTime: Boolean): String {
    val z = java.time.Instant.ofEpochMilli(at).atZone(java.time.ZoneId.systemDefault())
    val f = if (withTime) java.time.format.DateTimeFormatter.ofPattern("EEE HH:mm", java.util.Locale.ENGLISH)
    else java.time.format.DateTimeFormatter.ofPattern("EEE d MMM", java.util.Locale.ENGLISH)
    return f.format(z)
}

@Composable private fun Label(text: String, optional: String? = null) {
    Row(Modifier.padding(top = 4.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(text, style = CrundiType.Small.copy(color = CrundiColors.TextSecondary, fontWeight = FontWeight.SemiBold))
        if (optional != null) Text(optional, style = CrundiType.Meta)
    }
}

@Composable private fun Hint(text: String) = Text(text, style = CrundiType.Meta)

/** The web's invite modal: the three things to send, the passcode shown only here. */
@Composable
fun PasscodeCard(p: PasscodeShown, signInUrl: String, events: InfoEvents, modifier: Modifier = Modifier) {
    DialogCard(p.title ?: "Access created for ${p.name}", modifier, message = "Send these three things to them however you like.", confirm = "Done", onConfirm = events.onClosePasscode, dismiss = null) {
        CopyField("Sign in at", signInUrl, false, events.onCopy)
        CopyField("Name", p.name, false, events.onCopy)
        CopyField("Passcode", p.passcode, true, events.onCopy)
        Text(
            "The passcode is shown only here. It is stored hashed, so it cannot be looked up later, only replaced with a new one.",
            style = CrundiType.Small.copy(color = CrundiColors.Yellow),
        )
        if (p.projects.isNotEmpty()) Text("Projects: " + p.projects.joinToString(", "), style = CrundiType.Small)
        if (p.endsAt > 0) Text("Access ends: " + endsText(p.endsAt, false), style = CrundiType.Small)
        CrundiButton("Copy all", {
            events.onCopy(listOfNotNull(
                "Crundi access for ${p.name}", "", "Sign in: $signInUrl", "Name: ${p.name}", "Passcode: ${p.passcode}",
                p.projects.takeIf { it.isNotEmpty() }?.let { "Projects: " + it.joinToString(", ") },
                "", "Open the link, choose \"I was invited to a project\", and use the name and passcode above.",
            ).joinToString("\n"))
        }, icon = "copy")
    }
}

@Composable
private fun PasscodeDialog(p: PasscodeShown, signInUrl: String, events: InfoEvents) {
    androidx.compose.ui.window.Dialog(onDismissRequest = events.onClosePasscode) { PasscodeCard(p, signInUrl, events, Modifier.padding(16.dp)) }
}

@Composable
private fun CopyField(label: String, value: String, big: Boolean, onCopy: (String) -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
        Text(label, style = CrundiType.Meta)
        Row(
            Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgPrimary).border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm).padding(start = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(value, style = CrundiType.Code.copy(fontSize = if (big) 18.sp else CrundiType.Small.fontSize, fontWeight = if (big) FontWeight.Bold else FontWeight.Normal), modifier = Modifier.weight(1f))
            CrundiIconButton("copy", { onCopy(value) }, contentDescription = "Copy $label", size = Dimens.IconSm, touch = 40.dp)
        }
    }
}
