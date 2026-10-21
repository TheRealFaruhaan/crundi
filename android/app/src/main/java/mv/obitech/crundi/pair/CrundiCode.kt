package mv.obitech.crundi.pair

import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.floor
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * The Crundi code, the picture a phone scans to pair: a Kotlin port of
 * src/crundi-code.js (the reader half, plus `encode` so tests can draw one).
 *
 * The server draws the code with that file and this reads it; the two must
 * agree bit for bit. CrundiCodeTest holds this port to fixtures the server's
 * own encoder wrote (android/tools/gen-code-fixtures.mjs) and reads codes in
 * pictures painted the way the server's test paints them.
 *
 * Plain arrays and arithmetic, no Android: it runs in a JVM unit test.
 *
 * THE FORMAT IS THE SERVER'S. Change nothing here without the same change in
 * src/crundi-code.js, and the order of HOST_WORDS never changes.
 */
object CrundiCode {
    const val R_INNER = 0.38
    const val R_OUTER = 1.0
    val RING_RADII = doubleArrayOf(0.50, 0.60, 0.70, 0.80, 0.90)
    const val FINDER_WIDTH = 0.035

    class Cls(val name: String, val cells: IntArray, val parity: Int) {
        val bits = cells.sumOf { it - 1 }
        val bytes = bits / 8
        val payload = bytes - parity
        val stroke = 0.62 * ((2 * PI * RING_RADII[0]) / cells[0])
    }

    val CLASSES = listOf(
        Cls("S", intArrayOf(40, 48, 56, 64, 72), 8),
        Cls("L", intArrayOf(55, 66, 77, 88, 99), 10),
        Cls("XL", intArrayOf(70, 84, 98, 112, 126), 12),
        Cls("XXL", intArrayOf(85, 102, 119, 136, 153), 14),
    )

    private const val CODE_BYTES = 12
    private const val HOST_CHARS = "abcdefghijklmnopqrstuvwxyz0123456789-."
    private val HOST_WORDS = listOf(
        ".trycloudflare.com", ".cfargotunnel.com", ".ngrok-free.app", ".ngrok.io", ".duckdns.org", ".ts.net",
        ".com", ".net", ".org", ".dev", ".app", ".io", ".co.uk", ".local", ".lan", ".home.arpa",
        "www.", "192.168.", "10.0.", "172.16.",
    )
    private val WORDS_BY_LENGTH = HOST_WORDS.mapIndexed { i, w -> w to (HOST_CHARS.length + i) }.sortedByDescending { it.first.length }

    private fun hostSymbols(host: String): List<Int>? {
        val out = ArrayList<Int>()
        var i = 0
        while (i < host.length) {
            val word = WORDS_BY_LENGTH.firstOrNull { host.startsWith(it.first, i) }
            if (word != null) { out += word.second; i += word.first.length; continue }
            val v = HOST_CHARS.indexOf(host[i])
            if (v < 0) return null
            out += v; i++
        }
        return out
    }

    private fun hostBytes(symbols: Int) = (symbols * 6 + 7) / 8

    // ─── Reed-Solomon over GF(256), polynomial 0x11d ───
    private val EXP = IntArray(512)
    private val LOG = IntArray(256)

    init {
        var x = 1
        for (i in 0 until 255) { EXP[i] = x; LOG[x] = i; x = x shl 1; if (x and 0x100 != 0) x = x xor 0x11d }
        for (i in 255 until 512) EXP[i] = EXP[i - 255]
    }

    private fun mul(a: Int, b: Int) = if (a != 0 && b != 0) EXP[LOG[a] + LOG[b]] else 0
    private fun div(a: Int, b: Int) = if (a != 0) EXP[LOG[a] + 255 - LOG[b]] else 0
    private fun pow(n: Int) = EXP[((n % 255) + 255) % 255]

    fun rsEncode(data: IntArray, nsym: Int): IntArray {
        var g = intArrayOf(1)
        for (i in 0 until nsym) {
            val next = IntArray(g.size + 1)
            for (j in g.indices) { next[j] = next[j] xor g[j]; next[j + 1] = next[j + 1] xor mul(g[j], EXP[i]) }
            g = next
        }
        val rem = ArrayDeque(List(nsym) { 0 })
        for (byte in data) {
            val f = byte xor rem.removeFirst()
            rem.addLast(0)
            for (i in 0 until nsym) rem[i] = rem[i] xor mul(g[i + 1], f)
        }
        return data + rem.toIntArray()
    }

