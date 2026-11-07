package mv.obitech.crundi.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import kotlinx.serialization.json.jsonObject
import mv.obitech.crundi.api.CrundiJson
import mv.obitech.crundi.api.model.AgentState
import mv.obitech.crundi.api.model.Approvals
import mv.obitech.crundi.api.model.ChatEntry
import mv.obitech.crundi.api.model.ChatState
import mv.obitech.crundi.api.model.ResumableSession
import mv.obitech.crundi.ui.chats.ActionSheetCard
import mv.obitech.crundi.ui.chats.ApprovalsActions
import mv.obitech.crundi.ui.chats.ApprovalsScreen
import mv.obitech.crundi.ui.chats.ApprovalsState
import mv.obitech.crundi.ui.chats.ChatActions
import mv.obitech.crundi.ui.chats.ChatScreen
import mv.obitech.crundi.ui.chats.ChatSheet
import mv.obitech.crundi.ui.chats.ChatUi
import mv.obitech.crundi.ui.chats.RenameCard
import mv.obitech.crundi.ui.chats.SheetOverlay
import mv.obitech.crundi.ui.chats.UploadUi
import mv.obitech.crundi.ui.chats.chatMenu
import mv.obitech.crundi.ui.shell.Host
import mv.obitech.crundi.ui.theme.CrundiColors
import org.junit.Test

/** Chats, one chat in every state that matters, and approvals. */
class ChatsScreens : ScreenshotTest() {
    /** shoot() with inspection mode on, so spinners are drawn still (see Spinner). */
    private fun snap(name: String, host: Host = Host.None, content: @androidx.compose.runtime.Composable () -> Unit) =
        shoot(name, host = host) {
            androidx.compose.runtime.CompositionLocalProvider(androidx.compose.ui.platform.LocalInspectionMode provides true) { content() }
        }

    private val state = FakeApi.state
    private val noChat = object : ChatActions {}
    private val bubbleHost = object : Host { override val isBubble = true }

    private fun chat(events: Int = Int.MAX_VALUE) = FakeApi.chat(events)

    @Test fun chatHistory() = snap("chats-chat") {
        ChatScreen(ChatUi(chat(0), pane = state.chats("crundi").first(), open = setOf("x1")), noChat)
    }

    @Test fun chatStreaming() = snap("chats-chat-streaming") {
        ChatScreen(ChatUi(chat(5)), noChat)
    }

    @Test fun chatPermission() = snap("chats-chat-permission") {
        ChatScreen(ChatUi(chat(13)), noChat)
    }

    @Test fun chatQuestion() = snap("chats-chat-question") {
        ChatScreen(ChatUi(chat(14)), noChat)
    }

    @Test fun chatQueued() = snap("chats-chat-queued") {
        val c = chat(19)
        ChatScreen(ChatUi(c.copy(messages = c.messages.filter { !it.isPending }, state = AgentState.Working), draft = "and the 401 renewal", uploads = listOf(UploadUi("Screenshot_2026-10-21.png", 0.62f))), noChat)
    }

    @Test fun chatEnded() = snap("chats-chat-ended") {
        ChatScreen(ChatUi(chat()), noChat)
    }

    @Test fun chatSlash() = snap("chats-chat-slash") {
        ChatScreen(ChatUi(chat(0), draft = "/"), noChat)
    }

    @Test fun chatMode() = snap("chats-chat-mode") {
        ChatScreen(ChatUi(chat(0), sheet = ChatSheet.Mode), noChat)
    }

    @Test fun chatModel() = snap("chats-chat-model") {
        ChatScreen(ChatUi(chat(0), sheet = ChatSheet.Model), noChat)
    }

    @Test fun chatMenu() = snap("chats-chat-menu") {
        ChatScreen(ChatUi(chat(0), pane = state.chats("crundi").first(), sheet = ChatSheet.Menu), noChat)
    }

    @Test fun chatRename() = snap("chats-chat-rename") {
        ChatScreen(ChatUi(chat(0), sheet = ChatSheet.Rename("Android API client")), noChat)
    }

