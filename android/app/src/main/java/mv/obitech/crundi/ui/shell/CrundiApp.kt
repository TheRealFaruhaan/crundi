package mv.obitech.crundi.ui.shell

import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import mv.obitech.crundi.ui.chats.chatsGraph
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.errorText
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.nav.Navigator
import mv.obitech.crundi.ui.nav.Routes
import mv.obitech.crundi.ui.operations.operationsGraph
import mv.obitech.crundi.ui.planning.planningGraph
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.workbench.LocalWorkbench
import mv.obitech.crundi.ui.workbench.LocalWorkbenchStore
import mv.obitech.crundi.ui.workbench.rememberWorkbench
import mv.obitech.crundi.ui.workbench.workbenchGraph
import mv.obitech.crundi.ui.workspace.workspaceGraph

/**
 * The main window's whole UI, as the mobile web: the shell round one NavHost
 * that opens on the Workbench. This is the only NavHost in the app (the
 * bubble shows one chat with no navigation). Each area package adds its
 * routes through its own *Graph function; nothing else registers routes.
 *
 * Back: closes the drawer first; on a tab's front page the first back says
 * "Press back again to exit" and a second within two seconds leaves.
 *
 * `openRoute`: a route to go to once (a tapped notification); `onOpened`
 * says it has been taken so it is not opened again.
 */
@Composable
fun CrundiApp(
    services: AppServices,
    host: Host,
    modifier: Modifier = Modifier,
    nav: NavHostController = rememberNavController(),
    openRoute: String? = null,
    onOpened: () -> Unit = {},
    drawerOpenAtStart: Boolean = false,
    addMenuOpenAtStart: Boolean = false,
    approvalsOpenAtStart: Boolean = false,
) {
    val navigator = remember(nav) { Navigator(nav) }
    val data by services.shell.state.collectAsStateWithLifecycle()
    val entry by nav.currentBackStackEntryAsState()
    val route = entry?.destination?.route
    var drawer by remember { mutableStateOf(drawerOpenAtStart) }
    var addingProject by remember { mutableStateOf(false) }
    var approvalsOpen by remember { mutableStateOf(approvalsOpenAtStart) }
    var panelsOpen by remember { mutableStateOf(false) }
    val wb = rememberWorkbench(services.api, data.currentProject, LocalWorkbenchStore.current, host)
    LaunchedEffect(openRoute) {
        if (openRoute != null) { navigator.to(openRoute); onOpened() }
    }
    // Presence, reconnect on resume, pair again when signed out (API engineer).
    mv.obitech.crundi.api.LiveClientEffects(services.api, data.currentProject, host)

    val atRoot = route == null || Routes.isAreaRoot(route)
    val ctx = LocalContext.current
    var lastBack by remember { mutableLongStateOf(0L) }
    BackHandler(enabled = atRoot && !drawer && !approvalsOpen && wb.full == null) {
        val now = System.currentTimeMillis()
        if (now - lastBack < 2000) host.close()
        else { lastBack = now; Toast.makeText(ctx.applicationContext, "Press back again to exit", Toast.LENGTH_SHORT).show() }
    }

    // What's new: offered once per launch per server, once the handshake is
    // done (this is only composed after it) and the live connection is up.
    // The server says whether there is anything; nothing new, an error or an
    // older server without it shows nothing.
    val tour = remember(services) { mv.obitech.crundi.ui.operations.WhatsNewTour.of(services) }
    LaunchedEffect(tour) {
        if (tour.offered) return@LaunchedEffect
        kotlinx.coroutines.withTimeoutOrNull(20_000) { services.shell.state.first { it.connection == Connection.Connected } }
        kotlinx.coroutines.delay(1600)
        tour.autoShow()
    }

    // Mini's stage: one layer over the whole app, where the robot walks (ui.mini).
    mv.obitech.crundi.ui.mini.MiniStage(Modifier.fillMaxSize()) {
    CompositionLocalProvider(LocalAppServices provides services, LocalHost provides host, LocalNavigator provides navigator, LocalWorkbench provides wb, mv.obitech.crundi.ui.operations.LocalWhatsNew provides tour) {
        ShellScaffold(
            data = data,
            current = Routes.areaOf(route),
            showChrome = atRoot,
            callbacks = ShellCallbacks(
                onArea = navigator::area,
                onProject = { services.shell.selectProject(it); navigator.area(Area.Workbench) },
                onApprovals = { drawer = false; approvalsOpen = !approvalsOpen },
                onUsage = { navigator.to(Area.Usage.key) },
                onPanels = { drawer = false; panelsOpen = true },
                onAddProject = { addingProject = true },
                onAppSettings = host::openAppSettings,
                onPairAnother = host::pairAnother,
                onAdd = { navigator.area(Area.Workbench); wb.add(it) },
                onFit = wb::fit,
                onToggleLayout = wb::toggleLayout,
                layout = wb.layout,
            ),
            drawerOpen = drawer,
            onDrawer = { drawer = it },
            addMenuOpen = addMenuOpenAtStart,
            overlay = { if (approvalsOpen && atRoot) mv.obitech.crundi.ui.chats.ApprovalsDropdownRoute { approvalsOpen = false } },
            modifier = modifier,
        ) {
            NavHost(navController = nav, startDestination = Area.Workbench.key) {
                workbenchGraph()
                chatsGraph()
                workspaceGraph()
                planningGraph()
                operationsGraph()
            }
        }
        // The top bar's Panels button: its chips and their panels (ui.panels).
        if (panelsOpen) mv.obitech.crundi.ui.panels.PanelChipsPopover(onDismiss = { panelsOpen = false }, onOpenTab = { navigator.area(Area.Panels) })
        // What's new, over everything but a secret request (ui.operations.WhatsNew).
        mv.obitech.crundi.ui.operations.WhatsNewOverlay(tour)
        // Claude asking to use a secret: a card over every screen, answered with the PIN (ui.operations).
        mv.obitech.crundi.ui.operations.SecretPromptRoute()
        if (addingProject) AddProjectDialog(services, onDone = { alias ->
            addingProject = false
            if (alias != null) { services.shell.selectProject(alias); navigator.area(Area.Workbench) }
        })
    }
    }
}

/** "Add project" from the drawer: a name, and the folder if it is not the default one. */
@Composable
private fun AddProjectDialog(services: AppServices, onDone: (String?) -> Unit) {
    var alias by remember { mutableStateOf("") }
    var path by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    CrundiDialog(
        "Add project", onDismiss = { onDone(null) },
        message = "A short name for it, and its folder on the server. Leave the folder empty to make a new one in the projects folder.",
        confirm = if (busy) "Adding…" else "Add project",
        onConfirm = confirm@{
            val a = alias.trim().lowercase()
            if (!Regex("^[a-z0-9][a-z0-9._-]{0,63}$").matches(a)) { error = "Use letters, digits, dots, dashes or underscores."; return@confirm }
            busy = true
            scope.launch {
                try {
                    services.api.projects.add(a, path = path.trim().ifEmpty { null }, create = if (path.isBlank()) true else null)
                    onDone(a)
                } catch (e: Exception) { error = errorText(e); busy = false }
            }
        },
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiTextField(alias, { alias = it; error = null }, placeholder = "Name, e.g. my-app", mono = true, modifier = Modifier.fillMaxWidth())
            CrundiTextField(path, { path = it }, placeholder = "/home/you/projects/my-app (optional)", mono = true, modifier = Modifier.fillMaxWidth())
            error?.let { Text(it, style = CrundiType.Small.copy(color = mv.obitech.crundi.ui.theme.CrundiColors.Red)) }
        }
    }
}
