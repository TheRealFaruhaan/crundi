package mv.obitech.crundi

import android.app.Activity
import android.app.AlertDialog
import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import android.os.Build
import android.provider.Settings

/**
 * Bubbles switched off, and how to switch them on. A toast could not hold the
 * steps (it was cut off after one line), and on Samsung the app's own
 * bubble switch is not even shown until the phone-wide "Floating
 * notifications" setting is Bubbles, so "turn them on for this app" sent
 * people to a page with nothing to turn on.
 */
object BubbleHelp {
    private val samsung get() = Build.MANUFACTURER.equals("samsung", true)

    /** True when Android will not show this app's bubbles at all. */
    fun blocked(ctx: Context): Boolean {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) return false
        val nm = ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        return nm.bubblePreference == NotificationManager.BUBBLE_PREFERENCE_NONE
    }

    private fun label(ctx: Context) = ctx.applicationInfo.loadLabel(ctx.packageManager).toString()

    fun steps(ctx: Context): String {
        val app = label(ctx)
        return if (samsung) {
            "Bubbles are off for $app.\n\n" +
                "1. In the phone's Settings: Notifications, Advanced settings, Floating notifications: choose Bubbles.\n\n" +
                "2. Then for $app: Notifications, Bubbles (or \"Show as bubble\"): All conversations.\n\n" +
                "The app's bubble setting only appears after step 1."
        } else {
            "Bubbles are off for $app.\n\nIn its notification settings, set Bubbles to All conversations (or Selected, then allow this chat)."
        }
    }

    /** The app's own bubble page, or its notification page where that does not exist. */
    fun openAppSettings(ctx: Context) {
        val pkg = ctx.packageName
        runCatching { ctx.startActivity(Intent(Settings.ACTION_APP_NOTIFICATION_BUBBLE_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE, pkg).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
            .recoverCatching { ctx.startActivity(Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE, pkg).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
    }

    /** The phone's notification settings (where Samsung keeps Floating notifications). */
    fun openPhoneSettings(ctx: Context) {
        runCatching { ctx.startActivity(Intent("android.settings.NOTIFICATION_SETTINGS").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
            .recoverCatching { ctx.startActivity(Intent(Settings.ACTION_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
    }

    /** Explain, with buttons to the right pages. */
    fun show(activity: Activity) {
        val b = AlertDialog.Builder(activity, android.R.style.Theme_DeviceDefault_Dialog_Alert)
            .setTitle("Turn on bubbles")
            .setMessage(steps(activity))
            .setNegativeButton("Not now", null)
        if (samsung) {
            b.setPositiveButton("Phone settings") { _, _ -> openPhoneSettings(activity) }
            b.setNeutralButton("App settings") { _, _ -> openAppSettings(activity) }
        } else {
            b.setPositiveButton("Open settings") { _, _ -> openAppSettings(activity) }
        }
        b.show()
    }
}
