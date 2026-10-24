package mv.obitech.crundi.api

import mv.obitech.crundi.api.model.Skill
import mv.obitech.crundi.api.model.SkillDetail
import mv.obitech.crundi.api.model.SkillFile
import mv.obitech.crundi.api.model.SkillFileSaved
import mv.obitech.crundi.api.model.SkillInstall
import mv.obitech.crundi.api.model.SkillList
import java.io.InputStream
import java.io.OutputStream

/**
 * The Claude skills on the server (src/webapp.js "Skills", src/skills-store.js),
 * as the web's Settings > Skills uses them. Owner only: a collaborator gets 403.
 *
 * A clash ("a skill called x already exists there") comes back as an
 * [ApiException] with status 409 and `conflict`/`existing` in its body; an
 * upload that does not say what to call the skill has `needsName` in it.
 * Ask again with `overwrite` or a name.
 */
class SkillsApi internal constructor(private val rpc: Rpc) {
    suspend fun list(): SkillList = rpc.read(rpc.get("/api/skills"), null)
    suspend fun detail(scope: String, name: String): SkillDetail = rpc.read(rpc.get("/api/skills/detail", mapOf("scope" to scope, "name" to name)), null)
    suspend fun file(scope: String, name: String, path: String): SkillFile = rpc.read(rpc.get("/api/skills/file", mapOf("scope" to scope, "name" to name, "path" to path)), null)

    /** A new skill with a SKILL.md holding [name] and [description]. */
    suspend fun create(scope: String, name: String, description: String): Skill =
        rpc.read(rpc.post("/api/skills/create", mapOf("scope" to scope, "name" to name, "description" to description)), "skill")
    suspend fun rename(scope: String, name: String, to: String): Skill =
        rpc.read(rpc.post("/api/skills/rename", mapOf("scope" to scope, "name" to name, "to" to to)), "skill")
    suspend fun delete(scope: String, name: String) { rpc.post("/api/skills/delete", mapOf("scope" to scope, "name" to name)) }
    /** Copy (or [move]) a skill to scope [to], named [asName] ("" keeps its name). */
    suspend fun transfer(scope: String, name: String, to: String, asName: String, move: Boolean, overwrite: Boolean = false): Skill =
        rpc.read(rpc.post("/api/skills/transfer", mapOf("scope" to scope, "name" to name, "to" to to, "as" to asName, "move" to move, "overwrite" to overwrite)), "skill")

    /** Write a text file inside a skill (made if missing; "" makes an empty one). */
    suspend fun write(scope: String, name: String, path: String, content: String): SkillFileSaved =
        rpc.read(rpc.post("/api/skills/file", mapOf("scope" to scope, "name" to name, "path" to path, "content" to content)), null)
    suspend fun deleteFile(scope: String, name: String, path: String) { rpc.post("/api/skills/file-delete", mapOf("scope" to scope, "name" to name, "path" to path)) }

    /**
     * Install from a .zip or .skill archive (one or more skills) or a single
     * SKILL.md, sent as the raw bytes. [name] names a skill the upload does not
     * name; [overwrite] replaces skills of the same names.
     */
    suspend fun upload(scope: String, filename: String, size: Long, name: String? = null, overwrite: Boolean = false, progress: Progress? = null, open: () -> InputStream): SkillInstall =
        rpc.read(rpc.call("POST", "/api/skills/upload", mapOf("scope" to scope, "filename" to filename, "name" to name?.takeIf { it.isNotEmpty() }, "overwrite" to if (overwrite) "1" else null), upload = Uploads.raw(size, open), progress = progress), null)

    /** Add (or replace) one file of any kind at [path] inside a skill, sent as the raw bytes. */
    suspend fun uploadFile(scope: String, name: String, path: String, size: Long, progress: Progress? = null, open: () -> InputStream): SkillFileSaved =
        rpc.read(rpc.call("POST", "/api/skills/file-upload", mapOf("scope" to scope, "name" to name, "path" to path), upload = Uploads.raw(size, open), progress = progress), null)

    /** The whole skill as a zip, wrapped in a folder of its name (what [upload] takes back). */
    suspend fun download(scope: String, name: String, sink: OutputStream, progress: Progress? = null): DownloadInfo =
        rpc.transport.download(ApiRequest("GET", "/api/skills/download", mapOf("scope" to scope, "name" to name)), sink, progress)
    suspend fun downloadFile(scope: String, name: String, path: String, sink: OutputStream, progress: Progress? = null): DownloadInfo =
        rpc.transport.download(ApiRequest("GET", "/api/skills/file-download", mapOf("scope" to scope, "name" to name, "path" to path)), sink, progress)
}
