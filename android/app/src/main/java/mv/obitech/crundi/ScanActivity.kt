package mv.obitech.crundi

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Bundle
import android.util.Size
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.Preview
import androidx.camera.core.resolutionselector.ResolutionSelector
import androidx.camera.core.resolutionselector.ResolutionStrategy
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView
import mv.obitech.crundi.pair.ScanDecoder
import mv.obitech.crundi.ui.pair.ScanScreen
import mv.obitech.crundi.ui.pair.ScanState
import mv.obitech.crundi.ui.theme.CrundiTheme
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Scanning the pairing picture, natively: CameraX shows the camera and hands
 * frames to ScanDecoder, which reads the Crundi code (a Kotlin port of the
 * server's own reader, src/crundi-code.js) or the fallback QR code (ZXing).
 * Nothing leaves the phone; the result is a pairing link for PairActivity.
 */
class ScanActivity : ComponentActivity() {
    private var state by mutableStateOf(ScanState.Looking)
    private val done = AtomicBoolean(false)
    private var analysis: ExecutorService? = null
    private var preview: PreviewView? = null

    private val askCamera = registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        if (granted) startCamera() else state = ScanState.Denied
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            CrundiTheme {
                Box(Modifier.fillMaxSize()) {
                    ScanScreen(state, onCancel = { setResult(RESULT_CANCELED); finish() }) {
                        AndroidView(factory = { ctx -> PreviewView(ctx).also { preview = it; it.scaleType = PreviewView.ScaleType.FILL_CENTER } }, modifier = Modifier.fillMaxSize())
                    }
                }
            }
        }
        if (checkSelfPermission(Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) startCamera()
        else askCamera.launch(Manifest.permission.CAMERA)
    }

    private fun startCamera() {
        val future = ProcessCameraProvider.getInstance(this)
        future.addListener({
            val provider = runCatching { future.get() }.getOrNull()
            val view = preview
            if (provider == null || view == null || isFinishing) { state = ScanState.NoCamera; return@addListener }
            val pool = Executors.newSingleThreadExecutor().also { analysis = it }
            val show = Preview.Builder().build().also { it.surfaceProvider = view.surfaceProvider }
            val look = ImageAnalysis.Builder()
                .setOutputImageFormat(ImageAnalysis.OUTPUT_IMAGE_FORMAT_RGBA_8888)
                .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                .setResolutionSelector(
                    ResolutionSelector.Builder()
                        .setResolutionStrategy(ResolutionStrategy(Size(1280, 720), ResolutionStrategy.FALLBACK_RULE_CLOSEST_HIGHER_THEN_LOWER))
                        .build(),
                )
                .build()
            look.setAnalyzer(pool) { frame ->
                frame.use {
                    if (done.get()) return@use
                    val plane = it.planes[0]
                    val buf = plane.buffer.apply { rewind() }
                    val bytes = ByteArray(buf.remaining()).also { b -> buf.get(b) }
                    val link = runCatching { ScanDecoder.decode(bytes, it.width, it.height, plane.rowStride) }.getOrNull()
                    // Believe it only if it is the shape of a pairing link.
                    if (link != null && PairingLink.parse(link) != null && done.compareAndSet(false, true)) runOnUiThread { found(link) }
                }
            }
            try {
                provider.unbindAll()
                provider.bindToLifecycle(this, CameraSelector.DEFAULT_BACK_CAMERA, show, look)
            } catch (e: Exception) {
                state = ScanState.NoCamera
            }
        }, mainExecutor)
    }

    private fun found(link: String) {
        state = ScanState.Found
        setResult(RESULT_OK, Intent().putExtra(EXTRA_LINK, link))
        // A moment of green, so the person sees it worked.
        window.decorView.postDelayed({ finish() }, 350)
    }

    override fun onDestroy() {
        analysis?.shutdown()
        super.onDestroy()
    }

    companion object {
        const val EXTRA_LINK = "link"
    }
}
