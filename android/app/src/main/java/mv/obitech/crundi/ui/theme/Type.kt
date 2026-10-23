package mv.obitech.crundi.ui.theme

import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp

/**
 * The desktop's type, on a phone. The desktop sets the system sans
 * (-apple-system, Segoe UI, system-ui) on a 16px root, and a monospace stack
 * (var(--mono)) for code, terminals, badges and meta lines. Here that is the
 * phone's own sans and monospace. Sizes are the desktop's rem values at 16px,
 * nudged to whole sp.
 */
object CrundiType {
    val Sans = FontFamily.Default
    val Mono = FontFamily.Monospace

    /** Dialog and screen titles (#import-dialog h2: 1.2rem). */
    val Title = TextStyle(fontFamily = Sans, fontSize = 19.sp, fontWeight = FontWeight.SemiBold, lineHeight = 24.sp, color = CrundiColors.TextPrimary)
    /** A heading inside a panel (0.95rem, 600). */
    val Heading = TextStyle(fontFamily = Sans, fontSize = 15.sp, fontWeight = FontWeight.SemiBold, lineHeight = 20.sp, color = CrundiColors.TextPrimary)
    /** Running text and list rows (sidebar-item 0.88rem). */
    val Body = TextStyle(fontFamily = Sans, fontSize = 14.sp, lineHeight = 20.sp, color = CrundiColors.TextPrimary)
    /** Secondary lines under a row title, descriptions (0.82rem). */
    val Small = TextStyle(fontFamily = Sans, fontSize = 13.sp, lineHeight = 18.sp, color = CrundiColors.TextSecondary)
    /** Tab labels (0.82rem, 500). */
    val Tab = TextStyle(fontFamily = Sans, fontSize = 13.sp, fontWeight = FontWeight.Medium, color = CrundiColors.TextMuted)
    /** Section heads like the sidebar's "Projects" (0.75rem, 600, uppercase, 0.05em). Uppercase the text yourself. */
    val Section = TextStyle(fontFamily = Sans, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, letterSpacing = 0.05.em, color = CrundiColors.TextMuted)
    /** Buttons (0.9rem, 600 on primary). */
    val Button = TextStyle(fontFamily = Sans, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, color = CrundiColors.TextPrimary)
    /** Code, file contents, terminal text. */
    val Code = TextStyle(fontFamily = Mono, fontSize = 13.sp, lineHeight = 19.sp, color = CrundiColors.TextPrimary)
    /** Meta lines: times, sizes, ids (.cr-meta 0.72rem mono, muted). */
    val Meta = TextStyle(fontFamily = Mono, fontSize = 11.sp, lineHeight = 15.sp, color = CrundiColors.TextMuted)
    /** Status badges (0.75rem mono). */
    val Badge = TextStyle(fontFamily = Mono, fontSize = 12.sp, color = CrundiColors.TextSecondary)
    /** The wordmark beside the mark in the top bar (mono, 700, 0.04em). */
    val Wordmark = TextStyle(fontFamily = Mono, fontSize = 16.sp, fontWeight = FontWeight.Bold, letterSpacing = 0.04.em, color = CrundiColors.TextPrimary)
}
