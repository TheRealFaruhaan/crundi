package mv.obitech.crundi.ui.operations

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.PowerManager
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.launch
import mv.obitech.crundi.Servers
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.model.HomepoolOverview
import mv.obitech.crundi.api.model.HpNetwork
import mv.obitech.crundi.homepool.Homepool
import mv.obitech.crundi.homepool.NodeConfig
import mv.obitech.crundi.homepool.NodePhase
import mv.obitech.crundi.homepool.NodeSnapshot
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiDivider
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.errorText
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.shell.LocalHost
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/*
 * "Share this phone's connection" (docs/homepool.md, Android): this phone as
 * a home node of the server it is paired with. The node itself (foreground
 * service, notification, network rules) is the core in mv.obitech.crundi.homepool;
 * this is its face on Settings > Homepool, and the one-line status on the
 * Settings front page.
 */

/** The colour of a node's status: Connected green, Connecting blue, Paused amber, Refused red, Stopped grey. */
fun phaseColor(p: NodePhase): Color = when (p) {
    NodePhase.CONNECTED -> CrundiColors.Green
    NodePhase.CONNECTING -> CrundiColors.Sky
    NodePhase.PAUSED -> CrundiColors.Yellow
    NodePhase.REFUSED -> CrundiColors.Red
    NodePhase.STOPPED -> CrundiColors.TextMuted
}

/** The status in words, as the node's notification says it. */
fun phaseWords(s: NodeSnapshot): String = when (s.phase) {
    NodePhase.CONNECTED -> "Connected"
    NodePhase.CONNECTING -> "Connecting…"
    NodePhase.PAUSED -> "Paused" + (s.pauseReason?.takeIf { it.isNotBlank() }?.let { ": $it" } ?: "")
    NodePhase.STOPPED -> "Stopped"
    NodePhase.REFUSED -> "Refused: the server refused this node key"
}

/** "Sharing: Connected, 120 MB today" for the Settings front page; null when this phone is not a node. */
fun homepoolStatusLine(s: NodeSnapshot): String? {
    if (!s.configured) return null
    val state = when (s.phase) {
        NodePhase.PAUSED -> "Paused" + (s.pauseReason?.takeIf { it.isNotBlank() }?.let { " ($it)" } ?: "")
        NodePhase.REFUSED -> "Refused by the server"
        else -> phaseWords(s)
    }
    return "Sharing this phone: $state, ${Fmt.bytes(s.bytesToday)} today"
}

data class ShareState(
    val snapshot: NodeSnapshot = NodeSnapshot.EMPTY,
    /** This phone has a node key (Homepool.config). */
    val configured: Boolean = snapshot.configured,
    val notificationsOk: Boolean = true,
    val batteryOk: Boolean = true,
    val busy: Boolean = false,
    val error: String? = null,
    /** The server this phone's node belongs to, when it is not the one on screen. */
    val otherServer: String? = null,
    val now: Long = System.currentTimeMillis(),
)

class ShareEvents(
    /** Make this phone a node, in [network] ("" unassigned). Also "Set up again" after a refusal. */
    val onSetUp: (network: String) -> Unit = {},
    val onStart: () -> Unit = {},
    val onStop: () -> Unit = {},
    val onAutoStart: (Boolean) -> Unit = {},
    val onWifiOnly: (Boolean) -> Unit = {},
    val onMobileCap: (Int) -> Unit = {},
    val onNotifications: () -> Unit = {},
    val onBattery: () -> Unit = {},
    val onAppSettings: () -> Unit = {},
    val onOemHelp: () -> Unit = {},
    val onStopSharing: () -> Unit = {},
)

