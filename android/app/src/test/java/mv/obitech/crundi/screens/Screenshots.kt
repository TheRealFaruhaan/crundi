package mv.obitech.crundi.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.github.takahirom.roborazzi.RobolectricDeviceQualifiers
import com.github.takahirom.roborazzi.captureRoboImage
import mv.obitech.crundi.ui.shell.AppServices
import mv.obitech.crundi.ui.shell.Host
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.shell.LocalHost
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiTheme
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The base of every screenshot test. A phone (Pixel 7: 412 by 915 dp,
 * 420 dpi), drawn on the JVM by Robolectric, saved by Roborazzi.
 *
 *   class ServicesScreens : ScreenshotTest() {
 *       @Test fun list() = shoot("services-list") { ServicesScreen(state = FakeData.services) }
 *   }
 *
 * Name images "<area>-<screen>" in lower case with dashes. They are written
 * to app/build/outputs/screens/ and copied to the team's shared folder by
 *   ./gradlew --no-daemon testPlayDebugUnitTest exportScreens -PscreensDir=<shared>/screens
 */
@RunWith(AndroidJUnit4::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = RobolectricDeviceQualifiers.Pixel7)
abstract class ScreenshotTest {
    /** Draw `content` full screen in the Crundi theme, with test services, and save it. */
    fun shoot(name: String, services: AppServices = FakeServices.default(), host: Host = Host.None, content: @Composable () -> Unit) {
        require(Regex("^[a-z0-9]+(-[a-z0-9]+)*$").matches(name)) { "Screenshot names are lower-case-with-dashes: $name" }
        captureRoboImage(filePath = "build/outputs/screens/$name.png") {
            CrundiTheme {
                CompositionLocalProvider(LocalAppServices provides services, LocalHost provides host) {
                    Box(Modifier.fillMaxSize().background(CrundiColors.BgPrimary)) { content() }
                }
            }
        }
    }
}
