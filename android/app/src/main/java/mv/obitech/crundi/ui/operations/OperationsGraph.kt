package mv.obitech.crundi.ui.operations

import androidx.navigation.NavGraphBuilder
import androidx.navigation.compose.composable
import mv.obitech.crundi.ui.shell.Area

/**
 * Owned by the operations developer: Services, Forwards, Docker, Secrets,
 * Usage, Settings. Detail routes are prefixed with the area key
 * (docker/container/{id}, settings/server, ...).
 */
fun NavGraphBuilder.operationsGraph() {
    composable(Area.Services.key) { ServicesRoute() }
    composable(Area.Forwards.key) { ForwardsRoute() }
    composable(Area.Docker.key) { DockerRoute() }
    dockerRoutes()
    composable(Area.Secrets.key) { SecretsRoute() }
    composable(Area.Usage.key) { UsageRoute() }
    composable(Area.Settings.key) { SettingsRoute() }
    composable(SettingsPage.Server.route) { ServerPageRoute() }
    composable(SettingsPage.Notifications.route) { NotificationsRoute() }
    composable(SettingsPage.Prompt.route) { PromptRoute() }
    composable(SettingsPage.General.route) { GeneralRoute() }
    composable(SettingsPage.Config.route) { ConfigRoute() }
    composable(SettingsPage.Machine.route) { MachineRoute() }
    composable(SettingsPage.Logs.route) { LogsRoute() }
    composable(SettingsPage.Sessions.route) { SessionsRoute() }
}
