package mv.obitech.crundi.ui.components

import androidx.compose.ui.draw.clip
import androidx.compose.foundation.border
import androidx.compose.ui.Alignment
import androidx.compose.foundation.layout.fillMaxWidth
import mv.obitech.crundi.ui.theme.Dimens
import androidx.compose.runtime.setValue
import androidx.compose.runtime.getValue
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withLink
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType

/**
 * Markdown as Claude writes it, drawn natively: headings, paragraphs, bullet
 * and numbered lists, quotes, fenced code (highlighted), rules, simple tables
 * (as monospace), and inline bold, italic, `code`, ~~strike~~ and [links](url).
 * Links open through the platform's URI handler.
 */
object Markdown {
    sealed interface Block
    data class Heading(val level: Int, val text: String) : Block
    data class Paragraph(val text: String) : Block
    data class ListBlock(val ordered: Boolean, val items: List<Pair<Int, String>>, val start: Int = 1) : Block
    data class Quote(val text: String) : Block
    data class Fence(val language: String?, val code: String) : Block
    data class Table(val rows: List<List<String>>) : Block
    data object Rule : Block

    fun parse(src: String): List<Block> {
        val lines = src.replace("\r\n", "\n").split('\n')
        val out = mutableListOf<Block>()
        var i = 0
        val para = StringBuilder()
        fun flush() { if (para.isNotBlank()) out += Paragraph(para.toString().trim()); para.clear() }
        while (i < lines.size) {
            val line = lines[i]
            val t = line.trim()
            when {
                t.startsWith("```") || t.startsWith("~~~") -> {
                    flush()
                    val fence = t.take(3)
                    val lang = t.drop(3).trim().ifEmpty { null }
                    val body = StringBuilder()
                    i++
                    while (i < lines.size && !lines[i].trim().startsWith(fence)) { body.append(lines[i]).append('\n'); i++ }
                    out += Fence(lang, body.toString().trimEnd('\n'))
                }
                t.isEmpty() -> flush()
                Regex("^#{1,6}\\s").containsMatchIn(t) -> { flush(); val lvl = t.takeWhile { it == '#' }.length; out += Heading(lvl, t.drop(lvl).trim()) }
                Regex("^(-{3,}|\\*{3,}|_{3,})$").matches(t) -> { flush(); out += Rule }
                t.startsWith(">") -> {
                    flush()
                    val q = StringBuilder()
                    while (i < lines.size && lines[i].trim().startsWith(">")) { q.append(lines[i].trim().removePrefix(">").trim()).append('\n'); i++ }
                    out += Quote(q.toString().trim()); continue
                }
                t.startsWith("|") && i + 1 < lines.size && Regex("^\\|?\\s*:?-{2,}").containsMatchIn(lines[i + 1].trim()) -> {
                    flush()
                    val rows = mutableListOf<List<String>>()
                    while (i < lines.size && lines[i].trim().startsWith("|")) {
                        val r = lines[i].trim().trim('|').split('|').map { it.trim() }
                        if (!r.all { Regex("^:?-{2,}:?$").matches(it) }) rows += r
                        i++
                    }
                    out += Table(rows); continue
                }
                Regex("^\\s*([-*+]|\\d+[.)])\\s+").containsMatchIn(line) -> {
                    flush()
                    val ordered = Regex("^\\s*\\d+[.)]").containsMatchIn(line)
                    val start = if (ordered) Regex("\\d+").find(line)!!.value.toInt() else 1
                    val items = mutableListOf<Pair<Int, String>>()
                    while (i < lines.size) {
                        val m = Regex("^(\\s*)([-*+]|\\d+[.)])\\s+(.*)$").find(lines[i])
                        if (m != null) { items += (m.groupValues[1].length / 2) to m.groupValues[3]; i++ }
                        else if (lines[i].isNotBlank() && lines[i].startsWith("  ") && items.isNotEmpty()) { val (d, s) = items.removeAt(items.size - 1); items += d to (s + " " + lines[i].trim()); i++ }
                        else break
                    }
                    out += ListBlock(ordered, items, start); continue
                }
                else -> para.append(if (para.isEmpty()) t else "\n$t")
            }
            i++
        }
        flush()
        return out
    }

