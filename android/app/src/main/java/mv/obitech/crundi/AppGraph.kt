package mv.obitech.crundi

import android.content.Context
import mv.obitech.crundi.api.CrundiClient
import mv.obitech.crundi.api.LiveShellSource
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
        made.getOrPut(server.id) { create(ctx.applicationContext, server) }
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
