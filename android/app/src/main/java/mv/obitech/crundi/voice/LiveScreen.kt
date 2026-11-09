package mv.obitech.crundi.voice

import android.annotation.SuppressLint
import android.os.Handler
import android.os.Looper
import android.view.ViewGroup
import android.webkit.CookieManager
import android.webkit.JavascriptInterface
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalInspectionMode
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import mv.obitech.crundi.api.FrameMessage
import mv.obitech.crundi.api.KidStatus
import mv.obitech.crundi.api.LiveScreen
import mv.obitech.crundi.api.ScreenBlock
import mv.obitech.crundi.api.ScreenFilter
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType

/**
 * Cru's live screen on the voice screen (capability "voice-screen"), as
 * crundi-voice.js draws it: the title with an X, and under it Cru's blocks
 * drawn here in Crundi's style, or its page, game or video in a sandboxed
 * frame. In the voice view it takes the field's place; in the chat view it
 * sits above the messages and folds away ([chat]).
 */
@Composable
fun LivePanel(
    screen: LiveScreen,
    volume: Float,
    command: GameCommand?,
    actions: LiveActions,
    chat: Boolean,
    modifier: Modifier = Modifier,
    /** The X: not while kid mode is on (it would take the child's screen away). */
    closable: Boolean = true,
    /** The server's address, as the video player's referrer (YouTube refuses to play without one). */
    referer: String = "",
) {
    var folded by rememberSaveable { mutableStateOf(false) }
    val showBody = !(chat && folded)
    Column(modifier.padding(top = 8.dp)) {
        Row(Modifier.fillMaxWidth().padding(start = 2.dp, bottom = 6.dp), verticalAlignment = Alignment.CenterVertically) {
            CrundiIcon("panel", size = 14.dp, tint = CrundiColors.AccentHover)
            Text(
                screen.title.ifEmpty { "Cru's screen" },
                style = CrundiType.Body.copy(fontWeight = FontWeight.SemiBold, color = CrundiColors.TextPrimary),
                maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f).padding(start = 6.dp),
            )
            if (chat) CrundiIconButton(if (folded) "chevron-down" else "chevron-up", { folded = !folded }, contentDescription = if (folded) "Show the live screen" else "Fold the live screen", size = 15.dp, touch = 34.dp)
            if (closable) CrundiIconButton("x", actions.onClose, contentDescription = "Close the live screen", size = 15.dp, touch = 34.dp)
        }
        if (!showBody) return@Column
        val shape = RoundedCornerShape(14.dp)
        val box = Modifier.fillMaxWidth().clip(shape).background(CrundiColors.BgCard).border(1.dp, CrundiColors.Border, shape)
        if (screen.framed) {
            Column(box.then(if (chat) Modifier.height(300.dp) else Modifier.weight(1f, fill = true).heightIn(min = 220.dp))) {
                // A page with blocks too: the blocks above the frame.
                if (screen.blocks.isNotEmpty() && screen.videoUrl == null) Column(
                    Modifier.fillMaxWidth().heightIn(max = 160.dp).verticalScroll(rememberScrollState()).padding(horizontal = 12.dp, vertical = 10.dp),
                    verticalArrangement = Arrangement.spacedBy(10.dp),
                ) { Blocks(screen.blocks) }
                ScreenFrame(screen, volume, command, actions.onFrame, referer, Modifier.fillMaxWidth().weight(1f))
            }
        } else {
            Column(
                box.then(if (chat) Modifier.heightIn(max = 300.dp) else Modifier.weight(1f, fill = false))
                    .verticalScroll(rememberScrollState()).padding(horizontal = 14.dp, vertical = 12.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) { Blocks(screen.blocks) }
        }
    }
}

