package mv.obitech.crundi.ui.chats

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import mv.obitech.crundi.api.rememberServerImage
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import kotlin.math.pow
import kotlin.math.roundToInt

/**
 * A project's mark: one or two letters on a colour worked out from the name,
 * the same tile on every device. A Kotlin copy of src/project-mark.js, which
 * says the copies must agree; ProjectMarkTest pins the values it gives.
 */
data class Mark(val initials: String, val hue: Int, val color: Color, val colorDeep: Color)

/** "my-cool_project" -> "MC", "crundi" -> "C", "" -> "?". */
fun markInitials(name: String): String {
    val words = name.split(Regex("[^\\p{L}\\p{N}]+")).filter { it.isNotEmpty() }
    if (words.isEmpty()) return "?"
    fun first(w: String) = String(Character.toChars(w.codePointAt(0)))
    return (first(words[0]) + (if (words.size > 1) first(words[1]) else "")).uppercase()
}

/** A hue from 0 to 359, the same for the same name whatever its case (FNV-1a over UTF-16 units). */
fun markHue(name: String): Int {
    var h = 0x811c9dc5L
    for (ch in name.lowercase()) { h = h xor ch.code.toLong(); h = (h * 0x01000193L) and 0xffffffffL }
    return (h % 360).toInt()
}

private fun hslToRgb(h: Int, s: Double, l: Double): Int {
    val a = s * minOf(l, 1 - l)
    fun f(n: Int): Int { val k = (n + h / 30.0) % 12; return (255 * (l - a * maxOf(-1.0, minOf(k - 3, 9 - k, 1.0)))).roundToInt() }
    return (f(0) shl 16) or (f(8) shl 8) or f(4)
}

private fun luminance(rgb: Int): Double {
    fun ch(v: Int): Double { val c = v / 255.0; return if (c <= 0.03928) c / 12.92 else ((c + 0.055) / 1.055).pow(2.4) }
    return 0.2126 * ch(rgb shr 16 and 255) + 0.7152 * ch(rgb shr 8 and 255) + 0.0722 * ch(rgb and 255)
}

/** The tile for a project, stepped darker until white letters read on it at 3 to 1. */
fun projectMark(name: String): Mark {
    val hue = markHue(name)
    var l = 0.46
    var rgb = hslToRgb(hue, 0.58, l)
    var i = 0
    while (i < 12 && 1.05 / (luminance(rgb) + 0.05) < 3) { l = Math.round((l - 0.02) * 100) / 100.0; rgb = hslToRgb(hue, 0.58, l); i++ }
    val deep = hslToRgb(hue, 0.62, Math.round(l * 74) / 100.0)
    return Mark(markInitials(name), hue, Color(0xFF000000.toInt() or rgb), Color(0xFF000000.toInt() or deep))
}

/** A chat's mark: the project's colour with the chat's own initials ("Chat" keeps the project's). */
fun chatMark(project: String, title: String): Mark {
    val base = projectMark(project)
    val t = title.trim()
    val initials = if (t.isNotEmpty() && !t.equals("chat", ignoreCase = true)) markInitials(t) else base.initials
    return base.copy(initials = if (initials == "?") base.initials else initials)
}

/** The hex the web would print for a colour (#rrggbb), for tests. */
fun Color.hex(): String = "#%06x".format((value shr 32).toLong() and 0xffffff)

/**
 * The tile itself (#cp-mark): the project's own icon when the server found one
 * (/api/project-icon?..&v=.., through the client's images, so a new v= is a new
 * picture), otherwise the letters on the mark's gradient.
 */
@Composable
fun MarkTile(mark: Mark, icon: String?, size: Dp = 38.dp, modifier: Modifier = Modifier) {
    val shape = RoundedCornerShape(size * 0.32f)
    val img = if (!icon.isNullOrEmpty()) rememberServerImage(icon, 128) else null
    if (img != null) {
        Image(img, contentDescription = null, contentScale = ContentScale.Crop, modifier = modifier.size(size).clip(shape).background(CrundiColors.BgPrimary))
        return
    }
    Box(
        modifier.size(size).clip(shape).background(Brush.linearGradient(listOf(mark.color, mark.colorDeep))),
        contentAlignment = Alignment.Center,
    ) {
        Text(mark.initials, style = CrundiType.Body.copy(color = Color.White, fontWeight = FontWeight.Bold, fontSize = (size.value * 0.4f).sp, letterSpacing = 0.3.sp))
    }
}
