package mv.obitech.crundi.ui.workspace.terminal

import androidx.compose.foundation.background
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.DpOffset
import kotlinx.coroutines.launch
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.sin
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
 * that until the person scrolls back down (or taps the round arrow).
 *
 * As on the mobile web: a vertical swipe scrolls the scrollback; a long
 * press opens the radial key menu ([TermRadial], release over a key sends
 * it through [onKey]); "Select" swaps in plain selectable text so a drag
 * selects instead of scrolling, "Done" swaps back.
 */
@Composable
fun TerminalView(
    emu: TermEmulator,
    version: Long,
    fontSize: Int,
    modifier: Modifier = Modifier,
    onSize: (cols: Int, rows: Int) -> Unit = { _, _ -> },
    onKey: ((String) -> Unit)? = null,
    selecting: Boolean = false,
    onSelect: (() -> Unit)? = null,
    radialAt: Offset? = null,
    /** Follow new output to the bottom (false only for pictures of a scrolled-up view). */
    follow: Boolean = true,
) {
    val style = remember(fontSize) { termTextStyle(fontSize) }
    val measurer = rememberTextMeasurer()
    val density = LocalDensity.current
    val cell = remember(fontSize, density) { measurer.measure("MMMMMMMMMM", style).size.width / 10f }
    val rowH = with(density) { style.lineHeight.toDp() }
    val scope = rememberCoroutineScope()
    BoxWithConstraints(modifier.background(TermPalette.Background)) {
        val box = this
        val cols = with(density) { ((maxWidth - 12.dp).toPx() / cell).toInt() }.coerceIn(20, 300)
        val rows = ((maxHeight - 8.dp) / rowH).toInt().coerceIn(4, 200)
        LaunchedEffect(cols, rows) { onSize(cols, rows) }
        val list = rememberLazyListState()
        val atBottom by remember { derivedStateOf { val info = list.layoutInfo; info.visibleItemsInfo.lastOrNull()?.index?.let { it >= info.totalItemsCount - 2 } ?: true } }
        val count = emu.lineCount
        LaunchedEffect(version, count) { if (follow && atBottom && count > 0) list.scrollToItem(count - 1) }
        var radial by remember { mutableStateOf(radialAt?.let { RadialState(it, null) }) }
        val keys = remember(emu) { radialKeys(emu) }
        val radiusPx = with(density) { RADIAL_R.toPx() }
        val edgePx = with(density) { (RADIAL_R + 26.dp).toPx() }
        val wPx = with(density) { box.maxWidth.toPx() }
        val hPx = with(density) { box.maxHeight.toPx() }
        fun clamp(o: Offset) = Offset(o.x.coerceIn(edgePx, maxOf(edgePx, wPx - edgePx)), o.y.coerceIn(edgePx, maxOf(edgePx, hPx - edgePx)))
        if (selecting) {
            // Plain text of the whole buffer, selectable; no gestures of ours on top.
            val plain = remember(version, count) { buildString { for (i in 0 until count) { if (i > 0) append('\n'); append(emu.line(i).text()) } } }
            val sel = rememberScrollState()
            LaunchedEffect(Unit) { sel.scrollTo(sel.maxValue) }
            SelectionContainer(Modifier.fillMaxSize().verticalScroll(sel).padding(horizontal = 6.dp, vertical = 4.dp)) {
                Text(plain, style = style.copy(lineHeightStyle = null), softWrap = false)
            }
        } else {
            LazyColumn(
                state = list,
                contentPadding = PaddingValues(horizontal = 6.dp, vertical = 4.dp),
                modifier = Modifier.fillMaxSize().then(
                    if (onKey == null) Modifier else Modifier.pointerInput(emu) {
                        longPressRadial(
                            onOpen = { radial = RadialState(clamp(it), null) },
                            onMove = { p -> radial = radial?.let { r -> r.copy(active = radialPick(r.center, p, radiusPx)) } },
                            onEnd = { send -> val a = radial?.active; radial = null; if (send && a != null) onKey(keys[a].send()) },
                        )
                    },
                ),
            ) {
                items(count) { i ->
                    val cursorAt = if (emu.cursorVisible && i == emu.scrollbackSize + emu.cursorY) emu.cursorX else -1
                    val text = remember(version, i, count) { termLine(emu.line(i), cursorAt) }
                    Text(text, style = style, softWrap = false, maxLines = 1, modifier = Modifier.fillMaxWidth().height(rowH))
                }
            }
        }
        if (onSelect != null) {
            val on = selecting
            Box(
                Modifier.align(Alignment.TopEnd).padding(8.dp)
                    .background(if (on) CrundiColors.Accent else CrundiColors.BgSecondary, Dimens.ShapeSm)
                    .border(Dimens.Hairline, if (on) CrundiColors.Accent else CrundiColors.Border, Dimens.ShapeSm)
                    .clickable(onClick = onSelect)
                    .padding(horizontal = 10.dp, vertical = 5.dp),
            ) { Text(if (on) "Done" else "Select", style = CrundiType.Small.copy(color = if (on) Color.White else CrundiColors.TextSecondary)) }
        }
        if (!selecting && !atBottom && count > 0) {
            Box(
                Modifier.align(Alignment.BottomCenter).padding(bottom = 16.dp).size(36.dp)
                    .shadow(6.dp, CircleShape)
                    .background(CrundiColors.BgSecondary, CircleShape)
                    .border(Dimens.Hairline, CrundiColors.Border, CircleShape)
                    .clickable { scope.launch { list.scrollToItem(count - 1) } },
                contentAlignment = Alignment.Center,
            ) { CrundiIcon("chevron-down", size = 18.dp, tint = CrundiColors.TextSecondary) }
        }
        radial?.let { r -> TermRadial(r, keys) }
    }
}

