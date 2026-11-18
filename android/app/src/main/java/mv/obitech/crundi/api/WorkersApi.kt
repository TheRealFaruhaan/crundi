package mv.obitech.crundi.api

import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import mv.obitech.crundi.api.model.WorkerCode
import mv.obitech.crundi.api.model.WorkerEvent
import mv.obitech.crundi.api.model.WorkerJob
import mv.obitech.crundi.api.model.WorkerJobAnswer
import mv.obitech.crundi.api.model.WorkerJobs
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

    /**
     * The worker's jobs (commands chats ran on it), newest first. Null when
     * the server has no jobs yet (a phase-1 server answers "Unknown action.").
     */
    suspend fun jobs(id: String): List<WorkerJob>? = try {
        rpc.read<WorkerJobs>(act("jobs", "id" to id), null).jobs
    } catch (e: ApiException) {
        if (e.message.startsWith("Unknown action", ignoreCase = true)) null else throw e
    }
    /** One job with the last [tailBytes] of its output in [WorkerJob.tail]. */
    suspend fun jobLog(job: String, tailBytes: Int = JOB_TAIL): WorkerJob? =
        rpc.read<WorkerJobAnswer>(act("job-log", "job" to job, "tailBytes" to tailBytes), null).job
    suspend fun cancelJob(job: String): WorkerJob? = rpc.read<WorkerJobAnswer>(act("job-cancel", "job" to job), null).job

    companion object {
        /** How much of a job's output the page shows: 64 KB, as the web. */
        const val JOB_TAIL = 65536
        const val PATH = "/api/workers"
        const val CAPABILITY = "workers"
        fun supported(capabilities: Set<String>): Boolean = CAPABILITY in capabilities
    }
}
