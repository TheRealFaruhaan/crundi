package mv.obitech.crundi.api.model

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

// ─── Homepool (capability "homepool"; docs/homepool.md) ───
// GET /api/homepool answers the overview (src/homepool.js overview() and
// keyUsage in src/homepool-service.js). Everything has a default: the server
// adds fields freely and an older one leaves some out.

/** The server's own listeners: SOCKS5 and HTTP proxy ports, the port-block range, the addresses they bind. */
@Serializable data class HpPorts(val socks: Int = 0, val http: Int = 0, val blocks: List<Int> = emptyList(), val hosts: List<String> = emptyList())

@Serializable data class HpPoolNode(val id: String = "", val name: String = "")

/** The standalone homepool relay as the server last saw it (the "pool" network). */
@Serializable data class HpPool(val up: Boolean = false, val liveCount: Int = 0, val nodes: List<HpPoolNode> = emptyList(), val checkedAt: Long = 0, val error: String = "", val host: String = "")

/** A network: "server" and "pool" are built in; the rest are the owner's. [live]: exits up in it now. */
@Serializable
data class HpNetwork(
    val id: String,
    val name: String = id,
    val builtin: Boolean = false,
    val live: Int = 0,
    val createdAt: Long = 0,
    /** The pool network only: its relay, and whether the bridge to it is on. */
    val pool: HpPool? = null,
    val enabled: Boolean = true,
)

/** What a connected node says of itself. */
@Serializable
data class HpNodeInfo(
    val platform: String = "", val app: String = "", val version: String = "", val network: String = "",
    val publicIp: String = "", val paused: Boolean = false, val pauseReason: String = "",
    val bytesToday: Long = 0, val mobileBytesToday: Long = 0, val mobileCapMb: Int = 0,
)

/** Today's use, as the server counts it (keys: also refused and last used). */
@Serializable data class HpUsage(val bytes: Long = 0, val conns: Int = 0, val refused: Int = 0, val lastUsedAt: Long = 0)

/** A home node. [network] "" is unassigned (carries nothing until moved). [kind]: standalone or crundi-app. */
@Serializable
data class HpNode(
    val id: String,
    val name: String = id,
    val network: String = "",
    val kind: String = "standalone",
    val deviceId: String = "",
    val createdAt: Long = 0,
    val lastSeenAt: Long = 0,
    val enabled: Boolean = true,
    val platform: String = "",
    val up: Boolean = false,
    val paused: Boolean = false,
    val info: HpNodeInfo? = null,
    val connectedAt: Long = 0,
    /** Open streams through it now. */
    val streams: Int = 0,
    /** Slots bound to it now. */
    val slots: Int = 0,
    val usage: HpUsage = HpUsage(),
)

@Serializable data class HpKeyPorts(val base: Int = 0, val status: Int = 0)

/** A port block's lines: for software that cannot send a proxy login. */
@Serializable
data class HpBlockUse(
    val any: String = "", val slots: String = "", val status: String = "",
    val from: List<String> = emptyList(), val compose: List<String> = emptyList(),
)

/** How software uses a key ("<secret>" in place of the secret except right after add or reissue). */
@Serializable
data class HpKeyUse(
    val socks: String = "", val http: String = "", val status: String = "", val login: String = "", val fromDocker: String = "",
    val block: HpBlockUse? = null,
)

@Serializable data class HpSlot(val slot: String = "", val node: String = "", val boundAt: String = "", val rotatesInMs: Long = 0)

/** The relay-compatible status of a key (what its /status answers). */
@Serializable
data class HpKeyStatus(
    val liveCount: Int = 0, val egressAvailable: Boolean = false, val allowFallback: Boolean = false,
    val fallbackInUse: Boolean = false, val refusal: String = "", val slots: List<HpSlot> = emptyList(),
)

/** An access key (a grant). [kind]: chat (one chat), project (every chat of one project) or software (a named login). */
@Serializable
data class HpKey(
    val id: String,
    val kind: String = "software",
    val name: String = "",
    val chatId: String = "",
    val project: String = "",
    val networks: List<String> = emptyList(),
    val fallbackToServer: Boolean = false,
    val enabled: Boolean = true,
    /** Epoch ms; null never. */
    val expiresAt: Long? = null,
    val dailyCapMb: Int = 0,
    val slots: Int = 8,
    val portBlock: Boolean = false,
    val allowFrom: List<String> = emptyList(),
    val createdAt: Long = 0,
    val createdBy: String = "",
    val requestId: String = "",
    val ports: HpKeyPorts? = null,
    val usage: HpUsage = HpUsage(),
    /** Why it cannot be used now ("" when it can): off, expired, cap used up. */
    val refusal: String = "",
    /** Connections open through it now. */
    val open: Int = 0,
    val status: HpKeyStatus? = null,
    val use: HpKeyUse? = null,
)

/** A chat asking for access. [status]: pending, approved or refused. */
@Serializable
data class HpRequest(
    val id: String,
    val status: String = "pending",
    val chatId: String = "",
    val project: String = "",
    val kind: String = "chat",
    val name: String = "",
    val networks: List<String> = emptyList(),
    val reason: String = "",
    val createdAt: Long = 0,
    val decidedAt: Long = 0,
    val keyId: String = "",
) {
    val pending: Boolean get() = status == "pending"
}

