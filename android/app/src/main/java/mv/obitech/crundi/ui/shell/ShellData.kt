package mv.obitech.crundi.ui.shell

import androidx.compose.runtime.staticCompositionLocalOf
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import mv.obitech.crundi.ui.components.DotState

/** The live connection to the server, as the desktop's status badge says it. */
enum class Connection(val label: String) {
    Connected("connected"),
    Reconnecting("reconnecting"),
    Offline("offline"),
    Disconnected("disconnected"),
}

/** One project in the switcher (from GET /api/projects; `alias` is its key everywhere). */
data class ProjectSummary(
    val alias: String,
    val name: String = alias,
    val path: String = "",
    val state: DotState = DotState.Idle,
    /** Server path of its icon (/api/project-icon?...), or null. */
    val icon: String? = null,
)

/**
 * The top bar's usage glance: the desktop draws two thin rows behind its top
 * bar, weekly and 5-hour, each a usage bar over a time-to-reset bar. Values
 * are 0..1; null when the server has no usage to show.
 */
data class UsageGlance(val weekUsed: Float, val weekElapsed: Float, val fiveHourUsed: Float, val fiveHourElapsed: Float)

/** Everything the shell (top bar, tabs, project drawer) shows. */
data class ShellData(
    val serverHost: String = "",
    val username: String = "",
    val connection: Connection = Connection.Disconnected,
    val projects: List<ProjectSummary> = emptyList(),
    val currentProject: String? = null,
    /** Things waiting on the person (permission requests, questions, factory waits): the top bar's counter. */
    val approvals: Int = 0,
    val usage: UsageGlance? = null,
) {
    val project: ProjectSummary? get() = projects.firstOrNull { it.alias == currentProject }
}

/**
 * Where the shell's data comes from. The API engineer implements this over
 * the API client; tests and previews use [StaticShellSource].
 */
interface ShellSource {
    val state: StateFlow<ShellData>
    /** Pick the current project (remembered per server). */
    fun selectProject(alias: String)
    /** Fetch again (pull to refresh, coming back to the app). */
    fun refresh() {}
}

/** Fixed data: for screenshot tests, previews, and until a server is connected. */
class StaticShellSource(initial: ShellData) : ShellSource {
    private val flow = MutableStateFlow(initial)
    override val state: StateFlow<ShellData> = flow
    override fun selectProject(alias: String) { flow.value = flow.value.copy(currentProject = alias) }
}

/**
 * The app's services, handed down the tree. One object per paired server
 * session, built by AppGraph. The API engineer adds the API client (and the
 * fakes) here; screens get what they need with `LocalAppServices.current`.
 */
interface AppServices {
    val shell: ShellSource
}

val LocalAppServices = staticCompositionLocalOf<AppServices> { error("CrundiTheme content needs LocalAppServices (see AppGraph)") }
