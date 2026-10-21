package mv.obitech.crundi.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import mv.obitech.crundi.ui.planning.KanbanCallbacks
import mv.obitech.crundi.ui.planning.KanbanPopup
import mv.obitech.crundi.ui.planning.KanbanScreen
import mv.obitech.crundi.ui.planning.KanbanView
import mv.obitech.crundi.ui.planning.Load
import mv.obitech.crundi.ui.planning.TaskEditorCard
import mv.obitech.crundi.ui.planning.WithProject
import mv.obitech.crundi.ui.shell.Area
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.planning.MindMapScreen
import mv.obitech.crundi.ui.planning.MindPopup
import mv.obitech.crundi.ui.planning.NoteEditor
import mv.obitech.crundi.ui.planning.NotePageScreen
import mv.obitech.crundi.ui.planning.NotesListScreen
import mv.obitech.crundi.ui.planning.IdeaEditCard
import mv.obitech.crundi.api.model.FactoryDetail
import mv.obitech.crundi.api.model.FactoryValidation
import mv.obitech.crundi.api.model.RunPrecheck
import mv.obitech.crundi.ui.planning.*
import org.junit.Test

/** Kanban, notes, mind map, schedules and factories at phone size. */
class PlanningScreens : ScreenshotTest() {
    private val board = PlanningFakes.board

    // ─── Kanban ───
    @Test fun kanbanBoard() = shoot("kanban-board") { KanbanScreen(Load.Ready(board), KanbanView.Board, {}) }
    @Test fun kanbanEmpty() = shoot("kanban-empty") { KanbanScreen(Load.Ready(board.copy(tasks = emptyList(), deletedTasks = emptyList(), history = emptyList())), KanbanView.Board, {}) }
    @Test fun kanbanTrash() = shoot("kanban-trash") { KanbanScreen(Load.Ready(board), KanbanView.Trash, {}) }
    @Test fun kanbanHistory() = shoot("kanban-history") { KanbanScreen(Load.Ready(board), KanbanView.History, {}) }
    @Test fun kanbanError() = shoot("kanban-error") { KanbanScreen(Load.Failed("Could not reach dev.faruhaan.com."), KanbanView.Board, {}) }
    @Test fun kanbanEdit() = shoot("kanban-edit-task") {
        KanbanScreen(Load.Ready(board), KanbanView.Board, {})
        Box(Modifier.fillMaxSize().background(CrundiColors.Scrim), contentAlignment = Alignment.Center) {
            TaskEditorCard("Edit task", board.statuses, "in_progress", board.tasks[0].title, board.tasks[0].description, "Save", withDescription = true, onDismiss = {}) { _, _, _ -> }
        }
    }
    @Test fun kanbanNoProject() = shoot("kanban-no-project", services = FakeServices.default(FakeServices.shellData.copy(currentProject = null))) {
        WithProject(Area.Kanban) {}
    }

    // ─── Notes ───
    private val notes = PlanningFakes.notes
    @Test fun notesList() = shoot("notes-list") { NotesListScreen(Load.Ready(notes), false, {}, "", {}) }
    @Test fun notesTrash() = shoot("notes-trash") { NotesListScreen(Load.Ready(notes), true, {}, "", {}) }
    @Test fun notesEmpty() = shoot("notes-empty") { NotesListScreen(Load.Ready(emptyList()), false, {}, "", {}) }
    @Test fun notesPage() = shoot("notes-page") { NotePageScreen(Load.Ready(NoteEditor(PlanningFakes.page))) }
    @Test fun notesPageEditing() = shoot("notes-page-editing") {
        val ed = NoteEditor(PlanningFakes.page).apply { focus("b4"); touched() }
        NotePageScreen(Load.Ready(ed))
    }
    @Test fun notesPageTrash() = shoot("notes-page-trash") { NotePageScreen(Load.Ready(NoteEditor(PlanningFakes.page.copy(deleted = true)))) }
    @Test fun notesPageError() = shoot("notes-page-error") { NotePageScreen(Load.Failed("This page could not be opened.")) }

    // ─── Mind map ───
    private val mm = PlanningFakes.mindmap
    private val projects = listOf("crundi", "dhivo-socials", "ledger")
    @Test fun mindmap() = shoot("mindmap-map") { MindMapScreen(Load.Ready(mm), projects, "crundi") }
    @Test fun mindmapSelected() = shoot("mindmap-selected") { MindMapScreen(Load.Ready(mm), projects, "crundi", initialSelected = "mm6") }
    @Test fun mindmapFilter() = shoot("mindmap-filter") { MindMapScreen(Load.Ready(mm), projects, "crundi", initialFilter = "session") }
    @Test fun mindmapEmpty() = shoot("mindmap-empty") { MindMapScreen(Load.Ready(mm.copy(nodes = emptyList())), projects, "crundi") }
    @Test fun mindmapEdit() = shoot("mindmap-edit-idea") {
        MindMapScreen(Load.Ready(mm), projects, "crundi")
        Box(Modifier.fillMaxSize().background(CrundiColors.Scrim), contentAlignment = Alignment.Center) { IdeaEditCard(mm.nodes[0], {}) { _, _ -> } }
    }

