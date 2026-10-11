package com.crundi.app

import android.Manifest
import android.annotation.SuppressLint
import android.app.Activity
import android.app.DownloadManager
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.webkit.CookieManager
import android.webkit.JavascriptInterface
import android.webkit.PermissionRequest
import android.webkit.URLUtil
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.Toast

/**
 * The app itself: the server's own page, in a web view, with the phone's
 * features round it.
 *
 * Before the page is shown the app asks the server who it is (the handshake)
 * and each side checks the other is not too old. Only then is the page loaded.
 * The first time after pairing, the page's login is handed to it in the part
 * of the address a browser never sends (#token=…), which the page already
 * knows how to take; after that the page keeps and renews it by itself.
 */
class MainActivity : Activity() {
    private lateinit var root: FrameLayout
    private var web: WebView? = null
    private var server: Server? = null
    private var fileCallback: ValueCallback<Array<Uri>>? = null
    private var pendingPermission: PermissionRequest? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        root = FrameLayout(this).apply { setBackgroundColor(getColor(R.color.bg)); fitsSystemWindows = true }
        setContentView(root)
        open()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        // Back from pairing: there may be a different server to show.
        if (server?.id != Servers.current(this)?.id) open()
    }

    /** Show the current server, or the pairing screen if there is none. */
    private fun open() {
        val s = Servers.current(this)
        if (s == null) {
            startActivity(Intent(this, PairActivity::class.java))
            finish()
            return
        }
        server = s
        notice("Connecting to ${s.host}…", null)
        Bg.run({ Api.handshake(s.baseUrl) }) { result ->
            if (server?.id != s.id || isFinishing) return@run
            val reply = result.getOrNull()
            val verdict = reply?.json?.let { Gate.judge(it, BuildConfig.VERSION_NAME, BuildConfig.MIN_SERVER) }
            when {
                reply == null -> notice("Could not reach ${s.host}.\n\nCheck that this phone is online and the server is running.", "Try again") { open() }
                verdict == null -> notice("${s.host} did not answer as a Crundi server.", "Try again") { open() }
                verdict.appTooOld && verdict.serverTooOld -> notice(
                    "This app and this server are each too old for the other.\n\nThe server is ${verdict.serverVersion.ifEmpty { "an unknown version" }} and this app needs ${BuildConfig.MIN_SERVER} or newer. The server in turn needs a newer app${if (verdict.minApp.isNotEmpty()) " (${verdict.minApp} or newer)" else ""}.\n\nUpdate both.",
                    Updater.LABEL,
                ) { Updater.open(this) }
                verdict.appTooOld -> notice(
                    "Update required\n\nThis app is version ${BuildConfig.VERSION_NAME}. This server needs ${verdict.minApp.ifEmpty { "a newer version" }}${if (verdict.minApp.isNotEmpty()) " or newer" else ""}.",
                    Updater.LABEL,
                ) { Updater.open(this) }
                verdict.serverTooOld -> notice(
                    "This server needs updating\n\n${s.host} is version ${verdict.serverVersion.ifEmpty { "unknown" }}. This app needs ${BuildConfig.MIN_SERVER} or newer.\n\nUpdate the server from its own Settings, on a computer. If it is not your server, ask its owner.",
                    "Try again",
                ) { open() }
                else -> showPage(s)
            }
        }
    }

    /** A plain statement over the whole screen, with at most one thing to do, and a way to another server. */
    private fun notice(text: String, action: String?, onAction: (() -> Unit)? = null) {
        web?.visibility = View.GONE
        root.findViewWithTag<View>("notice")?.let { root.removeView(it) }
        val col = Ui.column(this).apply { tag = "notice"; gravity = Gravity.CENTER }
        col.addView(ImageView(this).apply { setImageResource(R.drawable.ic_logo) }, LinearLayout.LayoutParams(Ui.dp(this, 72), Ui.dp(this, 72)))
        col.addView(Ui.body(this, text).apply { setTextColor(getColor(R.color.text)) }, Ui.wide(this, 20))
        if (action != null && onAction != null) col.addView(Ui.button(this, action, true, onAction), Ui.wide(this, 24))
        if (action != null) col.addView(Ui.button(this, "Connect to another server", false) { startActivity(Intent(this, PairActivity::class.java)) }, Ui.wide(this, 10))
        root.addView(col, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
    }

    @SuppressLint("SetJavaScriptEnabled")
    private fun showPage(s: Server) {
        root.findViewWithTag<View>("notice")?.let { root.removeView(it) }
        web?.let { root.removeView(it); it.destroy() }
        val view = WebView(this)
        web = view
        view.setBackgroundColor(getColor(R.color.bg))
        view.settings.apply {
            javaScriptEnabled = true
            domStorageEnabled = true
            mediaPlaybackRequiresUserGesture = false
            allowFileAccess = false
            allowContentAccess = false
            // The server reads this to know which app, and which version, is
            // asking: it is how a too-old app is refused on every request.
            userAgentString = "$userAgentString ${Api.UA_SUFFIX}"
        }
        CookieManager.getInstance().setAcceptCookie(true)
        val base = Uri.parse(s.baseUrl)

        view.webViewClient = object : WebViewClient() {
            override fun shouldOverrideUrlLoading(v: WebView, request: WebResourceRequest): Boolean {
                val u = request.url
                // The server's own pages stay here. Anything else is somebody
                // else's site, and opens in the browser, where the person can
                // see where they are.
                if (u.scheme == base.scheme && u.host == base.host && u.port == base.port) return false
                if (!request.isForMainFrame) return false
                try { startActivity(Intent(Intent.ACTION_VIEW, u)) } catch (e: ActivityNotFoundException) { /* nothing can open it */ }
                return true
            }
        }
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
                // Only the server's own page may have the camera or microphone,
                // and only once Android has been asked on the person's behalf.
                val o = request.origin
                if (!(o.scheme == base.scheme && o.host == base.host && o.port == base.port)) { request.deny(); return }
                val need = request.resources.mapNotNull {
                    when (it) {
                        PermissionRequest.RESOURCE_VIDEO_CAPTURE -> Manifest.permission.CAMERA
                        PermissionRequest.RESOURCE_AUDIO_CAPTURE -> Manifest.permission.RECORD_AUDIO
                        else -> null
                    }
                }
                if (need.isEmpty()) { request.deny(); return }
                val missing = need.filter { checkSelfPermission(it) != PackageManager.PERMISSION_GRANTED }
                if (missing.isEmpty()) { request.grant(request.resources); return }
                pendingPermission = request
                requestPermissions(missing.toTypedArray(), REQ_PERMISSION)
            }
        }
        view.setDownloadListener { url, userAgent, contentDisposition, mimeType, _ -> download(url, userAgent, contentDisposition, mimeType) }
        // What the page may ask of the app. Nothing here returns anything
        // private: these are reachable from any frame in the page, including
        // ones showing other people's content, so they are limited to things
        // it would be harmless for such a frame to ask for.
        view.addJavascriptInterface(Bridge(), "CrundiNative")

        root.addView(view, 0, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        // First load after pairing: hand the page its login, then forget our
        // copy. A refresh token is spent by using it, so two holders of one
        // would cut each other off; from here on it is the page's alone.
        val handoff = if (s.pageToken.isNotEmpty()) "/#token=${s.pageToken}" + (if (s.pageRefresh.isNotEmpty()) "&refresh=${s.pageRefresh}" else "") else "/"
        if (s.pageToken.isNotEmpty()) {
            s.pageToken = ""; s.pageRefresh = ""
            Servers.save(this, s)
        }
        view.loadUrl(s.baseUrl + handoff)
    }

    private inner class Bridge {
        /** The back arrow in the compact chat. Here there is no bubble to put away, so it is plain back. */
        @JavascriptInterface fun close() { runOnUiThread { goBack() } }

        /** "Open in Crundi" from the compact chat: show a path of the server's in this window. */
        @JavascriptInterface fun openMain(path: String) {
            val s = server ?: return
            // A path on the server and nothing else: never an address a frame chose.
            if (!path.startsWith("/") || path.startsWith("//")) return
            runOnUiThread { web?.loadUrl(s.baseUrl + path) }
        }

        /** The compact chat saying what it is showing. Used by the bubble, when there is one. */
        @JavascriptInterface fun chatState(json: String) { /* nothing listens yet */ }
    }

    private fun download(url: String, userAgent: String?, contentDisposition: String?, mimeType: String?) {
        if (!url.startsWith("http://") && !url.startsWith("https://")) {
            Toast.makeText(this, "This file cannot be saved from here yet.", Toast.LENGTH_LONG).show()
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
            else setDestinationInExternalFilesDir(this@MainActivity, Environment.DIRECTORY_DOWNLOADS, name)
        }
        (getSystemService(Context.DOWNLOAD_SERVICE) as DownloadManager).enqueue(req)
        Toast.makeText(this, "Downloading $name", Toast.LENGTH_SHORT).show()
    }

    @Deprecated("The platform's own result call; enough for the file chooser.")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != REQ_FILE) return
        fileCallback?.onReceiveValue(WebChromeClient.FileChooserParams.parseResult(resultCode, data) ?: multiple(data))
        fileCallback = null
    }

    // Several files come back as clip data, which parseResult does not read.
    private fun multiple(data: Intent?): Array<Uri>? {
        val clip = data?.clipData ?: return null
        return Array(clip.itemCount) { clip.getItemAt(it).uri }
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode != REQ_PERMISSION) return
        val req = pendingPermission ?: return
        pendingPermission = null
        if (grantResults.isNotEmpty() && grantResults.all { it == PackageManager.PERMISSION_GRANTED }) req.grant(req.resources) else req.deny()
    }

    private fun goBack() {
        val w = web
        if (w != null && w.visibility == View.VISIBLE && w.canGoBack()) w.goBack() else moveTaskToBack(true)
    }

    @Deprecated("Still what a plain Activity is given for the back gesture.")
    override fun onBackPressed() = goBack()

    override fun onDestroy() {
        web?.destroy()
        web = null
        super.onDestroy()
    }

    companion object {
        private const val REQ_FILE = 2
        private const val REQ_PERMISSION = 3
    }
}
