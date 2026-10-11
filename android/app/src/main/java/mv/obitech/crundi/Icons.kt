package mv.obitech.crundi

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.BitmapShader
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.RectF
import android.graphics.Shader
import java.io.File
import java.security.MessageDigest

/**
 * A project's own icon, kept on the phone.
 *
 * The server finds an icon in a project's folder and names it by an address
 * that changes whenever the file does. So an address is fetched once, kept
 * as a file, and never asked for again; a changed icon arrives under a new
 * address and is fetched then. One file is kept per project, the newest.
 *
 * Fetching is done where a message from the server is handled, which is off
 * the main thread and is where an address is first heard of. Everything that
 * draws only reads what is already kept: no drawing waits on the network,
 * and a project whose icon could not be had is drawn with its letters.
 */
object Icons {
    private const val DIR = "project-icons"
    private const val MAX_BYTES = 2 * 1024 * 1024
    // Tried and failed (the file is gone, the server is older, a drawing the
    // phone cannot read): not tried again on every message.
    private val failed = HashMap<String, Long>()
    private const val RETRY_MS = 10 * 60 * 1000L
    private val memory = HashMap<String, Bitmap>()

    private fun sha(s: String) = MessageDigest.getInstance("SHA-1").digest(s.toByteArray()).joinToString("") { "%02x".format(it) }.take(24)
    // "/api/project-icon?project=x&v=abc" -> the project's part, and the version.
    private fun projectPart(path: String) = path.substringBefore("&v=")
    private fun stem(serverId: String, path: String) = sha("$serverId|${projectPart(path)}")
    private fun fileFor(ctx: Context, serverId: String, path: String) = File(File(ctx.filesDir, DIR), "${stem(serverId, path)}-${sha(path)}")
    private fun valid(path: String) = path.startsWith("/api/project-icon?project=") && path.contains("&v=") && path.length <= 300

    /** The icon, if it has already been fetched. Reads the disk at most; safe anywhere. */
    @Synchronized
    fun cached(ctx: Context, serverId: String, path: String): Bitmap? {
        if (path.isEmpty() || !valid(path)) return null
        val key = "$serverId|$path"
        memory[key]?.let { return it }
        val f = fileFor(ctx, serverId, path)
        if (!f.isFile) return null
        val bmp = runCatching { BitmapFactory.decodeFile(f.path) }.getOrNull()
        if (bmp == null) { f.delete(); return null }
        if (memory.size > 24) memory.clear()
        memory[key] = bmp
        return bmp
    }

    /** Make sure the icon is kept, fetching it if it is not. Blocking: call it off the main thread. */
    fun ensure(ctx: Context, serverId: String, path: String) {
        if (path.isEmpty() || !valid(path)) return
        val key = "$serverId|$path"
        synchronized(this) {
            if (fileFor(ctx, serverId, path).isFile) return
            val at = failed[key]
            if (at != null && System.currentTimeMillis() - at < RETRY_MS) return
        }
        val bytes = runCatching { Api.authedBytes(ctx, serverId, path, MAX_BYTES) }.getOrNull()
        // Only something the phone can actually draw is kept.
        val ok = bytes != null && runCatching { BitmapFactory.decodeByteArray(bytes, 0, bytes.size) }.getOrNull() != null
        synchronized(this) {
            if (!ok) { failed[key] = System.currentTimeMillis(); return }
            val dir = File(ctx.filesDir, DIR).apply { mkdirs() }
            val target = fileFor(ctx, serverId, path)
            val tmp = File(dir, target.name + ".tmp")
            runCatching {
                tmp.writeBytes(bytes!!)
                tmp.renameTo(target)
                // The older versions of this project's icon have been replaced.
                val mine = stem(serverId, path) + "-"
                dir.listFiles()?.forEach { if (it.name.startsWith(mine) && it.name != target.name) it.delete() }
                memory.keys.removeAll { it.startsWith("$serverId|${projectPart(path)}&v=") && it != key }
            }.onFailure { tmp.delete() }
            failed.remove(key)
        }
    }

    /** Forget a server's icons: it was removed from this phone. Its files are small and are simply left to be replaced. */
    @Synchronized
    fun forget(serverId: String) { memory.keys.removeAll { it.startsWith("$serverId|") }; failed.keys.removeAll { it.startsWith("$serverId|") } }

    /**
     * The icon as a head: square for a launcher-cut shape (`adaptive`), which
     * wants the picture inside the middle two thirds, or already round.
     *
     * An icon that fills its square (a photo, an app icon with its own
     * background) is drawn edge to edge. One with clear corners is a shape
     * on nothing, and is set on a plain ground chosen to show it: light
     * behind a dark mark, dark behind a light one.
     */
    fun head(icon: Bitmap, size: Int, adaptive: Boolean): Bitmap {
        val out = Bitmap.createBitmap(size, size, Bitmap.Config.ARGB_8888)
        val c = Canvas(out)
        val small = Bitmap.createScaledBitmap(icon, 24, 24, true)
        var clear = 0; var lum = 0.0; var seen = 0
        for (y in 0 until 24) for (x in 0 until 24) {
            val p = small.getPixel(x, y)
            val a = Color.alpha(p)
            val corner = (x < 3 || x > 20) && (y < 3 || y > 20)
            if (corner && a < 40) clear++
            if (a > 128) { lum += (0.2126 * Color.red(p) + 0.7152 * Color.green(p) + 0.0722 * Color.blue(p)) / 255.0; seen++ }
        }
        val solid = clear < 6
        val ground = if (seen > 0 && lum / seen > 0.6) 0xFF15151C.toInt() else 0xFFF4F4F7.toInt()
        val full = RectF(0f, 0f, size.toFloat(), size.toFloat())
        val paint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)
        if (adaptive) {
            if (solid) {
                // Edge to edge, but the launcher cuts the outer sixth away:
                // keep the whole picture inside what survives the cut.
                c.drawColor(small.getPixel(1, 1) or 0xFF000000.toInt())
                val inset = size * 0.14f
                c.drawBitmap(icon, null, RectF(inset, inset, size - inset, size - inset), paint)
            } else {
                c.drawColor(ground)
                val inset = size * 0.24f
                c.drawBitmap(icon, null, RectF(inset, inset, size - inset, size - inset), paint)
            }
            return out
        }
        if (solid) {
            val scaled = Bitmap.createScaledBitmap(icon, size, size, true)
            paint.shader = BitmapShader(scaled, Shader.TileMode.CLAMP, Shader.TileMode.CLAMP)
            c.drawCircle(size / 2f, size / 2f, size / 2f, paint)
        } else {
            c.drawCircle(size / 2f, size / 2f, size / 2f, Paint(Paint.ANTI_ALIAS_FLAG).apply { color = ground })
            val inset = size * 0.2f
            c.drawBitmap(icon, null, RectF(inset, inset, full.right - inset, full.bottom - inset), paint)
        }
        return out
    }
}
