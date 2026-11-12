package mv.obitech.crundi.voice

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.intOrNull

/**
 * Cru's reports nobody heard (capability "voice-unread"): the server counts
 * them instead of putting them in the inbox. The top bar's Cru mark and the
 * voice screen's chat button show a red dot while there are any; opening
 * the chat view marks them seen (POST /api/voice/seen).
 */
object CruUnread {
    private val _count = MutableStateFlow(0)
    val count: StateFlow<Int> = _count

    fun set(n: Int) { _count.value = n.coerceAtLeast(0) }

    /** The count from a 'voice-unread' event ({ count, latestAt }); null when it has none. */
    fun countOf(data: JsonObject): Int? = (data["count"] as? JsonPrimitive)?.intOrNull

    /** Where a push about Cru's unheard reports goes (payload cruChat: true): the voice screen's chat view. */
    const val CHAT_PATH = "/?cru=chat"

    /** [path] is the Cru chat view's link ("/?cru=chat", any other query too). */
    fun isChatPath(path: String): Boolean {
        val q = path.substringAfter('?', "")
        return path.substringBefore('?').let { it == "/" || it.isEmpty() } && q.split('&').any { it == "cru=chat" }
    }
}
