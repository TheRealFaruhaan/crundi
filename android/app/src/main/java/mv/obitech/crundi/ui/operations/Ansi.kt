package mv.obitech.crundi.ui.operations

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import mv.obitech.crundi.ui.theme.CrundiColors

/**
 * Just enough of ANSI for logs and a container shell: SGR colours (30-37,
 * 90-97, bold, dim, reset) become spans with the desktop's palette, and every
 * other escape (cursor moves, titles) is dropped.
 */
object Ansi {
    // The desktop's dk-log palette for 30..37 (and 90..97).
    private val palette = listOf(
        Color(0xFF5A5A78), Color(0xFFF87171), Color(0xFF34D399), Color(0xFFFBBF24),
        Color(0xFF818CF8), Color(0xFFC084FC), Color(0xFF22D3EE), Color(0xFFE8E8F0),
    )
    private val esc = Regex("\u001B(\\[[0-9;?]*[ -/]*[@-~]|\\][^\u0007\u001B]*(\u0007|\u001B\\\\)|[()][0-9A-Za-z]|[@-Z\\\\-_])")

    fun strip(s: String): String = esc.replace(s, "")

    /** [base] is the line's own colour (stderr is light red). */
    fun render(s: String, base: Color = CrundiColors.TextSecondary): AnnotatedString = buildAnnotatedString {
        var color: Color? = null
        var bold = false
        var dim = false
        var at = 0
        fun put(text: String) {
            if (text.isEmpty()) return
            val c = (color ?: base).let { if (dim) it.copy(alpha = 0.65f) else it }
            pushStyle(SpanStyle(color = c, fontWeight = if (bold) FontWeight.Bold else null))
            append(text)
            pop()
        }
        for (m in esc.findAll(s)) {
            put(s.substring(at, m.range.first))
            at = m.range.last + 1
            val seq = m.value
            if (!seq.startsWith("\u001B[") || !seq.endsWith("m")) continue
            val codes = seq.substring(2, seq.length - 1).split(';').map { it.toIntOrNull() ?: 0 }
            for (code in codes) when (code) {
                0 -> { color = null; bold = false; dim = false }
                1 -> bold = true
                2 -> dim = true
                22 -> { bold = false; dim = false }
                39 -> color = null
                in 30..37 -> color = palette[code - 30]
                in 90..97 -> color = palette[code - 90]
            }
        }
        put(s.substring(at))
    }

    /**
     * Fold terminal output into lines the way a plain terminal would show
     * them: \r\n ends a line, a lone \r goes back to the line's start, a
     * backspace takes a character off. Colours are kept for [render].
     */
    fun fold(prev: List<String>, chunk: String, max: Int = 3000): List<String> {
        val lines = prev.toMutableList().ifEmpty { mutableListOf("") }
        var cur = StringBuilder(lines.removeAt(lines.size - 1))
        var i = 0
        while (i < chunk.length) {
            val ch = chunk[i]
            when {
                ch == '\n' -> { lines += cur.toString(); cur = StringBuilder() }
                ch == '\r' && (i == chunk.length - 1 || chunk[i + 1] == '\n') -> {}
                ch == '\r' -> cur = StringBuilder()
                ch == '\b' -> if (cur.isNotEmpty()) cur.setLength(cur.length - 1)
                ch == '\u0007' -> {}
                else -> cur.append(ch)
            }
            i++
        }
        lines += cur.toString()
        return if (lines.size > max) lines.takeLast(max) else lines
    }
}
