package mv.obitech.crundi.ui.workspace.git

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType

/** The desktop's diff colours (.fe-diff-* in src/webapp-html.js). Only diffs use these. */
object DiffColors {
    val RemovedBg = Color(0xFF2D1215)
    val RemovedText = Color(0xFFF87171)
    val AddedBg = Color(0xFF0F291A)
    val AddedText = Color(0xFF34D399)
    val HunkBg = Color(0xFF1E293B)
}

/** A unified diff, read into files, chunks and lines. */
object DiffParser {
    enum class Kind { Context, Added, Removed, Note }
    data class Line(val kind: Kind, val text: String, val oldNo: Int?, val newNo: Int?)
    data class Hunk(val header: String, val lines: List<Line>, val raw: String)
    data class File(val name: String, val header: String, val hunks: List<Hunk>, val binary: Boolean = false) {
        val added get() = hunks.sumOf { h -> h.lines.count { it.kind == Kind.Added } }
        val removed get() = hunks.sumOf { h -> h.lines.count { it.kind == Kind.Removed } }
    }

    private val hunkRe = Regex("^@@ -(\\d+)(?:,\\d+)? \\+(\\d+)(?:,\\d+)? @@")

    fun parse(diff: String): List<File> {
        val files = ArrayList<File>()
        val lines = diff.replace("\r\n", "\n").split('\n')
        var i = 0
        while (i < lines.size) {
            if (!lines[i].startsWith("diff --git") && !(files.isEmpty() && lines[i].startsWith("--- "))) { i++; continue }
            val head = StringBuilder()
            var name = lines[i].removePrefix("diff --git ").substringAfter(" b/", lines[i].substringAfterLast(' '))
            var binary = false
            while (i < lines.size && !lines[i].startsWith("@@") && !(head.isNotEmpty() && lines[i].startsWith("diff --git"))) {
                val l = lines[i]
                if (l.startsWith("+++ ") && l != "+++ /dev/null") name = l.removePrefix("+++ ").removePrefix("b/")
                else if (l.startsWith("--- ") && name.isEmpty()) name = l.removePrefix("--- ").removePrefix("a/")
                if (l.startsWith("Binary files")) binary = true
                head.append(l).append('\n'); i++
            }
            val hunks = ArrayList<Hunk>()
            while (i < lines.size && lines[i].startsWith("@@")) {
                val h = lines[i]
                val m = hunkRe.find(h)
                var o = m?.groupValues?.get(1)?.toIntOrNull() ?: 0
                var n = m?.groupValues?.get(2)?.toIntOrNull() ?: 0
                val raw = StringBuilder(h).append('\n')
                val out = ArrayList<Line>()
                i++
                while (i < lines.size && !lines[i].startsWith("@@") && !lines[i].startsWith("diff --git")) {
                    val l = lines[i]
                    when {
                        l.startsWith("+") -> { out += Line(Kind.Added, l.substring(1), null, n); n++ }
                        l.startsWith("-") -> { out += Line(Kind.Removed, l.substring(1), o, null); o++ }
                        l.startsWith("\\") -> out += Line(Kind.Note, l, null, null)
                        l.startsWith(" ") -> { out += Line(Kind.Context, l.substring(1), o, n); o++; n++ }
                        l.isEmpty() && i == lines.lastIndex -> { i++; continue }
                        else -> { out += Line(Kind.Context, l, o, n); o++; n++ }
                    }
                    raw.append(l).append('\n'); i++
                }
                hunks += Hunk(h, out, raw.toString())
            }
            files += File(name, head.toString(), hunks, binary)
        }
        return files
    }

    /** A patch of one chunk alone, for `git apply --cached` (the desktop's stage-a-chunk). */
    fun patchFor(file: File, hunk: Hunk): String {
        val head = file.header.lines().filter { it.startsWith("diff --git") || it.startsWith("--- ") || it.startsWith("+++ ") || it.startsWith("new file") || it.startsWith("deleted file") || it.startsWith("index ") }
        val withFiles = if (head.none { it.startsWith("--- ") }) head + listOf("--- a/${file.name}", "+++ b/${file.name}") else head
        return withFiles.joinToString("\n", postfix = "\n") + hunk.raw
    }
}

/**
 * A unified diff as rows in a lazy list: per chunk a header bar (with a
 * Stage button when [onStage] is given), then each line with its old and new
 * numbers, green for added and red for removed, as the desktop's mobile diff.
 */
fun LazyListScope.diffItems(
    file: DiffParser.File,
    keyPrefix: String,
    staged: Set<Int> = emptySet(),
    onStage: ((Int) -> Unit)? = null,
) {
    file.hunks.forEachIndexed { hi, h ->
        item(key = "$keyPrefix-h$hi") { HunkBar(hi, h.header, staged = hi in staged, onStage = onStage?.let { { it(hi) } }) }
        items(h.lines.size, key = { "$keyPrefix-h$hi-l$it" }) { li -> DiffRow(h.lines[li]) }
    }
}

@Composable
private fun HunkBar(index: Int, header: String, staged: Boolean, onStage: (() -> Unit)?) {
    Row(
        Modifier.widthIn(min = LocalConfiguration.current.screenWidthDp.dp).background(DiffColors.HunkBg).padding(horizontal = 8.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        if (onStage != null) {
            val c = if (staged) CrundiColors.TextMuted else CrundiColors.Green
            Text(
                if (staged) "Staged" else "Stage",
                style = CrundiType.Code.copy(fontSize = 11.sp, fontWeight = FontWeight.Bold, color = c),
                modifier = Modifier.border(1.dp, c, androidx.compose.foundation.shape.RoundedCornerShape(3.dp))
                    .clickable(enabled = !staged, onClick = onStage).padding(horizontal = 10.dp, vertical = 4.dp),
            )
        }
        Text("Chunk ${index + 1}", style = CrundiType.Meta)
        Text(header.substringAfter("@@", "").substringAfter("@@", "").trim(), style = CrundiType.Meta, maxLines = 1)
    }
}

@Composable
fun DiffRow(l: DiffParser.Line) {
    val (bg, fg, sign) = when (l.kind) {
        DiffParser.Kind.Added -> Triple(DiffColors.AddedBg, DiffColors.AddedText, "+")
        DiffParser.Kind.Removed -> Triple(DiffColors.RemovedBg, DiffColors.RemovedText, "-")
        DiffParser.Kind.Note -> Triple(Color.Transparent, CrundiColors.TextMuted, "")
        DiffParser.Kind.Context -> Triple(Color.Transparent, CrundiColors.TextPrimary, "")
    }
    val num = CrundiType.Code.copy(fontSize = 11.sp, color = CrundiColors.TextMuted, textAlign = TextAlign.End)
    Row(Modifier.widthIn(min = LocalConfiguration.current.screenWidthDp.dp).background(bg)) {
        Text(l.oldNo?.toString() ?: "", style = num, modifier = Modifier.width(34.dp))
        Text(l.newNo?.toString() ?: "", style = num, modifier = Modifier.width(34.dp))
        Text(sign, style = CrundiType.Code.copy(color = fg), textAlign = TextAlign.Center, modifier = Modifier.width(16.dp))
        Text(l.text.replace("\t", "    "), style = CrundiType.Code.copy(color = fg), softWrap = false, modifier = Modifier.padding(end = 16.dp))
    }
}
