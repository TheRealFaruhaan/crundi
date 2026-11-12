package mv.obitech.crundi.api

import kotlinx.serialization.Serializable

/**
 * Backups of the whole of Crundi to S3-compatible storage, and restoring
 * them (Settings > Backup and restore, as on the web; src/backup.js). The
 * storage secret and the passphrase are write-only: the server says only
 * whether each is set, and a blank field keeps the saved one. Progress
 * comes live as "backup" events.
 *
 * A server without backups answers 404: [status] gives null.
 */
class BackupApi internal constructor(private val rpc: Rpc) {

    suspend fun status(): BackupStatus? = try {
        rpc.read(rpc.get("/api/backup"), null)
    } catch (e: ApiException) {
        if (e.status == 404) null else throw e
    }

    /** Storage, passphrase ("" keeps it), schedule and how many to keep, as the web's Save sends them. */
    suspend fun save(storage: BackupStorageForm, passphrase: String, schedule: BackupSchedule, keep: Int) {
        rpc.post("/api/backup/settings", mapOf(
            "storage" to storage.toMap(), "passphrase" to passphrase,
            "schedule" to mapOf("enabled" to schedule.enabled, "frequency" to schedule.frequency, "time" to schedule.time, "weekday" to schedule.weekday, "everyHours" to schedule.everyHours),
            "keep" to keep,
        ))
    }

    /** Can the bucket be listed with these (a blank secret: the saved one)? Throws with the storage's words if not. */
    suspend fun test(storage: BackupStorageForm) { rpc.post("/api/backup/test", mapOf("storage" to storage.toMap()), slow = true) }

    /** Back up now (or run the waiting scheduled one); progress comes as "backup" events. */
    suspend fun run() { rpc.post("/api/backup/run") }

    /** Skip the scheduled backup that waits for the owner to leave. */
    suspend fun skip() { rpc.post("/api/backup/skip") }

    suspend fun list(): List<BackupItem> = rpc.read(rpc.get("/api/backup/list", slow = true), "items", emptyList())

    suspend fun delete(key: String) { rpc.post("/api/backup/delete", mapOf("key" to key)) }

    /** Download and unpack [key] ("" passphrase: the saved one); applied when Crundi restarts. */
    suspend fun restore(key: String, passphrase: String) { rpc.post("/api/backup/restore", mapOf("key" to key, "passphrase" to passphrase)) }

    suspend fun cancelRestore() { rpc.post("/api/backup/cancel-restore") }

    /** Restart to finish a staged restore (only where something starts Crundi again: [BackupStatus.restartMode]). */
    suspend fun restart() { rpc.post("/api/backup/restart") }
}

@Serializable
data class BackupStorage(
    val endpoint: String = "", val region: String = "auto", val bucket: String = "", val prefix: String = "crundi/",
    val accessKeyId: String = "", val hasSecret: Boolean = false, val pathStyle: Boolean = true,
)

/** What the form sends: the secret only when typed. */
data class BackupStorageForm(
    val endpoint: String = "", val region: String = "auto", val bucket: String = "", val prefix: String = "crundi/",
    val accessKeyId: String = "", val secretAccessKey: String = "", val pathStyle: Boolean = true,
) {
    fun toMap(): Map<String, Any?> = mapOf(
        "endpoint" to endpoint.trim(), "bucket" to bucket.trim(), "region" to region.trim().ifEmpty { "auto" }, "prefix" to prefix.trim(),
        "accessKeyId" to accessKeyId.trim(), "secretAccessKey" to secretAccessKey.trim(), "pathStyle" to pathStyle,
    )

    companion object {
        fun of(s: BackupStorage) = BackupStorageForm(s.endpoint, s.region, s.bucket, s.prefix, s.accessKeyId, "", s.pathStyle)
    }
}

/** frequency: daily, weekly or hours (every [everyHours] from [time]). weekday: 0 Sunday. */
@Serializable
data class BackupSchedule(val enabled: Boolean = false, val frequency: String = "daily", val time: String = "03:00", val weekday: Int = 0, val everyHours: Int = 12)

/** phase: collecting, packing, uploading, tidying, downloading, unpacking. reason: manual, scheduled or restore. */
@Serializable
data class BackupRunning(val phase: String = "", val reason: String = "", val done: Double = 0.0, val total: Double = 0.0, val startedAt: Long = 0)

@Serializable
data class BackupWaiting(val since: Long = 0, val slot: Long = 0)

@Serializable
data class BackupLast(val ok: Boolean = false, val at: Long = 0, val reason: String = "", val key: String = "", val size: Long = 0, val files: Int = 0, val error: String = "")

/** This server was restored from a backup (shown for a week). */
@Serializable
data class BackupRestored(val at: Long = 0, val backupCreatedAt: String = "", val fromHost: String = "", val safetyCopy: String = "")

@Serializable
data class BackupAttempt(val ok: Boolean = false, val at: Long = 0, val key: String = "", val error: String = "")

@Serializable
data class BackupStatus(
    val storage: BackupStorage = BackupStorage(),
    val hasPassphrase: Boolean = false,
    val schedule: BackupSchedule = BackupSchedule(),
    val keep: Int = 14,
    /** Storage and a passphrase are set: backups can be made. */
    val configured: Boolean = false,
    val running: BackupRunning? = null,
    /** A scheduled backup is due and waits for the owner to leave Crundi. */
    val waiting: BackupWaiting? = null,
    val last: BackupLast? = null,
    val nextAt: Long? = null,
    val timezone: String = "",
    val restored: BackupRestored? = null,
    /** A restore is unpacked and is applied when Crundi restarts. */
    val staged: Boolean = false,
    val restoreAttempt: BackupAttempt? = null,
    /** systemd, pm2, desktop, or "": nothing here starts Crundi again, so it cannot restart itself. */
    val restartMode: String = "",
)

/** A backup in the bucket. reason: scheduled or manual. */
@Serializable
data class BackupItem(val key: String = "", val size: Long = 0, val at: Long = 0, val reason: String = "", val host: String = "")
