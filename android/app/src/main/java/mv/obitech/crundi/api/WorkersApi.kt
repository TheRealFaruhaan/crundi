package mv.obitech.crundi.api

import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import mv.obitech.crundi.api.model.WorkerCode
import mv.obitech.crundi.api.model.WorkerEvent
import mv.obitech.crundi.api.model.WorkersOverview
import mv.obitech.crundi.api.model.workerHistory

/**
 * Workers, the owner's PCs working for this server (capability
 * [CAPABILITY]; specs/workers-protocol.md, "Server HTTP API"). Owner only:
 * a collaborator gets 403. The server pushes `{type:"workers"}` on the UI
 * websocket whenever anything changes ([CrundiClient.workerChanges]); the
 * screen then asks for the list again.
 */
class WorkersApi internal constructor(private val rpc: Rpc) {
    suspend fun list(): WorkersOverview = rpc.read(rpc.get(PATH), null)

    private suspend fun act(action: String, vararg fields: Pair<String, Any?>) =
        rpc.post(PATH, mapOf("action" to action, *fields))

    /** A join code for a new PC: 8 characters, single use, 10 minutes. */
    suspend fun code(): WorkerCode = rpc.read(act("code"), null)
    suspend fun rename(id: String, name: String) { act("rename", "id" to id, "name" to name) }
    /** Only what is given changes. [labels]: the owner's own labels, the whole list. */
    suspend fun set(id: String, allowChats: Boolean? = null, allowCommands: Boolean? = null, paused: Boolean? = null, labels: List<String>? = null) {
        act("set", "id" to id, "allowChats" to allowChats, "allowCommands" to allowCommands, "paused" to paused, "labels" to labels)
    }
    /** Its key stops working; the worker is kept, shown as revoked. */
    suspend fun revoke(id: String) { act("revoke", "id" to id) }
    /** A new join code for a revoked worker, if the answer carries one. */
    suspend fun reissue(id: String): WorkerCode? {
        val a = act("reissue", "id" to id)
        return (a["code"] as? JsonPrimitive)?.contentOrNull?.let { rpc.read<WorkerCode>(a, null) }
    }
    suspend fun remove(id: String) { act("remove", "id" to id) }
    /** The worker's event log, newest first. */
    suspend fun history(id: String): List<WorkerEvent> = workerHistory(act("history", "id" to id))

    companion object {
        const val PATH = "/api/workers"
        const val CAPABILITY = "workers"
        fun supported(capabilities: Set<String>): Boolean = CAPABILITY in capabilities
    }
}
