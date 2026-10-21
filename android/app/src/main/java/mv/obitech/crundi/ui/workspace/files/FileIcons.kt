package mv.obitech.crundi.ui.workspace.files

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType

/**
 * The Files tab's file marks, as the desktop's fileIcon(): a coloured text
 * badge for the common languages (their own brand colours, as on the
 * desktop), an icon for the rest.
 */
object FileIcons {
    data class Mark(val text: String? = null, val color: Color = CrundiColors.TextMuted, val icon: String? = null)

    private val badges = mapOf(
        "js" to Mark("JS", Color(0xFFF7DF1E)), "mjs" to Mark("JS", Color(0xFFF7DF1E)), "cjs" to Mark("JS", Color(0xFFF7DF1E)), "jsx" to Mark("JSX", Color(0xFF61DAFB)),
        "ts" to Mark("TS", Color(0xFF3178C6)), "tsx" to Mark("TSX", Color(0xFF3178C6)),
        "py" to Mark("PY", Color(0xFF3776AB)), "md" to Mark("M↓", Color(0xFF519ABA)), "json" to Mark("{}", Color(0xFFCBCB41)),
        "yml" to Mark("YML", Color(0xFFCB171E)), "yaml" to Mark("YML", Color(0xFFCB171E)), "go" to Mark("GO", Color(0xFF00ADD8)),
        "rs" to Mark("RS", Color(0xFFDEA584)), "sh" to Mark("\$_", CrundiColors.Green), "bash" to Mark("\$_", CrundiColors.Green),
        "kt" to Mark("KT", Color(0xFFA97BFF)), "kts" to Mark("KT", Color(0xFFA97BFF)), "java" to Mark("JV", Color(0xFFE76F00)),
        "html" to Mark("<>", Color(0xFFE34C26)), "css" to Mark("#", Color(0xFF563D7C)), "c" to Mark("C", Color(0xFF555555)),
        "cpp" to Mark("C++", Color(0xFF00599C)), "rb" to Mark("RB", Color(0xFFCC342D)), "php" to Mark("PHP", Color(0xFF777BB4)),
        "sql" to Mark("SQL", Color(0xFFE38C00)), "toml" to Mark("TML", Color(0xFF9C4221)), "xml" to Mark("<>", Color(0xFF0060AC)),
    )

    fun of(name: String, isDir: Boolean): Mark {
        if (isDir) return Mark(color = CrundiColors.Yellow, icon = "folder")
        val lower = name.lowercase()
        val ext = lower.substringAfterLast('.', "")
        badges[ext]?.let { return it }
        return when {
            lower == "dockerfile" || lower.startsWith("docker-compose") || lower.startsWith("compose.") -> Mark(icon = "docker", color = Color(0xFF2496ED))
            lower.startsWith(".env") || ext == "lock" || lower.endsWith("lock.json") -> Mark(icon = "lock")
            ext in setOf("png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico", "avif") -> Mark(icon = "image")
            ext in setOf("zip", "tar", "gz", "tgz", "bz2", "xz", "7z", "rar", "jar", "apk", "aab") -> Mark(icon = "archive")
            ext == "pdf" -> Mark(icon = "file-text", color = CrundiColors.Red)
            ext == "log" -> Mark(icon = "list")
            ext in setOf("mp4", "mov", "webm", "mkv", "avi") -> Mark(icon = "film")
            ext in setOf("mp3", "wav", "ogg", "m4a", "flac", "opus") -> Mark(icon = "music")
            else -> Mark(icon = "file")
        }
    }

    /** The language name the highlighter uses for a file. */
    fun language(name: String): String? {
        val lower = name.lowercase()
        if (lower == "dockerfile") return "dockerfile"
        if (lower == "makefile") return "makefile"
        return when (val ext = lower.substringAfterLast('.', "")) {
            "" -> if (lower.startsWith(".")) "sh" else null
            "mjs", "cjs", "jsx" -> "js"
            "tsx" -> "ts"
            "kts" -> "kt"
            "yml" -> "yaml"
            "bash", "zsh" -> "sh"
            "env", "gitignore", "dockerignore" -> "sh"
            else -> ext
        }
    }

    fun isImage(name: String) = name.lowercase().substringAfterLast('.', "") in setOf("png", "jpg", "jpeg", "gif", "webp", "bmp", "svg")
}

@Composable
fun FileMark(name: String, isDir: Boolean, modifier: Modifier = Modifier) {
    val m = FileIcons.of(name, isDir)
    Box(modifier.size(22.dp), contentAlignment = Alignment.Center) {
        if (m.icon != null) CrundiIcon(m.icon, size = 16.dp, tint = m.color)
        else Text(m.text.orEmpty(), style = CrundiType.Code.copy(fontSize = if (m.text!!.length > 2) 8.sp else 10.sp, fontWeight = FontWeight.Bold, color = m.color), maxLines = 1)
    }
}
