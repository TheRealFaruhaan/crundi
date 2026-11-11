package mv.obitech.crundi.homepool

/**
 * What the node is doing.
 *   STOPPED     not running (never started, or stopped)
 *   CONNECTING  running, trying to reach the server (backing off between tries)
 *   CONNECTED   running, the server can route through it
 *   PAUSED      running, but the network rules say no (no network, VPN, Wi-Fi
 *               only, mobile cap); see [NodeSnapshot.pauseReason]
 *   REFUSED     the server refused the key (revoked, removed, switched off for
 *               good): not retried until a new code is set or Start is pressed;
 *               the reason is in [NodeSnapshot.pauseReason]
 */
enum class NodePhase { STOPPED, CONNECTING, CONNECTED, PAUSED, REFUSED }

/** One node's setup: the Crundi server and the node key (hpn_<id>.<secret>). */
data class NodeConfig(val server: String, val key: String) {
    /** The "n..." id part of the key, or "" if the key is not of that form. */
    val nodeId: String
        get() = Regex("^hpn_(n[0-9a-f]+)\\.").find(key)?.groupValues?.get(1).orEmpty()

    /** The code this config is shared as (what [parse] reads). */
    fun code(): String = NodeCode.make(server, key)

    override fun toString(): String = "NodeConfig(server=$server, node=$nodeId)"   // never the key

    companion object {
        /** crundi-homepool://node?s=<url-encoded server>&k=<key>; null if it is not one. */
        fun parse(code: String): NodeConfig? = NodeCode.parse(code)?.let { NodeConfig(it.first, it.second) }
    }
}

/** The server's view of this node, from its "stats" message (every ~10 s). */
data class ServerView(val conns: Int, val slots: Int, val bytesToday: Long, val connsToday: Int, val keys: List<String>)

data class NodeSnapshot(
    val phase: NodePhase,
    /** Why it is PAUSED, or why it was REFUSED; null otherwise. */
    val pauseReason: String?,
    /** "Wi-Fi", "Mobile data", "Ethernet", "VPN", "No network", "Other", or "-" before the first look. */
    val networkLabel: String,
    val publicIp: String?,
    /** When the websocket came up (epoch ms), 0 when not connected. */
    val connectedSince: Long,
    /** A node key is set. */
    val configured: Boolean,
    /** The node should be running (Start pressed, not Stopped since). */
    val enabled: Boolean,
    /** "Registered": starts at boot and after an update, and Android restarts it if it is killed. */
    val autoStart: Boolean,
    val wifiOnly: Boolean,
    /** Mobile data cap per day in MB; 0 = no cap. */
    val mobileCapMb: Int,
    /** Bytes carried for the pool today (both directions, counted on the node's own streams). */
    val bytesToday: Long,
    /** The part of [bytesToday] carried on mobile data. */
    val mobileBytesToday: Long,
    /** The node's name and network as the server knows them (from its "hello"). */
    val nodeName: String?,
    val networkName: String?,
    val server: ServerView?,
    /** The recent activity log, oldest first, "HH:mm:ss  message". */
    val log: List<String>,
    val serverUrl: String?,
) {
    /** "Connected", "Connecting…", "Paused — <reason>", "Stopped", "Refused — <reason>". */
    fun label(): String = when (phase) {
        NodePhase.CONNECTED -> "Connected"
        NodePhase.CONNECTING -> "Connecting…"
        NodePhase.PAUSED -> "Paused — ${pauseReason.orEmpty()}"
        NodePhase.STOPPED -> "Stopped"
        NodePhase.REFUSED -> "Refused — ${pauseReason ?: "the server refused this node key"}"
    }

    companion object {
        val EMPTY = NodeSnapshot(
            phase = NodePhase.STOPPED, pauseReason = null, networkLabel = "—", publicIp = null, connectedSince = 0,
            configured = false, enabled = false, autoStart = false, wifiOnly = false, mobileCapMb = DEFAULT_MOBILE_CAP_MB,
            bytesToday = 0, mobileBytesToday = 0, nodeName = null, networkName = null, server = null, log = emptyList(),
            serverUrl = null,
        )
    }
}

const val DEFAULT_MOBILE_CAP_MB = 500

/** 1.2 MB style, as the old node app showed data. */
fun formatBytes(b: Long): String = when {
    b >= 1024L * 1024 * 1024 -> String.format(java.util.Locale.US, "%.2f GB", b / 1073741824.0)
    b >= 1024L * 1024 -> String.format(java.util.Locale.US, "%.1f MB", b / 1048576.0)
    b >= 1024 -> String.format(java.util.Locale.US, "%.0f KB", b / 1024.0)
    else -> "$b B"
}

/** 3h 12m style. */
fun formatDuration(ms: Long): String {
    val s = (ms / 1000).coerceAtLeast(0)
    return when {
        s >= 86400 -> "${s / 86400}d ${(s % 86400) / 3600}h"
        s >= 3600 -> "${s / 3600}h ${(s % 3600) / 60}m"
        s >= 60 -> "${s / 60}m ${s % 60}s"
        else -> "${s}s"
    }
}