    /** Puts right up to nsym/2 wrong bytes; null when there are more (never a guess). */
    fun rsDecode(word: IntArray, nsym: Int): IntArray? {
        val n = word.size
        val msg = word.copyOf()
        fun evalAt(x: Int): Int { var y = 0; for (j in 0 until n) y = mul(y, x) xor msg[j]; return y }
        val synd = IntArray(nsym)
        var clean = true
        for (i in 0 until nsym) { synd[i] = evalAt(EXP[i]); if (synd[i] != 0) clean = false }
        if (clean) return msg.copyOf(n - nsym)

        var c = mutableListOf(1)
        var bPoly = mutableListOf(1)
        var l = 0; var m = 1; var b = 1
        for (k in 0 until nsym) {
            var d = synd[k]
            for (i in 1..l) d = d xor mul(c.getOrElse(i) { 0 }, synd[k - i])
            if (d == 0) { m++; continue }
            val t = c.toMutableList()
            val coef = div(d, b)
            while (c.size < bPoly.size + m) c.add(0)
            for (i in bPoly.indices) c[i + m] = c[i + m] xor mul(coef, bPoly[i])
            if (2 * l <= k) { l = k + 1 - l; bPoly = t; b = d; m = 1 } else m++
        }
        if (l * 2 > nsym) return null

        val at = ArrayList<Int>()
        for (p in 0 until n) {
            var y = 0
            for (i in l downTo 0) y = mul(y, pow(-p)) xor c.getOrElse(i) { 0 }
            if (y == 0) at += p
        }
        if (at.size != l) return null

        val omega = IntArray(nsym)
        for (i in 0 until nsym) for (j in 0..min(i, l)) omega[i] = omega[i] xor mul(synd[i - j], c.getOrElse(j) { 0 })
        for (p in at) {
            val xi = pow(-p)
            var num = 0; var den = 0
            for (i in nsym - 1 downTo 0) num = mul(num, xi) xor omega[i]
            var i = 1
            while (i <= l) { den = den xor mul(c.getOrElse(i) { 0 }, pow(-p * (i - 1))); i += 2 }
            if (den == 0) return null
            msg[n - 1 - p] = msg[n - 1 - p] xor mul(pow(p), div(num, den))
        }
        for (i in 0 until nsym) if (evalAt(EXP[i]) != 0) return null
        return msg.copyOf(n - nsym)
    }

    private fun whiten(bytes: IntArray): IntArray {
        val out = IntArray(bytes.size)
        var s = 0x5a
        for (i in bytes.indices) { s = (s * 73 + 41) and 0xff; out[i] = bytes[i] xor s xor ((i * 29) and 0xff) }
        return out
    }

    private class Parts(val https: Boolean, val symbols: List<Int>, val port: Int, val code: String)

    private val LINK = Regex("^(https?)://([a-z0-9.-]{1,253})(?::(\\d{1,5}))?/pair#([a-f0-9]{24})$")

    private fun splitLink(link: String): Parts? {
        val m = LINK.matchEntire(link) ?: return null
        val port = m.groupValues[3].takeIf { it.isNotEmpty() }?.toInt() ?: 0
        if (port > 65535) return null
        val symbols = hostSymbols(m.groupValues[2]) ?: return null
        if (symbols.size > 63) return null
        return Parts(m.groupValues[1] == "https", symbols, port, m.groupValues[4])
    }

    private fun pack(parts: Parts, size: Int): IntArray? {
        val need = 1 + CODE_BYTES + (if (parts.port != 0) 2 else 0) + hostBytes(parts.symbols.size)
        if (need > size) return null
        val out = IntArray(size)
        var o = 0
        out[o++] = (if (parts.https) 0x80 else 0) or (if (parts.port != 0) 0x40 else 0) or parts.symbols.size
        for (i in 0 until CODE_BYTES) out[o++] = parts.code.substring(i * 2, i * 2 + 2).toInt(16)
        if (parts.port != 0) { out[o++] = parts.port shr 8; out[o++] = parts.port and 0xff }
        var acc = 0; var n = 0
        for (v in parts.symbols) {
            acc = (acc shl 6) or v; n += 6
            while (n >= 8) { out[o++] = (acc shr (n - 8)) and 0xff; n -= 8 }
            acc = acc and ((1 shl n) - 1)
        }
        if (n != 0) out[o++] = (acc shl (8 - n)) and 0xff
        return out
    }

