package mv.obitech.crundi

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri

/**
 * Where this build gets its updates: the Play Store, and nowhere else.
 *
 * This file exists only in the `play` build. Nothing here, or anywhere in
 * this build, knows about GitHub releases.
 */
object Updater {
    const val LABEL = "Update in the Play Store"
    const val WHERE = "the Play Store"

    fun open(ctx: Context) {
        val id = ctx.packageName
        try {
            ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("market://details?id=$id")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        } catch (e: ActivityNotFoundException) {
            ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("https://play.google.com/store/apps/details?id=$id")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        }
    }
}
