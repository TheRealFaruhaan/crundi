package mv.obitech.crundi.ui.chats

import androidx.compose.ui.input.pointer.pointerInput
import android.annotation.SuppressLint
import android.app.Activity
import android.content.pm.ActivityInfo
import android.graphics.Bitmap
import android.os.Message
import android.view.View
import android.view.ViewGroup
import android.webkit.CookieManager
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalInspectionMode
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.min
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import mv.obitech.crundi.api.ApiRequest
import mv.obitech.crundi.api.model.ChatMedia
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.shell.LocalHost
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.Dimens

/**
 * The origin Claude's own HTML runs at: a name that can never resolve, so the
 * page has no address of ours and nothing it stores or asks for reaches the
 * server, as the web's sandboxed srcdoc frame has no origin at all.
 */
private const val HTML_ORIGIN = "https://embed.crundi.invalid/"

/** The frame's dark backdrop while it loads, as .wg-membed. */
private val FrameBg = Color(0xFF0F0F14)

/**
 * The box a frame gets, as the web's embedItem: an aspect (16:9 video, 9:16
 * shorts) is 640 wide when wide and 560 tall when tall; without one, the
 * stated width (default 640) by the stated height (default 420). Then fit to
 * [maxWidth], keeping the aspect when there is one.
 */
internal fun embedSize(m: ChatMedia, maxWidth: Dp): Pair<Dp, Dp> {
    return if (m.width > 0 && m.height > 0) {
        val want = if (m.width >= m.height) 640.dp else (560f * m.width / m.height).dp
        val w = min(want, maxWidth)
        w to w * (m.height.toFloat() / m.width)
    } else {
        min((m.frameWidth.takeIf { it > 0 } ?: 640).dp, maxWidth) to (m.frameHeight.takeIf { it > 0 } ?: 420).dp
    }
}

/**
 * A live frame Claude showed (show_embed), with its caption under it and
 * "Open on <provider>" when it has an address of its own. [maxWidth] caps
 * the box (a row of several gives each less than the chat's width).
 */
@Composable
fun EmbedItem(m: ChatMedia, actions: EntryActions, maxWidth: Dp? = null) {
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val (w, h) = embedSize(m, min(maxWidth ?: this.maxWidth, this.maxWidth))
        Column(Modifier.width(IntrinsicSize.Min)) {
            LiveEmbed(m, Modifier.size(w, h))
            EmbedCaption(m, actions, Modifier.width(w).padding(top = 4.dp))
        }
    }
}

@Composable
private fun EmbedCaption(m: ChatMedia, actions: EntryActions, modifier: Modifier) {
    val what = listOf(m.title, m.caption).filter { it.isNotEmpty() }.joinToString(" · ").ifEmpty { m.provider.ifEmpty { if (m.path != null) "Embedded HTML" else "" } }
    val open = m.openUrl
    if (what.isEmpty() && open == null) return
    val text = buildAnnotatedString {
        append(what)
        if (open != null) {
            if (what.isNotEmpty()) append(" · ")
            withStyle(SpanStyle(color = CrundiColors.AccentHover)) {
                append("Open" + if (m.provider.isNotEmpty() && what != m.provider) " on " + m.provider else "")
            }
        }
    }
    Text(
        text, style = ChatType.Small, maxLines = 3, overflow = TextOverflow.Ellipsis,
        modifier = modifier.then(if (open != null) Modifier.clickable(onClickLabel = "Open outside the app") { actions.media(m) } else Modifier),
    )
}

/**
 * The frame itself: a WebView made when the box nears the screen and destroyed
 * when it is far off or leaves the chat, so a long chat of embeds does not
 * keep dozens of pages running.
 *
 * A site's embed loads its own address with JavaScript, storage and cookies
 * (a player needs them). Claude's HTML is fetched as text with the phone's
 * sign-in and runs at a dead origin with no bridge into the app. Neither can
 * navigate the app: a link the person taps, or a window it opens, goes to the
 * phone's browser instead.
 */