    // ─── Schedules ───
    private val sched = PlanningFakes.schedules
    @Test fun schedulesList() = shoot("schedules-list") { SchedulesScreen(Load.Ready(sched), "crundi", false, {}) }
    @Test fun schedulesEmpty() = shoot("schedules-empty") { SchedulesScreen(Load.Ready(sched.copy(schedules = emptyList())), "crundi", true, {}) }
    @Test fun schedulesEditChat() = shoot("schedules-edit-chat") {
        ScheduleEditScreen(Load.Ready(ScheduleForm(sched.schedules[0])), projects, sched.services, PlanningFakes.board)
    }
    @Test fun schedulesEditService() = shoot("schedules-edit-service") {
        ScheduleEditScreen(Load.Ready(ScheduleForm(sched.schedules[2])), projects, sched.services, PlanningFakes.board, message = "Pick at least one day")
    }
    @Test fun schedulesNew() = shoot("schedules-new") { ScheduleEditScreen(Load.Ready(ScheduleForm(ScheduleForm.blank("crundi"))), projects, sched.services, PlanningFakes.board) }

    // ─── Factories ───
    private val fdata = FactoriesData(Load.Ready(PlanningFakes.summaries), PlanningFakes.templates, Load.Ready(PlanningFakes.runs), Load.Ready(PlanningFakes.triggers), Load.Ready(PlanningFakes.cleanup))
    @Test fun factoriesLibrary() = shoot("factories-library") { FactoriesScreen(fdata, FactoriesTab.Library, {}, "crundi") }
    @Test fun factoriesRuns() = shoot("factories-runs") { FactoriesScreen(fdata, FactoriesTab.Runs, {}, "crundi") }
    @Test fun factoriesTriggers() = shoot("factories-triggers") { FactoriesScreen(fdata, FactoriesTab.Triggers, {}, "crundi") }
    @Test fun factoriesCleanup() = shoot("factories-cleanup") { FactoriesScreen(fdata, FactoriesTab.Cleanup, {}, "crundi") }
    @Test fun factoriesRunsEmpty() = shoot("factories-runs-empty") { FactoriesScreen(fdata.copy(runs = Load.Ready(emptyList())), FactoriesTab.Runs, {}, "crundi") }
    @Test fun factoryDetail() = shoot("factories-factory") { FactoryDetailScreen(Load.Ready(FactoryDetail(PlanningFakes.factory, FactoryValidation()))) }
    @Test fun factoryRun() = shoot("factories-run") { FactoryRunScreen(Load.Ready(PlanningFakes.run)) }
    @Test fun factoryRunAsk() = shoot("factories-run-ask") { FactoryRunScreen(Load.Ready(PlanningFakes.runAsk)) }
    @Test fun factoryRunStep() = shoot("factories-run-step") { FactoryRunScreen(Load.Ready(PlanningFakes.run), initialSelected = "n_api") }
    @Test fun factoryRunTimeline() = shoot("factories-run-timeline") { FactoryRunScreen(Load.Ready(PlanningFakes.run), initialTab = RunTab.Timeline) }
    @Test fun factoryRunFinished() = shoot("factories-run-finished") { FactoryRunScreen(Load.Ready(PlanningFakes.runFinished)) }
    @Test fun factoryRunCrashed() = shoot("factories-run-crashed") { FactoryRunScreen(Load.Ready(PlanningFakes.runs[3])) }
    @Test fun factoryStart() = shoot("factories-start") {
        val form = StartForm("fac_android", "crundi").apply { title = "Usage chart per project"; sections["goal"] = "Weekly bars per project on the Usage tab." }
        StartRunScreen(form, Load.Ready(PlanningFakes.summaries), projects, RunPrecheck(branch = "dev", commit = "7c74f58", dirty = true))
    }
    @Test fun factoryTrigger() = shoot("factories-trigger") { TriggerEditScreen(Load.Ready(PlanningFakes.triggers[0] to PlanningFakes.summaries), "dev.faruhaan.com") }
    @Test fun factoryTriggerWebhook() = shoot("factories-trigger-webhook") { TriggerEditScreen(Load.Ready(PlanningFakes.triggers[2] to PlanningFakes.summaries), "dev.faruhaan.com") }

    // ─── Sheets (drawn without their window) ───
    private fun sheet(name: String, title: String, subtitle: String? = null, under: @androidx.compose.runtime.Composable () -> Unit, body: @androidx.compose.runtime.Composable androidx.compose.foundation.layout.ColumnScope.() -> Unit) = shoot(name) {
        under()
        Box(Modifier.fillMaxSize().background(CrundiColors.Scrim)) { SheetCard(title, {}, Modifier.align(Alignment.BottomCenter), subtitle, content = body) }
    }
    @Test fun kanbanTaskMenu() = sheet("kanban-task-menu", board.tasks[0].title, "In Progress", { KanbanScreen(Load.Ready(board), KanbanView.Board, {}) }) {
        TaskMenuItems(board.tasks[0], board.statuses, KanbanCallbacks(), {}, {}, {})
    }
    @Test fun mindmapDetails() = sheet("mindmap-idea", mm.nodes[0].text, under = { MindMapScreen(Load.Ready(mm), projects, "crundi") }) {
        IdeaDetails(mm, mm.nodes[0], MindMapCallbacks(), {}, androidx.compose.runtime.mutableStateListOf())
    }
    @Test fun mindmapLink() = sheet("mindmap-link", "Scope to a project \u2014 or link a task", under = { MindMapScreen(Load.Ready(mm), projects, "crundi") }) {
        LinkPicker(mm, mm.nodes[8], projects, "crundi", mapOf("crundi" to Load.Ready(board)), MindMapCallbacks()) {}
    }
}
