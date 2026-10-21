package mv.obitech.crundi.api

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import java.io.FilterOutputStream
import java.io.InputStream
import java.io.OutputStream
import java.util.Base64

/** Builds the server's upload bodies. */
object Uploads {
    /**
     * `{...fields, "data": "<base64 of source>"}`, streamed: the file is read
     * in chunks and encoded as it goes out, never held whole.
     */
    fun base64Json(fields: Map<String, Any?>, size: Long, source: () -> InputStream): Upload {
        val head = JsonObject(fields.filterValues { it != null }.mapValues { (_, v) -> toJson(v) }).toString()
        val prefix = (if (head == "{}") "{" else head.dropLast(1) + ",") + "\"data\":\""
        val bytes = prefix.toByteArray(Charsets.UTF_8)
        val b64 = if (size >= 0) ((size + 2) / 3) * 4 else -1
        val length = if (b64 >= 0) bytes.size + b64 + 2 else -1
        return Upload("application/json", length, write = { out, count ->
            out.write(bytes)
            // Base64's own stream closes what it wraps; the connection is not its to close.
            val enc = Base64.getEncoder().wrap(object : FilterOutputStream(out) {
                override fun write(b: ByteArray, off: Int, len: Int) { out.write(b, off, len) }
                override fun close() { flush() }
            })
            source().use { ins ->
                val buf = ByteArray(64 * 1024)
                var done = 0L
                while (true) {
                    val n = ins.read(buf)
                    if (n < 0) break
                    enc.write(buf, 0, n)
                    done += n
                    count(done)
                }
            }
            enc.close()
            out.write("\"}".toByteArray(Charsets.UTF_8))
        }, sourceSize = size)
    }

    internal fun toJson(v: Any?): kotlinx.serialization.json.JsonElement = when (v) {
        null -> kotlinx.serialization.json.JsonNull
        is kotlinx.serialization.json.JsonElement -> v
        is String -> JsonPrimitive(v)
        is Number -> JsonPrimitive(v)
        is Boolean -> JsonPrimitive(v)
        is Map<*, *> -> JsonObject(v.entries.associate { (k, x) -> k.toString() to toJson(x) })
        is Iterable<*> -> kotlinx.serialization.json.JsonArray(v.map { toJson(it) })
        else -> JsonPrimitive(v.toString())
    }
}

/** Write a body into memory (fakes and tests). */
fun Upload.toBytes(): ByteArray = java.io.ByteArrayOutputStream().also { write(it) {} }.toByteArray()

