package mv.obitech.crundi

import android.app.AlarmManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import org.json.JSONObject

/**
 * What happened to an urgent alert, told to the server that sent it, where
 * alert_user is waiting for it (POST /api/alerts/answer): "ringing" with how
 * long it will ring, then "attended", "dismissed" or "snoozed" with minutes.
 * Alerts from a server too old to send an alertId report nothing.
 *
 * Snooze: the alert goes away and comes back, ringing again, after 5, 15, 30
 * or 60 minutes. The alarm that brings it back is Android's inexact kind (an
 * exact one needs a permission Play keeps for alarm clocks), so it may come a
 * little late when the phone has been still for a long time.
 */
object AlertAnswers {
    const val EXTRA_ALERT = "alert.id"
    const val EXTRA_POSTED = "alert.posted"
    const val EXTRA_RING_MS = "alert.ringMs"
    const val EXTRA_PAYLOAD = "alert.payload"
    val SNOOZE_MINUTES = listOf(5, 15, 30, 60)

    fun snoozeLabel(minutes: Int): String = if (minutes >= 60) "${minutes / 60} hour" else "$minutes min"

    /**
     * A swipe and the ring time running out both reach the alert's delete
     * intent; one that comes within a few seconds of the end is the time
     * running out, not the person.
     */
    fun endedByTime(postedAt: Long, ringMs: Long, now: Long): Boolean = ringMs > 0 && postedAt > 0 && now - postedAt >= ringMs - 3_000

    /** Tell the server. `done` runs once it has been tried (for a receiver's goAsync). */
    fun report(ctx: Context, serverId: String, alertId: String, action: String, ringMs: Long = 0, minutes: Int = 0, done: () -> Unit = {}) {
        if (serverId.isEmpty() || alertId.isEmpty()) { done(); return }
        val app = ctx.applicationContext
        val body = JSONObject().put("alertId", alertId).put("action", action)
        if (ringMs > 0) body.put("ringMs", ringMs)
        if (minutes > 0) body.put("minutes", minutes)
        Bg.run({ Api.authed(app, serverId, "POST", "/api/alerts/answer", body) }) { r ->
            val reply = r.getOrNull()
            if (action != "ringing") PushLog.add(app, "Urgent alert $action${if (minutes > 0) " ($minutes min)" else ""}: ${if (reply?.ok == true) "told the server" else "the server was not told (${reply?.status ?: "no connection"})"}")
            done()
        }
    }

    /** Bring the alert back after `minutes`, as it came (minus the alertId: its answer has been given). */
    fun snooze(ctx: Context, serverId: String, payload: String, minutes: Int) {
        if (serverId.isEmpty() || payload.isEmpty()) return
        val at = System.currentTimeMillis() + minutes * 60_000L
        val pi = PendingIntent.getBroadcast(
            ctx, (serverId + payload).hashCode(),
            Intent(ctx, SnoozeReceiver::class.java).setAction("snooze:${payload.hashCode()}")
                .putExtra(Notifier.EXTRA_SERVER, serverId).putExtra(EXTRA_PAYLOAD, payload),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        (ctx.getSystemService(Context.ALARM_SERVICE) as AlarmManager).setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, pi)
        PushLog.add(ctx.applicationContext, "Urgent alert snoozed for $minutes min")
    }
}

/** A snoozed alert's time is up: it rings again. */
class SnoozeReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        val server = Servers.byId(ctx, intent.getStringExtra(Notifier.EXTRA_SERVER).orEmpty()) ?: return
        val payload = runCatching { JSONObject(intent.getStringExtra(AlertAnswers.EXTRA_PAYLOAD).orEmpty()) }.getOrNull() ?: return
        payload.remove("alertId")
        payload.put("at", System.currentTimeMillis())
        Notifier.alert(ctx.applicationContext, server, payload)
    }
}
