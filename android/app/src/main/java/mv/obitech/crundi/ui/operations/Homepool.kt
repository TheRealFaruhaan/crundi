package mv.obitech.crundi.ui.operations

import android.content.Intent
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.delay
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.HomepoolApi
import mv.obitech.crundi.api.model.HomepoolOverview
import mv.obitech.crundi.api.model.HpAnswer
import mv.obitech.crundi.api.model.HpKey
import mv.obitech.crundi.api.model.HpKeyInput
import mv.obitech.crundi.api.model.HpKeyUse
import mv.obitech.crundi.api.model.HpLogEntry
import mv.obitech.crundi.api.model.HpNetwork
import mv.obitech.crundi.api.model.HpNode
import mv.obitech.crundi.api.model.HpNodeSetup
import mv.obitech.crundi.api.model.HpRequest
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiChip
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiDivider
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.ListRow
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.StatusBadge
import mv.obitech.crundi.ui.components.TopBar
import mv.obitech.crundi.ui.components.errorOrUnsupported
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/*
 * Settings > Homepool (capability "homepool"; docs/homepool.md), as the web's
 * Settings > Homepool: this phone's own node at the top, then the exits
 * (networks and their nodes), the access keys, the requests from chats and
 * the log. Owner only. The overview is asked again every 3 s while the
 * screen is in front.
 */

const val HOMEPOOL_UNSUPPORTED = "This server does not support homepool yet. Update the server to use it."

enum class HpTab(val label: String) { Exits("Exits"), Access("Access"), Requests("Requests"), Activity("Activity") }

/** A node key or an access secret just made: shown once, then gone. */
sealed interface HpShown {
    data class Node(val name: String, val setup: HpNodeSetup) : HpShown
    data class Secret(val key: HpKey, val secret: String, val use: HpKeyUse?) : HpShown
}

data class HomepoolState(
    val overview: Load<HomepoolOverview> = Load.Loading,
    /** The server lists the "homepool" capability. */
    val supported: Boolean = true,
    /** Project aliases, for chat and project keys. */
    val projects: List<String> = emptyList(),
    val busy: String? = null,
    val note: Actions.Note? = null,
    val shown: HpShown? = null,
    val now: Long = System.currentTimeMillis(),
)

class HomepoolEvents(
    val onAddNetwork: (String) -> Unit = {},
    val onRenameNetwork: (HpNetwork, String) -> Unit = { _, _ -> },
    val onRemoveNetwork: (HpNetwork) -> Unit = {},
    val onPoolBridge: (Boolean) -> Unit = {},
    val onAddNode: (name: String, network: String) -> Unit = { _, _ -> },
    val onMoveNode: (HpNode, String) -> Unit = { _, _ -> },
    val onNodeEnabled: (HpNode, Boolean) -> Unit = { _, _ -> },
    val onReissueNode: (HpNode) -> Unit = {},
    val onRemoveNode: (HpNode) -> Unit = {},
    val onAddKey: (HpKeyInput) -> Unit = {},
    val onUpdateKey: (HpKey, HpKeyInput) -> Unit = { _, _ -> },
    val onKeyEnabled: (HpKey, Boolean) -> Unit = { _, _ -> },
    val onReissueKey: (HpKey) -> Unit = {},
    val onRemoveKey: (HpKey) -> Unit = {},
    val onApprove: (HpRequest) -> Unit = {},
    val onApproveWith: (HpRequest, HpKeyInput) -> Unit = { _, _ -> },
    val onRefuse: (HpRequest) -> Unit = {},
    val onCloseShown: () -> Unit = {},
    val onCopy: (label: String, text: String) -> Unit = { _, _ -> },
    val onShare: (label: String, text: String) -> Unit = { _, _ -> },
    val onCloseNote: () -> Unit = {},
    val onRetry: () -> Unit = {},
    val onBack: () -> Unit = {},
)

