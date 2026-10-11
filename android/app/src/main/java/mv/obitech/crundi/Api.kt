package mv.obitech.crundi

import android.os.Handler
import android.os.Looper
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.Executors

/** Runs work off the main thread and brings the answer back to it. */
object Bg {
    private val pool = Executors.newCachedThreadPool()
    private val main = Handler(Looper.getMainLooper())
    fun <T> run(work: () -> T, done: (Result<T>) -> Unit) {
        pool.execute {
            val r = runCatching(work)
            main.post { done(r) }
        }
    }
}

class Reply(val status: Int, val json: JSONObject?) {
    val ok get() = status in 200..299 && json?.optBoolean("ok", true) != false
    /** The server's own words for what went wrong, if it gave any. */
    val error get() = json?.optString("error", "")?.takeIf { it.isNotEmpty() }
}

/** This app's side of the server's HTTP API. Blocking: call it from Bg. */
object Api {
    /** How this app names itself. The server reads the version out of it to decide whether we are too old. */
    val CLIENT_TAG = "android-${BuildConfig.STORE}/${BuildConfig.VERSION_NAME}"
    val UA_SUFFIX = "CrundiAndroid/${BuildConfig.VERSION_NAME} (${BuildConfig.STORE})"

    fun request(method: String, url: String, body: JSONObject? = null, bearer: String? = null): Reply {
        val c = URL(url).openConnection() as HttpURLConnection
        try {
            c.requestMethod = method
            c.connectTimeout = 12_000
            c.readTimeout = 20_000
            c.instanceFollowRedirects = false
            c.setRequestProperty("X-Crundi-Client", CLIENT_TAG)
            c.setRequestProperty("User-Agent", "Crundi $UA_SUFFIX")
            c.setRequestProperty("Accept", "application/json")
            if (bearer != null) c.setRequestProperty("Authorization", "Bearer $bearer")
            if (body != null) {
                c.doOutput = true
                c.setRequestProperty("Content-Type", "application/json")
                c.outputStream.use { it.write(body.toString().toByteArray(Charsets.UTF_8)) }
            }
            val status = c.responseCode
            val text = (if (status in 200..399) c.inputStream else c.errorStream)?.use { it.readBytes().toString(Charsets.UTF_8) } ?: ""
            return Reply(status, runCatching { JSONObject(text) }.getOrNull())
        } finally {
            c.disconnect()
        }
    }

    fun handshake(baseUrl: String) = request("GET", "$baseUrl/api/handshake")

    /** Spend a pairing code. The answer carries the two logins and the server's handshake. */
    fun claim(link: PairingLink, deviceName: String) = request(
        "POST", "${link.baseUrl}/api/pair/claim",
        JSONObject().put("code", link.code).put("device", JSONObject().put("name", deviceName).put("platform", "android")),
    )

    /**
     * Renew this app's own login. Every renewal retires the refresh token it
     * used, so the new pair must be saved before anything else is tried.
     */
    fun refresh(server: Server): Boolean {
        val r = request("POST", "${server.baseUrl}/api/auth/refresh", JSONObject().put("refreshToken", server.bgRefresh))
        val j = r.json ?: return false
        if (!r.ok || !j.has("token")) return false
        server.bgToken = j.getString("token")
        server.bgRefresh = j.optString("refreshToken", server.bgRefresh)
        return true
    }
}
