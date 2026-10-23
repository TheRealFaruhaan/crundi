package mv.obitech.crundi.ui.workspace.terminal

/**
 * A terminal screen in memory: what xterm.js is on the desktop, cut down to
 * what a phone needs to show Claude, a shell and the usual full-screen
 * programs. Feed it the server's output ([write]); read back lines of cells
 * ([lineCount], [line]) and the cursor. Pure Kotlin, no Android, so it is
 * tested on the JVM.
 *
 * Covers: UTF-8 text (as decoded Kotlin strings, wide characters take two
 * cells), C0 controls, ESC and CSI sequences for cursor movement, erasing,
 * inserting and deleting, scroll regions, SGR colours (16, 256 and RGB) and
 * attributes, the alternate screen, cursor save/restore, the modes programs
 * switch (application cursor keys, autowrap, cursor visible, bracketed
 * paste, insert), device status answers, and OSC titles. Everything else is
 * read and ignored, so unknown sequences never show up as text.
 */
class TermEmulator(cols: Int = 80, rows: Int = 24, private val scrollbackMax: Int = 5000) {
    var cols = cols.coerceAtLeast(2); private set
    var rows = rows.coerceAtLeast(2); private set

    /** One row of cells: code points (0 empty, [WIDE_TAIL] the right half of a wide one) and packed styles. */
    class Line(n: Int) {
        var chars = IntArray(n); internal set
        var styles = LongArray(n); internal set
        /** This row continues onto the next (autowrap), so copying joins them. */
        var wrapped = false
        internal fun resize(n: Int) { chars = chars.copyOf(n); styles = styles.copyOf(n) }
        fun text(): String = buildString { for (c in chars) if (c != WIDE_TAIL) appendCodePoint(if (c == 0) ' '.code else c) }.trimEnd()
    }

    private var main = MutableList(this.rows) { Line(this.cols) }
    private var alt = MutableList(this.rows) { Line(this.cols) }
    private val scrollback = ArrayDeque<Line>()
    private var screen = main
    val altScreen get() = screen === alt

    var cursorX = 0; private set
    var cursorY = 0; private set
    private var wrapPending = false
    private var style = 0L
    private var top = 0
    private var bottom = this.rows - 1
    private var saved = Saved(0, 0, 0L)
    private data class Saved(val x: Int, val y: Int, val style: Long)

    // Modes
    var cursorVisible = true; private set
    /** DECCKM: arrows send ESC O A rather than ESC [ A. The keyboard row asks this. */
    var appCursorKeys = false; private set
    var bracketedPaste = false; private set
    private var autowrap = true
    private var insertMode = false

    /** The window title a program set (OSC 0/2), or null. */
    var title: String? = null; private set

    /** Answers the terminal owes the program (cursor position, device attributes): send them as input. */
    var respond: (String) -> Unit = {}

    /** Bumped on every change, for redrawing. */
    var version = 0L; private set

    // ─── Reading ───

    /** Scrollback lines plus the screen's rows (the alternate screen has no scrollback). */
    val lineCount get() = (if (altScreen) 0 else scrollback.size) + rows
    /** How many of [lineCount] are scrollback (above the screen). */
    val scrollbackSize get() = if (altScreen) 0 else scrollback.size
    fun line(i: Int): Line = if (altScreen) screen[i] else if (i < scrollback.size) scrollback[i] else screen[i - scrollback.size]
    /** The screen's text (no scrollback), for tests and copying. */
    fun screenText(): String = screen.joinToString("\n") { it.text() }.trimEnd('\n')

    // ─── Parser ───

    private enum class St { Ground, Esc, Csi, Osc, OscEsc, Charset, Dcs, DcsEsc }
    private var st = St.Ground
    private val params = StringBuilder()
    private val osc = StringBuilder()
    private var highSurrogate: Char? = null

    fun write(data: String) {
        for (ch in data) {
            when (st) {
                St.Ground -> ground(ch)
                St.Esc -> esc(ch)
                St.Csi -> csi(ch)
                St.Osc -> when (ch) {
                    '\u0007' -> { endOsc(); st = St.Ground }
                    '\u001b' -> st = St.OscEsc
                    else -> if (osc.length < 4096) osc.append(ch)
                }
                St.OscEsc -> { endOsc(); st = St.Ground; if (ch != '\\') esc(ch) }
                St.Charset -> st = St.Ground
                St.Dcs -> if (ch == '\u001b') st = St.DcsEsc else if (ch == '\u0007') st = St.Ground
                St.DcsEsc -> st = if (ch == '\\') St.Ground else St.Dcs
            }
        }
        version++
    }

