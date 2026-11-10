package mv.obitech.crundi.ui.chats

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import mv.obitech.crundi.api.model.ChatEntry
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.workbench.GhostLines

/**
 * What a parked chat last showed, drawn under the Resume veil (.parked-cell):
 * its last messages from /api/panes/<id>/preview, bottom-aligned as the chat
 * was. Nothing in it can be tapped. Until it loads, or when the server has no
 * copy, the grey lines stand in.
 */
@Composable
fun ParkedChatPreview(paneId: String, peek: Boolean = false, onResume: () -> Unit = {}) {
    val api = LocalAppServices.current.api
    var messages by remember(paneId) { mutableStateOf<List<ChatEntry>?>(null) }
    LaunchedEffect(paneId) { messages = runCatching { api.terminals.preview(paneId).messages }.getOrDefault(emptyList()) }
    val m = messages
    if (m.isNullOrEmpty()) GhostLines() else if (peek) ParkedChatPeek(m, onResume) else ParkedChatCopy(m)
}

/**
 * Peek (the parked view's button): the whole conversation, readable and
 * scrollable, newest at the bottom, without resuming the chat. Answering a
 * question or a permission in it resumes the chat (it is asked again there);
 * a reply goes through the reply box, which resumes it too.
 */
@Composable
fun ParkedChatPeek(messages: List<ChatEntry>, onResume: () -> Unit) {
    var open by remember { mutableStateOf(emptySet<String>()) }
    val acts = remember(onResume) {
        object : EntryActions {
            override fun toggle(id: String) { open = if (id in open) open - id else open + id }
            override fun allow(e: ChatEntry, always: Boolean) = onResume()
            override fun deny(e: ChatEntry) = onResume()
            override fun answer(e: ChatEntry, answers: Map<String, String>, notes: Map<String, String>) = onResume()
            override fun dismissQuestion(e: ChatEntry) = onResume()
            override fun factory(e: ChatEntry, choice: String, note: String?) = onResume()
        }
    }
    val shown = messages.filter { it.kind != "gone" }
    androidx.compose.foundation.lazy.LazyColumn(
        Modifier.fillMaxSize().background(CrundiColors.BgPrimary),
        reverseLayout = true,
        contentPadding = androidx.compose.foundation.layout.PaddingValues(start = 12.dp, end = 12.dp, top = 56.dp, bottom = 72.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        items(shown.size) { i -> val e = shown[shown.size - 1 - i]; ChatEntryView(e, "", open, acts) }
    }
}

/** The still copy itself (pure, for screenshots). */
@Composable
fun ParkedChatCopy(messages: List<ChatEntry>) {
    Box(Modifier.fillMaxSize().background(CrundiColors.BgPrimary), contentAlignment = Alignment.BottomStart) {
        Column(Modifier.fillMaxWidth().padding(12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            for (e in messages.filter { it.kind != "gone" }.takeLast(8)) ChatEntryView(e, "", emptySet(), object : EntryActions {})
        }
    }
}
