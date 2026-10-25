package mv.obitech.crundi

import android.Manifest
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.nav.DeepLinks
import mv.obitech.crundi.ui.shell.CrundiApp
import mv.obitech.crundi.ui.shell.CrundiMark
import mv.obitech.crundi.ui.shell.Host
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.CrundiTheme

/**
 * The app itself: the native Crundi UI for the current server.
 *
 * Before anything is shown the app asks the server who it is (the handshake)
 * and each side checks the other is not too old. Only then is the UI shown.
 * A tapped notification or live card arrives here with a server path
 * (/chat/<id>), which is turned into a route (DeepLinks) and opened.
 */
class MainActivity : ComponentActivity() {
    private var server: Server? = null
    /** What is on screen: null while connecting, a Notice, or Ready. */
    private var screen by mutableStateOf<Screen>(Screen.Connecting(""))
    /** A route to open once the UI is up (from a notification). */
    private var pendingRoute by mutableStateOf<String?>(null)
    private var caughtUpAt = 0L

    private sealed interface Screen {
        data class Connecting(val host: String) : Screen
        data class Notice(val text: String, val action: String?, val onAction: (() -> Unit)?) : Screen
        data class Ready(val server: Server) : Screen
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        take(intent)
        setContent {
            CrundiTheme {
                Box(Modifier.fillMaxSize().background(CrundiColors.BgPrimary).safeDrawingPadding()) {
                    when (val s = screen) {
                        is Screen.Connecting -> LoadingState(text = if (s.host.isEmpty()) null else "Connecting to ${s.host}")
                        is Screen.Notice -> NoticeScreen(s.text, s.action, s.onAction) { startActivity(Intent(this@MainActivity, PairActivity::class.java)) }
                        is Screen.Ready -> androidx.compose.runtime.CompositionLocalProvider(
                            mv.obitech.crundi.ui.workbench.LocalWorkbenchStore provides androidx.compose.runtime.remember(s.server.id) { mv.obitech.crundi.ui.workbench.WorkbenchStore.forServer(this@MainActivity, s.server.id) },
                        ) {
                            CrundiApp(
                                services = AppGraph.forServer(this@MainActivity, s.server),
                                host = host,
                                openRoute = pendingRoute,
                                onOpened = { pendingRoute = null },
                            )
                        }
                    }
                }
            }
        }
        open()
    }

    /** Any touch or key: the hour before the app lets go of its connection starts again (Link). */
    override fun onUserInteraction() {
        super.onUserInteraction()
        Link.touch(this)
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        OnScreen.mainFocused = hasFocus
    }

    /**
     * Each time the app comes to the front: ask the server which chats are at
     * work. The live card is otherwise drawn only from messages sent as a
     * state changes, and a phone that missed one showed nothing for that chat.
     */
    override fun onResume() {
        super.onResume()
        // Back from one of Android's pages the urgent alert setup opened: the next one, if any.
        UrgentSetup.next(this)
        val s = server ?: return
        val now = System.currentTimeMillis()
        if (now - caughtUpAt < 10_000) return
        caughtUpAt = now
        val app = applicationContext
        Bg.run({ LiveCards.catchUp(app, s.id) }) { }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        take(intent)
        // Back from pairing, or sent to another server by a notification.
        if (server?.id != Servers.current(this)?.id) open()
    }

    /** Read where an intent wants to go: which server, and which of its pages. */
    private fun take(intent: Intent?) {
        val id = intent?.getStringExtra(Notifier.EXTRA_SERVER).orEmpty()
        val path = intent?.getStringExtra(Notifier.EXTRA_PATH).orEmpty()
        if (id.isNotEmpty() && Servers.byId(this, id) != null) Servers.setCurrent(this, id)
        DeepLinks.route(path)?.let { pendingRoute = it }
    }

