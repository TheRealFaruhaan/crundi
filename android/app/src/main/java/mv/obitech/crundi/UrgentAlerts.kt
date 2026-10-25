package mv.obitech.crundi

import android.app.NotificationManager
import android.content.Context
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
    data class Decision(val channel: String, val insistent: Boolean, val outcome: Outcome) {
        val rings: Boolean get() = insistent
    }

    /**
     * @param urgent the payload's `urgent`
     * @param downgraded the payload's `downgraded`
     * @param serverOn the person's "Urgent alerts" switch for the server it came from
     * @param dndOptIn the person's "Ring through Do Not Disturb"
     * @param dndGranted whether Android has given the app Do Not Disturb access
     * @param quietChannel the channel for an alert that does not ring
     */
    fun decide(urgent: Boolean, downgraded: Boolean, serverOn: Boolean, dndOptIn: Boolean, dndGranted: Boolean, quietChannel: String): Decision = when {
        // The server already turned it into a normal alert; never ring it,
        // whatever the payload says besides.
        downgraded -> Decision(quietChannel, false, Outcome.DOWNGRADED)
        !urgent -> Decision(quietChannel, false, Outcome.NORMAL)
        !serverOn -> Decision(quietChannel, false, Outcome.QUIET_SERVER_OFF)
        dndOptIn && dndGranted -> Decision(CHANNEL_DND, true, Outcome.RANG_THROUGH_DND)
        else -> Decision(CHANNEL, true, Outcome.RANG)
    }

    /** The same, from the payload as it arrived. */
    fun decide(payload: JSONObject, serverOn: Boolean, dndOptIn: Boolean, dndGranted: Boolean, quietChannel: String): Decision =
        decide(payload.optBoolean("urgent"), payload.optBoolean("downgraded"), serverOn, dndOptIn, dndGranted, quietChannel)

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

    fun dndGranted(ctx: Context): Boolean =
        (ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager).isNotificationPolicyAccessGranted

    /** Does the bypassing channel exist and actually bypass? False if the person changed it, or it is not made yet. */
    fun dndChannelBypasses(ctx: Context): Boolean {
        val nm = ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        return nm.getNotificationChannel(CHANNEL_DND)?.canBypassDnd() == true
    }
}
