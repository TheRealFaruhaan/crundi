package mv.obitech.crundi.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import mv.obitech.crundi.api.model.Widget
import mv.obitech.crundi.api.model.WidgetDoc
import mv.obitech.crundi.api.model.WidgetVersion
import mv.obitech.crundi.ui.components.DialogCard
import mv.obitech.crundi.ui.operations.Load
import mv.obitech.crundi.ui.panels.ChipsPopoverCard
import mv.obitech.crundi.ui.panels.GrantBanner
import mv.obitech.crundi.ui.panels.PanelDock
import mv.obitech.crundi.ui.panels.PanelFrame
import mv.obitech.crundi.ui.panels.PanelPane
import mv.obitech.crundi.ui.panels.PanelPicker
import mv.obitech.crundi.ui.panels.PanelShot
import mv.obitech.crundi.ui.panels.PanelsScreen
import mv.obitech.crundi.ui.panels.PanelsState
import mv.obitech.crundi.ui.panels.SlotOptions
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import org.junit.Test

/**
 * Panels (widgets): the Panels tab, a Panel pane's picker, a chat's dock, the
 * top bar's chip popover and the approval banner. The panel's own HTML is a
 * WebView, which a screenshot cannot draw: PanelFrame shows a still stand-in
 * where it goes (inspection mode), so these images show everything native.
 */
class PanelsScreens : ScreenshotTest() {
    private val widgets: List<Widget> = FakeApi.model("widgets", "widgets")
    private val doc: WidgetDoc = FakeApi.model("widget-doc")
    private val versions: List<WidgetVersion> = FakeApi.model("widget-versions", "versions")
    private val chipValues = mapOf("test-count" to "412 passed", "usage-gauge" to "46%")

    @Composable
    private fun InDialog(content: @Composable () -> Unit) {
        Box(Modifier.fillMaxSize().background(CrundiColors.Scrim).padding(16.dp), contentAlignment = Alignment.Center) { content() }
    }

    @Test fun tab() = shoot("panels-tab") {
        PanelsScreen(PanelsState("crundi", Load.Ready(widgets), "build-board", chipValues = chipValues))
    }

    @Test fun tabGrant() = shoot("panels-tab-approve") {
        // The selected panel waits for approval: the banner sits above its frame.
        Column(Modifier.fillMaxSize()) {
            PanelsScreen(PanelsState("crundi", Load.Ready(widgets.filter { !it.isChip }), "deploy-log"))
        }
    }

    @Test fun history() = shoot("panels-history") {
        PanelsScreen(PanelsState("crundi", Load.Ready(widgets), "build-board", mv.obitech.crundi.ui.panels.PanelPane.History, chipValues, versions = Load.Ready(versions)))
    }

    @Test fun checks() = shoot("panels-checks") {
        PanelsScreen(PanelsState("crundi", Load.Ready(widgets), "build-board", mv.obitech.crundi.ui.panels.PanelPane.Checks, chipValues,
            shots = Load.Ready(listOf(PanelShot("cell", "live"), PanelShot("dock", "empty")))))
    }

    @Test fun closed() = shoot("panels-closed") {
        PanelsScreen(PanelsState("crundi", Load.Ready(widgets), "old-chart", chipValues = chipValues))
    }

    @Test fun empty() = shoot("panels-empty") {
        PanelsScreen(PanelsState("dhivo-socials", Load.Ready(emptyList())))
    }

    @Test fun globalEmpty() = shoot("panels-global-empty") {
        PanelsScreen(PanelsState("", Load.Ready(emptyList())))
    }

    @Test fun error() = shoot("panels-error") {
        PanelsScreen(PanelsState("crundi", Load.Failed("The server cannot be reached. Check the connection and try again.")))
    }

    @Test fun slot() = shoot("panels-slot") {
        Box(Modifier.fillMaxSize().background(CrundiColors.Scrim).padding(16.dp), contentAlignment = Alignment.Center) {
            DialogCard("Where this panel sits", dismiss = "Cancel") { SlotOptions(widgets.first(), {}) }
        }
    }

    @Test fun picker() = shoot("panels-pane-picker") {
        PanelPicker(Load.Ready(widgets), "crundi")
    }

    @Test fun pane() = shoot("panels-pane") {
        PanelFrame("crundi", "build-board", "cell", Modifier.fillMaxSize())
    }

    @Test fun dock() = shoot("panels-dock") {
        Column(Modifier.fillMaxSize()) {
            PanelDock("crundi", widgets.filter { it.slot == "dock" } + widgets.first().copy(id = "ci-status", title = "CI status", slot = "dock", collapsed = true), 380.dp)
            Box(Modifier.weight(1f).fillMaxWidth().padding(16.dp)) {
                Text("The chat's conversation goes on below the dock.", style = CrundiType.Small)
            }
        }
    }

    @Test fun grant() = shoot("panels-grant") {
        Column(Modifier.fillMaxSize()) { GrantBanner(doc.grant, {}, {}) }
    }

    @Test fun chips() = shoot("panels-chips") {
        ChipsPopoverCard(widgets.filter { it.isChip }, chipValues, null, "crundi")
    }

    @Test fun chipOpen() = shoot("panels-chip-open") {
        ChipsPopoverCard(widgets.filter { it.isChip }, chipValues, widgets.first { it.id == "test-count" }, "crundi")
    }
}
