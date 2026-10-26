package mv.obitech.crundi.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import mv.obitech.crundi.api.model.ResumableSession
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.workbench.LaunchMode
import mv.obitech.crundi.ui.workbench.SessionChooser
import mv.obitech.crundi.ui.workbench.SessionChooserCard
import mv.obitech.crundi.ui.workbench.TerminalLauncher
import org.junit.Test

/**
 * A new terminal pane's launcher, and the session chooser every Claude
 * button opens (the web's openSessionChooser): the list, a heavy
 * conversation's choice in UI Mode, and the list failing to load.
 */
class SessionChooserScreens : ScreenshotTest() {
    /** The fixture's two (one heavy and open elsewhere), and one known only by its size. */
    private val list = FakeApi.model<List<ResumableSession>>("resumable", "sessions") +
        ResumableSession("9a8b7c6d-5e4f-3a2b", "Tidy the release notes", updatedAt = "2026-08-02T09:00:00.000Z", sizeBytes = 61_440)
    private val now = java.time.Instant.parse("2026-10-21T11:15:00Z").toEpochMilli()

    @Composable
    private fun InDialog(content: @Composable () -> Unit) {
        Box(Modifier.fillMaxSize().background(CrundiColors.Scrim).padding(16.dp), contentAlignment = Alignment.Center) { content() }
    }

    @Test fun chooser() = shoot("workbench-session-chooser") {
        InDialog { SessionChooserCard(SessionChooser("p:n1", LaunchMode.Chat, list), now = now) }
    }

    @Test fun chooserTerminalSkip() = shoot("workbench-session-chooser-terminal-skip") {
        InDialog { SessionChooserCard(SessionChooser("p:n1", LaunchMode.TerminalSkip, list), now = now) }
    }

    @Test fun heavy() = shoot("workbench-session-chooser-heavy") {
        InDialog { SessionChooserCard(SessionChooser("p:n1", LaunchMode.Chat, list, heavy = list.first()), now = now) }
    }

    @Test fun failed() = shoot("workbench-session-chooser-failed") {
        InDialog { SessionChooserCard(SessionChooser("p:n1", LaunchMode.ChatSkip, emptyList(), failed = "Server error 500: disk unreadable"), now = now) }
    }

    @Test fun launcher() = shoot("workbench-terminal-launcher") { TerminalLauncher({}) }

    /** In a narrow pane (a split column), and busy while the list loads. */
    @Test fun launcherNarrowBusy() = shoot("workbench-terminal-launcher-narrow") {
        Row(Modifier.fillMaxSize()) {
            Box(Modifier.width(200.dp).fillMaxHeight().border(1.dp, CrundiColors.Border)) { TerminalLauncher({}, busy = true) }
            Box(Modifier.width(200.dp).fillMaxHeight().border(1.dp, CrundiColors.Border)) { TerminalLauncher({}, error = "Could not start: server error 500") }
        }
    }
}
