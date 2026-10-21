package mv.obitech.crundi

import android.content.Context
import mv.obitech.crundi.ui.shell.AppServices
import mv.obitech.crundi.ui.shell.Connection
import mv.obitech.crundi.ui.shell.ShellData
import mv.obitech.crundi.ui.shell.ShellSource
import mv.obitech.crundi.ui.shell.StaticShellSource

/**
 * Builds the services the native UI runs on, one set per paired server, kept
 * for as long as the process lives so the main window and every bubble share
 * one connection and one cache.
 *
 * FOUNDATION: the shell source here is a stand-in that knows only the
 * server's name. The API engineer replaces [create] with the real client
 * (live project list, connection state, approvals count, usage).
 */
object AppGraph {
    private val made = HashMap<String, AppServices>()

    fun forServer(ctx: Context, server: Server): AppServices = synchronized(made) {
        made.getOrPut(server.id) { create(ctx.applicationContext, server) }
    }

    /** The server was removed or paired again: drop what was built for it. */
    fun forget(serverId: String) = synchronized(made) { made.remove(serverId) }

    private fun create(app: Context, server: Server): AppServices = object : AppServices {
        override val shell: ShellSource = StaticShellSource(ShellData(serverHost = server.host, username = server.username, connection = Connection.Disconnected))
    }
}
