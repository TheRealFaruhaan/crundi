package mv.obitech.crundi.api

import android.content.Context
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.decodeFromJsonElement
import mv.obitech.crundi.PageLogin

/**
 * JSON calls over a [Transport]. Every answer is checked the same way: a
 * non-2xx status, or `{"ok": false}` (which the server often sends with 200),
 * becomes an [ApiException] carrying the server's own words.
 */
class Rpc(val transport: Transport, private val onSignedOut: () -> Unit = {}) {

    suspend fun call(method: String, path: String, query: Map<String, String?> = emptyMap(), body: JsonElement? = null, slow: Boolean = false, upload: Upload? = null, progress: Progress? = null): JsonObject {
        val r = try { transport.exchange(ApiRequest(method, path, query, body, upload, slow = slow), progress) } catch (e: ApiException) {
            if (e.signedOut) onSignedOut()
            throw e
        }
        if (r.status == 401) onSignedOut()
        val parsed = runCatching { CrundiJson.parseToJsonElement(r.text) }.getOrNull()
        val obj = parsed as? JsonObject
        if (r.status !in 200..299) throw ApiException(r.status, obj?.errorText() ?: OkHttpTransport.defaultMessage(r.status, r.text), obj)
        if (obj == null) {
            // A few routes answer with a bare array; wrap it so callers can read "items".
            if (parsed != null) return JsonObject(mapOf("items" to parsed))
            throw ApiException(r.status, "The server's answer could not be read.")
        }
        if ((obj["ok"] as? JsonPrimitive)?.booleanOrNull == false) throw ApiException(r.status, obj.errorText() ?: "The server could not do that.", obj)
        return obj
    }

    suspend fun get(path: String, query: Map<String, String?> = emptyMap(), slow: Boolean = false) = call("GET", path, query, slow = slow)
    suspend fun post(path: String, body: Map<String, Any?> = emptyMap(), query: Map<String, String?> = emptyMap(), slow: Boolean = false) =
        call("POST", path, query, Uploads.toJson(body.filterValues { it != null }), slow)
    suspend fun delete(path: String) = call("DELETE", path)
    suspend fun patch(path: String, body: Map<String, Any?>) = call("PATCH", path, body = Uploads.toJson(body))

    /** A field of an answer as a model; the whole answer when [key] is null. Missing → [fallback] or an error. */
    inline fun <reified T> read(obj: JsonObject, key: String?, fallback: T? = null): T {
        val el: JsonElement = if (key == null) obj else (obj[key] ?: JsonNull)
        if (el is JsonNull && fallback != null) return fallback
        return try { CrundiJson.decodeFromJsonElement<T>(el) } catch (e: Exception) {
            fallback ?: throw ApiException(200, "The server's answer was not what this app expected (${key ?: "body"}).", obj)
        }
    }
}

/** The phone's page login, kept and renewed by [PageLogin] (one renewal at a time for the whole app). */
class PageLoginCredentials(private val app: Context, private val serverId: String) : Credentials {
    override fun token(): String? = PageLogin.current(app, serverId)?.optString("token")?.takeIf { it.isNotEmpty() }

    override fun renew(stale: String?): String {
        val r = PageLogin.renew(app, serverId, stale ?: "")
        val token = r.optString("token")
        if (token.isNotEmpty()) return token
        val msg = r.optString("error").ifEmpty { "Could not sign in to the server" }
        throw ApiException(if (r.optBoolean("unpaired")) 401 else 0, msg)
    }
}
