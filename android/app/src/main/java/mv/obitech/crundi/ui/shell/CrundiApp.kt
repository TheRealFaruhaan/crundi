package mv.obitech.crundi.ui.shell

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import mv.obitech.crundi.ui.chats.chatsGraph
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.nav.Navigator
import mv.obitech.crundi.ui.nav.Routes
import mv.obitech.crundi.ui.operations.operationsGraph
import mv.obitech.crundi.ui.planning.planningGraph
import mv.obitech.crundi.ui.workspace.workspaceGraph

/**
 * The main window's whole UI: shell round one NavHost. This is the only
 * NavHost in the app (the bubble shows one chat with no navigation). Each
 * area package adds its routes through its own *Graph function; nothing else
 * registers routes.
 *
 * `openRoute`: a route to go to once (a tapped notification); `onOpened`
 * says it has been taken so it is not opened again.
 */
@Composable
fun CrundiApp(
    services: AppServices,
    host: mv.obitech.crundi.ui.shell.Host,
    modifier: Modifier = Modifier,
    nav: NavHostController = rememberNavController(),
    openRoute: String? = null,
    onOpened: () -> Unit = {},
) {
    val navigator = remember(nav) { Navigator(nav) }
    val data by services.shell.state.collectAsStateWithLifecycle()
    val entry by nav.currentBackStackEntryAsState()
    val route = entry?.destination?.route
    LaunchedEffect(openRoute) {
        if (openRoute != null) { navigator.to(openRoute); onOpened() }
    }
    // Presence, reconnect on resume, pair again when signed out (API engineer).
    mv.obitech.crundi.api.LiveClientEffects(services.api, data.currentProject, host)
    CompositionLocalProvider(LocalAppServices provides services, LocalHost provides host, LocalNavigator provides navigator) {
        ShellScaffold(
            data = data,
            current = Routes.areaOf(route),
            showChrome = route == null || Routes.isAreaRoot(route),
            onArea = navigator::area,
            onProject = services.shell::selectProject,
            onApprovals = { navigator.to(Routes.APPROVALS) },
            onAppSettings = host::openAppSettings,
            onPairAnother = host::pairAnother,
            modifier = modifier,
        ) {
            NavHost(navController = nav, startDestination = Area.Chats.key) {
                chatsGraph()
                workspaceGraph()
                planningGraph()
                operationsGraph()
            }
        }
    }
}
