package mv.obitech.crundi

/**
 * Version order, the same as the server's (compareVersions in src/whatsnew.js):
 *
 *   1.19.24-dev.1 < 1.19.24-dev.2 < 1.19.24-dev.10 < 1.19.24 < 1.19.25-dev.1
 *
 * Both sides have to agree on this or one of them refuses the other for being
 * "older" when it is not. VersionsTest pins the cases.
 */
object Versions {
    private val RE = Regex("""^v?(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$""")

    private class Parsed(val core: List<Int>, val pre: String)

    private fun parse(v: String?): Parsed? {
        val m = RE.find((v ?: "").trim()) ?: return null
        return Parsed(listOf(m.groupValues[1].toInt(), m.groupValues[2].toInt(), m.groupValues[3].toInt()), m.groupValues[4])
    }

    fun isVersion(v: String?) = parse(v) != null

    /** Negative if a is older than b, positive if newer, 0 if the same. Not-a-version sorts lowest. */
    fun compare(a: String?, b: String?): Int {
        val x = parse(a)
        val y = parse(b)
        if (x == null || y == null) return (if (x != null) 1 else 0) - (if (y != null) 1 else 0)
        for (i in 0..2) if (x.core[i] != y.core[i]) return if (x.core[i] < y.core[i]) -1 else 1
        // A release is above its own prereleases.
        if (x.pre.isEmpty() || y.pre.isEmpty()) return (if (x.pre.isEmpty()) 1 else 0) - (if (y.pre.isEmpty()) 1 else 0)
        val xs = x.pre.split('.')
        val ys = y.pre.split('.')
        for (i in 0 until maxOf(xs.size, ys.size)) {
            if (i >= xs.size) return -1
            if (i >= ys.size) return 1
            val xn = xs[i].toIntOrNull()
            val yn = ys[i].toIntOrNull()
            if (xn != null && yn != null) { if (xn != yn) return if (xn < yn) -1 else 1 }
            else if (xs[i] != ys[i]) return if (xs[i] < ys[i]) -1 else 1
        }
        return 0
    }

    /** True when `have` is below `minimum`. No minimum means nothing is too old. */
    fun below(have: String?, minimum: String?): Boolean {
        if (minimum.isNullOrEmpty()) return false
        return !isVersion(have) || compare(have, minimum) < 0
    }
}
