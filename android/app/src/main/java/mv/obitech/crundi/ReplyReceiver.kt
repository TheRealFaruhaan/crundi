package mv.obitech.crundi

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import androidx.core.app.RemoteInput
import org.json.JSONObject

/**
 * A reply typed into a notification.
 *
 * No page is open when this happens, so the app sends the message itself,
 * with its own login, to the server the chat belongs to. Whatever comes of it
 * is written back into the notification: the message as sent, or the reason
 * it was not.
 */
class ReplyReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        val text = RemoteInput.getResultsFromIntent(intent)?.getCharSequence(Notifier.KEY_REPLY)?.toString()?.trim().orEmpty()
        val serverId = intent.getStringExtra(Notifier.EXTRA_SERVER).orEmpty()
        val chatId = intent.getStringExtra(Notifier.EXTRA_CHAT).orEmpty()
        if (text.isEmpty() || serverId.isEmpty() || chatId.isEmpty()) return
        val app = ctx.applicationContext
        val pending = goAsync()
        Bg.run({
            val sent = Api.authed(app, serverId, "POST", "/api/ui-sessions/$chatId/send", JSONObject().put("text", text))
            // The chat's name and mark, for redrawing the notification.
            val list = Api.authed(app, serverId, "GET", "/api/ui-sessions")
            val sessions = list.json?.optJSONArray("sessions")
            var chat: ChatRef? = null
            if (sessions != null) for (i in 0 until sessions.length()) {
                val s = sessions.getJSONObject(i)
                if (s.optString("id") == chatId) chat = ChatRef.from(serverId, JSONObject().put("id", chatId).put("title", s.optString("longTitle").ifEmpty { s.optString("title") }).put("project", s.optString("project")).put("mark", s.optJSONObject("mark")))
            }
            Pair(sent, chat ?: ChatRef(serverId, chatId, "Chat", "", "C", "#6366f1", "#4f46e5"))
        }) { result ->
            val (sent, chat) = result.getOrNull() ?: Pair(Reply(0, null), ChatRef(serverId, chatId, "Chat", "", "C", "#6366f1", "#4f46e5"))
            val problem = when {
                sent.ok -> null
                sent.status == 426 -> "Not sent. Update Crundi to reply from here."
                sent.status == 401 || sent.status == 403 -> "Not sent. This phone is no longer signed in to that server."
                sent.status == 0 -> "Not sent. The server could not be reached."
                else -> "Not sent. ${sent.error ?: "Open Crundi to reply."}"
            }
            Notifier.afterReply(app, chat, text, problem)
            pending.finish()
        }
    }
}
