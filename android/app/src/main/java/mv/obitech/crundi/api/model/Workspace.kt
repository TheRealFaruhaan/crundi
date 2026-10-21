package mv.obitech.crundi.api.model

import kotlinx.serialization.Serializable

// ─── Files ───

/** One row of a folder (GET /api/files/list). [path] is absolute. */
@Serializable data class FileEntry(val name: String, val type: String = "file", val size: Long = 0, val path: String = "") {
    val isDir get() = type == "dir"
}

@Serializable data class Crumb(val name: String = "", val path: String = "")

/** A folder of a project (GET /api/files/list). */
@Serializable
data class Folder(
    val entries: List<FileEntry> = emptyList(),
    val crumbs: List<Crumb> = emptyList(),
    /** Absolute path of this folder. */
    val path: String = "",
    /** The project's own folder. */
    val root: String = "",
    val parent: String? = null,
    /** Inside the project's folder (an owner can browse outside it). */
    val inside: Boolean = true,
    /** The folder asked for was gone; this is the nearest one above it. */
    val fellBack: Boolean = false,
)

/** A search hit (GET /api/files/search): [rel] is relative to the project, with "/". */
@Serializable data class FileHit(val name: String, val rel: String = "", val type: String = "file", val path: String = "") {
    val isDir get() = type == "dir"
}

@Serializable data class FileSearch(val entries: List<FileHit> = emptyList(), val truncated: Boolean = false, val gitignoreApplied: Boolean = false)

/** GET /api/files/resolve: a path from a chat, made into one that exists. */
@Serializable data class ResolvedFile(val file: String, val dir: Boolean = false, val size: Long = 0)

/** GET /api/files/read: the file as text (files over 1 MB are refused). */
@Serializable data class FileText(val content: String = "", val size: Long = 0)

/** POST /api/files/transfer: what moved or copied, and what did not. */
@Serializable data class TransferResult(val done: List<TransferDone> = emptyList(), val failed: List<TransferFailed> = emptyList())
@Serializable data class TransferDone(val from: String = "", val to: String = "", val name: String = "", val renamed: Boolean = false)
@Serializable data class TransferFailed(val from: String = "", val error: String = "")

/** An uploaded attachment (POST /api/attachments/upload): put [path] into the message. */
@Serializable data class Attachment(val path: String, val name: String = "", val size: Long = 0)

// ─── Git ───

/** One changed file. The same path can appear twice: once staged, once not. */
@Serializable
data class GitFile(
    val file: String,
    /** Porcelain XY code. */
    val xy: String = "",
    val staged: Boolean = false,
    /** added, deleted, renamed, modified, untracked */
    val status: String = "modified",
    val add: Int? = null,
    val del: Int? = null,
)

/** GET /api/git/info. */
@Serializable data class GitStatus(val branch: String = "", val files: List<GitFile> = emptyList(), val ahead: Int = 0, val behind: Int = 0) {
    val staged get() = files.filter { it.staged }
    val unstaged get() = files.filter { !it.staged }
}

/** GET /api/git/diff: the unified diff and both whole versions. */
@Serializable data class GitDiff(val diff: String = "", val old: String = "", val new: String = "", val file: String = "")

// ─── Media library ───

@Serializable data class MediaLink(val type: String = "", val taskId: String = "", val todoId: String = "", val nodeId: String = "")

/** One item of the media library (GET /api/media/list). Fetch it from [rawPath]. */
@Serializable
data class MediaItem(
    val id: String,
    val originalName: String = "",
    val mime: String = "",
    val ext: String = "",
    val size: Long = 0,
    /** image, pdf, video, audio, other */
    val kind: String = "other",
    val project: String? = null,
    val projectName: String? = null,
    val link: MediaLink? = null,
    /** none, alive, deleted */
    val linkStatus: String = "none",
    val linkLabel: String? = null,
    val createdAt: String = "",
    val path: String = "",
) {
    val rawPath get() = "/api/media/raw/$id"
    val downloadPath get() = "/api/media/raw/$id?download=1"
}
