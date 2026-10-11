package mv.obitech.crundi

import android.app.Application

class CrundiApp : Application() {
    override fun onCreate() {
        super.onCreate()
        Push.init(this)
        Notifier.channels(this)
        LiveCards.channel(this)
    }
}
