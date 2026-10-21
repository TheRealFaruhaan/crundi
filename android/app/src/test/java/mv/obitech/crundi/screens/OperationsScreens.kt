package mv.obitech.crundi.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import mv.obitech.crundi.api.model.Container
import mv.obitech.crundi.api.model.ContainerDetail
import mv.obitech.crundi.api.model.DockerDisk
import mv.obitech.crundi.api.model.DockerEngine
import mv.obitech.crundi.api.model.DockerForward
import mv.obitech.crundi.api.model.DockerForwardContainer
import mv.obitech.crundi.api.model.DockerImage
import mv.obitech.crundi.api.model.DockerJob
import mv.obitech.crundi.api.model.DockerNetwork
import mv.obitech.crundi.api.model.DockerStack
import mv.obitech.crundi.api.model.DockerStats
import mv.obitech.crundi.api.model.DockerStatus
import mv.obitech.crundi.api.model.DockerVolume
import mv.obitech.crundi.api.model.Forwards
import mv.obitech.crundi.api.model.LoginSession
import mv.obitech.crundi.api.model.MaintenanceTask
import mv.obitech.crundi.api.model.NotifyChannel
import mv.obitech.crundi.api.model.Secrets
import mv.obitech.crundi.api.model.ServerLog
import mv.obitech.crundi.api.model.ServerSettings
import mv.obitech.crundi.api.model.Service
import mv.obitech.crundi.api.model.ServiceStats
import mv.obitech.crundi.api.model.Stats
import mv.obitech.crundi.api.model.UpdateStatus
import mv.obitech.crundi.api.model.Usage
import mv.obitech.crundi.api.model.UsagePoint
import mv.obitech.crundi.api.model.ClaudeUpdate
import mv.obitech.crundi.ui.components.DialogCard
import mv.obitech.crundi.ui.operations.AddForwardForm
import mv.obitech.crundi.ui.operations.AddSecretForm
import mv.obitech.crundi.ui.operations.ConfigScreen
import mv.obitech.crundi.ui.operations.ConfigState
import mv.obitech.crundi.ui.operations.ContainerScreen
import mv.obitech.crundi.ui.operations.ContainerState
import mv.obitech.crundi.ui.operations.ContainerTab
import mv.obitech.crundi.ui.operations.DockerScreen
import mv.obitech.crundi.ui.operations.DockerState
import mv.obitech.crundi.ui.operations.DockerView
import mv.obitech.crundi.ui.operations.ForwardsScreen
import mv.obitech.crundi.ui.operations.ForwardsState
import mv.obitech.crundi.ui.operations.GeneralScreen
import mv.obitech.crundi.ui.operations.GeneralState
import mv.obitech.crundi.ui.operations.JobCard
import mv.obitech.crundi.ui.operations.JobView
import mv.obitech.crundi.ui.operations.Load
import mv.obitech.crundi.ui.operations.LogLine
import mv.obitech.crundi.ui.operations.LogsScreen
import mv.obitech.crundi.ui.operations.LogsState
import mv.obitech.crundi.ui.operations.MachineScreen
import mv.obitech.crundi.ui.operations.MachineState
import mv.obitech.crundi.ui.operations.NotificationsScreen
import mv.obitech.crundi.ui.operations.NotificationsState
import mv.obitech.crundi.ui.operations.PinAsk
import mv.obitech.crundi.ui.operations.PinCard
import mv.obitech.crundi.ui.operations.PromptScreen
import mv.obitech.crundi.ui.operations.PromptState
import mv.obitech.crundi.ui.operations.RegisterServiceForm
import mv.obitech.crundi.ui.operations.SecretsScreen
import mv.obitech.crundi.ui.operations.SecretsState
import mv.obitech.crundi.ui.operations.ServerPageScreen
import mv.obitech.crundi.ui.operations.ServerPageState
import mv.obitech.crundi.ui.operations.ServicesScreen
import mv.obitech.crundi.ui.operations.ServicesState
import mv.obitech.crundi.ui.operations.SessionsScreen
import mv.obitech.crundi.ui.operations.SessionsState
import mv.obitech.crundi.ui.operations.SettingsHome
import mv.obitech.crundi.ui.operations.SettingsScreen
import mv.obitech.crundi.ui.operations.ShellView
import mv.obitech.crundi.ui.operations.StatSample
import mv.obitech.crundi.ui.operations.UsageScreen
import mv.obitech.crundi.ui.operations.UsageState
import mv.obitech.crundi.ui.theme.CrundiColors
import org.junit.Test

