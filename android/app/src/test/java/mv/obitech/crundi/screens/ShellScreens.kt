package mv.obitech.crundi.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import mv.obitech.crundi.ui.components.Banner
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CodeBlock
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiCard
import mv.obitech.crundi.ui.components.CrundiChip
import mv.obitech.crundi.ui.components.CrundiSwitchRow
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.DialogCard
import mv.obitech.crundi.ui.components.DotState
import mv.obitech.crundi.ui.components.ListRow
import mv.obitech.crundi.ui.components.MarkdownText
import mv.obitech.crundi.ui.components.PanelHeader
import mv.obitech.crundi.ui.components.SectionHeader
import mv.obitech.crundi.ui.components.StateDot
import mv.obitech.crundi.ui.components.StatusBadge
import mv.obitech.crundi.ui.components.TopBar
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.icons.CrundiIcons
import mv.obitech.crundi.ui.shell.Area
import mv.obitech.crundi.ui.shell.CrundiApp
import mv.obitech.crundi.ui.shell.Host
import mv.obitech.crundi.ui.shell.ShellScaffold
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import org.junit.Test

/** The frame round the app, and the design system's pieces on their own. */
class ShellScreens : ScreenshotTest() {
    /** Every area through the real NavHost, as the person sees it after tapping its tab. */
    @Test
    fun everyArea() {
        // The Workbench (the tab the app opens on) is shot live in WorkbenchScreens, so its panes have loaded.
        for (a in Area.entries.filter { it.tab && it != Area.Workbench }) shoot("shell-${a.key}") {
            CrundiApp(services = FakeServices.default(), host = Host.None, openRoute = if (a == Area.Workbench) null else a.key)
        }
    }

    @Test
    fun drawer() = shoot("shell-drawer") {
        ShellScaffold(
            data = FakeServices.shellData,
            current = Area.Workbench,
            showChrome = true,
            callbacks = mv.obitech.crundi.ui.shell.ShellCallbacks(),
            drawerOpen = true,
            onDrawer = {},
        ) { EmptyState("terminal", "Workbench") }
    }

    @Test
    fun icons() = shoot("design-icons") {
        Column(Modifier.verticalScroll(rememberScrollState()).padding(12.dp)) {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                for (n in CrundiIcons.names.sorted()) {
                    Column(Modifier.width(74.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                        CrundiIcon(n, size = 22.dp, tint = CrundiColors.TextPrimary)
                        Text(n, style = CrundiType.Meta, maxLines = 1)
                    }
                }
            }
        }
    }

    @Test
    fun components() = shoot("design-components") {
        Column(Modifier.verticalScroll(rememberScrollState())) {
            TopBar("deploy.sh", subtitle = "crundi / scripts", onBack = {})
            PanelHeader("Services", meta = "3 running")
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    CrundiButton("Start", {}, kind = ButtonKind.Primary, icon = "play")
                    CrundiButton("Logs", {}, icon = "file-text")
                    CrundiButton("Stop", {}, kind = ButtonKind.Danger, icon = "stop")
                }
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    StatusBadge("connected", BadgeKind.Good)
                    StatusBadge("reconnecting", BadgeKind.Warn)
                    StatusBadge("stopped", BadgeKind.Bad)
                    StatusBadge("dev", BadgeKind.Accent)
                }
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    CrundiChip("All", true, {})
                    CrundiChip("Running", false, {}, icon = "play")
                    CrundiChip("Stopped", false, {})
                }
                CrundiTextField("", {}, placeholder = "Search files", icon = "search", modifier = Modifier.fillMaxWidth())
                Banner("The server restarts in a moment.", kind = BadgeKind.Warn, icon = "alert-triangle")
            }
            SectionHeader("Projects")
            Column(Modifier.padding(horizontal = 8.dp)) {
                ListRow("crundi", subtitle = "/home/crundi/projects/crundi", active = true, leading = { StateDot(DotState.Working) })
                ListRow("dhivo-socials", subtitle = "2 chats", leading = { StateDot(DotState.Running) }, trailing = { Text("12:40", style = CrundiType.Meta) })
                ListRow("Settings", icon = "settings")
            }
            CrundiSwitchRow("Bubble on leave", true, {}, detail = "Put running chats in bubbles when you leave the app")
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                CrundiCard(Modifier.fillMaxWidth()) {
                    MarkdownText(
                        "## Plan\nRun the **tests** first, then `assemblePlayDebug`.\n\n- theme from the CSS variables\n- icons from *ICON_PATHS*\n\n```kotlin\nval x = CrundiIcons.get(\"folder\") // 24 grid\n```",
                    )
                }
                CodeBlock("export JAVA_HOME=\$HOME/android-toolchain/jdk\n./gradlew --no-daemon assemblePlayDebug", language = "sh")
                DialogCard("Delete note?", message = "\"Release checklist\" is removed for good.", confirm = "Delete note", danger = true)
            }
        }
    }
}
