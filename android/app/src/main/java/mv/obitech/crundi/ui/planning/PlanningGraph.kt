package mv.obitech.crundi.ui.planning

import androidx.compose.runtime.Composable
import androidx.navigation.NavGraphBuilder
import androidx.navigation.compose.composable
import mv.obitech.crundi.ui.shell.Area
import mv.obitech.crundi.ui.shell.AreaPlaceholder

/** Owned by the planning developer: Kanban, Notes, MindMap, Factories, Schedules. Add detail routes here, prefixed with the area key. */
fun NavGraphBuilder.planningGraph() {
    composable(Area.Kanban.key) { KanbanScreen() }
    composable(Area.Notes.key) { NotesScreen() }
    composable(Area.MindMap.key) { MindMapScreen() }
    composable(Area.Factories.key) { FactoriesScreen() }
    composable(Area.Schedules.key) { SchedulesScreen() }
}

@Composable fun KanbanScreen() = AreaPlaceholder(Area.Kanban)
@Composable fun NotesScreen() = AreaPlaceholder(Area.Notes)
@Composable fun MindMapScreen() = AreaPlaceholder(Area.MindMap)
@Composable fun FactoriesScreen() = AreaPlaceholder(Area.Factories)
@Composable fun SchedulesScreen() = AreaPlaceholder(Area.Schedules)
