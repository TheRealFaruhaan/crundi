package mv.obitech.crundi.voice

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import mv.obitech.crundi.api.ApiRequest
import mv.obitech.crundi.api.ScreenBlock
import mv.obitech.crundi.api.model.ChatMedia
import mv.obitech.crundi.api.model.ChatMediaParser
import mv.obitech.crundi.ui.chats.EntryActions
import mv.obitech.crundi.ui.chats.ImageViewer
import mv.obitech.crundi.ui.chats.MediaGroup
import mv.obitech.crundi.ui.chats.VideoViewer
import mv.obitech.crundi.ui.chats.queryOf
import mv.obitech.crundi.ui.shell.AppServices
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType

/**
 * What Cru showed in one of its replies (the server's voice/reply-media.js,
 * capability "voice-reply-media"), parsed with no Android in it (CruChatTest).
 */
object CruReplyItems {
    /** Cru's media session on the server (chat-media.js). */
    const val SESSION = "cru-voice"

    /** One card: a title and the live screen's blocks. */
    data class Card(val title: String, val blocks: List<ScreenBlock>)

    private fun kind(el: kotlinx.serialization.json.JsonElement) = ((el as? JsonObject)?.get("kind") as? JsonPrimitive)?.contentOrNull.orEmpty()

    /** Pictures, players, files and frames, as a chat's show_* calls are read. */
    fun media(items: JsonArray): List<ChatMedia> = ChatMediaParser.parseItems(SESSION, JsonArray(items.filter { kind(it) != "card" }))

    /** The cards, in order. */
    fun cards(items: JsonArray): List<Card> = items.filter { kind(it) == "card" }.mapNotNull { el ->
        val o = el as? JsonObject ?: return@mapNotNull null
        val blocks = (o["blocks"] as? JsonArray).orEmpty().mapNotNull { ScreenBlock.parse(it) }
        val title = (o["title"] as? JsonPrimitive)?.contentOrNull.orEmpty().take(200)
        if (blocks.isEmpty() && title.isBlank()) null else Card(title, blocks)
    }
}

/**
 * A reply's media under its bubble: its cards (the live screen's [Blocks]),
 * then its pictures, players, files and frames drawn by a Claude chat's own
 * [MediaGroup]. A picture opens large, a video plays full screen, a file
 * downloads through the app's downloads (a notification, then tap to open).
 * [services]: the paired server's client; without one only the cards show.
 */
@Composable
fun CruReplyMedia(items: JsonArray, services: AppServices?, modifier: Modifier = Modifier) {
    val cards = remember(items) { CruReplyItems.cards(items) }
    val media = remember(items) { CruReplyItems.media(items) }
    if (cards.isEmpty() && media.isEmpty()) return
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var viewing by remember { mutableStateOf<ChatMedia?>(null) }
    val download: (ChatMedia) -> Unit = { m ->
        val path = m.path
        if (path != null && services != null) scope.launch {
            val name = m.filename.ifEmpty { path.substringAfter("name=", "file").substringBefore('&') }
            val dl = mv.obitech.crundi.MediaDownloads.states
            dl[path] = mv.obitech.crundi.MediaDownloads.State(0, m.size)
            try {
                val saved = mv.obitech.crundi.Downloads.save(ctx, name, size = m.size, onProgress = { n -> dl[path] = mv.obitech.crundi.MediaDownloads.State(n, m.size) }) { sink ->
                    services.api.transport.download(ApiRequest("GET", path.substringBefore('?'), queryOf(path)), sink)
                }
                dl[path] = mv.obitech.crundi.MediaDownloads.State(saved.size, saved.size, savedWhere = saved.where + "/" + saved.name)
            } catch (e: Exception) {
                dl[path] = mv.obitech.crundi.MediaDownloads.State(failed = true)
            }
        }
    }
    val actions = remember(services) {
        object : EntryActions {
            override fun media(m: ChatMedia) {
                when {
                    m.kind == "image" || m.kind == "video" -> viewing = m
                    m.kind == "embed" && m.openUrl != null -> runCatching {
                        ctx.startActivity(android.content.Intent(android.content.Intent.ACTION_VIEW, android.net.Uri.parse(m.openUrl)).addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK))
                    }
                    else -> download(m.copy(kind = m.kind.removePrefix("file-")))
                }
            }
        }
    }
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        val shape = RoundedCornerShape(14.dp)
        for (c in cards) Column(
            Modifier.widthIn(max = 420.dp).clip(shape).background(CrundiColors.BgCard).border(1.dp, CrundiColors.Border, shape)
                .horizontalScroll(rememberScrollState()).padding(horizontal = 14.dp, vertical = 12.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            if (c.title.isNotBlank()) Text(c.title, style = CrundiType.Small.copy(color = CrundiColors.TextSecondary, fontWeight = FontWeight.SemiBold))
            Blocks(c.blocks)
        }
        if (media.isNotEmpty() && services != null) CompositionLocalProvider(LocalAppServices provides services) {
            MediaGroup(media, actions)
        }
    }
    val v = viewing
    if (v != null && services != null) Dialog({ viewing = null }, DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        CompositionLocalProvider(LocalAppServices provides services) {
            androidx.compose.foundation.layout.Box(Modifier.fillMaxSize().background(androidx.compose.ui.graphics.Color.Black)) {
                if (v.kind == "video") VideoViewer(v, onSave = { download(v) }, onClose = { viewing = null })
                else ImageViewer(v, onSave = { download(it) }, onClose = { viewing = null }, gallery = media.filter { it.kind == "image" })
            }
        }
    }
}