/**
 * Operations: Services, Forwards, Docker, Secrets, Usage, Settings. Every
 * screen at phone size with the fixtures' data (src/test/resources/api).
 * The clock is fixed so "ago" and "resets in" read the same every run.
 */
class OperationsScreens : ScreenshotTest() {
    /** 2026-10-21 11:20 UTC: just after the fixtures' last samples. */
    private val now = 1792581000000L

    private val services: List<Service> = FakeApi.model("services", "services")
    private val stats: Stats = FakeApi.model("stats")
    private val forwards: Forwards = FakeApi.model("forwards")
    private val serviceStats = mapOf(
        "crundi:docs" to ServiceStats("running", 48121, 0.4, 61235200, listOf(0.2, 0.5, 0.3, 0.9, 0.4, 0.6, 0.4, 0.3, 0.5, 0.4), listOf(58000000L, 59000000L, 60500000L, 61000000L, 61235200L)),
        "ledger:stack" to ServiceStats("running", 51200, 3.1, 402653184, listOf(2.0, 2.4, 5.1, 3.0, 2.8, 3.3, 3.1), listOf(380000000L, 395000000L, 402653184L)),
    )
    private val moreServices = services + Service(
        key = "crundi:worker", name = "worker", alias = "crundi", command = "node scripts/queue-worker.js --concurrency 2",
        status = "stopped", tunnelPort = 0,
    )

    /** A dialog drawn in place (Dialog windows are not part of a screenshot). */
    @Composable
    private fun InDialog(content: @Composable () -> Unit) {
        Box(Modifier.fillMaxSize().background(CrundiColors.Scrim).padding(16.dp), contentAlignment = Alignment.Center) { content() }
    }

    // ─── Services ───

    @Test fun servicesList() = shoot("services-list") {
        ServicesScreen(ServicesState(Load.Ready(moreServices), serviceStats, forwards.forwards, forwards.domain, project = "crundi", allProjects = true,
            logs = mapOf("crundi:docs" to Load.Ready(FakeApi.model<List<String>>("service-logs", "logs")))))
    }

    @Test fun servicesProject() = shoot("services-project") {
        ServicesScreen(ServicesState(Load.Ready(moreServices), serviceStats, forwards.forwards, forwards.domain, project = "crundi"))
    }

    @Test fun servicesEmpty() = shoot("services-empty") {
        ServicesScreen(ServicesState(Load.Ready(emptyList()), project = "dhivo-socials"))
    }

    @Test fun servicesRegister() = shoot("services-register") {
        InDialog { DialogCard("Register a service", dismiss = null) { RegisterServiceForm("crundi", {}, {}) } }
    }

    @Test fun servicesError() = shoot("services-error") {
        ServicesScreen(ServicesState(Load.Failed("The server could not be reached. Check the connection and try again.")))
    }

    // ─── Forwards ───

    private val behind = mapOf(
        "docs" to DockerForward("docs", 4811, "subdomain", false, "https://docs.dev.faruhaan.com", "crundi", null, listening = true),
        "ledger-api" to DockerForward("ledger-api", 4815, "path", true, "", "ledger", DockerForwardContainer("8f7e", "ledger-api-1", "running", true, "healthy", "ledger"), true),
        "old-demo" to DockerForward("old-demo", 4900, "path", false, "", "", null, false),
    )

    @Test fun forwardsList() = shoot("forwards-list") {
        val list = forwards.forwards + mv.obitech.crundi.api.model.Forward("old-demo", 4900, "path", description = "Demo for the client", url = "https://dev.faruhaan.com/tunnel/old-demo/")
        ForwardsScreen(ForwardsState(Load.Ready(list), behind, forwards.domain, true, FakeServices.projects.map { it.alias }))
    }

    @Test fun forwardsPlain() = shoot("forwards-plain") {
        ForwardsScreen(ForwardsState(Load.Ready(forwards.forwards), emptyMap(), forwards.domain, true))
    }

