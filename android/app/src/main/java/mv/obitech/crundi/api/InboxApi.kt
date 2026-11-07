package mv.obitech.crundi.api

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.decodeFromJsonElement

/**
 * Crundi's own inbox (capability "inbox"): everything the server meant for
 * the owner (messages, photos and files Claude sent, alerts, questions and
 * permission requests, Cru's reports, schedule results), in threads: one
 * per chat ('chat:<id>'), per project without a chat ('project:<alias>'),
 * Cru's ('cru') and Crundi's own ('system').
 *
 *   GET  /api/inbox/threads                        { ok, threads, unread }, newest activity first
 *   GET  /api/inbox?thread=&limit=&before=         { ok, items, unread }, newest first
 *   GET  /api/inbox/<id>                           { ok, item, unread }
 *   POST /api/inbox/read                           { thread } | { ids } | { all:true }
 *   POST /api/inbox/delete                         { thread } | { ids } | { allRead:true }
 *
 * Every change comes live on the event stream as `inbox` { unread, thread?, item? }
 * ([InboxEvent.parse]). An older server has none of it: [supported] is
 * false and nothing about the inbox shows.
 */
class InboxApi internal constructor(private val rpc: Rpc) {
    /** Every thread, newest activity first, and the total unread (the drawer's badge). */
    suspend fun threads(): InboxThreads = rpc.read(rpc.get("/api/inbox/threads"), null)

    /** One page of a thread (or of everything when [thread] is null), newest first; [before] is the `at` of the oldest item already shown. */
    suspend fun list(thread: String? = null, limit: Int = PAGE, before: Long? = null): InboxPage =
        rpc.read(rpc.get("/api/inbox", mapOf("thread" to thread, "limit" to limit.toString(), "before" to before?.toString())), null)

    /** One item (a notification's), with its thread. */
    suspend fun item(id: String): InboxItemReply = rpc.read(rpc.get("/api/inbox/" + java.net.URLEncoder.encode(id, "UTF-8").replace("+", "%20")), null)

    /** Only the total unread count (the drawer's badge). */
    suspend fun unread(): Int = threads().unread

    suspend fun markRead(ids: List<String>) { if (ids.isNotEmpty()) rpc.post("/api/inbox/read", mapOf("ids" to ids)) }
    suspend fun markThreadRead(thread: String) { rpc.post("/api/inbox/read", mapOf("thread" to thread)) }
    suspend fun markAllRead() { rpc.post("/api/inbox/read", mapOf("all" to true)) }
    suspend fun delete(ids: List<String>) { if (ids.isNotEmpty()) rpc.post("/api/inbox/delete", mapOf("ids" to ids)) }
    suspend fun deleteThread(thread: String) { rpc.post("/api/inbox/delete", mapOf("thread" to thread)) }

    companion object {
        const val CAPABILITY = "inbox"
        const val PAGE = 30
        fun supported(capabilities: Set<String>): Boolean = CAPABILITY in capabilities
    }
}

@Serializable
data class InboxMedia(
    /** Server-relative; fetched with the phone's login (ServerImages, Transport.download). */
    val url: String = "",
    val name: String = "",
    val mime: String = "",
    val size: Long = 0,
) {
    val isImage: Boolean get() = mime.startsWith("image/") || (mime.isEmpty() && Regex("\\.(png|jpe?g|gif|webp|bmp)$", RegexOption.IGNORE_CASE).containsMatchIn(name.ifEmpty { url.substringBefore('?') }))
}

