package mv.obitech.crundi.ui.chats

import androidx.annotation.OptIn
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.widthIn
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

/**
 * A video or a sound Claude showed (show_video, show_audio), playing in the
 * chat as the web's <video> and <audio> do. It used to be only a download
 * card. Streamed from the server with the phone's sign-in, with seeking
 * (the server honours byte ranges). Nothing plays until it is tapped.
 */
@OptIn(UnstableApi::class)
@Composable
fun MediaPlayerView(m: ChatMedia, modifier: Modifier = Modifier) {
    val ctx = LocalContext.current
    val transport = LocalAppServices.current.api.transport
    val path = m.path ?: return
    val video = m.kind == "video"
    val player = remember(path) {
        val http = DefaultHttpDataSource.Factory().setAllowCrossProtocolRedirects(true).setDefaultRequestProperties(transport.streamHeaders())
        ExoPlayer.Builder(ctx).setMediaSourceFactory(DefaultMediaSourceFactory(ctx).setDataSourceFactory(http)).build().apply {
            setMediaItem(MediaItem.fromUri(transport.baseUrl + path))
            prepare()
            playWhenReady = false
        }
    }
    DisposableEffect(player) { onDispose { player.release() } }
    val ratio = if (m.width > 0 && m.height > 0) (m.width.toFloat() / m.height).coerceIn(0.4f, 3f) else 16f / 9f
    Column(modifier) {
        AndroidView(
            factory = { c ->
                PlayerView(c).apply {
                    this.player = player
                    useController = true
                    setShowNextButton(false); setShowPreviousButton(false)
                    if (!video) { controllerShowTimeoutMs = 0; controllerHideOnTouch = false; showController() }
                }
            },
            modifier = Modifier.widthIn(max = 360.dp).fillMaxWidth()
                .then(if (video) Modifier.aspectRatio(ratio).heightIn(max = 420.dp) else Modifier.height(96.dp))
                .clip(Dimens.ShapeSm).background(androidx.compose.ui.graphics.Color.Black)
                .border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm),
        )
    }
}
