package mv.obitech.crundi.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/** The desktop's button kinds. */
enum class ButtonKind {
    /** Accent fill, white text: the one main thing on a screen or dialog. */
    Primary,
    /** --bg-tertiary with a border: everything else. */
    Secondary,
    /** Red text on red-dim: delete, stop, deny. */
    Danger,
    /** No background: low-weight actions in rows and bars. */
    Ghost,
}

/**
 * A button as the desktop draws one: radius 6, 9 by 14 padding, 14sp.
 * `icon` is an ICON_PATHS name shown before the label.
 */
@Composable
fun CrundiButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    kind: ButtonKind = ButtonKind.Secondary,
    icon: String? = null,
    enabled: Boolean = true,
) {
    val src = remember { MutableInteractionSource() }
    val pressed by src.collectIsPressedAsState()
    val (bg, fg, border) = when (kind) {
        ButtonKind.Primary -> Triple(if (pressed) CrundiColors.AccentHover else CrundiColors.Accent, Color.White, Color.Transparent)
        ButtonKind.Secondary -> Triple(if (pressed) CrundiColors.BgHover else CrundiColors.BgTertiary, CrundiColors.TextPrimary, CrundiColors.Border)
        ButtonKind.Danger -> Triple(if (pressed) CrundiColors.Red.copy(alpha = 0.25f) else CrundiColors.RedDim, CrundiColors.Red, Color.Transparent)
        ButtonKind.Ghost -> Triple(if (pressed) CrundiColors.BgHover else Color.Transparent, CrundiColors.TextSecondary, Color.Transparent)
    }
    Row(
        modifier = modifier
            .defaultMinSize(minHeight = 40.dp)
            .alpha(if (enabled) 1f else 0.45f)
            .clip(Dimens.ShapeSm)
            .background(bg)
            .border(Dimens.Hairline, border, Dimens.ShapeSm)
            .clickable(interactionSource = src, indication = null, enabled = enabled, role = Role.Button, onClick = onClick)
            .padding(horizontal = 14.dp, vertical = 9.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
    ) {
        if (icon != null) CrundiIcon(icon, size = Dimens.IconSm, tint = fg)
        Text(text, style = CrundiType.Button.copy(color = fg, fontWeight = if (kind == ButtonKind.Primary) CrundiType.Button.fontWeight else null))
    }
}

/**
 * An icon on its own as a button (top bar, row actions). 44dp to touch, the
 * icon at `size`. `active` colours it accent, as the desktop's active tab.
 */
@Composable
fun CrundiIconButton(
    icon: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    contentDescription: String? = null,
    size: Dp = Dimens.Icon,
    tint: Color = CrundiColors.TextSecondary,
    enabled: Boolean = true,
    touch: Dp = Dimens.Touch,
) {
    val src = remember { MutableInteractionSource() }
    val pressed by src.collectIsPressedAsState()
    Box(
        modifier = modifier
            .size(touch)
            .clip(Dimens.ShapeSm)
            .background(if (pressed) CrundiColors.BgHover else Color.Transparent)
            .clickable(interactionSource = src, indication = null, enabled = enabled, role = Role.Button, onClick = onClick)
            .alpha(if (enabled) 1f else 0.4f),
        contentAlignment = Alignment.Center,
    ) {
        CrundiIcon(icon, size = size, tint = if (pressed) CrundiColors.TextPrimary else tint, contentDescription = contentDescription)
    }
}
