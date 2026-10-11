package mv.obitech.crundi

import android.annotation.SuppressLint
import android.app.Activity
import android.app.PictureInPictureParams
import android.content.Intent
import android.content.res.Configuration
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.util.Rational
import android.view.ViewGroup
import android.webkit.JavascriptInterface
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout

/**
 * One chat by itself: the server's compact page (/chat/<id>) in a window of
 * its own. This is what a bubble opens, and what floats as picture in picture.
 *
 * It shares the main window's sign-in, because both are the same origin in
 * the same web view store. So it works once the main window has been opened
 * on that server at least once, which pairing does.
 */
class ChatActivity : Activity() {
    private var web: WebView? = null
    private var serverId = ""
    private var chatId = ""
    // Asked to open floating, and not yet done so.
    private var floatWhenReady = false

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        serverId = intent.getStringExtra(Notifier.EXTRA_SERVER).orEmpty()
        chatId = intent.getStringExtra(Notifier.EXTRA_CHAT).orEmpty()
        val server = Servers.byId(this, serverId)
        if (server == null || !Regex("^[A-Za-z0-9_-]{1,80}$").matches(chatId)) { finish(); return }
        intent.getStringExtra(EXTRA_TITLE)?.let { if (it.isNotEmpty()) title = it }
        val pip = intent.getBooleanExtra(EXTRA_PIP, false)

        val root = FrameLayout(this).apply { setBackgroundColor(getColor(R.color.bg)); fitsSystemWindows = true }
        val view = WebView(this)
        web = view
        view.setBackgroundColor(getColor(R.color.bg))
        view.settings.apply {
            javaScriptEnabled = true
            domStorageEnabled = true
            allowFileAccess = false
            allowContentAccess = false
            userAgentString = "$userAgentString ${Api.UA_SUFFIX}"
        }
        val base = Uri.parse(server.baseUrl)
        view.webViewClient = object : WebViewClient() {
            override fun shouldOverrideUrlLoading(v: WebView, request: WebResourceRequest): Boolean {
                val u = request.url
                if (u.scheme == base.scheme && u.host == base.host && u.port == base.port) {
                    // Anything but this chat's own page belongs in the main window.
                    if (u.path?.startsWith("/chat/") == true) return false
                    openMain(u.path.orEmpty() + (u.query?.let { "?$it" } ?: ""))
                    return true
                }
                if (!request.isForMainFrame) return false
                runCatching { startActivity(Intent(Intent.ACTION_VIEW, u)) }
                return true
            }
        }
        view.addJavascriptInterface(object {
            @JavascriptInterface fun close() { runOnUiThread { finish() } }
            @JavascriptInterface fun openMain(path: String) { runOnUiThread { this@ChatActivity.openMain(path) } }
            @JavascriptInterface fun chatState(json: String) { /* the bubble's label comes from its notification */ }
        }, "CrundiNative")
        root.addView(view, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        setContentView(root)
        view.loadUrl("${server.baseUrl}/chat/$chatId${if (pip) "?pip=1" else ""}")
        floatWhenReady = pip
    }

    override fun onResume() {
        super.onResume()
        // Android only lets a window that is fully in front shrink itself, so
        // this waits for that and not for onCreate.
        if (floatWhenReady) { floatWhenReady = false; floatNow() }
    }

    private fun openMain(path: String) {
        if (!path.startsWith("/") || path.startsWith("//")) return
        startActivity(
            Intent(this, MainActivity::class.java).setAction("open:$serverId:$path").putExtra(Notifier.EXTRA_SERVER, serverId).putExtra(Notifier.EXTRA_PATH, path)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        )
    }

    /** Shrink to a floating window. Nothing in it can be typed into at that size; it is for watching. */
    private fun floatNow() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        runCatching { enterPictureInPictureMode(PictureInPictureParams.Builder().setAspectRatio(Rational(3, 4)).build()) }
    }

    override fun onPictureInPictureModeChanged(isInPictureInPictureMode: Boolean, newConfig: Configuration) {
        super.onPictureInPictureModeChanged(isInPictureInPictureMode, newConfig)
        // The page drops its header and message box while it is this small,
        // and puts them back when it is made full size again.
        web?.evaluateJavascript("window.crundiSetPip && window.crundiSetPip(${if (isInPictureInPictureMode) "true" else "false"})", null)
    }

    @Deprecated("Still what a plain Activity is given for the back gesture.")
    override fun onBackPressed() { finish() }

    override fun onDestroy() {
        web?.destroy()
        web = null
        super.onDestroy()
    }

    companion object {
        const val EXTRA_TITLE = "title"
        const val EXTRA_PIP = "pip"
    }
}
