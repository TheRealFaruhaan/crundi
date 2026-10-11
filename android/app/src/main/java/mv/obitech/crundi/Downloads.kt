package mv.obitech.crundi

import android.app.DownloadManager
import android.content.Context
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.webkit.CookieManager
import android.webkit.URLUtil
import android.widget.Toast

/**
 * Saving a file to the phone, with the phone's own downloader: it goes to the
 * Downloads folder, with the system's progress and its "open" when done.
 *
 * Used by every window that shows a page of the server's: the main one, and a
 * bubble. A file Claude offers in a chat has to be savable from wherever the
 * chat is being read.
 */
object Downloads {
    /** A page asked, through the bridge, for a file at an address it gave. Only the paired server's own addresses are fetched. */
    fun fromPage(ctx: Context, server: Server?, url: String, name: String, userAgent: String?) {
        if (server == null) return
        // Callable from any frame in the page, so it must not become a way to
        // make the phone fetch an address of a frame's choosing.
        if (!url.startsWith(server.baseUrl + "/")) return
        val clean = name.filter { it.code >= 32 && it !in "\\/:*?\"<>|" }.trim().take(120).ifEmpty { "download" }
        start(ctx, url, userAgent, "attachment; filename=\"$clean\"", null)
    }

    fun start(ctx: Context, url: String, userAgent: String?, contentDisposition: String?, mimeType: String?) {
        if (!url.startsWith("http://") && !url.startsWith("https://")) {
            // A file the page made in memory. The phone's downloader cannot
            // reach that; the page hands real addresses over, so this is only
            // an old page or an unusual link.
            Toast.makeText(ctx, "This kind of file cannot be saved by the app. Update the server, or open Crundi in a browser to save it.", Toast.LENGTH_LONG).show()
            return
        }
        val name = URLUtil.guessFileName(url, contentDisposition, mimeType)
        val req = DownloadManager.Request(Uri.parse(url)).apply {
            setTitle(name)
            setMimeType(mimeType)
            setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED)
            CookieManager.getInstance().getCookie(url)?.let { addRequestHeader("Cookie", it) }
            if (userAgent != null) addRequestHeader("User-Agent", userAgent)
            // Before Android 10 the shared Downloads folder needs a permission
            // this app does not otherwise want; its own folder does not.
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) setDestinationInExternalPublicDir(Environment.DIRECTORY_DOWNLOADS, name)
            else setDestinationInExternalFilesDir(ctx, Environment.DIRECTORY_DOWNLOADS, name)
        }
        (ctx.getSystemService(Context.DOWNLOAD_SERVICE) as DownloadManager).enqueue(req)
        Toast.makeText(ctx, "Downloading $name", Toast.LENGTH_SHORT).show()
    }
}