    private fun ground(ch: Char) {
        when {
            ch == '\u001b' -> { st = St.Esc }
            ch.code < 0x20 || ch.code == 0x7f -> control(ch)
            ch.isHighSurrogate() -> highSurrogate = ch
            ch.isLowSurrogate() -> { val h = highSurrogate; highSurrogate = null; if (h != null) print(Character.toCodePoint(h, ch)) }
            else -> print(ch.code)
        }
    }

    private fun control(ch: Char) {
        when (ch) {
            '\r' -> { cursorX = 0; wrapPending = false }
            '\n', '\u000b', '\u000c' -> lineFeed()
            '\b' -> { if (wrapPending) wrapPending = false else if (cursorX > 0) cursorX-- }
            '\t' -> { cursorX = ((cursorX / 8) + 1) * 8; if (cursorX >= cols) cursorX = cols - 1; wrapPending = false }
            else -> {} // BEL, SO, SI, NUL, DEL: nothing to draw
        }
    }

    private fun esc(ch: Char) {
        st = St.Ground
        when (ch) {
            '[' -> { params.clear(); st = St.Csi }
            ']' -> { osc.clear(); st = St.Osc }
            'P', 'X', '^', '_' -> st = St.Dcs
            '(', ')', '*', '+', '-', '.', '/', '#', '%', ' ' -> st = St.Charset
            '7' -> saveCursor()
            '8' -> restoreCursor()
            'D' -> lineFeed()
            'E' -> { cursorX = 0; lineFeed() }
            'M' -> reverseIndex()
            'c' -> reset()
            else -> {} // '=', '>', 'H' and the like: modes we do not need
        }
    }

    private fun csi(ch: Char) {
        if (ch.code in 0x30..0x3f || ch.code in 0x20..0x2f) { if (params.length < 64) params.append(ch); return }
        st = St.Ground
        if (ch.code < 0x40 || ch.code > 0x7e) return
        val raw = params.toString()
        val priv = raw.firstOrNull()?.takeIf { it == '?' || it == '>' || it == '<' || it == '=' }
        val inter = raw.lastOrNull()?.takeIf { it.code in 0x20..0x2f }
        val body = raw.removePrefix(priv?.toString() ?: "").let { if (inter != null) it.dropLast(1) else it }
        val p = body.split(';').map { it.substringBefore(':').toIntOrNull() ?: -1 }
        fun n(i: Int, def: Int = 1) = p.getOrNull(i)?.takeIf { it > 0 } ?: def
        if (inter != null) return // DECSCUSR (cursor shape) and friends
        if (priv == '>' || priv == '<' || priv == '=') { if (ch == 'c' && priv == '>') respond("\u001b[>0;10;1c"); return }
        when (ch) {
            'A' -> moveTo(cursorX, maxOf(if (cursorY >= top) top else 0, cursorY - n(0)))
            'B', 'e' -> moveTo(cursorX, minOf(if (cursorY <= bottom) bottom else rows - 1, cursorY + n(0)))
            'C', 'a' -> moveTo(cursorX + n(0), cursorY)
            'D' -> moveTo(cursorX - n(0), cursorY)
            'E' -> moveTo(0, cursorY + n(0))
            'F' -> moveTo(0, cursorY - n(0))
            'G', '`' -> moveTo(n(0) - 1, cursorY)
            'd' -> moveTo(cursorX, n(0) - 1)
            'H', 'f' -> moveTo(n(1) - 1, n(0) - 1)
            'J' -> eraseDisplay(p.getOrNull(0)?.coerceAtLeast(0) ?: 0)
            'K' -> eraseLine(p.getOrNull(0)?.coerceAtLeast(0) ?: 0)
            'L' -> if (cursorY in top..bottom) { scrollDown(cursorY, bottom, n(0)); cursorX = 0 }
            'M' -> if (cursorY in top..bottom) { scrollUp(cursorY, bottom, n(0), toScrollback = false); cursorX = 0 }
            'P' -> deleteChars(n(0))
            '@' -> insertChars(n(0))
            'X' -> { val l = screen[cursorY]; for (x in cursorX until minOf(cols, cursorX + n(0))) { l.chars[x] = 0; l.styles[x] = blankStyle() } }
            'S' -> scrollUp(top, bottom, n(0), toScrollback = false)
            'T' -> scrollDown(top, bottom, n(0))
            'b' -> lastPrinted?.let { c -> repeat(n(0).coerceAtMost(cols * rows)) { print(c) } }
            'r' -> {
                val t = n(0) - 1; val b = n(1, rows) - 1
                if (t < b && b < rows) { top = t; bottom = b } else { top = 0; bottom = rows - 1 }
                moveTo(0, 0)
            }
            'm' -> sgr(body)
            's' -> saveCursor()
            'u' -> restoreCursor()
            'h', 'l' -> setModes(priv == '?', p, ch == 'h')
            'n' -> when (p.getOrNull(0)) {
                5 -> respond("\u001b[0n")
                6 -> respond("\u001b[${cursorY + 1};${cursorX + 1}R")
            }
            'c' -> respond("\u001b[?1;2c")
            else -> {} // t (window ops), q, and the rest
        }
    }