    private fun unpack(bytes: IntArray): String? {
        var o = 0
        val head = bytes[o++]
        val len = head and 0x3f
        if (len == 0) return null
        val code = StringBuilder()
        for (i in 0 until CODE_BYTES) code.append(bytes[o++].toString(16).padStart(2, '0'))
        var port = 0
        if (head and 0x40 != 0) { port = (bytes[o] shl 8) or bytes[o + 1]; o += 2; if (port == 0) return null }
        if (o + hostBytes(len) > bytes.size) return null
        val host = StringBuilder()
        var acc = 0; var n = 0; var read = 0
        while (read < len) {
            acc = ((acc shl 8) or bytes[o++]) and 0xffff; n += 8
            while (n >= 6 && read < len) {
                val v = (acc shr (n - 6)) and 0x3f; n -= 6; read++
                val piece = if (v < HOST_CHARS.length) HOST_CHARS[v].toString() else HOST_WORDS.getOrNull(v - HOST_CHARS.length) ?: return null
                host.append(piece)
            }
        }
        while (o < bytes.size) { if (bytes[o] != 0) return null; o++ }
        return "${if (head and 0x80 != 0) "https" else "http"}://$host${if (port != 0) ":$port" else ""}/pair#$code"
    }

    class Encoded(val cls: Cls, val rings: List<IntArray>)

    /** The rings for a pairing link (cell 0 of each is the amber marker), or null if it cannot be carried. */
    fun encode(link: String): Encoded? {
        val parts = splitLink(link) ?: return null
        for (cls in CLASSES) {
            val payload = pack(parts, cls.payload) ?: continue
            val word = whiten(rsEncode(payload, cls.parity))
            val rings = ArrayList<IntArray>()
            var bit = 0
            for (n in cls.cells) {
                val ring = IntArray(n)
                ring[0] = 1
                for (i in 1 until n) {
                    ring[i] = if (bit < word.size * 8) (word[bit shr 3] shr (7 - (bit and 7))) and 1 else bit and 1
                    bit++
                }
                rings += ring
            }
            return Encoded(cls, rings)
        }
        return null
    }

    /** The link back from rings already read in order, or null. */
    fun decodeRings(rings: List<IntArray>, cls: Cls): String? {
        val word = IntArray(cls.bytes)
        var bit = 0
        for (k in cls.cells.indices) {
            for (i in 1 until cls.cells[k]) {
                if (bit < cls.bytes * 8 && rings[k][i] != 0) word[bit shr 3] = word[bit shr 3] or (1 shl (7 - (bit and 7)))
                bit++
            }
        }
        val payload = rsDecode(whiten(word), cls.parity) ?: return null
        return unpack(payload)
    }

    // ─── Reading ───

    private class Ellipse(
        val cx: Double, val cy: Double,
        val p11: Double, val p12: Double, val p22: Double,
        val size: Double,
    )

