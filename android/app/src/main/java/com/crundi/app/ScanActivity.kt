package com.crundi.app

import android.Manifest
import android.annotation.SuppressLint
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Bundle
import android.webkit.JavascriptInterface
import android.webkit.PermissionRequest
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.webkit.WebViewAssetLoader

/**
 * Scanning the pairing picture.
 *
 * The picture is Crundi's own format, and the thing that reads it is the same
 * file the server draws it with (src/crundi-code.js, copied into this app's
 * assets when it is built). So this screen is a small page of our own, shown
 * in a web view, holding the camera and that reader. It loads nothing from
 * the network: every address it is asked for is answered from the app's own
 * files or refused.
 */
class ScanActivity : Activity() {
    private var web: WebView? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (checkSelfPermission(Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) start()
        else requestPermissions(arrayOf(Manifest.permission.CAMERA), 1)
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (grantResults.firstOrNull() == PackageManager.PERMISSION_GRANTED) start()
        else { setResult(RESULT_CANCELED); finish() }
    }

    @SuppressLint("SetJavaScriptEnabled")
    private fun start() {
        val loader = WebViewAssetLoader.Builder().addPathHandler("/assets/", WebViewAssetLoader.AssetsPathHandler(this)).build()
        val view = WebView(this)
        web = view
        view.setBackgroundColor(getColor(R.color.bg))
        view.settings.javaScriptEnabled = true
        view.settings.mediaPlaybackRequiresUserGesture = false
        view.settings.allowFileAccess = false
        view.settings.allowContentAccess = false
        view.webViewClient = object : WebViewClient() {
            // Our own files, or nothing.
            override fun shouldInterceptRequest(v: WebView, request: WebResourceRequest): WebResourceResponse? =
                loader.shouldInterceptRequest(request.url) ?: WebResourceResponse("text/plain", "utf-8", 404, "Not found", emptyMap(), null)
            override fun shouldOverrideUrlLoading(v: WebView, request: WebResourceRequest) = true
        }
        view.webChromeClient = object : WebChromeClient() {
            override fun onPermissionRequest(request: PermissionRequest) {
                val ours = request.origin == Uri.parse(ORIGIN) || request.origin.toString().trimEnd('/') == ORIGIN
                if (ours && PermissionRequest.RESOURCE_VIDEO_CAPTURE in request.resources) request.grant(arrayOf(PermissionRequest.RESOURCE_VIDEO_CAPTURE))
                else request.deny()
            }
        }
        view.addJavascriptInterface(object {
            @JavascriptInterface fun found(link: String) {
                // The page says it read something. Believe it only if it is
                // the shape of a pairing link.
                if (PairingLink.parse(link) == null) return
                runOnUiThread {
                    setResult(RESULT_OK, Intent().putExtra(EXTRA_LINK, link))
                    finish()
                }
            }
            @JavascriptInterface fun cancel() { runOnUiThread { setResult(RESULT_CANCELED); finish() } }
        }, "CrundiScan")
        setContentView(view)
        view.loadUrl("$ORIGIN/assets/scan.html")
    }

    override fun onDestroy() {
        web?.apply { loadUrl("about:blank"); destroy() }
        web = null
        super.onDestroy()
    }

    companion object {
        const val EXTRA_LINK = "link"
        private const val ORIGIN = "https://appassets.androidplatform.net"
    }
}