    private fun endOsc() {
        val s = osc.toString()
        val code = s.substringBefore(';')
        if ((code == "0" || code == "2") && s.contains(';')) title = s.substringAfter(';').take(200)
        osc.clear()
    }

    private fun setModes(private: Boolean, p: List<Int>, on: Boolean) {
        for (m in p) {
            if (!private) { if (m == 4) insertMode = on; continue }
            when (m) {
                1 -> appCursorKeys = on
                7 -> autowrap = on
                25 -> cursorVisible = on
                2004 -> bracketedPaste = on
                47, 1047 -> useAlt(on, clear = false)
                1049 -> { if (on) { saveCursor(); useAlt(true, clear = true) } else { useAlt(false, clear = false); restoreCursor() } }
            }
        }
    }

    private fun useAlt(on: Boolean, clear: Boolean) {
        if (on == altScreen) { if (on && clear) alt.forEach { clearLine(it) }; return }
        screen = if (on) alt else main
        if (on && clear) alt.forEach { clearLine(it) }
        wrapPending = false
    }

    // ─── SGR ───

    private fun sgr(body: String) {
        // Colons (38:2::r:g:b) are sub-parameters: treat them as separators.
        val p = if (body.isEmpty()) listOf(0) else body.replace(':', ';').split(';').map { it.toIntOrNull() ?: 0 }
        var i = 0
        while (i < p.size) {
            when (val c = p[i]) {
                0 -> style = 0L
                1 -> style = style or (BOLD shl FLAG_SHIFT)
                2 -> style = style or (DIM shl FLAG_SHIFT)
                3 -> style = style or (ITALIC shl FLAG_SHIFT)
                4 -> style = style or (UNDERLINE shl FLAG_SHIFT)
                7 -> style = style or (INVERSE shl FLAG_SHIFT)
                8 -> style = style or (HIDDEN shl FLAG_SHIFT)
                9 -> style = style or (STRIKE shl FLAG_SHIFT)
                21, 22 -> style = style and ((BOLD or DIM) shl FLAG_SHIFT).inv()
                23 -> style = style and (ITALIC shl FLAG_SHIFT).inv()
                24 -> style = style and (UNDERLINE shl FLAG_SHIFT).inv()
                27 -> style = style and (INVERSE shl FLAG_SHIFT).inv()
                28 -> style = style and (HIDDEN shl FLAG_SHIFT).inv()
                29 -> style = style and (STRIKE shl FLAG_SHIFT).inv()
                in 30..37 -> style = withFg(style, c - 30 + 1L)
                39 -> style = withFg(style, 0)
                in 40..47 -> style = withBg(style, c - 40 + 1L)
                49 -> style = withBg(style, 0)
                in 90..97 -> style = withFg(style, c - 90 + 8 + 1L)
                in 100..107 -> style = withBg(style, c - 100 + 8 + 1L)
                38, 48 -> {
                    val mode = p.getOrNull(i + 1)
                    var color: Long? = null
                    if (mode == 5) { color = (p.getOrNull(i + 2) ?: 0).coerceIn(0, 255) + 1L; i += 2 }
                    else if (mode == 2) {
                        // 38;2;r;g;b, or 38:2:cs:r:g:b with an empty colour space
                        val rest = p.drop(i + 2)
                        val rgb = if (rest.size >= 4 && body.contains(':')) rest.drop(1) else rest
                        val r = rgb.getOrNull(0) ?: 0; val g = rgb.getOrNull(1) ?: 0; val b = rgb.getOrNull(2) ?: 0
                        color = RGB or ((r.coerceIn(0, 255).toLong() shl 16) or (g.coerceIn(0, 255).toLong() shl 8) or b.coerceIn(0, 255).toLong())
                        i += 4 + (if (rest.size >= 4 && body.contains(':')) 1 else 0)
                    }
                    if (color != null) style = if (c == 38) withFg(style, color) else withBg(style, color)
                }
                else -> {}
            }
            i++
        }
    }

