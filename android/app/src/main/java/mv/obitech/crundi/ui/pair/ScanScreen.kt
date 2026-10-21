package mv.obitech.crundi.ui.pair

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.ClipOp
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType

/** What the scanner is doing, for the words under the frame. */
enum class ScanState(val text: String) {
    Looking("Point at the picture on your server"),
    Found("Got it"),
    NoCamera("The camera could not be opened. Go back and paste the pairing link instead."),
    Denied("Crundi needs the camera to read the picture. Go back and paste the pairing link instead."),
}

/**
 * The scanning screen: the camera full screen, a rounded frame to aim with
 * (green once a code is read), what is happening, and Cancel. `camera` is the
 * live preview; screenshot tests pass a plain dark box.
 */
@Composable
fun ScanScreen(state: ScanState, onCancel: () -> Unit, camera: @Composable () -> Unit) {
    Box(Modifier.fillMaxSize().background(CrundiColors.BgPrimary)) {
        camera()
        BoxWithConstraints(Modifier.fillMaxSize()) {
            val side = minOf(maxWidth * 0.74f, 320.dp)
            val shade = CrundiColors.BgPrimary.copy(alpha = 0.55f)
            // Darken everything outside the frame.
            Box(
                Modifier.fillMaxSize().drawWithContent {
                    val s = side.toPx(); val r = 28.dp.toPx()
                    val tl = Offset((size.width - s) / 2, (size.height - s) / 2 - 40.dp.toPx())
                    val hole = Path().apply { addRoundRect(androidx.compose.ui.geometry.RoundRect(tl.x, tl.y, tl.x + s, tl.y + s, CornerRadius(r))) }
                    clipPath(hole, ClipOp.Difference) { drawRect(shade) }
                },
            )
            // The frame sits 40dp above the middle; the shade's hole above is cut at the same place.
            Box(
                Modifier.align(Alignment.Center).offset(y = (-40).dp).size(side).border(
                    3.dp,
                    if (state == ScanState.Found) CrundiColors.Green else CrundiColors.AccentHover.copy(alpha = 0.9f),
                    RoundedCornerShape(28.dp),
                ),
            )
            Text(
                state.text,
                style = CrundiType.Body.copy(fontSize = CrundiType.Heading.fontSize),
                textAlign = TextAlign.Center,
                modifier = Modifier.align(Alignment.TopCenter).padding(top = maxHeight / 2 - 40.dp + side / 2 + 22.dp, start = 24.dp, end = 24.dp).widthIn(max = 300.dp),
            )
            CrundiButton("Cancel", onCancel, modifier = Modifier.align(Alignment.BottomCenter).navigationBarsPadding().padding(bottom = 28.dp))
        }
    }
}
