package mv.obitech.crundi

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import androidx.core.app.NotificationManagerCompat

/** Dismiss, pressed on an urgent alert: takes it away, which stops the ringing. */
class AlertDismissReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        val id = intent.getIntExtra(Notifier.EXTRA_NOTIFICATION, 0)
        if (id == 0) return
        NotificationManagerCompat.from(ctx).cancel(id)
        PushLog.add(ctx.applicationContext, "An urgent alert was dismissed from its notification")
    }
}
