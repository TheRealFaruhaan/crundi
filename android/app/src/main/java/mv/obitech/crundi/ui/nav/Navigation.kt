package mv.obitech.crundi.ui.nav

import androidx.compose.runtime.staticCompositionLocalOf
import androidx.navigation.NavGraph.Companion.findStartDestination
import androidx.navigation.NavHostController
import mv.obitech.crundi.ui.shell.Area

/**
 * Route names. An area's front page is its key ("files"). Everything under
 * an area starts with that key and a slash ("files/view?path=..."), so the
 * shell can tell which tab is current from any route. Each area package
 * declares its own detail routes in its own Routes object; only the few that
 * other parts of the app open live here.
 */
object Routes {
    /** One chat, full screen. Opened by notifications, live cards, the approvals list and other areas. */
    const val CHAT = "chats/chat/{chatId}"
    fun chat(chatId: String) = "chats/chat/" + java.net.URLEncoder.encode(chatId, "UTF-8")

    /** The "things waiting on you" list behind the top bar's counter (owned by ui.chats). */
    const val APPROVALS = "chats/approvals"

    /** The area a route belongs to, or null. */
    fun areaOf(route: String?): Area? = Area.byKey(route?.substringBefore('/')?.substringBefore('?'))

    /** True for an area's front page: the shell shows its top bar and tabs there and nowhere else. */
    fun isAreaRoot(route: String?): Boolean = route != null && Area.byKey(route)?.tab == true
}

/** Moving round the app. Get it with LocalNavigator.current. */
class Navigator(private val nav: NavHostController?) {
    /** Go to an area's front page, as tapping its tab does (one entry per area, state kept). */
    fun area(area: Area) {
        val n = nav ?: return
        n.navigate(area.key) {
            popUpTo(n.graph.findStartDestination().id) { saveState = true }
            launchSingleTop = true
            restoreState = true
        }
    }

    /** Open a route on top of the current one. */
    fun to(route: String) { nav?.navigate(route) { launchSingleTop = true } }

    fun chat(chatId: String) = to(Routes.chat(chatId))

    fun back(): Boolean = nav?.popBackStack() ?: false

    companion object {
        /** For tests and previews: goes nowhere. */
        val None = Navigator(null)
    }
}

val LocalNavigator = staticCompositionLocalOf { Navigator.None }

/**
 * Turning a server path (what notifications, live cards and bubbles carry)
 * into a route. Unknown paths give null: the app just opens.
 */
object DeepLinks {
    private val chat = Regex("^/chat/([A-Za-z0-9_-]{1,80})/?$")

    fun route(path: String?): String? {
        if (path.isNullOrEmpty() || !path.startsWith("/") || path.startsWith("//")) return null
        chat.find(path)?.let { return Routes.chat(it.groupValues[1]) }
        // "/?tab=services" and the like: the desktop's own way to point at a tab.
        val tab = path.substringAfter('?', "").split('&').firstOrNull { it.startsWith("tab=") }?.removePrefix("tab=")
        Area.byKey(tab)?.let { return it.key }
        return null
    }
}
