package mv.obitech.crundi.ui.chats

import androidx.annotation.OptIn
import androidx.compose.foundation.background
import androidx.compose.foundation.Image
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.window.DialogProperties
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiType
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.media3.common.MediaItem
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DefaultHttpDataSource
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.ui.PlayerView
import mv.obitech.crundi.api.model.ChatMedia
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.Dimens

/** A video in the log: its poster (or a dark frame), a play button and its length, the size a picture gets. */
@Composable
fun VideoPreview(m: ChatMedia, rowHeight: androidx.compose.ui.unit.Dp? = null, maxWidth: androidx.compose.ui.unit.Dp = 360.dp, onOpen: () -> Unit) {
    val posterLoad = rememberMediaImage(m.posterPath, 1280)
    val poster = posterLoad.image
    val ratio = when {
        poster != null && poster.height > 0 -> poster.width.toFloat() / poster.height
        m.width > 0 && m.height > 0 -> m.width.toFloat() / m.height
        else -> 16f / 9f
    }.coerceIn(0.3f, 3f)
    // In a row of several: that tall, as wide as its shape, within the row's cap.
    val frame = if (rowHeight != null) Modifier.height(rowHeight).width((rowHeight * ratio).coerceIn(90.dp, maxWidth))
    else Modifier.widthIn(max = 360.dp).heightIn(max = 420.dp).aspectRatio(ratio)
    Box(
        frame.clip(Dimens.ShapeSm).background(Color.Black)
            .border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm)
            // The poster's sweep while it loads. A poster that fails leaves the
            // dark frame: the video itself may still play, so the play button stays.
            .then(if (m.posterPath != null && posterLoad.loading) Modifier.shimmer() else Modifier)
            .clickable(onClickLabel = "Play", onClick = onOpen),
        contentAlignment = Alignment.Center,
    ) {
        if (poster != null) Image(poster, contentDescription = m.caption.ifEmpty { "Video" }, contentScale = ContentScale.Fit, modifier = Modifier.fillMaxSize())
        Box(Modifier.size(52.dp).clip(CircleShape).background(Color.Black.copy(alpha = 0.55f)), contentAlignment = Alignment.Center) {
            CrundiIcon("play", size = 24.dp, tint = Color.White)
        }
        if (m.durationSec > 0) Text(
            "%d:%02d".format((m.durationSec / 60).toInt(), (m.durationSec % 60).toInt()),
            style = CrundiType.Meta.copy(color = Color.White),
            modifier = Modifier.align(Alignment.BottomEnd).padding(6.dp).clip(RoundedCornerShape(4.dp)).background(Color.Black.copy(alpha = 0.6f)).padding(horizontal = 5.dp, vertical = 1.dp),
        )
    }
}

/**
 * A video opened from the log: over everything, playing at once, the status
 * bar gone. Its full-screen button hides every system bar and turns the
 * screen the video's way (landscape for a wide one, portrait for a tall one);
 * pressed again, or on closing, the phone goes back as it was.
 */
