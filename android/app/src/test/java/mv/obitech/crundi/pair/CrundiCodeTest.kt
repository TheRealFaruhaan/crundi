package mv.obitech.crundi.pair

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.floor
import kotlin.math.hypot
import kotlin.math.min
import kotlin.math.sin

/**
 * The app's reader for the pairing picture, held to the server's encoder
 * (fixtures written by android/tools/gen-code-fixtures.mjs from
 * src/crundi-code.js) and to pictures painted as a camera would see them,
 * the same way scripts/test-crundi-code.mjs paints them. It may fail to read
 * a code; it may never read one wrong.
 */
class CrundiCodeTest {
    private var seed = 20260L
    private fun rnd(): Double { seed = (seed * 1103515245 + 12345) and 0x7fffffff; return seed / 0x7fffffff.toDouble() }
    private fun rint(n: Int) = floor(rnd() * n).toInt()
    private fun deg(d: Double) = d * PI / 180

    private val fixtures = JSONObject(javaClass.classLoader!!.getResource("crundi-code.json")!!.readText())
    private val CODE = "0123456789abcdef01234567"
    private val S = "https://dev.faruhaan.com/pair#$CODE"
    private val L = "https://crundi.example-team.dev:8443/pair#$CODE"
    private val XL = "https://colgate-instances-moderate-spoken.trycloudflare.com/pair#$CODE"
    private val XXL = "https://a-very-long-subdomain-name.another-long-part.example-company.co.uk/pair#$CODE"

    @Test
    fun sameRingsAsTheServer() {
        val codes = fixtures.getJSONArray("codes")
        for (i in 0 until codes.length()) {
            val c = codes.getJSONObject(i)
            val link = c.getString("link")
            val rings = c.getJSONArray("rings").let { a -> List(a.length()) { k -> a.getString(k).map { it - '0' }.toIntArray() } }
            val cls = CrundiCode.CLASSES.first { it.name == c.getString("size") }
            assertEquals("server's rings read back: $link", link, CrundiCode.decodeRings(rings, cls))
            val mine = CrundiCode.encode(link)!!
            assertEquals(cls.name, mine.cls.name)
            for (k in rings.indices) assertTrue("ring $k of $link", rings[k].contentEquals(mine.rings[k]))
        }
        val bad = fixtures.getJSONArray("unencodable")
        for (i in 0 until bad.length()) assertNull(CrundiCode.encode(bad.getString(i)))
    }

    @Test
    fun errorCorrectionFixesOrRefusesNeverGuesses() {
        var fixed = 0; var wrong = 0; var runs = 0
        for (t in 0 until 600) {
            val cls = CrundiCode.CLASSES[t % 3]
            val data = IntArray(cls.payload) { rint(256) }
            val word = CrundiCode.rsEncode(data, cls.parity)
            fun damage(count: Int): IntArray {
                val bad = word.copyOf(); val at = HashSet<Int>()
                while (at.size < count) at.add(rint(bad.size))
                for (p in at) bad[p] = bad[p] xor (1 + rint(255))
                return bad
            }
            val a = CrundiCode.rsDecode(damage(rint(cls.parity / 2 + 1)), cls.parity)
            if (a != null && a.contentEquals(data)) fixed++
            val b = CrundiCode.rsDecode(damage(cls.parity / 2 + 1 + rint(8)), cls.parity)
            runs++
            if (b != null && !b.contentEquals(data)) wrong++
        }
        assertEquals(runs, fixed)
        assertEquals(0, wrong)
    }

