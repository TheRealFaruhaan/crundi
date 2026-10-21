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
fun ParkedChatPreview(paneId: String) {
    val api = LocalAppServices.current.api
    var messages by remember(paneId) { mutableStateOf<List<ChatEntry>?>(null) }
    LaunchedEffect(paneId) { messages = runCatching { api.terminals.preview(paneId).messages }.getOrDefault(emptyList()) }
    val m = messages
    if (m.isNullOrEmpty()) GhostLines() else ParkedChatCopy(m)
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
