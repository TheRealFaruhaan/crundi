package mv.obitech.crundi.ui.chats

import android.media.AudioFormat
import android.media.MediaCodec
import android.media.MediaExtractor
import android.media.MediaFormat
import android.util.LruCache
import androidx.annotation.OptIn
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
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
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalInspectionMode
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DefaultHttpDataSource
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withPermit
import kotlinx.coroutines.withContext
import mv.obitech.crundi.api.model.ChatMedia
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens
import kotlin.coroutines.coroutineContext
import kotlin.math.abs
import kotlin.math.max

/** How many bars a waveform has; drawn bars are resampled from these to fit the card. */
private const val BARS = 120

/**
 * Only one sound plays at a time across the app, as on the web: the player
 * that starts pauses whichever one was playing.
 */
private object AudioFocus {
    private var current: Player? = null
    fun started(p: Player) { if (current !== p) current?.pause(); current = p }
    fun released(p: Player) { if (current === p) current = null }
}

/**
 * A sound Claude showed (show_audio), drawn as SoundCloud draws a track: a
 * round play button, the title, a waveform that fills with the accent colour
 * as it plays (tap or drag it to seek) and the time. The player is made on
 * the first tap so a chat full of sounds downloads none of them until asked,
 * and released when the card leaves the chat.
 */
@OptIn(UnstableApi::class)
@Composable
fun AudioCard(m: ChatMedia, onDownload: () -> Unit) {
    val path = m.path ?: return
    val ctx = LocalContext.current
    val transport = LocalAppServices.current.api.transport
    val inspection = LocalInspectionMode.current

    var player by remember(path) { mutableStateOf<ExoPlayer?>(null) }
    var playing by remember { mutableStateOf(false) }
    var buffering by remember { mutableStateOf(false) }
    var failed by remember { mutableStateOf(false) }
    var positionMs by remember { mutableStateOf(0L) }
    var durationMs by remember { mutableStateOf((m.durationSec * 1000).toLong()) }
    // While the finger is on the waveform: where it is (0..1). The player
    // seeks when it lifts, so a drag does not fire a hundred seeks.
    var scrub by remember { mutableStateOf<Float?>(null) }
    // A seek asked for before the player knows the length.
    var pendingSeek by remember { mutableStateOf<Float?>(null) }
    var peaks by remember(path) { mutableStateOf(Waveforms.cached(path) ?: pseudoWaveform(m.filename.ifEmpty { path })) }

    fun ensurePlayer(): ExoPlayer = player ?: run {
        val http = DefaultHttpDataSource.Factory().setAllowCrossProtocolRedirects(true).setDefaultRequestProperties(transport.streamHeaders())
        ExoPlayer.Builder(ctx).setMediaSourceFactory(DefaultMediaSourceFactory(ctx).setDataSourceFactory(http)).build().also { p ->
            p.addListener(object : Player.Listener {
                override fun onIsPlayingChanged(isPlaying: Boolean) {
                    playing = isPlaying
                    if (isPlaying) AudioFocus.started(p)
                    positionMs = p.currentPosition
                }
                override fun onPlaybackStateChanged(state: Int) {
                    buffering = state == Player.STATE_BUFFERING
                    if (p.duration != C.TIME_UNSET && p.duration > 0) durationMs = p.duration
                    if (state == Player.STATE_READY) pendingSeek?.let { f -> pendingSeek = null; p.seekTo((p.duration * f).toLong()) }
                    // At the end it goes back to the start, ready to play again.
                    if (state == Player.STATE_ENDED) { p.pause(); p.seekTo(0); positionMs = 0 }
                }
                override fun onPlayerError(error: PlaybackException) { failed = true; buffering = false }
            })
            p.setMediaItem(MediaItem.fromUri(transport.baseUrl + path))
            p.prepare()
            player = p
        }
    }

    DisposableEffect(path) {
        onDispose { player?.let { AudioFocus.released(it); it.release() }; player = null }
    }
    // The position, ten times a second, only while it plays.
    LaunchedEffect(playing, player) {
        val p = player ?: return@LaunchedEffect
        while (playing) { positionMs = p.currentPosition; delay(100) }
    }
    // The real waveform: right away for a small file, else once it is played
    // (decoding means downloading, which a long recording should not do on a scroll).
    val decodeNow = m.size in 1..(8L shl 20) || player != null
    var decoding by remember(path) { mutableStateOf(false) }
    LaunchedEffect(path, decodeNow) {
        if (!decodeNow || inspection || Waveforms.cached(path) != null || Waveforms.failed(path)) return@LaunchedEffect
        decoding = true
        try { Waveforms.decode(path, transport.baseUrl + path, transport.streamHeaders())?.let { peaks = it } } finally { decoding = false }
    }

    fun seekTo(f: Float) {
        val p = ensurePlayer()
        val d = if (p.duration != C.TIME_UNSET && p.duration > 0) p.duration else 0L
        if (d > 0) { p.seekTo((d * f).toLong()); positionMs = (d * f).toLong() } else { pendingSeek = f; if (durationMs > 0) positionMs = (durationMs * f).toLong() }
        if (!p.isPlaying) p.play()
    }

    val progress = scrub ?: if (durationMs > 0) (positionMs.toFloat() / durationMs).coerceIn(0f, 1f) else 0f
    val title = m.caption.ifEmpty { m.filename.ifEmpty { "Audio" } }

    Row(
        Modifier.widthIn(max = 440.dp).fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgCard)
            .border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm).padding(start = 10.dp, end = 4.dp, top = 9.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Box(
            Modifier.size(42.dp).clip(CircleShape).background(if (failed) CrundiColors.BgTertiary else CrundiColors.Accent)
                .clickable(enabled = !failed, onClickLabel = if (playing) "Pause" else "Play") {
                    val p = ensurePlayer()
                    if (p.isPlaying) p.pause() else p.play()
                },
            contentAlignment = Alignment.Center,
        ) {
            when {
                buffering && player?.playWhenReady == true -> Spinner(20.dp)
                playing -> CrundiIcon("pause", size = 18.dp, tint = Color.White)
                // The triangle's weight sits left of its box; nudged so it looks centred.
                else -> CrundiIcon("play", size = 18.dp, tint = if (failed) CrundiColors.TextMuted else Color.White, modifier = Modifier.padding(start = 2.dp))
            }
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(title, style = ChatType.Small.copy(color = CrundiColors.TextPrimary), maxLines = 1, overflow = TextOverflow.Ellipsis)
            Waveform(
                peaks, progress,
                onScrub = { scrub = it },
                onSeek = { f -> scrub = null; seekTo(f) },
                // The sweep runs over the stand-in bars while the real ones are worked out.
                modifier = Modifier.fillMaxWidth().height(34.dp).then(if (decoding) Modifier.shimmer() else Modifier),
            )
            Row(verticalAlignment = Alignment.CenterVertically) {
                val shown = scrub?.let { (durationMs * it).toLong() } ?: positionMs
                Text(clock(shown), style = CrundiType.Meta.copy(color = if (playing || shown > 0) CrundiColors.AccentHover else CrundiColors.TextMuted))
                Box(Modifier.weight(1f))
                if (failed) Text(
                    "Could not load · retry", style = CrundiType.Meta.copy(color = CrundiColors.Red),
                    modifier = Modifier.clickable(onClickLabel = "Try again") {
                        failed = false
                        player?.let { it.prepare(); it.play() }
                    },
                )
                else Text(if (durationMs > 0) clock(durationMs) else "--:--", style = CrundiType.Meta.copy(color = CrundiColors.TextMuted))
            }
        }
        CrundiIconButton("download", onDownload, contentDescription = "Download", size = 16.dp, tint = CrundiColors.TextSecondary, modifier = Modifier.align(Alignment.Top))
    }
}

