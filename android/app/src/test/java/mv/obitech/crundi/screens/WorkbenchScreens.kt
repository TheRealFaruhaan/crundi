package mv.obitech.crundi.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import mv.obitech.crundi.api.model.Pane
import mv.obitech.crundi.ui.components.MarkdownText
import mv.obitech.crundi.ui.shell.AddMenuBody
import mv.obitech.crundi.ui.shell.Area
import mv.obitech.crundi.ui.shell.CrundiApp
import mv.obitech.crundi.ui.shell.Host
import mv.obitech.crundi.ui.shell.ShellCallbacks
import mv.obitech.crundi.ui.shell.ShellScaffold
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.Dimens
import mv.obitech.crundi.ui.workbench.LocalWorkbenchStore
import mv.obitech.crundi.ui.workbench.Mosaic
import mv.obitech.crundi.ui.workbench.PaneActions
import mv.obitech.crundi.ui.workbench.PaneFrame
import mv.obitech.crundi.ui.workbench.PaneKind
import mv.obitech.crundi.ui.workbench.PaneMenuBody
import mv.obitech.crundi.ui.workbench.PaneView
import mv.obitech.crundi.ui.workbench.PendingCell
import mv.obitech.crundi.ui.workbench.WbCell
import mv.obitech.crundi.ui.workbench.WbLayout
import mv.obitech.crundi.ui.workbench.WorkbenchActions
import mv.obitech.crundi.ui.workbench.WorkbenchScreen
import mv.obitech.crundi.ui.workbench.WorkbenchStore
import mv.obitech.crundi.ui.workbench.WorkbenchUi
import mv.obitech.crundi.ui.workbench.assign
import mv.obitech.crundi.ui.workbench.leafOf
import mv.obitech.crundi.ui.workbench.leaves
import mv.obitech.crundi.ui.workbench.splitLeaf
import org.junit.Test

/**
 * The Workbench (change 2): the whole app through the real NavHost and the
 * real pane screens, with the layout seeded in the phone's store; and the
 * stateless pieces (mid-swipe, full screen, menus) drawn on their own.
 */
class WorkbenchScreens : ScreenshotTest() {
    private val chat1 = "t:a1b2c3d4e5f60718"
    private val chat2 = "t:b2c3d4e5f6071829"

    /** The app on the Workbench, with [seed] laid out in the store for [project]. */
    private fun app(name: String, project: String = "crundi", addMenu: Boolean = false, seed: (WorkbenchStore) -> Unit = {}) {
        val store = WorkbenchStore.memory().also(seed)
        shoot(name, FakeServices.default(FakeServices.shellData.copy(currentProject = project))) {
            CompositionLocalProvider(LocalWorkbenchStore provides store) {
                CrundiApp(services = mv.obitech.crundi.ui.shell.LocalAppServices.current, host = Host.None, addMenuOpenAtStart = addMenu)
            }
        }
    }

    private fun cell(store: WorkbenchStore, project: String, kind: PaneKind, data: Map<String, String> = emptyMap()) {
        val c = WbCell("c" + kind.key, kind, data)
        store.setCells(project, listOf(c))
        store.setTree(project, Mosaic.leaf(c.key))
    }

    @Test fun columns() = app("workbench-columns")

    @Test fun split() = app("workbench-split") { st ->
        val c = WbCell("cf", PaneKind.Files)
        st.setCells("crundi", listOf(c))
        var t: Mosaic = Mosaic.split(Mosaic.Dir.Row, listOf(Mosaic.leaf(chat1), Mosaic.leaf(chat2)))
        t = t.splitLeaf(t.leafOf(chat1)!!.id, Mosaic.Dir.Col)
        st.setTree("crundi", t.assign(t.leaves().first { it.key == null }.id, c.key))
    }

    @Test fun emptyProject() = app("workbench-empty", project = "dhivo-socials")

    @Test fun stack() = app("workbench-stack") { it.layout = WbLayout.Stack }

    @Test fun parked() = app("workbench-pane-parked", project = "ledger")

    @Test fun terminal() = app("workbench-pane-terminal", project = "faruhaan.com")

