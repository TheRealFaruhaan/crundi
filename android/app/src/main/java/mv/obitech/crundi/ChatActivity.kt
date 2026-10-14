package mv.obitech.crundi

import android.annotation.SuppressLint
import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.view.ViewGroup
import android.Manifest
import android.content.ActivityNotFoundException
import android.content.pm.PackageManager
import android.webkit.JavascriptInterface
import android.webkit.PermissionRequest
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
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
    private var openKey = ""
    private var fileCallback: ValueCallback<Array<Uri>>? = null

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        serverId = intent.getStringExtra(Notifier.EXTRA_SERVER).orEmpty()
        chatId = intent.getStringExtra(Notifier.EXTRA_CHAT).orEmpty()
        val server = Servers.byId(this, serverId)
        if (server == null || !Regex("^[A-Za-z0-9_-]{1,80}$").matches(chatId)) { finish(); return }
        openKey = "$serverId:$chatId"
        opened(openKey)
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
            @JavascriptInterface fun seen(id: String) { if (id == chatId) runOnUiThread { Notifier.clearChat(this@ChatActivity, serverId, chatId, why = "its own window reported it seen") } }
            /** A file Claude offered in the chat: save it, exactly as the main window would. */
            @JavascriptInterface fun download(url: String, name: String) {
                runOnUiThread { Downloads.fromPage(this@ChatActivity, Servers.byId(this@ChatActivity, serverId), url, name, web?.settings?.userAgentString) }
            }
        }, "CrundiNative")
        // The same things the main window lets a page do: pick a file to
        // attach, and use the camera or microphone once Android has allowed
        // it. Without this a web view does neither, silently: the attach
        // button in a bubble simply did nothing.
        view.webChromeClient = object : WebChromeClient() {
            override fun onShowFileChooser(v: WebView, callback: ValueCallback<Array<Uri>>, params: FileChooserParams): Boolean {
                fileCallback?.onReceiveValue(null)
                fileCallback = callback
                return try {
                    @Suppress("DEPRECATION")
                    startActivityForResult(params.createIntent().apply { putExtra(Intent.EXTRA_ALLOW_MULTIPLE, params.mode == FileChooserParams.MODE_OPEN_MULTIPLE) }, REQ_FILE)
                    true
                } catch (e: ActivityNotFoundException) {
                    fileCallback = null
                    false
                }
            }

            override fun onPermissionRequest(request: PermissionRequest) {
                val o = request.origin
                if (!(o.scheme == base.scheme && o.host == base.host && o.port == base.port)) { request.deny(); return }
                // Only what Android has already allowed the app. Asking for a
                // new permission is done from the main window, not a bubble.
                val ok = request.resources.all {
                    when (it) {
                        PermissionRequest.RESOURCE_VIDEO_CAPTURE -> checkSelfPermission(Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED
                        PermissionRequest.RESOURCE_AUDIO_CAPTURE -> checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED
                        else -> false
                    }
                }
                if (ok) request.grant(request.resources) else request.deny()
            }
        }
        view.settings.mediaPlaybackRequiresUserGesture = false
        view.setDownloadListener { url, userAgent, contentDisposition, mimeType, _ -> Downloads.start(this, url, userAgent, contentDisposition, mimeType) }
        root.addView(view, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        setContentView(root)
        // Its login comes from the app, as the main window's does (PageLogin).
        if (LoginChannel.on(applicationContext, server.id)) LoginChannel.attach(view, server)
        view.loadUrl("${server.baseUrl}/chat/$chatId")
    }

    private fun openMain(path: String) {
        if (!path.startsWith("/") || path.startsWith("//")) return
        startActivity(
            Intent(this, MainActivity::class.java).setAction("open:$serverId:$path").putExtra(Notifier.EXTRA_SERVER, serverId).putExtra(Notifier.EXTRA_PATH, path)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        )
    }

    @Deprecated("The platform's own result call; enough for the file chooser.")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != REQ_FILE) return
        val picked = WebChromeClient.FileChooserParams.parseResult(resultCode, data)
            ?: data?.clipData?.let { clip -> Array(clip.itemCount) { clip.getItemAt(it).uri } }
        fileCallback?.onReceiveValue(picked)
        fileCallback = null
    }

    @Deprecated("Still what a plain Activity is given for the back gesture.")
    override fun onBackPressed() { finish() }

    override fun onDestroy() {
        if (openKey.isNotEmpty()) closed(openKey)
        web?.destroy()
        web = null
        super.onDestroy()
    }

    companion object {
        const val EXTRA_TITLE = "title"
        private const val REQ_FILE = 2

        // Which chats have a window of their own right now. Counted, since
        // Android can build a second window for a chat before it has taken
        // the first one down.
        private val open = HashMap<String, Int>()
        private fun opened(key: String) = synchronized(open) { open[key] = (open[key] ?: 0) + 1 }
        private fun closed(key: String) = synchronized(open) { val n = (open[key] ?: 0) - 1; if (n > 0) open[key] = n else open.remove(key) }
        fun isOpen(serverId: String, chatId: String): Boolean = synchronized(open) { open.containsKey("$serverId:$chatId") }
    }
}