@Serializable
data class InboxItem(
    val id: String,
    /** Epoch ms. */
    val at: Long = 0,
    /** message, photo, file, alert, question, permission, report, schedule, system */
    val kind: String = "message",
    val title: String = "",
    val text: String = "",
    val project: String = "",
    val chatId: String = "",
    val chatTitle: String = "",
    val media: List<InboxMedia> = emptyList(),
    val read: Boolean = false,
    /** A server path (or an address) to open for it, besides its chat. */
    val link: String = "",
    /** Its thread's key: chat:<id>, project:<alias>, cru, system. */
    val thread: String = "",
) {
    /** What the list shows on the first line: the title, else the text's first line. */
    val headline: String get() = title.ifBlank { text.lineSequence().map { it.trim() }.firstOrNull { it.isNotEmpty() } ?: kindLabel(kind) }

    /** "crundi · Fix the build", or what there is of it. */
    val where: String get() = listOf(project, chatTitle).filter { it.isNotBlank() }.joinToString(" · ")

    companion object {
        /** The flat icon (CrundiIcons) for each kind. */
        fun iconOf(kind: String): String = when (kind) {
            "message" -> "message-square"
            "photo" -> "image"
            "file" -> "file"
            "alert" -> "bell"
            "question" -> "help-circle"
            "permission" -> "shield-alert"
            "report" -> "file-text"
            "schedule" -> "clock"
            "system" -> "server"
            else -> "info"
        }

        fun kindLabel(kind: String): String = when (kind) {
            "message" -> "Message"
            "photo" -> "Photo"
            "file" -> "File"
            "alert" -> "Alert"
            "question" -> "Question"
            "permission" -> "Permission request"
            "report" -> "Report"
            "schedule" -> "Scheduled job"
            "system" -> "Crundi"
            else -> "Message"
        }
    }
}

@Serializable
data class InboxPage(val items: List<InboxItem> = emptyList(), val unread: Int = 0)

@Serializable
data class InboxItemReply(val item: InboxItem? = null, val unread: Int = 0)

/** A thread's newest item, as the thread list shows it. */
@Serializable
data class InboxLast(val id: String = "", val at: Long = 0, val kind: String = "message", val title: String = "", val text: String = "") {
    /** The line under the thread's title: the item's title, else its text's first line. */
    val line: String get() = title.ifBlank { text.lineSequence().map { it.trim() }.firstOrNull { it.isNotEmpty() } ?: InboxItem.kindLabel(kind) }
}

@Serializable
data class InboxThread(
    val key: String,
    /** chat, project, cru, system */
    val kind: String = "system",
    val title: String = "",
    val subtitle: String = "",
    val project: String = "",
    val chatId: String = "",
    val last: InboxLast? = null,
    val unread: Int = 0,
    val count: Int = 0,
) {
    val at: Long get() = last?.at ?: 0
    val name: String get() = title.ifBlank { when (kind) { "cru" -> "Cru"; "system" -> "Crundi"; "project" -> project.ifBlank { "Project" }; else -> "Chat" } }

    companion object {
        fun iconOf(kind: String): String = when (kind) {
            "chat" -> "message-square"
            "project" -> "folder"
            "cru" -> "mic"
            "system" -> "server"
            else -> "info"
        }
    }
}

@Serializable
data class InboxThreads(val threads: List<InboxThread> = emptyList(), val unread: Int = 0)

/**
 * The thread list as the Inbox screen holds it: newest activity first, the
 * total unread. Changes give a new value.
 */
data class InboxThreadList(val threads: List<InboxThread> = emptyList(), val unread: Int = 0, val loaded: Boolean = false) {
    fun loaded(t: InboxThreads) = InboxThreadList(t.threads.sortedByDescending { it.at }, t.unread, loaded = true)

    fun markRead(key: String): InboxThreadList {
        val had = threads.firstOrNull { it.key == key }?.unread ?: 0
        return copy(threads = threads.map { if (it.key == key) it.copy(unread = 0) else it }, unread = (unread - had).coerceAtLeast(0))
    }

    fun markAllRead() = copy(threads = threads.map { it.copy(unread = 0) }, unread = 0)

    fun delete(key: String): InboxThreadList {
        val had = threads.firstOrNull { it.key == key }?.unread ?: 0
        return copy(threads = threads.filterNot { it.key == key }, unread = (unread - had).coerceAtLeast(0))
    }