    @Test fun pending() = app("workbench-pane-new-terminal") { st ->
        st.setPending("crundi", listOf(PendingCell("n1")))
        st.setTree("crundi", Mosaic.leaf("p:n1"))
    }

    @Test fun kinds() {
        for (k in listOf(PaneKind.Files, PaneKind.Git, PaneKind.Kanban, PaneKind.Notes, PaneKind.MindMap, PaneKind.Media, PaneKind.Factory, PaneKind.Browser, PaneKind.Panel)) {
            app("workbench-pane-${k.key}") { cell(it, "crundi", k) }
        }
    }

    @Test fun noProject() = shoot("shell-no-project", FakeServices.default(FakeServices.shellData.copy(currentProject = null, projects = emptyList()))) {
        CrundiApp(services = mv.obitech.crundi.ui.shell.LocalAppServices.current, host = Host.None)
    }

    // ─── Stateless pieces ───

    private val fakeChat = PaneView(chat1, PaneKind.Chat, "Factory: Android team", mv.obitech.crundi.api.model.AgentState.Working, pane = Pane("a1", "crundi", "Factory: Android team", agentState = "working", status = "running"))
    private val fakeFactory = PaneView("w:f", PaneKind.Factory, "Factory", cell = WbCell("f", PaneKind.Factory))

    @Composable
    private fun FakeBody(v: PaneView) {
        Column(Modifier.fillMaxSize().padding(14.dp)) {
            MarkdownText(
                if (v.kind == PaneKind.Chat) "My recommendation is to add the Workbench as the place a project opens:\n\n- swipeable panes\n- a + to add a chat, terminal, files or git pane\n- widgets shown as panes\n- the top bar kept on screen.\n\nShall I send that to the team?"
                else "**Android team** is running.\n\nManager: delegating to the chats developer.",
            )
        }
    }

    @Test fun midSwipe() = shoot("workbench-mid-swipe") {
        val tree = Mosaic.split(Mosaic.Dir.Row, listOf(Mosaic.leaf(fakeChat.key), Mosaic.leaf(fakeFactory.key)))
        val ui = WorkbenchUi("crundi", tree, mapOf(fakeChat.key to fakeChat, fakeFactory.key to fakeFactory), focused = fakeChat.key)
        WorkbenchScreen(ui, WorkbenchActions.None, pager = rememberPagerState(0, 0.45f) { 2 }) { FakeBody(it) }
    }

    @Test fun fullScreen() = shoot("workbench-full-screen") {
        PaneFrame(fakeChat, PaneActions.None, Modifier.fillMaxSize(), focused = true, full = true) { FakeBody(fakeChat) }
    }

    @Test fun addMenu() = shoot("workbench-add-menu") {
        Box(Modifier.fillMaxSize()) {
            ShellScaffold(FakeServices.shellData, Area.Workbench, true, ShellCallbacks(), drawerOpen = false, onDrawer = {}) {
                WorkbenchScreen(WorkbenchUi("crundi", Mosaic.leaf(fakeChat.key), mapOf(fakeChat.key to fakeChat), focused = fakeChat.key), WorkbenchActions.None) { FakeBody(it) }
            }
            Box(Modifier.offset(x = 60.dp, y = 92.dp).width(androidx.compose.foundation.layout.IntrinsicSize.Max).background(CrundiColors.BgSecondary, Dimens.Shape).border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape)) { AddMenuBody(ShellCallbacks()) }
        }
    }

    @Test fun paneMenu() = shoot("workbench-pane-menu") {
        Box(Modifier.fillMaxSize()) {
            WorkbenchScreen(WorkbenchUi("crundi", Mosaic.leaf(fakeChat.key), mapOf(fakeChat.key to fakeChat), focused = fakeChat.key), WorkbenchActions.None) { FakeBody(it) }
            Box(Modifier.align(Alignment.TopEnd).offset(x = (-12).dp, y = 52.dp).width(androidx.compose.foundation.layout.IntrinsicSize.Max).background(CrundiColors.BgSecondary, Dimens.Shape).border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape)) { PaneMenuBody(fakeChat, PaneActions.None) }
        }
    }
}
