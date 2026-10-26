package mv.obitech.crundi.api

import mv.obitech.crundi.api.model.ChatSecretGrant

import kotlinx.serialization.json.JsonObject
import mv.obitech.crundi.api.model.ApprovalItem
import mv.obitech.crundi.api.model.Approvals
import mv.obitech.crundi.api.model.ClaudeUpdate
import mv.obitech.crundi.api.model.Container
import mv.obitech.crundi.api.model.ContainerDetail
import mv.obitech.crundi.api.model.DockerDisk
import mv.obitech.crundi.api.model.DockerEngine
import mv.obitech.crundi.api.model.DockerForward
import mv.obitech.crundi.api.model.DockerImage
import mv.obitech.crundi.api.model.DockerJob
import mv.obitech.crundi.api.model.DockerNetwork
import mv.obitech.crundi.api.model.DockerStack
import mv.obitech.crundi.api.model.DockerStats
import mv.obitech.crundi.api.model.DockerStatus
import mv.obitech.crundi.api.model.DockerVolume
import mv.obitech.crundi.api.model.Forward
import mv.obitech.crundi.api.model.ForwardOptions
import mv.obitech.crundi.api.model.Forwards
import mv.obitech.crundi.api.model.LoginSession
import mv.obitech.crundi.api.model.MaintenanceTask
import mv.obitech.crundi.api.model.NewForward
import mv.obitech.crundi.api.model.NewService
import mv.obitech.crundi.api.model.NotifyChannel
import mv.obitech.crundi.api.model.PruneResult
import mv.obitech.crundi.api.model.Registered
import mv.obitech.crundi.api.model.Secrets
import mv.obitech.crundi.api.model.ServerLog
import mv.obitech.crundi.api.model.ServerSettings
import mv.obitech.crundi.api.model.Service
import mv.obitech.crundi.api.model.SettingsChange
import mv.obitech.crundi.api.model.SettingsSaved
import mv.obitech.crundi.api.model.Stats
import mv.obitech.crundi.api.model.TunnelResult
import mv.obitech.crundi.api.model.UpdateStatus
import mv.obitech.crundi.api.model.Usage
import mv.obitech.crundi.api.model.UsagePoint
import mv.obitech.crundi.api.model.WhatsNew

/** Services. The list is live in [CrundiClient.state] (services); this adds cpu, containers and the actions. */
class ServicesApi internal constructor(private val rpc: Rpc) {
    private fun at(key: String, action: String) = "/api/services/${seg(key)}/$action"

    suspend fun list(): List<Service> = rpc.read(rpc.get("/api/services"), "services", emptyList())
    /** Register a service. A collaborator's waits for the owner: [Registered.pendingApproval]. */
    suspend fun register(s: NewService): Registered = try {
        val o = rpc.post("/api/services", mapOf("name" to s.name, "command" to s.command, "alias" to s.alias, "cwd" to s.cwd, "stopCommand" to s.stopCommand, "tunnelPort" to s.tunnelPort))
        Registered(o.str("key") ?: "")
    } catch (e: ApiException) {
        if (e.body?.bool("pendingApproval") == true) Registered(pendingApproval = true, message = e.message) else throw e
    }
    suspend fun start(key: String) { rpc.post(at(key, "start"), slow = true) }
    suspend fun stop(key: String) { rpc.post(at(key, "stop"), slow = true) }
    suspend fun restart(key: String) { rpc.post(at(key, "restart"), slow = true) }
    suspend fun delete(key: String) { rpc.post(at(key, "delete"), slow = true) }
    /** The last 100 lines (stderr prefixed "[err] "); empty if it never ran. */
    suspend fun logs(key: String): List<String> = rpc.read(rpc.get(at(key, "logs")), "logs", emptyList())
    /** Exposure on the server's tunnel: [port] to expose, [enabled] on/off (port 0 turns it off). */
    suspend fun setTunnel(key: String, enabled: Boolean, port: Int? = null): TunnelResult = rpc.read(rpc.post(at(key, "tunnel"), mapOf("enabled" to enabled, "port" to port)), null)
    /** The server's own tunnel and each service's (GET /api/tunnel): the Info tab's Tunnel section. */
    suspend fun tunnels(): mv.obitech.crundi.api.model.Tunnels = rpc.read(rpc.get("/api/tunnel"), null)
}

