package mv.obitech.crundi.api

import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import mv.obitech.crundi.api.model.CollabCreated
import mv.obitech.crundi.api.model.Collaborators
import mv.obitech.crundi.api.model.SandboxStatus
import mv.obitech.crundi.api.model.ServerBrowsers
import mv.obitech.crundi.api.model.Tunnels
import mv.obitech.crundi.api.model.Widget
import mv.obitech.crundi.api.model.WidgetData
import mv.obitech.crundi.api.model.WidgetDoc
import mv.obitech.crundi.api.model.WidgetVersion
import java.net.URLEncoder
import java.util.Base64

/**
 * Panels (widgets). Owner only. Refetch the list on the `widget` event
 * (data {project, id, kind}: kind list or doc means the list or a document
 * changed, data means a source has a new value, store means a panel saved
 * something). `project` is the project the page shows ("" for none: the
 * global panels only); a global panel is found from any project.
 */
class WidgetsApi internal constructor(private val rpc: Rpc) {
    /** The project's panels, then the global ones. */
    suspend fun list(project: String): List<Widget> = rpc.read(rpc.get("/api/widgets", mapOf("project" to project)), "widgets", emptyList())

    /** The document for a frame. [chip] gives the chip build (only the element marked data-chip). */
    suspend fun doc(project: String, id: String, chip: Boolean = false): WidgetDoc =
        rpc.read(rpc.get("/api/widgets/doc", mapOf("project" to project, "id" to id, "chip" to if (chip) "1" else null)), null)

    /** Current values: all sources, or one [source]. The first call starts the sources (it may wait up to 1.5 s). */
    suspend fun data(project: String, id: String, source: String? = null): WidgetData =
        rpc.read(rpc.get("/api/widgets/data", mapOf("project" to project, "id" to id, "source" to source), slow = true), null)

    /** Only the panel's saved store (after a `store` event). */
    suspend fun store(project: String, id: String): JsonObject =
        rpc.get("/api/widgets/data", mapOf("project" to project, "id" to id, "storeOnly" to "1")).obj("store") ?: JsonObject(emptyMap())

    /** Keep these panels' sources running: send every 25 s for what is on screen, as the web does. */
    suspend fun watch(project: String, ids: List<String>) { rpc.post("/api/widgets/watch", mapOf("project" to project, "ids" to ids.take(40))) }

    /**
     * A call from inside a frame (op store.set, emit, prompt, action; toast and
     * openLink the app does itself). Answers the server's whole reply, which
     * goes back to the frame as `value`. Fails with ApiException (its message
     * goes back as `error`; `body.needsGrant` when it waits for approval).
     */
    suspend fun call(project: String, id: String, op: String, args: JsonElement?): JsonObject =
        rpc.post("/api/widgets/call", mapOf("project" to project, "id" to id, "op" to op, "args" to (args ?: JsonObject(emptyMap()))))

    /**
     * The person's choices: [slot] (dock, cell, tab, chip), [closed], [collapsed],
     * [pinned]. Only what is set changes. Answers the panel as it now is.
     */
    suspend fun update(project: String, id: String, slot: String? = null, closed: Boolean? = null, collapsed: Boolean? = null, pinned: Boolean? = null): Widget =
        rpc.read(rpc.post("/api/widgets/user", mapOf("project" to project, "id" to id, "patch" to mapOf("slot" to slot, "closed" to closed, "collapsed" to collapsed, "pinned" to pinned).filterValues { it != null })), "widget")

    /** Approve what it asks for ([hash] from WidgetDoc.grant), or take the approval back. A stale hash: ApiException, reload the doc. */
    suspend fun grant(project: String, id: String, hash: String) { rpc.post("/api/widgets/grant", mapOf("project" to project, "id" to id, "hash" to hash)) }
    suspend fun revoke(project: String, id: String) { rpc.post("/api/widgets/grant", mapOf("project" to project, "id" to id, "revoke" to true)) }

    /** A script error inside a frame (send at most a handful per frame). */
    suspend fun fault(project: String, id: String, message: String, where: String = "") {
        rpc.post("/api/widgets/fault", mapOf("project" to project, "id" to id, "message" to message.take(500), "where" to where.take(120)))
    }

    /** Show it in every project, or back in its own. */
    suspend fun setGlobal(project: String, id: String, global: Boolean): Widget = rpc.read(rpc.post("/api/widgets/scope", mapOf("project" to project, "id" to id, "global" to global)), "widget")

    /** Delete it (its source folder too unless [keepSource]). */
    suspend fun delete(project: String, id: String, keepSource: Boolean = false) { rpc.post("/api/widgets/delete", mapOf("project" to project, "id" to id, "keepSource" to keepSource)) }

    suspend fun versions(project: String, id: String): List<WidgetVersion> = rpc.read(rpc.get("/api/widgets/versions", mapOf("project" to project, "id" to id)), "versions", emptyList())
    suspend fun rollback(project: String, id: String, version: Int) { rpc.post("/api/widgets/rollback", mapOf("project" to project, "id" to id, "version" to version)) }

