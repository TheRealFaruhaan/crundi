package mv.obitech.crundi.screens

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalInspectionMode
import androidx.compose.ui.unit.dp
import mv.obitech.crundi.api.CrundiJson
import mv.obitech.crundi.api.model.AgentState
import mv.obitech.crundi.api.model.ChatScheduleItem
import mv.obitech.crundi.api.model.SecretRequest
import mv.obitech.crundi.ui.chats.ChatActions
import mv.obitech.crundi.ui.chats.ChatScreen
import mv.obitech.crundi.ui.chats.ChatUi
import mv.obitech.crundi.ui.chats.ScheduleUi
import mv.obitech.crundi.ui.operations.SecretPromptDock
import mv.obitech.crundi.ui.operations.SecretPromptState
import mv.obitech.crundi.ui.shell.Host
import mv.obitech.crundi.ui.workbench.LocalPane
import mv.obitech.crundi.ui.workbench.PaneActions
import mv.obitech.crundi.ui.workbench.PaneFrame
import mv.obitech.crundi.ui.workbench.PaneInfo
import mv.obitech.crundi.ui.workbench.PaneKind
import mv.obitech.crundi.ui.workbench.PaneView
import org.junit.Test

/**
 * The chat as change 2 shows it: inside a Workbench pane (the frame draws
 * the header), its schedule clock closed, open and with sends queued, the
 * bubble's chat-only view, long titles; and Claude asking for a secret.
 */
class ChatPaneScreens : ScreenshotTest() {
    private fun still(name: String, host: Host = Host.None, content: @Composable () -> Unit) =
        shoot(name, host = host) { CompositionLocalProvider(LocalInspectionMode provides true) { content() } }

    private val noChat = object : ChatActions {}
    private val pane = FakeApi.state.chats("crundi").first()
    private val queued: List<ChatScheduleItem> = CrundiJson.decodeFromJsonElement(
        kotlinx.serialization.builtins.ListSerializer(ChatScheduleItem.serializer()), FakeApi.json("chat-schedule")["items"]!!,
    )
    private val longTitle = "Investigate why the notification reply sometimes lands in the wrong chat after the phone slept on a flaky connection"

    /** A chat as a Workbench pane: the pane's header, then the chat without its own. */
    @Composable
    private fun InPane(ui: ChatUi, title: String = pane.title, state: AgentState? = AgentState.NeedsInput) {
        val v = PaneView("t:" + pane.id, PaneKind.Chat, title, state, pane = pane.copy(title = title))
        Box(Modifier.fillMaxSize().padding(4.dp)) {
            PaneFrame(v, PaneActions.None, Modifier.fillMaxSize(), focused = true) {
                CompositionLocalProvider(LocalPane provides PaneInfo(v.key, PaneKind.Chat)) { ChatScreen(ui.copy(inPane = true), noChat) }
            }
        }
    }

    @Test fun chatPane() = still("chats-pane") { InPane(ChatUi(FakeApi.chat(13), pane = pane)) }

    @Test fun scheduleClosed() = still("chats-schedule-closed") { InPane(ChatUi(FakeApi.chat(0), pane = pane, schedule = ScheduleUi(loaded = true))) }

    @Test fun scheduleOpen() = still("chats-schedule-open") {
        InPane(ChatUi(FakeApi.chat(0), pane = pane, draft = "Run the tests again", schedule = ScheduleUi(open = true, loaded = true, text = "Run the tests again", items = queued)))
    }

    @Test fun scheduleQueued() = still("chats-schedule-queued") { InPane(ChatUi(FakeApi.chat(0), pane = pane, schedule = ScheduleUi(loaded = true, items = queued))) }

    @Test fun bubbleChatOnly() = still("chats-bubble-chat-only", host = object : Host { override val isBubble = true }) {
        ChatScreen(ChatUi(FakeApi.chat(0), pane = pane.copy(title = longTitle, longTitle = longTitle), isBubble = true), noChat)
    }

    @Test fun longTitlePane() = still("chats-pane-long-title") { InPane(ChatUi(FakeApi.chat(0), pane = pane), title = longTitle, state = AgentState.Working) }

    @Test fun longTitleFull() = still("chats-long-title") { ChatScreen(ChatUi(FakeApi.chat(0), pane = pane.copy(title = longTitle, longTitle = longTitle)), noChat) }

    // ─── Claude asking for a secret, over any screen ───

    private val requests = listOf(
        SecretRequest("r1", "s1", "GITHUB_TOKEN", "crundi", "Push the release tag", kind = "get"),
        SecretRequest("r2", "s2", "PLAY_UPLOAD_KEY", "crundi", "Sign the bundle", kind = "run", command = "./gradlew --no-daemon bundlePlayRelease", envName = "CRUNDI_UPLOAD_STORE_PASSWORD"),
    )

    @Test fun secretAsk() = still("secrets-request-dock") {
        Box(Modifier.fillMaxSize()) { InPane(ChatUi(FakeApi.chat(0), pane = pane)); SecretPromptDock(requests, SecretPromptState()) }
    }

    // The bubble's chat (ChatActivity, ui.chats.BubbleChat): the same prompt over the chat.
    @Test fun secretInBubble() = still("chats-bubble-secret-request", host = object : Host { override val isBubble = true }) {
        Box(Modifier.fillMaxSize()) {
            ChatScreen(ChatUi(FakeApi.chat(0), pane = pane, isBubble = true), noChat)
            SecretPromptDock(requests.drop(1), SecretPromptState(armed = "r2", pin = "48"))
        }
    }

    @Test fun secretPin() = still("secrets-request-pin") {
        Box(Modifier.fillMaxSize()) {
            InPane(ChatUi(FakeApi.chat(0), pane = pane))
            SecretPromptDock(requests.take(1), SecretPromptState(armed = "r1", pin = "1234", error = "Incorrect PIN"))
        }
    }
}
