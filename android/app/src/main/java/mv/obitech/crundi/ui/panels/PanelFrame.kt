package mv.obitech.crundi.ui.panels

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Color as AColor
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.webkit.WebMessage
import android.webkit.WebMessagePort
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalInspectionMode
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.CrundiClient
import mv.obitech.crundi.api.CrundiJson
import mv.obitech.crundi.api.model.WidgetData
import mv.obitech.crundi.api.model.WidgetDoc
import mv.obitech.crundi.api.model.WidgetGrant
import mv.obitech.crundi.api.str
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.Banner
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.errorText
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.shell.LocalHost
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens
import java.io.ByteArrayInputStream

/*
 * One panel frame: the native host of a widget's HTML, as the web's
 * createView in app/vendor/crundi-widgets.js.
 *
 * The WebView exception (android-architecture.md): a WebView is used here and
 * only here, and only to draw the panel's own document. It is shut tight:
 *  - the panel runs in <iframe sandbox="allow-scripts" srcdoc> inside a tiny
 *    host page with an opaque origin, exactly as on the web;
 *  - the page talks to the app over one WebMessagePort, never a JavaScript
 *    interface (addJavascriptInterface would reach the panel's frame too);
 *  - no file or content access, no storage, no pop-ups, every navigation
 *    refused, and every network request answered 403 (the panel's CSP has
 *    connect-src 'none' anyway: it gets its data from the app, not the net).
 */

/** Rate limit for a frame's calls, as the web: 40 in 10 s. */
internal class CallLimiter(private val max: Int = 40, private val windowMs: Long = 10_000) {
    private val times = ArrayDeque<Long>()
    fun allow(now: Long): Boolean {
        while (times.isNotEmpty() && now - times.first() >= windowMs) times.removeFirst()
        if (times.size >= max) return false
        times.addLast(now)
        return true
    }
}

/** http(s) only: a panel must not open javascript:, data: or intent: links. */
internal fun safeLink(url: String): Boolean = Regex("^https?://", RegexOption.IGNORE_CASE).containsMatchIn(url.trim())

/** Where the host page says it is from: a name that can never resolve (RFC 2606 .invalid). */
internal const val HOST_ORIGIN = "https://panel.crundi.invalid"

/** The host page: a sandboxed frame and a relay between it and the app's port. */
internal const val HOST_PAGE = """<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<style>html,body{margin:0;padding:0;height:100%;background:transparent;overflow:hidden}iframe{border:0;width:100%;height:100%;display:block;background:transparent}</style></head>
<body><iframe id="f" sandbox="allow-scripts" allow="autoplay" referrerpolicy="no-referrer" title="Panel"></iframe><script>
(function(){var f=document.getElementById('f'),port=null,q=[];
function up(s){if(port)port.postMessage(s);else q.push(s);}
window.addEventListener('message',function(e){
 if(e.source===f.contentWindow){var m=e.data;if(!m||m.crundi!==1)return;var s;try{s=JSON.stringify(m);}catch(x){return;}up(s);return;}
 if(!port&&e.data==='crundi-port'&&e.ports&&e.ports.length===1){port=e.ports[0];
  port.onmessage=function(ev){var d;try{d=JSON.parse(ev.data);}catch(x){return;}
   if(d&&d.t==='doc'){f.srcdoc=String(d.html||'');return;}
   if(f.contentWindow){d.crundi=1;try{f.contentWindow.postMessage(d,'*');}catch(x){}}};
  for(var i=0;i<q.length;i++)port.postMessage(q[i]);q=[];}
});})();
</script></body></html>"""

/** The WebView and its port. Main thread only. */
@SuppressLint("SetJavaScriptEnabled")
internal class FrameHost(context: Context, private val onMessage: (JsonObject) -> Unit) {
    val web = WebView(context)
    private var port: WebMessagePort? = null
    private val pending = ArrayList<String>()
    private var destroyed = false

