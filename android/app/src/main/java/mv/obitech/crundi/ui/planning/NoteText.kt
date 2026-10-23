package mv.obitech.crundi.ui.planning

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
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType

/**
 * A note block's text. The server keeps it as sanitised inline HTML
 * (strong/b, em/i, u, s, code, a, br). On the phone it is drawn as styled
 * text, and edited as light markdown that turns back into the same HTML:
 * **bold**, *italic*, __underline__, ~~strike~~, `code`, [text](url).
 */
object NoteText {
    data class Run(val text: String, val b: Boolean = false, val i: Boolean = false, val u: Boolean = false, val s: Boolean = false, val code: Boolean = false, val href: String? = null) {
        fun sameStyle(o: Run) = b == o.b && i == o.i && u == o.u && s == o.s && code == o.code && href == o.href
    }

    // ─── HTML ───

    fun parseHtml(html: String): List<Run> {
        val out = mutableListOf<Run>()
        var b = 0; var i = 0; var u = 0; var s = 0; var code = 0; var href: String? = null
        val sb = StringBuilder()
        fun flush() { if (sb.isNotEmpty()) { out += Run(decode(sb.toString()), b > 0, i > 0, u > 0, s > 0, code > 0, href); sb.clear() } }
        var p = 0
        while (p < html.length) {
            val c = html[p]
            if (c == '<') {
                val e = html.indexOf('>', p)
                if (e < 0) { sb.append(html.substring(p)); break }
                val raw = html.substring(p + 1, e).trim()
                val closing = raw.startsWith("/")
                val name = raw.removePrefix("/").takeWhile { it.isLetterOrDigit() }.lowercase()
                flush()
                val d = if (closing) -1 else 1
                when (name) {
                    "b", "strong" -> b = (b + d).coerceAtLeast(0)
                    "i", "em" -> i = (i + d).coerceAtLeast(0)
                    "u" -> u = (u + d).coerceAtLeast(0)
                    "s", "strike", "del" -> s = (s + d).coerceAtLeast(0)
                    "code" -> code = (code + d).coerceAtLeast(0)
                    "a" -> href = if (closing) null else Regex("href\\s*=\\s*\"([^\"]*)\"|href\\s*=\\s*'([^']*)'").find(raw)?.let { decode(it.groupValues[1].ifEmpty { it.groupValues[2] }) } ?: ""
                    "br" -> if (!closing) out += Run("\n", b > 0, i > 0, u > 0, s > 0, code > 0, href)
                    "div", "p" -> if (!closing && out.isNotEmpty()) out += Run("\n")
                }
                p = e + 1
            } else { sb.append(c); p++ }
        }
        flush()
        return merge(out)
    }

    fun toHtml(runs: List<Run>): String = buildString {
        for (r in merge(runs)) {
            var t = encode(r.text).replace("\n", "<br>")
            if (r.code) t = "<code>$t</code>"
            if (r.s) t = "<s>$t</s>"
            if (r.u) t = "<u>$t</u>"
            if (r.i) t = "<em>$t</em>"
            if (r.b) t = "<strong>$t</strong>"
            if (r.href != null) t = "<a href=\"${encode(r.href)}\">$t</a>"
            append(t)
        }
    }

    // ─── Editing source (light markdown) ───

    fun toSource(runs: List<Run>): String = buildString {
        for (r in merge(runs)) {
            var t = r.text
            if (r.code) t = "`$t`"
            if (r.s) t = "~~$t~~"
            if (r.u) t = "__${t}__"
            if (r.i) t = "*$t*"
            if (r.b) t = "**$t**"
            if (r.href != null) t = "[$t](${r.href})"
            append(t)
        }
    }

    private val link = Regex("^\\[([^\\]]*)]\\(([^)\\s]*)\\)")

    fun parseSource(src: String): List<Run> {
        val out = mutableListOf<Run>()
        var b = false; var i = false; var u = false; var s = false
        val sb = StringBuilder()
        fun flush(href: String? = null) { if (sb.isNotEmpty()) { out += Run(sb.toString(), b, i, u, s, false, href); sb.clear() } }
        var p = 0
        fun opens(tok: String) = src.indexOf(tok, p + tok.length) >= 0
        while (p < src.length) {
            when {
                src[p] == '`' && src.indexOf('`', p + 1) > p -> {
                    flush(); val e = src.indexOf('`', p + 1)
                    out += Run(src.substring(p + 1, e), b, i, u, s, code = true); p = e + 1
                }
                src[p] == '[' && link.find(src.substring(p)) != null -> {
                    flush(); val m = link.find(src.substring(p))!!
                    for (r in parseSource(m.groupValues[1])) out += r.copy(b = r.b || b, i = r.i || i, u = r.u || u, s = r.s || s, href = m.groupValues[2])
                    p += m.value.length
                }
                src.startsWith("**", p) && (b || opens("**")) -> { flush(); b = !b; p += 2 }
                src.startsWith("__", p) && (u || opens("__")) -> { flush(); u = !u; p += 2 }
                src.startsWith("~~", p) && (s || opens("~~")) -> { flush(); s = !s; p += 2 }
                src[p] == '*' && (i || opens("*")) -> { flush(); i = !i; p += 1 }
                else -> { sb.append(src[p]); p++ }
            }
        }
        flush()
        return merge(out)
    }

    fun htmlToSource(html: String) = toSource(parseHtml(html))
    fun sourceToHtml(src: String) = toHtml(parseSource(src))
    fun plain(html: String) = parseHtml(html).joinToString("") { it.text }

    /** For drawing: styled text with tappable links. */
    fun annotated(runs: List<Run>): AnnotatedString = buildAnnotatedString {
        for (r in runs) {
            val st = SpanStyle(
                fontWeight = if (r.b) FontWeight.Bold else null,
                fontStyle = if (r.i) FontStyle.Italic else null,
                textDecoration = when {
                    r.u && r.s -> TextDecoration.combine(listOf(TextDecoration.Underline, TextDecoration.LineThrough))
                    r.u -> TextDecoration.Underline
                    r.s -> TextDecoration.LineThrough
                    else -> null
                },
                fontFamily = if (r.code) CrundiType.Mono else null,
                background = if (r.code) CrundiColors.BgTertiary else androidx.compose.ui.graphics.Color.Unspecified,
                color = if (r.code) CrundiColors.AccentHover else androidx.compose.ui.graphics.Color.Unspecified,
            )
            if (r.href != null) withLink(LinkAnnotation.Url(r.href, TextLinkStyles(SpanStyle(color = CrundiColors.AccentHover, textDecoration = TextDecoration.Underline)))) { withStyle(st) { append(r.text) } }
            else withStyle(st) { append(r.text) }
        }
    }

    fun annotatedHtml(html: String) = annotated(parseHtml(html))

    private fun merge(runs: List<Run>): List<Run> {
        val out = mutableListOf<Run>()
        for (r in runs) {
            if (r.text.isEmpty()) continue
            val last = out.lastOrNull()
            if (last != null && last.sameStyle(r)) out[out.size - 1] = last.copy(text = last.text + r.text) else out += r
        }
        return out
    }

    fun decode(s: String): String = s.replace("&nbsp;", "\u00A0").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&#x27;", "'").replace("&amp;", "&")
    fun encode(s: String): String = s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace("\"", "&quot;")
}
