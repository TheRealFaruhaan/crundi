package mv.obitech.crundi.api.model

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject

// ─── Services ───

/** A new service (POST /api/services). A collaborator's goes to the owner first (pendingApproval). */
data class NewService(val name: String, val command: String, val alias: String? = null, val cwd: String? = null, val stopCommand: String? = null, val tunnelPort: Int? = null)

/** The answer to registering: [pendingApproval] when the owner has to approve it first. */
data class Registered(val key: String = "", val pendingApproval: Boolean = false, val message: String = "")

@Serializable data class TunnelResult(val enabled: Boolean = false, val port: Int = 0)

// ─── Forwards ───

/** A forward: [mode] subdomain (https://host.domain) or path (/tunnel/host/). [url] is where it is reached. */
@Serializable
data class Forward(
    val host: String,
    val port: Int = 0,
    val mode: String = "path",
    val public: Boolean = false,
    val description: String = "",
    val project: String = "",
    val createdAt: Long = 0,
    val watch: Boolean = false,
    val url: String = "",
)

@Serializable data class Forwards(val forwards: List<Forward> = emptyList(), val domain: String = "")

/** One way of exposing a port (GET /api/forwards/options). id: path, subdomain, tunnel. */
@Serializable data class ForwardChoice(val id: String, val label: String = "", val example: String = "", val available: Boolean = true, val setup: String = "", val caveat: String = "", val suggestedDomain: String = "")
@Serializable data class ForwardOptions(val domain: String = "", val choices: List<ForwardChoice> = emptyList())

data class NewForward(val port: Int, val name: String? = null, val mode: String? = null, val public: Boolean? = null, val description: String? = null, val project: String? = null)

// ─── Docker ───

@Serializable data class DockerStatus(val installed: Boolean = false, val reachable: Boolean = false, val error: String = "", val version: String = "", val apiVersion: String = "", val os: String = "", val arch: String = "")

@Serializable data class DockerPort(val ip: String = "", val public: Int = 0, val private: Int = 0, val type: String = "tcp")
@Serializable data class DockerNet(val name: String = "", val ip: String = "", val gateway: String = "", val mac: String = "")
@Serializable data class DockerMount(val type: String = "", val name: String = "", val source: String = "", val destination: String = "", val rw: Boolean = true)
@Serializable data class DockerStats(val at: Long = 0, val cpuPct: Double? = null, val cpus: Int = 0, val memory: Long = 0, val memoryLimit: Long = 0, val pids: Int = 0, val netRx: Long = 0, val netTx: Long = 0, val blkRead: Long = 0, val blkWrite: Long = 0)

@Serializable
data class Container(
    val id: String,
    val fullId: String = "",
    val name: String = "",
    val image: String = "",
    val imageId: String = "",
    val command: String = "",
    /** Unix seconds. */
    val created: Long = 0,
    val state: String = "",
    val status: String = "",
    /** "", healthy, unhealthy, starting */
    val health: String = "",
    val running: Boolean = false,
    /** The compose project, if any. */
    val project: String = "",
    val service: String = "",
    val ports: List<DockerPort> = emptyList(),
    val networks: List<DockerNet> = emptyList(),
    val mounts: List<DockerMount> = emptyList(),
    val stats: DockerStats? = null,
)

@Serializable data class DockerEnv(val key: String = "", val value: String = "", val hidden: Boolean = false)

@Serializable
data class ContainerOverview(
    val id: String = "", val name: String = "", val image: String = "", val imageId: String = "", val created: String = "",
    val startedAt: String = "", val finishedAt: String = "", val state: String = "", val exitCode: Int = 0, val error: String = "",
    val oomKilled: Boolean = false, val health: String = "", val restartCount: Int = 0, val restartPolicy: String = "", val tty: Boolean = false,
    val entrypoint: JsonElement? = null, val cmd: JsonElement? = null, val workingDir: String = "", val user: String = "", val hostname: String = "",
    val ports: List<DockerPort> = emptyList(), val networks: List<DockerNet> = emptyList(), val mounts: List<DockerMount> = emptyList(),
    val env: List<DockerEnv> = emptyList(), val labels: Map<String, String> = emptyMap(), val project: String = "", val service: String = "",
)

