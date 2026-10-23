package mv.obitech.crundi

import android.content.Context
import android.util.TypedValue

/** Sizes for the few things drawn outside Compose (notification pictures). */
object Ui {
    fun dp(ctx: Context, v: Int) = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v.toFloat(), ctx.resources.displayMetrics).toInt()
}
