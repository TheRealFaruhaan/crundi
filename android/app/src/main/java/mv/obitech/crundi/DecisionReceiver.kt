package mv.obitech.crundi

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import mv.obitech.crundi.api.seg
import org.json.JSONObject

/**
 * Approve or Deny, pressed on a notification.
 *
 * The answer goes to the one request the notification described, by its id.
 * If that request has been answered somewhere else in the meantime, the
 * server says it is no longer pending and nothing is approved by mistake.
 */
class DecisionReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        val serverId = intent.getStringExtra(Notifier.EXTRA_SERVER).orEmpty()
        val chatId = intent.getStringExtra(Notifier.EXTRA_CHAT).orEmpty()
        val requestId = intent.getStringExtra(Notifier.EXTRA_REQUEST).orEmpty()
        val behavior = if (intent.getStringExtra(Notifier.EXTRA_BEHAVIOR) == "allow") "allow" else "deny"
        if (serverId.isEmpty() || chatId.isEmpty() || requestId.isEmpty()) return
        val app = ctx.applicationContext
        val pending = goAsync()
        Bg.run({
            val r = Api.authed(app, serverId, "POST", "/api/ui-sessions/${seg(chatId)}/respond", JSONObject().put("requestId", requestId).put("behavior", behavior))
            val list = Api.authed(app, serverId, "GET", "/api/ui-sessions")
            val sessions = list.json?.optJSONArray("sessions")
            var chat: ChatRef? = null
            if (sessions != null) for (i in 0 until sessions.length()) {
                val s = sessions.getJSONObject(i)
                if (s.optString("id") == chatId) chat = ChatRef.from(serverId, JSONObject().put("id", chatId).put("title", s.optString("longTitle").ifEmpty { s.optString("title") }).put("project", s.optString("project")).put("mark", s.optJSONObject("mark")))
            }
            Pair(r, chat ?: ChatRef(serverId, chatId, "Chat", "", "C", "#6366f1", "#4f46e5"))
        }) { result ->
            val (r, chat) = result.getOrNull() ?: Pair(Reply(0, null), ChatRef(serverId, chatId, "Chat", "", "C", "#6366f1", "#4f46e5"))
            val outcome = when {
                r.ok -> if (behavior == "allow") "Approved" else "Denied"
                r.status == 426 -> "Not sent. Update Crundi to answer from here."
                r.status == 401 || r.status == 403 -> "Not sent. This phone is no longer signed in to that server."
                r.status == 0 -> "Not sent. The server could not be reached."
                else -> r.error ?: "That request is no longer waiting."
            }
            Notifier.afterDecision(app, chat, outcome)
            pending.finish()
        }
    }
}
