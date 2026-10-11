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
    fun registerAll(ctx: Context, force: Boolean = false) {
        if (!init(ctx)) return
        FirebaseMessaging.getInstance().token.addOnCompleteListener { task ->
            val token = if (task.isSuccessful) task.result else null
            if (token.isNullOrEmpty()) {
                PushLog.add(ctx, "Google gave no address for this phone: ${task.exception?.message ?: "unknown reason"}")
                Log.w(TAG, "No push token from Google: ${task.exception?.message}")
                return@addOnCompleteListener
            }
            onToken(ctx, token, force)
        }
    }

    private val registering = Any()

    /**
     * A token, new or old: give it to each server that does not have it yet.
     *
     * One at a time. On a first start this is called twice at once, by the
     * app asking for its token and by Google announcing the same token. Side
     * by side, each made its own key pair for the server, and the pair the
     * phone ended up keeping was not always the pair the server was last
     * given: everything then arrived sealed for a key the phone no longer
     * had, and nothing was ever shown. `force` registers again regardless.
     */
    fun onToken(ctx: Context, token: String, force: Boolean = false) {
        val app = ctx.applicationContext
        Bg.run({ synchronized(registering) {
            for (listed in Servers.all(app)) {
                // Work on a fresh copy each time: a renewal inside the call
                // below rewrites the stored server, and an older copy saved
                // over it would bring a spent login back.
                var server = Servers.byId(app, listed.id) ?: continue
                if (!force && server.pushToken == token && keysOf(server) != null && server.pushBuild == BuildConfig.VERSION_CODE) continue
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
                    server.pushBuild = BuildConfig.VERSION_CODE
                    Servers.save(app, server)
                    PushLog.add(app, "Registered with ${server.host}")
                } else {
                    PushLog.add(app, "${server.host} did not take the registration (${r.status}${if (r.error != null) ": ${r.error}" else ""})")
                    Log.w(TAG, "${server.host} did not take the registration: ${r.status} ${r.error ?: ""}")
                }
            }
        } }) { }
    }
}