    init {
        web.setBackgroundColor(AColor.TRANSPARENT)
        web.isVerticalScrollBarEnabled = false
        web.settings.apply {
            javaScriptEnabled = true
            allowFileAccess = false
            allowContentAccess = false
            domStorageEnabled = false
            databaseEnabled = false
            javaScriptCanOpenWindowsAutomatically = false
            setSupportMultipleWindows(false)
            setGeolocationEnabled(false)
            mediaPlaybackRequiresUserGesture = false
            cacheMode = WebSettings.LOAD_NO_CACHE
            mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
            builtInZoomControls = false
        }
        web.webViewClient = object : WebViewClient() {
            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest) = true
            override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? {
                val scheme = request.url.scheme?.lowercase()
                // data: and about: stay inside the page; anything that would reach out is refused.
                if (scheme == "data" || scheme == "about" || scheme == "blob") return null
                return WebResourceResponse("text/plain", "utf-8", 403, "Forbidden", emptyMap(), ByteArrayInputStream(ByteArray(0)))
            }
            override fun onPageFinished(view: WebView, url: String?) { connect() }
        }
        // A real https origin (never fetched: the page is given here, and every
        // request is refused above). With a null base the page's origin is
        // opaque, and Android's message port did not reach it: the panel's
        // document never arrived and the pane stayed blank.
        web.loadDataWithBaseURL(HOST_ORIGIN, HOST_PAGE, "text/html", "utf-8", null)
    }

    private fun connect() {
        if (port != null || destroyed) return
        val (mine, theirs) = web.createWebMessageChannel()
        mine.setWebMessageCallback(object : WebMessagePort.WebMessageCallback() {
            override fun onMessage(p: WebMessagePort, message: WebMessage) {
                val obj = runCatching { CrundiJson.parseToJsonElement(message.data ?: "") as? JsonObject }.getOrNull() ?: return
                onMessage(obj)
            }
        }, Handler(Looper.getMainLooper()))
        web.postWebMessage(WebMessage("crundi-port", arrayOf(theirs)), Uri.parse(HOST_ORIGIN))
        port = mine
        pending.forEach { mine.postMessage(WebMessage(it)) }
        pending.clear()
    }

    fun send(msg: JsonObject) {
        if (destroyed) return
        val s = msg.toString()
        port?.postMessage(WebMessage(s)) ?: pending.add(s)
    }

    fun destroy() {
        if (destroyed) return
        destroyed = true
        runCatching { port?.close() }
        web.stopLoading()
        web.destroy()
    }
}

/** What a frame says about itself to its host's screen. */
data class FrameMeta(val title: String, val needsGrant: Boolean)

/**
 * A panel's frame. [frame] is what the panel is told it is in: cell, dock,
 * tab, chip, inline. [autoHeight] frames take the height the panel asks for,
 * between 36dp and [maxHeight]; the others fill what they are given.
 * [reloadKey] reloads the document when it changes (a Reload button).
 */
