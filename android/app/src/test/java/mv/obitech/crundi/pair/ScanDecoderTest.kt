package mv.obitech.crundi.pair

import com.google.zxing.BarcodeFormat
import com.google.zxing.qrcode.QRCodeWriter
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** The scanner's per-frame work: the fallback QR code, padded camera rows, and only pairing links. */
class ScanDecoderTest {
    private val link = "https://dev.faruhaan.com/pair#0123456789abcdef01234567"

    /** A QR code of `text`, dark on light, in an RGBA frame with `pad` spare bytes at the end of each row. */
    private fun qrFrame(text: String, w: Int = 1280, h: Int = 720, pad: Int = 0, invert: Boolean = false): Triple<ByteArray, Int, Int> {
        val side = 480
        val m = QRCodeWriter().encode(text, BarcodeFormat.QR_CODE, side, side)
        val stride = w * 4 + pad
        val out = ByteArray(stride * h)
        val ox = (w - side) / 2; val oy = (h - side) / 2
        for (y in 0 until h) for (x in 0 until w) {
            val inQr = x - ox in 0 until side && y - oy in 0 until side && m[x - ox, y - oy]
            var v = if (inQr) 20 else 235
            if (invert) v = 255 - v
            val p = y * stride + x * 4
            out[p] = v.toByte(); out[p + 1] = v.toByte(); out[p + 2] = v.toByte(); out[p + 3] = -1
        }
        return Triple(out, w, stride)
    }

    @Test fun readsTheFallbackQrCode() {
        val (data, w, stride) = qrFrame(link)
        assertEquals(link, ScanDecoder.decode(data, w, 720, stride))
    }

    @Test fun readsPaddedRowsAndLightOnDark() {
        val (data, w, stride) = qrFrame(link, pad = 64, invert = true)
        assertEquals(link, ScanDecoder.decode(data, w, 720, stride))
    }

    @Test fun ignoresQrCodesThatAreNotPairingLinks() {
        val (data, w, stride) = qrFrame("https://example.com/somewhere")
        assertNull(ScanDecoder.decode(data, w, 720, stride))
    }

    @Test fun nothingInABlankFrame() {
        assertNull(ScanDecoder.decode(ByteArray(640 * 480 * 4), 640, 480))
    }

    @Test fun readsTheCrundiCodeAfterShrinking() {
        // A code painted at 2x the size the reader works at: shrink, then read.
        val enc = CrundiCode.encode(link)!!
        val w = 1280; val h = 960; val size = 300.0
        val data = ByteArray(w * h * 4)
        for (y in 0 until h) for (x in 0 until w) {
            val cx = (x - w / 2.0) / size; val cy = (y - h / 2.0) / size
            val r = Math.hypot(cx, cy)
            var c = intArrayOf(10, 10, 15)
            if (Math.abs(r - CrundiCode.R_INNER) < CrundiCode.FINDER_WIDTH / 2 || Math.abs(r - CrundiCode.R_OUTER) < CrundiCode.FINDER_WIDTH / 2) c = intArrayOf(129, 140, 248)
            for (k in enc.rings.indices) {
                if (Math.abs(r - CrundiCode.RING_RADII[k]) >= enc.cls.stroke / 2) continue
                val n = enc.rings[k].size
                var a = Math.atan2(cy, cx) % (Math.PI * 2); if (a < 0) a += Math.PI * 2
                val i = minOf(n - 1, (a / (Math.PI * 2 / n)).toInt())
                if (enc.rings[k][i] == 1) c = if (i == 0) intArrayOf(251, 191, 36) else intArrayOf(129, 140, 248)
            }
            val p = (y * w + x) * 4
            data[p] = c[0].toByte(); data[p + 1] = c[1].toByte(); data[p + 2] = c[2].toByte(); data[p + 3] = -1
        }
        assertEquals(link, ScanDecoder.decode(data, w, h))
    }
}
