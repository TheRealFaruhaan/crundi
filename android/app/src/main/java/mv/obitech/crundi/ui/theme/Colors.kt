package mv.obitech.crundi.ui.theme

import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color

/**
 * The desktop's colours, one for one with the CSS variables at the top of
 * src/webapp-html.js (:root). Dark only, like the desktop. Use these names,
 * never a raw hex in a screen: if a colour is missing here, add it here.
 */
object CrundiColors {
    val BgPrimary = Color(0xFF0A0A0F)      // --bg-primary: the page
    val BgSecondary = Color(0xFF12121A)    // --bg-secondary: top bar, sidebar, panels, dialogs
    val BgTertiary = Color(0xFF1A1A28)     // --bg-tertiary: buttons, inputs on panels
    val BgHover = Color(0xFF22223A)        // --bg-hover: pressed / hovered rows
    val BgCard = Color(0xFF14141F)         // --bg-card: cards
    val Border = Color(0xFF2A2A3D)         // --border
    val BorderSubtle = Color(0xFF1E1E30)   // --border-subtle: dividers inside a panel
    val TextPrimary = Color(0xFFE8E8F0)    // --text-primary
    val TextSecondary = Color(0xFF8888A8)  // --text-secondary
    val TextMuted = Color(0xFF5A5A78)      // --text-muted
    val Accent = Color(0xFF6366F1)         // --accent
    val AccentHover = Color(0xFF818CF8)    // --accent-hover: active tab, active row text
    val AccentDeep = Color(0xFF4F46E5)     // --accent-deep
    val AccentDim = Color(0x266366F1)      // --accent-dim (15%)
    val Amber = Color(0xFFFBBF24)          // --amber: the mark's warm parts only
    val Green = Color(0xFF10B981)          // --green: running, connected, success
    val GreenDim = Color(0x2610B981)
    val Red = Color(0xFFEF4444)            // --red: errors, stop, delete
    val RedDim = Color(0x26EF4444)
    val Yellow = Color(0xFFF59E0B)         // --yellow: working, reconnecting, warnings
    val YellowDim = Color(0x26F59E0B)
    val Sky = Color(0xFF38BDF8)            // --sky: waiting
    val SkyDim = Color(0x2E38BDF8)
    val Scrim = Color(0xB3000000)          // dialog backdrop: rgba(0,0,0,0.7)
    val Ask = Color(0xFFC084FC)            // factory graph: an Ask step, "waiting on you"
    val Wait = Color(0xFFD946EF)           // factory graph: a Wait step's token
    val WaitLight = Color(0xFFF0ABFC)      // factory graph: a Wait step's head

    /** --accent-grad: the active tab's underline, the active row's edge, primary emphasis. */
    val AccentGrad: Brush = Brush.linearGradient(listOf(Accent, AccentHover))
}
