package mv.obitech.crundi.ui.theme

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.unit.dp

/** The desktop's spacing and shapes. Use these, not loose numbers. */
object Dimens {
    val TopBarHeight = 48.dp        // --topbar-height
    val TabBarHeight = 42.dp        // .tab-btn: 10px padding round a 17px icon + 2px line
    val Radius = 10.dp              // --radius: cards, dialogs, panels
    val RadiusSm = 6.dp             // --radius-sm: rows, buttons, inputs
    val RadiusPill = 12.dp          // status badges
    val Hairline = 1.dp             // every border

    val S1 = 4.dp
    val S2 = 8.dp
    val S3 = 12.dp
    val S4 = 16.dp
    val S5 = 20.dp
    val S6 = 24.dp
    val S8 = 32.dp

    /** Side padding of a screen's content (the desktop's 16px topbar padding). */
    val ScreenPad = 16.dp
    /** A list row's padding (sidebar-item: 8px 12px), stretched for a finger. */
    val RowPadH = 12.dp
    val RowPadV = 10.dp
    /** Smallest touch target. */
    val Touch = 44.dp
    val IconSm = 15.dp
    val Icon = 18.dp
    val IconLg = 22.dp

    val ShapeSm = RoundedCornerShape(RadiusSm)
    val Shape = RoundedCornerShape(Radius)
    val ShapePill = RoundedCornerShape(RadiusPill)
}