private fun clock(ms: Long): String {
    val s = (ms / 1000).coerceAtLeast(0)
    return if (s >= 3600) "%d:%02d:%02d".format(s / 3600, s / 60 % 60, s % 60) else "%d:%02d".format(s / 60, s % 60)
}

/**
 * The bars: the played part in the accent colour, the rest muted. A tap seeks
 * there; a drag shows where it will land and seeks when the finger lifts.
 */
@Composable
private fun Waveform(peaks: FloatArray, progress: Float, onScrub: (Float) -> Unit, onSeek: (Float) -> Unit, modifier: Modifier = Modifier) {
    var last by remember { mutableStateOf(0f) }
    Canvas(
        modifier
            .pointerInput(Unit) { detectTapGestures { onSeek((it.x / size.width).coerceIn(0f, 1f)) } }
            .pointerInput(Unit) {
                detectHorizontalDragGestures(
                    onDragStart = { last = (it.x / size.width).coerceIn(0f, 1f); onScrub(last) },
                    onDragEnd = { onSeek(last) },
                    onDragCancel = { onSeek(last) },
                ) { change, _ -> last = (change.position.x / size.width).coerceIn(0f, 1f); onScrub(last); change.consume() }
            },
    ) {
        val slot = 3.5.dp.toPx()
        val n = max(8, (size.width / slot).toInt())
        val barW = slot * 0.62f
        val step = size.width / n
        val mid = size.height / 2
        for (i in 0 until n) {
            // The loudest of the source bars this drawn bar covers, so a narrow card keeps its peaks.
            val a = i * peaks.size / n
            val b = max(a + 1, (i + 1) * peaks.size / n)
            var v = 0f
            for (k in a until b.coerceAtMost(peaks.size)) v = max(v, peaks[k])
            val h = (0.12f + 0.88f * v.coerceIn(0f, 1f)) * size.height
            val x = i * step + (step - barW) / 2
            val played = (i + 0.5f) / n <= progress
            drawRoundRect(
                if (played) CrundiColors.Accent else CrundiColors.Border,
                topLeft = Offset(x, mid - h / 2), size = Size(barW, h), cornerRadius = CornerRadius(barW / 2),
            )
        }
    }
}