@OptIn(UnstableApi::class)
@Composable
fun VideoViewer(m: ChatMedia, onSave: () -> Unit, onClose: () -> Unit) {
    val path = m.path ?: return
    val ctx = LocalContext.current
    val activity = remember(ctx) { generateSequence(ctx) { (it as? android.content.ContextWrapper)?.baseContext }.filterIsInstance<android.app.Activity>().firstOrNull() }
    val transport = LocalAppServices.current.api.transport
    val player = remember(path) {
        val http = DefaultHttpDataSource.Factory().setAllowCrossProtocolRedirects(true).setDefaultRequestProperties(transport.streamHeaders())
        ExoPlayer.Builder(ctx).setMediaSourceFactory(DefaultMediaSourceFactory(ctx).setDataSourceFactory(http)).build().apply {
            setMediaItem(MediaItem.fromUri(transport.baseUrl + path))
            prepare()
            playWhenReady = true
        }
    }
    var full by remember { mutableStateOf(false) }
    // Loading until the first frame is drawn, and again while it buffers; a
    // failure says so and can be tried again.
    var firstFrame by remember { mutableStateOf(false) }
    var buffering by remember { mutableStateOf(true) }
    var failed by remember { mutableStateOf(false) }
    DisposableEffect(player) {
        val l = object : androidx.media3.common.Player.Listener {
            override fun onRenderedFirstFrame() { firstFrame = true }
            override fun onPlaybackStateChanged(state: Int) { buffering = state == androidx.media3.common.Player.STATE_BUFFERING || state == androidx.media3.common.Player.STATE_IDLE && !failed }
            override fun onPlayerError(error: androidx.media3.common.PlaybackException) { failed = true; buffering = false }
        }
        player.addListener(l)
        onDispose { player.removeListener(l) }
    }
    val before = remember { activity?.requestedOrientation ?: android.content.pm.ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED }
    DisposableEffect(player) {
        onDispose {
            player.release()
            activity?.requestedOrientation = before
        }
    }
    Dialog(onClose, DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        val view = androidx.compose.ui.platform.LocalView.current
        val window = (view.parent as? androidx.compose.ui.window.DialogWindowProvider)?.window
        val ctl = window?.let { androidx.core.view.WindowCompat.getInsetsController(it, view) }
        LaunchedEffect(full) {
            ctl?.systemBarsBehavior = androidx.core.view.WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            if (full) ctl?.hide(androidx.core.view.WindowInsetsCompat.Type.systemBars())
            else { ctl?.show(androidx.core.view.WindowInsetsCompat.Type.navigationBars()); ctl?.hide(androidx.core.view.WindowInsetsCompat.Type.statusBars()) }
            if (full) {
                // The video's own shape; the poster's, or its stated size, until it has started.
                val vs = player.videoSize
                val (w, h) = if (vs.width > 0 && vs.height > 0) vs.width to vs.height else m.width to m.height
                activity?.requestedOrientation = if (w > 0 && h > 0 && w > h) android.content.pm.ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
                else android.content.pm.ActivityInfo.SCREEN_ORIENTATION_SENSOR_PORTRAIT
            } else activity?.requestedOrientation = before
        }
        DisposableEffect(Unit) { onDispose { ctl?.show(androidx.core.view.WindowInsetsCompat.Type.systemBars()) } }
        Column(Modifier.fillMaxSize().background(Color.Black)) {
            // Pinch and double-tap zoom the picture only (its TextureView), as
            // in the picture viewer; the controls stay where they are.
            var scale by remember { mutableStateOf(1f) }
            var pan by remember { mutableStateOf(androidx.compose.ui.geometry.Offset.Zero) }
            var size by remember { mutableStateOf(androidx.compose.ui.unit.IntSize.Zero) }
            fun clamp(o: androidx.compose.ui.geometry.Offset, s: Float) = androidx.compose.ui.geometry.Offset(
                o.x.coerceIn(-size.width * (s - 1f) / 2f, size.width * (s - 1f) / 2f),
                o.y.coerceIn(-size.height * (s - 1f) / 2f, size.height * (s - 1f) / 2f),
            )
            Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
            AndroidView(
                factory = { c ->
                    (android.view.LayoutInflater.from(c).inflate(mv.obitech.crundi.R.layout.crundi_video_player, null) as PlayerView).apply {
                        this.player = player
                        useController = true
                        setShowNextButton(false); setShowPreviousButton(false)
                        setFullscreenButtonClickListener { full = it }
                    }
                },
                update = {
                    it.setFullscreenButtonState(full)
                    it.videoSurfaceView?.apply { scaleX = scale; scaleY = scale; translationX = pan.x; translationY = pan.y }
                },
                modifier = Modifier.fillMaxSize()
                    .onSizeChanged { size = it }
                    .pointerInput(Unit) {
                        detectTapGestures(onDoubleTap = { at ->
                            if (scale > 1.05f) { scale = 1f; pan = androidx.compose.ui.geometry.Offset.Zero }
                            // From 1x the tapped point stays under the finger: o' = (at - c) * (1 - s).
                            else { val c = androidx.compose.ui.geometry.Offset(size.width / 2f, size.height / 2f); scale = 2.5f; pan = clamp((at - c) * (1f - 2.5f), 2.5f) }
                        })
                    }
                    .pointerInput(Unit) {
                        detectTransformGestures { centroid, p, zoom, _ ->
                            val ns = (scale * zoom).coerceIn(1f, 6f)
                            val c = androidx.compose.ui.geometry.Offset(size.width / 2f, size.height / 2f)
                            // The point under the fingers stays there: the old
                            // offset scales with the picture, then the drag is added.
                            val o = pan * (ns / scale) + (centroid - c) * (1 - ns / scale) + p
                            scale = ns
                            pan = if (ns <= 1f) androidx.compose.ui.geometry.Offset.Zero else clamp(o, ns)
                        }
                    },
            )
            if (failed) Box(Modifier.size(200.dp, 120.dp)) {
                LoadFailed({ failed = false; buffering = true; player.prepare(); player.play() })
            } else if (!firstFrame || buffering) LoadingRing()
            }
            if (!full) Row(
                Modifier.fillMaxWidth().background(Color.Black).navigationBarsPadding().padding(start = 14.dp, end = 4.dp, top = 2.dp, bottom = 2.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(m.caption.ifEmpty { m.filename }, style = CrundiType.Small.copy(color = CrundiColors.TextSecondary), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                CrundiIconButton("download", onSave, contentDescription = "Save to Downloads", tint = CrundiColors.TextPrimary)
                CrundiIconButton("x", onClose, contentDescription = "Close", tint = CrundiColors.TextPrimary)
            }
        }
    }
}
