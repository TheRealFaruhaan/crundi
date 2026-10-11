package mv.obitech.crundi

import android.content.Context
import android.util.Log
import com.google.firebase.FirebaseApp
import com.google.firebase.FirebaseOptions
import com.google.firebase.messaging.FirebaseMessaging
import org.json.JSONObject
import java.util.Base64

/**
 * Getting this phone onto each server's list of phones to notify.
 *
 * The phone asks Google for a token (an address Google can deliver to), makes
 * a key pair for the server, and gives the server both. The server then seals
 * each notification for that key and hands it to the relay with the token;
 * see android/RELAY.md for the whole path.
 */
object Push {
    private const val TAG = "CrundiPush"

    /** False in a build made without google-services.json: everything else works, notifications do not. */
    val available: Boolean get() = BuildConfig.FIREBASE_APP_ID.isNotEmpty() && BuildConfig.FIREBASE_API_KEY.isNotEmpty()

    /**
     * Firebase is set up by hand, not by Google's build plugin, so that the
     * project builds without the file that describes it. Safe to call often.
     */
    fun init(ctx: Context): Boolean {
        if (!available) return false
        if (FirebaseApp.getApps(ctx).isNotEmpty()) return true
        FirebaseApp.initializeApp(
            ctx.applicationContext,
            FirebaseOptions.Builder()
                .setProjectId(BuildConfig.FIREBASE_PROJECT_ID)
                .setApplicationId(BuildConfig.FIREBASE_APP_ID)
                .setApiKey(BuildConfig.FIREBASE_API_KEY)
                .setGcmSenderId(BuildConfig.FIREBASE_SENDER_ID)
                .build(),
        )
        return true
    }

    fun keysOf(server: Server): PushBox.Keys? {
        if (server.pushPrivate.isEmpty() || server.pushPublic.isEmpty() || server.pushAuth.isEmpty()) return null
        return try {
            PushBox.Keys(Base64.getDecoder().decode(server.pushPrivate), PushBox.unb64(server.pushPublic), PushBox.unb64(server.pushAuth))
        } catch (e: Exception) {
            null
        }
    }

    /** Ask Google for this phone's token, then make sure every paired server has it. */
    fun registerAll(ctx: Context) {
        if (!init(ctx)) return
        FirebaseMessaging.getInstance().token.addOnCompleteListener { task ->
            val token = if (task.isSuccessful) task.result else null
            if (token.isNullOrEmpty()) { Log.w(TAG, "No push token from Google: ${task.exception?.message}"); return@addOnCompleteListener }
            onToken(ctx, token)
        }
    }

    /** A token, new or old: give it to each server that does not have it yet. */
    fun onToken(ctx: Context, token: String) {
        val app = ctx.applicationContext
        Bg.run({
            for (listed in Servers.all(app)) {
                // Work on a fresh copy each time: a renewal inside the call
                // below rewrites the stored server, and an older copy saved
                // over it would bring a spent login back.
                var server = Servers.byId(app, listed.id) ?: continue
                if (server.pushToken == token && keysOf(server) != null) continue
                if (keysOf(server) == null) {
                    val k = PushBox.generate()
                    server.pushPrivate = Base64.getEncoder().encodeToString(k.privatePkcs8)
                    server.pushPublic = k.p256dh
                    server.pushAuth = k.authText
                    server.pushToken = ""
                    Servers.save(app, server)
                }
                val body = JSONObject().put("platform", "android").put("token", token)
                    .put("keys", JSONObject().put("p256dh", server.pushPublic).put("auth", server.pushAuth))
                val r = Api.authed(app, server.id, "POST", "/api/push/mobile/register", body)
                if (r.ok) {
                    server = Servers.byId(app, server.id) ?: continue
                    server.pushToken = token
                    Servers.save(app, server)
                } else {
                    Log.w(TAG, "${server.host} did not take the registration: ${r.status} ${r.error ?: ""}")
                }
            }
        }) { }
    }
}