// ─── The radial key menu (long-press a terminal) ───

/** One key on the radial menu: what it shows, where it sits (degrees, 0 = right), what it sends. */
class RadialKey(val label: String, val angle: Float, val send: () -> String)

/** The web's RADIAL_SLOTS: arrows on the cardinals, Esc / Tab / ^C / Enter on the diagonals. */
fun radialKeys(emu: TermEmulator): List<RadialKey> = listOf(
    RadialKey("\u25B2", -90f) { TermKeys.arrow('A', emu.appCursorKeys) },
    RadialKey("\u25BC", 90f) { TermKeys.arrow('B', emu.appCursorKeys) },
    RadialKey("\u25C0", 180f) { TermKeys.arrow('D', emu.appCursorKeys) },
    RadialKey("\u25B6", 0f) { TermKeys.arrow('C', emu.appCursorKeys) },
    RadialKey("Esc", -135f) { TermKeys.ESC },
    RadialKey("Tab", -45f) { TermKeys.TAB },
    RadialKey("^C", 135f) { TermKeys.CTRL_C },
    RadialKey("\u23CE", 45f) { TermKeys.ENTER },
)

data class RadialState(val center: Offset, val active: Int?)

private val RADIAL_R = 80.dp

/** Which key the finger at [p] points to, from the menu's [center]: the nearest by angle, none within 30 dp of the centre. */
fun radialPick(center: Offset, p: Offset, radiusPx: Float, slots: List<Float> = listOf(-90f, 90f, 180f, 0f, -135f, -45f, 135f, 45f)): Int? {
    val d = p - center
    if (d.getDistance() < radiusPx * 30f / 80f) return null
    val ang = atan2(d.y, d.x)
    var best = Float.MAX_VALUE; var at = 0
    slots.forEachIndexed { i, deg ->
        val sa = Math.toRadians(deg.toDouble()).toFloat()
        var diff = abs(ang - sa); if (diff > PI.toFloat()) diff = 2 * PI.toFloat() - diff
        if (diff < best) { best = diff; at = i }
    }
    return at
}

/**
 * A long press (380 ms, the finger still within 12 dp) opens the menu; the
 * finger then picks a key and releasing sends it. Anything else (a swipe, a
 * tap, a second finger) is left alone, so the list scrolls as usual.
 */
private suspend fun androidx.compose.ui.input.pointer.PointerInputScope.longPressRadial(onOpen: (Offset) -> Unit, onMove: (Offset) -> Unit, onEnd: (send: Boolean) -> Unit) {
    val slop = 12.dp.toPx()
    awaitEachGesture {
        val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
        val start = down.position
        val gaveUp = withTimeoutOrNull(380L) {
            while (true) {
                val ev = awaitPointerEvent(PointerEventPass.Initial)
                val c = ev.changes.firstOrNull { it.id == down.id }
                if (c == null || !c.pressed || ev.changes.size > 1 || c.isConsumed || (c.position - start).getDistance() > slop) return@withTimeoutOrNull true
            }
            @Suppress("UNREACHABLE_CODE") true
        }
        if (gaveUp != null) return@awaitEachGesture
        onOpen(start)
        while (true) {
            val ev = awaitPointerEvent(PointerEventPass.Initial)
            val c = ev.changes.firstOrNull { it.id == down.id }
            if (c == null) { onEnd(false); return@awaitEachGesture }
            c.consume()
            if (!c.pressed) { onEnd(true); return@awaitEachGesture }
            onMove(c.position)
        }
    }
}

/** The menu itself: a dashed centre and eight round keys, the picked one lit and larger (.term-radial). */
@Composable
fun TermRadial(r: RadialState, keys: List<RadialKey>) {
    val density = LocalDensity.current
    Box(Modifier.fillMaxSize()) {
        val c = with(density) { DpOffset(r.center.x.toDp(), r.center.y.toDp()) }
        Box(
            Modifier.offset(c.x - 18.dp, c.y - 18.dp).size(36.dp)
                .background(Color(0x590A0A0F), CircleShape)
                .border(BorderStroke(2.dp, CrundiColors.TextMuted), CircleShape),
        )
        keys.forEachIndexed { i, k ->
            val a = Math.toRadians(k.angle.toDouble())
            val x = c.x + RADIAL_R * cos(a).toFloat()
            val y = c.y + RADIAL_R * sin(a).toFloat()
            val on = r.active == i
            Box(
                Modifier.offset(x - 24.dp, y - 24.dp).size(48.dp)
                    .graphicsLayer { scaleX = if (on) 1.22f else 1f; scaleY = if (on) 1.22f else 1f }
                    .shadow(8.dp, CircleShape)
                    .background(if (on) CrundiColors.Accent else CrundiColors.BgSecondary, CircleShape)
                    .border(Dimens.Hairline, if (on) CrundiColors.Accent else CrundiColors.Border, CircleShape),
                contentAlignment = Alignment.Center,
            ) { Text(k.label, style = CrundiType.Code.copy(fontSize = 13.sp, fontWeight = FontWeight.Bold, color = if (on) Color.White else CrundiColors.TextPrimary)) }
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

