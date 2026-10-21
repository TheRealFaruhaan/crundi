package mv.obitech.crundi.ui.workspace.terminal

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/**
 * The desktop's xterm theme (TERM_THEME in src/webapp-html.js), one for one.
 * Only terminals use these: everything else draws with CrundiColors.
 */
object TermPalette {
    val Background = CrundiColors.BgPrimary
    val Foreground = CrundiColors.TextPrimary
    val Cursor = CrundiColors.Accent
    val Selection = Color(0x4D6366F1)

    /** black, red, green, yellow, blue, magenta, cyan, white; then the bright ones. */
    val ansi = listOf(
        Color(0xFF1A1A28), Color(0xFFEF4444), Color(0xFF10B981), Color(0xFFF59E0B),
        Color(0xFF6366F1), Color(0xFFA855F7), Color(0xFF06B6D4), Color(0xFFE8E8F0),
        Color(0xFF5A5A78), Color(0xFFF87171), Color(0xFF34D399), Color(0xFFFBBF24),
        Color(0xFF818CF8), Color(0xFFC084FC), Color(0xFF22D3EE), Color(0xFFFFFFFF),
    )

    private val cube = intArrayOf(0, 95, 135, 175, 215, 255)

    /** A packed colour from [TermEmulator] (0 is the default, given as [default]). */
    fun color(packed: Long, default: Color): Color = when {
        packed == 0L -> default
        packed and TermEmulator.RGB != 0L -> Color(0xFF000000.toInt() or (packed and 0xFFFFFF).toInt())
        else -> index((packed - 1).toInt())
    }

    fun index(i: Int): Color = when {
        i < 16 -> ansi[i]
        i < 232 -> { val n = i - 16; Color(cube[n / 36], cube[(n / 6) % 6], cube[n % 6]) }
        else -> { val v = 8 + (i - 232) * 10; Color(v, v, v) }
    }
}

/** One terminal row as styled text: colours, bold, italic, underline, inverse, and the cursor. */
fun termLine(line: TermEmulator.Line, cursorX: Int = -1): AnnotatedString = buildAnnotatedString {
    val n = line.chars.size
    var lastNonBlank = -1
    for (x in 0 until n) if (line.chars[x] != 0 || TermEmulator.bg(line.styles[x]) != 0L || x == cursorX) lastNonBlank = x
    var x = 0
    while (x <= lastNonBlank) {
        val s = line.styles[x]
        var e = x + 1
        while (e <= lastNonBlank && line.styles[e] == s && e != cursorX && x != cursorX) e++
        val run = StringBuilder()
        for (i in x until e) { val c = line.chars[i]; if (c != TermEmulator.WIDE_TAIL) run.appendCodePoint(if (c == 0) ' '.code else c) }
        withStyle(spanOf(s, x == cursorX)) { append(run.toString()) }
        x = e
    }
}

private fun spanOf(style: Long, cursor: Boolean): SpanStyle {
    val f = TermEmulator.flags(style)
    var fgPacked = TermEmulator.fg(style)
    // Bold with one of the first 8 colours shows its bright twin, as xterm does.
    if (f and TermEmulator.BOLD != 0L && fgPacked in 1L..8L) fgPacked += 8
    var fg = TermPalette.color(fgPacked, TermPalette.Foreground)
    var bg = TermPalette.color(TermEmulator.bg(style), Color.Transparent)
    if (f and TermEmulator.INVERSE != 0L) { val t = fg; fg = if (bg == Color.Transparent) TermPalette.Background else bg; bg = t }
    if (cursor) { bg = TermPalette.Cursor; fg = TermPalette.Background }
    if (f and TermEmulator.DIM != 0L) fg = fg.copy(alpha = 0.6f)
    if (f and TermEmulator.HIDDEN != 0L) fg = Color.Transparent
    return SpanStyle(
        color = fg,
        background = bg,
        fontWeight = if (f and TermEmulator.BOLD != 0L) FontWeight.Bold else null,
        fontStyle = if (f and TermEmulator.ITALIC != 0L) FontStyle.Italic else null,
        textDecoration = when {
            f and TermEmulator.UNDERLINE != 0L && f and TermEmulator.STRIKE != 0L -> TextDecoration.combine(listOf(TextDecoration.Underline, TextDecoration.LineThrough))
            f and TermEmulator.UNDERLINE != 0L -> TextDecoration.Underline
            f and TermEmulator.STRIKE != 0L -> TextDecoration.LineThrough
            else -> null
        },
    )
}

/** The text style of terminal cells at [fontSize] sp: the desktop's mono, rows a fixed height. */
fun termTextStyle(fontSize: Int): TextStyle = CrundiType.Code.copy(
    fontSize = fontSize.sp,
    lineHeight = (fontSize * 1.25f).sp,
    color = TermPalette.Foreground,
    lineHeightStyle = LineHeightStyle(LineHeightStyle.Alignment.Center, LineHeightStyle.Trim.None),
)

/**
 * The terminal's screen and scrollback, drawn from [emu]. [version] is the
 * emulator's [TermEmulator.version] read as state, so new output redraws.
 * Measures how many cells fit and calls [onSize] (cols, rows) whenever that
 * changes, so the program on the server can lay itself out to the phone.
 * Follows the output while the view is at the bottom; scrolling up stops
 * that until the person scrolls back down.
 */
