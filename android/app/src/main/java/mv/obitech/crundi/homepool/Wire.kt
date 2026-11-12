package mv.obitech.crundi.homepool

import java.net.Inet4Address
import java.net.Inet6Address
import java.net.InetAddress
import java.net.URI
import java.net.URLDecoder

/*
 * The homepool node's wire format: the same one app/homepool-node.js speaks
 * (that file is the authoritative description, and the server's codec).
 *
 * Binary: [type u8][stream id u32 BE][payload]
 *   OPEN    1  server -> node   payload: JSON {"host","port"}
 *   DATA    2  both            payload: bytes
 *   CLOSE   3  both            payload: empty
 *   OPENED  4  node -> server
 *   ERROR   5  node -> server   payload: a short reason (utf8)
 *   PAUSE   6  both            receiver's socket is full: stop sending on this stream
 *   RESUME  7  both            it drained: carry on
 *
 * Everything in this file is plain JVM code (no Android classes), so the unit
 * tests can run it as it is.
 */

/** Frame types. */
object FrameType {
    const val OPEN = 1
    const val DATA = 2
    const val CLOSE = 3
    const val OPENED = 4
    const val ERROR = 5
    const val PAUSE = 6
    const val RESUME = 7
}

/** Where the node's websocket is, on the server. */
const val NODE_PATH = "/api/homepool/node"

/** Above this many bytes queued on the websocket, stop reading the TCP side; carry on below half. */
const val WS_HIGH_WATER = 1024L * 1024

class Frame(val type: Int, val id: Int, val payload: ByteArray) {
    /** The stream id as the unsigned number it is on the wire. */
    val streamId: Long get() = id.toLong() and 0xFFFFFFFFL
}

object Wire {
    fun encode(type: Int, id: Int, payload: ByteArray? = null): ByteArray {
        val p = payload ?: EMPTY
        val b = ByteArray(5 + p.size)
        b[0] = type.toByte()
        b[1] = (id ushr 24).toByte()
        b[2] = (id ushr 16).toByte()
        b[3] = (id ushr 8).toByte()
        b[4] = id.toByte()
        System.arraycopy(p, 0, b, 5, p.size)
        return b
    }

    fun encode(type: Int, id: Int, text: String): ByteArray = encode(type, id, text.toByteArray(Charsets.UTF_8))

    /** Null for anything shorter than a header. */
    fun decode(b: ByteArray?): Frame? {
        if (b == null || b.size < 5) return null
        val id = ((b[1].toInt() and 0xFF) shl 24) or ((b[2].toInt() and 0xFF) shl 16) or
            ((b[3].toInt() and 0xFF) shl 8) or (b[4].toInt() and 0xFF)
        return Frame(b[0].toInt() and 0xFF, id, b.copyOfRange(5, b.size))
    }

    private val EMPTY = ByteArray(0)
}

/**
 * The connection code a node is set up from, scanned or pasted:
 *   crundi-homepool://node?s=<url-encoded server url>&k=<node key>
 * The key alone is enough; it is not a Crundi login and opens nothing else.
 */
object NodeCode {
    const val SCHEME = "crundi-homepool"

    fun make(server: String, key: String): String =
        "$SCHEME://node?s=${java.net.URLEncoder.encode(server, "UTF-8")}&k=${java.net.URLEncoder.encode(key, "UTF-8")}"

    /** (server without trailing slash, key) or null. Same rules as parseNodeCode in homepool-node.js. */
    fun parse(code: String?): Pair<String, String>? {
        val text = code?.trim().orEmpty()
        if (text.isEmpty()) return null
        val uri = try { URI(text) } catch (e: Exception) { return null }
        if (!SCHEME.equals(uri.scheme, ignoreCase = true)) return null
        val q = query(uri.rawQuery ?: return null)
        val s = q["s"].orEmpty()
        val k = q["k"].orEmpty()
        if (!Regex("^https?://", RegexOption.IGNORE_CASE).containsMatchIn(s) || !k.startsWith("hpn_")) return null
        return s.trimEnd('/') to k
    }