    // ─── Printing and moving ───

    private var lastPrinted: Int? = null

    private fun print(cp: Int) {
        val w = charWidth(cp)
        if (w == 0) return
        lastPrinted = cp
        if (wrapPending) {
            if (autowrap) { screen[cursorY].wrapped = true; cursorX = 0; lineFeed() }
            wrapPending = false
        }
        if (w == 2 && cursorX == cols - 1) {
            // A wide character never splits: it goes to the next row.
            screen[cursorY].chars[cursorX] = 0
            if (autowrap) { screen[cursorY].wrapped = true; cursorX = 0; lineFeed() } else return
        }
        val l = screen[cursorY]
        if (insertMode) shiftRight(l, cursorX, w)
        // Writing over half of a wide character clears the other half.
        if (l.chars[cursorX] == WIDE_TAIL && cursorX > 0) l.chars[cursorX - 1] = 0
        l.chars[cursorX] = cp; l.styles[cursorX] = style
        if (w == 2) { l.chars[cursorX + 1] = WIDE_TAIL; l.styles[cursorX + 1] = style }
        else if (cursorX + 1 < cols && l.chars[cursorX + 1] == WIDE_TAIL) l.chars[cursorX + 1] = 0
        cursorX += w
        if (cursorX >= cols) { cursorX = cols - 1; wrapPending = true }
    }

    private fun moveTo(x: Int, y: Int) {
        cursorX = x.coerceIn(0, cols - 1)
        cursorY = y.coerceIn(0, rows - 1)
        wrapPending = false
    }

    private fun lineFeed() {
        wrapPending = false
        if (cursorY == bottom) scrollUp(top, bottom, 1, toScrollback = !altScreen && top == 0)
        else if (cursorY < rows - 1) cursorY++
    }

    private fun reverseIndex() {
        wrapPending = false
        if (cursorY == top) scrollDown(top, bottom, 1) else if (cursorY > 0) cursorY--
    }

    private fun scrollUp(from: Int, to: Int, n: Int, toScrollback: Boolean) {
        repeat(n.coerceAtMost(to - from + 1)) {
            val gone = screen.removeAt(from)
            if (toScrollback) {
                scrollback.addLast(gone)
                while (scrollback.size > scrollbackMax) scrollback.removeFirst()
                screen.add(to, Line(cols).also { fillBlank(it) })
            } else { clearLine(gone); screen.add(to, gone) }
        }
    }

    private fun scrollDown(from: Int, to: Int, n: Int) {
        repeat(n.coerceAtMost(to - from + 1)) {
            val gone = screen.removeAt(to)
            clearLine(gone)
            screen.add(from, gone)
        }
    }

    private fun eraseDisplay(mode: Int) {
        when (mode) {
            0 -> { eraseLine(0); for (y in cursorY + 1 until rows) clearLine(screen[y]) }
            1 -> { eraseLine(1); for (y in 0 until cursorY) clearLine(screen[y]) }
            2 -> screen.forEach { clearLine(it) }
            3 -> if (!altScreen) scrollback.clear()
        }
    }

    private fun eraseLine(mode: Int) {
        val l = screen[cursorY]
        val (a, b) = when (mode) { 0 -> cursorX to cols; 1 -> 0 to cursorX + 1; else -> 0 to cols }
        for (x in a until b.coerceAtMost(cols)) { l.chars[x] = 0; l.styles[x] = blankStyle() }
        if (mode != 1) l.wrapped = false
    }

    private fun deleteChars(n: Int) {
        val l = screen[cursorY]
        val k = n.coerceAtMost(cols - cursorX)
        for (x in cursorX until cols) {
            val src = x + k
            l.chars[x] = if (src < cols) l.chars[src] else 0
            l.styles[x] = if (src < cols) l.styles[src] else blankStyle()
        }
        wrapPending = false
    }

    private fun insertChars(n: Int) { shiftRight(screen[cursorY], cursorX, n); wrapPending = false }

    private fun shiftRight(l: Line, at: Int, n: Int) {
        val k = n.coerceAtMost(cols - at)
        for (x in cols - 1 downTo at + k) { l.chars[x] = l.chars[x - k]; l.styles[x] = l.styles[x - k] }
        for (x in at until at + k) { l.chars[x] = 0; l.styles[x] = blankStyle() }
    }

