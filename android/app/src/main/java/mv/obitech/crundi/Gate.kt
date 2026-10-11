package mv.obitech.crundi

import org.json.JSONObject

/**
 * Whether this app and a server can work together.
 *
 * Each side holds the oldest version of the other it accepts. The server's
 * number for us arrives in its handshake; ours for it is built in
 * (BuildConfig.MIN_SERVER). Either can refuse, and both can at once: an old
 * app pointed at an old server, each needing the other to be newer.
 */
object Gate {
    class Verdict(
        val appTooOld: Boolean,
        val serverTooOld: Boolean,
        val serverVersion: String,
        val minApp: String,
        val capabilities: Set<String>,
    ) {
        val ok get() = !appTooOld && !serverTooOld
    }

    /** `handshake` is the body of GET /api/handshake. */
    fun judge(handshake: JSONObject, appVersion: String, minServer: String): Verdict {
        val server = handshake.optJSONObject("server") ?: JSONObject()
        val serverVersion = server.optString("version", "")
        val minApp = server.optJSONObject("minClient")?.optString("android", "") ?: ""
        val caps = mutableSetOf<String>()
        server.optJSONArray("capabilities")?.let { for (i in 0 until it.length()) caps.add(it.optString(i)) }
        // The server's own verdict on us counts as well as our arithmetic: it
        // may refuse for a reason this version of the app does not know about.
        val refused = handshake.optJSONObject("client")?.let { it.has("accepted") && !it.optBoolean("accepted", true) } ?: false
        return Verdict(
            appTooOld = refused || Versions.below(appVersion, minApp),
            serverTooOld = Versions.below(serverVersion, minServer),
            serverVersion = serverVersion,
            minApp = minApp,
            capabilities = caps,
        )
    }
}
