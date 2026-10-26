package mv.obitech.crundi.api

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.encodeToJsonElement
import mv.obitech.crundi.api.model.Factory

/**
 * Saving a factory from the phone without losing what the phone does not
 * know. The models carry every field src/factory-model.js keeps today, but a
 * newer server may store more: the edited factory is laid over the JSON the
 * server last sent, so a field the app has never heard of goes back as it
 * came. Objects merge key by key; lists of things with an id (people, steps,
 * connections, options, form sections) match by id, so a deleted one is gone
 * and a kept one keeps its unknown fields; any other list is the app's.
 */
object FactoryJson {
    /** Nulls written out, so clearing a value (who answers a decision) reaches the server. */
    private val withNulls = Json(from = CrundiJson) { explicitNulls = true }

    /** The definition to POST: [factory] over [raw] (the server's last copy). */
    fun forSave(factory: Factory, raw: JsonObject?): JsonObject =
        overlay(raw, withNulls.encodeToJsonElement(factory)) as JsonObject

    fun overlay(raw: JsonElement?, typed: JsonElement): JsonElement = when {
        raw is JsonObject && typed is JsonObject -> JsonObject(LinkedHashMap<String, JsonElement>().apply {
            putAll(raw)
            for ((k, v) in typed) {
                // A null the app writes only where the server had the key: no
                // "decision": null on every step that is not a decision.
                if (v is JsonNull && k !in raw) continue
                put(k, overlay(raw[k], v))
            }
        })
        typed is JsonObject -> JsonObject(typed.filterValues { it !is JsonNull }.mapValues { (_, v) -> overlay(null, v) })
        raw is JsonArray && typed is JsonArray && typed.isNotEmpty() && typed.all { idOf(it) != null } -> {
            val before = raw.associateBy { idOf(it) }
            JsonArray(typed.map { overlay(before[idOf(it)], it) })
        }
        typed is JsonArray -> JsonArray(typed.map { overlay(null, it) })
        else -> typed
    }

    private fun idOf(e: JsonElement): String? = ((e as? JsonObject)?.get("id") as? JsonPrimitive)?.takeIf { it.isString }?.contentOrNull
}