    private val FILE_EXT = Regex("\\.(js|mjs|cjs|ts|tsx|jsx|json|md|txt|log|py|sh|bash|yml|yaml|toml|ini|conf|cfg|env|html|htm|css|scss|xml|svg|png|jpe?g|gif|webp|ico|pdf|csv|sql|go|rs|rb|php|java|kt|kts|gradle|swift|c|h|cc|cpp|hpp|vue|lock|dockerfile|service)$", RegexOption.IGNORE_CASE)
    private val BARE_PATH = Regex("^(\\.{0,2}/)?[\\w.@+~-]+(/[\\w.@+~-]+)+$")

    /** A code span that is only a file path (src/a.js, ./x/y.md, /etc/z): the web's looksLikePath. */
    fun looksLikePath(c: String): Boolean {
        if (c.isEmpty() || c.length > 260 || c.any { it.isWhitespace() } || c.startsWith("http:") || c.startsWith("https:") || c.any { it in "<>&*|$`" }) return false
        val bare = c.replace(Regex(":\\d+(:\\d+)?$"), "")
        if (BARE_PATH.matches(bare)) return true
        return Regex("^[\\w-][\\w.@+~-]*$").matches(bare) && FILE_EXT.containsMatchIn(bare)
    }

    private val PATH_STYLE = TextLinkStyles(SpanStyle(textDecoration = TextDecoration.Underline))
    private val TEXT_PATH = Regex("^/(?:[\\w.@+~%,=-]+/)+[\\w.@+~%,=-]+(?::\\d+(?::\\d+)?)?")

    /** Inline marks to styled text. [onPath]: file paths become tappable (code spans that are paths, and absolute paths in the text). */
    fun inline(src: String, base: SpanStyle = SpanStyle(), onPath: ((String) -> Unit)? = null, onLink: ((String) -> Unit)? = null): AnnotatedString = buildAnnotatedString {
        // A link opens directly, or (in a chat) asks first: open or copy.
        fun link(url: String, text: String) {
            val style = TextLinkStyles(SpanStyle(color = CrundiColors.AccentHover, textDecoration = TextDecoration.Underline))
            if (onLink != null) withLink(LinkAnnotation.Clickable("link:$url", style) { onLink(url) }) { append(text) }
            else withLink(LinkAnnotation.Url(url, style)) { append(text) }
        }
        var i = 0
        val n = src.length
        while (i < n) {
            val c = src[i]
            fun closing(token: String, from: Int) = src.indexOf(token, from).takeIf { it > from }
            when {
                c == '`' -> {
                    val e = src.indexOf('`', i + 1)
                    if (e > i) {
                        val code = src.substring(i + 1, e)
                        if (onPath != null && looksLikePath(code)) withLink(LinkAnnotation.Clickable("path:$code", PATH_STYLE) { onPath(code) }) { withStyle(InlineCodeStyle) { append(" " + code + " ") } }
                        // Other code: copyable on a tap where a chat offers it (the web's copy button on a code span).
                        else if (onLink != null) withLink(LinkAnnotation.Clickable("code:$code") { onLink("code:" + code) }) { withStyle(InlineCodeStyle) { append(" " + code + " ") } }
                        else withStyle(InlineCodeStyle) { append(" " + code + " ") }
                        i = e + 1
                    } else { append(c); i++ }
                }
                src.startsWith("**", i) || src.startsWith("__", i) -> {
                    val tok = src.substring(i, i + 2); val e = closing(tok, i + 2)
                    if (e != null) { withStyle(SpanStyle(fontWeight = FontWeight.SemiBold)) { append(inline(src.substring(i + 2, e), onPath = onPath, onLink = onLink)) }; i = e + 2 } else { append(tok); i += 2 }
                }
                src.startsWith("~~", i) -> {
                    val e = closing("~~", i + 2)
                    if (e != null) { withStyle(SpanStyle(textDecoration = TextDecoration.LineThrough)) { append(inline(src.substring(i + 2, e))) }; i = e + 2 } else { append("~~"); i += 2 }
                }
                (c == '*' || c == '_') && i + 1 < n && !src[i + 1].isWhitespace() && (i == 0 || !src[i - 1].isLetterOrDigit()) -> {
                    val e = closing(c.toString(), i + 1)
                    if (e != null && !src[e - 1].isWhitespace()) { withStyle(SpanStyle(fontStyle = FontStyle.Italic)) { append(inline(src.substring(i + 1, e))) }; i = e + 1 } else { append(c); i++ }
                }
                c == '[' -> {
                    val m = Regex("^\\[([^\\]]+)]\\(([^)\\s]+)\\)").find(src.substring(i))
                    if (m != null) {
                        link(m.groupValues[2], m.groupValues[1])
                        i += m.value.length
                    } else { append(c); i++ }
                }
                // A bare web address in the text is a link, as the web's linkify.
                (src.startsWith("https://", i) || src.startsWith("http://", i)) && (i == 0 || !src[i - 1].isLetterOrDigit()) -> {
                    var e = i
                    while (e < n && !src[e].isWhitespace() && src[e] != '<' && src[e] != '`') e++
                    var u = src.substring(i, e)
                    u = u.trimEnd('.', ',', ';', ':', '!', '?', ')', ']', '\'', '"', '*', '_')
                    link(u, u)
                    i += u.length
                }
                // An absolute path written in the text ("in /a/b.js:12,"), as the web's linkify.
                onPath != null && c == '/' && (i == 0 || src[i - 1].isWhitespace() || src[i - 1] in "([\"'") && TEXT_PATH.find(src.substring(i)) != null -> {
                    var p = TEXT_PATH.find(src.substring(i))!!.value
                    p = p.trimEnd('.', ',', ';', ':', '!', '?', ')', ']', '\'', '"')
                    withLink(LinkAnnotation.Clickable("path:$p", PATH_STYLE) { onPath(p) }) { append(p) }
                    i += p.length
                }
                else -> { append(c); i++ }
            }
        }
    }
}

