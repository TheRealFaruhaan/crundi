package mv.obitech.crundi.screens

import androidx.compose.runtime.Composable
import mv.obitech.crundi.ui.chats.ChatActions
import mv.obitech.crundi.ui.chats.ChatScreen
import mv.obitech.crundi.ui.chats.ChatUi
import mv.obitech.crundi.ui.planning.FactoryLiveScreen
import mv.obitech.crundi.ui.planning.FactoryPanelScreen
import mv.obitech.crundi.ui.planning.LivePaneState
import mv.obitech.crundi.ui.planning.Load
import mv.obitech.crundi.ui.planning.OpenChat
import mv.obitech.crundi.ui.planning.PaneScaled
import mv.obitech.crundi.ui.planning.RunTab
import mv.obitech.crundi.ui.shell.Area
import mv.obitech.crundi.ui.shell.ShellCallbacks
import mv.obitech.crundi.ui.shell.ShellScaffold
import mv.obitech.crundi.ui.workbench.Mosaic
import mv.obitech.crundi.ui.workbench.PaneKind
import mv.obitech.crundi.ui.workbench.PaneView
import mv.obitech.crundi.ui.workbench.WbCell
import mv.obitech.crundi.ui.workbench.WorkbenchActions
import mv.obitech.crundi.ui.workbench.WorkbenchScreen
import mv.obitech.crundi.ui.workbench.WorkbenchUi
import org.junit.Test
import org.robolectric.annotation.Config

/**
 * The workbench's Factory pane, as the web's project panel: during a run its
 * live graph fills the pane (working-now strip, live activity on the steps,
 * the needs-you banner, Details), a person's transcript opens inside it, a
 * narrow pane's bar is icon-only and a wide one has the words; with no run
 * going, Start, earlier runs and triggers.
 */
class FactoryPaneScreens : ScreenshotTest() {
    private val now = PlanningFakes.now.toEpochMilli()

    /** The shell, the workbench and the pane frame around the screen, as PlanningScreens' panes. */
    private fun pane(name: String, body: @Composable () -> Unit) = shoot(name) {
        val v = PaneView("w:cfactory", PaneKind.Factory, PaneKind.Factory.label, cell = WbCell("cfactory", PaneKind.Factory))
        val ui = WorkbenchUi("crundi", Mosaic.leaf(v.key), mapOf(v.key to v), focused = v.key)
        ShellScaffold(FakeServices.shellData, Area.Workbench, true, ShellCallbacks(), drawerOpen = false, onDrawer = {}) {
            WorkbenchScreen(ui, WorkbenchActions.None) { PaneScaled { body() } }
        }
    }

    private val noChat = object : ChatActions {}

    /** A still chat where the app shows the person's live one. */
    private val stillChat: @Composable (OpenChat) -> Unit = { ChatScreen(ChatUi(FakeApi.chat(13), inPane = true), noChat) }

    @Test fun running() = pane("factory-pane-running") { FactoryLiveScreen(Load.Ready(PlanningFakes.runLive), now = now) }

    @Test fun crashed() = pane("factory-pane-crashed") { FactoryLiveScreen(Load.Ready(PlanningFakes.runCrashed), now = now) }

    @Test fun question() = pane("factory-pane-question") {
        FactoryLiveScreen(Load.Ready(PlanningFakes.runAsk), state = LivePaneState(drawer = true, tab = RunTab.Run), now = now)
    }

    @Test fun details() = pane("factory-pane-details") {
        FactoryLiveScreen(Load.Ready(PlanningFakes.runLive), state = LivePaneState(drawer = true, selected = "n_api"), now = now)
    }

    @Test fun transcript() = pane("factory-pane-transcript") {
        FactoryLiveScreen(Load.Ready(PlanningFakes.runLive), state = LivePaneState(chat = OpenChat("c3", "Android dev: chats")), now = now, transcript = stillChat)
    }

    /** A phone's pane is narrower than the web's 560: the bar is icons and the run's name keeps its room. */
    @Test fun narrow() = pane("factory-pane-narrow") {
        FactoryLiveScreen(Load.Ready(PlanningFakes.runLive.copy(snapshot = PlanningFakes.factory.copy(name = "Android team: the native app, every area at once"))), now = now)
    }

    /** Wide enough for the words: Follow, Details, Pause, Stop and the progress line. */
    @Config(qualifiers = "+land")
    @Test fun wide() = pane("factory-pane-wide") { FactoryLiveScreen(Load.Ready(PlanningFakes.runLive), now = now) }

    @Test fun noRun() = pane("factory-pane-no-run") {
        val past = listOf(
            PlanningFakes.runs[2],
            PlanningFakes.runs[3].copy(id = "run_e", status = "stopped"),
            PlanningFakes.runFinished.copy(id = "run_f", factoryName = "Android team", cr = mv.obitech.crundi.api.model.RunCr("Bubble page for chats")),
        )
        FactoryPanelScreen(past, Load.Ready(PlanningFakes.triggers), now = now, initialRunsOpen = true)
    }

    @Test fun noRunEver() = pane("factory-pane-no-run-empty") { FactoryPanelScreen(emptyList(), Load.Ready(emptyList()), now = now) }
}
