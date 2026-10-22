package mv.obitech.crundi.screens

import androidx.activity.ComponentActivity
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalInspectionMode
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onRoot
import com.github.takahirom.roborazzi.captureRoboImage
import mv.obitech.crundi.ui.shell.AppServices
import mv.obitech.crundi.ui.shell.Host
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.shell.LocalHost
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiTheme
import mv.obitech.crundi.ui.workbench.LocalWorkbenchStore
import mv.obitech.crundi.ui.workbench.WorkbenchStore
import org.junit.Rule

/**
 * Screenshots of live screens: the real routes over the fake server
 * (FakeApi), given time for its answers to arrive before the picture is
 * taken. A route loads on a background dispatcher; shoot() captures as soon
 * as the first frame is idle, which is still the spinner. [shootLive] lets the
 * main looper and the background work settle first, so the workbench's panes
 * show their real content.
 */
abstract class LiveScreenshotTest : ScreenshotTest() {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    fun shootLive(
        name: String,
        services: AppServices = FakeServices.default(),
        host: Host = Host.None,
        store: WorkbenchStore = WorkbenchStore.memory(),
        settleMs: Long = 2500,
        content: @Composable () -> Unit,
    ) {
        require(Regex("^[a-z0-9]+(-[a-z0-9]+)*$").matches(name)) { "Screenshot names are lower-case-with-dashes: $name" }
        compose.setContent {
            CrundiTheme {
                CompositionLocalProvider(
                    LocalAppServices provides services, LocalHost provides host,
                    LocalInspectionMode provides true, LocalWorkbenchStore provides store,
                ) {
                    Box(Modifier.fillMaxSize().background(CrundiColors.BgPrimary)) { content() }
                }
            }
        }
        val end = System.currentTimeMillis() + settleMs
        while (System.currentTimeMillis() < end) {
            compose.waitForIdle()
            Thread.sleep(40)
            org.robolectric.shadows.ShadowLooper.idleMainLooper()
        }
        compose.waitForIdle()
        compose.onRoot().captureRoboImage("build/outputs/screens/$name.png")
    }
}
