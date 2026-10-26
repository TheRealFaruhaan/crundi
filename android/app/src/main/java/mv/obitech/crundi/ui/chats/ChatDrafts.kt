package mv.obitech.crundi.ui.chats

import android.content.Context
import android.content.SharedPreferences
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.put

/**
 * What is typed and not sent yet, kept in the app's own storage as the web
 * keeps it in localStorage (claude-chat.js: DRAFT_KEY, ANS_KEY, pruneDrafts),
 * with the same keys and the same timestamped records ({v, t}):
 *
 * - the composer's text per chat (crundi_chat_draft_<chat>), written on every
 *   change and removed when the box is empty, so sending clears it;
 * - the same text per project (crundi_chat_draft_p_<project>), because a chat
 *   woken or relaunched can come back under a new id. A chat adopts it only
 *   when it has nothing of its own and no other open chat has claimed it, and
 *   only the claimer may write or clear it, so a second chat in the project
 *   can neither steal nor wipe the first one's text;
 * - a question card's half-made answer (crundi_chat_ans_<chat>_<entry>):
 *   choices, Other and notes, cleared when the card is submitted;
 * - and all of them dropped after 30 days, since the ids they name do not
 *   outlive a restart and nothing else would ever clear them.
 *
 * The main window, a Workbench pane and a bubble all run the same chat route
 * in one process, so they share one store and one set of claims.
 */
