package mv.obitech.crundi.ui.browser

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Bitmap
import android.view.ViewGroup
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.ListRow
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens
import mv.obitech.crundi.ui.workbench.LocalPane
import mv.obitech.crundi.ui.workbench.WbCell
import java.net.URLEncoder

/*
 * Owned by ui.workspace. The Browser pane and the Browsers tab's "On this
 * phone" browser: one of the two places a WebView is allowed (approved
 * exception, android-architecture.md). An ordinary interactive browser:
 * URL bar, back / forward / reload, and the desktop's device emulation
 * (BRZ_DEVICES in src/webapp-html.js): Responsive fills the pane; a device
 * gets its own viewport size and user agent, scaled to fit and centred.
 *
 * Locked down: no file or content access, no JavaScript bridge, no new
 * windows, only http(s) and about: pages; nothing of the app's sign-in goes
 * into it (it is a browser for the open web and the person's own sites).
 */

/** One entry of the desktop's BRZ_DEVICES. [w]/[h] in CSS pixels. */
data class BrzDevice(val name: String, val w: Int = 0, val h: Int = 0, val dpr: Float = 1f, val mobile: Boolean = false, val ua: String = "", val responsive: Boolean = false) {
    val label get() = if (responsive) "Responsive (fill)" else "$name  ${w}×$h"
}

private const val UA_IOS16 = "Mozilla/5.0 (iPhone; CPU iPhone OS 16_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.0 Mobile/15E148 Safari/604.1"
private const val UA_IOS17 = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1"
private const val UA_IOS18 = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1"
private const val UA_IPAD = "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1"