    /** Claude's own check renders of the panel (index.json), and one as a picture path for rememberServerImage. */
    suspend fun renders(project: String, id: String): JsonObject? = rpc.get("/api/widgets/renders", mapOf("project" to project, "id" to id)).obj("renders")
    fun renderImagePath(project: String, id: String, name: String) =
        "/api/widgets/render-image?project=${URLEncoder.encode(project, "UTF-8")}&id=${URLEncoder.encode(id, "UTF-8")}&name=${URLEncoder.encode(name, "UTF-8")}"
}

/**
 * The server's own browsers (the ones Claude opens with browser_open), for
 * the Browsers tab's list. The Browser pane itself is a WebView on the phone
 * and needs no server call.
 */
class BrowsersApi internal constructor(private val rpc: Rpc) {
    /** A project's browsers ("" for all), and where a new one can run. */
    suspend fun list(project: String = ""): ServerBrowsers = rpc.read(rpc.get("/api/browsers", mapOf("alias" to project)), null)
    /** [on]: "server", "desktop", or null for the default. Answers its key. */
    suspend fun open(project: String, url: String, name: String = "default", width: Int = 1280, height: Int = 720, on: String? = null): String =
        rpc.post("/api/browsers/open", mapOf("alias" to project, "url" to url, "name" to name, "width" to width, "height" to height, "on" to on), slow = true).str("key") ?: ""
    suspend fun close(key: String) { rpc.post("/api/browsers/${seg(key)}/close") }
    suspend fun navigate(key: String, url: String) { rpc.post("/api/browsers/${seg(key)}/navigate", mapOf("url" to url), slow = true) }
    /** What it shows now, as PNG bytes. */
    suspend fun screenshot(key: String): ByteArray {
        val b64 = rpc.get("/api/browsers/${seg(key)}/screenshot", slow = true).str("data") ?: throw ApiException(200, "The browser sent no picture.")
        return Base64.getDecoder().decode(b64)
    }
}

/** Outside collaborators (Info tab). Owner only. The state push changes when these do. */
class CollaboratorsApi internal constructor(private val rpc: Rpc) {
    suspend fun list(): Collaborators = rpc.read(rpc.get("/api/collaborators"), null)

    private suspend fun act(action: String, vararg f: Pair<String, Any?>) = rpc.post("/api/collaborators", mapOf("action" to action, *f), slow = true)

    /**
     * Give [name] access to [projects] for [hours] (or [days]). Answers the
     * invitations and, once, the passcode to hand over. No sandbox yet:
     * ApiException whose body has needsSandbox (offer [setupSandbox]).
     */
    suspend fun create(name: String, projects: List<String>, hours: Int? = null, days: Int? = null, telegram: String? = null, withPasscode: Boolean = true, tokenLimit: Long? = null): CollabCreated =
        rpc.read(act("create", "name" to name, "projects" to projects, "hours" to hours, "days" to days, "telegram" to telegram, "withPasscode" to withPasscode, "tokenLimit" to tokenLimit), null)
    /** Add a project to someone who has access (they keep their passcode). */
    suspend fun grant(name: String, project: String, hours: Int? = null, days: Int? = null, telegram: String? = null): CollabCreated =
        rpc.read(act("grant", "name" to name, "project" to project, "hours" to hours, "days" to days, "telegram" to telegram), null)
    suspend fun sandbox(): SandboxStatus = rpc.read(act("sandboxStatus"), "sandbox")
    /** Install what the sandbox needs (may take minutes). */
    suspend fun setupSandbox(): JsonObject = act("sandboxSetup")
    suspend fun setLimit(id: String, tokenLimit: Long) { act("setLimit", "id" to id, "tokenLimit" to tokenLimit) }
    suspend fun resetUsage(id: String) { act("resetUsage", "id" to id) }
    suspend fun removeDomain(id: String, host: String) { act("removeDomain", "id" to id, "host" to host) }
    /** Change fields of an invitation (expiresAt, telegram, ...). */
    suspend fun update(id: String, fields: Map<String, Any?>) { rpc.post("/api/collaborators", mapOf("action" to "update", "id" to id) + fields) }
    /** A new passcode (answered once). */
    suspend fun reissue(id: String): String = act("reissue", "id" to id).str("passcode") ?: ""
    /** End access now (their chats close). */
    suspend fun revoke(id: String) { act("revoke", "id" to id) }
    /** Remove the invitation and its worktree. */
    suspend fun remove(id: String) { act("remove", "id" to id) }
    /** What merging their branch would do, then do it (conflict: ApiException 409). */
    suspend fun mergePreview(id: String): JsonObject = act("mergePreview", "id" to id)
    suspend fun merge(id: String): JsonObject = act("merge", "id" to id)
    /** Sharing a folder that is not a repository: what the first commit would take in, then make it one. */
    suspend fun gitPreview(project: String): JsonObject = act("gitPreview", "project" to project)
    suspend fun initGit(project: String) { act("initGit", "project" to project) }
}