@Composable
fun TerminalView(emu: TermEmulator, version: Long, fontSize: Int, modifier: Modifier = Modifier, onSize: (cols: Int, rows: Int) -> Unit = { _, _ -> }) {
    val style = remember(fontSize) { termTextStyle(fontSize) }
    val measurer = rememberTextMeasurer()
    val density = LocalDensity.current
    val cell = remember(fontSize, density) { measurer.measure("MMMMMMMMMM", style).size.width / 10f }
    val rowH = with(density) { style.lineHeight.toDp() }
    BoxWithConstraints(modifier.background(TermPalette.Background).padding(horizontal = 6.dp, vertical = 4.dp)) {
        val cols = with(density) { (maxWidth.toPx() / cell).toInt() }.coerceIn(20, 300)
        val rows = (maxHeight / rowH).toInt().coerceIn(4, 200)
        LaunchedEffect(cols, rows) { onSize(cols, rows) }
        val list = rememberLazyListState()
        val atBottom by remember { derivedStateOf { val info = list.layoutInfo; info.visibleItemsInfo.lastOrNull()?.index?.let { it >= info.totalItemsCount - 2 } ?: true } }
        val count = emu.lineCount
        LaunchedEffect(version, count) { if (atBottom && count > 0) list.scrollToItem(count - 1) }
        SelectionContainer {
            LazyColumn(state = list, modifier = Modifier.fillMaxSize()) {
                items(count) { i ->
                    val cursorAt = if (emu.cursorVisible && i == emu.scrollbackSize + emu.cursorY) emu.cursorX else -1
                    val text = remember(version, i, count) { termLine(emu.line(i), cursorAt) }
                    Text(text, style = style, softWrap = false, maxLines = 1, modifier = Modifier.fillMaxWidth().height(rowH))
                }
            }
        }
    }
}

/** The keys a phone keyboard lacks, as the desktop's key panel sends them. */
object TermKeys {
    const val ESC = "\u001b"
    const val TAB = "\t"
    const val SHIFT_TAB = "\u001b[Z"
    const val ENTER = "\r"
    const val CTRL_C = "\u0003"
    fun arrow(dir: Char, app: Boolean) = (if (app) "\u001bO" else "\u001b[") + dir
    /** Ctrl with a key: letters and @[\]^_ to their control codes; anything else as it is. */
    fun ctrl(c: Char): String {
        val u = c.uppercaseChar()
        return when {
            u in 'A'..'Z' -> (u.code - 64).toChar().toString()
            u in "@[\\]^_" -> (u.code - 64).toChar().toString()
            c == ' ' || c == '2' -> "\u0000"
            c == '?' -> "\u007f"
            else -> c.toString()
        }
    }
}

/**
 * The row of keys under a terminal: Esc, Tab, Ctrl (held for the next key
 * typed), ^C, the arrows and Enter. Sends through [onKey].
 */
@Composable
fun TerminalKeyRow(ctrl: Boolean, onCtrl: () -> Unit, appCursor: Boolean, onKey: (String) -> Unit, modifier: Modifier = Modifier) {
    Row(
        modifier.fillMaxWidth().background(CrundiColors.BgSecondary).horizontalScroll(rememberScrollState()).padding(horizontal = 6.dp, vertical = 5.dp),
        horizontalArrangement = Arrangement.spacedBy(5.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TermKey("Esc") { onKey(TermKeys.ESC) }
        TermKey("Tab") { onKey(TermKeys.TAB) }
        TermKey("Ctrl", active = ctrl, onClick = onCtrl)
        TermKey("^C") { onKey(TermKeys.CTRL_C) }
        TermKey(icon = "chevron-left") { onKey(TermKeys.arrow('D', appCursor)) }
        TermKey(icon = "chevron-up") { onKey(TermKeys.arrow('A', appCursor)) }
        TermKey(icon = "chevron-down") { onKey(TermKeys.arrow('B', appCursor)) }
        TermKey(icon = "chevron-right") { onKey(TermKeys.arrow('C', appCursor)) }
        TermKey("Enter") { onKey(TermKeys.ENTER) }
        TermKey("⇧Tab") { onKey(TermKeys.SHIFT_TAB) }
    }
}

@Composable
private fun TermKey(label: String? = null, icon: String? = null, active: Boolean = false, onClick: () -> Unit) {
    Box(
        Modifier
            .defaultMinSize(minWidth = 44.dp)
            .height(36.dp)
            .background(if (active) CrundiColors.AccentDim else CrundiColors.BgTertiary, Dimens.ShapeSm)
            .border(Dimens.Hairline, if (active) CrundiColors.Accent else CrundiColors.Border, Dimens.ShapeSm)
            .clickable(onClick = onClick)
            .padding(horizontal = 10.dp),
        contentAlignment = Alignment.Center,
    ) {
        val c = if (active) CrundiColors.AccentHover else CrundiColors.TextPrimary
        if (icon != null) CrundiIcon(icon, size = 16.dp, tint = c)
        else Text(label.orEmpty(), style = CrundiType.Code.copy(fontSize = 12.sp, fontWeight = FontWeight.Bold, color = c))
    }
}
