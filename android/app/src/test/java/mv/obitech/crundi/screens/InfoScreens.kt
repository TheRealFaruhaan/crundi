package mv.obitech.crundi.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import mv.obitech.crundi.api.model.Collaborators
import mv.obitech.crundi.api.model.MaintenanceTask
import mv.obitech.crundi.api.model.SandboxStatus
import mv.obitech.crundi.api.model.ServerLog
import mv.obitech.crundi.api.model.Stats
import mv.obitech.crundi.api.model.Tunnels
import mv.obitech.crundi.ui.components.DialogCard
import mv.obitech.crundi.ui.operations.InfoEvents
import mv.obitech.crundi.ui.operations.InfoProject
import mv.obitech.crundi.ui.operations.InfoScreen
import mv.obitech.crundi.ui.operations.InfoState
import mv.obitech.crundi.ui.operations.InviteForm
import mv.obitech.crundi.ui.operations.Load
import mv.obitech.crundi.ui.operations.PasscodeCard
import mv.obitech.crundi.ui.operations.PasscodeShown
import mv.obitech.crundi.ui.theme.CrundiColors
import org.junit.Test

/** The Info tab: with a project, with none, collaborators, the invite form, the passcode, failures. */
class InfoScreens : ScreenshotTest() {
    /** 2026-10-18 11:20 UTC: Aisha's access has a day left, Ibrahim's has ended. */
    private val now = 1792322400000L

    private val state = FakeApi.state
    private val stats: Stats = FakeApi.model("stats")
    private val collab: Collaborators = FakeApi.model("collaborators")
    private val tunnel: Tunnels = FakeApi.model("tunnel")
    private val logs: List<ServerLog> = FakeApi.model<List<ServerLog>>("server-logs", "logs") + listOf(
        ServerLog(now - 60_000, "info", "[services] crundi:docs started (pid 48121)"),
        ServerLog(now - 40_000, "info", "[chat] a1b2c3d4 turn ended in 41s, 18.2k tokens"),
        ServerLog(now - 20_000, "error", "[forwards] ledger.dev.faruhaan.com: nothing answers on 4815"),
        ServerLog(now - 5_000, "info", "[push] sent 1 notification to Pixel 7"),
    )
    private val tasks = listOf(
        MaintenanceTask("npm-cache", "npm cache", "Downloaded packages npm keeps for reinstalls.", bytes = 1_288_490_188),
        MaintenanceTask("journal", "System journal", "Old logs kept by systemd.", elevated = true, bytes = 402_653_184),
        MaintenanceTask("docker-build", "Docker build cache", "Layers left from earlier builds.", bytes = 3_221_225_472),
    )
    private val project = state.projects.first().let { p ->
        val svcs = state.servicesOf(p.alias)
        InfoProject(p, true, svcs.size, svcs.count { it.status == "running" })
    }

    private fun full(p: InfoProject? = project) = InfoState(
        project = p,
        stats = Load.Ready(stats),
        tasks = Load.Ready(tasks),
        collab = Load.Ready(collab),
        tunnel = Load.Ready(tunnel),
        logs = Load.Ready(logs),
        forwards = 3,
        projects = state.projects,
        signInUrl = "https://dev.faruhaan.com",
        now = now,
    )

    @Composable
    private fun InDialog(content: @Composable () -> Unit) {
        Box(Modifier.fillMaxSize().background(CrundiColors.Scrim).padding(16.dp), contentAlignment = Alignment.Center) { content() }
    }

    @Test fun infoProject() = shoot("info-project") { InfoScreen(full()) }

    @Test fun infoGlobal() = shoot("info-global") { InfoScreen(full(p = null)) }

    /** Scrolled to the collaborators: the machine sections are above. */
    @Test fun infoCollaborators() = shoot("info-collaborators") {
        InfoScreen(full(p = null).copy(stats = Load.Failed("The machine's numbers are for the owner."), tasks = Load.Ready(emptyList())))
    }

    @Test fun infoNoSandbox() = shoot("info-no-sandbox") {
        InfoScreen(full(p = null).copy(
            stats = Load.Ready(Stats()), tasks = Load.Ready(emptyList()),
            collab = Load.Ready(Collaborators(sandbox = SandboxStatus(ok = false, problems = listOf("bubblewrap (bwrap) is not installed", "socat is not installed"), canAutoFix = true))),
        ))
    }

    @Test fun infoInvite() = shoot("info-invite") {
        InDialog { DialogCard("Invite someone", dismiss = null) { InviteForm(state.projects, project.project.alias, sandboxOk = true, now = now, onCancel = {}, onCreate = {}) } }
    }

    @Test fun infoPasscode() = shoot("info-passcode") {
        InDialog { PasscodeCard(PasscodeShown("Sam", "TQ7M-4KX2-PV9D", listOf("crundi", "ledger"), now + 7 * 86_400_000L), "https://dev.faruhaan.com", InfoEvents()) }
    }

    @Test fun infoError() = shoot("info-error") {
        val off = "The server cannot be reached. Check the connection and try again."
        InfoScreen(InfoState(project = project, stats = Load.Failed(off), tasks = Load.Failed(off), collab = Load.Failed(off), tunnel = Load.Failed(off), logs = Load.Failed(off), now = now))
    }
}