/** Cru's blocks, every text as text (nothing is read as markup). */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun Blocks(blocks: List<ScreenBlock>) {
    for (b in blocks) when (b) {
        is ScreenBlock.Heading -> Text(b.text, style = CrundiType.Heading.copy(fontSize = 16.sp))
        is ScreenBlock.Text -> Text(b.text, style = CrundiType.Body.copy(color = CrundiColors.TextPrimary))
        is ScreenBlock.Items -> Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
            b.items.forEachIndexed { i, t ->
                Row {
                    Text(if (b.ordered) "${i + 1}." else "•", style = CrundiType.Body.copy(color = CrundiColors.TextSecondary), modifier = Modifier.width(22.dp))
                    Text(t, style = CrundiType.Body.copy(color = CrundiColors.TextPrimary), modifier = Modifier.weight(1f))
                }
            }
        }
        is ScreenBlock.Checklist -> Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            for (c in b.items) Row(verticalAlignment = Alignment.Top) {
                Box(
                    Modifier.padding(top = 3.dp).size(14.dp).clip(RoundedCornerShape(4.dp))
                        .background(if (c.done) CrundiColors.Green else Color.Transparent)
                        .border(1.5.dp, if (c.done) CrundiColors.Green else CrundiColors.TextMuted, RoundedCornerShape(4.dp)),
                    contentAlignment = Alignment.Center,
                ) { if (c.done) CrundiIcon("check", size = 10.dp, tint = Color.White) }
                Text(
                    c.text,
                    style = CrundiType.Body.copy(color = if (c.done) CrundiColors.TextSecondary else CrundiColors.TextPrimary, textDecoration = if (c.done) TextDecoration.LineThrough else null),
                    modifier = Modifier.weight(1f).padding(start = 8.dp),
                )
            }
        }
        is ScreenBlock.Table -> Column {
            if (b.headers.isNotEmpty()) TableRow(b.headers, head = true)
            for (r in b.rows) TableRow(r, head = false)
        }
        is ScreenBlock.Progress -> Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text((if (b.label.isNotEmpty()) b.label + " " else "") + fmtPercent(b.value), style = CrundiType.Small.copy(fontSize = 12.sp))
            Box(Modifier.fillMaxWidth().height(8.dp).clip(RoundedCornerShape(4.dp)).background(CrundiColors.BgTertiary)) {
                Box(Modifier.fillMaxWidth((b.value / 100.0).toFloat().coerceIn(0f, 1f)).height(8.dp).clip(RoundedCornerShape(4.dp)).background(androidx.compose.ui.graphics.Brush.horizontalGradient(listOf(CrundiColors.Sky, CrundiColors.Accent))))
            }
        }
        is ScreenBlock.Image -> ScreenImage(b.url, b.alt)
        is ScreenBlock.Chips -> FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            for (c in b.items) {
                val col = toneColour(c.tone)
                Text(
                    c.text, style = CrundiType.Small.copy(fontSize = 12.sp, color = col),
                    modifier = Modifier.clip(RoundedCornerShape(99.dp)).background(col.copy(alpha = 0.12f)).border(1.dp, col.copy(alpha = 0.45f), RoundedCornerShape(99.dp)).padding(horizontal = 10.dp, vertical = 2.dp),
                )
            }
        }
    }
}

@Composable
private fun TableRow(cells: List<String>, head: Boolean) {
    Column {
        Row(Modifier.fillMaxWidth().padding(vertical = 4.dp)) {
            cells.forEachIndexed { i, c ->
                Text(
                    c, style = CrundiType.Small.copy(fontSize = 13.sp, color = if (head || i == 0) CrundiColors.TextSecondary else CrundiColors.TextPrimary, fontWeight = if (head) FontWeight.Medium else null),
                    modifier = Modifier.weight(1f).padding(end = 8.dp),
                )
            }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(CrundiColors.Border))
    }
}

/** ok green, warn yellow, bad red, info sky, muted grey (crundi-voice.js TONE_COLOURS). */
fun toneColour(tone: String): Color = when (tone) {
    "ok" -> CrundiColors.Green
    "warn" -> CrundiColors.Yellow
    "bad" -> CrundiColors.Red
    "muted" -> CrundiColors.TextMuted
    else -> CrundiColors.Sky
}

