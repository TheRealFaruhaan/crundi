package mv.obitech.crundi.voice

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Matrix
import android.net.Uri
import android.provider.OpenableColumns
import android.webkit.MimeTypeMap
import androidx.core.content.FileProvider
import java.io.ByteArrayOutputStream
import java.io.File

/** The phone's side of files for Cru: what a picked file is, a picture's preview, the app's own copies (camera, shared text). */
object CruFiles {
    data class Info(val name: String, val size: Long, val mime: String)

    /** A picture's preview for Cru (base64 JPEG, longest side 1568) and a small one for the tray. */
    class Preview(val jpegBase64: String, val thumb: Bitmap)

    private const val PREVIEW_SIDE = 1568
    private const val PREVIEW_MAX = 4 * 1024 * 1024
    private const val THUMB_SIDE = 96

    fun authority(ctx: Context) = ctx.packageName + ".crufiles"

    /** The folder the app's own copies go in (res/xml/cru_files.xml). */
    private fun dir(ctx: Context) = File(ctx.cacheDir, "cru").apply { mkdirs() }

    /** A new file for the camera to save a photo to, as a content URI it may write. */
    fun cameraFile(ctx: Context): Uri {
        val stamp = java.text.SimpleDateFormat("yyyy-MM-dd HH.mm.ss", java.util.Locale.US).format(java.util.Date())
        return FileProvider.getUriForFile(ctx, authority(ctx), File(dir(ctx), "Photo $stamp.jpg"))
    }

    /** Long shared text, as a file Cru can read. */
    fun textFile(ctx: Context, text: String): Uri {
        // A folder each: two shares at once don't write over each other.
        val f = File(File(dir(ctx), "t" + System.nanoTime()).apply { mkdirs() }, "Shared text.txt")
        f.writeText(text, Charsets.UTF_8)
        return FileProvider.getUriForFile(ctx, authority(ctx), f)
    }

    /** The app's own copy gone (only ever one of ours). */
    fun delete(ctx: Context, uri: Uri) {
        if (uri.authority != authority(ctx)) return
        runCatching { ctx.contentResolver.delete(uri, null, null) }
    }

    /** Its name, size (-1: not known) and type. */
    fun info(ctx: Context, uri: Uri): Info {
        var name = uri.lastPathSegment?.substringAfterLast('/') ?: "file"
        var size = -1L
        runCatching {
            ctx.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)?.use { c ->
                if (c.moveToFirst()) {
                    if (!c.isNull(0)) c.getString(0)?.takeIf { it.isNotBlank() }?.let { name = it }
                    if (!c.isNull(1)) size = c.getLong(1)
                }
            }
        }
        if (size < 0) size = runCatching { ctx.contentResolver.openAssetFileDescriptor(uri, "r")?.use { it.length } }.getOrNull() ?: -1L
        val ext = name.substringAfterLast('.', "").lowercase()
        val mime = ctx.contentResolver.getType(uri)?.takeIf { it.isNotBlank() && it != "application/octet-stream" }
            ?: MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext)
            ?: "application/octet-stream"
        // A file with no extension gets one from its type, so the server knows what it is.
        if (ext.isEmpty()) MimeTypeMap.getSingleton().getExtensionFromMimeType(mime)?.let { name = "$name.$it" }
        return Info(name, size, mime)
    }

    /**
     * A smaller JPEG of a picture, upright, on white (a transparent PNG is
     * not black), for Cru to look at; null when it can't be read or comes
     * out over 4 MB. Decoded at a reduced size, never whole.
     */
    fun preview(ctx: Context, uri: Uri): Preview? {
        val cr = ctx.contentResolver
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        cr.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, bounds) } ?: return null
        val w = bounds.outWidth
        val h = bounds.outHeight
        if (w <= 0 || h <= 0) return null
        var sample = 1
        while (maxOf(w, h) / (sample * 2) >= PREVIEW_SIDE) sample *= 2
        val raw = cr.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, BitmapFactory.Options().apply { inSampleSize = sample }) } ?: return null
        val turn = orientation(ctx, uri)
        val k = minOf(1f, PREVIEW_SIDE.toFloat() / maxOf(raw.width, raw.height))
        val m = Matrix().apply { postScale(k, k); if (turn.first != 0f) postRotate(turn.first); if (turn.second) postScale(-1f, 1f) }
        val turned = Bitmap.createBitmap(raw, 0, 0, raw.width, raw.height, m, true)
        if (turned !== raw) raw.recycle()
        val out = Bitmap.createBitmap(turned.width, turned.height, Bitmap.Config.ARGB_8888)
        Canvas(out).apply { drawColor(Color.WHITE); drawBitmap(turned, 0f, 0f, null) }
        turned.recycle()
        val bytes = ByteArrayOutputStream().also { out.compress(Bitmap.CompressFormat.JPEG, 85, it) }.toByteArray()
        val tk = THUMB_SIDE.toFloat() / maxOf(out.width, out.height)
        val thumb = if (tk < 1f) Bitmap.createScaledBitmap(out, maxOf(1, (out.width * tk).toInt()), maxOf(1, (out.height * tk).toInt()), true) else out
        if (thumb !== out) out.recycle()
        if (bytes.size > PREVIEW_MAX) return null
        return Preview(android.util.Base64.encodeToString(bytes, android.util.Base64.NO_WRAP), thumb)
    }

    /** The photo's EXIF turn: degrees, and whether it is mirrored. */
    private fun orientation(ctx: Context, uri: Uri): Pair<Float, Boolean> = runCatching {
        ctx.contentResolver.openInputStream(uri)?.use { ins ->
            when (android.media.ExifInterface(ins).getAttributeInt(android.media.ExifInterface.TAG_ORIENTATION, android.media.ExifInterface.ORIENTATION_NORMAL)) {
                android.media.ExifInterface.ORIENTATION_ROTATE_90 -> 90f to false
                android.media.ExifInterface.ORIENTATION_ROTATE_180 -> 180f to false
                android.media.ExifInterface.ORIENTATION_ROTATE_270 -> 270f to false
                android.media.ExifInterface.ORIENTATION_FLIP_HORIZONTAL -> 0f to true
                android.media.ExifInterface.ORIENTATION_FLIP_VERTICAL -> 180f to true
                android.media.ExifInterface.ORIENTATION_TRANSPOSE -> 90f to true
                android.media.ExifInterface.ORIENTATION_TRANSVERSE -> 270f to true
                else -> 0f to false
            }
        }
    }.getOrNull() ?: (0f to false)
}
