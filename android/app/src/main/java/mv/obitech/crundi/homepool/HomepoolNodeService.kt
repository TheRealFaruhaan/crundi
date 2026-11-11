package mv.obitech.crundi.homepool

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.PowerManager
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicLong

/**
 * Keeps the homepool node connected for as long as it is switched on.
 *
 * A foreground service (with its notification) is what lets it run with the
 * screen off; a partial wake lock and a Wi-Fi lock stop Doze freezing the CPU
 * or the radio under it. It watches the default network and pauses when the
 * rules say so (no network, a VPN in the way, mobile data with Wi-Fi only on,
 * today's mobile cap used up), and reconnects at once when the network
 * changes instead of waiting for the old socket's pings to time out.
 *
 * While paused with a network it stays connected and tells the server
 * paused:true (so the owner sees why), and refuses new streams; with no
 * network at all it disconnects.
 */
class HomepoolNodeService : Service() {

    companion object {
        const val ACTION_START = "mv.obitech.crundi.homepool.START"
        const val ACTION_STOP = "mv.obitech.crundi.homepool.STOP"
        const val CHANNEL_ID = "homepool_node"
        private const val NOTIF_ID = 0x4870   // "Hp"
        private const val TICK_MS = 5_000L
        private const val IP_REFRESH_MS = 10 * 60_000L

        /** The running service, if any. Set and cleared on the main thread. */
        @Volatile internal var instance: HomepoolNodeService? = null
            private set

        private val http: OkHttpClient by lazy { OkHttpClient() }
    }

    private lateinit var prefs: NodePrefs
    private lateinit var cm: ConnectivityManager
    private val main = Handler(Looper.getMainLooper())
    private var wakeLock: PowerManager.WakeLock? = null
    private var wifiLock: WifiManager.WifiLock? = null
    private var foreground = false
    private var lastNotice = ""

    private var link: NodeLink? = null
    private var linkState = NodeLink.State.STOPPED
    private var linkNetwork: Network? = null
    @Volatile private var currentNet: Network? = null
    @Volatile private var currentCaps: NetworkCapabilities? = null
    private var pauseReason: String? = null
    private var lastIpCheck = 0L

    private val pendingTotal = AtomicLong()
    private val pendingMobile = AtomicLong()

