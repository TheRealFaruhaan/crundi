package mv.obitech.crundi.homepool

import android.content.Context
import android.content.Intent
import androidx.core.content.ContextCompat
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import java.text.SimpleDateFormat
import java.util.ArrayDeque
import java.util.Date
import java.util.Locale

/**
 * "Share this phone's connection": the homepool node, as the app's screens see
 * it. One node per app. Used by the Crundi app (Settings › Homepool) and by
 * the standalone Homepool Node app, which compiles this package as it is.
 *
 * The service ([HomepoolNodeService]) does the work; this object holds the
 * settings, starts and stops it, and publishes what it is doing as [snapshot].
 */
object Homepool {
    private val state = MutableStateFlow(NodeSnapshot.EMPTY)

    /** What the node is doing, updated as it changes (and every few seconds while it runs). */
    val snapshot: StateFlow<NodeSnapshot> = state.asStateFlow()

    fun config(ctx: Context): NodeConfig? = NodePrefs(ctx).config

    /**
     * Set the node key (from a scanned or pasted code). Null: stop, autoStart
     * off, forget the key. A new config while running restarts with it.
     */
    fun setConfig(ctx: Context, c: NodeConfig?) {
        val p = NodePrefs(ctx)
        p.refused = null
        if (c == null) {
            p.enabled = false
            p.autoStart = false
            p.config = null
            HomepoolNodeService.instance?.shutdownFromApp()
            NodeRuntime.reset()
            NodeRuntime.log("node key removed from this phone")
            publish(ctx)
            return
        }
        val old = p.config
        p.config = c
        if (old != c) {
            NodeRuntime.reset()
            NodeRuntime.log("node key set for ${c.server}")
        }
        val svc = HomepoolNodeService.instance
        if (p.enabled && svc != null) svc.restartFromApp()
        else if (p.enabled) launch(ctx)
        publish(ctx)
    }

    /** Run the node now (enabled = true): starts the foreground service. Does nothing without a config. */
    fun start(ctx: Context) {
        val p = NodePrefs(ctx)
        if (p.config == null) { NodeRuntime.log("not configured: scan or paste a node code first"); publish(ctx); return }
        p.refused = null
        p.enabled = true
        val svc = HomepoolNodeService.instance
        if (svc != null) svc.reevaluateFromApp() else launch(ctx)
        publish(ctx)
    }

    /** Stop the node now. It stays registered (autoStart) if it was. */
    fun stop(ctx: Context) {
        NodePrefs(ctx).enabled = false
        HomepoolNodeService.instance?.shutdownFromApp()
        NodeRuntime.running = false
        publish(ctx)
    }

    /**
     * "Registered as a service": starts at boot and after an app update, and
     * Android restarts it if it is killed (START_STICKY). On also starts it now;
     * off leaves it running until Stop (the old app's Unregister = this + [stop]).
     */
    fun setAutoStart(ctx: Context, on: Boolean) {
        NodePrefs(ctx).autoStart = on
        if (on) start(ctx) else publish(ctx)
    }

    fun setWifiOnly(ctx: Context, on: Boolean) {
        NodePrefs(ctx).wifiOnly = on
        HomepoolNodeService.instance?.reevaluateFromApp()
        publish(ctx)
    }

    /** Mobile data per day in MB; 0 = no cap. Default 500. */
    fun setMobileCapMb(ctx: Context, mb: Int) {
        NodePrefs(ctx).mobileCapMb = mb
        HomepoolNodeService.instance?.reevaluateFromApp()
        publish(ctx)
    }

    /**
     * Call at app start. A fresh process where the node is not actually
     * running: a registered one is brought back; one only started by hand was
     * ended by Android, so it is recorded as stopped rather than claimed to run.
     */
    fun reconcile(ctx: Context) {
        val p = NodePrefs(ctx)
        if (p.enabled && !NodeRuntime.running && HomepoolNodeService.instance == null) {
            if (p.autoStart && p.config != null && p.refused == null) launch(ctx) else p.enabled = false
        }
        publish(ctx)
    }

    // ── inside the package ──

    internal fun launch(ctx: Context): Boolean {
        NodeRuntime.running = true
        NodeRuntime.phase = NodePhase.CONNECTING
        NodeRuntime.pauseReason = null
        return try {
            ContextCompat.startForegroundService(ctx, Intent(ctx, HomepoolNodeService::class.java).setAction(HomepoolNodeService.ACTION_START))
            true
        } catch (e: Exception) {
            // Android 12+ refuses a foreground service started from the background.
            NodeRuntime.running = false
            NodeRuntime.log("could not start the node: ${e.message ?: e.javaClass.simpleName}")
            false
        }
    }

    /** Rebuild [snapshot] from the settings and what the service is doing. */
    internal fun publish(ctx: Context) {
        val p = NodePrefs(ctx)
        val c = p.config
        val r = NodeRuntime
        val refused = p.refused
        val running = r.running && p.enabled
        val phase = when {
            refused != null -> NodePhase.REFUSED
            !running -> NodePhase.STOPPED
            else -> r.phase
        }
        state.value = NodeSnapshot(
            phase = phase,
            pauseReason = when (phase) { NodePhase.REFUSED -> refused; NodePhase.PAUSED -> r.pauseReason; else -> null },
            networkLabel = r.networkLabel,
            publicIp = r.publicIp,
            connectedSince = if (phase == NodePhase.CONNECTED || phase == NodePhase.PAUSED) r.connectedSince else 0,
            configured = c != null,
            enabled = p.enabled,
            autoStart = p.autoStart,
            wifiOnly = p.wifiOnly,
            mobileCapMb = p.mobileCapMb,
            bytesToday = p.bytesToday,
            mobileBytesToday = p.mobileBytesToday,
            nodeName = r.nodeName,
            networkName = r.networkName,
            server = if (phase == NodePhase.CONNECTED || phase == NodePhase.PAUSED) r.server else null,
            log = r.recentLog(),
            serverUrl = c?.server,
        )
    }
}

/** The service's live state (same process, so a singleton is enough). */
internal object NodeRuntime {
    /** The service is running or being started. */
    @Volatile var running = false
    @Volatile var phase: NodePhase = NodePhase.CONNECTING
    @Volatile var pauseReason: String? = null
    @Volatile var networkLabel: String = "—"
    @Volatile var publicIp: String? = null
    @Volatile var connectedSince: Long = 0
    @Volatile var nodeName: String? = null
    @Volatile var networkName: String? = null
    @Volatile var server: ServerView? = null

    private val lines = ArrayDeque<String>()
    private val fmt = SimpleDateFormat("HH:mm:ss", Locale.US)

    @Synchronized
    fun log(line: String) {
        lines.addLast("${fmt.format(Date())}  $line")
        while (lines.size > 60) lines.removeFirst()
    }

    @Synchronized
    fun recentLog(): List<String> = lines.toList()

    /** A different node (new key, or none): forget what the old one was. */
    fun reset() {
        nodeName = null; networkName = null; server = null; connectedSince = 0
    }
}