    /** Erased cells keep the current background, as xterm does. */
    private fun blankStyle(): Long = style and (MASK shl BG_SHIFT)
    private fun clearLine(l: Line) { fillBlank(l); l.wrapped = false }
    private fun fillBlank(l: Line) { val s = blankStyle(); l.chars.fill(0); l.styles.fill(s) }

    private fun saveCursor() { saved = Saved(cursorX, cursorY, style) }
    private fun restoreCursor() { cursorX = saved.x.coerceIn(0, cols - 1); cursorY = saved.y.coerceIn(0, rows - 1); style = saved.style; wrapPending = false }

    fun reset() {
        style = 0; main = MutableList(rows) { Line(cols) }; alt = MutableList(rows) { Line(cols) }; screen = main
        scrollback.clear(); cursorX = 0; cursorY = 0; top = 0; bottom = rows - 1
        cursorVisible = true; appCursorKeys = false; bracketedPaste = false; autowrap = true; insertMode = false; wrapPending = false
        version++
    }

    /**
     * New size in cells. No reflow (as xterm): rows are cut or padded. A
     * shorter screen pushes its top rows into the scrollback so the cursor
     * stays in view; a taller one pulls them back.
     */
    fun resize(newCols: Int, newRows: Int) {
        val c = newCols.coerceAtLeast(2); val r = newRows.coerceAtLeast(2)
        if (c == cols && r == rows) return
        for (buf in listOf(main, alt)) {
            for (l in buf) l.resize(c)
        }
        for (l in scrollback) if (l.chars.size != c) l.resize(c)
        if (r < rows) {
            val drop = (cursorY - (r - 1)).coerceAtLeast(0)
            repeat(drop) { scrollback.addLast(main.removeAt(0)); alt.removeAt(0) }
            while (main.size > r) main.removeAt(main.size - 1)
            while (alt.size > r) alt.removeAt(alt.size - 1)
            cursorY -= drop
        } else if (r > rows) {
            var pulled = 0
            while (main.size < r) {
                if (screen === main && scrollback.isNotEmpty() && pulled < r - rows) { main.add(0, scrollback.removeLast()); pulled++ }
                else main.add(Line(c))
            }
            while (alt.size < r) alt.add(Line(c))
            if (screen === main) cursorY += pulled
        }
        while (scrollback.size > scrollbackMax) scrollback.removeFirst()
        cols = c; rows = r
        screen = if (screen === alt) alt else main
        top = 0; bottom = rows - 1
        cursorX = cursorX.coerceIn(0, cols - 1); cursorY = cursorY.coerceIn(0, rows - 1); wrapPending = false
        version++
    }

    companion object {
        const val WIDE_TAIL = -1
        const val BOLD = 1L; const val DIM = 2L; const val ITALIC = 4L; const val UNDERLINE = 8L
        const val INVERSE = 16L; const val HIDDEN = 32L; const val STRIKE = 64L
        /** A packed colour: 0 default, 1..256 a palette index + 1, or [RGB] with 24 bits of colour. */
        const val RGB = 1L shl 24
        const val MASK = (1L shl 26) - 1
        const val BG_SHIFT = 26
        const val FLAG_SHIFT = 52

        fun fg(style: Long): Long = style and MASK
        fun bg(style: Long): Long = (style shr BG_SHIFT) and MASK
        fun flags(style: Long): Long = (style shr FLAG_SHIFT) and 0xff
        private fun withFg(s: Long, c: Long) = (s and MASK.inv()) or (c and MASK)
        private fun withBg(s: Long, c: Long) = (s and (MASK shl BG_SHIFT).inv()) or ((c and MASK) shl BG_SHIFT)

        /** Cells a code point takes: 0 for joiners and combining marks, 2 for East Asian wide and emoji. */
        fun charWidth(cp: Int): Int = when {
            cp in 0x0300..0x036F || cp in 0x200B..0x200F || cp in 0xFE00..0xFE0F || cp == 0x20E3 || cp in 0x1F3FB..0x1F3FF -> 0
            cp in 0x1100..0x115F || cp in 0x2E80..0x303E || cp in 0x3041..0x33FF || cp in 0x3400..0x4DBF || cp in 0x4E00..0x9FFF ||
                cp in 0xA000..0xA4CF || cp in 0xAC00..0xD7A3 || cp in 0xF900..0xFAFF || cp in 0xFE30..0xFE4F || cp in 0xFF00..0xFF60 ||
                cp in 0xFFE0..0xFFE6 || cp in 0x1F300..0x1F64F || cp in 0x1F900..0x1F9FF || cp in 0x1FA70..0x1FAFF || cp in 0x20000..0x3FFFD -> 2
            else -> 1
        }
    }
}