@Composable
fun HomepoolRoute() {
    val app = LocalAppServices.current
    val api = app.api
    val nav = LocalNavigator.current
    val ctx = LocalContext.current
    val clip = LocalClipboardManager.current
    val shell by app.shell.state.collectAsStateWithLifecycle()
    val supported = HomepoolApi.supported(shell.capabilities)
    val actions = rememberActions()
    var overview by remember { mutableStateOf<Load<HomepoolOverview>>(Load.Loading) }
    var shown by remember { mutableStateOf<HpShown?>(null) }
    var tick by remember { mutableStateOf(0) }
    suspend fun fetch() {
        overview = try { Load.Ready(api.homepool.overview()) } catch (e: ApiException) {
            if (overview is Load.Ready) overview else Load.Failed(errorOrUnsupported(e))
        }
    }
    // Every 3 s while the screen is in front (and at once after a retry).
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    LaunchedEffect(supported, tick, lifecycle) {
        if (!supported) return@LaunchedEffect
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) { while (true) { fetch(); delay(3_000) } }
    }
    /** Run a change; its answer carries the overview after it. */
    fun change(key: String, done: String?, block: suspend () -> HpAnswer, then: (HpAnswer) -> Unit = {}) =
        actions.run(key, done) { val a = block(); a.overview?.let { overview = Load.Ready(it) }; then(a) }
    val hp = api.homepool
    HomepoolScreen(
        HomepoolState(overview, supported, shell.projects.map { it.alias }, actions.busy, actions.note, shown, System.currentTimeMillis()),
        HomepoolEvents(
            onAddNetwork = { n -> change("net", "Network $n added", { hp.addNetwork(n) }) },
            onRenameNetwork = { net, n -> change(net.id, "Renamed to $n", { hp.renameNetwork(net.id, n) }) },
            onRemoveNetwork = { net -> change(net.id, "${net.name} removed", { hp.removeNetwork(net.id) }) },
            onPoolBridge = { on -> change("pool", if (on) "Standalone homepool bridged" else "Standalone homepool off", { hp.setPoolBridge(on) }) },
            onAddNode = { name, net -> change("node", null, { hp.addNode(name, net) }) { a -> a.node?.let { shown = HpShown.Node(name, it) } } },
            onMoveNode = { n, net -> change(n.id, "${n.name} moved", { hp.updateNode(n.id, network = net) }) },
            onNodeEnabled = { n, on -> change(n.id, if (on) "${n.name} switched on" else "${n.name} switched off", { hp.updateNode(n.id, enabled = on) }) },
            onReissueNode = { n -> change(n.id, null, { hp.reissueNode(n.id) }) { a -> a.node?.let { shown = HpShown.Node(n.name, it) } } },
            onRemoveNode = { n -> change(n.id, "${n.name} removed", { hp.removeNode(n.id) }) },
            onAddKey = { k -> change("key", null, { hp.addKey(k) }) { a -> a.key?.let { shown = HpShown.Secret(it, a.secret, a.use) } } },
            onUpdateKey = { k, input -> change(k.id, "Access saved", { hp.updateKey(k.id, input) }) },
            onKeyEnabled = { k, on -> change(k.id, if (on) "${k.name} switched on" else "${k.name} switched off", { hp.setKeyEnabled(k.id, on) }) },
            onReissueKey = { k -> change(k.id, null, { hp.reissueKey(k.id) }) { a -> a.key?.let { shown = HpShown.Secret(it, a.secret, a.use) } } },
            onRemoveKey = { k -> change(k.id, "${k.name} revoked", { hp.removeKey(k.id) }) },
            onApprove = { r -> change(r.id, "Approved", { hp.decide(r.id, true) }) },
            onApproveWith = { r, input -> change(r.id, "Approved with changes", { hp.decide(r.id, true, input) }) },
            onRefuse = { r -> change(r.id, "Refused", { hp.decide(r.id, false) }) },
            onCloseShown = { shown = null },
            onCopy = { label, text -> clip.setText(AnnotatedString(text)); actions.note = Actions.Note("$label copied", false) },
            onShare = { label, text ->
                val send = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, text)
                runCatching { ctx.startActivity(Intent.createChooser(send, "Share $label").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
            },
            onCloseNote = actions::clear,
            onRetry = { overview = Load.Loading; tick++ },
            onBack = { nav.back() },
        ),
        share = { ov -> HomepoolShareSection(ov) },
    )
}

@Composable
fun HomepoolScreen(
    state: HomepoolState,
    events: HomepoolEvents = HomepoolEvents(),
    tab: HpTab? = null,
    share: @Composable (HomepoolOverview?) -> Unit = {},
) {
    var current by rememberSaveable { mutableStateOf(tab) }
    Column(Modifier.fillMaxSize()) {
        TopBar("Homepool", onBack = events.onBack, subtitle = state.overview.valueOrNull?.let { ov ->
            val up = ov.nodes.count { it.up && it.enabled }
            "$up of ${ov.nodes.size} node${if (ov.nodes.size == 1) "" else "s"} up · ${ov.keys.size} key${if (ov.keys.size == 1) "" else "s"}"
        })
        NoteBanner(state.note, events.onCloseNote)
        if (!state.supported) {
            EmptyState("network", "Homepool", body = HOMEPOOL_UNSUPPORTED)
            return@Column
        }
        when (val l = state.overview) {
            Load.Loading -> LoadingState(text = "Loading homepool")
            is Load.Failed -> ErrorState(l.message, onRetry = events.onRetry)
            is Load.Ready -> {
                val ov = l.value
                val pending = ov.pending.size
                val t = current ?: if (pending > 0) HpTab.Requests else HpTab.Exits
                Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(vertical = 8.dp)) {
                    share(ov)
                    Segments(
                        HpTab.entries.map { it.name to (if (it == HpTab.Requests && pending > 0) "${it.label} $pending" else it.label) },
                        t.name, { current = HpTab.valueOf(it) },
                        Modifier.fillMaxWidth().padding(horizontal = Dimens.ScreenPad, vertical = 8.dp),
                    )
                    when (t) {
                        HpTab.Exits -> ExitsSection(ov, state, events)
                        HpTab.Access -> AccessSection(ov, state, events)
                        HpTab.Requests -> RequestsSection(ov, state, events)
                        HpTab.Activity -> ActivitySection(ov, state.now)
                    }
                }
            }
        }
    }
    when (val s = state.shown) {
        is HpShown.Node -> NodeKeyDialog(s, events)
        is HpShown.Secret -> SecretDialog(s, events)
        null -> {}
    }
}

// ─── Exits: networks and their nodes ───