/** GET /api/docker/containers/detail: [inspect] is docker's raw inspect output. */
@Serializable data class ContainerDetail(val container: Container, val overview: ContainerOverview = ContainerOverview(), val inspect: JsonObject? = null)

@Serializable data class DockerStack(val name: String, val directory: String = "", val files: List<String> = emptyList(), val filesPresent: Boolean = false, val live: Int = 0, val total: Int = 0, val containers: List<Container> = emptyList())
@Serializable data class DockerImage(val id: String, val fullId: String = "", val tags: List<String> = emptyList(), val name: String = "", val size: Long = 0, val created: Long = 0, val dangling: Boolean = false, val usedBy: List<String> = emptyList())
@Serializable data class DockerVolume(val name: String, val driver: String = "", val mountpoint: String = "", val created: String = "", val project: String = "", val anonymous: Boolean = false, val usedBy: List<String> = emptyList())
@Serializable data class DockerNetwork(val id: String, val fullId: String = "", val name: String = "", val driver: String = "", val scope: String = "", val subnet: String = "", val gateway: String = "", val internal: Boolean = false, val builtin: Boolean = false, val project: String = "", val usedBy: List<String> = emptyList())
@Serializable data class DockerEngine(val version: String = "", val apiVersion: String = "", val compose: String = "", val os: String = "", val kernel: String = "", val arch: String = "", val cpus: Int = 0, val memory: Long = 0, val storageDriver: String = "", val cgroup: String = "", val rootDir: String = "", val name: String = "", val containers: Int = 0, val running: Int = 0, val paused: Int = 0, val stopped: Int = 0, val images: Int = 0, val warnings: List<String> = emptyList())
@Serializable data class DiskUse(val count: Int = 0, val size: Long = 0, val reclaimable: Long = 0, val error: String = "")
@Serializable data class DockerDisk(val images: DiskUse? = null, val containers: DiskUse? = null, val volumes: DiskUse? = null, val build: DiskUse? = null)
@Serializable data class PruneResult(val removed: Int = 0, val reclaimed: Long = 0)
/** A long docker job (stack up, image pull): poll with [next] until [done]. */
@Serializable data class DockerJob(val title: String = "", val text: String = "", val next: Int = 0, val done: Boolean = false, val code: Int? = null)
@Serializable data class DockerForwardContainer(val id: String = "", val name: String = "", val state: String = "", val running: Boolean = false, val health: String = "", val project: String = "")
@Serializable data class DockerForward(val host: String, val port: Int = 0, val mode: String = "", val public: Boolean = false, val url: String = "", val project: String = "", val container: DockerForwardContainer? = null, val listening: Boolean = false, val state: String = "")

/** A docker log line from the socket: [stream] stdout or stderr. */
@Serializable data class DockerLogFrame(val stream: String = "stdout", val text: String = "")

// ─── Machine ───

@Serializable data class CpuStats(val overall: Double? = null, val cores: List<Double?> = emptyList())
@Serializable data class MemStats(val total: Long = 0, val available: Long = 0, val used: Long = 0, val cached: Long = 0, val swapTotal: Long = 0, val swapUsed: Long = 0)
@Serializable data class DiskStats(val path: String = "", val total: Long = 0, val free: Long = 0, val used: Long = 0)
@Serializable data class NetStats(val rxPerSec: Double? = null, val txPerSec: Double? = null)
@Serializable data class LoadStats(val one: Double = 0.0, val five: Double = 0.0, val fifteen: Double = 0.0)
@Serializable data class HostInfo(val hostname: String = "", val platform: String = "", val release: String = "", val arch: String = "", val cpuModel: String = "", val cores: Int = 0, val nodeVersion: String = "")
@Serializable data class StatsHistory(val cpu: List<Double?> = emptyList(), val mem: List<Double?> = emptyList())

@Serializable
data class SystemStats(
    val at: Long = 0, val cpu: CpuStats = CpuStats(), val mem: MemStats = MemStats(), val disk: DiskStats = DiskStats(),
    val net: NetStats = NetStats(), val load: LoadStats? = null, val uptimeSec: Double = 0.0, val processUptimeSec: Double = 0.0,
    val processRssBytes: Long = 0, val host: HostInfo = HostInfo(), val history: StatsHistory = StatsHistory(),
)

