package mv.obitech.crundi.ui.operations

import android.annotation.SuppressLint
import android.view.View
import android.view.ViewGroup
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalInspectionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import kotlinx.coroutines.launch
import mv.obitech.crundi.api.model.WhatsNew
import mv.obitech.crundi.api.model.WhatsNewStep
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.shell.AppServices
import mv.obitech.crundi.ui.shell.CrundiMark
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType

/*
 * What's new: the tour of what the latest update added, as the web's
 * app/vendor/crundi-whatsnew.js draws it on a phone. The animation on top in
 * a little window, the words under it, the buttons at the bottom within reach
 * of a thumb; swiping moves between steps; a recap at the end.
 *
 * The server decides what there is to show (GET /api/whatsnew, src/whatsnew.js)
 * and keeps what has been seen, shared with the web: seen here is seen there.
 */

/** How the tour was closed. Both mark everything seen, unless it was opened from Settings. */
enum class WhatsNewClose { Skip, Done }

/**
 * The tour for one server: whether it is open, on which page, and what
 * closing it does. Kept for as long as the process lives (one per
 * [AppServices]), so it is offered once per launch per server and survives
 * the window being made again.
 */
class WhatsNewTour(
    private val fetch: suspend (all: Boolean) -> WhatsNew,
    private val markSeen: suspend () -> Unit,
) {
    /** What is on screen; [all] is "Show it again" from Settings, which marks nothing seen. */
    data class Open(val data: WhatsNew, val all: Boolean)

    var open by mutableStateOf<Open?>(null)
        private set
    /** The page: 0 until the number of steps; the last page is the recap. */
    var at by mutableIntStateOf(0)
        private set
    /** It has been offered on this launch (a reply came, with steps or without). */
    var offered = false
        private set

    val pages: Int get() = (open?.data?.steps?.size ?: 0) + 1

    /**
     * Once per launch, after the handshake: show the tour if the server has
     * steps for this person. An error, an older server without the route, or
     * nothing new: nothing is shown and nothing is said. A failed ask is
     * tried again the next time the window opens.
     */
    suspend fun autoShow(): Boolean {
        if (offered || open != null) return false
        val d = try { fetch(false) } catch (e: kotlinx.coroutines.CancellationException) { throw e } catch (e: Exception) { return false }
        offered = true
        if (d.steps.isEmpty() || open != null) return false
        show(Open(d, all = false))
        return true
    }

    /** Settings: the latest edition again. False when there is nothing to show (or the server could not say). */
    suspend fun showAgain(): Boolean {
        val d = try { fetch(true) } catch (e: kotlinx.coroutines.CancellationException) { throw e } catch (e: Exception) { return false }
        if (d.steps.isEmpty()) return false
        show(Open(d, all = true))
        return true
    }

    private fun show(o: Open) { at = 0; open = o }

    fun go(i: Int) { if (open != null) at = i.coerceIn(0, pages - 1) }
    fun next() = go(at + 1)
    fun back() = go(at - 1)

    /** Close it. Skip and Done mark everything seen on the server, except after "Show it again". */
    suspend fun close(@Suppress("UNUSED_PARAMETER") how: WhatsNewClose) {
        val o = open ?: return
        open = null
        if (!o.all) runCatching { markSeen() }
    }

    companion object {
        private val tours = java.util.WeakHashMap<AppServices, WhatsNewTour>()

        /** The tour of this server's services, made once per process. */
        fun of(services: AppServices): WhatsNewTour = synchronized(tours) {
            tours.getOrPut(services) { WhatsNewTour({ services.api.system.whatsNew(it) }, { services.api.system.whatsNewSeen() }) }
        }
    }
}

/** The tour of the server on screen, for Settings' "Show it again". Null outside the app (tests, the bubble). */
val LocalWhatsNew = staticCompositionLocalOf<WhatsNewTour?> { null }

// ─── The animation's page ───

