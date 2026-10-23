package mv.obitech.crundi.api

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.RectF
import android.util.LruCache
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import mv.obitech.crundi.ui.shell.LocalAppServices

/**
 * Pictures from the server (project icons, media, chat images, video
 * posters), fetched with the phone's login and kept in memory. A drawing
 * (.svg, as project icons can be) is drawn to a bitmap.
 */
class ServerImages internal constructor(private val transport: Transport) {
    private val cache = object : LruCache<String, ImageBitmap>(24 * 1024 * 1024) {
        override fun sizeOf(key: String, value: ImageBitmap) = value.width * value.height * 4
    }
    // A failure is remembered for a short while, not for good: a large picture
    // over a slow or broken connection failed once and then never showed again.
    private val failed = HashMap<String, Long>()
    private val retryAfterMs = 20_000L
    private val locks = HashMap<String, Mutex>()

    /** Already in memory? (No network.) */
    fun cached(path: String, maxPx: Int = 1024): ImageBitmap? = cache.get("$maxPx|$path")

    /**
     * The picture at [path] ("/api/project-icon?...", "/api/media/raw/<id>",
     * "/api/chat-media?..."), at most [maxPx] on its longer side. Null if it
     * is not a picture or cannot be fetched (not retried until [forget]).
     */
    suspend fun load(path: String, maxPx: Int = 1024): ImageBitmap? {
        val key = "$maxPx|$path"
        cache.get(key)?.let { return it }
        if (synchronized(failed) { (failed[key] ?: 0L) > System.currentTimeMillis() - retryAfterMs }) return null
        val lock = synchronized(locks) { locks.getOrPut(key) { Mutex() } }
        return lock.withLock {
            cache.get(key)?.let { return@withLock it }
            val img = runCatching {
                val r = transport.exchange(ApiRequest("GET", path.substringBefore('?'), query(path), headers = mapOf("Accept" to "image/*,*/*")))
                val bytes = r.bytes ?: r.text.takeIf { it.isNotEmpty() }?.toByteArray()
                if (r.status !in 200..299 || bytes == null) null
                else withContext(Dispatchers.Default) { decode(bytes, r.headers.entries.firstOrNull { it.key.equals("content-type", true) }?.value.orEmpty(), maxPx) }
            }.getOrNull()
            if (img == null) synchronized(failed) { failed[key] = System.currentTimeMillis() } else { synchronized(failed) { failed.remove(key) }; cache.put(key, img) }
            img
        }
    }

    fun forget() { cache.evictAll(); synchronized(failed) { failed.clear() } }

    private fun query(path: String): Map<String, String?> = path.substringAfter('?', "").split('&').filter { it.isNotEmpty() }.associate {
        java.net.URLDecoder.decode(it.substringBefore('='), "UTF-8") to java.net.URLDecoder.decode(it.substringAfter('=', ""), "UTF-8")
    }

    companion object {
        internal fun decode(bytes: ByteArray, type: String, maxPx: Int): ImageBitmap? {
            if (type.contains("svg") || (bytes.size > 4 && String(bytes, 0, minOf(bytes.size, 256)).contains("<svg"))) return svg(bytes, maxPx.coerceAtMost(512))
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
            if (bounds.outWidth <= 0) return null
            var sample = 1
            while (maxOf(bounds.outWidth, bounds.outHeight) / (sample * 2) >= maxPx) sample *= 2
            val bmp = BitmapFactory.decodeByteArray(bytes, 0, bytes.size, BitmapFactory.Options().apply { inSampleSize = sample }) ?: return null
            return bmp.asImageBitmap()
        }

        private fun svg(bytes: ByteArray, side: Int): ImageBitmap? = runCatching {
            val svg = com.caverock.androidsvg.SVG.getFromInputStream(bytes.inputStream())
            if (svg.documentViewBox == null && svg.documentWidth > 0 && svg.documentHeight > 0) svg.setDocumentViewBox(0f, 0f, svg.documentWidth, svg.documentHeight)
            svg.setDocumentWidth("100%"); svg.setDocumentHeight("100%")
            val bmp = Bitmap.createBitmap(side, side, Bitmap.Config.ARGB_8888)
            svg.renderToCanvas(Canvas(bmp), RectF(0f, 0f, side.toFloat(), side.toFloat()))
            bmp.asImageBitmap()
        }.getOrNull()
    }
}

/**
 * A server picture for a composable: null while loading or if it failed
 * (draw a placeholder then). Uses the current server's client.
 */
@Composable
fun rememberServerImage(path: String?, maxPx: Int = 1024): ImageBitmap? {
    val images = LocalAppServices.current.api.images
    var img by remember(path, maxPx) { mutableStateOf(path?.let { images.cached(it, maxPx) }) }
    LaunchedEffect(path, maxPx) { if (path != null && img == null) img = images.load(path, maxPx) }
    return img
}