@Composable
private fun ExitsSection(ov: HomepoolOverview, state: HomepoolState, events: HomepoolEvents) {
    var addNet by remember { mutableStateOf(false) }
    var addNode by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxWidth().padding(horizontal = Dimens.ScreenPad), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text("Traffic leaves through a network: the server itself, the standalone homepool, or your own networks of home nodes. A key only uses the networks it is given.", style = CrundiType.Small)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiButton("Add node", { addNode = true }, kind = ButtonKind.Primary, icon = "plus", enabled = state.busy != "node")
            CrundiButton("Add network", { addNet = true }, icon = "network", enabled = state.busy != "net")
        }
        HpCard {
            NetHead("server", "Server", "built in", BadgeKind.Neutral)
            Text("The server's own connection. Always there; a key falls back to it only if it is allowed to.", style = CrundiType.Small)
        }
        ov.pool?.let { pool -> PoolCard(pool, state.busy == "pool", events) }
        for (net in ov.own) NetworkCard(net, ov.nodes.filter { it.network == net.id }, ov, state, events)
        val loose = ov.nodes.filter { n -> n.network.isEmpty() || ov.networks.none { it.id == n.network } }
        if (loose.isNotEmpty()) HpCard {
            NetHead("minus", "Unassigned", "${loose.size}", BadgeKind.Neutral)
            Text("These nodes carry nothing until they are moved into a network.", style = CrundiType.Small)
            for (n in loose) { CrundiDivider(); NodeRow(n, ov, state, events) }
        }
        if (ov.nodes.isEmpty()) Text("No nodes yet. Share this phone's connection above, or add a node for a PC, a friend's line or a box running Docker.", style = CrundiType.Small)
    }
    if (addNet) NameDialog("Add a network", "Home, Mum's flat, Office…", "", "Add network", onDismiss = { addNet = false }) { addNet = false; events.onAddNetwork(it) }
    if (addNode) AddNodeDialog(ov, onDismiss = { addNode = false }) { name, net -> addNode = false; events.onAddNode(name, net) }
}

@Composable
private fun NetHead(icon: String, title: String, badge: String?, kind: BadgeKind, trailing: @Composable () -> Unit = {}) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        CrundiIcon(icon, size = Dimens.Icon, tint = CrundiColors.TextSecondary)
        Text(title, style = CrundiType.Heading, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
        if (badge != null) StatusBadge(badge, kind)
        trailing()
    }
}

@Composable
private fun PoolCard(pool: HpNetwork, busy: Boolean, events: HomepoolEvents) {
    val p = pool.pool
    HpCard {
        NetHead("layers", pool.name.ifEmpty { "Standalone homepool" },
            when { !pool.enabled -> "off"; p?.up == true -> "${p.liveCount} up"; else -> "down" },
            when { !pool.enabled -> BadgeKind.Neutral; p?.up == true -> BadgeKind.Good; else -> BadgeKind.Bad }) {
            HpSwitch(pool.enabled, events.onPoolBridge, enabled = !busy)
        }
        Text("The standalone homepool relay as it runs today, bridged in as one network while its nodes move over.", style = CrundiType.Small)
        if (p != null) {
            if (p.nodes.isNotEmpty()) Text("Nodes: " + p.nodes.joinToString(", ") { it.name.ifEmpty { it.id } }, style = CrundiType.Meta)
            if (p.error.isNotEmpty() && pool.enabled) Text("Last check: ${p.error}", style = CrundiType.Small.copy(color = CrundiColors.Yellow))
        }
    }
}

@Composable
private fun NetworkCard(net: HpNetwork, nodes: List<HpNode>, ov: HomepoolOverview, state: HomepoolState, events: HomepoolEvents) {
    var rename by remember { mutableStateOf(false) }
    var remove by remember { mutableStateOf(false) }
    HpCard {
        NetHead("network", net.name, "${net.live} up", if (net.live > 0) BadgeKind.Good else BadgeKind.Neutral) {
            CrundiIconButton("pencil", { rename = true }, contentDescription = "Rename", size = Dimens.IconSm, touch = 36.dp)
            CrundiIconButton("trash", { remove = true }, contentDescription = "Remove", size = Dimens.IconSm, touch = 36.dp, tint = CrundiColors.Red)
        }
        if (nodes.isEmpty()) Text("No nodes in this network yet.", style = CrundiType.Small)
        for (n in nodes) { CrundiDivider(); NodeRow(n, ov, state, events) }
    }
    if (rename) NameDialog("Rename ${net.name}", "Name", net.name, "Rename", onDismiss = { rename = false }) { rename = false; events.onRenameNetwork(net, it) }
    if (remove) CrundiDialog(
        "Remove ${net.name}?", { remove = false },
        message = "Its nodes are kept, unassigned (they carry nothing until moved). Keys simply lose this network.",
        confirm = "Remove network", danger = true, onConfirm = { remove = false; events.onRemoveNetwork(net) },
    )
}

/** up, paused, off or down, as a badge. */
fun nodeState(n: HpNode): Pair<String, BadgeKind> = when {
    !n.enabled -> "off" to BadgeKind.Neutral
    n.up && (n.paused || n.info?.paused == true) -> "paused" to BadgeKind.Warn
    n.up -> "up" to BadgeKind.Good
    else -> "down" to BadgeKind.Neutral
}