/** The desktop's list, in its order. */
val BRZ_DEVICES = listOf(
    BrzDevice("Responsive", responsive = true),
    BrzDevice("iPhone SE", 375, 667, 2f, true, UA_IOS16),
    BrzDevice("iPhone 12 Pro", 390, 844, 3f, true, UA_IOS16),
    BrzDevice("iPhone 15 Pro", 393, 852, 3f, true, UA_IOS17),
    BrzDevice("iPhone 16 Pro", 393, 852, 3f, true, UA_IOS18),
    BrzDevice("iPhone 16 Pro Max", 430, 932, 3f, true, UA_IOS18),
    BrzDevice("iPad Mini", 768, 1024, 2f, true, UA_IPAD),
    BrzDevice("iPad Air", 820, 1180, 2f, true, UA_IPAD),
    BrzDevice("iPad Pro", 1024, 1366, 2f, true, UA_IPAD),
    BrzDevice("Pixel 5", 393, 851, 2.75f, true, "Mozilla/5.0 (Linux; Android 11; Pixel 5) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/90.0.4430.91 Mobile Safari/537.36"),
    BrzDevice("Pixel 7", 412, 915, 2.625f, true, "Mozilla/5.0 (Linux; Android 13; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36"),
    BrzDevice("Pixel 8 Pro", 430, 932, 2.625f, true, "Mozilla/5.0 (Linux; Android 14; Pixel 8 Pro) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36"),
    BrzDevice("Galaxy S22", 360, 800, 3f, true, "Mozilla/5.0 (Linux; Android 13; SM-S901B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36"),
    BrzDevice("Galaxy S24 Ultra", 412, 915, 2.625f, true, "Mozilla/5.0 (Linux; Android 14; SM-S928B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36"),
    BrzDevice("Galaxy Z Fold 5", 344, 882, 2.8f, true, "Mozilla/5.0 (Linux; Android 13; SM-F946B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/114.0.0.0 Mobile Safari/537.36"),
    BrzDevice("Galaxy Z Flip 5", 264, 844, 2.8f, true, "Mozilla/5.0 (Linux; Android 13; SM-F731B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/114.0.0.0 Mobile Safari/537.36"),
    BrzDevice("Surface Pro 9", 1200, 1800, 2f, true, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"),
    BrzDevice("Desktop 1280", 1280, 800, 1f, false, ""),
    BrzDevice("Full HD 1920", 1920, 1080, 1f, false, ""),
)

const val BRZ_START = "https://www.google.com"

/**
 * What the URL bar's text means, as the web's brzGo: a URL with a scheme as
 * it is, something that looks like a domain gets https://, anything else is
 * a Google search. Null for nothing typed.
 */
fun brzTarget(typed: String): String? {
    val v = typed.trim()
    if (v.isEmpty()) return null
    val hasScheme = Regex("^https?://", RegexOption.IGNORE_CASE).containsMatchIn(v) || v.startsWith("about:")
    val looksDomain = !v.contains(' ') && v.indexOf('.') > 0
    return when {
        hasScheme -> v
        looksDomain -> "https://$v"
        Regex("^localhost(:|/|$)", RegexOption.IGNORE_CASE).containsMatchIn(v) -> "http://$v"
        else -> "https://www.google.com/search?q=" + URLEncoder.encode(v, "UTF-8")
    }
}

/**
 * Where a device's viewport goes in a stage of [stageW] x [stageH] (dp):
 * scaled to fit both sides inside a 10 dp margin (it may grow for small
 * devices), centred. Answers scale, left, top. As the web's brzComputeRect.
 */
fun brzFit(d: BrzDevice, stageW: Float, stageH: Float): Triple<Float, Float, Float> {
    if (d.responsive || d.w <= 0 || d.h <= 0) return Triple(1f, 0f, 0f)
    val pad = 10f
    val availW = maxOf(40f, stageW - pad * 2)
    val availH = maxOf(40f, stageH - pad * 2)
    val scale = minOf(availW / d.w, availH / d.h)
    return Triple(scale, (stageW - d.w * scale) / 2f, (stageH - d.h * scale) / 2f)
}

/** What the bar shows. [field] is the URL box's text while it is being edited. */
data class BrowserUi(
    val url: String = BRZ_START,
    val field: String = url,
    val device: Int = 0,
    val canBack: Boolean = false,
    val canForward: Boolean = false,
    val loading: Boolean = false,
    val title: String = "",
    /** "Loading..." before the first page, or why a page could not load. */
    val hint: String? = "Loading…",
)

/**
 * The browser: bar (.brz-bar: back, forward, reload, the URL pill, the
 * device picker) over the stage (.brz-stage, dotted). [stage] draws the
 * page into the stage; tests give it a stand-in.
 */
@Composable
fun BrowserScreen(
    ui: BrowserUi,
    onField: (String) -> Unit,
    onGo: () -> Unit,
    onBack: () -> Unit,
    onForward: () -> Unit,
    onReload: () -> Unit,
    onDevice: (Int) -> Unit,
    modifier: Modifier = Modifier,
    pickingDevice: Boolean = false,
    stage: @Composable BoxScope.(BrzDevice) -> Unit,
) {
    var picking by remember { mutableStateOf(pickingDevice) }
    val dev = BRZ_DEVICES.getOrElse(ui.device) { BRZ_DEVICES[0] }
    Column(modifier.fillMaxSize().background(CrundiColors.BgPrimary)) {
        Row(
            Modifier.fillMaxWidth().background(CrundiColors.BgSecondary).padding(horizontal = 8.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            BrzNav("chevron-left", "Back", ui.canBack, onBack)
            BrzNav("chevron-right", "Forward", ui.canForward, onForward)
            BrzNav("refresh", "Reload", true, onReload, spinning = ui.loading)
            UrlField(ui.field, onField, onGo, Modifier.weight(1f))
            Row(
                Modifier.height(36.dp).widthIn(max = 120.dp).clip(Dimens.ShapeSm)
                    .background(CrundiColors.BgTertiary).border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm)
                    .clickable(onClickLabel = "Emulate device") { picking = true }.padding(horizontal = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                CrundiIcon(if (dev.responsive) "maximize" else if (dev.mobile) "smartphone" else "server", size = 14.dp, tint = CrundiColors.TextSecondary)
                Text(if (dev.responsive) "Fill" else dev.name, style = CrundiType.Small.copy(fontSize = 12.sp), maxLines = 1, overflow = TextOverflow.Ellipsis)
                CrundiIcon("chevron-down", size = 12.dp, tint = CrundiColors.TextMuted)
            }
        }
        Box(Modifier.fillMaxWidth().height(Dimens.Hairline).background(CrundiColors.Border))
        Box(Modifier.weight(1f).fillMaxWidth().clip(RoundedCornerShape(0.dp))) {
            DottedStage()
            stage(dev)
            if (ui.hint != null) Text(ui.hint, style = CrundiType.Small.copy(color = CrundiColors.TextMuted), modifier = Modifier.align(Alignment.Center).padding(24.dp))
        }
    }
    if (picking) {
        CrundiDialog("Emulate device", onDismiss = { picking = false }) {
            LazyColumn(Modifier.height(420.dp)) {
                itemsIndexed(BRZ_DEVICES) { i, d ->
                    ListRow(
                        if (d.responsive) "Responsive (fill)" else d.name,
                        subtitle = if (d.responsive) "Fills the pane, this phone's own browser" else "${d.w}×${d.h}" + if (d.mobile) " · touch" else "",
                        icon = if (d.responsive) "maximize" else if (d.mobile) "smartphone" else "server",
                        active = i == ui.device,
                        onClick = { picking = false; onDevice(i) },
                    )
                }
            }
        }
    }
}

/** .brz-stage's dot grid: 1 px dots of --border every 16 px. */
@Composable
private fun DottedStage() {
    Canvas(Modifier.fillMaxSize()) {
        val step = 16.dp.toPx(); val r = 1.dp.toPx()
        var y = step / 2
        while (y < size.height) { var x = step / 2; while (x < size.width) { drawCircle(CrundiColors.Border, r, Offset(x, y)); x += step }; y += step }
    }
}

/** A .brz-nav square: dimmed when it cannot be used; the reload icon spins while a page loads. */
@Composable
private fun BrzNav(icon: String, label: String, enabled: Boolean, onClick: () -> Unit, spinning: Boolean = false) {
    val turn = if (spinning) {
        val t = rememberInfiniteTransition(label = "brz-spin")
        t.animateFloat(0f, 360f, infiniteRepeatable(tween(800, easing = LinearEasing), RepeatMode.Restart), label = "brz-spin").value
    } else 0f
    Box(
        Modifier.size(36.dp).clip(Dimens.ShapeSm)
            .background(CrundiColors.BgTertiary).border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm)
            .clickable(enabled = enabled, onClickLabel = label, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) { CrundiIcon(icon, Modifier.rotate(turn), size = 16.dp, tint = CrundiColors.TextSecondary.copy(alpha = if (enabled) 1f else 0.35f), contentDescription = label) }
}

/** .brz-url: a mono pill; Go on the keyboard opens what was typed. */
@Composable
private fun UrlField(value: String, onChange: (String) -> Unit, onGo: () -> Unit, modifier: Modifier) {
    var focused by remember { mutableStateOf(false) }
    val focus = LocalFocusManager.current
    val pill = RoundedCornerShape(999.dp)
    BasicTextField(
        value, onChange,
        singleLine = true,
        textStyle = CrundiType.Code.copy(fontSize = 13.sp, color = CrundiColors.TextPrimary),
        cursorBrush = SolidColor(CrundiColors.AccentHover),
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Go, autoCorrectEnabled = false),
        keyboardActions = KeyboardActions(onGo = { onGo(); focus.clearFocus() }),
        modifier = modifier.onFocusChanged { focused = it.isFocused },
        decorationBox = { inner ->
            Box(
                Modifier.fillMaxWidth().height(36.dp).background(CrundiColors.BgPrimary, pill)
                    .border(Dimens.Hairline, if (focused) CrundiColors.Accent else CrundiColors.Border, pill)
                    .padding(horizontal = 12.dp),
                contentAlignment = Alignment.CenterStart,
            ) {
                if (value.isEmpty()) Text("Enter URL or search…", style = CrundiType.Code.copy(fontSize = 13.sp, color = CrundiColors.TextMuted), maxLines = 1)
                inner()
            }
        },
    )
}

// ─── Live: the WebView ───

/** Where a browser keeps its page and device on this phone, by [id] (the pane's cell id, or "tab"). */
private class BrowserMemory(ctx: Context, private val id: String) {
    private val prefs = ctx.getSharedPreferences("crundi.browser", Context.MODE_PRIVATE)
    var url: String
        get() = prefs.getString("url.$id", null) ?: BRZ_START
        set(v) { prefs.edit().putString("url.$id", v).apply() }
    var device: Int
        get() = prefs.getInt("device.$id", 0).coerceIn(0, BRZ_DEVICES.size - 1)
        set(v) { prefs.edit().putInt("device.$id", v).apply() }
}

/**
 * The CSS width an emulated device gives its page (0 = the view's own, for
 * Responsive / Fill), and the script that sets it, kept per WebView.
 */
private val emulatedWidth = java.util.WeakHashMap<WebView, Int>()
private val viewportScript = java.util.WeakHashMap<WebView, androidx.webkit.ScriptHandler>()

/**
 * Give the page the device's width. The WebView is laid out at the size it
 * is shown at (scaled to the stage), and the page is told the device's width
 * through its viewport tag, so the browser zooms it to fit: the page lays out
 * at e.g. 393 wide and its screen is exactly the device frame. (Shrinking a
 * device-sized WebView with a view scale instead left part of it outside the
 * window, and Android then took only the visible part as the screen.)
 *
 * The tag is set before the page's own scripts run (a document-start script
 * that also rewrites the page's tag as it arrives), so the page loads at that
 * width from the start. Changing it on a page already shown kept the old
 * zoom (switching device left Google huge, and pinching could not undo it),
 * so a change of device loads the page again.
 */
private fun setViewport(web: WebView, width: Int) {
    viewportScript.remove(web)?.remove()
    if (width <= 0) return
    if (!androidx.webkit.WebViewFeature.isFeatureSupported(androidx.webkit.WebViewFeature.DOCUMENT_START_SCRIPT)) return
    val js = "(function(){var c='width=$width';" +
        "function fix(m){if(m.getAttribute('content')!==c)m.setAttribute('content',c);}" +
        "function ensure(){var ms=document.querySelectorAll('meta[name=viewport]');" +
        "if(!ms.length){var m=document.createElement('meta');m.name='viewport';(document.head||document.documentElement).appendChild(m);ms=[m];}" +
        "for(var i=0;i<ms.length;i++)fix(ms[i]);}" +
        "ensure();new MutationObserver(ensure).observe(document.documentElement,{childList:true,subtree:true,attributes:true,attributeFilter:['content']});})();"
    viewportScript[web] = androidx.webkit.WebViewCompat.addDocumentStartJavaScript(web, js, setOf("*"))
}

@SuppressLint("SetJavaScriptEnabled")
private fun newWebView(ctx: Context, onState: (WebView, String?, Boolean, String?) -> Unit): WebView = WebView(ctx).apply {
    settings.apply {
        javaScriptEnabled = true
        domStorageEnabled = true
        allowFileAccess = false
        allowContentAccess = false
        setSupportMultipleWindows(false)
        javaScriptCanOpenWindowsAutomatically = false
        useWideViewPort = true
        loadWithOverviewMode = true
        builtInZoomControls = true
        displayZoomControls = false
        mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
    }
    setBackgroundColor(android.graphics.Color.WHITE)
    webViewClient = object : WebViewClient() {
        override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
            val s = request.url.scheme?.lowercase()
            return s != "http" && s != "https" && s != "about"   // intent:, file:, mailto: ... stay out
        }
        override fun onPageStarted(view: WebView, url: String?, favicon: Bitmap?) = onState(view, url, true, null)
        override fun onPageFinished(view: WebView, url: String?) = onState(view, url, false, null)
        override fun doUpdateVisitedHistory(view: WebView, url: String?, isReload: Boolean) = onState(view, url, view.progress < 100, null)
        override fun onReceivedError(view: WebView, request: WebResourceRequest, error: android.webkit.WebResourceError) {
            if (request.isForMainFrame) onState(view, request.url.toString(), false, "Could not open this page: " + error.description)
        }
    }
    webChromeClient = object : WebChromeClient() {
        override fun onProgressChanged(view: WebView, newProgress: Int) = onState(view, view.url, newProgress < 100, null)
    }
}