    @Test fun chatAgent() = snap("chats-chat-agent") {
        ChatScreen(ChatUi(chat(10), sheet = ChatSheet.Agent("toolu_01Ef")), noChat)
    }

    @Test fun chatBubble() = snap("chats-chat-bubble", host = bubbleHost) {
        ChatScreen(ChatUi(chat(14), isBubble = true), noChat)
    }

    // Put to sleep while open: the workbench's sleeping view over what it showed, "asleep" in the header and the footer.
    private fun parked(c: ChatState) = mv.obitech.crundi.api.model.Pane(c.id, project = "crundi", title = c.title, kind = "parked", paneKind = "ui", status = "parked", reason = "idle", hasSession = true)
    @Test fun chatAsleep() = snap("chats-chat-asleep") {
        val c = chat(0)
        ChatScreen(ChatUi(c, pane = parked(c), draft = "and the 401 renewal"), noChat)
    }

    @Test fun chatAsleepBubbleWaking() = snap("chats-chat-asleep-bubble-waking", host = bubbleHost) {
        val c = chat(0)
        ChatScreen(ChatUi(c, pane = parked(c), draft = "and the 401 renewal", isBubble = true, waking = "resume"), noChat)
    }

    @Test fun chatAsleepWakeFailed() = snap("chats-chat-asleep-failed", host = bubbleHost) {
        val c = chat(0)
        ChatScreen(ChatUi(c, pane = parked(c), draft = "and the 401 renewal", isBubble = true, error = mv.obitech.crundi.ui.chats.ChatSleep.WAKE_FAILED + "project \"crundi\" no longer exists."), noChat)
    }

    /** Voice mode on (ChatVoice): the mic lit beside Send, what it hears live over the buttons. */
    @Test fun chatVoiceMode() = snap("chats-chat-voice-mode") {
        ChatScreen(ChatUi(chat(0), voice = mv.obitech.crundi.ui.chats.ChatVoiceUi(mode = mv.obitech.crundi.ui.chats.ChatVoiceUi.Mode.On, interim = "and make the retry wait twice as long each time")), noChat)
    }

    private val levels = kotlinx.coroutines.flow.MutableStateFlow(listOf(0.05f, 0.1f, 0.3f, 0.55f, 0.4f, 0.7f, 0.85f, 0.6f, 0.35f, 0.5f, 0.75f, 0.9f, 0.65f, 0.4f, 0.2f, 0.45f, 0.7f, 0.55f, 0.3f, 0.6f, 0.8f, 0.5f, 0.25f, 0.4f, 0.65f, 0.45f, 0.2f, 0.1f))
    private fun recording(cancel: Float) = mv.obitech.crundi.ui.chats.ChatVoiceUi(
        mode = mv.obitech.crundi.ui.chats.ChatVoiceUi.Mode.Holding, interim = "and make the retry wait twice as long",
        since = System.currentTimeMillis() - 7_400, cancelProgress = cancel,
    )

    /** Push to talk, held: the recording bar in Send's place, Cancel at the far right, the mic a send button. */
    @Test fun chatVoiceHolding() = snap("chats-chat-voice-holding") {
        ChatScreen(ChatUi(chat(0), voice = recording(0f), voiceLevels = levels), noChat)
    }

    /** Held and slid onto Cancel: letting go here sends nothing. */
    @Test fun chatVoiceHoldingCancel() = snap("chats-chat-voice-holding-cancel") {
        ChatScreen(ChatUi(chat(0), voice = recording(1f), voiceLevels = levels), noChat)
    }

    /** Turned on while Cru listens: the chat asks to take the microphone. */
    @Test fun chatVoiceAskSwitch() = snap("chats-chat-voice-ask-switch") {
        ChatScreen(ChatUi(chat(0), voice = mv.obitech.crundi.ui.chats.ChatVoiceUi(askSwitch = true)), noChat)
    }

