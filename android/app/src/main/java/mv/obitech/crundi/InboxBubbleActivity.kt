package mv.obitech.crundi

import android.content.Intent
import android.os.Bundle
import android.widget.Toast
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import mv.obitech.crundi.ui.inbox.InboxThreadRoute
import mv.obitech.crundi.ui.shell.Host
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.shell.LocalHost
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiTheme

/**
 * One inbox thread by itself: what an inbox thread's bubble opens. The same
 * thread screen as the main window's (ui.inbox.InboxThreadRoute), told by
 * LocalHost that it is in a bubble: its back arrow closes the bubble, and
 * "Open chat" opens the chat's own bubble (or the chat in the main window).
 * Apart from the chat's bubble (ChatActivity): another conversation to
 * Android ("inbox:<server>:<thread>", Notifier.showInbox).
 */
class InboxBubbleActivity : androidx.fragment.app.FragmentActivity() {
    private val pinKeeper by lazy { ActivityPinKeeper(this) }
    private var serverId = ""
    private var thread = ""
    private var openKey = ""
    private var open by mutableStateOf<String?>(null)
    private var generation by mutableStateOf(0)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        serverId = intent.getStringExtra(Notifier.EXTRA_SERVER).orEmpty()
        thread = intent.getStringExtra(Notifier.EXTRA_THREAD).orEmpty()
        val server = Servers.byId(this, serverId)
        if (server == null || !Regex("^[A-Za-z0-9_.:@-]{1,200}$").matches(thread)) { finish(); return }
        open = intent.getStringExtra(Notifier.EXTRA_OPEN)?.takeIf { it.isNotEmpty() }
        openKey = "$serverId:$thread"
        opened(openKey)
        Link.hold(applicationContext, "bubbles", true)
        intent.getStringExtra(ChatActivity.EXTRA_TITLE)?.takeIf { it.isNotEmpty() }?.let {
            title = it
            @Suppress("DEPRECATION")
            setTaskDescription(android.app.ActivityManager.TaskDescription(it))
        }
        enableEdgeToEdge()
        val services = AppGraph.forServer(this, server)
        setContent {
            CrundiTheme {
                CompositionLocalProvider(LocalAppServices provides services, LocalHost provides host, LocalPinKeeper provides pinKeeper) {
                    Box(Modifier.fillMaxSize().background(CrundiColors.BgPrimary).safeDrawingPadding()) {
                        // A new item named while open (the bubble's notification again): the thread again, at that item.
                        androidx.compose.runtime.key(generation) { InboxThreadRoute(thread, open) }
                        mv.obitech.crundi.ui.shell.KeyboardBackCloses()
                    }
                }
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        if (intent.getStringExtra(Notifier.EXTRA_THREAD) != thread) return
        open = intent.getStringExtra(Notifier.EXTRA_OPEN)?.takeIf { it.isNotEmpty() }
        generation++
    }

    private val host = object : Host {
        override val isBubble = true
        override fun inboxOnScreen(thread: String, onScreen: Boolean) {
            if (onScreen && thread == this@InboxBubbleActivity.thread) Notifier.clearInbox(this@InboxBubbleActivity, serverId, thread)
        }
        override fun openChat(chatId: String, title: String, project: String): Boolean {
            if (!Regex("^[A-Za-z0-9_-]{1,80}$").matches(chatId)) return true
            // The chat's own bubble, as its notifications open it; the main window when bubbles cannot.
            val mark = mv.obitech.crundi.ui.chats.projectMark(project.ifEmpty { title })
            fun hex(c: androidx.compose.ui.graphics.Color) = "#%06X".format(c.toArgb() and 0xFFFFFF)
            val icon = AppGraph.forServer(this@InboxBubbleActivity, Servers.byId(this@InboxBubbleActivity, serverId) ?: return true)
                .shell.state.value.projects.firstOrNull { it.alias == project || it.name == project }?.icon.orEmpty()
            val chat = ChatRef(serverId, chatId, title.ifEmpty { "Chat" }, project, mark.initials, hex(mark.color), hex(mark.colorDeep), icon)
            val problem = if (Notifier.bubblesOn(this@InboxBubbleActivity)) Notifier.bubble(this@InboxBubbleActivity, chat, expand = true) else "off"
            if (problem != null) openMain("/chat/$chatId")
            return true
        }
        override fun openInMain(path: String) = this@InboxBubbleActivity.openMain(path)
        override fun close() = finish()
        override fun pairAgain(reason: String) {
            startActivity(Intent(this@InboxBubbleActivity, PairActivity::class.java).putExtra(PairActivity.EXTRA_AGAIN, serverId).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        }
        override fun openExternal(url: String) {
            if (!url.startsWith("https://") && !url.startsWith("http://")) return
            runCatching { startActivity(Intent(Intent.ACTION_VIEW, android.net.Uri.parse(url))) }
                .onFailure { Toast.makeText(this@InboxBubbleActivity, "No app here opens that address.", Toast.LENGTH_SHORT).show() }
        }
    }

    private fun openMain(path: String) {
        if (!path.startsWith("/") || path.startsWith("//")) return
        startActivity(
            Intent(this, MainActivity::class.java).setAction("open:$serverId:$path").putExtra(Notifier.EXTRA_SERVER, serverId).putExtra(Notifier.EXTRA_PATH, path)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP),
        )
    }

    override fun onUserInteraction() {
        super.onUserInteraction()
        Link.touch(this)
    }

    // Open (not folded away): this thread is in front of the person (OnScreen).
    override fun onResume() {
        super.onResume()
        if (openKey.isNotEmpty()) OnScreen.bubbleOpen("inbox:$openKey", true)
    }

    override fun onPause() {
        if (openKey.isNotEmpty()) OnScreen.bubbleOpen("inbox:$openKey", false)
        super.onPause()
    }

    override fun onDestroy() {
        if (openKey.isNotEmpty()) closed(openKey)
        Link.hold(applicationContext, "bubbles", anyOpen() || ChatActivity.anyOpenNow())
        super.onDestroy()
    }

    companion object {
        private val open = HashMap<String, Int>()
        private fun opened(key: String) = synchronized(open) { open[key] = (open[key] ?: 0) + 1 }
        private fun closed(key: String) = synchronized(open) { val n = (open[key] ?: 0) - 1; if (n > 0) open[key] = n else open.remove(key) }
        fun anyOpen() = synchronized(open) { open.isNotEmpty() }
        fun isOpen(serverId: String, thread: String): Boolean = synchronized(open) { open.containsKey("$serverId:$thread") }
    }
}
