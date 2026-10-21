package mv.obitech.crundi.screens

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.platform.LocalInspectionMode
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flow
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.ApiRequest
import mv.obitech.crundi.api.ApiResponse
import mv.obitech.crundi.api.CrundiClient
import mv.obitech.crundi.api.DownloadInfo
import mv.obitech.crundi.api.Progress
import mv.obitech.crundi.api.SocketConnection
import mv.obitech.crundi.api.SocketListener
import mv.obitech.crundi.api.SseFrame
import mv.obitech.crundi.api.Transport
import mv.obitech.crundi.api.model.Pane
import mv.obitech.crundi.ui.chats.ChatsListActions
import mv.obitech.crundi.ui.chats.ChatsListState
import mv.obitech.crundi.ui.chats.ChatsScreen
import mv.obitech.crundi.ui.shell.AppServices
import mv.obitech.crundi.ui.shell.Area
import mv.obitech.crundi.ui.shell.Connection
import mv.obitech.crundi.ui.shell.CrundiApp
import mv.obitech.crundi.ui.shell.Host
import mv.obitech.crundi.ui.shell.ProjectSummary
import mv.obitech.crundi.ui.shell.ShellData
import mv.obitech.crundi.ui.shell.ShellSource
import mv.obitech.crundi.ui.shell.StaticShellSource
import org.junit.Test
import java.io.OutputStream

/**
 * The Tester's break-it screens: every area through the real NavHost when the
 * server cannot be reached and when it answers every call with a 500, plus
 * very long names. Images are "edge-<case>-<area>".
 */
class EdgeScreens : ScreenshotTest() {
    /** Every call fails the same way. */
    private class FailingTransport(val offline: Boolean) : Transport {
        override val baseUrl = "https://dev.example.com"
        override suspend fun exchange(request: ApiRequest, progress: Progress?): ApiResponse {
            if (offline) throw ApiException(0, "Could not reach dev.example.com")
            return ApiResponse(500, """{"ok":false,"error":"Internal server error"}""")
        }
        override suspend fun download(request: ApiRequest, sink: OutputStream, progress: Progress?): DownloadInfo =
            throw ApiException(if (offline) 0 else 500, if (offline) "Could not reach dev.example.com" else "Internal server error")
        override fun events(path: String): Flow<SseFrame> = if (offline) flow { throw ApiException(0, "Could not reach dev.example.com") } else emptyFlow()
        override fun socket(listener: SocketListener): SocketConnection {
            listener.onClosed(0, if (offline) "offline" else "error")
            return object : SocketConnection { override fun send(text: String) = false; override fun close() {} }
        }
    }

    private fun services(shell: ShellData, transport: Transport): AppServices = object : AppServices {
        override val shell: ShellSource = StaticShellSource(shell)
        override val api: CrundiClient = CrundiClient(transport)
    }

    private fun still(name: String, services: AppServices = FakeServices.default(), content: @Composable () -> Unit) =
        shoot(name, services) { CompositionLocalProvider(LocalInspectionMode provides true) { content() } }

    private fun everyArea(case: String, shell: ShellData, transport: () -> Transport) {
        for (a in Area.entries) {
            val s = services(shell, transport())
            still("edge-$case-${a.key}", s) { CrundiApp(services = s, host = Host.None, openRoute = if (a == Area.Chats) null else a.key) }
        }
    }

    @Test fun offline() = everyArea("offline", FakeServices.shellData.copy(connection = Connection.Offline, approvals = 0)) { FailingTransport(offline = true) }

    @Test fun serverError() = everyArea("error", FakeServices.shellData.copy(approvals = 0)) { FailingTransport(offline = false) }

    private val longName = "a-project-with-a-really-quite-unreasonably-long-name-for-a-phone"

    @Test fun longProject() {
        val shell = FakeServices.shellData.copy(
            serverHost = "a-very-long-server-host-name.internal.example-company.com",
            projects = FakeServices.projects + ProjectSummary(longName, longName, "/home/crundi/projects/$longName"),
            currentProject = longName,
            approvals = 128,
        )
        val s = FakeServices.default(shell)
        still("edge-long-shell", s) { CrundiApp(services = s, host = Host.None, openRoute = Area.Services.key) }
    }

    @Test fun longChatTitles() = still("edge-long-chats") {
        val t = "Investigate why the notification reply sometimes lands in the wrong chat after the phone was asleep for a long time and then woke up on a flaky connection"
        ChatsScreen(
            ChatsListState(
                "crundi",
                chats = listOf(
                    Pane("p1", "crundi", title = t, longTitle = t, kind = "ui", status = "running", agentState = "working", model = "opus"),
                    Pane("p2", "crundi", title = "x".repeat(140), kind = "ui", status = "running", model = "sonnet"),
                    Pane("p3", "crundi", title = "", kind = "ui", status = "running"),
                ),
                error = "The server said: " + "something went badly wrong while listing the chats, ".repeat(4),
            ),
            object : ChatsListActions {},
        )
    }
}
