package mv.obitech.crundi.ui.components

import android.content.Context
import android.view.MotionEvent
import android.widget.FrameLayout

/**
 * A holder for an Android view (a WebView) inside Compose that keeps every
 * touch that starts on it. Compose hands an embedded view its touches only
 * after the Compose parents have had them, and a touch any of them claims (the
 * workbench's column swipe, a list's scroll) reaches the view as cancelled.
 * Asking the parent not to intercept as the finger lands makes the view the
 * first and only one to get the gesture: the browser and panels can be
 * scrolled, dragged and pinched without the columns moving.
 *
 * [handBack] is for a view that sits in something that scrolls up and down
 * (a docked panel in its list): a vertical swipe the page cannot scroll that
 * way goes back to the list. Off everywhere else. The WebView only knows
 * whether the page itself scrolls, not a box inside it that does, so a panel
 * that scrolls inside a box would hand every swipe away and never scroll.
 */
class ClaimTouchesFrame(context: Context, private val handBack: Boolean = false) : FrameLayout(context) {
    private val slop = android.view.ViewConfiguration.get(context).scaledTouchSlop
    private var x0 = 0f
    private var y0 = 0f
    private var decided = false

    override fun dispatchTouchEvent(ev: MotionEvent): Boolean {
        when (ev.actionMasked) {
            MotionEvent.ACTION_DOWN -> { x0 = ev.x; y0 = ev.y; decided = false; parent?.requestDisallowInterceptTouchEvent(true) }
            // A vertical swipe the page cannot scroll that way goes back to
            // what is around it (a chat scrolls past a panel shown in it).
            MotionEvent.ACTION_MOVE -> if (handBack && !decided) {
                val dx = ev.x - x0; val dy = ev.y - y0
                if (dx * dx + dy * dy > slop * slop) {
                    decided = true
                    val child = getChildAt(0)
                    if (child != null && kotlin.math.abs(dy) > kotlin.math.abs(dx) && !child.canScrollVertically(if (dy < 0) 1 else -1)) parent?.requestDisallowInterceptTouchEvent(false)
                }
            }
        }
        return super.dispatchTouchEvent(ev)
    }
}