    /** Cru opened on purpose: voice mode waits, and goes on by itself after. */
    @Test fun chatVoicePausedForCru() = snap("chats-chat-voice-paused") {
        ChatScreen(ChatUi(chat(0), voice = mv.obitech.crundi.ui.chats.ChatVoiceUi(mode = mv.obitech.crundi.ui.chats.ChatVoiceUi.Mode.Paused)), noChat)
    }

    @Test fun chatLoading() = snap("chats-chat-loading") {
        ChatScreen(ChatUi(ChatState("a1b2c3d4e5f60718")), noChat)
    }

    /** chat-summaries.jsonl: a batch of tool calls with its label, an agent's progress line, an idle recap. */
    private fun summaries(): ChatState {
        var s = ChatState("c5d6e7f8a9b0c1d2")
        for (line in FakeApi.text("chat-summaries.jsonl").lines().filter { it.isNotBlank() }) s = mv.obitech.crundi.api.ChatReducer.message(s, CrundiJson.parseToJsonElement(line).jsonObject)
        return s
    }

    @Test fun chatSummaries() = snap("chats-chat-summaries") {
        ChatScreen(ChatUi(summaries(), recapOk = true, asideOk = true), noChat)
    }

    @Test fun chatSummariesOpen() = snap("chats-chat-summaries-open") {
        ChatScreen(ChatUi(summaries(), open = setOf("g:t1")), noChat)
    }

    @Test fun asideAsk() = snap("chats-aside-ask") {
        ChatScreen(ChatUi(summaries(), asideOk = true, sheet = ChatSheet.Aside("What is left to do?")), noChat)
    }

    @Test fun asideAnswer() = snap("chats-aside-answer") {
        ChatScreen(ChatUi(summaries(), asideOk = true, sheet = ChatSheet.Aside("What is left to do?", asked = true, answer = "Only the pull request: the parser is tidied and **its tests pass**.")), noChat)
    }

    @Test fun factoryResult() = snap("chats-chat-factory-result") {
        val card = ChatEntry.from(CrundiJson.parseToJsonElement(FACTORY_RESULT).jsonObject)
        val c = chat(0)
        ChatScreen(ChatUi(c.copy(title = "Manager", messages = c.messages.take(4) + card, factory = mv.obitech.crundi.api.model.FactoryRole("run_mvhwe7gr9b668d", "manager"))), noChat)
    }

    @Test fun approvals() = snap("chats-approvals") {
        ApprovalsScreen(ApprovalsState(FakeApi.model<Approvals>("approvals")), object : ApprovalsActions {})
    }

    @Test fun approvalsEmpty() = snap("chats-approvals-empty") {
        ApprovalsScreen(ApprovalsState(), object : ApprovalsActions {})
    }

    @Test fun rename() = snap("chats-rename") {
        Overlay { RenameCard("chat", "Android API client", {}, {}, {}) }
    }

    @androidx.compose.runtime.Composable
    private fun Overlay(content: @androidx.compose.runtime.Composable () -> Unit) {
        Box(Modifier.fillMaxSize().background(CrundiColors.Scrim).padding(16.dp), contentAlignment = Alignment.Center) { content() }
    }

    companion object {
        val FACTORY_RESULT = """
            {"id":"fr1","kind":"factory-result","seq":40,"runId":"run_mvhwe7gr9b668d","factory":"Android team","state":"ask",
             "summary":"The app is native now: every area has a Compose screen, the bubble opens the native chat, and all four Gradle tasks pass.",
             "branch":"factory/native-android","parentBranch":"dev",
             "diff":{"added":9093,"removed":592,"count":111,"files":[
               {"file":"android/app/src/main/java/mv/obitech/crundi/ui/chats/Chat.kt","added":412,"removed":0},
               {"file":"android/app/src/main/java/mv/obitech/crundi/api/CrundiClient.kt","added":139,"removed":0},
               {"file":"android/app/build.gradle.kts","added":38,"removed":12}]},
             "usage":{"costUsd":41.27,"turns":1840,"handoffs":14,"chats":9,"activeMs":15720000},
             "followUps":[{"id":"F3","text":"Git history needs a server route","where":"ui/workspace","blocking":false}]}
        """.trimIndent()
    }
}
