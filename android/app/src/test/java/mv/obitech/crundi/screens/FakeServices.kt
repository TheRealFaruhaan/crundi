package mv.obitech.crundi.screens

import mv.obitech.crundi.api.CrundiClient
import mv.obitech.crundi.api.FakeTransport
import mv.obitech.crundi.ui.components.DotState
import mv.obitech.crundi.ui.shell.AppServices
import mv.obitech.crundi.ui.shell.Connection
import mv.obitech.crundi.ui.shell.ProjectSummary
import mv.obitech.crundi.ui.shell.ShellData
import mv.obitech.crundi.ui.shell.ShellSource
import mv.obitech.crundi.ui.shell.StaticShellSource
import mv.obitech.crundi.ui.shell.UsageGlance

/**
 * Test data for screenshots. Realistic, so the images show what the app will
 * look like in use. The API engineer extends FakeServices with fake API data.
 */
object FakeServices {
    val projects = listOf(
        ProjectSummary("crundi", "crundi", "/home/crundi/projects/crundi", DotState.Working, liveService = true, hasService = true, scheduled = true, factory = mv.obitech.crundi.ui.shell.FactoryMark.Running),
        ProjectSummary("dhivo-socials", "dhivo-socials", "/home/crundi/projects/dhivo-socials", DotState.Running, hasService = true, factory = mv.obitech.crundi.ui.shell.FactoryMark.Needs),
        ProjectSummary("faruhaan.com", "faruhaan.com", "/home/crundi/projects/faruhaan.com", DotState.NeedsInput),
        ProjectSummary("ledger", "ledger", "/home/crundi/projects/ledger", DotState.Idle),
        ProjectSummary("home-automation", "home-automation", "/home/crundi/projects/home-automation", DotState.Waiting),
    )

    val shellData = ShellData(
        serverHost = "dev.faruhaan.com",
        username = "faruhaan",
        connection = Connection.Connected,
        projects = projects,
        currentProject = "crundi",
        approvals = 2,
        usage = UsageGlance(
            weekUsed = 0.61f, weekElapsed = 0.70f, fiveHourUsed = 0.39f, fiveHourElapsed = 0.58f,
            weekResetsAt = System.currentTimeMillis() + 2 * 86_400_000L + 3 * 3_600_000L,
            fiveHourResetsAt = System.currentTimeMillis() + 2 * 3_600_000L + 61_000L,
            fetchedAt = System.currentTimeMillis() - 2 * 60_000L,
        ),
        tabBadges = mapOf("factories" to 1, "secrets" to 2, "settings" to -1),
        panels = 2,
    )

    /**
     * Services over a server in memory: [FakeApi.transport] answers every
     * route with realistic data (see FakeApi), so a Route can be shot too.
     */
    fun default(shell: ShellData = shellData, transport: FakeTransport = FakeApi.transport()): AppServices = object : AppServices {
        override val shell: ShellSource = StaticShellSource(shell)
        override val api: CrundiClient = CrundiClient(transport)
    }
}