    // ─── A camera's view, painted as scripts/test-crundi-code.mjs paints it ───
    private fun paint(
        link: String, w: Int = 360, h: Int = 360, size: Double = 150.0, turn: Double = 0.0, tiltX: Double = 0.0, tiltY: Double = 0.0,
        depth: Double = 4.0, page: IntArray = intArrayOf(18, 18, 26), clutter: Boolean = false, hole: DoubleArray? = null,
    ): CrundiCode.Image {
        val enc = CrundiCode.encode(link)!!
        val rings = enc.rings; val cls = enc.cls
        val rots = DoubleArray(rings.size) { rnd() * PI * 2 }
        val indigo = intArrayOf(129, 140, 248); val amber = intArrayOf(251, 191, 36); val dark = intArrayOf(10, 10, 15)
        fun shade(x: Double, y: Double): IntArray {
            if (abs(x) > 1.14 || abs(y) > 1.14) return if (clutter && (y < -1.5 || (x > 1.3 && y > 0.9))) intArrayOf(245, 245, 250) else page
            val r = hypot(x, y)
            if (hole != null && hypot(x - hole[0], y - hole[1]) < hole[2]) return dark
            if (abs(r - CrundiCode.R_INNER) < CrundiCode.FINDER_WIDTH / 2 || abs(r - CrundiCode.R_OUTER) < CrundiCode.FINDER_WIDTH / 2) return indigo
            if (r < 0.26) return if (hypot(x, y + 0.2) < 0.045) amber else if (abs(x) < 0.19 && abs(y - 0.03) < 0.15) intArrayOf(99, 102, 241) else dark
            for (k in rings.indices) {
                if (abs(r - CrundiCode.RING_RADII[k]) >= cls.stroke / 2) continue
                val n = rings[k].size; val step = PI * 2 / n
                var a = (atan2(y, x) - rots[k]) % (PI * 2); if (a < 0) a += PI * 2
                val i = min(n - 1, floor(a / step).toInt())
                if (rings[k][i] == 0) return dark
                val inset = 0.009 / CrundiCode.RING_RADII[k]; val into = a - i * step
                val joinPrev = i > 1 && rings[k][i - 1] != 0; val joinNext = i > 0 && i < n - 1 && rings[k][i + 1] != 0
                if ((into < inset && !joinPrev) || (step - into < inset && !joinNext)) return dark
                return if (i == 0) amber else indigo
            }
            return dark
        }
        val ct = cos(turn); val st = sin(turn); val cx = cos(tiltX); val sx = sin(tiltX); val cy = cos(tiltY); val sy = sin(tiltY)
        val m = arrayOf(
            doubleArrayOf(cy * ct + sy * sx * st, -cy * st + sy * sx * ct),
            doubleArrayOf(cx * st, cx * ct),
            doubleArrayOf(-sy * ct + cy * sx * st, sy * st + cy * sx * ct),
        )
        val data = ByteArray(w * h * 4)
        val ss = 3
        for (py in 0 until h) for (px in 0 until w) {
            var rr = 0; var gg = 0; var bb = 0
            for (sy2 in 0 until ss) for (sx2 in 0 until ss) {
                val u = (px + (sx2 + 0.5) / ss - w / 2.0) / size; val v = (py + (sy2 + 0.5) / ss - h / 2.0) / size
                val a11 = m[0][0] - (u * m[2][0]) / depth; val a12 = m[0][1] - (u * m[2][1]) / depth
                val a21 = m[1][0] - (v * m[2][0]) / depth; val a22 = m[1][1] - (v * m[2][1]) / depth
                val det = a11 * a22 - a12 * a21
                val c = shade((u * a22 - v * a12) / det, (v * a11 - u * a21) / det)
                rr += c[0]; gg += c[1]; bb += c[2]
            }
            val p = (py * w + px) * 4
            data[p] = (rr / (ss * ss)).toByte(); data[p + 1] = (gg / (ss * ss)).toByte(); data[p + 2] = (bb / (ss * ss)).toByte(); data[p + 3] = -1
        }
        return CrundiCode.Image(data, w, h)
    }

    private fun blur(img: CrundiCode.Image, radius: Int): CrundiCode.Image {
        val w = img.width; val h = img.height
        for (pass in 0 until 2) {
            val src = img.data.copyOf()
            for (y in 0 until h) for (x in 0 until w) for (ch in 0 until 3) {
                var s = 0; var n = 0
                for (d in -radius..radius) {
                    val xx = if (pass == 1) x else x + d; val yy = if (pass == 1) y + d else y
                    if (xx < 0 || yy < 0 || xx >= w || yy >= h) continue
                    s += src[(yy * w + xx) * 4 + ch].toInt() and 0xff; n++
                }
                img.data[(y * w + x) * 4 + ch] = (s / n).toByte()
            }
        }
        return img
    }