    /** Show the current server, or the pairing screen if there is none. */
    private fun open() {
        val s = Servers.current(this)
        if (s == null) {
            startActivity(Intent(this, PairActivity::class.java))
            finish()
            return
        }
        server = s
        screen = Screen.Connecting(s.host)
        Bg.run({ Api.handshake(s.baseUrl) }) { result ->
            if (server?.id != s.id || isFinishing) return@run
            val reply = result.getOrNull()
            val verdict = reply?.json?.let { Gate.judge(it, BuildConfig.VERSION_NAME, BuildConfig.MIN_SERVER) }
            when {
                reply == null -> notice("Could not reach ${s.host}.\n\nCheck that this phone is online and the server is running.", "Try again") { open() }
                verdict == null -> notice("${s.host} did not answer as a Crundi server.", "Try again") { open() }
                verdict.appTooOld && verdict.serverTooOld -> notice(
                    "This app and this server are each too old for the other.\n\nThe server is ${verdict.serverVersion.ifEmpty { "an unknown version" }} and this app needs ${BuildConfig.MIN_SERVER} or newer. The server in turn needs a newer app${if (verdict.minApp.isNotEmpty()) " (${verdict.minApp} or newer)" else ""}.\n\nUpdate both.",
                    Updater.LABEL,
                ) { Updater.open(this) }
                verdict.appTooOld -> notice(
                    "Update required\n\nThis app is version ${BuildConfig.VERSION_NAME}. This server needs ${verdict.minApp.ifEmpty { "a newer version" }}${if (verdict.minApp.isNotEmpty()) " or newer" else ""}.",
                    Updater.LABEL,
                ) { Updater.open(this) }
                verdict.serverTooOld -> notice(
                    "This server needs updating\n\n${s.host} is version ${verdict.serverVersion.ifEmpty { "unknown" }}. This app needs ${BuildConfig.MIN_SERVER} or newer.\n\nUpdate the server from its own Settings, on a computer. If it is not your server, ask its owner.",
                    "Try again",
                ) { open() }
                else -> {
                    // What this server can do, for the API client to read.
                    ServerCaps.set(applicationContext, s.id, verdict.capabilities)
                    ServerCaps.setVersion(applicationContext, s.id, verdict.serverVersion)
                    // The login a pairing has just handed over for the UI is
                    // taken into the app's keeping (PageLogin), where the API
                    // client renews it, one renewal at a time.
                    if (s.pageToken.isNotEmpty()) {
                        PageLogin.adopt(applicationContext, s)
                        s.pageToken = ""; s.pageRefresh = ""
                        Servers.save(this, s)
                    }
                    screen = Screen.Ready(s)
                    askToNotify()
                    offerUrgentSetup()
                    Push.registerAll(this)
                }
            }
        }
    }

    private fun notice(text: String, action: String?, onAction: (() -> Unit)?) {
        screen = Screen.Notice(text, action, onAction)
    }

    /** Android 13 and later ask the person before an app may notify. Asked once, when there is a server to be notified by. */
    private fun askToNotify() {
        if (!Push.available || Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        if (checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED) return
        val prefs = getSharedPreferences("crundi", Context.MODE_PRIVATE)
        if (prefs.getBoolean("asked.notify", false)) return
        prefs.edit().putBoolean("asked.notify", true).apply()
        requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), REQ_NOTIFY)
    }

    /** Notifications allowed (or nothing to ask): the one-time urgent alert setup follows. */
    private fun offerUrgentSetup() {
        if (!Push.available) return
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) return
        UrgentSetup.offer(this)
    }

    @Deprecated("The platform call is what requestPermissions answers to")
    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == REQ_NOTIFY) offerUrgentSetup()
    }

    // Until when "this chat is on screen" is ignored: a bubble was just asked for.
    @Volatile private var bubbleAskedUntil = 0L

    /** What the native screens ask of this window (the old page bridge, CrundiNative). */
    private val host = object : Host {
        override fun chatOnScreen(chatId: String, onScreen: Boolean) {
            val s = server ?: return
            if (!onScreen || !Regex("^[A-Za-z0-9_-]{1,80}$").matches(chatId)) return
            // The bubble was asked for a moment ago, from this very screen:
            // the app is on its way out, and this is the last look.
            if (System.currentTimeMillis() < bubbleAskedUntil) return
            // With a bubble open over it this window is still drawn but nobody
            // is reading it: the bubble has the focus.
            if (!hasWindowFocus()) { Notifier.traceOnce(this@MainActivity, "chat:${s.id}:$chatId".hashCode(), "$chatId is on screen, but the app window does not have the focus"); return }
            OnScreen.seenInMain("chat:${s.id}:$chatId")
            Notifier.clearChat(this@MainActivity, s.id, chatId, evenBubble = true, why = "the same chat is open in the main window")
        }

        override fun openBubble(chatId: String) {
            withChat(chatId) { chat ->
                if (BubbleHelp.blocked(this@MainActivity)) { BubbleHelp.show(this@MainActivity); return@withChat }
                val problem = Notifier.bubble(this@MainActivity, chat)
                if (problem != null) { Toast.makeText(this@MainActivity, problem, Toast.LENGTH_LONG).show(); return@withChat }
                // A chat open in this window loses its bubble, so the app steps aside.
                bubbleAskedUntil = System.currentTimeMillis() + 6000
                moveTaskToBack(true)
            }
        }

        override fun openInMain(path: String) { DeepLinks.route(path)?.let { pendingRoute = it } }
        override fun close() { moveTaskToBack(true) }
        override fun pairAgain(reason: String) { this@MainActivity.pairAgain(reason) }
        override fun pairAnother() { startActivity(Intent(this@MainActivity, PairActivity::class.java)) }
        override fun openAppSettings() { startActivity(Intent(this@MainActivity, SettingsActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NO_USER_ACTION)) }
        override fun openExternal(url: String) {
            if (!url.startsWith("https://") && !url.startsWith("http://")) return
            try { startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url))) } catch (e: ActivityNotFoundException) { /* nothing can open it */ }
        }
    }

    /** Look a chat up on the server, for its name and mark, then hand it on. */
    private fun withChat(chatId: String, then: (ChatRef) -> Unit) {
        val s = server ?: return
        if (!Regex("^[A-Za-z0-9_-]{1,80}$").matches(chatId)) return
        Bg.run({
            val r = Api.authed(applicationContext, s.id, "GET", "/api/ui-sessions")
            // While still off the main thread: have the project's icon kept,
            // so the bubble's head is drawn with it the first time.
            r.json?.optJSONArray("sessions")?.let { list ->
                for (i in 0 until list.length()) {
                    val x = list.optJSONObject(i) ?: continue
                    if (x.optString("id") == chatId) x.optJSONObject("mark")?.optString("icon")?.takeIf { it.isNotEmpty() }?.let { runCatching { Icons.ensure(applicationContext, s.id, it) } }
                }
            }
            r
        }) { result ->
            val sessions = result.getOrNull()?.json?.optJSONArray("sessions")
            var found: ChatRef? = null
            if (sessions != null) for (i in 0 until sessions.length()) {
                val x = sessions.getJSONObject(i)
                if (x.optString("id") == chatId) found = ChatRef.from(s.id, org.json.JSONObject().put("id", chatId).put("title", x.optString("longTitle").ifEmpty { x.optString("title") }).put("project", x.optString("project")).put("mark", x.optJSONObject("mark")))
            }
            if (found != null) { then(found); return@run }
            val reply = result.getOrNull()
            when {
                reply != null && (reply.status == 401 || reply.status == 403) -> pairAgain("This phone is no longer paired with ${s.host}. Pair it again to use bubbles and notifications.")
                sessions == null -> Toast.makeText(this, "Could not reach ${s.host}. Check the connection and try again.", Toast.LENGTH_LONG).show()
                else -> Toast.makeText(this, "That chat is no longer open on the server.", Toast.LENGTH_LONG).show()
            }
        }
    }

    /** Send the person to pair this phone with this server again. */
    private fun pairAgain(why: String = "") {
        val s = server ?: return
        if (why.isNotEmpty()) Toast.makeText(this, why, Toast.LENGTH_LONG).show()
        startActivity(Intent(this, PairActivity::class.java).putExtra(PairActivity.EXTRA_AGAIN, s.id))
    }

    /**
     * The person is leaving the app (home, or another app) while chats are
     * still at work. Put a bubble up for each, closed, so that what is
     * running stays one touch away. Android lets an app raise a bubble only
     * while it is still in front, so it is done here, on the way out.
     */
    override fun onUserLeaveHint() {
        super.onUserLeaveHint()
        val s = server ?: return
        if (Pickers.active) return     // picking a file "leaves" for a moment: that is not leaving
        if (!getSharedPreferences("crundi", Context.MODE_PRIVATE).getBoolean(PREF_BUBBLE_ON_LEAVE, true)) return
        for (chat in LiveCards.active(this, s.id)) Notifier.bubble(this, chat, expand = false)
    }

    companion object {
        private const val REQ_NOTIFY = 4
        const val PREF_BUBBLE_ON_LEAVE = "bubble.onleave"
    }
}

