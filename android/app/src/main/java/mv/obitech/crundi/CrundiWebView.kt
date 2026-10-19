package mv.obitech.crundi

import android.annotation.SuppressLint
import android.content.Context
import android.util.Base64
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.webkit.WebView
import android.widget.Toast
import androidx.core.view.ContentInfoCompat
import androidx.core.view.ViewCompat
import androidx.core.view.inputmethod.EditorInfoCompat
import androidx.core.view.inputmethod.InputConnectionCompat

/**
 * The web view the server's pages are shown in, taught one thing a plain one
 * does not do: take a picture from the keyboard.
 *
 * A keyboard offers "Paste" for a copied picture or a screenshot only to a
 * text field that says it accepts pictures. A web view says no such thing, so
 * the offer never appeared. This one says it accepts images whenever a field
 * in the page has the cursor, and when the keyboard hands one over, gives it
 * to the page as a paste: the same event the page already handles when a
 * picture is pasted in a browser, so the page needs no change and a server
 * of any version works.
 */
@SuppressLint("ViewConstructor")
class CrundiWebView(context: Context) : WebView(context) {
    // "Any picture", and then each kind by name. The first is the proper way
    // to say it and is enough for a keyboard that matches kinds as Android
    // does. Some compare the words exactly (Samsung's clipboard panel listed
    // copied text here and no copied pictures), and to those only a kind
    // named outright counts.
    private val accepts = arrayOf("image/*", "image/png", "image/jpeg", "image/jpg", "image/gif", "image/webp", "image/bmp", "image/heic", "image/heif")

    init {
        // The page runs in a renderer process of its own, which Android lowers
        // and may end once the view is out of sight: a bubble folded away
        // then reloaded from nothing on its next tap. Kept important even
        // when hidden, so it is still there, still connected (see Link).
        setRendererPriorityPolicy(RENDERER_PRIORITY_IMPORTANT, false)
        ViewCompat.setOnReceiveContentListener(this, accepts) { _, payload ->
            // Take the pictures, leave anything else (text) to be handled as usual.
            val split = payload.partition { item -> item.uri != null }
            val pictures = split.first
            if (pictures != null) {
                val clip = pictures.clip
                for (i in 0 until clip.itemCount) clip.getItemAt(i).uri?.let { handOver(it) }
            }
            split.second
        }
    }

    override fun onCreateInputConnection(outAttrs: EditorInfo): InputConnection? {
        val ic = super.onCreateInputConnection(outAttrs) ?: return null
        EditorInfoCompat.setContentMimeTypes(outAttrs, accepts)
        return InputConnectionCompat.createWrapper(this, ic, outAttrs)
    }

    private fun handOver(uri: android.net.Uri) {
        val ctx = context
        Bg.run({
            val mime = ctx.contentResolver.getType(uri) ?: "image/png"
            val bytes = ctx.contentResolver.openInputStream(uri)?.use { it.readBytes() } ?: ByteArray(0)
            // A screenshot copied as a PNG can be several megabytes, which is
            // a slow upload on a phone for no gain. Past a size it is sent as
            // a JPEG, no larger than a phone screen needs. Smaller pictures
            // go exactly as they are.
            if (bytes.size > SHRINK_OVER && mime != "image/gif") shrink(bytes)?.let { return@run Pair("image/jpeg", it) }
            Pair(mime, bytes)
        }) { result ->
            val (mime, bytes) = result.getOrNull() ?: Pair("", ByteArray(0))
            if (bytes.isEmpty() || !mime.startsWith("image/")) { Toast.makeText(ctx, "That picture could not be read.", Toast.LENGTH_SHORT).show(); return@run }
            // A string through to the page: fine for a screenshot, not for a
            // film. Past this the attach button is the way.
            if (bytes.size > MAX_BYTES) { Toast.makeText(ctx, "That picture is too large to paste. Use the attach button.", Toast.LENGTH_LONG).show(); return@run }
            val ext = when (mime) { "image/jpeg" -> "jpg"; "image/webp" -> "webp"; "image/gif" -> "gif"; else -> "png" }
            val b64 = Base64.encodeToString(bytes, Base64.NO_WRAP)
            // Build a real File in the page and deliver it as a paste to
            // whatever has the cursor. Nothing in the string is the page's to
            // misread: base64 and two fixed words.
            evaluateJavascript(
                "(function(b,m,n){try{var s=atob(b),u=new Uint8Array(s.length);for(var i=0;i<s.length;i++)u[i]=s.charCodeAt(i);" +
                    "var f=new File([u],n,{type:m}),d=new DataTransfer();d.items.add(f);" +
                    "var t=document.activeElement||document.body;" +
                    "t.dispatchEvent(new ClipboardEvent('paste',{clipboardData:d,bubbles:true,cancelable:true}));}catch(e){}})" +
                    "('$b64','$mime','pasted-${System.currentTimeMillis()}.$ext')",
                null,
            )
        }
    }

    private fun shrink(bytes: ByteArray): ByteArray? = try {
        val bounds = android.graphics.BitmapFactory.Options().apply { inJustDecodeBounds = true }
        android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        var sample = 1
        while (maxOf(bounds.outWidth, bounds.outHeight) / sample > 2 * LONG_SIDE) sample *= 2
        val bmp = android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.size, android.graphics.BitmapFactory.Options().apply { inSampleSize = sample })
        val long = maxOf(bmp.width, bmp.height)
        val fitted = if (long > LONG_SIDE) android.graphics.Bitmap.createScaledBitmap(bmp, bmp.width * LONG_SIDE / long, bmp.height * LONG_SIDE / long, true) else bmp
        java.io.ByteArrayOutputStream().also { fitted.compress(android.graphics.Bitmap.CompressFormat.JPEG, 90, it) }.toByteArray()
    } catch (e: Throwable) {
        null
    }

    companion object {
        private const val MAX_BYTES = 12 * 1024 * 1024
        private const val SHRINK_OVER = 1536 * 1024
        private const val LONG_SIDE = 2560
    }
}