/**
 * The page a step's animation is shown in. The fragment is the server's own
 * file, drawn for a 640 x 400 stage with the app's CSS variables; it is
 * wrapped here with those variables set to this app's colours and scaled to
 * the width it is given. Markup and styles only: any script is taken out,
 * scripts are off in the WebView and the page's policy allows none.
 */
object WhatsNewArt {
    const val STAGE_W = 640
    const val STAGE_H = 400

    private fun css(c: Color): String {
        val a = (c.toArgb() ushr 24) and 0xFF
        val r = (c.toArgb() shr 16) and 0xFF
        val g = (c.toArgb() shr 8) and 0xFF
        val b = c.toArgb() and 0xFF
        return if (a == 0xFF) "#%02x%02x%02x".format(r, g, b) else "rgba($r, $g, $b, ${"%.2f".format(java.util.Locale.ROOT, a / 255f)})"
    }

    /** The CSS variables the animations use (README: "Use the app's colours"), from the app's theme. */
    fun themeVars(): Map<String, String> = linkedMapOf(
        "--bg-primary" to css(CrundiColors.BgPrimary),
        "--bg-secondary" to css(CrundiColors.BgSecondary),
        "--bg-tertiary" to css(CrundiColors.BgTertiary),
        "--bg-hover" to css(CrundiColors.BgHover),
        "--bg-card" to css(CrundiColors.BgCard),
        "--border" to css(CrundiColors.Border),
        "--border-subtle" to css(CrundiColors.BorderSubtle),
        "--text-primary" to css(CrundiColors.TextPrimary),
        "--text-secondary" to css(CrundiColors.TextSecondary),
        "--text-muted" to css(CrundiColors.TextMuted),
        "--accent" to css(CrundiColors.Accent),
        "--accent-hover" to css(CrundiColors.AccentHover),
        "--accent-deep" to css(CrundiColors.AccentDeep),
        "--accent-dim" to css(CrundiColors.AccentDim),
        "--amber" to css(CrundiColors.Amber),
        "--green" to css(CrundiColors.Green),
        "--green-dim" to css(CrundiColors.GreenDim),
        "--red" to css(CrundiColors.Red),
        "--red-dim" to css(CrundiColors.RedDim),
        "--yellow" to css(CrundiColors.Yellow),
        "--yellow-dim" to css(CrundiColors.YellowDim),
        "--sky" to css(CrundiColors.Sky),
        "--mono" to "\"Cascadia Code\", \"Fira Code\", \"JetBrains Mono\", \"Roboto Mono\", monospace",
    )

    private val SCRIPT = Regex("<script\\b[\\s\\S]*?(</script\\s*>|$)", RegexOption.IGNORE_CASE)
    private val ON_ATTR = Regex("\\s on[a-z]+\\s*=\\s*(\"[^\"]*\"|'[^']*'|[^\\s>]+)".replace(" ", ""), RegexOption.IGNORE_CASE)

    /** The fragment as shown: scripts and event attributes taken out (the tests refuse them on the server too). */
    fun clean(art: String): String = art.replace(SCRIPT, "").replace(ON_ATTR, "")

    /** The whole page for [art], its stage scaled by [scale] (the box's width in CSS pixels over 640). */
    fun page(art: String, scale: Float, vars: Map<String, String> = themeVars()): String {
        val s = "%.4f".format(java.util.Locale.ROOT, scale.coerceIn(0.05f, 8f))
        val root = vars.entries.joinToString(";") { "${it.key}:${it.value}" }
        return "<!doctype html><html><head><meta charset=\"utf-8\">" +
            "<meta name=\"viewport\" content=\"width=device-width,initial-scale=1,maximum-scale=1,user-scalable=no\">" +
            "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:\">" +
            "<style>:root{$root}" +
            "html,body{margin:0;padding:0;width:100%;height:100%;overflow:hidden;background:var(--bg-secondary);color:var(--text-primary);" +
            "font-family:system-ui,-apple-system,Roboto,sans-serif;-webkit-user-select:none;user-select:none;-webkit-tap-highlight-color:transparent}" +
            ".wn-stage{position:absolute;left:0;top:0;width:${STAGE_W}px;height:${STAGE_H}px;transform-origin:0 0;transform:scale($s);overflow:hidden}" +
            ".wn-stage .wna{position:absolute;inset:0;overflow:hidden}" +
            "@media (prefers-reduced-motion:reduce){*{animation-play-state:paused!important}}" +
            "</style></head><body><div class=\"wn-stage\">" + clean(art) + "</div></body></html>"
    }
}