    private fun grade(img: CrundiCode.Image, gain: Double = 1.0, lift: Double = 0.0, noise: Double = 0.0, tint: DoubleArray = doubleArrayOf(1.0, 1.0, 1.0)): CrundiCode.Image {
        var i = 0
        while (i < img.data.size) {
            for (ch in 0 until 3) {
                val v = (img.data[i + ch].toInt() and 0xff) * gain * tint[ch] + lift + (if (noise != 0.0) (rnd() + rnd() - 1) * noise else 0.0)
                img.data[i + ch] = v.toInt().coerceIn(0, 255).toByte()
            }
            i += 4
        }
        return img
    }

    @Test
    fun readsCodesTheWayACameraSeesThem() {
        val views = listOf<Triple<String, String, () -> CrundiCode.Image>>(
            Triple("S straight on", S) { paint(S) },
            Triple("L straight on", L) { paint(L) },
            Triple("XL straight on", XL) { paint(XL) },
            Triple("S small, bright screen round it", S) { paint(S, w = 640, h = 480, size = 80.0, clutter = true) },
            Triple("S turned", S) { paint(S, turn = deg(123.0)) },
            Triple("S leaning back 30", S) { paint(S, tiltX = deg(30.0), size = 140.0) },
            Triple("S leaning sideways 40, close", S) { paint(S, tiltY = deg(40.0), turn = deg(-50.0), depth = 3.0, size = 130.0) },
            Triple("L leaning both ways", L) { paint(L, tiltX = deg(-22.0), tiltY = deg(26.0), turn = deg(15.0), size = 140.0) },
            Triple("S out of focus", S) { blur(paint(S), 2) },
            Triple("S grainy", S) { grade(paint(S), noise = 34.0) },
            Triple("S washed out", S) { grade(paint(S), gain = 0.55, lift = 55.0) },
            Triple("S warm", S) { grade(paint(S), tint = doubleArrayOf(1.0, 0.86, 0.62)) },
            Triple("S everything at once", S) { grade(blur(paint(S, tiltX = deg(20.0), tiltY = deg(-15.0), turn = deg(200.0), size = 145.0), 1), gain = 0.7, lift = 30.0, noise = 16.0) },
            Triple("XXL straight on", XXL) { paint(XXL, w = 420, h = 420, size = 180.0) },
            Triple("S smudged", S) { paint(S, hole = doubleArrayOf(0.52, 0.42, 0.1)) },
        )
        for ((name, want, make) in views) assertEquals("read: $name", want, CrundiCode.decodeImage(make()))
    }

    @Test
    fun neverReadsWrong() {
        assertNull("static", CrundiCode.decodeImage(CrundiCode.Image(ByteArray(240 * 240 * 4) { rint(256).toByte() }, 240, 240)))
        assertNull("black", CrundiCode.decodeImage(CrundiCode.Image(ByteArray(240 * 240 * 4), 240, 240)))
        assertNull("tiny", CrundiCode.decodeImage(CrundiCode.Image(ByteArray(16), 2, 2)))
        val links = listOf(S, L, XL)
        for (i in 0 until 12) {
            val want = links[i % 3]
            val img = grade(
                blur(paint(want, w = 260, h = 260, size = 60.0 + rint(60), tiltX = deg(rint(70) - 35.0), tiltY = deg(rint(70) - 35.0), turn = rnd() * 6.28, depth = 2.5 + rnd() * 2), rint(3)),
                gain = 0.4 + rnd() * 0.6, lift = rint(70).toDouble(), noise = rint(60).toDouble(),
            )
            val got = CrundiCode.decodeImage(img)
            assertTrue("degraded frame $i read as $got", got == null || got == want)
        }
    }
}