/**
 * A live browser: the [BrowserScreen] with a real WebView on its stage.
 * [id] keys what it remembers (page, device). Back steps back in the page's
 * history when it can and the browser fills the screen.
 */
@Composable
fun LiveBrowser(id: String, modifier: Modifier = Modifier, startUrl: String? = null) {
    val ctx = LocalContext.current
    val memory = remember(id) { BrowserMemory(ctx, id) }
    var ui by remember(id) { mutableStateOf(BrowserUi(url = startUrl ?: memory.url, device = memory.device)) }
    var editing by remember { mutableStateOf(false) }
    val web = remember(id) {
        newWebView(ctx) { v, url, loading, error ->
            val u = url ?: ui.url
            if (!url.isNullOrEmpty() && url != "about:blank") memory.url = url
            ui = ui.copy(
                url = u, field = if (editing) ui.field else u,
                canBack = v.canGoBack(), canForward = v.canGoForward(), loading = loading, title = v.title.orEmpty(),
                hint = error ?: if (u.isEmpty() || u == "about:blank") ui.hint else null,
            )
        }.also { it.loadUrl(startUrl ?: memory.url) }
    }
    val holder = remember(id) { FrameLayout(ctx).apply { clipChildren = true; addView(web) } }
    DisposableEffect(id) { onDispose { holder.removeAllViews(); web.stopLoading(); web.destroy() } }
    var lastUa by remember { mutableStateOf<String?>(null) }
    val pane = LocalPane.current
    BackHandler(enabled = ui.canBack && (pane == null || pane.full)) { web.goBack() }
    BrowserScreen(
        ui,
        onField = { editing = true; ui = ui.copy(field = it) },
        onGo = { editing = false; brzTarget(ui.field)?.let { web.loadUrl(it); ui = ui.copy(url = it, field = it) } },
        onBack = { if (web.canGoBack()) web.goBack() },
        onForward = { if (web.canGoForward()) web.goForward() },
        onReload = { web.reload() },
        onDevice = { i -> memory.device = i; ui = ui.copy(device = i) },
        modifier = modifier,
    ) { dev ->
        BoxWithConstraints(Modifier.fillMaxSize()) {
            val density = LocalDensity.current
            val (scale, left, top) = brzFit(dev, maxWidth.value, maxHeight.value)
            // A device's own user agent; the phone's own for Responsive and the desktops without one.
            val ua = dev.ua.ifEmpty { null }
            if (ua != lastUa) {
                lastUa = ua
                web.settings.userAgentString = ua
                if (web.url != null) web.reload()
            }
            AndroidView(
                factory = { holder },
                modifier = Modifier.fillMaxSize(),
                update = {
                    val px = density.density
                    // Laid out at the size it is shown at; the page gets the
                    // device's width from setViewport, not from a view scale.
                    val lp = if (dev.responsive) FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
                    else FrameLayout.LayoutParams((dev.w * scale * px).toInt(), (dev.h * scale * px).toInt())
                    web.layoutParams = lp
                    web.scaleX = 1f; web.scaleY = 1f
                    web.translationX = left * px; web.translationY = top * px
                    val want = if (dev.responsive) 0 else dev.w.toInt()
                    if (emulatedWidth[web] != want) {
                        val first = emulatedWidth[web] == null
                        emulatedWidth[web] = want
                        setViewport(web, want)
                        // A fresh load, not reload(): a reload brings back the old zoom and scroll.
                        // The first time, only if a page is already there without it.
                        if (!first || want > 0) web.url?.let { web.loadUrl(it) }
                    }
                },
            )
        }
    }
}

/** A Browser pane in the workbench. [cell].data may give a start url. */
@Composable
fun BrowserPane(cell: WbCell?) = LiveBrowser("pane." + (cell?.id ?: "default"), startUrl = null)