/** Forwards: a port on this machine reached at a subdomain or /tunnel/<name>/. */
class ForwardsApi internal constructor(private val rpc: Rpc) {
    suspend fun list(): Forwards = rpc.read(rpc.get("/api/forwards"), null)
    suspend fun options(): ForwardOptions = rpc.read(rpc.get("/api/forwards/options"), null)
    suspend fun add(f: NewForward): Forward = rpc.read(rpc.post("/api/forwards", mapOf("name" to f.name, "port" to f.port, "mode" to f.mode, "public" to f.public, "description" to f.description, "project" to f.project)), "forward")
    suspend fun remove(host: String) { rpc.delete("/api/forwards/${seg(host)}") }
    /** Watch it (tell the person when it goes down or comes back). */
    suspend fun setWatch(host: String, watch: Boolean): Forward = rpc.read(rpc.patch("/api/forwards/${seg(host)}", mapOf("watch" to watch)), "forward")
    /** Give it to a project ("" for none). */
    suspend fun setProject(host: String, project: String): Forward = rpc.read(rpc.patch("/api/forwards/${seg(host)}", mapOf("project" to project)), "forward")
}

/** Docker. Live logs and shells go over the socket ([CrundiClient.dockerLogs], [CrundiClient.dockerShell]). Owner only. */
class DockerApi internal constructor(private val rpc: Rpc) {
    suspend fun status(force: Boolean = false): DockerStatus = rpc.read(rpc.get("/api/docker/status", mapOf("force" to if (force) "1" else null)), null)
    suspend fun containers(stats: Boolean = false): List<Container> = rpc.read(rpc.get("/api/docker/containers", mapOf("stats" to if (stats) "1" else null)), "containers", emptyList())
    suspend fun container(id: String): ContainerDetail = rpc.read(rpc.get("/api/docker/containers/detail", mapOf("id" to id)), null)
    /** The last [tail] lines at once (for live lines use the socket). */
    suspend fun logs(id: String, tail: Int = 500): String = rpc.get("/api/docker/containers/logs", mapOf("id" to id, "tail" to tail.toString())).str("text") ?: ""
    suspend fun stats(id: String): DockerStats? = rpc.read(rpc.get("/api/docker/containers/stats", mapOf("id" to id)), "stats", DockerStats()).takeIf { it.at > 0 }
    /** start, stop, restart, pause, unpause, kill */
    suspend fun act(id: String, action: String) { rpc.post("/api/docker/containers/action", mapOf("id" to id, "action" to action), slow = true) }
    suspend fun rename(id: String, name: String) { rpc.post("/api/docker/containers/rename", mapOf("id" to id, "name" to name)) }
    suspend fun remove(id: String, force: Boolean = false, volumes: Boolean = false) { rpc.post("/api/docker/containers/remove", mapOf("id" to id, "force" to force, "volumes" to volumes), slow = true) }
    suspend fun stacks(): List<DockerStack> = rpc.read(rpc.get("/api/docker/stacks"), "stacks", emptyList())
    /** up, pull, restart, stop, start, down. Answers a job id: follow it with [job]. */
    suspend fun stackAction(name: String, action: String): String = rpc.post("/api/docker/stacks/action", mapOf("name" to name, "action" to action)).str("job") ?: ""
    suspend fun images(): List<DockerImage> = rpc.read(rpc.get("/api/docker/images"), "images", emptyList())
    suspend fun removeImage(id: String, force: Boolean = false) { rpc.post("/api/docker/images/remove", mapOf("id" to id, "force" to force)) }
    /** Answers a job id. */
    suspend fun pull(reference: String): String = rpc.post("/api/docker/images/pull", mapOf("reference" to reference)).str("job") ?: ""
    suspend fun volumes(): List<DockerVolume> = rpc.read(rpc.get("/api/docker/volumes"), "volumes", emptyList())
    suspend fun removeVolume(name: String) { rpc.post("/api/docker/volumes/remove", mapOf("name" to name)) }
    suspend fun networks(): List<DockerNetwork> = rpc.read(rpc.get("/api/docker/networks"), "networks", emptyList())
    suspend fun removeNetwork(id: String) { rpc.post("/api/docker/networks/remove", mapOf("id" to id)) }
    suspend fun engine(): DockerEngine = rpc.read(rpc.get("/api/docker/system"), "engine")
    suspend fun disk(): DockerDisk = rpc.read(rpc.get("/api/docker/system/df", slow = true), "disk")
    /** containers, images, images-all, volumes, networks, build */
    suspend fun prune(kind: String): PruneResult = rpc.read(rpc.post("/api/docker/prune", mapOf("kind" to kind), slow = true), null)
    /** A job's output from [from]; call again with [DockerJob.next] until done. */
    suspend fun job(id: String, from: Int = 0): DockerJob = rpc.read(rpc.get("/api/docker/job", mapOf("id" to id, "from" to from.toString())), null)
    /** Forwards that point at containers, with whether each is listening. */
    suspend fun forwards(): List<DockerForward> = rpc.read(rpc.get("/api/docker/forwards"), "forwards", emptyList())
}