private fun fmtPercent(v: Double): String = (if (v == Math.floor(v)) v.toLong().toString() else "%.1f".format(java.util.Locale.US, v)) + "%"

/** A picture: data:image decoded here; https fetched without the phone's sign-in, cookies or referrer. */
@Composable
private fun ScreenImage(url: String, alt: String) {
    val inspecting = LocalInspectionMode.current
    val img by produceState<ImageBitmap?>(null, url) {
        if (inspecting) return@produceState
        value = withContext(Dispatchers.IO) { runCatching { LiveImages.load(url) }.getOrNull() }
    }
    val bmp = img
    if (bmp != null) Image(bmp, contentDescription = alt.ifEmpty { null }, contentScale = ContentScale.FillWidth, modifier = Modifier.fillMaxWidth().heightIn(max = 360.dp).clip(RoundedCornerShape(10.dp)))
    else if (alt.isNotEmpty()) Text(alt, style = CrundiType.Small.copy(color = CrundiColors.TextMuted))
}

internal object LiveImages {
    private const val MAX = 4 * 1024 * 1024

    fun load(url: String): ImageBitmap? {
        val bytes = if (url.startsWith("data:", true)) {
            val comma = url.indexOf(',')
            if (comma < 0 || !url.substring(0, comma).contains(";base64", true)) return null
            java.util.Base64.getDecoder().decode(url.substring(comma + 1).trim())
        } else {
            if (!url.startsWith("https://", true)) return null
            val c = java.net.URL(url).openConnection() as java.net.HttpURLConnection
            try {
                c.connectTimeout = 8000; c.readTimeout = 12000; c.instanceFollowRedirects = true
                c.setRequestProperty("Accept", "image/*")
                if (c.responseCode != 200) return null
                val out = java.io.ByteArrayOutputStream()
                c.inputStream.use { ins ->
                    val buf = ByteArray(16 * 1024)
                    while (true) { val n = ins.read(buf); if (n < 0) break; out.write(buf, 0, n); if (out.size() > MAX) return null }
                }
                out.toByteArray()
            } finally { c.disconnect() }
        }
        return android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.size)?.asImageBitmap()
    }
}

/**
 * Cru's page, a game or a video, in its own WebView: JavaScript on and
 * nothing else. No file or content access, no storage for a page, no new
 * windows, never a navigation; every request but data:/blob: (and, for a
 * video, youtube-nocookie's player) is answered 403 here, on top of the
 * page's own no-network CSP. The page reaches the app only through the
 * bridge's one call: what it posts to window.parent (cru.say, cru.tell, a
 * game's events), read by [ScreenFilter.message].
 */
@Composable
private fun ScreenFrame(screen: LiveScreen, volume: Float, command: GameCommand?, onMessage: (FrameMessage) -> Unit, referer: String, modifier: Modifier) {
    if (LocalInspectionMode.current) {
        Box(modifier.background(Color(0xFF0F0F14)), contentAlignment = Alignment.Center) { CrundiIcon(if (screen.videoUrl != null) "film" else "panel", size = 28.dp, tint = CrundiColors.TextMuted) }
        return
    }
    val video = screen.videoUrl
    val doc = screen.doc
    val send by rememberUpdatedState(onMessage)
    val frame = remember { arrayOfNulls<WebView>(1) }
    var loaded by remember { mutableStateOf(false) }
    // The game's sounds follow Cru's volume.
    LaunchedEffect(volume, loaded) { if (loaded && video == null) frame[0]?.evaluateJavascript(ScreenFrames.volumeJs(volume), null) }
    // Kid Cru's command for the game.
    LaunchedEffect(command?.seq) { if (command != null && video == null) frame[0]?.evaluateJavascript(ScreenFrames.commandJs(command.action), null) }
    key(video ?: doc) {
        AndroidView(
            factory = { c ->
                loaded = false
                ScreenFrames.make(c, video != null, onLoaded = { loaded = true }) { m -> send(m) }.also { wv ->
                    frame[0] = wv
                    if (video != null) wv.loadUrl(video, if (referer.isNotEmpty()) mapOf("Referer" to referer.trimEnd('/') + "/") else emptyMap())
                    else wv.loadDataWithBaseURL(null, ScreenFrames.withBridge(doc), "text/html", "utf-8", null)
                }
            },
            onRelease = { wv -> if (frame[0] === wv) frame[0] = null; ScreenFrames.destroy(wv) },
            modifier = modifier.clip(RoundedCornerShape(14.dp)),
        )
    }
}

