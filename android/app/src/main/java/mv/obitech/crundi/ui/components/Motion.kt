package mv.obitech.crundi.ui.components

import android.provider.Settings
import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalInspectionMode

/**
 * Should motion stay still? Yes when the phone's animations are turned off
 * (Developer options or Accessibility "Remove animations" set the animator
 * duration scale to 0, the app's prefers-reduced-motion), and in a preview
 * or screenshot, where an endless animation never lets the test go idle.
 */
@Composable
fun motionStill(): Boolean {
    val ctx = LocalContext.current
    return LocalInspectionMode.current ||
        runCatching { Settings.Global.getFloat(ctx.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f }.getOrDefault(false)
}