/** A plain statement over the whole screen, with at most one thing to do, and a way to another server. */
@Composable
private fun NoticeScreen(text: String, action: String?, onAction: (() -> Unit)?, onOtherServer: () -> Unit) {
    Box(Modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
        Column(Modifier.widthIn(max = 380.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
            CrundiMark(size = 72.dp)
            Text(text, style = CrundiType.Body, textAlign = TextAlign.Center, modifier = Modifier.padding(top = 8.dp, bottom = 12.dp))
            if (action != null && onAction != null) CrundiButton(action, onAction, kind = ButtonKind.Primary, modifier = Modifier.fillMaxWidth())
            if (action != null) CrundiButton("Connect to another server", onOtherServer, modifier = Modifier.fillMaxWidth())
        }
    }
}

/**
 * Whether a system picker (files, camera) is open on top of the app. Set by
 * whoever launches one, so leaving for it does not put bubbles up.
 */
object Pickers {
    @Volatile var active: Boolean = false
}

/** What each server said it can do in its handshake (Gate.Verdict.capabilities). */
object ServerCaps {
    fun set(ctx: Context, serverId: String, caps: Set<String>) {
        ctx.getSharedPreferences("crundi", Context.MODE_PRIVATE).edit().putStringSet("caps.$serverId", caps).apply()
    }

    fun get(ctx: Context, serverId: String): Set<String> =
        ctx.getSharedPreferences("crundi", Context.MODE_PRIVATE).getStringSet("caps.$serverId", emptySet()) ?: emptySet()

    /** The server's version from the same handshake ("" until one has been made). */
    fun setVersion(ctx: Context, serverId: String, version: String) {
        ctx.getSharedPreferences("crundi", Context.MODE_PRIVATE).edit().putString("version.$serverId", version).apply()
    }

    fun version(ctx: Context, serverId: String): String =
        ctx.getSharedPreferences("crundi", Context.MODE_PRIVATE).getString("version.$serverId", "") ?: ""
}
