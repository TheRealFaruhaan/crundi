package mv.obitech.crundi.api

import android.content.SharedPreferences
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import mv.obitech.crundi.api.model.AgentState
import mv.obitech.crundi.api.model.Approvals
import mv.obitech.crundi.api.model.ServerState
import mv.obitech.crundi.api.model.Usage
import mv.obitech.crundi.ui.components.DotState
import mv.obitech.crundi.ui.shell.Connection
import mv.obitech.crundi.ui.shell.Host
import mv.obitech.crundi.ui.shell.ProjectSummary
import mv.obitech.crundi.ui.shell.ShellData
import mv.obitech.crundi.ui.shell.ShellSource
import mv.obitech.crundi.ui.shell.UsageGlance

/**
 * The shell's data from the live stream: the project list (state.projects,
 * with each project's dot), the connection badge, the approvals count and
 * the usage backdrop. The current project is remembered per server in
 * [prefs] under [key].
 */
class LiveShellSource(
    private val client: CrundiClient,
    private val serverHost: String,
    private val username: String,
    private val prefs: SharedPreferences?,
    private val key: String,
    scope: CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Default),
) : ShellSource {
    private val chosen = MutableStateFlow(prefs?.getString(key, null))

    override val state: StateFlow<ShellData> = combine(client.events.state, client.events.approvals, client.events.usage, client.status, chosen) { st, ap, us, status, pick ->
        build(st, ap, us, status, pick)
    }.stateIn(scope, SharingStarted.WhileSubscribed(5_000), ShellData(serverHost = serverHost, username = username, connection = Connection.Reconnecting))

    override fun selectProject(alias: String) {
        chosen.value = alias
        prefs?.edit()?.putString(key, alias)?.apply()
    }

    override fun refresh() = client.reconnect()

    private fun build(st: ServerState?, ap: Approvals, us: Usage?, status: LiveStatus, pick: String?): ShellData {
        val projects = st?.projects.orEmpty().map { p ->
            ProjectSummary(p.alias, p.label, p.path, dotOf(st?.projectState(p.alias)), p.icon.ifEmpty { null })
        }
        val current = pick?.takeIf { a -> projects.isEmpty() || projects.any { it.alias == a } } ?: projects.firstOrNull()?.alias
        return ShellData(
            serverHost = serverHost, username = username, connection = connectionOf(status),
            projects = projects, currentProject = current, approvals = ap.count, usage = glance(us),
        )
    }

    companion object {
        fun dotOf(s: AgentState?): DotState = when (s) {
            AgentState.NeedsInput -> DotState.NeedsInput
            AgentState.Working -> DotState.Working
            AgentState.Waiting -> DotState.Waiting
            AgentState.Idle -> DotState.Running
            else -> DotState.Idle
        }

        fun connectionOf(s: LiveStatus): Connection = when (s) {
            LiveStatus.Connected -> Connection.Connected
            LiveStatus.Connecting, LiveStatus.Reconnecting -> Connection.Reconnecting
            LiveStatus.Offline -> Connection.Offline
            LiveStatus.Idle, LiveStatus.SignedOut -> Connection.Disconnected
        }

        private const val WEEK_MS = 7 * 24 * 3600_000L
        private const val FIVE_H_MS = 5 * 3600_000L

        /** The top bar's two rows: how much is used, and how much of each window has gone by. */
        fun glance(u: Usage?, now: Long = System.currentTimeMillis()): UsageGlance? {
            if (u == null || (u.week == null && u.fiveHour == null)) return null
            fun elapsed(resetsAt: Long, window: Long) = if (resetsAt <= 0) 0f else (1f - (resetsAt - now).toFloat() / window).coerceIn(0f, 1f)
            return UsageGlance(
                weekUsed = (u.week?.utilization ?: 0) / 100f, weekElapsed = elapsed(u.week?.resetsAtMs ?: 0, WEEK_MS),
                fiveHourUsed = (u.fiveHour?.utilization ?: 0) / 100f, fiveHourElapsed = elapsed(u.fiveHour?.resetsAtMs ?: 0, FIVE_H_MS),
            )
        }
    }
}

/**
 * The main window's ties to the live client: presence while the app is in
 * front (the server holds back phone notifications for what is on screen),
 * reconnecting when it comes back, and asking to pair again when the
 * server has ended this phone's sign-in.
 */
@Composable
fun LiveClientEffects(client: CrundiClient, project: String?, host: Host) {
    val owner = LocalLifecycleOwner.current
    DisposableEffect(owner, client, project) {
        val obs = LifecycleEventObserver { _, e ->
            when (e) {
                Lifecycle.Event.ON_RESUME -> { client.setPresence(true, project); client.reconnect() }
                Lifecycle.Event.ON_PAUSE -> client.setPresence(false, project)
                else -> {}
            }
        }
        owner.lifecycle.addObserver(obs)
        if (owner.lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) client.setPresence(true, project)
        onDispose { owner.lifecycle.removeObserver(obs) }
    }
    val signedOut by client.signedOut.collectAsState()
    LaunchedEffect(signedOut) { if (signedOut) host.pairAgain("The server ended this phone's sign-in.") }
}