@Composable
fun LiveEmbed(m: ChatMedia, modifier: Modifier = Modifier) {
    val services = LocalAppServices.current
    val transport = services.api.transport
    val host = LocalHost.current
    val view = LocalView.current
    val ctx = LocalContext.current
    val activity = remember(ctx) { generateSequence(ctx) { (it as? android.content.ContextWrapper)?.baseContext }.filterIsInstance<Activity>().firstOrNull() }
    val inspection = LocalInspectionMode.current

    // A frame of the server's own address would run as the person; the web
    // refuses it too.
    val serverHost = remember(transport.baseUrl) { runCatching { java.net.URI(transport.baseUrl).host }.getOrNull() }
    val src = m.src?.takeIf { runCatching { java.net.URI(it).host }.getOrNull()?.equals(serverHost, ignoreCase = true) != true }

    var near by remember { mutableStateOf(false) }
    var loading by remember { mutableStateOf(true) }
    var failed by remember { mutableStateOf(false) }
    // Bumped by a retry, so the frame is made afresh.
    var attempt by remember { mutableStateOf(0) }
    var html by remember(m.path) { mutableStateOf<String?>(null) }
    var gone by remember(m.path) { mutableStateOf(false) }
    val full = remember { EmbedFullscreen(activity, m) }
    var fullOn by remember { mutableStateOf(false) }
    full.onChange = { fullOn = it }

    LaunchedEffect(m.path, near) {
        val path = m.path ?: return@LaunchedEffect
        if (!near || html != null || gone) return@LaunchedEffect
        // Kept trying every few seconds while the box is near (the phone may
        // be between networks); a 4xx means the server no longer has it.
        while (html == null && !gone) {
            val r = runCatching { transport.exchange(ApiRequest("GET", path.substringBefore('?'), queryOf(path))) }.getOrNull()
            if (r != null && r.status == 200) html = r.text
            else if (r != null && r.status in 400..499) gone = true
            else kotlinx.coroutines.delay(5000)
        }
    }
    DisposableEffect(Unit) { onDispose { full.hide() } }
    if (fullOn) BackHandler { full.hide() }

    Box(
        modifier.clip(Dimens.ShapeSm).background(FrameBg).border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm)
            .onGloballyPositioned { c ->
                // Near: within a screen of the viewport. Far: more than two and
                // a half away. The gap keeps a frame from being rebuilt while
                // it sits at the edge. Full screen is never far.
                val top = c.positionInWindow().y
                val bottom = top + c.size.height
                val screen = view.height.toFloat().coerceAtLeast(1f)
                if (!near && bottom > -screen && top < 2 * screen) near = true
                else if (near && !fullOn && (bottom < -2.5f * screen || top > 3.5f * screen)) near = false
            },
        contentAlignment = Alignment.Center,
    ) {
        when {
            gone -> Text("This embed is no longer stored.", style = ChatType.Small.copy(color = CrundiColors.TextMuted))
            m.path == null && src == null -> Text("This frame cannot be shown here.", style = ChatType.Small.copy(color = CrundiColors.TextMuted))
            inspection -> CrundiIcon("globe", size = 28.dp, tint = CrundiColors.TextMuted)
            failed -> LoadFailed({ failed = false; loading = true; attempt++ })
            near && (src != null || html != null) -> androidx.compose.runtime.key(attempt) {
                val doc = html
                var frame by remember { mutableStateOf<WebView?>(null) }
                AndroidView(
                    factory = { c ->
                        loading = true
                        makeFrame(c, ownHtml = src == null, onLoaded = { loading = false }, onFailed = { failed = true; loading = false }, onOpen = { host.openExternal(it) }, full = full).also { frame = it }.apply {
                            if (src != null) loadUrl(src, mapOf("Referer" to transport.baseUrl.trimEnd('/') + "/"))
                            else loadDataWithBaseURL(HTML_ORIGIN, doc ?: "", "text/html", "utf-8", null)
                        }
                    },
                    onRelease = { wv -> frame = null; full.hide(); destroyFrame(wv) },
                    // A swipe that starts here scrolls the frame while it can
                    // scroll that way (and any sideways swipe is the frame's);
                    // at its end the chat scrolls instead, so a tall embed
                    // never traps the page. The chat took every swipe before.
                    modifier = Modifier.fillMaxSize().pointerInput(Unit) {
                        awaitPointerEventScope {
                            while (true) {
                                val e = awaitPointerEvent(androidx.compose.ui.input.pointer.PointerEventPass.Main)
                                val w = frame ?: continue
                                for (ch in e.changes) {
                                    if (!ch.pressed || !ch.previousPressed) continue
                                    val dx = ch.position.x - ch.previousPosition.x
                                    val dy = ch.position.y - ch.previousPosition.y
                                    if (dx == 0f && dy == 0f) continue
                                    val mine = kotlin.math.abs(dx) > kotlin.math.abs(dy) || w.canScrollVertically(if (dy < 0) 1 else -1)
                                    if (mine) ch.consume()
                                }
                            }
                        }
                    },
                )
            }
        }
        if (near && loading && !gone && !failed && (src != null || m.path != null) && !inspection) LoadingRing()
    }
}