@Composable
fun PanelFrame(
    project: String,
    id: String,
    frame: String,
    modifier: Modifier = Modifier,
    chip: Boolean = false,
    autoHeight: Boolean = false,
    maxHeight: Dp = 520.dp,
    reloadKey: Int = 0,
    onMeta: (FrameMeta) -> Unit = {},
) {
    if (LocalInspectionMode.current) {
        // Screenshots run without a browser engine: draw a still stand-in where the panel goes.
        FrameStandIn(id, frame, modifier.then(if (autoHeight) Modifier.height(minOf(maxHeight, 140.dp)) else Modifier.fillMaxSize()))
        return
    }
    val api = LocalAppServices.current.api ?: return
    val host = LocalHost.current
    val ctx = LocalContext.current
    val density = LocalDensity.current
    val scope = rememberCoroutineScope()
    val meta by rememberUpdatedState(onMeta)

    var doc by remember(project, id) { mutableStateOf<WidgetDoc?>(null) }
    var error by remember(project, id) { mutableStateOf<String?>(null) }
    var grantHidden by remember(project, id) { mutableStateOf(false) }
    var reloads by remember(project, id) { mutableIntStateOf(0) }
    var heightPx by remember(project, id) { mutableIntStateOf(0) }
    var size by remember { mutableStateOf(0 to 0) }

    // Retry builds the frame again from nothing (a new page and port).
    var retries by remember(project, id) { mutableIntStateOf(0) }
    val state = remember(project, id, retries) { FrameState() }
    val fh = remember(project, id, retries) {
        lateinit var h: FrameHost
        h = FrameHost(ctx) { m -> state.onMessage(m, h, api, project, id, frame, chip, autoHeight, scope, host::openExternal, ctx, { size }) { heightPx = it } }
        h
    }
    DisposableEffect(fh) { onDispose { state.destroyed = true; fh.destroy() } }

    // The document: on open, on Reload, and when Claude saves a change (a doc event).
    LaunchedEffect(fh, reloadKey, reloads) {
        try {
            val d = api.widgets.doc(project, id, chip)
            state.started = false; state.missed = false; state.faults = 0
            doc = d; error = null; grantHidden = false
            d.widget?.let { meta(FrameMeta(it.title.ifEmpty { id }, d.grant.needed && !d.grant.granted)) }
            fh.send(buildJsonObject { put("t", "doc"); put("html", d.html) })
        } catch (e: ApiException) {
            error = errorText(e)
        }
    }
    // Live: new values, the store, a new document.
    LaunchedEffect(fh) {
        api.events.changes("widget").collect { ev ->
            val p = ev.project ?: return@collect
            if (p != "_global" && p != panelKey(project)) return@collect
            if (ev.id != id) return@collect
            when (ev.data.str("kind")) {
                "data" -> state.sendData(fh, api, project, id, ev.data.str("source"), scope)
                "store" -> if (state.started) runCatching { api.widgets.store(project, id) }.getOrNull()?.let { s -> fh.send(buildJsonObject { put("t", "store"); put("store", s) }) }
                "doc" -> reloads++
            }
        }
    }
    // Keep its sources running while it is on screen, as the web's keep-alive.
    LaunchedEffect(fh) {
        while (true) { runCatching { api.widgets.watch(project, listOf(id)) }; delay(25_000) }
    }
    // Tell the panel its new size after a resize settles.
    LaunchedEffect(size) {
        if (!state.started) return@LaunchedEffect
        delay(150)
        fh.send(buildJsonObject { put("t", "context"); put("context", frameContext(frame, size.first, size.second)) })
    }

    // A panel that has not said it is ready after a while: say so, with Retry,
    // rather than leave an empty pane.
    var stalled by remember(project, id) { mutableStateOf(false) }
    LaunchedEffect(fh, reloads, doc) {
        stalled = false
        if (doc == null) return@LaunchedEffect
        delay(8000)
        if (!state.started && !state.destroyed) stalled = true
    }
    Column(modifier) {
        error?.let { Banner(it, Modifier.fillMaxWidth().padding(8.dp), kind = BadgeKind.Bad, icon = "alert-triangle") }
        if (stalled && error == null) Row(Modifier.fillMaxWidth().padding(8.dp), verticalAlignment = Alignment.CenterVertically) {
            Banner("This panel did not start.", Modifier.weight(1f), kind = BadgeKind.Warn, icon = "alert-triangle")
            mv.obitech.crundi.ui.components.CrundiButton("Retry", { stalled = false; retries++ }, modifier = Modifier.padding(start = 8.dp))
        }
        val g = doc?.grant
        if (g != null && g.needed && !g.granted && !grantHidden) {
            GrantBanner(g, onApprove = {
                scope.launch {
                    try { api.widgets.grant(project, id, g.hash); toast(ctx, "Approved"); reloads++ }
                    catch (e: ApiException) { toast(ctx, "Could not approve: " + errorText(e)); reloads++ }
                }
            }, onLater = { grantHidden = true })
        }
        val hDp = with(density) { heightPx.toDp() }
        val frameMod = if (autoHeight) Modifier.fillMaxWidth().height(if (heightPx > 0) hDp.coerceIn(36.dp, maxHeight) else 80.dp.coerceAtMost(maxHeight))
        else Modifier.fillMaxSize()
        if (error == null) androidx.compose.runtime.key(fh) { AndroidView(
            // Its touches are its own (see ClaimTouchesFrame).
            factory = { c -> (fh.web.parent as? android.view.ViewGroup)?.removeView(fh.web); mv.obitech.crundi.ui.components.ClaimTouchesFrame(c).apply { addView(fh.web) } },
            modifier = frameMod.onSizeChanged { with(density) { size = it.width.toDp().value.toInt() to it.height.toDp().value.toInt() } },
        ) }
    }
}

/** Per-frame bookkeeping the message handler needs. */
internal class FrameState {
    var started = false
    var missed = false
    var faults = 0
    var destroyed = false
    val limiter = CallLimiter()

    fun sendData(fh: FrameHost, api: CrundiClient, project: String, id: String, source: String?, scope: kotlinx.coroutines.CoroutineScope) {
        if (destroyed) return
        // A change while the frame is still starting is remembered and caught up after init.
        if (!started) { missed = true; return }
        scope.launch {
            val d = runCatching { api.widgets.data(project, id, source) }.getOrNull() ?: return@launch
            for ((name, v) in d.data) fh.send(dataMessage(name, v.value, v.error))
        }
    }

