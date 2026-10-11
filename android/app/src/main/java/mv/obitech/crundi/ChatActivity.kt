package mv.obitech.crundi

import android.annotation.SuppressLint
import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.view.ViewGroup
import android.webkit.JavascriptInterface
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout

/**
 * One chat by itself: the server's compact page (/chat/<id>) in a window of
 * its own. This is what a bubble opens.
 *
 * It shares the main window's sign-in, because both are the same origin in
 * the same web view store. So it works once the main window has been opened
 * on that server at least once, which pairing does.
 */
class ChatActivity : Activity() {
    private var web: WebView? = null
    private var serverId = ""
    private var chatId = ""

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        serverId = intent.getStringExtra(Notifier.EXTRA_SERVER).orEmpty()
        chatId = intent.getStringExtra(Notifier.EXTRA_CHAT).orEmpty()
        val server = Servers.byId(this, serverId)
        if (server == null || !Regex("^[A-Za-z0-9_-]{1,80}$").matches(chatId)) { finish(); return }
        // In the list of recent apps this window is the chat, not "Crundi"
        // again: its own name, so it can be told from the main window.
        intent.getStringExtra(EXTRA_TITLE)?.let {
            if (it.isNotEmpty()) {
                title = it
                @Suppress("DEPRECATION")
                setTaskDescription(android.app.ActivityManager.TaskDescription(it))
            }
        }

        val root = FrameLayout(this).apply { setBackgroundColor(getColor(R.color.bg)); fitsSystemWindows = true }
        val view = CrundiWebView(this)
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
        view.loadUrl("${server.baseUrl}/chat/$chatId")
    }

    private fun openMain(path: String) {
        if (!path.startsWith("/") || path.startsWith("//")) return
        startActivity(
            Intent(this, MainActivity::class.java).setAction("open:$serverId:$path").putExtra(Notifier.EXTRA_SERVER, serverId).putExtra(Notifier.EXTRA_PATH, path)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        )
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
    }
}
