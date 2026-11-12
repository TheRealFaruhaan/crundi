package mv.obitech.crundi

import org.json.JSONObject

/**
 * Where a push that is also in Crundi's inbox goes (its payload carries
 * `inboxId` / `inboxThread`).
 *
 * - A request (a question, a permission, a secret: something answered where
 *   it was asked) stays exactly as before: the chat's own notification and
 *   bubble, or the server's notification.
 * - Something sent to the owner (a message, a picture, a file, a report),
 *   with bubbles allowed: a conversation of its own for that inbox THREAD,
 *   with its own shortcut and bubble, separate from the chat's bubble and
 *   from the main window.
 * - The same with bubbles off: the notification it always was, and a tap
 *   opens the thread in the main window.
 */
object InboxNotify {
    enum class Route {
        /** Not an inbox push, or a request: as before. */
        AsBefore,
        /** Its own inbox-thread conversation, able to bubble. */
        InboxBubble,
        /** The usual notification; a tap opens the inbox thread in the main window. */
        InboxMain,
    }

    /** Kinds that ask the owner something, answered in the chat or by its own card. */
    val REQUEST_KINDS = setOf("secretRequest", "collabPermission", "needsInput", "question", "permission")

    private val ID = Regex("^[A-Za-z0-9_.:-]{1,120}$")
    private val THREAD = Regex("^[A-Za-z0-9_.:@-]{1,200}$")

    fun inboxId(payload: JSONObject): String? = payload.optString("inboxId").takeIf { ID.matches(it) }

    /** The push's thread: as sent, else the chat's own thread for an inbox item about a chat. */
    fun thread(payload: JSONObject): String? {
        payload.optString("inboxThread").takeIf { THREAD.matches(it) }?.let { return it }
        if (inboxId(payload) == null) return null
        val chat = payload.optJSONObject("chat")?.optString("id").orEmpty()
        return if (Regex("^[A-Za-z0-9_-]{1,80}$").matches(chat)) "chat:$chat" else null
    }

    fun isRequest(payload: JSONObject): Boolean =
        payload.optJSONObject("prompt")?.optString("requestId")?.isNotEmpty() == true || payload.optString("kind") in REQUEST_KINDS

    fun route(payload: JSONObject, bubblesOn: Boolean): Route {
        if (isRequest(payload)) return Route.AsBefore
        val thread = thread(payload)
        if (thread != null && bubblesOn) return Route.InboxBubble
        if (Notifier.inboxPath(payload) != null || thread != null) return Route.InboxMain
        return Route.AsBefore
    }

    /** What the thread is called in its notification, bubble and shortcut. */
    fun title(thread: String, payload: JSONObject): String {
        payload.optString("inboxThreadTitle").takeIf { it.isNotBlank() }?.let { return it.take(80) }
        return when {
            thread == "cru" -> "Cru"
            thread == "system" -> "Crundi"
            thread.startsWith("project:") -> thread.removePrefix("project:")
            else -> payload.optJSONObject("chat")?.optString("title")?.takeIf { it.isNotBlank() } ?: "Inbox"
        }
    }
}

/** One inbox thread, as much of it as a notification, a shortcut and a bubble need. */
class InboxRef(val serverId: String, val thread: String, val title: String, val project: String = "", val icon: String = "") {
    /** Android's name for this conversation: never the chat's ("chat:..."), so the two bubbles stay apart. */
    val key: String get() = "inbox:$serverId:$thread"
    val notificationId: Int get() = key.hashCode()
    val chatId: String get() = thread.takeIf { it.startsWith("chat:") }?.removePrefix("chat:").orEmpty()
}
