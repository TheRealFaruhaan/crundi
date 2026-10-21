package mv.obitech.crundi.ui.operations

import androidx.compose.runtime.Composable
import androidx.navigation.NavGraphBuilder
import androidx.navigation.compose.composable
import mv.obitech.crundi.ui.shell.Area
import mv.obitech.crundi.ui.shell.AreaPlaceholder

/** Owned by the operations developer: Services, Forwards, Docker, Secrets, Usage, Settings. Add detail routes here, prefixed with the area key. */
fun NavGraphBuilder.operationsGraph() {
    composable(Area.Services.key) { ServicesScreen() }
    composable(Area.Forwards.key) { ForwardsScreen() }
    composable(Area.Docker.key) { DockerScreen() }
    composable(Area.Secrets.key) { SecretsScreen() }
    composable(Area.Usage.key) { UsageScreen() }
    composable(Area.Settings.key) { SettingsScreen() }
}

@Composable fun ServicesScreen() = AreaPlaceholder(Area.Services)
@Composable fun ForwardsScreen() = AreaPlaceholder(Area.Forwards)
@Composable fun DockerScreen() = AreaPlaceholder(Area.Docker)
@Composable fun SecretsScreen() = AreaPlaceholder(Area.Secrets)
@Composable fun UsageScreen() = AreaPlaceholder(Area.Usage)
@Composable fun SettingsScreen() = AreaPlaceholder(Area.Settings)
