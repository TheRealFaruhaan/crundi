package mv.obitech.crundi

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import androidx.core.app.NotificationManagerCompat

/**
 * An urgent alert's Dismiss button, and its delete intent: swiped away, or
 * the ring time ran out. A dismissal is told to the server (alert_user is
 * waiting); the time running out is not an answer, and leaves a quiet
 * "missed" notice instead.
 */
class AlertDismissReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        val id = intent.getIntExtra(Notifier.EXTRA_NOTIFICATION, 0)
        if (id == 0) return
        val app = ctx.applicationContext
        val serverId = intent.getStringExtra(Notifier.EXTRA_SERVER).orEmpty()
        val alertId = intent.getStringExtra(AlertAnswers.EXTRA_ALERT).orEmpty()
        val deleted = intent.action.orEmpty().startsWith("deleted:")
        if (deleted && AlertAnswers.endedByTime(intent.getLongExtra(AlertAnswers.EXTRA_POSTED, 0), intent.getLongExtra(AlertAnswers.EXTRA_RING_MS, 0), System.currentTimeMillis())) {
            PushLog.add(app, "An urgent alert rang out with nobody answering")
            Notifier.alertMissed(app, serverId, id, intent.getStringExtra(AlertActivity.EXTRA_TITLE).orEmpty(), intent.getStringExtra(AlertActivity.EXTRA_WHERE).orEmpty(), intent.getStringExtra(Notifier.EXTRA_PATH).orEmpty().ifEmpty { "/" })
            return
        }
        if (!deleted) NotificationManagerCompat.from(ctx).cancel(id)
        PushLog.add(app, if (deleted) "An urgent alert was swiped away" else "An urgent alert was dismissed from its notification")
        val pending = goAsync()
        AlertAnswers.report(app, serverId, alertId, "dismissed") { pending.finish() }
    }
}