    fun onMessage(
        m: JsonObject, fh: FrameHost, api: CrundiClient, project: String, id: String, frame: String, chip: Boolean, autoHeight: Boolean,
        scope: kotlinx.coroutines.CoroutineScope, openLink: (String) -> Unit, ctx: Context, size: () -> Pair<Int, Int>, onHeight: (Int) -> Unit,
    ) {
        if (destroyed) return
        when (m.str("t")) {
            "ready" -> scope.launch {
                val d = runCatching { api.widgets.data(project, id) }.getOrNull() ?: WidgetData()
                if (destroyed) return@launch
                started = true
                val (w, h) = size()
                fh.send(initMessage(d, frameContext(frame, w, h)))
                if (missed) { missed = false; sendData(fh, api, project, id, null, scope) }
                if (chip || autoHeight) { delay(200); fh.send(buildJsonObject { put("t", "measure") }) }
            }
            "height" -> {
                val h = (m["h"] as? JsonPrimitive)?.content?.toDoubleOrNull() ?: 0.0
                // The panel measures in CSS pixels, which are dp in this WebView.
                onHeight((h * ctx.resources.displayMetrics.density).toInt())
            }
            "fault" -> if (faults++ < 5) scope.launch { runCatching { api.widgets.fault(project, id, m.str("message").orEmpty(), m.str("where").orEmpty()) } }
            "call" -> {
                val seq = m["seq"] ?: JsonNull
                if (!limiter.allow(System.currentTimeMillis())) { fh.send(result(seq, false, error = "Too many requests; slow down.")); return }
                val op = m.str("op").orEmpty()
                val args = m["args"] as? JsonObject ?: JsonObject(emptyMap())
                when (op) {
                    "toast" -> { toast(ctx, args.str("text").orEmpty().take(200)); fh.send(result(seq, true, JsonPrimitive(true))) }
                    "openLink" -> {
                        val url = args.str("url").orEmpty()
                        if (!safeLink(url)) fh.send(result(seq, false, error = "Only http and https links can be opened"))
                        else { openLink(url); fh.send(result(seq, true, JsonPrimitive(true))) }
                    }
                    else -> scope.launch {
                        try { fh.send(result(seq, true, api.widgets.call(project, id, op, args))) }
                        catch (e: ApiException) { fh.send(result(seq, false, error = e.message ?: "Failed")) }
                    }
                }
            }
        }
    }
}

/** The panel's key for a project, as the server writes it in events ("_global" for none). */
internal fun panelKey(alias: String): String = alias.lowercase().replace(Regex("[^a-z0-9._-]"), "_").ifEmpty { "_global" }

internal fun frameContext(frame: String, w: Int, h: Int) = buildJsonObject {
    put("frame", frame); put("platform", "mobile"); put("touch", true); put("width", w); put("height", h)
}

internal fun initMessage(d: WidgetData, context: JsonObject) = buildJsonObject {
    put("t", "init")
    put("data", JsonObject(d.data.mapValues { (_, v) -> CrundiJson.encodeToJsonElement(mv.obitech.crundi.api.model.WidgetValue.serializer(), v) }))
    put("store", d.store ?: JsonObject(emptyMap()))
    put("context", context)
}

internal fun dataMessage(source: String, value: JsonElement?, error: String) = buildJsonObject {
    put("t", "data"); put("source", source); put("value", value ?: JsonNull); put("error", error)
}

internal fun result(seq: JsonElement, ok: Boolean, value: JsonElement? = null, error: String? = null) = buildJsonObject {
    put("t", "result"); put("seq", seq); put("ok", ok)
    if (ok) put("value", value ?: JsonNull) else put("error", error ?: "Failed")
}

private fun toast(ctx: Context, text: String) {
    if (text.isNotBlank()) android.widget.Toast.makeText(ctx.applicationContext, text, android.widget.Toast.LENGTH_SHORT).show()
}

/** "This panel is asking for more than it gets by default", with what it asks and Approve / Not now. */
@Composable
fun GrantBanner(g: WidgetGrant, onApprove: () -> Unit, onLater: () -> Unit, modifier: Modifier = Modifier) {
    Column(
        modifier.fillMaxWidth().background(CrundiColors.YellowDim).padding(horizontal = 12.dp, vertical = 10.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text("This panel is asking for more than it gets by default.", style = CrundiType.Small.copy(color = CrundiColors.TextPrimary))
        Text("It keeps doing these after the chat that built it is gone.", style = CrundiType.Small)
        for (it in g.items) Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(it.what, style = CrundiType.Small.copy(color = CrundiColors.TextPrimary))
            if (it.detail.isNotEmpty()) Text(it.detail, style = CrundiType.Meta.copy(color = CrundiColors.TextPrimary), maxLines = 2,
                modifier = Modifier.background(CrundiColors.BgPrimary.copy(alpha = 0.5f), Dimens.ShapeSm).padding(horizontal = 6.dp, vertical = 2.dp))
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiButton("Approve", onApprove, kind = ButtonKind.Primary, icon = "shield")
            CrundiButton("Not now", onLater)
        }
    }
}

/** What a screenshot shows where the panel's HTML would be. */
@Composable
private fun FrameStandIn(id: String, frame: String, modifier: Modifier) {
    Box(
        modifier.background(CrundiColors.BgPrimary).border(Dimens.Hairline, CrundiColors.BorderSubtle),
        contentAlignment = Alignment.Center,
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
            CrundiIcon("panel", size = 22.dp, tint = CrundiColors.AccentHover)
            Text(id, style = CrundiType.Meta)
            Text("panel HTML ($frame frame)", style = CrundiType.Meta.copy(color = CrundiColors.TextMuted))
        }
    }
}