/** The share section as Settings > Homepool shows it: the core's live state, wired to the server. */
@Composable
fun HomepoolShareSection(ov: HomepoolOverview?) {
    val ctx = LocalContext.current
    val app = ctx.applicationContext
    val api = LocalAppServices.current.api
    val host = LocalHost.current
    val scope = rememberCoroutineScope()
    val snap by Homepool.snapshot.collectAsStateWithLifecycle()
    var resumed by remember { mutableIntStateOf(0) }
    LifecycleEventEffect(Lifecycle.Event.ON_RESUME) { resumed++ }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val server = remember { Servers.current(app) }
    val config = remember(snap, resumed) { Homepool.config(app) }
    val notifOk = remember(resumed) { notificationsAllowed(app) }
    val batteryOk = remember(resumed) { ignoringBattery(app) }
    val askNotifications = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { resumed++; if (!ignoringBattery(app)) requestBatteryExemption(ctx) }

    fun setUp(network: String) {
        val s = server ?: run { error = "Pair this phone with a Crundi server first."; return }
        busy = true; error = null
        scope.launch {
            try {
                // The phone's node identity is its paired device (docs/homepool.md).
                val a = api.homepool.selfNode(s.id, s.deviceName.ifEmpty { Build.MODEL }, network)
                val key = a.node?.key.orEmpty()
                if (key.isEmpty()) error = "The server did not give this phone a node key."
                else {
                    Homepool.setConfig(app, NodeConfig(s.baseUrl.trimEnd('/'), key))
                    Homepool.setAutoStart(app, true)
                    // As the standalone app: ask for what keeps it running in the background.
                    if (!notificationsAllowed(app) && Build.VERSION.SDK_INT >= 33) askNotifications.launch(Manifest.permission.POST_NOTIFICATIONS)
                    else if (!ignoringBattery(app)) requestBatteryExemption(ctx)
                }
            } catch (e: ApiException) { error = errorText(e) } finally { busy = false }
        }
    }

    ShareCard(
        ShareState(
            snap, configured = config != null, notificationsOk = notifOk, batteryOk = batteryOk, busy = busy, error = error,
            otherServer = config?.server?.takeIf { server != null && it.trimEnd('/') != server.baseUrl.trimEnd('/') }?.substringAfter("://"),
        ),
        ShareEvents(
            onSetUp = ::setUp,
            onStart = { Homepool.start(app) },
            onStop = { Homepool.stop(app) },
            onAutoStart = { Homepool.setAutoStart(app, it) },
            onWifiOnly = { Homepool.setWifiOnly(app, it) },
            onMobileCap = { Homepool.setMobileCapMb(app, it) },
            onNotifications = { if (Build.VERSION.SDK_INT >= 33) askNotifications.launch(Manifest.permission.POST_NOTIFICATIONS) },
            onBattery = { requestBatteryExemption(ctx) },
            onAppSettings = { runCatching { ctx.startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:${ctx.packageName}"))) } },
            onOemHelp = { host.openExternal("https://dontkillmyapp.com/") },
            onStopSharing = {
                val id = Homepool.config(app)?.nodeId.orEmpty()
                busy = true; error = null
                scope.launch {
                    // Off on the phone whatever the server says; the server's record goes too when it can.
                    Homepool.setConfig(app, null)
                    try { if (id.isNotEmpty() && ov?.nodes?.any { it.id == id } != false) api.homepool.removeNode(id) }
                    catch (e: ApiException) { if (!e.message.contains("No such node")) error = "Stopped on this phone, but the server kept the node: ${errorText(e)}" }
                    finally { busy = false; resumed++ }
                }
            },
        ),
        networks = ov?.own.orEmpty(),
    )
}

@Composable
fun ShareCard(state: ShareState, events: ShareEvents = ShareEvents(), networks: List<HpNetwork> = emptyList(), askNetwork: Boolean = false) {
    var pick by remember { mutableStateOf(askNetwork) }
    var confirmStop by remember { mutableStateOf(false) }
    val s = state.snapshot
    Group("This phone") {
        if (!state.configured) {
            Text("Let the server send traffic out through this phone's internet, as a home node. It only dials out; nothing listens on the phone, and only what you grant here may use it.", style = CrundiType.Small)
            CrundiButton(if (state.busy) "Setting up…" else "Share this phone's connection", { pick = true }, kind = ButtonKind.Primary, icon = "share", enabled = !state.busy)
            state.error?.let { Text(it, style = CrundiType.Small.copy(color = CrundiColors.Red)) }
        } else {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Box(Modifier.size(10.dp).clip(CircleShape).background(phaseColor(s.phase)))
                Text(phaseWords(s), style = CrundiType.Heading.copy(color = phaseColor(s.phase)), modifier = Modifier.weight(1f))
            }
            if (state.otherServer != null) Text("Sharing with ${state.otherServer}, not the server on screen.", style = CrundiType.Small.copy(color = CrundiColors.Yellow))
            if (s.phase == NodePhase.REFUSED) {
                Text("The server refused this node key: it was removed, switched off or given a new key elsewhere.", style = CrundiType.Small)
                CrundiButton(if (state.busy) "Setting up…" else "Set up again", { pick = true }, kind = ButtonKind.Primary, icon = "refresh", enabled = !state.busy)
            }
            KeyValue("Node", listOfNotNull(s.nodeName, s.networkName?.let { "in $it" }).joinToString(" ").ifEmpty { "-" })
            KeyValue("Network", s.networkLabel)
            KeyValue("Public IP", s.publicIp ?: "-")
            if (s.phase == NodePhase.CONNECTED && s.connectedSince > 0) KeyValue("Connected for", Fmt.span(state.now - s.connectedSince))
            KeyValue("Data today", Fmt.bytes(s.bytesToday) + " (mobile ${Fmt.bytes(s.mobileBytesToday)}" + (if (s.mobileCapMb > 0) " of ${s.mobileCapMb} MB" else "") + ")")
            s.server?.let { v ->
                CrundiDivider()
                Text("The server's view", style = CrundiType.Small)
                KeyValue("Carrying", "${v.slots} slot${if (v.slots == 1) "" else "s"} · ${v.conns} open")
                KeyValue("Today", "${Fmt.bytes(v.bytesToday)} · ${v.connsToday} connection${if (v.connsToday == 1) "" else "s"}")
                if (v.keys.isNotEmpty()) KeyValue("Used by", v.keys.joinToString(", "))
            }
            @OptIn(ExperimentalLayoutApi::class)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                if (s.enabled) CrundiButton("Stop", events.onStop, icon = "stop", enabled = !state.busy)
                else CrundiButton("Start", events.onStart, kind = ButtonKind.Primary, icon = "play", enabled = !state.busy)
            }
            CrundiDivider()
            ShareSwitch("Start at boot", "Starts with the phone and after an update; Android restarts it if it is stopped.", s.autoStart, events.onAutoStart)
            ShareSwitch("Wi-Fi only", "Pauses on mobile data.", s.wifiOnly, events.onWifiOnly)
            var cap by remember(s.mobileCapMb) { mutableStateOf(if (s.mobileCapMb > 0) s.mobileCapMb.toString() else "") }
            Text("Mobile data a day, MB (empty: no cap)", style = CrundiType.Small)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                CrundiTextField(cap, { cap = it.filter(Char::isDigit).take(6) }, Modifier.weight(1f), placeholder = "no cap", mono = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number))
                CrundiButton("Save", { events.onMobileCap(cap.toIntOrNull() ?: 0) }, enabled = (cap.toIntOrNull() ?: 0) != s.mobileCapMb)
            }
            CrundiDivider()
            Text("Running in the background", style = CrundiType.Small)
            PermissionRow(state.notificationsOk, "Notifications allowed", "Notifications blocked: the node's status will not show", "Allow", events.onNotifications)
            PermissionRow(state.batteryOk, "Unrestricted battery: can run in the background", "Battery optimised: Android may stop the node", "Allow", events.onBattery)
            @OptIn(ExperimentalLayoutApi::class)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                CrundiButton("App settings", events.onAppSettings, kind = ButtonKind.Ghost, icon = "settings")
                CrundiButton("Phone maker's limits", events.onOemHelp, kind = ButtonKind.Ghost, icon = "external-link")
            }
            if (s.log.isNotEmpty()) {
                CrundiDivider()
                Text("Recent activity", style = CrundiType.Small)
                Column(
                    Modifier.fillMaxWidth().heightIn(max = 180.dp).clip(Dimens.ShapeSm).background(CrundiColors.BgPrimary)
                        .border(Dimens.Hairline, CrundiColors.BorderSubtle, Dimens.ShapeSm).verticalScroll(rememberScrollState(Int.MAX_VALUE)).padding(8.dp),
                ) { for (line in s.log.takeLast(40)) Text(line, style = CrundiType.Code.copy(fontSize = 11.sp, lineHeight = 15.sp, color = CrundiColors.TextSecondary)) }
            }
            state.error?.let { Text(it, style = CrundiType.Small.copy(color = CrundiColors.Red)) }
            CrundiButton("Stop sharing", { confirmStop = true }, kind = ButtonKind.Danger, icon = "x", enabled = !state.busy)
        }
    }
    if (pick) {
        var net by remember { mutableStateOf(networks.firstOrNull { it.id == "home" }?.id ?: "") }
        CrundiDialog(
            "Share this phone's connection", { pick = false },
            message = "Which network does this phone belong to? Keys reach it through that network. Unassigned carries nothing until you move it.",
            confirm = "Share", onConfirm = { pick = false; events.onSetUp(net) },
        ) { NetworkChoice(networks, net, allowNone = true) { net = it } }
    }
    if (confirmStop) CrundiDialog(
        "Stop sharing this phone?", { confirmStop = false },
        message = "The node stops, its key is forgotten on this phone, and the server removes the node. Slots it carries move to another node, if one is up.",
        confirm = "Stop sharing", danger = true, onConfirm = { confirmStop = false; events.onStopSharing() },
    )
}

