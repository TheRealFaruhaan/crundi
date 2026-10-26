package mv.obitech.crundi.ui.planning

import androidx.navigation.NavBackStackEntry
import androidx.navigation.NavGraphBuilder
import androidx.navigation.compose.composable
import androidx.navigation.navArgument
import mv.obitech.crundi.ui.shell.Area

/** Owned by the planning developer: Kanban, Notes, MindMap, Factories, Schedules. Detail routes are prefixed with the area key. */
fun NavGraphBuilder.planningGraph() {
    composable(Area.Kanban.key) { KanbanRoute() }

    composable(Area.Notes.key) { NotesRoute() }
    composable(NotesRoutes.PAGE) { NotePageRoute(it.arg("id")) }

    composable(Area.MindMap.key) { MindMapRoute() }

    composable(Area.Schedules.key) { SchedulesRoute() }
    composable(ScheduleRoutes.EDIT, arguments = listOf(navArgument("id") { defaultValue = "" })) { ScheduleEditRoute(it.arg("id")) }

    composable(Area.Factories.key) { FactoriesRoute() }
    composable(FactoryRoutes.RUN) { FactoryRunRoute(it.arg("id")) }
    composable(FactoryRoutes.DIFF) { RunDiffRoute(it.arg("id")) }
    composable(FactoryRoutes.FACTORY) { FactoryDetailRoute(it.arg("id")) }
    composable(FactoryRoutes.EDIT, arguments = listOf(navArgument("focus") { defaultValue = "" })) { FactoryEditRoute(it.arg("id"), it.arg("focus")) }
    composable(FactoryRoutes.START, arguments = listOf(navArgument("factory") { defaultValue = "" })) { StartRunRoute(it.arg("factory")) }
    composable(FactoryRoutes.TRIGGER, arguments = listOf(navArgument("id") { defaultValue = "" })) { TriggerEditRoute(it.arg("id")) }
}

private fun NavBackStackEntry.arg(name: String): String = java.net.URLDecoder.decode(arguments?.getString(name).orEmpty(), "UTF-8")
