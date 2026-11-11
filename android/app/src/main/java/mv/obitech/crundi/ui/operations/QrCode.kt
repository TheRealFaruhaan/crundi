package mv.obitech.crundi.ui.operations

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.common.BitMatrix
import com.google.zxing.qrcode.QRCodeWriter
import com.google.zxing.qrcode.decoder.ErrorCorrectionLevel
import mv.obitech.crundi.ui.theme.Dimens

/** The QR modules of [text] (no quiet zone: the white box round it is the margin), or null if it cannot be made. */
fun qrMatrix(text: String): BitMatrix? = runCatching {
    QRCodeWriter().encode(text, BarcodeFormat.QR_CODE, 0, 0, mapOf(EncodeHintType.MARGIN to 0, EncodeHintType.ERROR_CORRECTION to ErrorCorrectionLevel.M))
}.getOrNull()

/** [text] as a QR code, black on white (what a scanner reads best), [size] square. Nothing when it cannot be made. */
@Composable
fun QrCode(text: String, modifier: Modifier = Modifier, size: Dp = 220.dp, description: String = "QR code") {
    val m = remember(text) { qrMatrix(text) } ?: return
    Box(modifier.background(Color.White, Dimens.ShapeSm).padding(12.dp).semantics { contentDescription = description }) {
        Canvas(Modifier.size(size)) {
            val cell = this.size.width / m.width
            for (y in 0 until m.height) for (x in 0 until m.width) {
                if (m[x, y]) drawRect(Color.Black, Offset(x * cell, y * cell), Size(cell + 0.5f, cell + 0.5f))
            }
        }
    }
}
