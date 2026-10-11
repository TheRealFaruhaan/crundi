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
        val sealed = message.data["c"] ?: return
        for (server in Servers.all(applicationContext)) {
            val keys = Push.keysOf(server) ?: continue
            val text = PushBox.open(sealed, keys) ?: continue
            val payload = try { JSONObject(text) } catch (e: Exception) { return }
            Notifier.show(applicationContext, server, payload)
            return
        }
    }
}
