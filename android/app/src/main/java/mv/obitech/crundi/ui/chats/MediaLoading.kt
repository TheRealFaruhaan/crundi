package mv.obitech.crundi.ui.chats

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.platform.LocalInspectionMode
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import mv.obitech.crundi.api.ApiRequest
import mv.obitech.crundi.api.ServerImages
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType

/*
 * The one loading treatment for media in the chat: a soft light sweeping
 * across the box a picture, poster or waveform will fill, and the web's
 * accent ring (.wg-membed.loading) for frames and players. A failed load
 * says so and offers a retry, instead of a static icon that looked the same
 * as "still loading" and never changed.
 */

/** A light band sweeping across while something loads. Still in screenshot tests, which never go idle with an endless animation. */
fun Modifier.shimmer(): Modifier = composed {
    if (LocalInspectionMode.current) return@composed this
    val t = rememberInfiniteTransition(label = "shimmer")
    val x by t.animateFloat(-0.6f, 1.6f, infiniteRepeatable(tween(1400, easing = LinearEasing)), label = "sweep")
    drawWithContent {
        drawContent()
        val w = size.width.coerceAtLeast(1f)
        val band = (w * 0.45f).coerceIn(60f, 420f)
        val at = w * x
        drawRect(
            Brush.linearGradient(
                listOf(Color.Transparent, Color.White.copy(alpha = 0.075f), Color.Transparent),
                start = Offset(at - band, 0f), end = Offset(at + band, size.height * 0.35f),
            ),
        )
    }
}

/** The web's loading ring: a faint track with an accent arc turning on it. */
@Composable
fun LoadingRing(size: Dp = 34.dp, modifier: Modifier = Modifier) {
    val still = LocalInspectionMode.current
    val angle = if (still) 0f else {
        val t = rememberInfiniteTransition(label = "ring")
        val a by t.animateFloat(0f, 360f, infiniteRepeatable(tween(800, easing = LinearEasing)), label = "turn")
        a
    }
    Canvas(modifier.size(size)) {
        val stroke = 3.dp.toPx()
        val inset = stroke / 2
        val arcSize = androidx.compose.ui.geometry.Size(this.size.width - stroke, this.size.height - stroke)
        drawArc(Color.White.copy(alpha = 0.18f), 0f, 360f, false, topLeft = Offset(inset, inset), size = arcSize, style = Stroke(stroke))
        rotate(angle) {
            drawArc(CrundiColors.Accent, -90f, 90f, false, topLeft = Offset(inset, inset), size = arcSize, style = Stroke(stroke, cap = StrokeCap.Round))
        }
    }
}

/** "Could not load", tap to try again: filling whatever box the media had. */
@Composable
fun LoadFailed(onRetry: () -> Unit, modifier: Modifier = Modifier, compact: Boolean = false) {
    Column(
        modifier.fillMaxSize().clickable(onClickLabel = "Try again", onClick = onRetry).padding(8.dp),
        horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(4.dp, Alignment.CenterVertically),
    ) {
        CrundiIcon("refresh", size = if (compact) 18.dp else 24.dp, tint = CrundiColors.TextSecondary)
        Text("Could not load", style = ChatType.Small.copy(color = CrundiColors.TextSecondary), textAlign = TextAlign.Center)
        if (!compact) Text("Tap to try again", style = CrundiType.Meta, textAlign = TextAlign.Center)
    }
}

/** A server picture and where its load stands. */
class MediaImage(val image: ImageBitmap?, val failed: Boolean, val retry: () -> Unit) {
    val loading get() = image == null && !failed
}

/**
 * [mv.obitech.crundi.api.rememberServerImage] that also tells loading from
 * failed, with a retry. The shared cache holds a failure back for a while; a
 * retry the person asks for fetches again straight away rather than waiting
 * that out.
 */
@Composable
fun rememberMediaImage(path: String?, maxPx: Int): MediaImage {
    val api = LocalAppServices.current.api
    val images = api.images
    var img by remember(path, maxPx) { mutableStateOf(path?.let { images.cached(it, maxPx) }) }
    var failed by remember(path, maxPx) { mutableStateOf(false) }
    var attempt by remember(path, maxPx) { mutableIntStateOf(0) }
    LaunchedEffect(path, maxPx, attempt) {
        if (path == null || img != null) return@LaunchedEffect
        failed = false
        img = if (attempt == 0) images.load(path, maxPx) else fetchFresh(api.transport, path, maxPx)
        if (img == null) failed = true
    }
    return MediaImage(img, failed = failed || path == null) { if (failed) attempt++ }
}

private suspend fun fetchFresh(transport: mv.obitech.crundi.api.Transport, path: String, maxPx: Int): ImageBitmap? = runCatching {
    val r = transport.exchange(ApiRequest("GET", path.substringBefore('?'), queryOf(path), headers = mapOf("Accept" to "image/*,*/*")))
    val bytes = r.bytes ?: r.text.takeIf { it.isNotEmpty() }?.toByteArray()
    if (r.status !in 200..299 || bytes == null) null
    else withContext(Dispatchers.Default) { ServerImages.decode(bytes, r.headers.entries.firstOrNull { it.key.equals("content-type", true) }?.value.orEmpty(), maxPx) }
}.getOrNull()

/** A placeholder box's fill while loading: the card colour with the sweep over it. */
fun Modifier.loadingFill(): Modifier = this.background(CrundiColors.BgSecondary).shimmer()