    private fun fitEllipse(xs: IntArray, ys: IntArray, count: Int, stride: Int): Ellipse? {
        var cx = 0.0; var cy = 0.0; var n = 0
        run { var i = 0; while (i < count) { cx += xs[i]; cy += ys[i]; n++; i += stride } }
        cx /= n; cy /= n
        var sc = 0.0
        run { var i = 0; while (i < count) { sc += abs(xs[i] - cx) + abs(ys[i] - cy); i += stride } }
        sc = (sc / n).let { if (it == 0.0 || it.isNaN()) 1.0 else it }
        val mm = Array(5) { DoubleArray(6) }
        val row = DoubleArray(5)
        run {
            var i = 0
            while (i < count) {
                val x = (xs[i] - cx) / sc; val y = (ys[i] - cy) / sc
                row[0] = x * x; row[1] = x * y; row[2] = y * y; row[3] = x; row[4] = y
                for (a in 0 until 5) { for (b in 0 until 5) mm[a][b] += row[a] * row[b]; mm[a][5] += row[a] }
                i += stride
            }
        }
        for (c in 0 until 5) {
            var best = c
            for (r in c + 1 until 5) if (abs(mm[r][c]) > abs(mm[best][c])) best = r
            if (abs(mm[best][c]) < 1e-12) return null
            val tmp = mm[c]; mm[c] = mm[best]; mm[best] = tmp
            for (r in 0 until 5) {
                if (r == c) continue
                val k = mm[r][c] / mm[c][c]
                for (j in c until 6) mm[r][j] -= k * mm[c][j]
            }
        }
        val a = mm[0][5] / mm[0][0]; val b = mm[1][5] / mm[1][1]; val c = mm[2][5] / mm[2][2]
        val d = mm[3][5] / mm[3][3]; val e = mm[4][5] / mm[4][4]
        val det = 4 * a * c - b * b
        if (det <= 1e-12) return null
        val x0 = (b * e - 2 * c * d) / det; val y0 = (b * d - 2 * a * e) / det
        val k = 1 - (a * x0 * x0 + b * x0 * y0 + c * y0 * y0 + d * x0 + e * y0)
        if (k <= 0) return null
        val p11 = a / k; val p12 = b / (2 * k); val p22 = c / k
        val tr = p11 + p22; val disc = sqrt(max(0.0, (p11 - p22) * (p11 - p22) / 4 + p12 * p12))
        val l1 = tr / 2 + disc; val l2 = tr / 2 - disc
        if (l2 <= 0) return null
        val s1 = 1 / sqrt(l1); val s2 = 1 / sqrt(l2)
        return Ellipse(cx + x0 * sc, cy + y0 * sc, p11 / (sc * sc), p12 / (sc * sc), p22 / (sc * sc), sqrt(s1 * s2) * sc)
    }

    private fun findRings(v: IntArray, w: Int, h: Int): List<Ellipse> {
        val integ = DoubleArray((w + 1) * (h + 1))
        for (y in 0 until h) {
            var row = 0.0
            for (x in 0 until w) { row += v[y * w + x]; integ[(y + 1) * (w + 1) + x + 1] = integ[y * (w + 1) + x + 1] + row }
        }
        val half = max(8, (min(w, h) / 12.0).roundToInt())
        val on = ByteArray(w * h)
        for (y in 0 until h) {
            val y0 = max(0, y - half); val y1 = min(h, y + half + 1)
            for (x in 0 until w) {
                val x0 = max(0, x - half); val x1 = min(w, x + half + 1)
                val sum = integ[y1 * (w + 1) + x1] - integ[y0 * (w + 1) + x1] - integ[y1 * (w + 1) + x0] + integ[y0 * (w + 1) + x0]
                if (v[y * w + x] > (sum / ((x1 - x0) * (y1 - y0))) * 1.3 + 14) on[y * w + x] = 1
            }
        }
        val rings = ArrayList<Ellipse>()
        val xs = IntArray(w * h); val ys = IntArray(w * h)
        val minSide = max(14.0, min(w, h) / 24.0)
        for (start in 0 until w * h) {
            if (on[start].toInt() != 1) continue
            var count: Int
            var minX = w; var maxX = 0; var minY = h; var maxY = 0
            xs[0] = start % w; ys[0] = start / w; on[start] = 2; count = 1
            var i = 0
            while (i < count) {
                val x = xs[i]; val y = ys[i]
                if (x < minX) minX = x; if (x > maxX) maxX = x; if (y < minY) minY = y; if (y > maxY) maxY = y
                for (dy in -1..1) {
                    val yy = y + dy; if (yy < 0 || yy >= h) continue
                    for (dx in -1..1) {
                        val xx = x + dx; if (xx < 0 || xx >= w) continue
                        val j = yy * w + xx
                        if (on[j].toInt() == 1) { on[j] = 2; xs[count] = xx; ys[count] = yy; count++ }
                    }
                }
                i++
            }
            val bw = maxX - minX + 1; val bh = maxY - minY + 1
            if (bw < minSide || bh < minSide || bw > bh * 2.5 || bh > bw * 2.5) continue
            if (count > bw * bh * 0.4 || count < (bw + bh)) continue
            val e = fitEllipse(xs, ys, count, max(1, count / 3000)) ?: continue
            var dev = 0.0; var seen = 0; var n = 0
            val stride = max(1, count / 2000)
            var q = 0
            while (q < count) {
                val dx = xs[q] - e.cx; val dy = ys[q] - e.cy
                dev += abs(sqrt(e.p11 * dx * dx + 2 * e.p12 * dx * dy + e.p22 * dy * dy) - 1); n++
                seen = seen or (1 shl (floor(((atan2(dy, dx) + PI) / (2 * PI)) * 24).toInt() % 24))
                q += stride
            }
            if (dev / n > 0.07 || seen != 0xffffff) continue
            rings += e
        }
        return rings
    }