/** The frame's WebView and the scripts it is given (the pure parts are tested in ScreenFilterTest). */
object ScreenFrames {
    /** The bridge's name on window: one call, post(json). */
    const val BRIDGE = "CruBridge"

    /**
     * Read before the page's own scripts: what the page posts to its parent
     * (itself, in a WebView) goes to the bridge; what this side posts into it
     * (volume, commands) does not come back.
     */
    const val BRIDGE_JS = "<script>(function(){var B=window.$BRIDGE;if(!B)return;" +
        "window.addEventListener('message',function(e){if(e.source!==window)return;var d=e.data;if(!d||typeof d!=='object')return;" +
        "if(d.cru==='volume'||d.type==='command')return;try{B.post(JSON.stringify(d))}catch(x){}});})();</script>"

    /** [doc] with the bridge right after its <head> (the server's frame document always has one). */
    fun withBridge(doc: String): String {
        val m = Regex("<head\\b[^>]*>", RegexOption.IGNORE_CASE).find(doc) ?: return BRIDGE_JS + doc
        return doc.substring(0, m.range.last + 1) + BRIDGE_JS + doc.substring(m.range.last + 1)
    }

    /** { cru: 'volume', value, muted } into the page, as the web frame gets it. */
    fun volumeJs(volume: Float): String = "window.postMessage({cru:'volume',value:${"%.2f".format(java.util.Locale.US, volume.coerceIn(0f, 1f))},muted:false},'*')"

    /** { type: 'command', action } into the page. */
    fun commandJs(action: String): String = "window.postMessage({type:'command',action:" + kotlinx.serialization.json.JsonPrimitive(action.take(40)).toString() + "},'*')"

    @SuppressLint("SetJavaScriptEnabled", "JavascriptInterface", "AddJavascriptInterface")
    fun make(c: android.content.Context, video: Boolean, onLoaded: () -> Unit, onMessage: (FrameMessage) -> Unit): WebView = WebView(c).apply {
        setBackgroundColor(android.graphics.Color.TRANSPARENT)
        layoutParams = ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
        settings.apply {
            javaScriptEnabled = true
            // The player keeps its state in storage; a page or game has none.
            domStorageEnabled = video
            databaseEnabled = false
            allowFileAccess = false
            allowContentAccess = false
            @Suppress("DEPRECATION") allowFileAccessFromFileURLs = false
            @Suppress("DEPRECATION") allowUniversalAccessFromFileURLs = false
            setGeolocationEnabled(false)
            setSupportMultipleWindows(false)
            javaScriptCanOpenWindowsAutomatically = false
            // A game's sounds play at once (the web frame has allow="autoplay"); a video waits for a tap.
            mediaPlaybackRequiresUserGesture = video
            mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
            cacheMode = if (video) WebSettings.LOAD_DEFAULT else WebSettings.LOAD_NO_CACHE
            setSupportZoom(false)
            loadWithOverviewMode = false
            useWideViewPort = false
        }
        CookieManager.getInstance().setAcceptThirdPartyCookies(this, false)
        if (!video) {
            val main = Handler(Looper.getMainLooper())
            addJavascriptInterface(object {
                @JavascriptInterface fun post(json: String) {
                    val m = ScreenFilter.message(json.take(4000)) ?: return
                    main.post { onMessage(m) }
                }
            }, BRIDGE)
        }
        webViewClient = object : WebViewClient() {
            override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? =
                if (ScreenFilter.allows(request.url.toString(), video)) null else blocked()
            // Never a navigation away: a link, a form, a script setting location.
            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean =
                !(video && !request.hasGesture() && ScreenFilter.allows(request.url.toString(), true))
            override fun onPageFinished(view: WebView, url: String?) { onLoaded() }
        }
        webChromeClient = WebChromeClient()
    }

