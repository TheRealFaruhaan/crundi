package mv.obitech.crundi

import android.content.Context
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.util.TypedValue
import android.view.Gravity
import android.view.ViewGroup
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView

/** The few native screens this app has are built here, in one style, with no layout files. */
object Ui {
    fun dp(ctx: Context, v: Int) = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v.toFloat(), ctx.resources.displayMetrics).toInt()

    fun column(ctx: Context) = LinearLayout(ctx).apply {
        orientation = LinearLayout.VERTICAL
        gravity = Gravity.CENTER_HORIZONTAL
        setBackgroundColor(ctx.getColor(R.color.bg))
        val p = dp(ctx, 24)
        setPadding(p, p, p, p)
    }

    fun title(ctx: Context, text: String) = TextView(ctx).apply {
        this.text = text
        setTextColor(ctx.getColor(R.color.text))
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 20f)
        typeface = Typeface.DEFAULT_BOLD
        gravity = Gravity.CENTER
    }

    fun body(ctx: Context, text: String) = TextView(ctx).apply {
        this.text = text
        setTextColor(ctx.getColor(R.color.text_dim))
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f)
        gravity = Gravity.CENTER
        setLineSpacing(0f, 1.25f)
    }

    fun button(ctx: Context, text: String, primary: Boolean, onClick: () -> Unit) = Button(ctx).apply {
        this.text = text
        isAllCaps = false
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f)
        setTextColor(ctx.getColor(R.color.text))
        stateListAnimator = null
        background = GradientDrawable().apply {
            cornerRadius = dp(ctx, 10).toFloat()
            setColor(ctx.getColor(if (primary) R.color.accent else R.color.surface_hi))
            if (!primary) setStroke(dp(ctx, 1), ctx.getColor(R.color.border))
        }
        setOnClickListener { onClick() }
    }

    fun wide(ctx: Context, topDp: Int = 12) = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = dp(ctx, topDp) }
    fun wrap(ctx: Context, topDp: Int = 12) = LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = dp(ctx, topDp) }
}