/**
 * Secrets: names only. Values are revealed with the secret's PIN (6 digits, older
 * secrets) or passphrase. Claude's
 * requests to use one arrive live ([CrundiClient.secretRequests]) and are
 * approved with the PIN or denied.
 */
class SecretsApi internal constructor(private val rpc: Rpc) {
    suspend fun list(): Secrets = rpc.read(rpc.get("/api/secrets"), null)
    suspend fun add(name: String, value: String, pin: String, description: String = ""): String = rpc.post("/api/secrets", mapOf("action" to "add", "name" to name, "description" to description, "value" to value, "pin" to pin)).str("id") ?: ""
    suspend fun rename(id: String, name: String? = null, description: String? = null) { rpc.post("/api/secrets", mapOf("action" to "updateMeta", "id" to id, "name" to name, "description" to description)) }
    suspend fun delete(id: String) { rpc.post("/api/secrets", mapOf("action" to "delete", "id" to id)) }
    /** The value. Wrong PIN: ApiException "Incorrect PIN". Show it briefly; never store it. */
    suspend fun reveal(id: String, pin: String): String = rpc.post("/api/secrets", mapOf("action" to "reveal", "id" to id, "pin" to pin)).str("value") ?: ""
    /** Is this the PIN? (Starting a factory run: 'factory-secrets'.) Never returns the value. Wrong: ApiException "Incorrect PIN". */
    suspend fun check(id: String, pin: String) { rpc.post("/api/secrets", mapOf("action" to "check", "id" to id, "pin" to pin)) }
    /**
     * Let Claude have it (the value goes to Claude, not to this phone).
     * [allowForChat], a duration key ([ChatSecretGrant.PRESETS], 'chat-secret-preauth'):
     * also keep the PIN for the chat that asked. Answers whether it was kept.
     */
    suspend fun approve(requestId: String, pin: String, allowForChat: String? = null): Boolean {
        val r = rpc.post("/api/secrets", mapOf("action" to "approve", "reqId" to requestId, "pin" to pin, "allowForChat" to allowForChat?.let { mapOf("duration" to it) }))
        return r.bool("preauthorised") == true
    }

    // ─── Pre-authorised for one chat (capability 'chat-secret-preauth') ───
    // Names and expiry only come back; the PINs stay in the server's memory.
    /** What is pre-authorised for [sessionId]. */
    suspend fun chatGrants(sessionId: String): List<ChatSecretGrant> = rpc.read(rpc.get("/api/secrets/chat", mapOf("sessionId" to sessionId)), "grants", emptyList())
    /** Check each PIN and keep them for [sessionId] for [duration] (a preset key). None kept unless all are right. */
    suspend fun grantChat(sessionId: String, pins: Map<String, String>, duration: String): List<ChatSecretGrant> =
        rpc.read(rpc.post("/api/secrets/chat", mapOf("action" to "grant", "sessionId" to sessionId, "duration" to duration, "secrets" to pins.map { (id, pin) -> mapOf("id" to id, "pin" to pin) })), "grants", emptyList())
    suspend fun revokeChat(sessionId: String, secretId: String): List<ChatSecretGrant> =
        rpc.read(rpc.post("/api/secrets/chat", mapOf("action" to "revoke", "sessionId" to sessionId, "secretId" to secretId)), "grants", emptyList())
    suspend fun revokeAllChat(sessionId: String): List<ChatSecretGrant> =
        rpc.read(rpc.post("/api/secrets/chat", mapOf("action" to "revokeAll", "sessionId" to sessionId)), "grants", emptyList())
    suspend fun deny(requestId: String) { rpc.post("/api/secrets", mapOf("action" to "deny", "reqId" to requestId)) }

    /**
     * Edit a secret ('secret-edit'), proven by its current lock [pin]:
     * [newPin] re-locks it with a passphrase (8+ characters, not digits only),
     * [value] replaces the value under the same lock, [name] and
     * [description] as [rename]. Also the upgrade of an old lock
     * ('secret-upgrade': [newPin] only). Never returns the value. Wrong lock:
     * ApiException "Incorrect PIN" / "Incorrect passphrase".
     */
    suspend fun edit(id: String, pin: String, newPin: String? = null, value: String? = null, name: String? = null, description: String? = null) {
        rpc.post("/api/secrets", mapOf("action" to "edit", "id" to id, "pin" to pin, "newPin" to newPin, "value" to value, "name" to name, "description" to description))
    }
}

