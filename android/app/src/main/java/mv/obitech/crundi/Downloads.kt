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

    /**
     * [size]: the expected size if known (a determinate progress bar; else it
     * counts the bytes). The download shows as a notification, as Android's own
     * downloads do: progress while it runs, then "Downloaded" to tap and open.
     */
    suspend fun save(ctx: Context, name: String, mime: String? = null, size: Long = 0, onProgress: ((Long) -> Unit)? = null, fetch: suspend (OutputStream) -> DownloadInfo): Saved = withContext(Dispatchers.IO) {
        val clean = cleanName(name)
        val note = DownloadNote(ctx, clean, size, onProgress)
        try {
            val saved = saveQuietly(ctx, clean, mime) { out -> fetch(note.counting(out)) }
            note.done(saved.uri, mime ?: java.net.URLConnection.guessContentTypeFromName(clean))
            saved
        } catch (t: Throwable) {
            note.failed(); throw t
        }
    }

    private suspend fun saveQuietly(ctx: Context, clean: String, mime: String?, fetch: suspend (OutputStream) -> DownloadInfo): Saved = withContext(Dispatchers.IO) {
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

/** A download's notification: progress while it runs, then done (tap to open) or failed. */
private class DownloadNote(private val ctx: Context, private val name: String, private val size: Long, private val onProgress: ((Long) -> Unit)?) {
    private val id = (System.nanoTime() and 0x7fffffff).toInt()
    private val nm = androidx.core.app.NotificationManagerCompat.from(ctx)
    private var last = 0L

    init {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val sys = ctx.getSystemService(android.app.NotificationManager::class.java)
            sys?.createNotificationChannel(android.app.NotificationChannel(CHANNEL, "Downloads", android.app.NotificationManager.IMPORTANCE_LOW).apply { setShowBadge(false) })
        }
        show(0)
    }

    private fun base() = androidx.core.app.NotificationCompat.Builder(ctx, CHANNEL)
        .setSmallIcon(android.R.drawable.stat_sys_download).setContentTitle(name).setOnlyAlertOnce(true)

    @android.annotation.SuppressLint("MissingPermission")
    private fun post(n: android.app.Notification) { if (nm.areNotificationsEnabled()) runCatching { nm.notify(id, n) } }

    private fun show(done: Long) {
        val b = base().setOngoing(true)
        if (size > 0) b.setProgress(100, ((done * 100) / size).toInt().coerceIn(0, 100), false).setContentText(sizeText(done) + " of " + sizeText(size))
        else b.setProgress(0, 0, true).setContentText(if (done > 0) sizeText(done) else "Starting…")
        post(b.build())
    }

    /** The stream the bytes go through, counted for the progress bar (at most a few updates a second). */
    fun counting(out: OutputStream): OutputStream = object : java.io.FilterOutputStream(out) {
        var n = 0L
        override fun write(b: Int) { out.write(b); tick(1) }
        override fun write(b: ByteArray, off: Int, len: Int) { out.write(b, off, len); tick(len) }
        private fun tick(k: Int) { n += k; val now = System.currentTimeMillis(); if (now - last > 200) { last = now; show(n); onProgress?.invoke(n) } }
    }

    fun done(uri: Uri, mime: String?) {
        val open = android.content.Intent(android.content.Intent.ACTION_VIEW).setDataAndType(uri, mime ?: "*/*")
            .addFlags(android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION or android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
        val pi = android.app.PendingIntent.getActivity(ctx, id, open, android.app.PendingIntent.FLAG_IMMUTABLE or android.app.PendingIntent.FLAG_UPDATE_CURRENT)
        post(base().setSmallIcon(android.R.drawable.stat_sys_download_done).setContentText("Downloaded · tap to open").setContentIntent(pi).setAutoCancel(true).build())
    }

    fun failed() = post(base().setSmallIcon(android.R.drawable.stat_notify_error).setContentText("Download failed").setAutoCancel(true).build())

    private fun sizeText(b: Long): String = when {
        b >= 1L shl 30 -> "%.1f GB".format(b / (1L shl 30).toDouble())
        b >= 1L shl 20 -> "%.1f MB".format(b / (1L shl 20).toDouble())
        b >= 1L shl 10 -> "%.0f KB".format(b / (1L shl 10).toDouble())
        else -> "$b B"
    }

    companion object { const val CHANNEL = "downloads" }
}

/**
 * Downloads started from a chat, by the media's path: how far along, then
 * where the file went, for the file's own card to show.
 */
object MediaDownloads {
    data class State(val done: Long = 0, val total: Long = 0, val savedWhere: String? = null, val failed: Boolean = false)
    val states = androidx.compose.runtime.mutableStateMapOf<String, State>()
}
