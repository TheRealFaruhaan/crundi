package mv.obitech.crundi.homepool

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/**
 * Starts a registered node (autoStart) after a reboot or an app update. One
 * that is not registered never starts on its own, and is recorded as stopped:
 * it did not survive the reboot.
 */
class HomepoolBootReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        if (intent.action != Intent.ACTION_BOOT_COMPLETED && intent.action != Intent.ACTION_MY_PACKAGE_REPLACED) return
        val p = NodePrefs(ctx)
        if (!p.autoStart || p.config == null || p.refused != null) {
            p.enabled = false
            return
        }
        p.enabled = true
        NodeRuntime.log(if (intent.action == Intent.ACTION_BOOT_COMPLETED) "starting after a reboot" else "starting after an update")
        Homepool.launch(ctx)
    }
}
