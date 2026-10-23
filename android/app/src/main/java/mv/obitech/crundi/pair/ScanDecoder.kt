package mv.obitech.crundi.pair

import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.qrcode.QRCodeReader

/**
 * What the scanner does with one camera frame: look for the Crundi code (the
 * picture the server shows) and, failing that, an ordinary QR code of the same
 * link (the server's fallback). Either way only a pairing link is accepted.
 *
 * Plain JVM (no Android), so it is tested as it runs.
 */
object ScanDecoder {
    /** Frames are looked at no bigger than this on the long side: plenty, and a few milliseconds a look. */
    const val LONG_SIDE = 640

    private val PAIR_LINK = Regex("^https?://[^\\s/]+/pair/?#[a-f0-9]{24}$")
    private val hints = mapOf(DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE), DecodeHintType.TRY_HARDER to true)

    /**
     * Read a frame. `rgba` is 4 bytes a pixel, rows `rowStride` bytes apart
     * (a camera pads its rows). Returns the pairing link, or null for "not yet".
     */
    fun decode(rgba: ByteArray, width: Int, height: Int, rowStride: Int = width * 4): String? {
        val img = shrink(rgba, width, height, rowStride)
        CrundiCode.decodeImage(img)?.let { if (PAIR_LINK.matches(it)) return it }
        return qr(img)?.takeIf { PAIR_LINK.matches(it) }
    }

    /** Nearest-neighbour down to LONG_SIDE, packed rows. */
    fun shrink(rgba: ByteArray, width: Int, height: Int, rowStride: Int): CrundiCode.Image {
        val scale = minOf(1.0, LONG_SIDE.toDouble() / maxOf(width, height))
        val w = maxOf(1, (width * scale).toInt()); val h = maxOf(1, (height * scale).toInt())
        if (w == width && h == height && rowStride == width * 4) return CrundiCode.Image(rgba, w, h)
        val out = ByteArray(w * h * 4)
        for (y in 0 until h) {
            val sy = minOf(height - 1, (y / scale).toInt())
            for (x in 0 until w) {
                val sx = minOf(width - 1, (x / scale).toInt())
                System.arraycopy(rgba, sy * rowStride + sx * 4, out, (y * w + x) * 4, 4)
            }
        }
        return CrundiCode.Image(out, w, h)
    }

    private fun qr(img: CrundiCode.Image): String? {
        val n = img.width * img.height
        val lum = ByteArray(n)
        val d = img.data
        for (i in 0 until n) {
            val r = d[i * 4].toInt() and 0xff; val g = d[i * 4 + 1].toInt() and 0xff; val b = d[i * 4 + 2].toInt() and 0xff
            lum[i] = ((r * 77 + g * 150 + b * 29) shr 8).toByte()
        }
        val src = PlanarYUVLuminanceSource(lum, img.width, img.height, 0, 0, img.width, img.height, false)
        return try {
            QRCodeReader().decode(BinaryBitmap(HybridBinarizer(src)), hints).text
        } catch (e: Exception) {
            // Not found, or a bad checksum: either way, not yet. Inverted too:
            // a QR code drawn light on a dark page reads only that way.
            try { QRCodeReader().decode(BinaryBitmap(HybridBinarizer(src.invert())), hints).text } catch (e2: Exception) { null }
        }
    }
}
