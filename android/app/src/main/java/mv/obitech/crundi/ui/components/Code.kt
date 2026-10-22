package mv.obitech.crundi.ui.components

import androidx.compose.runtime.setValue
import androidx.compose.runtime.getValue
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/** The colours code is drawn in. One set for the whole app: chats, files, diffs. */
object SyntaxColors {
    val Plain = CrundiColors.TextPrimary
    val Keyword = CrundiColors.AccentHover
    val Str = CrundiColors.Green
    val Comment = CrundiColors.TextMuted
    val Number = CrundiColors.Yellow
    val Type = CrundiColors.Sky
    val Punct = CrundiColors.TextSecondary
}

/**
 * A small highlighter that is right often enough for reading code on a
 * phone: comments, strings, numbers, keywords, capitalised names as types.
 * Language-agnostic on purpose (one set of rules for the C family, scripts,
 * shells, JSON, YAML). `language` only switches comment syntax.
 */
object Highlighter {
    private val keywords = setOf(
        "abstract", "as", "async", "await", "break", "case", "catch", "class", "const", "continue", "data", "def", "default", "defer",
        "del", "delete", "do", "elif", "else", "enum", "export", "extends", "false", "final", "finally", "fn", "for", "from", "fun",
        "func", "function", "go", "if", "impl", "import", "in", "interface", "is", "lambda", "let", "match", "mod", "module", "mut",
        "new", "nil", "None", "not", "null", "object", "of", "or", "and", "override", "package", "pass", "private", "protected", "pub",
        "public", "raise", "return", "self", "static", "struct", "super", "switch", "this", "throw", "throws", "trait", "True", "False",
        "true", "try", "type", "typeof", "undefined", "use", "val", "var", "void", "when", "where", "while", "with", "yield",
        "then", "fi", "done", "esac", "echo", "local",
    )

    fun highlight(code: String, language: String? = null): AnnotatedString {
        val lang = language?.lowercase().orEmpty()
        val hashComments = lang in setOf("py", "python", "sh", "bash", "shell", "zsh", "yaml", "yml", "toml", "rb", "ruby", "dockerfile", "make", "makefile", "conf", "ini", "r")
        val slashComments = !hashComments || lang.isEmpty()
        return buildAnnotatedString {
            var i = 0
            val n = code.length
            fun put(s: String, c: androidx.compose.ui.graphics.Color, italic: Boolean = false) =
                withStyle(SpanStyle(color = c, fontStyle = if (italic) FontStyle.Italic else null)) { append(s) }
            while (i < n) {
                val ch = code[i]
                when {
                    slashComments && ch == '/' && i + 1 < n && code[i + 1] == '/' -> {
                        val e = code.indexOf('\n', i).let { if (it < 0) n else it }; put(code.substring(i, e), SyntaxColors.Comment, true); i = e
                    }
                    slashComments && ch == '/' && i + 1 < n && code[i + 1] == '*' -> {
                        val e = code.indexOf("*/", i + 2).let { if (it < 0) n else it + 2 }; put(code.substring(i, e), SyntaxColors.Comment, true); i = e
                    }
                    hashComments && ch == '#' -> {
                        val e = code.indexOf('\n', i).let { if (it < 0) n else it }; put(code.substring(i, e), SyntaxColors.Comment, true); i = e
                    }
                    ch == '"' || ch == '\'' || ch == '`' -> {
                        var e = i + 1
                        while (e < n && code[e] != ch && !(code[e] == '\n' && ch != '`')) { if (code[e] == '\\') e++; e++ }
                        e = (e + 1).coerceAtMost(n); put(code.substring(i, e), SyntaxColors.Str); i = e
                    }
                    ch.isDigit() -> {
                        var e = i; while (e < n && (code[e].isLetterOrDigit() || code[e] == '.' || code[e] == '_')) e++
                        put(code.substring(i, e), SyntaxColors.Number); i = e
                    }
                    ch.isLetter() || ch == '_' || ch == '$' -> {
                        var e = i; while (e < n && (code[e].isLetterOrDigit() || code[e] == '_' || code[e] == '$')) e++
                        val w = code.substring(i, e)
                        when {
                            w in keywords -> put(w, SyntaxColors.Keyword)
                            w[0].isUpperCase() && w.length > 1 -> put(w, SyntaxColors.Type)
                            else -> put(w, SyntaxColors.Plain)
                        }
                        i = e
                    }
                    ch in "{}[]()<>=;:,.+-*/%!&|^~?" -> { put(ch.toString(), SyntaxColors.Punct); i++ }
                    else -> { append(ch); i++ }
                }
            }
        }
    }
}

/** `code` in running text: mono, tertiary background, accent-hover text. */
val InlineCodeStyle = SpanStyle(fontFamily = CrundiType.Mono, background = CrundiColors.BgTertiary, color = CrundiColors.AccentHover)

/**
 * A block of code: --bg-primary, 1px border, radius 6, scrolls sideways
 * rather than wrapping, selectable. The language (if any) and a copy button
 * sit in a thin header.
 */
@Composable
fun CodeBlock(code: String, modifier: Modifier = Modifier, language: String? = null, highlight: Boolean = true, showHeader: Boolean = true) {
    val text = remember(code, language, highlight) { if (highlight) Highlighter.highlight(code, language) else AnnotatedString(code) }
    val clipboard = LocalClipboardManager.current
    Column(
        modifier
            .fillMaxWidth()
            .background(CrundiColors.BgPrimary, Dimens.ShapeSm)
            .border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm),
    ) {
        if (showHeader) {
            Row(Modifier.fillMaxWidth().padding(start = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(language.orEmpty(), style = CrundiType.Meta, modifier = Modifier.weight(1f))
                // Says it copied: the tick for a moment, as the web's copy button.
                var copied by androidx.compose.runtime.remember { androidx.compose.runtime.mutableStateOf(false) }
                androidx.compose.runtime.LaunchedEffect(copied) { if (copied) { kotlinx.coroutines.delay(1500); copied = false } }
                CrundiIconButton(if (copied) "check" else "copy", { clipboard.setText(AnnotatedString(code)); copied = true }, contentDescription = "Copy", size = 14.dp, touch = 32.dp, tint = if (copied) CrundiColors.Green else CrundiColors.TextMuted)
            }
            CrundiDivider()
        }
        Box(Modifier.horizontalScroll(rememberScrollState()).padding(10.dp)) {
            SelectionContainer { Text(text, style = CrundiType.Code, softWrap = false) }
        }
    }
}
