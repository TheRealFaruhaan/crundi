package mv.obitech.crundi

import android.content.Context
import mv.obitech.crundi.api.CrundiClient
import mv.obitech.crundi.api.LiveShellSource
import mv.obitech.crundi.api.NetworkChange
import mv.obitech.crundi.ui.shell.AppServices
import mv.obitech.crundi.ui.shell.ShellSource

/**
 * Builds the services the native UI runs on, one set per paired server,
 * kept for as long as the process lives so the main window and every bubble
 * share one connection and one cache.
 */
object AppGraph {
    private val made = HashMap<String, AppServices>()

    fun forServer(ctx: Context, server: Server): AppServices = synchronized(made) {
        watchNetwork(ctx.applicationContext)
        made.getOrPut(server.id) { create(ctx.applicationContext, server) }
    }

    private var watching = false
    private val network = NetworkChange { synchronized(made) { made.values.toList() }.forEach { it.api.networkChanged() } }

    /**
     * Connections made again when the phone's network changes (Wi-Fi to
     * mobile data and back): the old ones are dead without knowing it, and a
     * chat on one stopped mid-reply and stayed "working" until a ping noticed.
     */
    private fun watchNetwork(app: Context) {
        if (watching) return
        watching = true
        runCatching { app.getSystemService(android.net.ConnectivityManager::class.java).registerDefaultNetworkCallback(network.callback) }
    }

    /** The server was removed or paired again: drop what was built for it. */
    fun forget(serverId: String) = synchronized(made) { made.remove(serverId) }

    private fun create(app: Context, server: Server): AppServices {
        val client = CrundiClient.forServer(app, server)
        // The handshake's word is read each time, not kept: it is made again
        // on every open, and the server may have been updated since.
        val shell = LiveShellSource(client, server.host, server.username, app.getSharedPreferences("crundi", Context.MODE_PRIVATE), "project.${server.id}",
            about = { ServerCaps.version(app, server.id) to ServerCaps.get(app, server.id) })
        return object : AppServices {
            override val shell: ShellSource = shell
            override val api: CrundiClient = client
        }
    }
}
