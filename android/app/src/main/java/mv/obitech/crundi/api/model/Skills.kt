package mv.obitech.crundi.api.model

import kotlinx.serialization.Serializable

// ─── Skills (src/skills-store.js, the /api/skills routes) ───

/**
 * One Claude skill: a folder with a SKILL.md, in ~/.claude/skills (scope
 * "global"), a project's .claude/skills ("project:<alias>"), or synced from
 * the Claude account ("synced"). [origin] is user, bundled (ships with
 * Crundi), synced or linked; anything but user is [readOnly]. [problem] says
 * why Claude would not load it. [files] and [size] are a floor when [more].
 */
@Serializable
data class Skill(
    val scope: String = "",
    val name: String = "",
    val title: String = "",
    val description: String = "",
    val files: Int = 0,
    val more: Boolean = false,
    val size: Long = 0,
    val origin: String = "user",
    val readOnly: Boolean = false,
    val problem: String = "",
)

/** A place a skill can live, for the pickers: "global" (All projects) first, then one per project. */
@Serializable data class SkillScope(val id: String = "", val label: String = "", val path: String = "")

/** The server's limits (skills-store.js LIMITS), with its values as the defaults. */
@Serializable
data class SkillLimits(
    val upload: Long = 50L * 1024 * 1024,
    val unpacked: Long = 100L * 1024 * 1024,
    val files: Int = 2000,
    val editable: Long = 1024L * 1024,
    val file: Long = 25L * 1024 * 1024,
)

/** GET /api/skills. */
@Serializable data class SkillList(val skills: List<Skill> = emptyList(), val scopes: List<SkillScope> = emptyList(), val limits: SkillLimits = SkillLimits())

@Serializable data class SkillFileEntry(val path: String = "", val size: Long = 0)

/** GET /api/skills/detail: the files, the first [SkillLimits.files] of them when [more]. */
@Serializable
data class SkillDetail(
    val skill: Skill? = null,
    val path: String = "",
    val files: List<SkillFileEntry> = emptyList(),
    val more: Boolean = false,
    val skippedLinks: Int = 0,
    val readOnlyReason: String = "",
)

/** GET /api/skills/file: [text] unless [binary] (or [tooLarge], which is binary too). */
@Serializable
data class SkillFile(
    val path: String = "",
    val size: Long = 0,
    val readOnly: Boolean = false,
    val binary: Boolean = false,
    val tooLarge: Boolean = false,
    val text: String = "",
)

/** POST /api/skills/file: where it went and how large it is now. */
@Serializable data class SkillFileSaved(val path: String = "", val size: Long = 0, val created: Boolean = false)

/** POST /api/skills/upload: what went in, and what was left out of the archive. */
@Serializable data class SkillInstall(val installed: List<Skill> = emptyList(), val dropped: List<String> = emptyList())