/**
 * A stand-in waveform, the same every time for the same name: shown while the
 * real one decodes, or for good when it cannot be decoded.
 */
internal fun pseudoWaveform(seed: String): FloatArray {
    val r = java.util.Random(seed.hashCode().toLong() * 2654435761L)
    val out = FloatArray(BARS)
    var v = 0.5f
    for (i in 0 until BARS) {
        // A random walk, smoothed, with the quiet start and end a track has.
        v = (v * 0.6f + r.nextFloat() * 0.4f + (r.nextFloat() - 0.5f) * 0.25f).coerceIn(0.15f, 1f)
        val edge = minOf(1f, i / 6f + 0.35f, (BARS - 1 - i) / 6f + 0.35f)
        out[i] = v * edge
    }
    return out
}

/**
 * Real waveforms, decoded from the file in the background: the loudest sample
 * in each of [BARS] slices of the first ten minutes. One decode at a time
 * across the app, kept per path so a card scrolled back to does not decode again.
 */
private object Waveforms {
    private val cache = LruCache<String, FloatArray>(64)
    private val bad = java.util.Collections.synchronizedSet(HashSet<String>())
    private val gate = Semaphore(1)
    private const val LIMIT_US = 10L * 60 * 1_000_000

    fun cached(path: String): FloatArray? = cache.get(path)
    fun failed(path: String) = path in bad

    suspend fun decode(path: String, url: String, headers: Map<String, String>): FloatArray? = gate.withPermit {
        cached(path)?.let { return@withPermit it }
        withContext(Dispatchers.IO) {
            val peaks = runCatching { decodePeaks(url, headers) }.getOrNull()
            if (peaks == null) bad += path else cache.put(path, peaks)
            peaks
        }
    }

    private suspend fun decodePeaks(url: String, headers: Map<String, String>): FloatArray? {
        val ex = MediaExtractor()
        var codec: MediaCodec? = null
        try {
            ex.setDataSource(url, headers)
            val track = (0 until ex.trackCount).firstOrNull { ex.getTrackFormat(it).getString(MediaFormat.KEY_MIME)?.startsWith("audio/") == true } ?: return null
            val fmt = ex.getTrackFormat(track)
            ex.selectTrack(track)
            val dur = if (fmt.containsKey(MediaFormat.KEY_DURATION)) fmt.getLong(MediaFormat.KEY_DURATION) else 0L
            val span = (if (dur > 0) dur else LIMIT_US).coerceAtMost(LIMIT_US)
            val c = MediaCodec.createDecoderByType(fmt.getString(MediaFormat.KEY_MIME)!!)
            codec = c
            c.configure(fmt, null, null, 0)
            c.start()
            val peaks = FloatArray(BARS)
            var floatPcm = false
            val info = MediaCodec.BufferInfo()
            var inputDone = false
            var outputDone = false
            while (!outputDone) {
                coroutineContext.ensureActive()
                if (!inputDone) {
                    val i = c.dequeueInputBuffer(10_000)
                    if (i >= 0) {
                        val buf = c.getInputBuffer(i)!!
                        val n = ex.readSampleData(buf, 0)
                        if (n < 0 || ex.sampleTime > span) { c.queueInputBuffer(i, 0, 0, 0, MediaCodec.BUFFER_FLAG_END_OF_STREAM); inputDone = true }
                        else { c.queueInputBuffer(i, 0, n, ex.sampleTime, 0); ex.advance() }
                    }
                }
                val o = c.dequeueOutputBuffer(info, 10_000)
                if (o == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) {
                    val of = c.outputFormat
                    floatPcm = android.os.Build.VERSION.SDK_INT >= 24 && of.containsKey(MediaFormat.KEY_PCM_ENCODING) && of.getInteger(MediaFormat.KEY_PCM_ENCODING) == AudioFormat.ENCODING_PCM_FLOAT
                } else if (o >= 0) {
                    if (info.size > 0) {
                        val bar = ((info.presentationTimeUs.toDouble() / span) * BARS).toInt().coerceIn(0, BARS - 1)
                        val out = c.getOutputBuffer(o)!!.order(java.nio.ByteOrder.nativeOrder())
                        out.position(info.offset); out.limit(info.offset + info.size)
                        var peak = peaks[bar]
                        if (floatPcm) { val fb = out.asFloatBuffer(); while (fb.hasRemaining()) peak = max(peak, abs(fb.get())) }
                        else { val sb = out.asShortBuffer(); while (sb.hasRemaining()) peak = max(peak, abs(sb.get().toInt()) / 32768f) }
                        peaks[bar] = peak
                    }
                    c.releaseOutputBuffer(o, false)
                    if (info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0) outputDone = true
                }
            }
            // Scaled so the loudest bar fills the height; a silent file stays flat.
            val top = peaks.maxOrNull() ?: 0f
            if (top <= 0.001f) return null
            for (i in peaks.indices) peaks[i] = peaks[i] / top
            return peaks
        } finally {
            runCatching { codec?.stop() }
            runCatching { codec?.release() }
            runCatching { ex.release() }
        }
    }
}