/** Where a tapped file path goes (the chat sets it); null leaves paths as plain text. */
val LocalPathTap = androidx.compose.runtime.staticCompositionLocalOf<((String) -> Unit)?> { null }
/** Where a tapped link goes (the chat asks: open or copy); null opens it directly. */
val LocalLinkTap = androidx.compose.runtime.staticCompositionLocalOf<((String) -> Unit)?> { null }

/** Draw markdown. `compact` tightens spacing (chat bubbles, notes previews). */
@Composable
fun MarkdownText(markdown: String, modifier: Modifier = Modifier, compact: Boolean = false) {
    val blocks = remember(markdown) { Markdown.parse(markdown) }
    Column(modifier, verticalArrangement = Arrangement.spacedBy(if (compact) 6.dp else 10.dp)) {
        for (b in blocks) MarkdownBlock(b)
    }
}

@Composable
private fun MarkdownBlock(b: Markdown.Block) {
    val onPath = LocalPathTap.current
    val onLink = LocalLinkTap.current
    when (b) {
        is Markdown.Heading -> Text(
            Markdown.inline(b.text, onPath = onPath, onLink = onLink),
            style = CrundiType.Heading.copy(fontSize = when (b.level) { 1 -> 19.sp; 2 -> 17.sp; 3 -> 15.sp; else -> 14.sp }),
        )
        is Markdown.Paragraph -> Text(Markdown.inline(b.text, onPath = onPath, onLink = onLink), style = CrundiType.Body)
        is Markdown.Quote -> Row(Modifier.height(IntrinsicSize.Min)) {
            Box(Modifier.width(3.dp).fillMaxHeight().background(CrundiColors.Border))
            Text(Markdown.inline(b.text, onPath = onPath, onLink = onLink), style = CrundiType.Body.copy(color = CrundiColors.TextSecondary), modifier = Modifier.padding(start = 10.dp))
        }
        is Markdown.Fence -> CodeBlock(b.code, language = b.language)
        is Markdown.Rule -> CrundiDivider(strong = true)
        is Markdown.Table -> MarkdownTable(b.rows, onPath, onLink)
        is Markdown.ListBlock -> Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            b.items.forEachIndexed { idx, (depth, text) ->
                Row(Modifier.padding(start = (depth * 16).dp)) {
                    Text(
                        if (b.ordered) "${b.start + idx}." else "•",
                        style = CrundiType.Body.copy(color = CrundiColors.TextSecondary),
                        modifier = Modifier.widthIn(min = 18.dp),
                    )
                    Text(Markdown.inline(text, onPath = onPath, onLink = onLink), style = CrundiType.Body, modifier = Modifier.padding(start = 6.dp))
                }
            }
        }
    }
}

/**
 * Plain text (a message you wrote) with only its links and absolute paths
 * made tappable, nothing else styled: what the web's linkify does to your
 * own messages.
 */