    @Test fun forwardsEmpty() = shoot("forwards-empty") { ForwardsScreen(ForwardsState(Load.Ready(emptyList()), domain = "dev.faruhaan.com")) }

    @Test fun forwardsAdd() = shoot("forwards-add") {
        InDialog { DialogCard("Add forward", dismiss = null) { AddForwardForm("dev.faruhaan.com", true, {}, { _, _ -> }, initialPort = 5173, initialName = "vite") } }
    }

    // ─── Docker ───

    private val containers: List<Container> = FakeApi.model<List<Container>>("docker-containers", "containers") + listOf(
        Container("3c4d5e6f7a8b", name = "ledger-api-1", image = "ghcr.io/faruhaan/ledger-api:2.4", state = "running", status = "Up 6 days (unhealthy)", health = "unhealthy",
            running = true, project = "ledger", service = "api", ports = listOf(mv.obitech.crundi.api.model.DockerPort("0.0.0.0", 4815, 8080)),
            stats = DockerStats(1, 1.9, 4, 100663296, 8589934592)),
        Container("9a8b7c6d5e4f", name = "uptime-kuma", image = "louislam/uptime-kuma:1", state = "paused", status = "Up 2 hours (Paused)", running = false),
    )
    private fun docker(view: DockerView) = DockerState(
        status = Load.Ready(FakeApi.model<DockerStatus>("docker-status")),
        view = view,
        containers = Load.Ready(containers),
        stacks = Load.Ready(FakeApi.model<List<DockerStack>>("docker-stacks", "stacks") + DockerStack("monitoring", "/home/crundi/projects/home-automation", listOf("docker-compose.yml"), true, 0, 2)),
        images = Load.Ready(FakeApi.model<List<DockerImage>>("docker-images", "images")),
        volumes = Load.Ready(FakeApi.model<List<DockerVolume>>("docker-volumes", "volumes") + DockerVolume("b1f0c2d3e4a5968778695a4b3c2d1e0f", "local", anonymous = true, created = "2026-07-01T10:00:00Z")),
        networks = Load.Ready(FakeApi.model<List<DockerNetwork>>("docker-networks", "networks") + DockerNetwork("bb22cc33dd44", name = "ledger_default", driver = "bridge", scope = "local", subnet = "172.18.0.0/16", gateway = "172.18.0.1", project = "ledger", usedBy = listOf("ledger-db-1", "ledger-api-1"))),
        engine = Load.Ready(FakeApi.model<DockerEngine>("docker-system", "engine")),
        disk = Load.Ready(FakeApi.model<DockerDisk>("docker-df", "disk")),
    )

    @Test fun dockerContainers() = shoot("docker-containers") { DockerScreen(docker(DockerView.Containers)) }
    @Test fun dockerStacks() = shoot("docker-stacks") { DockerScreen(docker(DockerView.Stacks)) }
    @Test fun dockerImages() = shoot("docker-images") { DockerScreen(docker(DockerView.Images)) }
    @Test fun dockerVolumes() = shoot("docker-volumes") { DockerScreen(docker(DockerView.Volumes)) }
    @Test fun dockerNetworks() = shoot("docker-networks") { DockerScreen(docker(DockerView.Networks)) }
    @Test fun dockerSystem() = shoot("docker-system") { DockerScreen(docker(DockerView.System)) }
    @Test fun dockerFiltered() = shoot("docker-filtered") { DockerScreen(docker(DockerView.Containers).copy(filter = "ledger")) }

    @Test fun dockerNotInstalled() = shoot("docker-not-installed") { DockerScreen(DockerState(status = Load.Ready(DockerStatus(installed = false)))) }
    @Test fun dockerUnreachable() = shoot("docker-unreachable") {
        DockerScreen(DockerState(status = Load.Ready(DockerStatus(installed = true, reachable = false, error = "connect EACCES /var/run/docker.sock"))))
    }

    @Test fun dockerJob() = shoot("docker-job") {
        InDialog {
            JobCard(JobView("j1", "ledger: compose pull", DockerJob("ledger: compose pull", " db Pulling \n api Pulling \n db Pulled \n api Pulled \n", 4, done = false)), {})
        }
    }

