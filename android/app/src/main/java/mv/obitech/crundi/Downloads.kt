package mv.obitech.crundi

import android.content.ContentValues
import android.content.Context
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.MediaStore
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import mv.obitech.crundi.api.DownloadInfo
import java.io.File
import java.io.OutputStream

/**
 * Saving a file from the server to the phone's Downloads folder. The bytes
 * come through the API client (signed in with the phone's login, streamed,
 * any size); this only decides where they land:
 * - Android 10 and later: the shared Downloads folder, through MediaStore,
 *   with no permission needed. The entry stays hidden until it is complete.
 * - Before that: the app's own Downloads folder (the shared one would need a
 *   storage permission this app does not otherwise want).
 *
 *   Downloads.save(ctx, "report.pdf") { sink -> api.files.download(project, path, sink, progress) }
 */
object Downloads {
    /** Where a saved file went: [uri] to open or share it, [where] to tell the person. */
    data class Saved(val uri: Uri, val name: String, val where: String, val size: Long)

    suspend fun save(ctx: Context, name: String, mime: String? = null, fetch: suspend (OutputStream) -> DownloadInfo): Saved = withContext(Dispatchers.IO) {
        val clean = cleanName(name)
        val type = mime?.takeIf { it.isNotEmpty() && it != "application/octet-stream" }
            ?: java.net.URLConnection.guessContentTypeFromName(clean)
            ?: "application/octet-stream"
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            val resolver = ctx.contentResolver
            val values = ContentValues().apply {
                put(MediaStore.Downloads.DISPLAY_NAME, clean)
                put(MediaStore.Downloads.MIME_TYPE, type)
                put(MediaStore.Downloads.IS_PENDING, 1)
            }
            val uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values) ?: throw java.io.IOException("The phone would not make a file in Downloads")
            try {
                val info = resolver.openOutputStream(uri)!!.use { fetch(it) }
                resolver.update(uri, ContentValues().apply { put(MediaStore.Downloads.IS_PENDING, 0) }, null, null)
                Saved(uri, clean, "Downloads", info.size)
            } catch (t: Throwable) {
                runCatching { resolver.delete(uri, null, null) }
                throw t
            }
        } else {
            val dir = ctx.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS) ?: File(ctx.filesDir, "downloads")
            dir.mkdirs()
            val file = freeFile(dir, clean)
            try {
                val info = file.outputStream().use { fetch(it) }
                Saved(Uri.fromFile(file), file.name, "the app's Downloads folder", info.size)
            } catch (t: Throwable) {
                file.delete(); throw t
            }
        }
    }

    /** A name safe for any file system: no path separators or control characters. */
    fun cleanName(name: String): String = name.filter { it.code >= 32 && it !in "\\/:*?\"<>|" }.trim().take(120).ifEmpty { "download" }

    private fun freeFile(dir: File, name: String): File {
        var f = File(dir, name)
        var n = 2
        val base = name.substringBeforeLast('.'); val ext = name.substringAfterLast('.', "").let { if (it.isEmpty()) "" else ".$it" }
        while (f.exists()) { f = File(dir, "$base ($n)$ext"); n++ }
        return f
    }
}
