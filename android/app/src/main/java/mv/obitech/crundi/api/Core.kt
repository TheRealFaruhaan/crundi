package mv.obitech.crundi.api

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonPrimitive

/**
 * How every answer from the server is read. Lenient on purpose: the server
 * adds fields freely, sends numbers where a string was once, and leaves
 * fields out; a model only names what the app uses, and a missing field
 * takes its default rather than failing the whole screen.
 */
val CrundiJson: Json = Json {
    ignoreUnknownKeys = true
    isLenient = true
    coerceInputValues = true
    explicitNulls = false
    encodeDefaults = true
}

/**
 * The server said no, or could not be reached. [message] is fit to show to
 * the person: the server's own words when it gave any, otherwise what
 * happened in plain terms.
 */
class ApiException(
    /** HTTP status; 0 when the server could not be reached at all. */
    val status: Int,
    override val message: String,
    /** The raw answer, when there was one (for the odd screen that needs a field off an error). */
    val body: JsonObject? = null,
) : Exception(message) {
    /** No network, a timeout, a dropped connection. Worth a "try again". */
    val offline: Boolean get() = status == 0
    /** This phone's sign-in is over: the app should offer to pair again (Host.pairAgain). */
    val signedOut: Boolean get() = status == 401
    /** Refused: not allowed (a collaborator, a route the server will not do for this caller). */
    val forbidden: Boolean get() = status == 403
    val notFound: Boolean get() = status == 404
}

/** Small readers for the loosely shaped parts of the API (tool inputs, settings). */
fun JsonElement?.str(): String? = (this as? JsonPrimitive)?.takeIf { it !is JsonNull }?.contentOrNull
fun JsonObject.str(key: String): String? = this[key].str()
fun JsonObject.bool(key: String): Boolean? = (this[key] as? JsonPrimitive)?.booleanOrNull
fun JsonObject.obj(key: String): JsonObject? = this[key] as? JsonObject
fun JsonObject.long(key: String): Long? = (this[key] as? JsonPrimitive)?.contentOrNull?.toDoubleOrNull()?.toLong()

/** The server's error text in an answer, if it gave one. */
internal fun JsonObject.errorText(): String? =
    (this["error"] as? JsonPrimitive)?.contentOrNull?.takeIf { it.isNotBlank() }
        ?: (this["message"] as? JsonPrimitive)?.contentOrNull?.takeIf { it.isNotBlank() && this["ok"]?.jsonPrimitive?.booleanOrNull == false }