@Composable
private fun NodeRow(n: HpNode, ov: HomepoolOverview, state: HomepoolState, events: HomepoolEvents) {
    var move by remember { mutableStateOf(false) }
    var reissue by remember { mutableStateOf(false) }
    var remove by remember { mutableStateOf(false) }
    val busy = state.busy == n.id
    val (word, kind) = nodeState(n)
    val info = n.info
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiIcon(if (n.kind == "crundi-app" || n.platform.contains("android", true)) "smartphone" else "server", size = Dimens.IconSm, tint = CrundiColors.TextSecondary)
            Text(n.name, style = CrundiType.Body, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
            StatusBadge(word, kind)
        }
        val about = listOfNotNull(
            (info?.platform?.ifEmpty { null } ?: n.platform.ifEmpty { null }),
            info?.network?.ifEmpty { null },
            info?.publicIp?.ifEmpty { null },
        )
        if (about.isNotEmpty()) Text(about.joinToString(" · "), style = CrundiType.Meta)
        if (n.up) {
            Text(
                listOf(
                    "connected for ${Fmt.span(state.now - n.connectedAt)}",
                    "${Fmt.bytes(n.usage.bytes)} today",
                    "${n.slots} slot${if (n.slots == 1) "" else "s"}",
                    "${n.streams} stream${if (n.streams == 1) "" else "s"}",
                ).joinToString(" · "),
                style = CrundiType.Small,
            )
            if (info?.paused == true && info.pauseReason.isNotEmpty()) Text("Paused: ${info.pauseReason}", style = CrundiType.Small.copy(color = CrundiColors.Yellow))
        } else Text(if (n.lastSeenAt > 0) "Last seen ${Fmt.ago(n.lastSeenAt, state.now)} · ${Fmt.bytes(n.usage.bytes)} today" else "Never connected", style = CrundiType.Small)
        @OptIn(ExperimentalLayoutApi::class)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(top = 4.dp)) {
            CrundiButton(ov.networkName(n.network), { move = true }, icon = "network", enabled = !busy)
            CrundiButton(if (n.enabled) "Switch off" else "Switch on", { events.onNodeEnabled(n, !n.enabled) }, icon = if (n.enabled) "pause" else "play", enabled = !busy)
            CrundiButton("New key", { reissue = true }, icon = "key", enabled = !busy)
            CrundiButton("Remove", { remove = true }, kind = ButtonKind.Danger, icon = "trash", enabled = !busy)
        }
    }
    if (move) CrundiDialog("Move ${n.name} to…", { move = false }) {
        Column(Modifier.heightIn(max = 360.dp).verticalScroll(rememberScrollState())) {
            ListRow("Unassigned", subtitle = "Carries nothing", icon = "minus", active = n.network.isEmpty(), onClick = { move = false; events.onMoveNode(n, "") })
            for (net in ov.own) ListRow(net.name, icon = "network", active = net.id == n.network, onClick = { move = false; events.onMoveNode(n, net.id) })
        }
    }
    if (reissue) CrundiDialog(
        "New key for ${n.name}?", { reissue = false },
        message = "The node's present key stops working at once and it disconnects. Give the new key to the node to bring it back.",
        confirm = "Make a new key", onConfirm = { reissue = false; events.onReissueNode(n) },
    )
    if (remove) CrundiDialog(
        "Remove ${n.name}?", { remove = false },
        message = "Its key stops working and the slots it carries move to another node, if one is up.",
        confirm = "Remove node", danger = true, onConfirm = { remove = false; events.onRemoveNode(n) },
    )
}

@Composable
private fun AddNodeDialog(ov: HomepoolOverview, onDismiss: () -> Unit, onAdd: (String, String) -> Unit) {
    var name by remember { mutableStateOf("") }
    var net by remember { mutableStateOf(ov.own.firstOrNull { it.id == "home" }?.id ?: ov.own.firstOrNull()?.id.orEmpty()) }
    CrundiDialog(
        "Add a node", onDismiss,
        message = "For a PC, a friend's line, or a box with Docker. Its key is shown once, with a QR code for the Crundi or homepool app and a Docker one-liner.",
        confirm = "Add node", onConfirm = { if (name.isNotBlank()) onAdd(name.trim(), net) },
    ) {
        CrundiTextField(name, { name = it.take(60) }, placeholder = "Name (Mum's PC, office box)")
        Text("Network", style = CrundiType.Small)
        NetworkChoice(ov.own, net, allowNone = true) { net = it }
    }
}

@Composable
fun NetworkChoice(own: List<HpNetwork>, selected: String, allowNone: Boolean, onSelect: (String) -> Unit) {
    @OptIn(ExperimentalLayoutApi::class)
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (allowNone) CrundiChip("Unassigned", selected.isEmpty(), { onSelect("") })
        for (n in own) CrundiChip(n.name, selected == n.id, { onSelect(n.id) }, icon = "network")
    }
}

@Composable
private fun NameDialog(title: String, placeholder: String, initial: String, confirm: String, onDismiss: () -> Unit, onSave: (String) -> Unit) {
    var name by remember { mutableStateOf(initial) }
    CrundiDialog(title, onDismiss, confirm = confirm, onConfirm = { if (name.isNotBlank()) onSave(name.trim()) }) {
        CrundiTextField(name, { name = it.take(60) }, placeholder = placeholder)
    }
}

// ─── Access ───

private fun kindWord(kind: String) = when (kind) { "chat" -> "chat"; "project" -> "project"; else -> "software" }

/** Who a key is for, in words. */
fun keyWho(k: HpKey): String = when (k.kind) {
    "chat" -> "One chat: ${k.chatId}" + if (k.project.isNotEmpty()) " in ${k.project}" else ""
    "project" -> "Every chat of ${k.project}"
    else -> "Software login: user ${k.id}"
}

