package mv.obitech.crundi

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import mv.obitech.crundi.ui.chats.ChatRoute
import mv.obitech.crundi.ui.shell.Host
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.shell.LocalHost
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiTheme

/**
 * One chat by itself, native: what a bubble opens. The same chat screen as
 * the main window's (ui.chats.ChatRoute), told by LocalHost that it is in a
 * bubble, so its back arrow closes the bubble and it offers "Open in Crundi".
 *
 * It shares the main window's services (AppGraph), so with the app already
 * running it shows the chat without connecting again.
 */
class ChatActivity : ComponentActivity() {
    private var serverId = ""
    private var chatId = ""
    private var openKey = ""

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        serverId = intent.getStringExtra(Notifier.EXTRA_SERVER).orEmpty()
        chatId = intent.getStringExtra(Notifier.EXTRA_CHAT).orEmpty()
        val server = Servers.byId(this, serverId)
        if (server == null || !Regex("^[A-Za-z0-9_-]{1,80}$").matches(chatId)) { finish(); return }
        openKey = "$serverId:$chatId"
        opened(openKey)
        // While any bubble is up, the app stays running and online (Link).
        Link.hold(applicationContext, "bubbles", true)
        // In the list of recent apps this window is the chat, not "Crundi" again.
        intent.getStringExtra(EXTRA_TITLE)?.let {
            if (it.isNotEmpty()) {
                title = it
                @Suppress("DEPRECATION")
                setTaskDescription(android.app.ActivityManager.TaskDescription(it))
            }
        }
        enableEdgeToEdge()
        val services = AppGraph.forServer(this, server)
        setContent {
            CrundiTheme {
                CompositionLocalProvider(LocalAppServices provides services, LocalHost provides host) {
                    Box(Modifier.fillMaxSize().background(CrundiColors.BgPrimary).safeDrawingPadding()) {
                        ChatRoute(chatId)
                    }
                }
            }
        }
    }

    private val host = object : Host {
        override val isBubble = true
        override fun chatOnScreen(chatId: String, onScreen: Boolean) {
            if (onScreen && chatId == this@ChatActivity.chatId) Notifier.clearChat(this@ChatActivity, serverId, chatId, why = "its own window reported it seen")
        }
        override fun openInMain(path: String) = this@ChatActivity.openMain(path)
        override fun close() = finish()
        override fun pairAgain(reason: String) {
            startActivity(Intent(this@ChatActivity, PairActivity::class.java).putExtra(PairActivity.EXTRA_AGAIN, serverId).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        }
        override fun openExternal(url: String) {
            if (!url.startsWith("https://") && !url.startsWith("http://")) return
            runCatching { startActivity(Intent(Intent.ACTION_VIEW, android.net.Uri.parse(url))) }
        }
    }

    private fun openMain(path: String) {
        if (!path.startsWith("/") || path.startsWith("//")) return
        startActivity(
            Intent(this, MainActivity::class.java).setAction("open:$serverId:$path").putExtra(Notifier.EXTRA_SERVER, serverId).putExtra(Notifier.EXTRA_PATH, path)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP),
        )
    }

    /** Any touch or key: the hour before the app lets go of its connection starts again (Link). */
    override fun onUserInteraction() {
        super.onUserInteraction()
        Link.touch(this)
    }

    // Open (not folded away): this chat is in front of the person, and a
    // notification for it is left out (OnScreen).
    override fun onResume() {
        super.onResume()
        if (openKey.isNotEmpty()) OnScreen.bubbleOpen("chat:$openKey", true)
    }

    override fun onPause() {
        if (openKey.isNotEmpty()) OnScreen.bubbleOpen("chat:$openKey", false)
        super.onPause()
    }

    override fun onDestroy() {
        if (openKey.isNotEmpty()) closed(openKey)
        Link.hold(applicationContext, "bubbles", anyOpen())
        super.onDestroy()
    }

    companion object {
        const val EXTRA_TITLE = "title"

        // Which chats have a window of their own right now. Counted, since
        // Android can build a second window for a chat before it has taken
        // the first one down.
        private val open = HashMap<String, Int>()
        private fun opened(key: String) = synchronized(open) { open[key] = (open[key] ?: 0) + 1 }
        private fun closed(key: String) = synchronized(open) { val n = (open[key] ?: 0) - 1; if (n > 0) open[key] = n else open.remove(key) }
        private fun anyOpen() = synchronized(open) { open.isNotEmpty() }
        fun isOpen(serverId: String, chatId: String): Boolean = synchronized(open) { open.containsKey("$serverId:$chatId") }
    }
}