@Composable
private fun ShareSwitch(label: String, detail: String, checked: Boolean, onChange: (Boolean) -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(label, style = CrundiType.Body)
            Text(detail, style = CrundiType.Small)
        }
        HpSwitch(checked, onChange)
    }
}

@Composable
private fun PermissionRow(ok: Boolean, okText: String, badText: String, action: String, onAction: () -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        CrundiIcon(if (ok) "check" else "alert-triangle", size = Dimens.IconSm, tint = if (ok) CrundiColors.Green else CrundiColors.Yellow)
        Text(if (ok) okText else badText, style = CrundiType.Small.copy(color = if (ok) CrundiColors.TextSecondary else CrundiColors.TextPrimary), modifier = Modifier.weight(1f))
        if (!ok) CrundiButton(action, onAction)
    }
}

fun notificationsAllowed(ctx: Context): Boolean =
    Build.VERSION.SDK_INT < 33 || ctx.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED

fun ignoringBattery(ctx: Context): Boolean =
    runCatching { ctx.getSystemService(PowerManager::class.java).isIgnoringBatteryOptimizations(ctx.packageName) }.getOrDefault(true)

/** The system's "let it run in the background" prompt, or its list when the prompt is not there. */
fun requestBatteryExemption(ctx: Context) {
    if (ignoringBattery(ctx)) return
    runCatching {
        @Suppress("BatteryLife")
        ctx.startActivity(Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:${ctx.packageName}")))
    }.onFailure { runCatching { ctx.startActivity(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS)) } }
}