@Composable
private fun AccessSection(ov: HomepoolOverview, state: HomepoolState, events: HomepoolEvents) {
    var add by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxWidth().padding(horizontal = Dimens.ScreenPad), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text("Nothing uses homepool unless you granted it. A chat or a project's chats use their key through Crundi's tools; software gets a proxy login, or a port block of its own.", style = CrundiType.Small)
        Row { CrundiButton("Add access", { add = true }, kind = ButtonKind.Primary, icon = "plus", enabled = state.busy != "key") }
        if (ov.keys.isEmpty()) Text("No access granted yet.", style = CrundiType.Small)
        for (k in ov.keys) KeyCard(k, ov, state, events)
    }
    if (add) KeyFormDialog("Add access", "Add access", HpKeyInput(), ov, state.projects, lockWho = false, onDismiss = { add = false }) { add = false; events.onAddKey(it) }
}

@Composable
private fun KeyCard(k: HpKey, ov: HomepoolOverview, state: HomepoolState, events: HomepoolEvents) {
    var edit by remember { mutableStateOf(false) }
    var reissue by remember { mutableStateOf(false) }
    var revoke by remember { mutableStateOf(false) }
    var howTo by remember { mutableStateOf(false) }
    val busy = state.busy == k.id
    HpCard(Modifier.alpha(if (k.enabled) 1f else 0.7f)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiIcon(when (k.kind) { "chat" -> "message-square"; "project" -> "folder"; else -> "key" }, size = Dimens.Icon, tint = CrundiColors.TextSecondary)
            Text(k.name.ifEmpty { k.id }, style = CrundiType.Heading, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
            StatusBadge(kindWord(k.kind), BadgeKind.Accent)
            HpSwitch(k.enabled, { events.onKeyEnabled(k, it) }, enabled = !busy)
        }
        Text(keyWho(k), style = CrundiType.Small, maxLines = 2, overflow = TextOverflow.Ellipsis)
        Text("Through " + k.networks.joinToString(", ") { ov.networkName(it) } + if (k.fallbackToServer) ", falling back to the server" else "", style = CrundiType.Small)
        when {
            k.refusal.isNotEmpty() -> Text("Refused: ${k.refusal}", style = CrundiType.Small.copy(color = CrundiColors.Red))
            k.status?.refusal?.isNotEmpty() == true -> Text("Now: ${k.status.refusal}", style = CrundiType.Small.copy(color = CrundiColors.Yellow))
            k.status?.fallbackInUse == true -> Text("Now: leaving from the server (no home exit up)", style = CrundiType.Small.copy(color = CrundiColors.Yellow))
        }
        KeyValue("Today", "${Fmt.bytes(k.usage.bytes)} · ${k.usage.conns} conn${if (k.usage.conns == 1) "" else "s"} · ${k.usage.refused} refused" + if (k.open > 0) " · ${k.open} open" else "")
        KeyValue("Last used", Fmt.ago(k.usage.lastUsedAt, state.now))
        KeyValue("Limits", listOf(
            k.expiresAt?.let { "expires ${Fmt.date(it)}" } ?: "no expiry",
            if (k.dailyCapMb > 0) "${k.dailyCapMb} MB a day" else "no cap",
            "${k.slots} slot${if (k.slots == 1) "" else "s"}",
        ).joinToString(" · "))
        if (k.portBlock) KeyValue("Port block", k.ports?.let { p -> "${p.base}-${p.base + k.slots}, status ${p.status}" } ?: "waiting for ports")
        if (k.portBlock && k.allowFrom.isNotEmpty()) KeyValue("From", k.allowFrom.joinToString(", "))
        Row(
            Modifier.fillMaxWidth().clip(Dimens.ShapeSm).clickable { howTo = !howTo }.padding(vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            CrundiIcon(if (howTo) "chevron-up" else "chevron-down", size = Dimens.IconSm, tint = CrundiColors.TextMuted)
            Text("How to use", style = CrundiType.Small.copy(color = CrundiColors.AccentHover))
        }
        if (howTo) k.use?.let { UseLines(it, events) }
        @OptIn(ExperimentalLayoutApi::class)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiButton("Edit", { edit = true }, icon = "pencil", enabled = !busy)
            CrundiButton("New secret", { reissue = true }, icon = "key", enabled = !busy)
            CrundiButton("Revoke", { revoke = true }, kind = ButtonKind.Danger, icon = "trash", enabled = !busy)
        }
    }
    if (edit) KeyFormDialog("Edit ${k.name}", "Save", HpKeyInput.of(k), ov, state.projects, lockWho = false, onDismiss = { edit = false }) { edit = false; events.onUpdateKey(k, it) }
    if (reissue) CrundiDialog(
        "New secret for ${k.name}?", { reissue = false },
        message = "The present secret stops working at once. The new one is shown once.",
        confirm = "Make a new secret", onConfirm = { reissue = false; events.onReissueKey(k) },
    )
    if (revoke) CrundiDialog(
        "Revoke ${k.name}?", { revoke = false },
        message = "Refused from this moment; its open connections are cut and its ports closed.",
        confirm = "Revoke access", danger = true, onConfirm = { revoke = false; events.onRemoveKey(k) },
    )
}

