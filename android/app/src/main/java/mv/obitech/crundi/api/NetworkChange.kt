package mv.obitech.crundi.api

import android.net.ConnectivityManager
import android.net.Network

/**
 * Says when the phone's default network becomes a different one (Wi-Fi to
 * mobile data, or back), or comes back after none: what was connected on
 * the old one is dead without knowing it. Not for the first network seen
 * (registering reports the one in use), nor for the same one again.
 *
 * [changed] is what to do; [seen] and [lost] are what the system callback
 * reports, kept apart from it so they are tested without Android.
 */
class NetworkChange(private val changed: () -> Unit) {
    private var currentKey: String? = null
    private var started = false

    /** A default network is in use ([key] tells two networks apart). */
    @Synchronized fun seen(key: String) {
        val was = currentKey
        currentKey = key
        if (!started) { started = true; return }
        if (was != key) changed()
    }

    /** The default network went away (nothing replaces it yet). */
    @Synchronized fun lost(key: String) { if (currentKey == key) currentKey = null }

    val callback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) = seen(network.toString())
        override fun onLost(network: Network) = lost(network.toString())
    }
}