    private val detail: ContainerDetail = FakeApi.model("docker-detail")
    private fun container(tab: ContainerTab) = ContainerState(
        detail = Load.Ready(detail), tab = tab,
        logs = listOf(
            LogLine(false, "2026-10-21 11:02:10.120", "\u001b[32mLOG\u001b[0m:  database system is ready to accept connections"),
            LogLine(false, "2026-10-21 11:04:44.008", "LOG:  checkpoint starting: time"),
            LogLine(false, "2026-10-21 11:05:12.551", "LOG:  checkpoint complete: wrote 42 buffers (0.3%); 0 WAL file(s) added"),
            LogLine(true, "2026-10-21 11:09:30.902", "ERROR:  relation \"ledger_entries_old\" does not exist at character 15"),
            LogLine(true, "2026-10-21 11:09:30.902", "STATEMENT:  SELECT * FROM ledger_entries_old LIMIT 10"),
            LogLine(false, "2026-10-21 11:14:44.010", "LOG:  checkpoint starting: time"),
        ),
        times = true,
        stats = (0 until 40).map { i ->
            StatSample(now - (40 - i) * 2000L, 1.0 + (i % 7) * 0.4, 290_000_000L + i * 300_000L, 2000.0 + (i % 5) * 900, 1200.0 + (i % 3) * 400, (i % 9) * 4000.0, (i % 4) * 12000.0)
        },
        latest = detail.container.stats,
        shell = ShellView(open = true, lines = listOf(
            "root@8f7e6d5c4b3a:/# ls /var/lib/postgresql/data",
            "PG_VERSION  base  global  pg_hba.conf  pg_wal  postgresql.conf",
            "root@8f7e6d5c4b3a:/# psql -U ledger -c 'select count(*) from entries'",
            " count ",
            "-------",
            " 18234",
            "(1 row)",
            "",
            "root@8f7e6d5c4b3a:/# ",
        )),
    )

    @Test fun dockerContainerOverview() = shoot("docker-container-overview") { ContainerScreen(container(ContainerTab.Overview)) }
    @Test fun dockerContainerLogs() = shoot("docker-container-logs") { ContainerScreen(container(ContainerTab.Logs)) }
    @Test fun dockerContainerStats() = shoot("docker-container-stats") { ContainerScreen(container(ContainerTab.Stats)) }
    @Test fun dockerContainerShell() = shoot("docker-container-shell") { ContainerScreen(container(ContainerTab.Shell)) }
    @Test fun dockerContainerInspect() = shoot("docker-container-inspect") { ContainerScreen(container(ContainerTab.Inspect)) }

    // ─── Secrets ───

    private val secrets: Secrets = FakeApi.model("secrets")

    @Test fun secretsList() = shoot("secrets-list") {
        SecretsScreen(SecretsState(Load.Ready(secrets.secrets + mv.obitech.crundi.api.model.Secret("2b3c", "CLOUDFLARE_API_TOKEN", "DNS edits for faruhaan.com", "2026-06-01T12:00:00Z")),
            secrets.requests + mv.obitech.crundi.api.model.SecretRequest("8d9e", "1a2b3c4d5e6f7081", "OPENAI_KEY", "dhivo-socials", "Generate captions for the October posts"),
            revealed = mapOf("1a2b3c4d5e6f7081" to "sk-proj-7Hq2kT9vLm3XwPz")))
    }

    @Test fun secretsEmpty() = shoot("secrets-empty") { SecretsScreen(SecretsState(Load.Ready(emptyList()))) }

    @Test fun secretsPin() = shoot("secrets-pin") {
        InDialog { PinCard(PinAsk("Approve access", "Enter the PIN for \"GITHUB_TOKEN\" to release it to Claude.", "Approve") {}, "1234", {}, null, false, {}, {}) }
    }

    @Test fun secretsPinWrong() = shoot("secrets-pin-wrong") {
        InDialog { PinCard(PinAsk("Reveal secret", "Enter the PIN for \"OPENAI_KEY\".", "Reveal") {}, "", {}, "Incorrect PIN", false, {}, {}) }
    }

    @Test fun secretsAdd() = shoot("secrets-add") { InDialog { DialogCard("Add a secret", dismiss = null) { AddSecretForm({}, { _, _, _, _ -> }) } } }

    // ─── Usage ───

