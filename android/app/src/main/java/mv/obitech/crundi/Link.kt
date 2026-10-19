package mv.obitech.crundi

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat

/**
 * Keeps the app running, and online, while a chat is in reach: a bubble is
 * up (open or folded away) or a live card is showing.
 *
 * Without this Android freezes a background app and cuts its network after
 * a while, and the pages it holds drop their connection to the server. A tap
 * on the bubble or the card then showed "Reconnecting" before the chat. A
 * foreground service is Android's way of saying the app is doing something
 * the person is relying on: the app stays up, keeps its network, and the
 * pages stay connected, so a tap shows the chat at once.
 *
 * It asks for nothing of its own: no connection, no polling. It only keeps
 * the pages' own connections alive, and stops by itself when the last bubble
 * closes and the last live card ends.
 *
 * Reasons are counted by key ("bubbles", "live"); the service runs while any
 * reason holds. And not for ever: after an hour with nobody touching the app
 * (the main window or a bubble), it lets go even if a bubble or card is still
 * up, and takes hold again at the next touch.
 */
object Link {
    private val reasons = HashSet<String>()
    /** How long without a touch before the app stops holding on. */
    const val IDLE_MS = 60 * 60 * 1000L
    @Volatile private var lastTouch = System.currentTimeMillis()
    /** Let go for being idle; taken up again by the next touch. */
    @Volatile private var idle = false

    fun hold(ctx: Context, key: String, on: Boolean) {
        val app = ctx.applicationContext
        val changed: Boolean
        val any: Boolean
        val first: Boolean
        synchronized(reasons) {
            first = on && reasons.isEmpty()
            changed = if (on) reasons.add(key) else reasons.remove(key)
            any = reasons.isNotEmpty()
        }
        if (!changed) return
        if (first) { lastTouch = System.currentTimeMillis(); idle = false }
        if (any) { if (!idle) start(app) } else { idle = false; app.stopService(Intent(app, LinkService::class.java)) }
    }

    /** The person did something in the app: the hour starts again, and a let-go hold comes back. */
    fun touch(ctx: Context) {
        lastTouch = System.currentTimeMillis()
        if (!idle) return
        idle = false
        val any = synchronized(reasons) { reasons.isNotEmpty() }
        if (any) start(ctx.applicationContext)
    }

    /** Asked by the service now and then: has it been an hour? Then it stops. */
    fun idleNow(): Boolean {
        if (System.currentTimeMillis() - lastTouch < IDLE_MS) return false
        idle = true
        return true
    }

    private fun start(app: Context) {
        try {
            androidx.core.content.ContextCompat.startForegroundService(app, Intent(app, LinkService::class.java))
        } catch (e: Exception) {
            // Android 12+ refuses a foreground service started from the
            // background outside a few allowed moments (a high-priority push,
            // the person in the app). Then the app simply works as before.
            PushLog.add(app, "Could not stay connected in the background: ${e.javaClass.simpleName}")
            synchronized(reasons) { reasons.clear() }
        }
    }

    const val CHANNEL = "link"
}

class LinkService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel(Link.CHANNEL, "Staying connected", NotificationManager.IMPORTANCE_MIN).apply {
            description = "Shown while a bubble or a live card is up, so they open without reconnecting"
            setShowBadge(false)
        })
        val open = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK), PendingIntent.FLAG_IMMUTABLE)
        val n: Notification = NotificationCompat.Builder(this, Link.CHANNEL)
            .setSmallIcon(R.drawable.ic_stat)
            .setContentTitle("Staying connected")
            .setContentText("So a bubble or live card opens without reconnecting")
            .setContentIntent(open)
            .setOngoing(true)
            .setSilent(true)
            .setPriority(NotificationCompat.PRIORITY_MIN)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .build()
        val type = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE else 0
        try {
            ServiceCompat.startForeground(this, NOTIFICATION_ID, n, type)
        } catch (e: Exception) {
            PushLog.add(this, "Could not stay connected: ${e.javaClass.simpleName}")
            stopSelf()
        }
        // Every few minutes: an hour without a touch, and it lets go.
        handler.removeCallbacks(check)
        handler.postDelayed(check, CHECK_MS)
        // Not restarted by Android after it is killed: the reasons it held
        // lived in the process that went with it.
        return START_NOT_STICKY
    }

    private val handler = android.os.Handler(android.os.Looper.getMainLooper())
    private val check: Runnable = object : Runnable {
        override fun run() {
            if (Link.idleNow()) {
                PushLog.add(this@LinkService, "Stopped holding the connection: an hour without a touch")
                stopSelf()
            } else handler.postDelayed(this, CHECK_MS)
        }
    }

    override fun onDestroy() {
        handler.removeCallbacks(check)
        super.onDestroy()
    }

    companion object {
        private const val NOTIFICATION_ID = 0x11CC
        private const val CHECK_MS = 5 * 60 * 1000L
    }
}

/**
 * Which chat is in front of the person right now, so a notification for it
 * can be left out: no bubble, no buzz, for the chat already being read.
 *
 * In the main window, a chat is in front when the window has the focus and
 * its page has said, in the last few seconds, that the chat is on screen (it
 * says so every three seconds while it is). In a bubble, while that bubble is
 * open (resumed); a bubble folded away is not in front.
 */
object OnScreen {
    private val seenInMain = HashMap<String, Long>()   // chat key -> when
    private val bubbles = HashSet<String>()              // chat keys of open bubbles
    @Volatile var mainFocused = false
    private const val FRESH_MS = 7_000L

    fun seenInMain(key: String) = synchronized(this) { seenInMain[key] = System.currentTimeMillis() }
    fun bubbleOpen(key: String, open: Boolean) = synchronized(this) { if (open) bubbles.add(key) else bubbles.remove(key) }

    fun showing(key: String): Boolean = synchronized(this) {
        if (key in bubbles) return true
        val at = seenInMain[key] ?: return false
        mainFocused && System.currentTimeMillis() - at < FRESH_MS
    }
}
