package mv.obitech.crundi.ui.chats

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import mv.obitech.crundi.api.model.ChatMedia
import mv.obitech.crundi.api.rememberServerImage
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType

/**
 * A picture Claude showed, on the whole screen: pinch to zoom, drag to look
 * around, double-tap to zoom in or back out. It was a small card inside the
 * chat pane, no bigger than the picture in the log, and could not zoom.
 */
@Composable
fun ImageViewer(media: ChatMedia, onSave: () -> Unit, onClose: () -> Unit) {
    Dialog(onClose, DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        val img = rememberServerImage(media.path, 4096)
        var scale by remember { mutableFloatStateOf(1f) }
        var offset by remember { mutableStateOf(Offset.Zero) }
        var box by remember { mutableStateOf(IntSize.Zero) }
        // Keep the picture on screen: at 1x it is centred; zoomed, it can be
        // dragged only as far as its edges.
        fun clamp(o: Offset, s: Float): Offset {
            val mx = box.width * (s - 1f) / 2f
            val my = box.height * (s - 1f) / 2f
            return Offset(o.x.coerceIn(-mx, mx), o.y.coerceIn(-my, my))
        }
        Box(Modifier.fillMaxSize().background(Color.Black)) {
            Box(
                Modifier.fillMaxSize().clipToBounds().onSizeChanged { box = it }
                    .pointerInput(Unit) {
                        detectTapGestures(onDoubleTap = { at ->
                            if (scale > 1.05f) { scale = 1f; offset = Offset.Zero }
                            else {
                                val s = 2.5f
                                // Zoom in on the place tapped.
                                val c = Offset(box.width / 2f, box.height / 2f)
                                scale = s; offset = clamp((c - at) * (s - 1f), s)
                            }
                        })
                    }
                    .pointerInput(Unit) {
                        detectTransformGestures { centroid, pan, zoom, _ ->
                            val ns = (scale * zoom).coerceIn(1f, 8f)
                            val c = Offset(box.width / 2f, box.height / 2f)
                            // Zoom about the fingers, then follow the drag.
                            val o = (offset + (centroid - c) * (1 - ns / scale)) + pan
                            scale = ns
                            offset = if (ns <= 1f) Offset.Zero else clamp(o, ns)
                        }
                    },
                contentAlignment = Alignment.Center,
            ) {
                if (img != null) Image(
                    img, contentDescription = media.caption.ifEmpty { "Image" }, contentScale = ContentScale.Fit,
                    modifier = Modifier.fillMaxSize().graphicsLayer { scaleX = scale; scaleY = scale; translationX = offset.x; translationY = offset.y },
                )
                else CrundiIcon("image", size = 40.dp, tint = CrundiColors.TextMuted)
            }
            Column(
                Modifier.align(Alignment.BottomCenter).fillMaxWidth().background(Color.Black.copy(alpha = 0.6f)).navigationBarsPadding().padding(12.dp),
                horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                if (media.caption.isNotEmpty()) Text(media.caption, style = CrundiType.Small.copy(color = CrundiColors.TextPrimary))
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    CrundiButton("Save to Downloads", onSave, icon = "download")
                    CrundiButton("Close", onClose)
                }
            }
        }
    }
}
