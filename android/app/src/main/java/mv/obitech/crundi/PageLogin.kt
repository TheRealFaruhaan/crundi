package mv.obitech.crundi

import android.content.Context
import org.json.JSONObject

/**
 * The login of the pages this app shows, kept by the app.
 *
 * Pairing gives a phone two logins: its own (background) one, for what it
 * does with no page open, and one for the pages. The page login used to be
 * handed to the web view once, and from then on every page the app showed
 * (the main window, each bubble) held the refresh token in shared storage
 * and renewed it by itself. A renewal spends the token it uses, and a spent
 * token offered again is how a stolen one looks: when two pages renewed
 * close together, or one renewed with a copy it had read before the other
 * saved, or Android stopped the app before the web view had written the new
 * one to disk, the server cut the whole phone off.
 *
 * Now, with a server that offers it ("app-page-login"), the app keeps the
 * page login here and every page asks for it:
 *  - one renewal at a time, for every page, behind one lock;
 *  - a page that asks with a token someone else has already replaced is
 *    simply given the new one, with no renewal at all;
 *  - the new pair is on disk (commit, not apply) before any page sees it.
 * A renewal whose answer is lost on the network leaves the old token here,
 * and offering it again is what the server forgives (see rotateRefresh).
 *
 * Kept in an entry of its own, not in the Server record: that record is read,
 * changed and written back whole by several threads, and a write from a stale
 * copy would put back a spent refresh token.
 */
object PageLogin {
    private const val PREFS = "crundi"
    private val lock = Any()
    /** Renew this long before expiry rather than hand out a token about to lapse. */
    private const val MARGIN_MS = 60_000L

    class Login(val token: String, val refresh: String, val expiresAt: Long)

    private fun key(serverId: String) = "pagelogin.$serverId"

    private fun read(ctx: Context, serverId: String): Login? {
        val sealed = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString(key(serverId), null) ?: return null
        val j = Vault.open(sealed)?.let { runCatching { JSONObject(it) }.getOrNull() } ?: return null
        return Login(j.optString("token"), j.optString("refresh"), j.optLong("expiresAt"))
    }

    /** Synchronously on disk: a renewed login lost to a killed process is a spent token offered later. */
    private fun write(ctx: Context, serverId: String, login: Login?) {
        val p = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
        if (login == null) p.remove(key(serverId))
        else p.putString(key(serverId), Vault.seal(JSONObject().put("token", login.token).put("refresh", login.refresh).put("expiresAt", login.expiresAt).toString()))
        p.commit()
    }

    private fun fromReply(j: JSONObject, fallbackRefresh: String = ""): Login? {
        val token = j.optString("token")
        if (token.isEmpty()) return null
        val secs = j.optLong("expiresIn", 900L)
        return Login(token, j.optString("refreshToken", fallbackRefresh).ifEmpty { fallbackRefresh }, System.currentTimeMillis() + secs * 1000)
    }

    /** What a page gets: the access token and how long it has left. Never the refresh token. */
    private fun answer(l: Login) = JSONObject()
        .put("token", l.token)
        .put("expiresIn", ((l.expiresAt - System.currentTimeMillis()) / 1000).coerceAtLeast(0))

    /**
     * Take the page login a pairing has just handed over (Server.pageToken),
     * so the first page does not need a renewal. Returns true if one was taken.
     */
    fun adopt(ctx: Context, server: Server, expiresIn: Long = 900L): Boolean = synchronized(lock) {
        if (server.pageRefresh.isEmpty()) return false
        write(ctx, server.id, Login(server.pageToken, server.pageRefresh, System.currentTimeMillis() + expiresIn * 1000))
        true
    }

    /** The current token, if there is one with time left. No network. */
    fun current(ctx: Context, serverId: String): JSONObject? = synchronized(lock) {
        val l = read(ctx, serverId) ?: return null
        if (l.token.isEmpty() || l.expiresAt - System.currentTimeMillis() < MARGIN_MS) return null
        answer(l)
    }

    /**
     * A page's token was refused, or is about to lapse: give it a good one.
     * `stale` is the token the page has. If another page has already renewed
     * since, the page just gets the newer token. Blocking: call from Bg.
     */
    fun renew(ctx: Context, serverId: String, stale: String): JSONObject = synchronized(lock) {
        val server = Servers.byId(ctx, serverId) ?: return JSONObject().put("error", "This phone is not paired with that server")
        val now = System.currentTimeMillis()
        val have = read(ctx, serverId)
        if (have != null && have.token.isNotEmpty() && have.token != stale && have.expiresAt - now > MARGIN_MS) {
            return answer(have)
        }
        if (have != null && have.refresh.isNotEmpty()) {
            val r = Api.request("POST", "${server.baseUrl}/api/auth/refresh", JSONObject().put("refreshToken", have.refresh),
                headers = mapOf("X-Crundi-Page" to "app, page login"))
            val renewed = r.json?.takeIf { r.ok }?.let { fromReply(it, have.refresh) }
            if (renewed != null) { write(ctx, serverId, renewed); return answer(renewed) }
            // No answer at all: keep the token. Offering it again later is safe,
            // because a renewal whose answer never arrived is answered again.
            if (r.status == 0 || r.json == null) return JSONObject().put("error", "Could not reach ${server.host}").put("offline", true)
            // Refused: this page login is over. Forget it, and fall through to
            // asking for a new one with the phone's own login.
            write(ctx, serverId, null)
        }
        val r = Api.authed(ctx, serverId, "POST", "/api/pair/page-login")
        val fresh = r.json?.takeIf { r.ok }?.let { fromReply(it) }
        if (fresh != null) { write(ctx, serverId, fresh); return answer(fresh) }
        if (r.status == 0 || r.json == null) return JSONObject().put("error", "Could not reach ${server.host}").put("offline", true)
        JSONObject().put("error", r.error ?: "This phone's sign-in has ended").put("unpaired", r.status == 401 || r.status == 403)
    }

    /** The server was removed from this phone, or paired again. */
    fun forget(ctx: Context, serverId: String) = synchronized(lock) { write(ctx, serverId, null) }
}
