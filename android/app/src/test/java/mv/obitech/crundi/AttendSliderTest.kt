package mv.obitech.crundi

import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeLeft
import androidx.compose.ui.test.swipeRight
import androidx.test.ext.junit.runners.AndroidJUnit4
import mv.obitech.crundi.ui.theme.CrundiTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/**
 * The full-screen alert's slider, dragged as a finger would. It once sat at
 * the end without attending: locking itself cancelled the drag's own block
 * before the action ran.
 */
@RunWith(AndroidJUnit4::class)
@Config(sdk = [35])
class AttendSliderTest {
    @get:Rule val compose = createComposeRule()

    private var attended = 0
    private var dismissed = 0

    private fun slider() = compose.setContent { CrundiTheme { AttendSlider(onAttend = { attended++ }, onDismiss = { dismissed++ }) } }

    @Test fun slidToTheRightAttendsOnce() {
        slider()
        compose.onNodeWithTag("alert-thumb").performTouchInput { swipeRight(startX = centerX, endX = centerX + 2000f) }
        compose.waitForIdle()
        assertEquals(1, attended); assertEquals(0, dismissed)
        // Locked after: another slide does nothing.
        compose.onNodeWithTag("alert-thumb").performTouchInput { swipeLeft(startX = centerX, endX = centerX - 2000f) }
        compose.waitForIdle()
        assertEquals(1, attended); assertEquals(0, dismissed)
    }

    @Test fun slidToTheLeftDismissesOnce() {
        slider()
        compose.onNodeWithTag("alert-thumb").performTouchInput { swipeLeft(startX = centerX, endX = centerX - 2000f) }
        compose.waitForIdle()
        assertEquals(0, attended); assertEquals(1, dismissed)
    }

    @Test fun aShortSlideDoesNothing() {
        slider()
        compose.onNodeWithTag("alert-thumb").performTouchInput { swipeRight(startX = centerX, endX = centerX + 40f) }
        compose.waitForIdle()
        assertEquals(0, attended); assertEquals(0, dismissed)
    }
}