@SuppressLint("SetJavaScriptEnabled")
private fun makeFrame(c: android.content.Context, ownHtml: Boolean, onLoaded: () -> Unit, onFailed: () -> Unit, onOpen: (String) -> Unit, full: EmbedFullscreen): WebView =
    WebView(c).apply {
        setBackgroundColor(android.graphics.Color.rgb(15, 15, 20))
        layoutParams = ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
        settings.apply {
            javaScriptEnabled = true
            // A site's player keeps its state in storage; Claude's page has a
            // dead origin, so whatever it stores is its own and goes nowhere.
            domStorageEnabled = true
            allowFileAccess = false
            allowContentAccess = false
            setSupportMultipleWindows(true)
            javaScriptCanOpenWindowsAutomatically = false
            mediaPlaybackRequiresUserGesture = true
            mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
            setSupportZoom(false)
            // Laid out at the frame's own width, as an iframe is. With a wide
            // viewport a page without its own viewport tag (an X post) laid out
            // at desktop width and was zoomed out, small in a large empty box.
            loadWithOverviewMode = false
            useWideViewPort = false
        }
        CookieManager.getInstance().setAcceptThirdPartyCookies(this, !ownHtml)
        webViewClient = object : WebViewClient() {
            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                val url = request.url
                val web = url.scheme == "https" || url.scheme == "http"
                // A tap on a link leaves for the browser; the frame's own
                // redirects and script loads stay inside it. Other schemes
                // (intent:, file:) are never followed.
                if (!web) return true
                if (request.isForMainFrame && request.hasGesture() && !request.isRedirect) { onOpen(url.toString()); return true }
                return false
            }
            override fun onPageStarted(view: WebView, url: String?, favicon: Bitmap?) {}
            override fun onPageFinished(view: WebView, url: String?) { onLoaded() }
            // The page itself could not be fetched (offline, DNS): say so with
            // a retry. Errors of a part of the page are the page's business.
            override fun onReceivedError(view: WebView, request: WebResourceRequest, error: android.webkit.WebResourceError) {
                if (request.isForMainFrame) onFailed()
            }
        }
        webChromeClient = object : WebChromeClient() {
            override fun onShowCustomView(view: View, callback: CustomViewCallback) { full.show(view, callback) }
            override fun onHideCustomView() { full.hide() }
            // A window the frame opens (target=_blank, window.open after a tap)
            // is caught by a throwaway WebView and its address sent to the browser.
            override fun onCreateWindow(view: WebView, isDialog: Boolean, isUserGesture: Boolean, resultMsg: Message): Boolean {
                if (!isUserGesture) return false
                val catcher = WebView(view.context)
                catcher.webViewClient = object : WebViewClient() {
                    override fun shouldOverrideUrlLoading(v: WebView, r: WebResourceRequest): Boolean {
                        val u = r.url
                        if (u.scheme == "https" || u.scheme == "http") onOpen(u.toString())
                        v.post { v.destroy() }
                        return true
                    }
                }
                (resultMsg.obj as? WebView.WebViewTransport)?.webView = catcher
                resultMsg.sendToTarget()
                return true
            }
        }
    }

private fun destroyFrame(wv: WebView) {
    runCatching {
        wv.stopLoading()
        wv.loadUrl("about:blank")
        (wv.parent as? ViewGroup)?.removeView(wv)
        wv.destroy()
    }
}

/**
 * An embed's video in full screen (YouTube's button): the player's own view
 * laid over the whole app, system bars hidden and the screen turned the
 * video's way; put back as it was when the player, Back or leaving ends it.
 */
private class EmbedFullscreen(private val activity: Activity?, private val m: ChatMedia) {
    private var view: View? = null
    private var callback: WebChromeClient.CustomViewCallback? = null
    private var orientation = ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
    var onChange: (Boolean) -> Unit = {}

    fun show(v: View, cb: WebChromeClient.CustomViewCallback) {
        val a = activity ?: run { cb.onCustomViewHidden(); return }
        if (view != null) hide()
        val decor = a.window.decorView as? FrameLayout ?: run { cb.onCustomViewHidden(); return }
        view = v; callback = cb
        v.setBackgroundColor(android.graphics.Color.BLACK)
        decor.addView(v, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        WindowCompat.getInsetsController(a.window, decor).apply {
            systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            hide(WindowInsetsCompat.Type.systemBars())
        }
        orientation = a.requestedOrientation
        a.requestedOrientation = when {
            m.width > 0 && m.height > m.width -> ActivityInfo.SCREEN_ORIENTATION_SENSOR_PORTRAIT
            m.width > 0 && m.height > 0 -> ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
            else -> ActivityInfo.SCREEN_ORIENTATION_SENSOR
        }
        onChange(true)
    }

    fun hide() {
        val v = view ?: return
        val a = activity ?: return
        view = null
        (v.parent as? ViewGroup)?.removeView(v)
        WindowCompat.getInsetsController(a.window, a.window.decorView).show(WindowInsetsCompat.Type.systemBars())
        a.requestedOrientation = orientation
        val cb = callback; callback = null
        runCatching { cb?.onCustomViewHidden() }
        onChange(false)
    }
}