@Serializable data class HpPoolBridge(val enabled: Boolean = true, val statusUrl: String = "", val relayHost: String = "", val relayPort: Int = 0)
@Serializable data class HpSettings(val bindTtlMs: Long = 0, val poolBridge: HpPoolBridge = HpPoolBridge())

/** One line of the change log: [what] and whatever fields came with it (node, key, network, by...). */
data class HpLogEntry(val at: Long, val what: String, val fields: Map<String, String>)

/** GET /api/homepool: everything the owner's screen shows. */
@Serializable
data class HomepoolOverview(
    val ports: HpPorts = HpPorts(),
    val networks: List<HpNetwork> = emptyList(),
    val nodes: List<HpNode> = emptyList(),
    val keys: List<HpKey> = emptyList(),
    /** Newest first. */
    val requests: List<HpRequest> = emptyList(),
    val settings: HpSettings = HpSettings(),
    /** Newest first, as the server sends it (read with [entries]). */
    val log: List<JsonObject> = emptyList(),
    val usageDay: String = "",
    val publicUrl: String = "",
) {
    val pool: HpNetwork? get() = networks.firstOrNull { it.id == POOL }
    /** The owner's own networks (not Server, not the standalone pool). */
    val own: List<HpNetwork> get() = networks.filter { !it.builtin && it.id != SERVER && it.id != POOL }
    val pending: List<HpRequest> get() = requests.filter { it.pending }

    fun networkName(id: String): String = when {
        id.isEmpty() -> "Unassigned"
        else -> networks.firstOrNull { it.id == id }?.name ?: id
    }

    fun nodeName(id: String): String = nodes.firstOrNull { it.id == id }?.name ?: id
    fun keyName(id: String): String = keys.firstOrNull { it.id == id }?.name?.ifEmpty { null } ?: id

    val entries: List<HpLogEntry> get() = log.map { o ->
        val fields = o.filterKeys { it != "at" && it != "what" }.mapValues { (_, v) ->
            when (v) {
                is JsonPrimitive -> v.contentOrNull ?: ""
                is kotlinx.serialization.json.JsonArray -> v.joinToString(", ") { (it as? JsonPrimitive)?.contentOrNull ?: it.toString() }
                else -> v.toString()
            }
        }
        HpLogEntry((o["at"] as? JsonPrimitive)?.contentOrNull?.toDoubleOrNull()?.toLong() ?: 0L, (o["what"] as? JsonPrimitive)?.contentOrNull ?: "", fields)
    }

    companion object {
        const val SERVER = "server"
        const val POOL = "pool"
    }
}

/** A node's key, shown once: [code] is the crundi-homepool:// code (empty when the server has no public address), [docker] the one-liner. */
@Serializable data class HpNodeSetup(val id: String = "", val key: String = "", val code: String = "", val server: String = "", val docker: String = "")

/**
 * The answer to POST /api/homepool: [overview] after the change, and what the
 * action made ([node] for node-add, self-node, node-reissue; [key], [secret]
 * and [use] for key-add and key-reissue; [id] for network-add).
 */
@Serializable
data class HpAnswer(
    val overview: HomepoolOverview? = null,
    val id: String = "",
    val node: HpNodeSetup? = null,
    val key: HpKey? = null,
    val secret: String = "",
    val use: HpKeyUse? = null,
    val request: HpRequest? = null,
)

/**
 * The fields of an access key, as the add-access form makes them (key-add,
 * key-update, and the changes of request-decide). The server cleans them
 * (cleanKeyInput in src/homepool-store.js).
 */
data class HpKeyInput(
    val kind: String = "software",
    val name: String = "",
    val chatId: String = "",
    val project: String = "",
    val networks: List<String> = listOf(HomepoolOverview.SERVER),
    val fallbackToServer: Boolean = false,
    /** Epoch ms; null never. */
    val expiresAt: Long? = null,
    val dailyCapMb: Int = 0,
    val slots: Int = 8,
    val portBlock: Boolean = false,
    val allowFrom: List<String> = emptyList(),
) {
    /** The whole key, for key-add and key-update. */
    fun toMap(): Map<String, Any?> = mapOf(
        "kind" to kind, "name" to name, "chatId" to chatId, "project" to project, "networks" to networks,
        "fallbackToServer" to fallbackToServer, "expiresAt" to expiresAt, "dailyCapMb" to dailyCapMb, "slots" to slots,
        "portBlock" to portBlock, "allowFrom" to allowFrom,
    )

    /** What request-decide may change (the chat and project stay the request's own). */
    fun toChanges(): Map<String, Any?> = mapOf(
        "kind" to kind, "name" to name, "networks" to networks, "fallbackToServer" to fallbackToServer, "expiresAt" to expiresAt,
        "dailyCapMb" to dailyCapMb, "slots" to slots, "portBlock" to portBlock, "allowFrom" to allowFrom,
    )

    companion object {
        fun of(k: HpKey) = HpKeyInput(k.kind, k.name, k.chatId, k.project, k.networks, k.fallbackToServer, k.expiresAt, k.dailyCapMb, k.slots, k.portBlock, k.allowFrom)
        fun of(r: HpRequest) = HpKeyInput(kind = r.kind, name = r.name, chatId = r.chatId, project = r.project, networks = r.networks.ifEmpty { listOf(HomepoolOverview.SERVER) })
    }
}
