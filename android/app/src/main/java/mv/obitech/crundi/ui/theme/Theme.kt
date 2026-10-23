package mv.obitech.crundi.ui.theme

import androidx.compose.foundation.text.selection.LocalTextSelectionColors
import androidx.compose.foundation.text.selection.TextSelectionColors
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.material3.LocalTextStyle

private val scheme = darkColorScheme(
    primary = CrundiColors.Accent,
    onPrimary = CrundiColors.TextPrimary,
    primaryContainer = CrundiColors.AccentDim,
    onPrimaryContainer = CrundiColors.AccentHover,
    secondary = CrundiColors.AccentHover,
    onSecondary = CrundiColors.BgPrimary,
    tertiary = CrundiColors.Amber,
    background = CrundiColors.BgPrimary,
    onBackground = CrundiColors.TextPrimary,
    surface = CrundiColors.BgSecondary,
    onSurface = CrundiColors.TextPrimary,
    surfaceVariant = CrundiColors.BgTertiary,
    onSurfaceVariant = CrundiColors.TextSecondary,
    surfaceContainer = CrundiColors.BgSecondary,
    surfaceContainerHigh = CrundiColors.BgTertiary,
    surfaceContainerHighest = CrundiColors.BgHover,
    surfaceContainerLow = CrundiColors.BgCard,
    surfaceContainerLowest = CrundiColors.BgPrimary,
    outline = CrundiColors.Border,
    outlineVariant = CrundiColors.BorderSubtle,
    error = CrundiColors.Red,
    onError = CrundiColors.TextPrimary,
    errorContainer = CrundiColors.RedDim,
    scrim = CrundiColors.Scrim,
)

private val typography = Typography(
    titleLarge = CrundiType.Title,
    titleMedium = CrundiType.Heading,
    bodyLarge = CrundiType.Body,
    bodyMedium = CrundiType.Body,
    bodySmall = CrundiType.Small,
    labelLarge = CrundiType.Button,
    labelMedium = CrundiType.Tab,
    labelSmall = CrundiType.Meta,
)

/**
 * Every screen of the app runs inside this. It sets Material 3 to the
 * desktop's colours and type so that the few Material pieces we use (drawer,
 * text field cursor, ripple) match, but screens draw with the Crundi*
 * components and CrundiColors / CrundiType / Dimens directly.
 */
@Composable
fun CrundiTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = scheme, typography = typography) {
        CompositionLocalProvider(
            LocalContentColor provides CrundiColors.TextPrimary,
            LocalTextStyle provides CrundiType.Body,
            LocalTextSelectionColors provides TextSelectionColors(CrundiColors.Accent, CrundiColors.AccentDim),
            content = content,
        )
    }
}