class ChatDrafts(
    private val kv: Store,
    /** Which server these are for: claims are per server, as the store is. */
    private val scope: String = "",
    private val claims: MutableMap<String, String> = CLAIMS,
    private val now: () -> Long = System::currentTimeMillis,
) {
    /** The few things asked of the storage underneath (SharedPreferences in the app, a map in tests). */
    interface Store {
        fun get(key: String): String?
        fun put(key: String, value: String)
        fun remove(key: String)
        fun keys(): Collection<String>
    }

    /** One question's half-made answer, as the web saves it ({chosen, other, note, noteOpt}). */
    data class Pick(val chosen: List<String> = emptyList(), val other: String = "", val note: String = "", val noteOpt: String = "") {
        val empty get() = chosen.isEmpty() && other.isEmpty() && note.isEmpty()
    }

    private fun write(key: String, text: String) {
        if (text.isNotEmpty()) kv.put(key, record(JsonPrimitive(text))) else kv.remove(key)
    }

    private fun record(v: kotlinx.serialization.json.JsonElement) = buildJsonObject { put("v", v); put("t", now()) }.toString()

    /** A draft's text; one written before drafts carried a timestamp is the bare text. */
    private fun read(key: String): String {
        val raw = kv.get(key) ?: return ""
        if (!raw.startsWith("{")) return raw
        return try { (Json.parseToJsonElement(raw).jsonObject["v"] as? JsonPrimitive)?.contentOrNull.orEmpty() } catch (e: Exception) { raw }
    }

    /** Drops draft and answer records older than [TTL_MS]; untimestamped ones are left alone, as on the web. */
    fun prune() {
        val t = now()
        val kill = kv.keys().filter { k ->
            if (!k.startsWith(DRAFT) && !k.startsWith(ANSWER)) return@filter false
            val raw = kv.get(k) ?: return@filter false
            if (!raw.startsWith("{")) return@filter false
            val at = try { (Json.parseToJsonElement(raw).jsonObject["t"] as? JsonPrimitive)?.longOrNull } catch (e: Exception) { null }
            at != null && at > 0 && t - at > TTL_MS
        }
        kill.forEach { kv.remove(it) }
    }

    /** The composer of one open chat: [load] when it opens, [save] on every change, [release] when it closes. */
    inner class Composer(val chatId: String) {
        private val chatKey = DRAFT + chatId
        private var projectKey: String? = null
        private var claimed = false
        private fun claimKey(k: String) = "$scope|$k"

        /** The chat's own draft (prunes the old ones first, as the web does on mount). */
        fun load(): String { prune(); return read(chatKey) }

        /**
         * The chat's project is known (it can come a moment after the chat).
         * [current] is what is in the box now: when it is empty the project's
         * draft is adopted, if nobody has claimed it. Whatever the box then
         * holds is claimed and anchored under this chat. Returns the text the
         * box should show, or null to leave it.
         */
        fun attach(project: String, current: String): String? {
            if (project.isEmpty() || projectKey != null) return null
            val pk = DRAFT_PROJECT + project
            projectKey = pk
            val owner = claims[claimKey(pk)]
            val adopted = if (current.isEmpty() && owner == null) read(pk) else ""
            val text = current.ifEmpty { adopted }
            if (text.isNotEmpty() && (owner == null || owner == chatId)) {
                claimed = true
                claims[claimKey(pk)] = chatId
                save(text)   // re-anchor under this chat's id
            }
            return adopted.ifEmpty { null }
        }

        fun save(text: String) {
            write(chatKey, text)
            val pk = projectKey ?: return
            // The claim gates the write, not just the read: only the owner (or nobody) may touch it.
            val owner = claims[claimKey(pk)]
            if (owner != null && owner != chatId) return
            if (text.isNotEmpty()) {
                write(pk, text)
                claimed = true
                claims[claimKey(pk)] = chatId
            } else if (claimed) {
                write(pk, "")
                claimed = false
                claims.remove(claimKey(pk))
            }
        }

        /** Closed: let a later chat in the project pick the draft back up. The draft itself stays. */
        fun release() {
            val pk = projectKey ?: return
            if (claims[claimKey(pk)] == chatId) claims.remove(claimKey(pk))
            claimed = false
        }

        /** The chat came back under another id, which carries the text on: this id's copy goes. */
        fun forget() { kv.remove(chatKey); release() }
    }

    private fun answerKey(chatId: String, entryId: String) = "$ANSWER${chatId}_$entryId"

    /** A card's saved answer, one [Pick] per question, or null when there is none. */
    fun loadAnswer(chatId: String, entryId: String): List<Pick>? {
        val raw = kv.get(answerKey(chatId, entryId)) ?: return null
        return try {
            val el = Json.parseToJsonElement(raw)
            // Older answers were a bare array, before they carried a timestamp.
            val arr = (el as? JsonArray) ?: (el as? JsonObject)?.get("v") as? JsonArray ?: return null
            arr.map { p ->
                val o = p as? JsonObject ?: return@map Pick()
                fun s(k: String) = (o[k] as? JsonPrimitive)?.contentOrNull.orEmpty()
                Pick((o["chosen"] as? JsonArray)?.mapNotNull { (it as? JsonPrimitive)?.contentOrNull }.orEmpty(), s("other"), s("note"), s("noteOpt"))
            }.takeIf { it.isNotEmpty() }
        } catch (e: Exception) { null }
    }

    /** Saved on every change; removed when nothing is picked or written. */
    fun saveAnswer(chatId: String, entryId: String, picks: List<Pick>) {
        val key = answerKey(chatId, entryId)
        if (picks.all { it.empty }) { kv.remove(key); return }
        kv.put(key, record(buildJsonArray {
            for (p in picks) add(buildJsonObject {
                put("chosen", JsonArray(p.chosen.map { JsonPrimitive(it) }))
                put("other", p.other); put("note", p.note); put("noteOpt", p.noteOpt)
            })
        }))
    }

    fun clearAnswer(chatId: String, entryId: String) = kv.remove(answerKey(chatId, entryId))

    companion object {
        const val TTL_MS = 30L * 24 * 60 * 60 * 1000
        const val DRAFT = "crundi_chat_draft_"
        const val DRAFT_PROJECT = "crundi_chat_draft_p_"
        const val ANSWER = "crundi_chat_ans_"

        /** Which open chat owns each project's draft, for as long as the process lives (the web's CLAIMED). */
        private val CLAIMS = java.util.concurrent.ConcurrentHashMap<String, String>()

        /** The store for one server (by its host): a file of its own, as the web's is its origin's. */
        fun of(ctx: Context, server: String): ChatDrafts {
            val name = "crundi_chat_drafts" + server.lowercase().replace(Regex("[^a-z0-9.-]"), "_").let { if (it.isEmpty()) "" else "_$it" }
            return ChatDrafts(Prefs(ctx.applicationContext.getSharedPreferences(name, Context.MODE_PRIVATE)), server)
        }
    }

    private class Prefs(private val p: SharedPreferences) : Store {
        override fun get(key: String) = p.getString(key, null)
        override fun put(key: String, value: String) { p.edit().putString(key, value).apply() }
        override fun remove(key: String) { p.edit().remove(key).apply() }
        override fun keys(): Collection<String> = p.all.keys
    }
}