/** Things waiting on the person, across every chat and collaborator. Live in [CrundiClient.approvals]. */
class ApprovalsApi internal constructor(private val rpc: Rpc) {
    suspend fun list(): Approvals = rpc.read(rpc.get("/api/approvals"), null)
    /** Decide a collaborator's request. [scope]: once, session, always. Chat items are answered in the chat ([ChatsApi.allow]); secrets with [SecretsApi.approve]. */
    suspend fun decide(id: String, approve: Boolean, scope: String? = null, note: String? = null): JsonObject = rpc.post("/api/approvals", mapOf("id" to id, "approve" to approve, "scope" to scope, "note" to note))
    /** Take items off the list (a chat item is only hidden; a collab one is denied). */
    suspend fun dismiss(items: List<ApprovalItem>): Int = rpc.post("/api/approvals", mapOf("action" to "dismiss", "items" to items.map { mapOf("id" to it.id, "source" to it.source) })).long("dismissed")?.toInt() ?: 0
}

/** Claude usage (live in [CrundiClient.usage]) and its history. */
class UsageApi internal constructor(private val rpc: Rpc) {
    suspend fun now(force: Boolean = false): Usage = rpc.read(rpc.get("/api/usage", mapOf("force" to if (force) "1" else null)), null)
    /** range: 5h, day, week, month, all */
    suspend fun history(range: String = "week"): List<UsagePoint> = rpc.read(rpc.get("/api/usage/history", mapOf("range" to range)), "records", emptyList())
}

/** The server itself: settings, notifications, updates, logs, machine stats, sign-ins. */
class SystemApi internal constructor(private val rpc: Rpc) {
    suspend fun settings(): ServerSettings = rpc.read(rpc.get("/api/settings"), null)
    suspend fun saveSettings(c: SettingsChange): SettingsSaved = rpc.read(rpc.post("/api/settings", mapOf(
        "settings" to c.env, "chatId" to c.chatId, "limitWarmup" to c.limitWarmup, "notifyPrefs" to c.notifyPrefs,
        "autoParkMinutes" to c.autoParkMinutes, "systemPrompt" to c.systemPrompt,
    )), null)
    suspend fun channels(): List<NotifyChannel> = rpc.read(rpc.get("/api/notify/channels"), "channels", emptyList())
    suspend fun setChannel(id: String, enabled: Boolean) { rpc.post("/api/notify/channels", mapOf("id" to id, "enabled" to enabled)) }
    suspend fun update(force: Boolean = false): UpdateStatus = rpc.read(rpc.get("/api/update/status", mapOf("force" to if (force) "1" else null), slow = force), null)
    /** production or dev */
    suspend fun setUpdateChannel(channel: String) { rpc.post("/api/update/channel", mapOf("channel" to channel)) }
    suspend fun applyUpdate() { rpc.post("/api/update/apply", slow = true) }
    /** Restart Crundi: every chat and terminal on the server ends. Confirm with the person first. */
    suspend fun restart() { rpc.post("/api/restart") }
    suspend fun claude(force: Boolean = false): ClaudeUpdate = rpc.read(rpc.get("/api/claude-update/status", mapOf("force" to if (force) "1" else null)), "claude")
    /** Update the Claude CLI. needsConfirm in the error body: ask, then call again with [allowBeyondTested]. */
    suspend fun updateClaude(version: String? = null, allowBeyondTested: Boolean = false) { rpc.post("/api/claude-update/apply", mapOf("version" to version, "allowBeyondTested" to allowBeyondTested.takeIf { it }), slow = true) }
    suspend fun maintenance(): List<MaintenanceTask> = rpc.read(rpc.get("/api/maintenance"), "tasks", emptyList())
    suspend fun runMaintenance(id: String): Long = rpc.post("/api/maintenance", mapOf("id" to id), slow = true).long("freed") ?: 0
    /** The last 500 lines; live lines come from [CrundiClient.serverLogs]. */
    suspend fun logs(): List<ServerLog> = rpc.read(rpc.get("/api/server-logs"), "logs", emptyList())
    suspend fun stats(): Stats = rpc.read(rpc.get("/api/stats"), null)
    suspend fun sessions(): List<LoginSession> = rpc.read(rpc.get("/api/auth/sessions"), "sessions", emptyList())
    suspend fun revokeSession(id: String) { rpc.post("/api/auth/sessions/revoke", mapOf("id" to id)) }
    suspend fun revokeOtherSessions(): Int = rpc.post("/api/auth/sessions/revoke", mapOf("others" to true)).long("revoked")?.toInt() ?: 0
    suspend fun whatsNew(all: Boolean = false): WhatsNew = rpc.read(rpc.get("/api/whatsnew", mapOf("all" to if (all) "1" else null)), null)
    suspend fun whatsNewSeen() { rpc.post("/api/whatsnew/seen") }
}