    private class Flat(val x0: Double, val y0: Double, val s11: Double, val s12: Double, val s22: Double)

    private fun conicEllipse(q: DoubleArray): Flat? {
        val (a, b, c) = Triple(q[0], q[1], q[2]); val d = q[3]; val e = q[4]; val f = q[5]
        val det = a * c - b * b
        if (abs(det) < 1e-12) return null
        val x0 = (b * e - c * d) / det; val y0 = (b * d - a * e) / det
        val k = -(a * x0 * x0 + 2 * b * x0 * y0 + c * y0 * y0 + 2 * d * x0 + 2 * e * y0 + f)
        if (!(k > 0) && !(k < 0)) return null
        val p11 = a / k; val p12 = b / k; val p22 = c / k
        val tr = p11 + p22; val disc = sqrt(max(0.0, (p11 - p22) * (p11 - p22) / 4 + p12 * p12))
        val l1 = tr / 2 + disc; val l2 = tr / 2 - disc
        if (l2 <= 0) return null
        var vx = p12; var vy = l1 - p11
        if (abs(vx) + abs(vy) < 1e-12) { vx = 1.0; vy = 0.0 }
        val vn = hypot(vx, vy); vx /= vn; vy /= vn
        val s1 = 1 / sqrt(l1); val s2 = 1 / sqrt(l2)
        return Flat(x0, y0, s1 * vx * vx + s2 * vy * vy, (s1 - s2) * vx * vy, s1 * vy * vy + s2 * vx * vx)
    }

    private fun interface At { fun at(radius: Double, ux: Double, uy: Double): DoubleArray }

    private fun locate(inner: Ellipse, outer: Ellipse): At? {
        val scale = outer.size; val ox = outer.cx; val oy = outer.cy
        fun conic(e: Ellipse): DoubleArray {
            val cx = (e.cx - ox) / scale; val cy = (e.cy - oy) / scale
            val a = e.p11 * scale * scale; val b = e.p12 * scale * scale; val c = e.p22 * scale * scale
            return doubleArrayOf(a, b, c, -(a * cx + b * cy), -(b * cx + c * cy), a * cx * cx + 2 * b * cx * cy + c * cy * cy - 1)
        }
        fun straighten(q: DoubleArray, l1: Double, l2: Double): DoubleArray {
            val (a, b, c) = Triple(q[0], q[1], q[2]); val d = q[3]; val e = q[4]; val f = q[5]
            return doubleArrayOf(
                a - 2 * d * l1 + f * l1 * l1, b - d * l2 - e * l1 + f * l1 * l2, c - 2 * e * l2 + f * l2 * l2,
                d - f * l1, e - f * l2, f,
            )
        }
        val ci = conic(inner); val co = conic(outer)
        fun miss(l1: Double, l2: Double): DoubleArray? {
            val a = conicEllipse(straighten(ci, l1, l2)) ?: return null
            val b = conicEllipse(straighten(co, l1, l2)) ?: return null
            return doubleArrayOf(a.x0 - b.x0, a.y0 - b.y0)
        }
        var l1 = 0.0; var l2 = 0.0
        for (it in 0 until 12) {
            val m = miss(l1, l2)
            if (m == null) { l1 = 0.0; l2 = 0.0; break }
            if (hypot(m[0], m[1]) < 1e-6) break
            val hh = 1e-4
            val mx = miss(l1 + hh, l2); val my = miss(l1, l2 + hh)
            if (mx == null || my == null) { l1 = 0.0; l2 = 0.0; break }
            val j11 = (mx[0] - m[0]) / hh; val j21 = (mx[1] - m[1]) / hh; val j12 = (my[0] - m[0]) / hh; val j22 = (my[1] - m[1]) / hh
            val det = j11 * j22 - j12 * j21
            if (abs(det) < 1e-9) break
            l1 -= (j22 * m[0] - j12 * m[1]) / det
            l2 -= (j11 * m[1] - j21 * m[0]) / det
            if (hypot(l1, l2) > 0.9) { l1 = 0.0; l2 = 0.0; break }
        }
        val flat = conicEllipse(straighten(co, l1, l2)) ?: conicEllipse(co) ?: return null
        val fl1 = l1; val fl2 = l2
        return At { radius, ux, uy ->
            val k = radius / R_OUTER
            val qx = flat.x0 + k * (flat.s11 * ux + flat.s12 * uy); val qy = flat.y0 + k * (flat.s12 * ux + flat.s22 * uy)
            val ww = 1 - fl1 * qx - fl2 * qy
            doubleArrayOf(ox + (scale * qx) / ww, oy + (scale * qy) / ww)
        }
    }

