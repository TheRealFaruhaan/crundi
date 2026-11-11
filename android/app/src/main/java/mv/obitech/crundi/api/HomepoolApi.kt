package mv.obitech.crundi.api

import mv.obitech.crundi.api.model.HomepoolOverview
import mv.obitech.crundi.api.model.HpAnswer
import mv.obitech.crundi.api.model.HpKeyInput

/**
 * Homepool, the shared exit (capability [CAPABILITY]; docs/homepool.md):
 * the owner's networks, nodes, access keys, requests from chats and the log.
 * Owner only: a collaborator gets 403. Every change answers the overview
 * after it ([HpAnswer.overview]), so the screen does not have to ask again.
 */
class HomepoolApi internal constructor(private val rpc: Rpc) {
    suspend fun overview(): HomepoolOverview = rpc.read(rpc.get(PATH), null)

    private suspend fun act(action: String, vararg fields: Pair<String, Any?>): HpAnswer =
        rpc.read(rpc.post(PATH, mapOf("action" to action, *fields)), null)

    // ── networks ──
    suspend fun addNetwork(name: String) = act("network-add", "name" to name)
    suspend fun renameNetwork(id: String, name: String) = act("network-rename", "id" to id, "name" to name)
    suspend fun removeNetwork(id: String) = act("network-remove", "id" to id)

    // ── nodes ──
    /** A node for a friend or a standalone box: its key comes back once ([HpAnswer.node]). [network] "" is unassigned. */
    suspend fun addNode(name: String, network: String, kind: String = "standalone") = act("node-add", "name" to name, "network" to network, "kind" to kind)
    /** This device's own node, made or given a fresh key (the old one stops working). */
    suspend fun selfNode(deviceId: String, name: String, network: String) = act("self-node", "deviceId" to deviceId, "name" to name, "network" to network)
    /** Only what is given changes. */
    suspend fun updateNode(id: String, name: String? = null, network: String? = null, enabled: Boolean? = null) =
        act("node-update", "id" to id, "name" to name, "network" to network, "enabled" to enabled)
    suspend fun reissueNode(id: String) = act("node-reissue", "id" to id)
    suspend fun removeNode(id: String) = act("node-remove", "id" to id)

    // ── access keys ──
    /** A new key: its secret and the lines to use it come back once. */
    suspend fun addKey(k: HpKeyInput) = act("key-add", "key" to k.toMap())
    suspend fun updateKey(id: String, k: HpKeyInput) = act("key-update", "id" to id, "key" to k.toMap())
    /** Switch a key on or off, nothing else. */
    suspend fun setKeyEnabled(id: String, enabled: Boolean) = act("key-update", "id" to id, "key" to mapOf("enabled" to enabled))
    suspend fun reissueKey(id: String) = act("key-reissue", "id" to id)
    suspend fun removeKey(id: String) = act("key-remove", "id" to id)

    // ── requests from chats ──
    /** Approve as asked ([changes] null), approve with changes, or refuse. */
    suspend fun decide(id: String, approve: Boolean, changes: HpKeyInput? = null) =
        act("request-decide", "id" to id, "approve" to approve, "changes" to (changes?.toChanges() ?: emptyMap<String, Any?>()))

    // ── settings ──
    /** The bridge to the standalone homepool relay (the "pool" network) on or off. */
    suspend fun setPoolBridge(enabled: Boolean) = act("settings", "settings" to mapOf("poolBridge" to mapOf("enabled" to enabled)))

    companion object {
        const val PATH = "/api/homepool"
        const val CAPABILITY = "homepool"
        fun supported(capabilities: Set<String>): Boolean = CAPABILITY in capabilities
    }
}