/** The lines to use a key, each with Copy. */
@Composable
fun UseLines(use: HpKeyUse, events: HomepoolEvents) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        CopyLine("SOCKS5", use.socks, events)
        CopyLine("HTTP proxy", use.http, events)
        CopyLine("Status", use.status, events)
        if (use.login.isNotEmpty()) Text(use.login, style = CrundiType.Small)
        if (use.fromDocker.isNotEmpty()) Text("From a container: ${use.fromDocker}", style = CrundiType.Small)
        use.block?.let { b ->
            CrundiDivider()
            Text("Port block (no login needed)", style = CrundiType.Body)
            CopyLine("Any slot", b.any, events)
            if (b.slots.isNotEmpty()) Text("Slots: ${b.slots}", style = CrundiType.Small)
            CopyLine("Status", b.status, events)
            if (b.from.isNotEmpty()) Text("Only from: ${b.from.joinToString(", ")}", style = CrundiType.Small)
            if (b.compose.isNotEmpty()) CopyLine("Compose lines", b.compose.joinToString("\n"), events)
        }
    }
}

@Composable
fun CopyLine(label: String, text: String, events: HomepoolEvents, share: Boolean = false) {
    if (text.isEmpty()) return
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(label, style = CrundiType.Small, modifier = Modifier.weight(1f))
            if (share) CrundiIconButton("share", { events.onShare(label, text) }, contentDescription = "Share $label", size = Dimens.IconSm, touch = 32.dp)
            CrundiIconButton("copy", { events.onCopy(label, text) }, contentDescription = "Copy $label", size = Dimens.IconSm, touch = 32.dp)
        }
        SelectionContainer {
            Text(
                text, style = CrundiType.Code.copy(fontSize = 12.sp, lineHeight = 17.sp),
                modifier = Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgPrimary).border(Dimens.Hairline, CrundiColors.BorderSubtle, Dimens.ShapeSm).padding(8.dp),
            )
        }
    }
}

private val EXPIRY = listOf("never" to "Never", "1d" to "1 day", "7d" to "1 week", "30d" to "30 days")
private fun expiryMs(choice: String, now: Long): Long? = when (choice) {
    "1d" -> now + 86_400_000L
    "7d" -> now + 7 * 86_400_000L
    "30d" -> now + 30 * 86_400_000L
    else -> null
}

/**
 * The add-access form (also edit, and approve-with-changes). [lockWho]: the
 * chat and project are the request's own and cannot change here.
 */
@Composable
fun KeyForm(initial: HpKeyInput, ov: HomepoolOverview, projects: List<String>, lockWho: Boolean, confirm: String, onCancel: () -> Unit, onSave: (HpKeyInput) -> Unit) {
    var kind by remember { mutableStateOf(initial.kind) }
    var name by remember { mutableStateOf(initial.name) }
    var chatId by remember { mutableStateOf(initial.chatId) }
    var project by remember { mutableStateOf(initial.project) }
    var nets by remember { mutableStateOf(initial.networks.toSet()) }
    var fallback by remember { mutableStateOf(initial.fallbackToServer) }
    var expiry by remember { mutableStateOf(if (initial.expiresAt == null) "never" else "keep") }
    var cap by remember { mutableStateOf(if (initial.dailyCapMb > 0) initial.dailyCapMb.toString() else "") }
    var slots by remember { mutableStateOf(initial.slots.toString()) }
    var block by remember { mutableStateOf(initial.portBlock) }
    var from by remember { mutableStateOf(initial.allowFrom.joinToString(", ")) }
    var error by remember { mutableStateOf<String?>(null) }
    Column(Modifier.heightIn(max = 520.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text("For", style = CrundiType.Small)
        Segments(listOf("chat" to "One chat", "project" to "A project", "software" to "Software"), kind, { if (!lockWho || it != "chat" || chatId.isNotEmpty()) kind = it }, Modifier.fillMaxWidth())
        CrundiTextField(name, { name = it.take(60) }, placeholder = if (kind == "software") "Name (insta-scraper)" else "Name (optional)")
        if (kind != "software") {
            Text("Project", style = CrundiType.Small)
            if (lockWho) Text(project.ifEmpty { "(none)" }, style = CrundiType.Code)
            else {
                @OptIn(ExperimentalLayoutApi::class)
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    for (p in (projects + listOfNotNull(project.ifEmpty { null })).distinct()) CrundiChip(p, project == p, { project = p }, icon = "folder")
                }
            }
        }
        if (kind == "chat") {
            if (lockWho) Text("Chat ${chatId}", style = CrundiType.Meta)
            else CrundiTextField(chatId, { chatId = it.trim().take(80) }, placeholder = "Chat id (CRUNDI_CHAT_ID)", mono = true)
        }
        Text("Networks", style = CrundiType.Small)
        @OptIn(ExperimentalLayoutApi::class)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            for (n in ov.networks) CrundiChip(n.name, n.id in nets, { nets = if (n.id in nets) nets - n.id else nets + n.id }, icon = if (n.id == HomepoolOverview.SERVER) "server" else "network")
        }
        CheckRow("Fall back to the server when no home exit is up", fallback) { fallback = it }
        Text("Expires", style = CrundiType.Small)
        @OptIn(ExperimentalLayoutApi::class)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            if (initial.expiresAt != null) CrundiChip("Keep ${Fmt.date(initial.expiresAt)}", expiry == "keep", { expiry = "keep" })
            for ((id, label) in EXPIRY) CrundiChip(label, expiry == id, { expiry = id })
        }
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Column(Modifier.weight(1f)) {
                Text("Data a day, MB", style = CrundiType.Small)
                CrundiTextField(cap, { cap = it.filter(Char::isDigit).take(7) }, placeholder = "no cap", mono = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number))
            }
            Column(Modifier.weight(1f)) {
                Text("Slots", style = CrundiType.Small)
                CrundiTextField(slots, { slots = it.filter(Char::isDigit).take(2) }, placeholder = "8", mono = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number))
            }
        }
        CheckRow("A port block (for software that cannot send a proxy login)", block) { block = it }
        if (block) {
            CrundiTextField(from, { from = it.take(300) }, placeholder = "From: homepool, 172.20.0.0/16, loopback", mono = true)
            Text("Docker network names, CIDR ranges or loopback, separated by commas. Nothing else may use the block.", style = CrundiType.Small)
        }
        error?.let { Text(it, style = CrundiType.Small.copy(color = CrundiColors.Red)) }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp, Alignment.End)) {
            CrundiButton("Cancel", onCancel)
            CrundiButton(confirm, {
                error = when {
                    nets.isEmpty() -> "Give the key at least one network."
                    kind == "chat" && chatId.isBlank() -> "A chat key needs the chat's id."
                    kind == "project" && project.isBlank() -> "Pick the project."
                    kind == "software" && name.isBlank() -> "Name the software."
                    else -> null
                }
                if (error == null) onSave(
                    HpKeyInput(
                        kind = kind, name = name.trim(), chatId = if (kind == "chat") chatId else "", project = if (kind == "software") "" else project,
                        networks = ov.networks.map { it.id }.filter { it in nets } + nets.filter { id -> ov.networks.none { it.id == id } },
                        fallbackToServer = fallback,
                        expiresAt = if (expiry == "keep") initial.expiresAt else expiryMs(expiry, System.currentTimeMillis()),
                        dailyCapMb = cap.toIntOrNull() ?: 0, slots = (slots.toIntOrNull() ?: 8).coerceIn(0, 32),
                        portBlock = block, allowFrom = if (block) from.split(',').map { it.trim() }.filter { it.isNotEmpty() } else emptyList(),
                    ),
                )
            }, kind = ButtonKind.Primary, icon = "check")
        }
    }
}

