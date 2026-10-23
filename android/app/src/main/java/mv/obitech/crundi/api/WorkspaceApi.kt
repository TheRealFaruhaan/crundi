package mv.obitech.crundi.api

import mv.obitech.crundi.api.model.Folder
import mv.obitech.crundi.api.model.FileSearch
import mv.obitech.crundi.api.model.FileText
import mv.obitech.crundi.api.model.GitCommitDetail
import mv.obitech.crundi.api.model.GitDiff
import mv.obitech.crundi.api.model.GitLog
import mv.obitech.crundi.api.model.GitStatus
import mv.obitech.crundi.api.model.MediaItem
import mv.obitech.crundi.api.model.Project
import mv.obitech.crundi.api.model.ProjectsConfig
import mv.obitech.crundi.api.model.ResolvedFile
import mv.obitech.crundi.api.model.TransferResult
import java.io.InputStream
import java.io.OutputStream

/** Projects: the switcher's list comes live from [CrundiClient.state]; these change it. */
class ProjectsApi internal constructor(private val rpc: Rpc) {
    suspend fun list(): List<Project> = rpc.read(rpc.get("/api/projects"), "projects", emptyList())
    suspend fun config(): ProjectsConfig = rpc.read(rpc.get("/api/projects/config"), null)
    /** Does this folder exist on the server (multi mode's "Add project")? */
    suspend fun pathExists(path: String): Boolean = rpc.post("/api/projects/check-path", mapOf("path" to path)).bool("exists") == true
    /** Add a project. In single mode give only [alias] (and [name]); the server makes its folder. */
    suspend fun add(alias: String, name: String? = null, path: String? = null, create: Boolean? = null) {
        rpc.post("/api/projects", mapOf("alias" to alias, "name" to name, "path" to path, "create" to create))
    }
    /** Remove a registered project (files stay; its chats, terminals and services go). Answers how many services were removed. */
    suspend fun remove(alias: String): Int = rpc.delete("/api/projects/${seg(alias)}").long("servicesRemoved")?.toInt() ?: 0
    suspend fun reorder(order: List<String>) { rpc.post("/api/projects/reorder", mapOf("order" to order)) }
    /** The project's own layer of Claude's system prompt (max 8000 chars; "" clears it). */
    suspend fun setSystemPrompt(project: String, text: String) { rpc.post("/api/projects/system-prompt", mapOf("project" to project, "text" to text)) }
}

/** Files of a project. Paths may be absolute or relative to the project. */
class FilesApi internal constructor(private val rpc: Rpc) {
    /** A folder; [dir] null for the project's own. A folder that is gone falls back to the nearest one above (Folder.fellBack). */
    suspend fun list(project: String, dir: String? = null): Folder = rpc.read(rpc.get("/api/files/list", mapOf("project" to project, "dir" to dir)), null)
    /** Files and folders whose NAME contains [q] (at most 400; .gitignore applied in a repo). */
    suspend fun search(project: String, q: String): FileSearch = rpc.read(rpc.get("/api/files/search", mapOf("project" to project, "q" to q), slow = true), null)
    /** A path as a chat wrote it ("src/a.js:12") made into one that exists. */
    suspend fun resolve(project: String, path: String): ResolvedFile = rpc.read(rpc.get("/api/files/resolve", mapOf("project" to project, "path" to path)), null)
    /** The file as text. Over 1 MB is refused ("File too large"): offer a download instead. */
    suspend fun read(project: String, file: String): FileText = rpc.read(rpc.get("/api/files/read", mapOf("project" to project, "file" to file)), null)
    suspend fun write(project: String, file: String, content: String) { rpc.post("/api/files/write", mapOf("project" to project, "file" to file, "content" to content)) }
    /** Upload into [dir] (absolute or relative; created if missing). Overwrites a file of the same name. */
    suspend fun upload(project: String, dir: String, name: String, size: Long, progress: Progress? = null, open: () -> InputStream): Long =
        rpc.call("POST", "/api/files/upload", upload = Uploads.base64Json(mapOf("project" to project, "dir" to dir, "name" to name), size, open), progress = progress).long("size") ?: size
    /** Stream a file into [sink] (any size). */
    suspend fun download(project: String, file: String, sink: OutputStream, progress: Progress? = null): DownloadInfo =
        rpc.transport.download(ApiRequest("GET", "/api/files/download", mapOf("project" to project, "file" to file)), sink, progress)
    /** A link that works without a login for 5 minutes ("/dl/<token>"; add ?inline=1 to view). For handing to another app. */
    suspend fun downloadLink(project: String, file: String): String = rpc.transport.baseUrl + (rpc.post("/api/files/download-link", mapOf("project" to project, "file" to file)).str("url") ?: "")
    /** Move or copy [items] into [to]. Never overwrites: a clash is renamed. */
    suspend fun transfer(project: String, items: List<String>, to: String, copy: Boolean = false): TransferResult =
        rpc.read(rpc.post("/api/files/transfer", mapOf("project" to project, "op" to if (copy) "copy" else "move", "items" to items, "to" to to)), null)
    /** Delete a file or a whole folder. */
    suspend fun delete(project: String, file: String) { rpc.post("/api/files/delete", mapOf("project" to project, "file" to file)) }
}