    private val usage: Usage = FakeApi.model("usage")
    private val history: List<UsagePoint> = run {
        // A week of samples every 2 hours, climbing through the window, the 5-hour one resetting.
        val weekReset = "2026-10-24T08:00:00.000Z"
        val start = now - 4 * 24 * 3_600_000L
        (0..48).map { i ->
            val at = start + i * 7_200_000L
            val fiveStart = at - at % 18_000_000L
            UsagePoint(at, five = ((at - fiveStart) / 180_000L).toInt().coerceAtMost(95), week = 4 + i * 42 / 48, fiveReset = java.time.Instant.ofEpochMilli(fiveStart + 18_000_000L).toString(), weekReset = weekReset)
        }
    }

    @Test fun usageWeek() = shoot("usage-week") { UsageScreen(UsageState(Load.Ready(usage), Load.Ready(history), now = now)) }
    @Test fun usageBoth() = shoot("usage-both") { UsageScreen(UsageState(Load.Ready(usage.copy(fiveHour = usage.fiveHour?.copy(utilization = 88))), Load.Ready(history), showFive = true, now = now)) }
    @Test fun usageCollaborator() = shoot("usage-collaborator") { UsageScreen(UsageState(Load.Ready(null), Load.Ready(emptyList()), now = now)) }

    // ─── Settings ───

    private val settings: ServerSettings = FakeApi.model("settings")

    @Test fun settingsHome() = shoot("settings-home") { SettingsScreen(SettingsHome(FakeApi.model<UpdateStatus>("update"), "dev.faruhaan.com", "1.1.0-dev.1")) }

    @Test fun settingsServer() = shoot("settings-server") {
        ServerPageScreen(ServerPageState(Load.Ready(FakeApi.model<UpdateStatus>("update")), Load.Ready(ClaudeUpdate("2.1.40", "2.1.45", "2.1.42", available = true, beyondTested = true, canUpdate = true, needsSudo = true))))
    }

    @Test fun settingsNotifications() = shoot("settings-notifications") {
        NotificationsScreen(NotificationsState(Load.Ready(FakeApi.model<List<NotifyChannel>>("notify-channels", "channels")), Load.Ready(settings.notifyPrefs)))
    }

    @Test fun settingsPrompt() = shoot("settings-prompt") {
        PromptScreen(PromptState(Load.Ready(settings), "crundi", "This repo deploys on merge to main. Work on dev; never push to main."))
    }

    @Test fun settingsGeneral() = shoot("settings-general") { GeneralScreen(GeneralState(Load.Ready(settings))) }

    @Test fun settingsConfig() = shoot("settings-config") { ConfigScreen(ConfigState(Load.Ready(settings), restartRequired = true)) }

    @Test fun settingsMachine() = shoot("settings-machine") {
        val s = stats.copy(system = stats.system!!.copy(history = mv.obitech.crundi.api.model.StatsHistory(cpu = listOf(9.0, 10.2, 14.1, 12.0, 30.5, 22.0, 12.4, 11.0, 13.2, 12.4))))
        MachineScreen(MachineState(Load.Ready(s), Load.Ready(listOf(
            MaintenanceTask("npm-cache", "npm cache", "Downloaded packages npm keeps for later installs", bytes = 1_288_490_188),
            MaintenanceTask("journal", "System journal", "Old systemd logs beyond the last two weeks", elevated = true, bytes = 402_653_184),
            MaintenanceTask("docker-stopped", "Stopped containers", "Containers that have exited", danger = true, bytes = 800_000),
        ))))
    }

    @Test fun settingsLogs() = shoot("settings-logs") {
        LogsScreen(LogsState(Load.Ready(FakeApi.model<List<ServerLog>>("server-logs", "logs") + listOf(
            ServerLog(1792322005000, "info", "[claude] Chat a1b2c3d4 started in crundi"),
            ServerLog(1792322011000, "info", "[services] crundi:docs running (pid 48121)"),
            ServerLog(1792322030000, "error", "[forwards] old-demo: nothing answers on 4900"),
            ServerLog(1792322042000, "info", "[push] Sent to Pixel 7"),
        ))))
    }

    @Test fun settingsSessions() = shoot("settings-sessions") {
        SessionsScreen(SessionsState(Load.Ready(FakeApi.model<List<LoginSession>>("sessions", "sessions")), now = now))
    }
}
