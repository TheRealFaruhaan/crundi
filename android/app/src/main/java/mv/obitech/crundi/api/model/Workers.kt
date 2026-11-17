package mv.obitech.crundi.api.model

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

// ─── Workers (capability "workers"; specs/workers-protocol.md) ───
// GET /api/workers answers { workers:[...], codes:[{code,expiresAt}] }.
// Everything has a default: the server adds fields freely and an older one
// leaves some out. Times may come as epoch ms or as ISO text; [timeMs]
// (State.kt) reads either.

@Serializable data class WorkerCpu(val model: String = "", val cores: Int = 0, val threads: Int = 0)

@Serializable data class WorkerGpu(val vendor: String = "", val name: String = "", val vramMb: Long? = null)

/** Of the worker's work folder. */
@Serializable data class WorkerDisk(val path: String = "", val freeMb: Long = 0, val totalMb: Long = 0)

@Serializable data class WorkerAllow(val chats: Boolean = false, val commands: Boolean = false)

/** What a worker says of its PC (the link's `info` frame). */
@Serializable
data class WorkerInfo(
    val os: String = "",
    val osVersion: String = "",
    val arch: String = "",
    val hostname: String = "",
    val cpu: WorkerCpu? = null,
    val ramMb: Long = 0,
    val ramFreeMb: Long = 0,
    val load: List<Double>? = null,
    val gpus: List<WorkerGpu> = emptyList(),
    val disk: WorkerDisk? = null,
    val workDir: String = "",
    val onBattery: Boolean? = null,
    val idleSec: Long? = null,
    /** Tool name to version, the found ones only (a non-text value beside them, like claudeLoggedIn, is skipped by [toolVersions]). */
    val tools: JsonObject = JsonObject(emptyMap()),
    val claudeLoggedIn: Boolean? = null,
    val allow: WorkerAllow? = null,
    val paused: Boolean = false,
) {
    /** The tools found with their versions, by name. */
    val toolVersions: List<Pair<String, String>>
        get() = tools.entries.mapNotNull { (k, v) -> (v as? JsonPrimitive)?.takeIf { it.isString }?.content?.let { k to it } }.sortedBy { it.first }

    /** RAM in whole GB, rounded down (as the ram:<n>g label). */
    val ramGb: Long get() = ramMb / 1024
}

/** One of the last log lines a worker sent (`log` frames). Read from either an object or a bare string. */
data class WorkerLogLine(val at: Long, val level: String, val text: String)

/** One event of a worker's history (joined, connected, renamed, paused, terminal opened...). */
data class WorkerEvent(val at: Long, val what: String, val fields: Map<String, String>)

private fun JsonObject.text(vararg keys: String): String? = keys.firstNotNullOfOrNull { k -> (this[k] as? JsonPrimitive)?.contentOrNull }

fun workerLogLine(e: JsonElement): WorkerLogLine? = when (e) {
    is JsonPrimitive -> e.contentOrNull?.let { WorkerLogLine(0, "info", it) }
    is JsonObject -> WorkerLogLine(timeMs(e["at"] ?: e["t"] ?: e["time"] ?: e["ts"]), e.text("level") ?: "info", e.text("text", "message", "msg").orEmpty())
    else -> null
}

fun workerEvent(e: JsonElement): WorkerEvent? {
    if (e is JsonPrimitive) return e.contentOrNull?.let { WorkerEvent(0, it, emptyMap()) }
    val o = e as? JsonObject ?: return null
    val timeKeys = setOf("at", "t", "time", "ts")
    val whatKeys = listOf("what", "event", "type", "kind")
    val whatKey = whatKeys.firstOrNull { (o[it] as? JsonPrimitive)?.isString == true }
    val fields = o.entries.filter { it.key !in timeKeys && it.key != whatKey && it.value is JsonPrimitive }
        .associate { it.key to ((it.value as JsonPrimitive).contentOrNull ?: "") }
    return WorkerEvent(timeMs(timeKeys.firstNotNullOfOrNull { o[it] }), whatKey?.let { o.text(it) }.orEmpty(), fields)
}

/** A worker, as GET /api/workers lists it. */
@Serializable
data class Worker(
    val id: String = "",
    val name: String = "",
    /** tray, headless, electron or native. */
    val kind: String = "",
    val version: String = "",
    val online: Boolean = false,
    val connectedSince: JsonElement? = null,
    val lastSeenAt: JsonElement? = null,
    val info: WorkerInfo? = null,
    /** Every label: those the server derives from info, and the owner's own. */
    val labels: List<String> = emptyList(),
    val customLabels: List<String> = emptyList(),
    val allowChats: Boolean = false,
    val allowCommands: Boolean = false,
    val paused: Boolean = false,
    /** Who paused it: "pc" (the tray) or the owner; null when not paused. */
    val pausedBy: String? = null,
    val createdAt: JsonElement? = null,
    val deviceId: String? = null,
    val logs: List<JsonElement> = emptyList(),
    /** Its key was revoked: the worker is kept but cannot link until it joins again. */
    val revoked: Boolean = false,
) {
    val title: String get() = name.ifEmpty { id }
    val connectedMs: Long get() = timeMs(connectedSince)
    val lastSeenMs: Long get() = timeMs(lastSeenAt)
    val createdMs: Long get() = timeMs(createdAt)
    /** The labels the server worked out, without the owner's own. */
    val derivedLabels: List<String> get() = labels.filter { it !in customLabels }
    val logLines: List<WorkerLogLine> get() = logs.mapNotNull(::workerLogLine)
}

/** A join code: 8 characters, single use, 10 minutes. */
@Serializable
data class WorkerCode(val code: String = "", val expiresAt: JsonElement? = null) {
    val expiresMs: Long get() = timeMs(expiresAt)
}

@Serializable
data class WorkersOverview(val workers: List<Worker> = emptyList(), val codes: List<WorkerCode> = emptyList())

/** The history of one worker: whichever array the answer carries ("history", "events", "entries", "log" or "items"). */
fun workerHistory(answer: JsonObject): List<WorkerEvent> {
    val arr = listOf("history", "events", "entries", "log", "items").firstNotNullOfOrNull { answer[it] as? JsonArray } ?: return emptyList()
    return arr.mapNotNull(::workerEvent).sortedByDescending { it.at }
}

/** Custom labels as the owner types them: comma separated, [a-z0-9:._-], up to 20. Null with the reason when one is not allowed. */
fun parseWorkerLabels(text: String): Pair<List<String>?, String?> {
    val parts = text.split(',').map { it.trim().lowercase() }.filter { it.isNotEmpty() }.distinct()
    val bad = parts.firstOrNull { !WORKER_LABEL.matches(it) }
    return when {
        bad != null -> null to "\"$bad\" is not a label: use a-z, 0-9 and : . _ - only, up to 40 characters."
        parts.size > 20 -> null to "Up to 20 labels."
        else -> parts to null
    }
}

private val WORKER_LABEL = Regex("^[a-z0-9:._-]{1,40}$")
