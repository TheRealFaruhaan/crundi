package mv.obitech.crundi

import android.app.Activity
import android.app.AlertDialog
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings

/**
 * Once, after notifications are allowed (first install, or the update that
 * brought it): offer the two urgent-alert options, both ticked, since each
 * also needs a switch in Android's settings the person would not find alone.
 * "Turn on" records the choice, then opens Android's page for each one still
 * needing it, one after the other as the app comes back to the front.
 * Either can be changed later in App settings.
 */
object UrgentSetup {
    private const val PREFS = "crundi"
    private const val PREF_ASKED = "asked.urgentSetup"
    private const val PREF_PENDING = "urgentSetup.pending"
    const val FULL = "full"
    const val DND = "dnd"

    /** Which of Android's pages still need visiting, in order, for what was chosen. Pure, for tests. */
    fun steps(full: Boolean, fullGranted: Boolean, dnd: Boolean, dndGranted: Boolean): List<String> =
        listOfNotNull(FULL.takeIf { full && !fullGranted }, DND.takeIf { dnd && !dndGranted })

    fun offer(activity: Activity) {
        if (!Push.available) return
        val prefs = activity.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        if (prefs.getBoolean(PREF_ASKED, false)) return
        prefs.edit().putBoolean(PREF_ASKED, true).apply()
        val picked = booleanArrayOf(true, true)
        AlertDialog.Builder(activity, android.R.style.Theme_DeviceDefault_Dialog_Alert)
            .setTitle("Urgent alerts")
            .setMultiChoiceItems(
                arrayOf("Full screen when the phone is locked, like an alarm", "Ring even in Do Not Disturb"),
                picked,
            ) { _, which, on -> picked[which] = on }
            .setPositiveButton("Turn on") { _, _ ->
                UrgentAlerts.setFullScreenOptIn(activity, picked[0])
                UrgentAlerts.setDndOptIn(activity, picked[1])
                val todo = steps(picked[0], UrgentAlerts.fullScreenGranted(activity), picked[1], UrgentAlerts.dndGranted(activity))
                prefs.edit().putString(PREF_PENDING, todo.joinToString(",")).apply()
                PushLog.add(activity.applicationContext, "Urgent alert setup: full screen ${if (picked[0]) "on" else "off"}, through Do Not Disturb ${if (picked[1]) "on" else "off"}")
                next(activity)
            }
            .setNegativeButton("Not now", null)
            .show()
    }

    /** Open Android's page for the next step still to do, if any. Called when the app comes back to the front. */
    fun next(activity: Activity) {
        val prefs = activity.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val pending = prefs.getString(PREF_PENDING, "").orEmpty().split(',').filter { it.isNotEmpty() }
        if (pending.isEmpty()) return
        prefs.edit().putString(PREF_PENDING, pending.drop(1).joinToString(",")).apply()
        val intent = when (pending.first()) {
            FULL -> if (Build.VERSION.SDK_INT >= 34) Intent(Settings.ACTION_MANAGE_APP_USE_FULL_SCREEN_INTENT, Uri.parse("package:${activity.packageName}")) else null
            DND -> Intent(Settings.ACTION_NOTIFICATION_POLICY_ACCESS_SETTINGS)
            else -> null
        }
        if (intent == null || runCatching { activity.startActivity(intent) }.isFailure) next(activity)
    }
}