// ─── On screen ───

/**
 * The tour over the whole app while [tour] is open. Back is Skip, as Escape
 * is on the web.
 */
@Composable
fun WhatsNewOverlay(tour: WhatsNewTour) {
    val o = tour.open ?: return
    val scope = rememberCoroutineScope()
    val close: (WhatsNewClose) -> Unit = { how -> scope.launch { tour.close(how) } }
    BackHandler { close(WhatsNewClose.Skip) }
    WhatsNewScreen(o.data, tour.at, onGo = tour::go, onClose = close)
}

private val StageBar = 34.dp
/** The web's body grey (#b9b9cc), between text-primary and text-secondary. */
private val BodyText = CrundiColors.TextPrimary.copy(alpha = 0.8f)

/**
 * The tour as drawn: [data] from the server, on page [at] (the last page is
 * the recap). Stateless, for screenshot tests.
 */
@Composable
fun WhatsNewScreen(data: WhatsNew, at: Int, onGo: (Int) -> Unit = {}, onClose: (WhatsNewClose) -> Unit = {}) {
    val steps = data.steps
    val n = steps.size + 1
    val page = at.coerceIn(0, n - 1)
    val last = page == n - 1
    val step = steps.getOrNull(page)
    val swipe = with(LocalDensity.current) { 55.dp.toPx() }
    Box(
        Modifier
            .fillMaxSize()
            .background(CrundiColors.BgPrimary)
            .background(Brush.radialGradient(listOf(CrundiColors.Accent.copy(alpha = 0.14f), Color.Transparent), radius = 1400f))
            // Nothing under the tour gets a touch while it is up.
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, onClick = {})
            .pointerInput(page, n) {
                var dx = 0f
                detectHorizontalDragGestures(
                    onDragStart = { dx = 0f },
                    onDragEnd = { swipeTarget(page, n, dx, swipe).let { if (it != page) onGo(it) } },
                    onHorizontalDrag = { change, amount -> dx += amount; change.consume() },
                )
            },
    ) {
        Column(Modifier.fillMaxSize()) {
            TourTop(data.version, showSkip = !last, onSkip = { onClose(WhatsNewClose.Skip) })
            // The little window: the step's animation, or the recap's list.
            Box(Modifier.fillMaxWidth().padding(start = 14.dp, end = 14.dp, top = 4.dp), contentAlignment = Alignment.TopCenter) {
                TourFrame(
                    title = if (last) "In this update" else step?.artTitle?.ifEmpty { step.tag }.orEmpty(),
                    modifier = Modifier.widthIn(max = 640.dp).fillMaxWidth(),
                ) {
                    if (last) Recap(steps, onGo, Modifier.fillMaxWidth().aspectRatio(WhatsNewArt.STAGE_W.toFloat() / WhatsNewArt.STAGE_H))
                    else ArtStage(step?.art.orEmpty(), Modifier.fillMaxWidth().aspectRatio(WhatsNewArt.STAGE_W.toFloat() / WhatsNewArt.STAGE_H))
                }
            }
            AnimatedContent(
                targetState = page,
                transitionSpec = { (fadeIn(tween(320)) + slideInVertically(tween(320)) { it / 30 }) togetherWith fadeOut(tween(120)) },
                label = "whatsnew-copy",
                modifier = Modifier.weight(1f).fillMaxWidth(),
            ) { p ->
                Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(start = 22.dp, end = 22.dp, top = 16.dp, bottom = 12.dp)) {
                    val s = steps.getOrNull(p)
                    if (s == null) {
                        Eyebrow("That is everything", null)
                        Title("You are up to date")
                        Body("This shows once after each update. You can open it again any time from Settings, under What's new.")
                    } else {
                        Eyebrow(s.tag.ifEmpty { "New" }, s.version.takeIf { it.isNotEmpty() }?.let { "new in $it" })
                        Title(s.title)
                        Body(s.body)
                        if (s.keys.isNotEmpty()) Keys(s)
                    }
                }
            }
            TourFoot(page, n, onGo = onGo, onDone = { onClose(WhatsNewClose.Done) })
        }
    }
}