    /** A picture: RGBA bytes, 4 per pixel, row by row. */
    class Image(val data: ByteArray, val width: Int, val height: Int)

    private fun readRing(img: Image, at: At, radius: Double, cells: Int, stroke: Double): IntArray? {
        val data = img.data; val w = img.width; val h = img.height
        val os = 6; val total = cells * os
        val lum = FloatArray(total); val amber = FloatArray(total)
        val offs = doubleArrayOf(-0.25, 0.0, 0.25)
        for (j in 0 until total) {
            val a = (2 * PI * j) / total; val ux = cos(a); val uy = sin(a)
            var r = 0; var g = 0; var b = 0
            for (off in offs) {
                val p = at.at(radius + off * stroke, ux, uy)
                val x = Math.round(p[0]).toInt(); val y = Math.round(p[1]).toInt()
                if (x < 0 || y < 0 || x >= w || y >= h) return null
                val i = (y * w + x) * 4
                r += data[i].toInt() and 0xff; g += data[i + 1].toInt() and 0xff; b += data[i + 2].toInt() and 0xff
            }
            lum[j] = max(r, max(g, b)) / 3f
            amber[j] = ((r + g) / 2f - b) / 3f
        }
        var bestPhase = 0; var bestScore = -1.0; var bestVals = FloatArray(0)
        for (ph in 0 until os) {
            val vals = FloatArray(cells)
            var sum = 0.0; var sq = 0.0
            for (i in 0 until cells) {
                val c = ph + i * os + (os shr 1)
                val x = (lum[(c - 1) % total] + lum[c % total] + lum[(c + 1) % total]) / 3
                vals[i] = x; sum += x; sq += x.toDouble() * x
            }
            val score = sq / cells - (sum / cells) * (sum / cells)
            if (score > bestScore) { bestScore = score; bestPhase = ph; bestVals = vals }
        }
        var lo = 255f; var hi = 0f
        for (x in bestVals) { if (x < lo) lo = x; if (x > hi) hi = x }
        if (hi - lo < 30) return null
        val mid = (lo + hi) / 2
        var sync = -1; var syncScore = 0f
        val bits = IntArray(cells)
        for (i in 0 until cells) {
            if (bestVals[i] <= mid) continue
            bits[i] = 1
            val c = bestPhase + i * os + (os shr 1)
            val a = (amber[(c - 1) % total] + amber[c % total] + amber[(c + 1) % total]) / 3
            if (a > syncScore) { syncScore = a; sync = i }
        }
        if (sync < 0 || syncScore < 20) return null
        return IntArray(cells) { bits[(sync + it) % cells] }
    }

    /** Find and read a Crundi code in a picture: the pairing link, or null. Never a wrong link. */
    fun decodeImage(img: Image): String? {
        val w = img.width; val h = img.height
        if (w < 32 || h < 32 || img.data.size < w * h * 4) return null
        val d = img.data
        val v = IntArray(w * h) { i -> max(d[i * 4].toInt() and 0xff, max(d[i * 4 + 1].toInt() and 0xff, d[i * 4 + 2].toInt() and 0xff)) }
        val found = findRings(v, w, h).sortedByDescending { it.size }
        for (outer in found) {
            for (inner in found) {
                val ratio = inner.size / outer.size
                if (ratio < 0.30 || ratio > 0.47) continue
                if (hypot(inner.cx - outer.cx, inner.cy - outer.cy) > outer.size * 0.2) continue
                val at = locate(inner, outer) ?: continue
                for (cls in CLASSES) {
                    val rings = ArrayList<IntArray>()
                    for (k in cls.cells.indices) rings += readRing(img, at, RING_RADII[k], cls.cells[k], cls.stroke) ?: break
                    if (rings.size != cls.cells.size) continue
                    decodeRings(rings, cls)?.let { return it }
                }
            }
        }
        return null
    }
}