@Serializable data class ServiceStats(val status: String = "", val pid: Int? = null, val cpuPct: Double? = null, val memory: Long? = null, val cpuHistory: List<Double?> = emptyList(), val memHistory: List<Long?> = emptyList())

/** GET /api/stats: the machine (null for a collaborator) and each running service. */
@Serializable data class Stats(val system: SystemStats? = null, val services: Map<String, ServiceStats> = emptyMap())

// ─── Settings and the server ───

/**
 * GET /api/settings. [env] holds TELEGRAM_BOT_TOKEN, ALLOWED_USERNAME,
 * PROJECTS_DIR, WEB_PORT, CLOUDFLARE_TUNNEL_TOKEN, CLOUDFLARE_TUNNEL_URL,
 * DATA_DIR. [notifyPrefs] maps each event to always, away or never.
 */
@Serializable
data class ServerSettings(
    val settings: Map<String, String> = emptyMap(),
    val envPath: String = "",
    val chatId: String = "",
    val notifyPrefs: Map<String, String> = emptyMap(),
    val limitWarmup: JsonObject? = null,
    val queuedLimitReset: Int = 0,
    val systemPrompt: String = "",
    val systemPromptMax: Int = 8000,
    val autoParkMinutes: Int = 0,
    val autoParkMin: Int = 10,
)

/** Changes to send (POST /api/settings); only what is set is changed. */
data class SettingsChange(
    val env: Map<String, String>? = null,
    val chatId: String? = null,
    val limitWarmup: Boolean? = null,
    val notifyPrefs: Map<String, String>? = null,
    val autoParkMinutes: Int? = null,
    val systemPrompt: String? = null,
)

@Serializable data class SettingsSaved(val restartRequired: Boolean = false, val clearedLimitReset: Int = 0)

/** A way the server tells the person things (GET /api/notify/channels). id: telegram, desktop, webpush, mobile. */
@Serializable data class NotifyChannel(val id: String, val label: String = "", val describe: String = "", val available: Boolean = true, val unavailableReason: String = "", val enabled: Boolean = false)

@Serializable
data class UpdateInfo(
    val current: String = "", val latest: String = "", val notesUrl: String = "", val available: Boolean = false, val checkedAt: Long = 0,
    val error: String = "", val applying: Boolean = false, val blocker: String = "", val channel: String = "production",
    val latestKind: String = "", val currentIsDev: Boolean = false,
)

@Serializable data class UpdateStatus(val update: UpdateInfo = UpdateInfo(), val log: String = "", val canRestart: Boolean = false, val embedded: Boolean = false)

@Serializable data class ClaudeUpdate(val installed: String = "", val latest: String = "", val tested: String = "", val available: Boolean = false, val beyondTested: Boolean = false, val canUpdate: Boolean = false, val needsSudo: Boolean = false, val blocker: String = "", val applying: Boolean = false, val lastResult: JsonElement? = null, val checkedAt: Long = 0, val error: String = "")

@Serializable data class MaintenanceTask(val id: String, val label: String = "", val description: String = "", val elevated: Boolean = false, val danger: Boolean = false, val available: Boolean = true, val bytes: Long = 0)

@Serializable data class ServerLog(val time: Long = 0, val level: String = "info", val text: String = "")

@Serializable data class SessionDevice(val id: String = "", val name: String = "", val platform: String = "")
/** A signed-in browser or device (GET /api/auth/sessions). */
@Serializable data class LoginSession(val id: String, val current: Boolean = false, val device: SessionDevice? = null, val username: String = "", val createdAt: Long = 0, val lastSeenAt: Long = 0, val userAgent: String = "", val ip: String = "")

@Serializable data class WhatsNewKey(val keys: List<String> = emptyList(), val text: String = "")
@Serializable data class WhatsNewStep(val id: String = "", val version: String = "", val tag: String = "", val title: String = "", val body: String = "", val keys: List<WhatsNewKey> = emptyList())
@Serializable data class WhatsNew(val version: String = "", val steps: List<WhatsNewStep> = emptyList())