@Composable
private fun TourTop(version: String, showSkip: Boolean, onSkip: () -> Unit) {
    Row(Modifier.fillMaxWidth().padding(start = 14.dp, end = 14.dp, top = 12.dp, bottom = 10.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(9.dp)) {
        CrundiMark(size = 26.dp)
        Text("Crundi", style = CrundiType.Heading.copy(fontWeight = FontWeight.Bold))
        if (version.isNotEmpty()) Text(
            version,
            style = CrundiType.Meta.copy(color = CrundiColors.AccentHover, fontWeight = FontWeight.SemiBold, fontSize = 11.5.sp),
            modifier = Modifier.clip(RoundedCornerShape(99.dp)).background(CrundiColors.AccentDim)
                .border(1.dp, CrundiColors.Accent.copy(alpha = 0.35f), RoundedCornerShape(99.dp)).padding(horizontal = 8.dp, vertical = 3.dp),
        )
        Spacer(Modifier.weight(1f))
        // Hidden on the recap, keeping its room so the bar does not jump.
        Box(Modifier.alpha(if (showSkip) 1f else 0f)) { PillButton("Skip", "x", onClick = { if (showSkip) onSkip() }) }
    }
}

@Composable
private fun PillButton(text: String, icon: String, onClick: () -> Unit) {
    val src = remember { MutableInteractionSource() }
    val pressed by src.collectIsPressedAsState()
    Row(
        Modifier.clip(RoundedCornerShape(99.dp))
            .border(1.dp, if (pressed) CrundiColors.Accent else CrundiColors.Border, RoundedCornerShape(99.dp))
            .clickable(interactionSource = src, indication = null, role = Role.Button, onClick = onClick)
            .defaultMinSize(minHeight = 36.dp)
            .padding(horizontal = 14.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text(text, style = CrundiType.Small.copy(color = if (pressed) CrundiColors.TextPrimary else CrundiColors.TextSecondary))
        CrundiIcon(icon, size = 13.dp, tint = if (pressed) CrundiColors.TextPrimary else CrundiColors.TextSecondary)
    }
}

/** The window round the animation: a bar with three dots and a label, as .wn-frame. */
@Composable
private fun TourFrame(title: String, modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    val shape = RoundedCornerShape(14.dp)
    Column(modifier.clip(shape).background(CrundiColors.BgSecondary).border(1.dp, CrundiColors.Border, shape)) {
        Row(Modifier.fillMaxWidth().height(StageBar).padding(horizontal = 14.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            repeat(3) { Box(Modifier.size(8.dp).clip(RoundedCornerShape(50)).background(CrundiColors.BgHover)) }
            Text(title, style = CrundiType.Small.copy(fontSize = 12.sp, fontWeight = FontWeight.SemiBold), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(start = 4.dp))
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(CrundiColors.Border))
        content()
    }
}

/**
 * The step's animation in a WebView with no scripts, no network and no
 * touches, laid out at the box's width and its 640 x 400 stage scaled to it.
 * In previews and screenshot tests (no WebView there) a still placeholder.
 */
@Composable
private fun ArtStage(art: String, modifier: Modifier) {
    BoxWithConstraints(modifier.background(CrundiColors.BgSecondary)) {
        val scale = maxWidth.value / WhatsNewArt.STAGE_W
        if (LocalInspectionMode.current) {
            Box(Modifier.matchParentSize(), contentAlignment = Alignment.Center) { CrundiIcon("play", size = 28.dp, tint = CrundiColors.TextMuted) }
            return@BoxWithConstraints
        }
        if (art.isEmpty()) return@BoxWithConstraints
        val html = remember(art, scale) { WhatsNewArt.page(art, scale) }
        AndroidView(
            factory = { c -> artView(c) },
            update = { wv -> if (wv.tag != html) { wv.tag = html; wv.loadDataWithBaseURL(null, html, "text/html", "utf-8", null) } },
            onRelease = { wv -> runCatching { wv.stopLoading(); (wv.parent as? ViewGroup)?.removeView(wv); wv.destroy() } },
            modifier = Modifier.matchParentSize(),
        )
        // Over the frame: its touches are the tour's (swipes), never the page's.
        Box(Modifier.matchParentSize().pointerInput(Unit) {})
    }
}

@SuppressLint("ClickableViewAccessibility")
private fun artView(c: android.content.Context): WebView = WebView(c).apply {
    setBackgroundColor(CrundiColors.BgSecondary.toArgb())
    layoutParams = ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
    settings.apply {
        javaScriptEnabled = false
        blockNetworkLoads = true
        allowFileAccess = false
        allowContentAccess = false
        domStorageEnabled = false
        setSupportZoom(false)
        builtInZoomControls = false
        displayZoomControls = false
        // CSS pixels are dp: the stage's scale is worked out from the box's width.
        useWideViewPort = false
        loadWithOverviewMode = false
    }
    isFocusable = false
    isFocusableInTouchMode = false
    isClickable = false
    isLongClickable = false
    isVerticalScrollBarEnabled = false
    isHorizontalScrollBarEnabled = false
    overScrollMode = View.OVER_SCROLL_NEVER
    importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS
    setOnTouchListener { _, _ -> true }
    webViewClient = object : WebViewClient() {
        override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean = true
        // Nothing is fetched: the page is self-contained.
        override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? =
            if (request.url.scheme == "data") null else WebResourceResponse("text/plain", "utf-8", java.io.ByteArrayInputStream(ByteArray(0)))
    }
}

/** The recap's list: every step, ticked; a tap goes back to it. */
@Composable
private fun Recap(steps: List<WhatsNewStep>, onGo: (Int) -> Unit, modifier: Modifier) {
    Column(modifier.verticalScroll(rememberScrollState()).padding(horizontal = 14.dp, vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        steps.forEachIndexed { i, s ->
            val shape = RoundedCornerShape(12.dp)
            Row(
                Modifier.fillMaxWidth().clip(shape).background(CrundiColors.BgCard).border(1.dp, CrundiColors.Border, shape)
                    .clickable(role = Role.Button) { onGo(i) }.padding(horizontal = 13.dp, vertical = 9.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                CrundiIcon("check", size = 17.dp, tint = CrundiColors.Green, modifier = Modifier.padding(top = 1.dp))
                Column {
                    Text(s.title, style = CrundiType.Body.copy(fontWeight = FontWeight.SemiBold), maxLines = 2, overflow = TextOverflow.Ellipsis)
                    if (s.tag.isNotEmpty()) Text(s.tag, style = CrundiType.Small.copy(fontSize = 12.sp))
                }
            }
        }
    }
}

@Composable
private fun Eyebrow(tag: String, note: String?) {
    Row(Modifier.padding(bottom = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Text(tag.uppercase(), style = CrundiType.Section.copy(color = CrundiColors.AccentHover, fontWeight = FontWeight.Bold, fontSize = 11.sp, letterSpacing = 0.14.em))
        if (note != null) Text(note, style = CrundiType.Small.copy(color = CrundiColors.TextMuted, fontSize = 11.sp, letterSpacing = 0.04.em))
    }
}

@Composable
private fun Title(text: String) {
    Text(text, style = CrundiType.Title.copy(fontSize = 27.sp, lineHeight = 30.sp, fontWeight = FontWeight.Bold, letterSpacing = (-0.02).em), modifier = Modifier.padding(bottom = 9.dp))
}

@Composable
private fun Body(text: String) {
    Text(text, style = CrundiType.Body.copy(fontSize = 15.sp, lineHeight = 22.sp, color = BodyText), modifier = Modifier.padding(bottom = 14.dp))
}

/** The keys rows: a key, or a word such as "Ask", then what it does. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Keys(s: WhatsNewStep) {
    Column(verticalArrangement = Arrangement.spacedBy(9.dp)) {
        for (k in s.keys.take(5)) {
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                FlowRow(Modifier.width(132.dp), horizontalArrangement = Arrangement.spacedBy(4.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    for (key in k.keys) Text(
                        key,
                        style = CrundiType.Meta.copy(color = CrundiColors.TextPrimary, fontWeight = FontWeight.SemiBold, fontSize = 11.5.sp),
                        maxLines = 1,
                        modifier = Modifier.clip(RoundedCornerShape(6.dp)).background(CrundiColors.BgSecondary)
                            .border(1.dp, CrundiColors.Border, RoundedCornerShape(6.dp)).padding(horizontal = 7.dp, vertical = 3.dp),
                    )
                }
                Text(k.text, style = CrundiType.Body.copy(fontSize = 13.sp, color = BodyText), modifier = Modifier.weight(1f))
            }
        }
    }
}

/** The dots, then Back and Next (Done on the recap). */
@Composable
private fun TourFoot(at: Int, n: Int, onGo: (Int) -> Unit, onDone: () -> Unit) {
    val last = at == n - 1
    Column(
        Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, top = 10.dp, bottom = 18.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(7.dp), verticalAlignment = Alignment.CenterVertically) {
            for (i in 0 until n) {
                val w by animateDpAsState(if (i == at) 26.dp else 7.dp, label = "whatsnew-dot")
                Box(
                    Modifier.width(w).height(7.dp).clip(RoundedCornerShape(99.dp))
                        .background(if (i == at) CrundiColors.Accent else if (i < at) CrundiColors.Accent.copy(alpha = 0.5f) else CrundiColors.BgHover)
                        .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { onGo(i) },
                )
            }
        }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            TourButton("Back", leading = "chevron-left", primary = false, onClick = { onGo(at - 1) }, modifier = Modifier.alpha(if (at == 0) 0f else 1f), enabled = at > 0)
            if (last) TourButton("Done", leading = "check", primary = true, onClick = onDone, modifier = Modifier.weight(1f))
            else TourButton("Next", trailing = "chevron-right", primary = true, onClick = { onGo(at + 1) }, modifier = Modifier.weight(1f))
        }
    }
}

@Composable
private fun TourButton(text: String, primary: Boolean, onClick: () -> Unit, modifier: Modifier = Modifier, leading: String? = null, trailing: String? = null, enabled: Boolean = true) {
    val src = remember { MutableInteractionSource() }
    val pressed by src.collectIsPressedAsState()
    val shape = RoundedCornerShape(12.dp)
    val bg = when {
        primary -> if (pressed) CrundiColors.AccentHover else CrundiColors.Accent
        else -> if (pressed) CrundiColors.BgHover else CrundiColors.BgCard
    }
    val fg = if (primary) Color.White else CrundiColors.TextPrimary
    Row(
        modifier.clip(shape).background(bg).border(1.dp, if (primary) CrundiColors.Accent else CrundiColors.Border, shape)
            .clickable(interactionSource = src, indication = null, enabled = enabled, role = Role.Button, onClick = onClick)
            .defaultMinSize(minHeight = 48.dp).padding(horizontal = 18.dp, vertical = 13.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
    ) {
        if (leading != null) CrundiIcon(leading, size = 15.dp, tint = fg)
        Text(text, style = CrundiType.Button.copy(color = fg, fontSize = 14.5.sp), maxLines = 1)
        if (trailing != null) CrundiIcon(trailing, size = 15.dp, tint = fg)
    }
}

/** Where a swipe of [dx] pixels goes from page [at] of [n]: left for the next, right for the one before. */
internal fun swipeTarget(at: Int, n: Int, dx: Float, threshold: Float): Int = when {
    dx < -threshold && at < n - 1 -> at + 1
    dx > threshold && at > 0 -> at - 1
    else -> at
}
