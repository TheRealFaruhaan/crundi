package mv.obitech.crundi

import android.app.NotificationManager
import android.content.Context
import android.os.Build
import org.json.JSONObject

/**
 * Alerts from the alert_user tool (push kind "alert"), and the person's say
 * over them: urgent alerts on or off per server, and whether they may ring
 * through Do Not Disturb. android/URGENT-ALERTS.md has the policy check and
 * the reasons for the two channels.
 *
 * What to do with one alert is decided by [decide], which knows nothing of
 * Android and is unit-tested; [Notifier.alert] does what it says.
 */
object UrgentAlerts {
    // Two channels, because Android does not let an app change a channel's
    // sound, audio usage or Do Not Disturb bypass once it has made it, and an
    // app may set bypass only while it holds Do Not Disturb access. So the
    // bypassing channel is a channel of its own, made only once the person
    // has opted in and granted that access (Android then keeps the bypass it
    // was made with), and deleted when they opt out. The plain one is always
    // there, so the person can tune or mute it in Android's settings.
    const val CHANNEL = "urgent"
    const val CHANNEL_DND = "urgent.dnd"

    private const val PREFS = "crundi"
    private const val PREF_DND = "urgent.dnd"
    private const val PREF_FULL = "urgent.fullscreen"
    private fun prefServer(serverId: String) = "urgent.server.$serverId"

    /** How an alert ends up, for the notification log. */
    enum class Outcome(val log: String) {
        RANG("rang"),
        RANG_THROUGH_DND("rang, through Do Not Disturb"),
        QUIET_SERVER_OFF("shown quietly: urgent alerts are off for this server"),
        DOWNGRADED("shown quietly: downgraded by the server (too many urgent alerts this hour)"),
        NORMAL("shown"),
    }

    /**
     * @param channel Notifier's channel id to post on.
     * @param insistent whether the sound repeats until the notification is seen (FLAG_INSISTENT).
     */
    /** @param fullScreen whether it also takes over the screen (lock screen, or a heads-up while the phone is in use). */
    data class Decision(val channel: String, val insistent: Boolean, val outcome: Outcome, val fullScreen: Boolean = false) {
        val rings: Boolean get() = insistent
    }

    /**
     * @param urgent the payload's `urgent`
     * @param downgraded the payload's `downgraded`
     * @param serverOn the person's "Urgent alerts" switch for the server it came from
     * @param dndOptIn the person's "Ring through Do Not Disturb"
     * @param dndGranted whether Android has given the app Do Not Disturb access
     * @param quietChannel the channel for an alert that does not ring
     * @param fullScreenReady the person turned on "Full-screen urgent alerts" and Android allows it
     */
    fun decide(urgent: Boolean, downgraded: Boolean, serverOn: Boolean, dndOptIn: Boolean, dndGranted: Boolean, quietChannel: String, fullScreenReady: Boolean = false): Decision =
        decideRing(urgent, downgraded, serverOn, dndOptIn, dndGranted, quietChannel).let { if (it.rings && fullScreenReady) it.copy(fullScreen = true) else it }

    private fun decideRing(urgent: Boolean, downgraded: Boolean, serverOn: Boolean, dndOptIn: Boolean, dndGranted: Boolean, quietChannel: String): Decision = when {
        // The server already turned it into a normal alert; never ring it,
        // whatever the payload says besides.
        downgraded -> Decision(quietChannel, false, Outcome.DOWNGRADED)
        !urgent -> Decision(quietChannel, false, Outcome.NORMAL)
        !serverOn -> Decision(quietChannel, false, Outcome.QUIET_SERVER_OFF)
        dndOptIn && dndGranted -> Decision(CHANNEL_DND, true, Outcome.RANG_THROUGH_DND)
        else -> Decision(CHANNEL, true, Outcome.RANG)
    }

    /** The same, from the payload as it arrived. */
    fun decide(payload: JSONObject, serverOn: Boolean, dndOptIn: Boolean, dndGranted: Boolean, quietChannel: String, fullScreenReady: Boolean = false): Decision =
        decide(payload.optBoolean("urgent"), payload.optBoolean("downgraded"), serverOn, dndOptIn, dndGranted, quietChannel, fullScreenReady)

    /** A notification id of its own for each alert (by when it was sent), never an earlier one's. */
    fun alertId(serverId: String, chatId: String, at: Long): Int = "alert:$serverId:$chatId:$at".hashCode()

    fun isUrgentChannel(channel: String?): Boolean = channel == CHANNEL || channel == CHANNEL_DND

    /** How long after it arrives an alert left in the shade keeps other notifications silent. */
    const val HUSH_MS = 10 * 60 * 1000L

    /** One of the app's showing notifications: its channel, flags, and how long ago it was posted. */
    data class Showing(val channel: String?, val flags: Int, val ageMs: Long)

    /**
     * Is an urgent alert still ringing? On an urgent channel, insistent, and
     * posted less than [HUSH_MS] ago: one left unread in the shade must not
     * keep everything else silent for good.
     */
    fun ringing(showing: List<Showing>): Boolean =
        showing.any { isUrgentChannel(it.channel) && it.flags and android.app.Notification.FLAG_INSISTENT != 0 && it.ageMs in 0 until HUSH_MS }

    /** "urgent" or "normal", as the log says it. Downgraded alerts were asked for as urgent. */
    fun levelOf(urgent: Boolean, downgraded: Boolean): String = if (urgent || downgraded) "urgent" else "normal"

    // ─── The person's choices ───

    fun serverOn(ctx: Context, serverId: String): Boolean =
        ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean(prefServer(serverId), true)

    fun setServerOn(ctx: Context, serverId: String, on: Boolean) =
        ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean(prefServer(serverId), on).apply()

    /** A server removed: its switch goes with it. */
    fun forget(ctx: Context, serverId: String) =
        ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().remove(prefServer(serverId)).apply()

    fun dndOptIn(ctx: Context): Boolean = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean(PREF_DND, false)

    fun setDndOptIn(ctx: Context, on: Boolean) = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean(PREF_DND, on).apply()

    // Full screen: like an alarm clock, the alert lights up the lock screen.
    // Google Play grants this permission by itself only to calling and alarm
    // apps; for Crundi the person turns it on in Android's settings, and
    // without it the alert rings as a heads-up, as before.
    fun fullScreenOptIn(ctx: Context): Boolean = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean(PREF_FULL, false)

    fun setFullScreenOptIn(ctx: Context, on: Boolean) = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean(PREF_FULL, on).apply()

    /** Android 14 and up ask; before that the permission comes with the install. */
    fun fullScreenGranted(ctx: Context): Boolean =
        if (Build.VERSION.SDK_INT >= 34) (ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager).canUseFullScreenIntent() else true

    fun dndGranted(ctx: Context): Boolean =
        (ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager).isNotificationPolicyAccessGranted

    /** Does the bypassing channel exist and actually bypass? False if the person changed it, or it is not made yet. */
    fun dndChannelBypasses(ctx: Context): Boolean {
        val nm = ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        return nm.getNotificationChannel(CHANNEL_DND)?.canBypassDnd() == true
    }
}