    fun byKey(key: String) = threads.firstOrNull { it.key == key }
}

/** The live `inbox` event: the new total unread, the thread involved and the item that changed, when the server sends them. */
data class InboxEvent(val unread: Int, val item: InboxItem?, val thread: String? = null) {
    /** This event touches [key] (an item of it, or the thread named); null thread: could be any. */
    fun touches(key: String): Boolean = (thread ?: item?.thread?.takeIf { it.isNotEmpty() }).let { it == null || it == key }

    companion object {
        const val NAME = "inbox"

        /** Null when it is not an inbox event or has no count. */
        fun parse(data: JsonObject): InboxEvent? {
            val type = data.str("type")
            if (type != null && type != NAME) return null
            val unread = data.long("unread")?.toInt() ?: return null
            val item = (data["item"] as? JsonObject)?.let { runCatching { CrundiJson.decodeFromJsonElement<InboxItem>(it) }.getOrNull() }
            val thread = data.str("thread")?.takeIf { it.isNotEmpty() } ?: item?.thread?.takeIf { it.isNotEmpty() }
            return InboxEvent(unread.coerceAtLeast(0), item, thread)
        }
    }
}

/**
 * What the Inbox screen holds: the items loaded so far (newest first, no
 * duplicates), the unread count and whether older pages may follow. Every
 * change gives a new value, so the screen and tests read it the same way.
 */
data class InboxList(
    val items: List<InboxItem> = emptyList(),
    val unread: Int = 0,
    /** The last page was full: there may be older items (load with before = [oldest]). */
    val hasMore: Boolean = true,
    val loaded: Boolean = false,
) {
    val oldest: Long? get() = items.lastOrNull()?.at

    /** The first page (pull to refresh, opening the screen): it replaces what is shown, keeping older pages already loaded. */
    fun first(page: InboxPage, limit: Int = InboxApi.PAGE): InboxList {
        val fresh = page.items
        val cut = fresh.lastOrNull()?.at
        // Keep the older pages below what the first page covers; the first page wins for what it has.
        val older = if (cut == null || !loaded) emptyList() else items.filter { it.at < cut && fresh.none { f -> f.id == it.id } }
        val all = sorted(fresh + older)
        return InboxList(all, page.unread, hasMore = if (older.isEmpty()) fresh.size >= limit else hasMore, loaded = true)
    }

    /** An older page (scrolled to the end). */
    fun older(page: InboxPage, limit: Int = InboxApi.PAGE): InboxList =
        copy(items = sorted(items + page.items.filter { n -> items.none { it.id == n.id } }), unread = page.unread, hasMore = page.items.size >= limit, loaded = true)

    fun markRead(ids: Collection<String>): InboxList {
        val now = items.count { it.id in ids && !it.read }
        return copy(items = items.map { if (it.id in ids && !it.read) it.copy(read = true) else it }, unread = (unread - now).coerceAtLeast(0))
    }

    fun markAllRead(): InboxList = copy(items = items.map { if (it.read) it else it.copy(read = true) }, unread = 0)

    fun delete(ids: Collection<String>): InboxList {
        val gone = items.count { it.id in ids && !it.read }
        return copy(items = items.filterNot { it.id in ids }, unread = (unread - gone).coerceAtLeast(0))
    }

    /**
     * A live event: its count, and its item put in (or replaced) when it has
     * one. With [thread], an item of another thread is left out.
     */
    fun apply(e: InboxEvent, thread: String? = null): InboxList {
        val n = e.item ?: return copy(unread = e.unread)
        if (thread != null && n.thread.isNotEmpty() && n.thread != thread) return this
        return copy(items = sorted(items.filterNot { it.id == n.id } + n), unread = e.unread)
    }

    fun byId(id: String): InboxItem? = items.firstOrNull { it.id == id }

    private fun sorted(l: List<InboxItem>) = l.sortedByDescending { it.at }
}