    private fun blocked() = WebResourceResponse("text/plain", "utf-8", 403, "Blocked", mapOf("Cache-Control" to "no-store"), java.io.ByteArrayInputStream(ByteArray(0)))

    fun destroy(wv: WebView) {
        runCatching {
            wv.stopLoading()
            wv.removeJavascriptInterface(BRIDGE)
            wv.loadUrl("about:blank")
            (wv.parent as? ViewGroup)?.removeView(wv)
            wv.destroy()
        }
    }
}

/**
 * Kid mode is on (it runs on the PC): the phone's voice screen says so, and
 * the parent can pause it, add time or end it from here. The kid screen
 * itself is not drawn on the phone.
 */
@Composable
fun KidOnNote(kid: KidStatus, onKid: (String) -> Unit, modifier: Modifier = Modifier) {
    var confirmEnd by remember { mutableStateOf(false) }
    val now by produceState(System.currentTimeMillis()) { while (true) { kotlinx.coroutines.delay(30_000); value = System.currentTimeMillis() } }
    Column(
        modifier.fillMaxWidth().padding(top = 8.dp).clip(RoundedCornerShape(12.dp)).background(CrundiColors.YellowDim)
            .border(1.dp, CrundiColors.Yellow.copy(alpha = 0.4f), RoundedCornerShape(12.dp)).padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(8.dp).clip(CircleShape).background(if (kid.paused) CrundiColors.TextMuted else CrundiColors.Yellow))
            Text("Kid mode is on (on the PC)", style = CrundiType.Body.copy(fontWeight = FontWeight.SemiBold, color = CrundiColors.TextPrimary), modifier = Modifier.padding(start = 8.dp))
        }
        Text(kidLine(kid, now), style = CrundiType.Small)
        KidButtons(kid, confirmEnd, onConfirm = { confirmEnd = it }, onKid = onKid)
    }
}

/** "12 min, +15 added · Letters · paused". */
fun kidLine(kid: KidStatus, now: Long): String =
    listOfNotNull(
        "${kid.minutes(now)} min" + (if (kid.extraMinutes > 0) ", +${kid.extraMinutes} added" else ""),
        kid.playing.takeIf { it.isNotBlank() },
        if (kid.paused) "paused" else null,
    ).joinToString(" · ")

/** Pause or Resume, Add 15 min, End kid mode (asks once more first). */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun KidButtons(kid: KidStatus, confirmEnd: Boolean, onConfirm: (Boolean) -> Unit, onKid: (String) -> Unit) {
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (confirmEnd) {
            Text("End kid mode on the kid screen now?", style = CrundiType.Small.copy(color = CrundiColors.TextPrimary), modifier = Modifier.align(Alignment.CenterVertically))
            CrundiButton("End kid mode", { onConfirm(false); onKid("end") }, kind = ButtonKind.Danger, icon = "stop")
            CrundiButton("Cancel", { onConfirm(false) }, kind = ButtonKind.Ghost)
        } else {
            CrundiButton(if (kid.paused) "Resume" else "Pause", { onKid(if (kid.paused) "resume" else "pause") }, icon = if (kid.paused) "play" else "pause")
            CrundiButton("Add 15 min", { onKid("time") }, icon = "clock")
            CrundiButton("End kid mode", { onConfirm(true) }, kind = ButtonKind.Danger)
        }
    }
}
