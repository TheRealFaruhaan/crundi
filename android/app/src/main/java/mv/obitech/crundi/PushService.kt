package mv.obitech.crundi

import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import org.json.JSONObject

/**
 * Where a notification arrives.
 *
 * What Google delivers is one field, `c`: ciphertext the server sealed for
 * this phone. Nothing is shown from it until it has been opened here, and it
 * is opened by trying each paired server's keys. The pair that works says
 * which server it came from; if none does, it was not for this phone and is
 * dropped without a word.
 */
class PushService : FirebaseMessagingService() {
    override fun onNewToken(token: String) {
        Push.onToken(applicationContext, token)
    }

    override fun onMessageReceived(message: RemoteMessage) {
        val app = applicationContext
        try {
            val sealed = message.data["c"]
            if (sealed == null) { PushLog.add(app, "A message arrived with nothing in it"); return }
            val servers = Servers.all(app)
            for (server in servers) {
                val keys = Push.keysOf(server) ?: continue
                val text = PushBox.open(sealed, keys) ?: continue
                val payload = JSONObject(text)
                val kind = payload.optString("kind")
                // A change of state draws the live card and makes no sound;
                // anything else is a notification.
                if (payload.optBoolean("silent") && kind == "state") {
                    PushLog.add(app, "From ${server.host}: a chat is now ${payload.optString("state")}")
                    LiveCards.onState(app, server, payload)
                    // Working again means somebody has answered it, here or
                    // on another device: what its notification said is done.
                    if (payload.optString("state") == "working") payload.optJSONObject("chat")?.optString("id")?.takeIf { it.isNotEmpty() }?.let { Notifier.clearChat(app, server.id, it, why = "it started working") }
                } else if (payload.optBoolean("silent") && kind == "clear") {
                    PushLog.add(app, "From ${server.host}: requests answered, notifications taken down")
                    Notifier.clearKinds(app, server, payload)
                    Notifier.clearChats(app, server, payload)
                } else if (payload.optBoolean("silent") && kind == "suggestion") {
                    PushLog.add(app, "From ${server.host}: a suggested reply ${if (payload.optString("suggestion").isEmpty()) "withdrawn" else "offered"}")
                    Notifier.suggest(app, server, payload)
                } else {
                    PushLog.add(app, "From ${server.host}: a notification ($kind)")
                    Notifier.show(app, server, payload)
                }
                return
            }
            // Sealed for a key this phone does not have. Registering again
            // gives every server the keys it does have.
            PushLog.add(app, "A message arrived (${sealed.length} characters) that none of ${servers.size} paired server's keys could open. Registering again.")
            Push.registerAll(app, force = true)
        } catch (e: Exception) {
            PushLog.add(app, "A message arrived and handling it failed: ${e.javaClass.simpleName}: ${e.message}")
        }
    }
}