@Composable
private fun KeyFormDialog(title: String, confirm: String, initial: HpKeyInput, ov: HomepoolOverview, projects: List<String>, lockWho: Boolean, onDismiss: () -> Unit, onSave: (HpKeyInput) -> Unit) {
    CrundiDialog(title, onDismiss, dismiss = null) { KeyForm(initial, ov, projects, lockWho, confirm, onDismiss, onSave) }
}

// ─── Requests ───

private fun askWords(r: HpRequest) = when (r.kind) {
    "software" -> "a software key \"${r.name.ifEmpty { "software" }}\""
    "project" -> "access for every chat of its project"
    else -> "access for this chat"
}

@Composable
private fun RequestsSection(ov: HomepoolOverview, state: HomepoolState, events: HomepoolEvents) {
    var change by remember { mutableStateOf<HpRequest?>(null) }
    val pending = ov.requests.filter { it.pending }
    val decided = ov.requests.filter { !it.pending }
    Column(Modifier.fillMaxWidth().padding(horizontal = Dimens.ScreenPad), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        if (ov.requests.isEmpty()) Text("No requests. A chat asks with homepool_request; it shows here, and on this phone as a notification.", style = CrundiType.Small)
        for (r in pending + decided) RequestCard(r, ov, state, events) { change = r }
    }
    change?.let { r ->
        KeyFormDialog("Approve with changes", "Approve", HpKeyInput.of(r), ov, state.projects, lockWho = true, onDismiss = { change = null }) { change = null; events.onApproveWith(r, it) }
    }
}

@Composable
private fun RequestCard(r: HpRequest, ov: HomepoolOverview, state: HomepoolState, events: HomepoolEvents, onChange: () -> Unit) {
    val busy = state.busy == r.id
    HpCard(Modifier.alpha(if (r.pending) 1f else 0.55f)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiIcon("message-square", size = Dimens.Icon, tint = CrundiColors.TextSecondary)
            Text(r.project.ifEmpty { "A chat" }, style = CrundiType.Heading, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
            StatusBadge(r.status, when (r.status) { "pending" -> BadgeKind.Warn; "approved" -> BadgeKind.Good; else -> BadgeKind.Bad })
        }
        Text("Asks for ${askWords(r)} through ${r.networks.joinToString(", ") { ov.networkName(it) }}.", style = CrundiType.Body)
        if (r.reason.isNotEmpty()) Text("Why: ${r.reason}", style = CrundiType.Small)
        Text("Chat ${r.chatId} · asked ${Fmt.ago(r.createdAt, state.now)}" + if (!r.pending && r.decidedAt > 0) " · decided ${Fmt.ago(r.decidedAt, state.now)}" else "", style = CrundiType.Meta)
        if (r.pending) {
            @OptIn(ExperimentalLayoutApi::class)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                CrundiButton("Approve", { events.onApprove(r) }, kind = ButtonKind.Primary, icon = "check", enabled = !busy)
                CrundiButton("Change", onChange, icon = "sliders", enabled = !busy)
                CrundiButton("Refuse", { events.onRefuse(r) }, kind = ButtonKind.Danger, icon = "x", enabled = !busy)
            }
        }
    }
}

// ─── Activity ───