fun linkifyPlain(src: String, onPath: ((String) -> Unit)?, onLink: ((String) -> Unit)?): AnnotatedString = buildAnnotatedString {
    val re = Regex("(https?://[^\\s<`]+)|((?<=^|[\\s(\\[\"'])/(?:[\\w.@+~%,=-]+/)+[\\w.@+~%,=-]+(?::\\d+(?::\\d+)?)?)")
    var i = 0
    for (m in re.findAll(src)) {
        if (m.range.first < i) continue
        append(src.substring(i, m.range.first))
        val raw = m.value.trimEnd('.', ',', ';', ':', '!', '?', ')', ']', '\'', '"')
        val style = TextLinkStyles(SpanStyle(textDecoration = TextDecoration.Underline))
        val isUrl = raw.startsWith("http")
        val handler = if (isUrl) onLink else onPath
        if (handler != null) withLink(LinkAnnotation.Clickable((if (isUrl) "link:" else "path:") + raw, style) { handler(raw) }) { append(raw) }
        else if (isUrl) withLink(LinkAnnotation.Url(raw, style)) { append(raw) } else append(raw)
        i = m.range.first + raw.length
    }
    append(src.substring(i))
}

/**
 * A table as the web draws one: a header row, cells with their own
 * formatting (code, links, paths), hairline rules, sideways scrolling when
 * wide, and a copy button for the table's text.
 */
@Composable
private fun MarkdownTable(rows: List<List<String>>, onPath: ((String) -> Unit)?, onLink: ((String) -> Unit)?) {
    if (rows.isEmpty()) return
    val cols = rows.maxOf { it.size }
    val clipboard = androidx.compose.ui.platform.LocalClipboardManager.current
    var copied by androidx.compose.runtime.remember { androidx.compose.runtime.mutableStateOf(false) }
    androidx.compose.runtime.LaunchedEffect(copied) { if (copied) { kotlinx.coroutines.delay(1500); copied = false } }
    val shape = Dimens.ShapeSm
    Column(Modifier.fillMaxWidth().border(Dimens.Hairline, CrundiColors.Border, shape).clip(shape)) {
        Row(Modifier.fillMaxWidth().background(CrundiColors.BgPrimary).padding(start = 10.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("table", style = CrundiType.Meta, modifier = Modifier.weight(1f))
            CrundiIconButton(if (copied) "check" else "copy", {
                clipboard.setText(AnnotatedString(rows.joinToString("\n") { r -> r.joinToString("\t") }))
                copied = true
            }, contentDescription = "Copy table", size = 14.dp, touch = 32.dp, tint = if (copied) CrundiColors.Green else CrundiColors.TextMuted)
        }
        Box(Modifier.fillMaxWidth().height(Dimens.Hairline).background(CrundiColors.Border))
        Box(Modifier.horizontalScroll(androidx.compose.foundation.rememberScrollState())) {
            // Columns as wide as their widest cell, up to a limit, then wrapping.
            androidx.compose.ui.layout.Layout(
                content = {
                    rows.forEachIndexed { r, row ->
                        for (c in 0 until cols) {
                            val cell = row.getOrNull(c).orEmpty()
                            Text(
                                Markdown.inline(cell, onPath = onPath, onLink = onLink),
                                style = CrundiType.Body.copy(fontWeight = if (r == 0) FontWeight.SemiBold else null),
                                modifier = Modifier.background(if (r == 0) CrundiColors.BgTertiary else androidx.compose.ui.graphics.Color.Transparent)
                                    .border(androidx.compose.ui.unit.Dp.Hairline, CrundiColors.Border.copy(alpha = 0.6f)).padding(horizontal = 10.dp, vertical = 6.dp),
                            )
                        }
                    }
                },
            ) { measurables, _ ->
                val maxCol = 260.dp.roundToPx()
                val widths = IntArray(cols) { c -> (0 until rows.size).maxOf { r -> measurables[r * cols + c].maxIntrinsicWidth(Int.MAX_VALUE) }.coerceAtMost(maxCol) }
                // Every cell in a row as tall as the tallest, so the rules line up
                // (sizes from intrinsics: a cell may be measured only once).
                val heights = IntArray(rows.size) { r -> (0 until cols).maxOf { c -> measurables[r * cols + c].maxIntrinsicHeight(widths[c]) } }
                val stretched = measurables.mapIndexed { i, m -> m.measure(androidx.compose.ui.unit.Constraints.fixed(widths[i % cols], heights[i / cols])) }
                layout(widths.sum(), heights.sum()) {
                    var y = 0
                    for (r in rows.indices) {
                        var x = 0
                        for (c in 0 until cols) { stretched[r * cols + c].place(x, y); x += widths[c] }
                        y += heights[r]
                    }
                }
            }
        }
    }
}