/** Git of a project. */
class GitApi internal constructor(private val rpc: Rpc) {
    /** Branch, changed files, ahead/behind. Not a repo: ApiException with the server's words. */
    suspend fun status(project: String): GitStatus = rpc.read(rpc.get("/api/git/info", mapOf("project" to project)), null)
    suspend fun diff(project: String, file: String, staged: Boolean): GitDiff = rpc.read(rpc.get("/api/git/diff", mapOf("project" to project, "file" to file, "cached" to if (staged) "1" else "0")), null)
    suspend fun stage(project: String, files: List<String>) { rpc.post("/api/git/stage", mapOf("project" to project, "files" to files)) }
    suspend fun unstage(project: String, files: List<String>) { rpc.post("/api/git/unstage", mapOf("project" to project, "files" to files)) }
    /** Stage part of a file: [patch] is a unified diff of the hunk (git apply --cached). */
    suspend fun stageHunk(project: String, patch: String) { rpc.post("/api/git/stageHunk", mapOf("project" to project, "patch" to patch)) }
    /** Throw away a file's changes (an untracked file is deleted). */
    suspend fun discard(project: String, file: String) { rpc.post("/api/git/discard", mapOf("project" to project, "file" to file)) }
    suspend fun commit(project: String, message: String) { rpc.post("/api/git/commit", mapOf("project" to project, "message" to message)) }
    suspend fun push(project: String) { rpc.post("/api/git/push", mapOf("project" to project), slow = true) }
    /** Answers git's output. */
    suspend fun pull(project: String): String = rpc.post("/api/git/pull", mapOf("project" to project), slow = true).str("output") ?: ""
    /** The branch's history, newest first: [limit] commits after skipping [skip] (GET /api/git/log). */
    suspend fun log(project: String, limit: Int = 50, skip: Int = 0): GitLog =
        rpc.read(rpc.get("/api/git/log", mapOf("project" to project, "limit" to limit.toString(), "skip" to skip.toString())), null)
    /** One commit: message, files with line counts, and its patch (cut at 400 KB, see [GitCommitDetail.truncated]). */
    suspend fun show(project: String, hash: String): GitCommitDetail = rpc.read(rpc.get("/api/git/show", mapOf("project" to project, "hash" to hash)), "commit")
}

/** The media library. Pictures and files show through [CrundiClient.images] / download with [MediaItem.rawPath]. */
class MediaApi internal constructor(private val rpc: Rpc) {
    /**
     * Items, newest first. [project] with [includeGeneral] also lists the
     * unscoped ones (the desktop's project view). [kind]: kanban, mindmap,
     * unlinked.
     */
    suspend fun list(project: String? = null, includeGeneral: Boolean = true, kind: String? = null): List<MediaItem> =
        rpc.read(rpc.get("/api/media/list", mapOf("project" to project, "includeGeneral" to if (project != null && includeGeneral) "1" else null, "kind" to kind)), "items", emptyList())
    /** Add a file. [link] e.g. mapOf("type" to "task", "taskId" to id) or ("node", nodeId). */
    suspend fun upload(name: String, size: Long, project: String? = null, link: Map<String, String>? = null, progress: Progress? = null, open: () -> InputStream): MediaItem =
        rpc.read(rpc.call("POST", "/api/media/upload", upload = Uploads.base64Json(mapOf("name" to name, "project" to project, "link" to link), size, open), progress = progress), "item")
    suspend fun delete(id: String) { rpc.post("/api/media/delete", mapOf("id" to id)) }
    suspend fun download(item: MediaItem, sink: OutputStream, progress: Progress? = null): DownloadInfo =
        rpc.transport.download(ApiRequest("GET", "/api/media/raw/${item.id}", mapOf("download" to "1")), sink, progress)
}
