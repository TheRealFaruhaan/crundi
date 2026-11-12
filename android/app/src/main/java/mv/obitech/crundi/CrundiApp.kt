package mv.obitech.crundi

import android.app.Application

class CrundiApp : Application() {
    override fun onCreate() {
        super.onCreate()
        Push.init(this)
        Notifier.channels(this)
        LiveCards.channel(this)
        // "Hey Crundi" (Phase 6): knows when the app is in front, for in-app listening.
        mv.obitech.crundi.voice.HeyCrundi.init(this)
        // Share this phone's connection (homepool): a registered node comes back with the app.
        mv.obitech.crundi.homepool.Homepool.reconcile(this)
    }
}