/** A log line in words, with names for the ids it carries. */
fun logWords(e: HpLogEntry, ov: HomepoolOverview): String {
    val f = e.fields
    val node = f["node"]?.let { ov.nodeName(it) }
    val key = f["key"]?.let { ov.keyName(it) }
    val net = f["network"]?.let { ov.networkName(it) }
    return when (e.what) {
        "network-added" -> "Network $net added"
        "network-renamed" -> "Network $net renamed"
        "network-removed" -> "Network ${f["network"]} removed"
        "node-added" -> "Node $node added" + if (!f["network"].isNullOrEmpty()) " to $net" else ""
        "node-up" -> "$node connected"
        "node-down" -> "$node disconnected"
        "node-moved" -> "$node moved from ${ov.networkName(f["from"].orEmpty())} to ${ov.networkName(f["to"].orEmpty())}"
        "node-enabled" -> "$node switched on"
        "node-disabled" -> "$node switched off"
        "node-key-reissued" -> "$node given a new key"
        "node-removed" -> "Node ${f["node"]} removed"
        "key-added" -> "Access $key granted" + (f["by"]?.let { " by $it" } ?: "") + (f["networks"]?.takeIf { it.isNotEmpty() }?.let { n -> " through " + n.split(", ").joinToString(", ") { ov.networkName(it) } } ?: "")
        "key-changed" -> "Access $key changed"
        "key-reissued" -> "Access $key given a new secret"
        "key-revoked" -> "Access ${f["key"]} revoked"
        "request" -> "A chat" + (f["project"]?.takeIf { it.isNotEmpty() }?.let { " in $it" } ?: "") + " asked for ${f["kind"] ?: "chat"} access"
        "request-approved" -> "Request approved" + (key?.let { ": $it" } ?: "")
        "request-refused" -> "Request refused"
        "settings-changed" -> "Settings changed"
        "refused-login" -> "Refused a proxy login" + (f["user"]?.let { " as $it" } ?: "") + (f["from"]?.let { " from $it" } ?: "")
        else -> e.what.replace('-', ' ').replaceFirstChar { it.uppercase() } + if (f.isNotEmpty()) " (" + f.entries.joinToString(", ") { "${it.key} ${it.value}" } + ")" else ""
    }
}

@Composable
private fun ActivitySection(ov: HomepoolOverview, now: Long) {
    val entries = ov.entries
    Column(Modifier.fillMaxWidth().padding(horizontal = Dimens.ScreenPad), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        if (entries.isEmpty()) Text("Nothing yet. Every grant and every change is written here.", style = CrundiType.Small)
        else HpCard {
            for ((i, e) in entries.withIndex()) {
                if (i > 0) CrundiDivider()
                Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text(Fmt.date(e.at), style = CrundiType.Meta, modifier = Modifier.padding(top = 2.dp))
                    Text(logWords(e, ov), style = CrundiType.Small.copy(color = if (e.what == "refused-login") CrundiColors.Yellow else CrundiColors.TextPrimary), modifier = Modifier.weight(1f))
                }
            }
        }
    }
}

// ─── Shown once ───

@Composable
private fun NodeKeyDialog(s: HpShown.Node, events: HomepoolEvents) {
    CrundiDialog("Key for ${s.name}", events.onCloseShown, message = "Shown once. Scan it with the Crundi or homepool app, or run the Docker line on the box. A new key can be made later; the old one then stops working.", confirm = "Done", onConfirm = events.onCloseShown, dismiss = null) {
        Column(Modifier.heightIn(max = 520.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            if (s.setup.code.isNotEmpty()) {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Center) { QrCode(s.setup.code, size = 200.dp, description = "The node's code as a QR code") }
                CopyLine("Code", s.setup.code, events, share = true)
            } else {
                Text("The server has no public address set, so there is no code: give the node the server's address and this key.", style = CrundiType.Small.copy(color = CrundiColors.Yellow))
                CopyLine("Node key", s.setup.key, events, share = true)
            }
            CopyLine("Docker", s.setup.docker, events, share = true)
        }
    }
}

@Composable
private fun SecretDialog(s: HpShown.Secret, events: HomepoolEvents) {
    CrundiDialog("Secret for ${s.key.name.ifEmpty { s.key.id }}", events.onCloseShown, message = "Shown once: copy it now. Keep it with the software (a secret, its .env), not in a file you commit.", confirm = "Done", onConfirm = events.onCloseShown, dismiss = null) {
        Column(Modifier.heightIn(max = 520.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            CopyLine("Secret", s.secret, events)
            s.use?.let { UseLines(it, events) }
        }
    }
}

// ─── Pieces ───

@Composable
fun HpCard(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Column(
        modifier.fillMaxWidth().clip(Dimens.Shape).background(CrundiColors.BgCard).border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape).padding(horizontal = 14.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp), content = content,
    )
}

@Composable
fun HpSwitch(checked: Boolean, onChange: (Boolean) -> Unit, enabled: Boolean = true) {
    Switch(
        checked = checked, onCheckedChange = onChange, enabled = enabled,
        colors = SwitchDefaults.colors(
            checkedThumbColor = CrundiColors.TextPrimary, checkedTrackColor = CrundiColors.Accent, checkedBorderColor = CrundiColors.Accent,
            uncheckedThumbColor = CrundiColors.TextSecondary, uncheckedTrackColor = CrundiColors.BgTertiary, uncheckedBorderColor = CrundiColors.Border,
        ),
    )
}
