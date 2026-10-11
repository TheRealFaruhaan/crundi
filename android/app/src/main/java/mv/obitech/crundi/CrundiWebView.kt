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
    private val accepts = arrayOf("image/*")

    init {
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

    companion object { private const val MAX_BYTES = 12 * 1024 * 1024 }
}
