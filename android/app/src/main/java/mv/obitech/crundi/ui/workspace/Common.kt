package mv.obitech.crundi.ui.workspace

import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import mv.obitech.crundi.Pickers
import mv.obitech.crundi.api.ApiException
import java.net.URLEncoder
import java.time.Duration
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

/** The workspace areas' detail routes. Each starts with its area key, so the shell knows the tab. */
object WorkspaceRoutes {
    private fun enc(s: String) = URLEncoder.encode(s, "UTF-8")

    const val TERMINAL = "terminals/term/{id}"
    fun terminal(id: String) = "terminals/term/" + enc(id)

    const val FILE = "files/view?path={path}"
    fun file(path: String) = "files/view?path=" + enc(path)

    const val DIFF = "git/diff?file={file}&staged={staged}"
    fun diff(file: String, staged: Boolean) = "git/diff?file=" + enc(file) + "&staged=" + staged

    const val HISTORY = "git/history"

    const val COMMIT = "git/commit/{hash}"
    fun commit(hash: String) = "git/commit/" + enc(hash)

    const val MEDIA_ITEM = "media/item/{id}"
    fun mediaItem(id: String) = "media/item/" + enc(id)
}

/** Words for a failure, fit to show: the server's own when it gave some. */
/** A short message at the bottom of the screen, as the desktop's toasts. */
@Composable
fun rememberToast(): (String) -> Unit {
    val ctx = LocalContext.current
    return remember(ctx) { { msg: String -> Toast.makeText(ctx.applicationContext, msg, Toast.LENGTH_SHORT).show() } }
}

/** "0 B", "8.0 KB", "2.3 MB", as the desktop's Files tab. */
fun formatSize(bytes: Long): String = when {
    bytes <= 0 -> ""
    bytes < 1024 -> "$bytes B"
    bytes < 1024 * 1024 -> String.format(Locale.US, "%.1f KB", bytes / 1024.0)
    bytes < 1024L * 1024 * 1024 -> String.format(Locale.US, "%.1f MB", bytes / 1048576.0)
    else -> String.format(Locale.US, "%.1f GB", bytes / 1073741824.0)
}

/** "just now", "5 min ago", "3 h ago", "2 days ago", then the date. [now] for tests. */
fun relativeTime(iso: String, now: Instant = Instant.now()): String {
    val t = runCatching { Instant.parse(iso) }.recoverCatching { java.time.OffsetDateTime.parse(iso).toInstant() }.getOrNull() ?: return iso
    val d = Duration.between(t, now)
    return when {
        d.isNegative || d.seconds < 60 -> "just now"
        d.toMinutes() < 60 -> "${d.toMinutes()} min ago"
        d.toHours() < 24 -> "${d.toHours()} h ago"
        d.toDays() < 7 -> if (d.toDays() == 1L) "yesterday" else "${d.toDays()} days ago"
        else -> DateTimeFormatter.ofPattern("d MMM yyyy", Locale.UK).withZone(ZoneId.systemDefault()).format(t)
    }
}

/** A file picked on the phone: its name, size and a way to read it. */
class PickedFile(val name: String, val size: Long, val open: () -> java.io.InputStream)

fun pickedFile(ctx: Context, uri: Uri): PickedFile {
    var name = uri.lastPathSegment?.substringAfterLast('/') ?: "file"
    var size = -1L
    ctx.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)?.use { c ->
        if (c.moveToFirst()) {
            c.getString(0)?.let { name = it }
            if (!c.isNull(1)) size = c.getLong(1)
        }
    }
    return PickedFile(name, size, { ctx.contentResolver.openInputStream(uri) ?: throw java.io.FileNotFoundException(name) })
}

/**
 * The system's file picker, for uploads. Marks [Pickers.active] while it is
 * open so leaving for it does not put chats up as bubbles.
 */
@Composable
fun rememberFilePicker(onPicked: (List<PickedFile>) -> Unit): () -> Unit {
    val ctx = LocalContext.current
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.GetMultipleContents()) { uris ->
        Pickers.active = false
        if (uris.isNotEmpty()) onPicked(uris.map { pickedFile(ctx, it) })
    }
    return remember(launcher) { { Pickers.active = true; launcher.launch("*/*") } }
}