    private val netCallback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) {
            currentNet = network
            currentCaps = cm.getNetworkCapabilities(network)
            evaluate()
        }
        override fun onCapabilitiesChanged(network: Network, caps: NetworkCapabilities) {
            currentNet = network
            currentCaps = caps
            evaluate()
        }
        override fun onLost(network: Network) {
            if (network == currentNet) { currentNet = null; currentCaps = null }
            evaluate()
        }
    }

    private val tick = object : Runnable {
        override fun run() {
            flushUsage()
            evaluate()
            maybeRefreshIp()
            main.postDelayed(this, TICK_MS)
        }
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        instance = this
        prefs = NodePrefs(this)
        cm = getSystemService(ConnectivityManager::class.java)
        createChannel()
        runCatching { cm.registerDefaultNetworkCallback(netCallback, main) }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            prefs.enabled = false
            NodeRuntime.log("stopped from the notification")
            shutdown()
            return START_NOT_STICKY
        }
        // START, or a sticky restart after Android killed us (null intent). Every
        // startForegroundService must be followed by startForeground, even one we refuse.
        instance = this
        goForeground()
        if (!foreground || !prefs.enabled || prefs.config == null || prefs.refused != null) {
            if (prefs.enabled && !foreground) prefs.enabled = false
            shutdown()
            return START_NOT_STICKY
        }
        NodeRuntime.running = true
        acquireLocks()
        main.removeCallbacks(tick)
        main.post(tick)
        // Only a registered node asks Android to bring it back if it is killed.
        return if (prefs.autoStart) START_STICKY else START_NOT_STICKY
    }

    override fun onDestroy() {
        main.removeCallbacks(tick)
        runCatching { cm.unregisterNetworkCallback(netCallback) }
        dropLink()
        flushUsage()
        releaseLocks()
        if (instance === this) instance = null
        if (instance == null) {
            NodeRuntime.running = false
            NodeRuntime.connectedSince = 0
            Homepool.publish(this)
        }
        super.onDestroy()
    }

    // ── from Homepool (any thread) ──

    internal fun shutdownFromApp() = main.post { shutdown() }

    internal fun reevaluateFromApp() = main.post {
        if (!prefs.enabled || instance !== this) return@post
        NodeRuntime.running = true
        if (!foreground) goForeground()
        acquireLocks()
        main.removeCallbacks(tick)
        main.post(tick)
    }

    /** A new node key: start again with it. */
    internal fun restartFromApp() = main.post {
        dropLink()
        reevaluateFromApp()
    }

    // ── decisions (main thread) ──

    private fun evaluate() {
        if (Looper.myLooper() != Looper.getMainLooper()) { main.post { evaluate() }; return }
        if (!NodeRuntime.running) return
        val conf = prefs.config
        if (!prefs.enabled || conf == null || prefs.refused != null) return

        val net = currentNet
        val caps = currentCaps
        NodeRuntime.networkLabel = networkLabel(caps)
        val capBytes = prefs.mobileCapMb.toLong() * 1024 * 1024
        val noNetwork = net == null || caps == null || !caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
        val reason = when {
            noNetwork -> "no network"
            caps!!.hasTransport(NetworkCapabilities.TRANSPORT_VPN) -> "a VPN is active (traffic would leave from the VPN, not this connection)"
            isMobile(caps) && prefs.wifiOnly -> "on mobile data, and Wi-Fi only is on"
            isMobile(caps) && capBytes > 0 && prefs.mobileBytesToday >= capBytes -> "today's mobile data cap (${prefs.mobileCapMb} MB) is used up"
            else -> null
        }
        if (reason != pauseReason) {
            if (reason != null) NodeRuntime.log("paused: $reason") else if (pauseReason != null) NodeRuntime.log("resumed")
            pauseReason = reason
        }

        var l = link
        if (noNetwork) {
            // Nothing to connect over: let the socket go, come back when a network does.
            if (l != null && linkState != NodeLink.State.STOPPED) l.stop()
        } else {
            if (l == null) {
                l = newLink(conf)
                link = l
                linkNetwork = net
                l.paused = reason != null
                l.start()
            } else {
                val wasPaused = l.paused
                l.paused = reason != null
                if (linkState == NodeLink.State.STOPPED) { linkNetwork = net; l.start() }
                else if (linkNetwork != null && net != linkNetwork) {
                    // Wi-Fi <-> mobile: the old socket is dead, and waiting for the pings to notice would strand its slots.
                    linkNetwork = net
                    lastIpCheck = 0
                    NodeRuntime.publicIp = null
                    l.kick()
                } else if (wasPaused != l.paused) l.sendInfo()
                if (linkNetwork == null) linkNetwork = net
            }
        }

        NodeRuntime.pauseReason = reason
        NodeRuntime.phase = when {
            reason != null -> NodePhase.PAUSED
            linkState == NodeLink.State.CONNECTED -> NodePhase.CONNECTED
            else -> NodePhase.CONNECTING
        }
        Homepool.publish(this)
        updateNotification()
    }

    private fun newLink(conf: NodeConfig): NodeLink = NodeLink(
        server = conf.server,
        key = conf.key,
        info = { infoMessage() },
        events = object : NodeLink.Events {
            override fun onState(state: NodeLink.State, detail: String) { main.post { onLinkState(state, detail) } }
            override fun onLog(line: String) { NodeRuntime.log(line); main.post { Homepool.publish(this@HomepoolNodeService) } }
            override fun onHello(name: String?, network: String?, networkName: String?) {
                NodeRuntime.nodeName = name
                NodeRuntime.networkName = networkName ?: network
                main.post { Homepool.publish(this@HomepoolNodeService); updateNotification() }
            }
            override fun onStats(msg: JSONObject) {
                val keys = msg.optJSONArray("keys")
                NodeRuntime.server = ServerView(
                    conns = msg.optInt("conns"), slots = msg.optInt("slots"), bytesToday = msg.optLong("bytesToday"),
                    connsToday = msg.optInt("connsToday"),
                    keys = if (keys == null) emptyList() else List(keys.length()) { keys.optString(it) },
                )
                msg.optString("networkName").takeIf { it.isNotEmpty() }?.let { NodeRuntime.networkName = it }
                main.post { Homepool.publish(this@HomepoolNodeService) }
            }
            override fun onBytes(up: Long, down: Long) {
                val n = up + down
                pendingTotal.addAndGet(n)
                if (isMobile(currentCaps)) pendingMobile.addAndGet(n)
            }
        },
    )

    private fun onLinkState(state: NodeLink.State, detail: String) {
        if (link == null) return
        linkState = state
        when (state) {
            NodeLink.State.CONNECTED -> {
                NodeRuntime.connectedSince = System.currentTimeMillis()
                lastIpCheck = 0
                maybeRefreshIp()
            }
            NodeLink.State.REFUSED -> {
                val why = detail.ifEmpty { "the server refused this node key" }
                NodeRuntime.log("refused: $why")
                prefs.refused = why
                prefs.enabled = false
                shutdown()
                return
            }
            else -> NodeRuntime.connectedSince = 0
        }
        evaluate()
    }

    private fun dropLink() {
        val l = link ?: return
        link = null
        linkState = NodeLink.State.STOPPED
        linkNetwork = null
        // close() joins nothing, but OkHttp's close handshake may touch the network: off the main thread.
        Thread({ runCatching { l.close() } }, "homepool-close").start()
    }

    private fun infoMessage(): JSONObject {
        val ver = runCatching { packageManager.getPackageInfo(packageName, 0).versionName }.getOrNull().orEmpty()
        return JSONObject()
            .put("platform", "android")
            .put("app", if (packageName == "mv.obitech.crundi") "crundi" else "homepool-node")
            .put("version", ver)
            .put("network", NodeRuntime.networkLabel)
            .put("publicIp", NodeRuntime.publicIp.orEmpty())
            .put("paused", pauseReason != null)
            .put("pauseReason", pauseReason.orEmpty())
            .put("bytesToday", prefs.bytesToday + pendingTotal.get())
            .put("mobileBytesToday", prefs.mobileBytesToday + pendingMobile.get())
            .put("mobileCapMb", prefs.mobileCapMb)
    }

    // ── data ──

    private fun flushUsage() {
        if (!::prefs.isInitialized) return
        prefs.addUsage(pendingTotal.getAndSet(0), pendingMobile.getAndSet(0))
    }

    // ── public IP (what the pool's traffic actually leaves from) ──

    private fun maybeRefreshIp() {
        if (linkState != NodeLink.State.CONNECTED) return
        val now = System.currentTimeMillis()
        if (now - lastIpCheck < IP_REFRESH_MS) return
        lastIpCheck = now
        Thread({
            val ip = runCatching {
                http.newBuilder().callTimeout(10, TimeUnit.SECONDS).build()
                    .newCall(Request.Builder().url("https://api.ipify.org").build()).execute()
                    .use { r -> if (r.isSuccessful) r.body?.string()?.trim() else null }
            }.getOrNull()?.takeIf { it.isNotEmpty() && it.length < 60 }
            if (ip != null) main.post {
                if (ip != NodeRuntime.publicIp) {
                    NodeRuntime.publicIp = ip
                    NodeRuntime.log("public IP $ip")
                    link?.sendInfo()
                }
                Homepool.publish(this)
                updateNotification()
            }
        }, "homepool-ip").start()
    }

    // ── foreground plumbing ──

    private fun createChannel() {
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL_ID, "Homepool node", NotificationManager.IMPORTANCE_LOW).apply {
                description = "Shows that this phone is sharing its connection, and how."
                setShowBadge(false)
            }
        )
    }

    private fun smallIcon(): Int {
        val id = resources.getIdentifier("ic_homepool_node", "drawable", packageName)
        return if (id != 0) id else android.R.drawable.stat_sys_upload
    }

    private fun noticeText(): Pair<String, String> {
        val s = Homepool.snapshot.value
        val title = "Homepool node — ${if (NodeRuntime.running) s.label() else "Stopped"}"
        val text = buildString {
            append(NodeRuntime.networkLabel)
            NodeRuntime.publicIp?.let { append(" · ").append(it) }
            append(" · ").append(formatBytes(s.bytesToday)).append(" today")
            if (s.mobileBytesToday > 0 || s.mobileCapMb > 0 && isMobile(currentCaps)) {
                append(" (mobile ").append(formatBytes(s.mobileBytesToday))
                if (s.mobileCapMb > 0) append(" of ").append(s.mobileCapMb).append(" MB")
                append(")")
            }
        }
        return title to text
    }

    private fun buildNotification(title: String, text: String): Notification {
        val launch = packageManager.getLaunchIntentForPackage(packageName)
        val open = launch?.let {
            PendingIntent.getActivity(this, 0, it.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        }
        val stop = PendingIntent.getService(
            this, 1, Intent(this, HomepoolNodeService::class.java).setAction(ACTION_STOP), PendingIntent.FLAG_IMMUTABLE
        )
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(smallIcon())
            .setContentTitle(title)
            .setContentText(text)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
            .apply { if (open != null) setContentIntent(open) }
            .addAction(0, "Stop", stop)
            .build()
    }

    private fun goForeground() {
        val (title, text) = noticeText()
        val type = if (Build.VERSION.SDK_INT >= 34) ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE else 0
        try {
            ServiceCompat.startForeground(this, NOTIF_ID, buildNotification(title, text), type)
            foreground = true
            lastNotice = "$title\n$text"
        } catch (e: Exception) {
            // Android 12+: not allowed from the background right now. Nothing to run without it.
            NodeRuntime.log("could not run in the foreground: ${e.message ?: e.javaClass.simpleName}")
            foreground = false
        }
    }

    private fun updateNotification() {
        if (!foreground) return
        val (title, text) = noticeText()
        val key = "$title\n$text"
        if (key == lastNotice) return
        lastNotice = key
        runCatching { getSystemService(NotificationManager::class.java).notify(NOTIF_ID, buildNotification(title, text)) }
    }

    private fun acquireLocks() {
        if (wakeLock == null) runCatching {
            wakeLock = getSystemService(PowerManager::class.java)
                .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "homepool:node")
                .apply { setReferenceCounted(false); acquire() }
        }
        if (wifiLock == null) runCatching {
            @Suppress("DEPRECATION")
            wifiLock = applicationContext.getSystemService(WifiManager::class.java)
                .createWifiLock(WifiManager.WIFI_MODE_FULL_HIGH_PERF, "homepool:wifi")
                .apply { setReferenceCounted(false); acquire() }
        }
    }

    private fun releaseLocks() {
        runCatching { wakeLock?.release() }; wakeLock = null
        runCatching { wifiLock?.release() }; wifiLock = null
    }

    private fun shutdown() {
        main.removeCallbacks(tick)
        dropLink()
        flushUsage()
        releaseLocks()
        NodeRuntime.running = false
        NodeRuntime.pauseReason = null
        NodeRuntime.connectedSince = 0
        pauseReason = null
        if (foreground) ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
        foreground = false
        // No longer "the running node": a Start from now on starts the service afresh.
        if (instance === this) instance = null
        Homepool.publish(this)
        stopSelf()
    }
}

internal fun isMobile(caps: NetworkCapabilities?): Boolean =
    caps != null && caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) &&
        !caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) &&
        !caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET)

internal fun networkLabel(caps: NetworkCapabilities?): String = when {
    caps == null -> "No network"
    caps.hasTransport(NetworkCapabilities.TRANSPORT_VPN) -> "VPN"
    caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> "Wi-Fi"
    caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) -> "Ethernet"
    caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> "Mobile data"
    else -> "Other"
}