    private fun query(raw: String): Map<String, String> {
        val out = HashMap<String, String>()
        for (part in raw.split('&')) {
            if (part.isEmpty()) continue
            val i = part.indexOf('=')
            val name = dec(if (i < 0) part else part.substring(0, i))
            val value = if (i < 0) "" else dec(part.substring(i + 1))
            if (name !in out) out[name] = value   // the first wins, as URLSearchParams.get does
        }
        return out
    }

    private fun dec(s: String): String = try { URLDecoder.decode(s, "UTF-8") } catch (e: Exception) { s }
}

/**
 * May a node open this? Never the node's own machine or its private network.
 * The same list as isPrivateTarget in homepool-node.js, plus the addresses a
 * host NAME resolves to (checked separately, with [isPrivateAddress], after
 * the phone has resolved it: the name alone proves nothing).
 */
object Targets {
    /** By the host as given: names that are private by their form, and IP literals. Does no DNS. */
    fun isPrivateHost(host: String?): Boolean {
        val h = host.orEmpty().trim().lowercase().removePrefix("[").removeSuffix("]").trimEnd('.')
        if (h.isEmpty()) return true
        if (h == "localhost" || h.endsWith(".localhost") || h.endsWith(".local") || h.endsWith(".internal")) return true
        val addr = literal(h) ?: return false
        return isPrivateAddress(addr)
    }

    /** An IP literal (v4 dotted quad, or v6), parsed without any DNS; null for a name. */
    fun literal(h: String): InetAddress? {
        if (IPV4.matches(h)) {
            val parts = h.split('.').map { it.toInt() }
            if (parts.any { it > 255 }) return null
            return InetAddress.getByAddress(ByteArray(4) { parts[it].toByte() })
        }
        if (h.contains(':') && IPV6_CHARS.matches(h)) {
            // A literal with a colon is never looked up by getByName.
            return try { InetAddress.getByName(h.substringBefore('%')) } catch (e: Exception) { null }
        }
        return null
    }

    fun isPrivateAddress(a: InetAddress): Boolean {
        val b = a.address
        if (a is Inet4Address) return v4Private(b)
        if (a is Inet6Address) {
            // An IPv4 address carried in IPv6 (mapped ::ffff:a.b.c.d, or the old compatible ::a.b.c.d).
            if (a.isLoopbackAddress || a.isAnyLocalAddress) return true
            val mapped = (0 until 10).all { b[it].toInt() == 0 } && (b[10].toInt() and 0xFF) == 0xFF && (b[11].toInt() and 0xFF) == 0xFF
            val compat = (0 until 12).all { b[it].toInt() == 0 }
            if (mapped || compat) return v4Private(b.copyOfRange(12, 16))
            val b0 = b[0].toInt() and 0xFF
            val b1 = b[1].toInt() and 0xFF
            return (b0 and 0xFE) == 0xFC ||                 // fc00::/7 unique local
                (b0 == 0xFE && (b1 and 0xC0) == 0x80) || // fe80::/10 link local
                (b0 == 0xFE && (b1 and 0xC0) == 0xC0) || // fec0::/10 old site local
                b0 == 0xFF                              // multicast
        }
        return true
    }

    private fun v4Private(b: ByteArray): Boolean {
        val a0 = b[0].toInt() and 0xFF
        val a1 = b[1].toInt() and 0xFF
        return a0 == 10 || a0 == 127 || a0 == 0 || (a0 == 169 && a1 == 254) || (a0 == 172 && a1 in 16..31) ||
            (a0 == 192 && a1 == 168) || (a0 == 100 && a1 in 64..127) || a0 >= 224
    }

    private val IPV4 = Regex("^\\d{1,3}\\.\\d{1,3}\\.\\d{1,3}\\.\\d{1,3}$")
    private val IPV6_CHARS = Regex("^[0-9a-f:.%a-z]+$")
}
