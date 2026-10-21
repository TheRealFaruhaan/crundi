package mv.obitech.crundi.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/** A card or panel: --bg-secondary, 1px --border, radius 10. */
@Composable
fun CrundiCard(
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
    padding: androidx.compose.ui.unit.Dp = Dimens.S3,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(
        modifier
            .clip(Dimens.Shape)
            .background(CrundiColors.BgSecondary)
            .border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape)
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(padding),
        content = content,
    )
}

/** A full-width 1px line in --border-subtle (or --border for `strong`). */
@Composable
fun CrundiDivider(modifier: Modifier = Modifier, strong: Boolean = false) {
    Box(modifier.fillMaxWidth().height(Dimens.Hairline).background(if (strong) CrundiColors.Border else CrundiColors.BorderSubtle))
}

/** A section head, as the sidebar's "PROJECTS": small, muted, uppercase. */
@Composable
fun SectionHeader(text: String, modifier: Modifier = Modifier, trailing: @Composable RowScope.() -> Unit = {}) {
    Row(
        modifier.fillMaxWidth().padding(horizontal = Dimens.ScreenPad, vertical = Dimens.S3),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(text.uppercase(), style = CrundiType.Section, modifier = Modifier.weight(1f))
        trailing()
    }
}

/** The state dot beside a project or chat (the sidebar's .dot). */
enum class DotState(val color: Color, val halo: Color?) {
    Idle(CrundiColors.TextMuted, null),
    Running(CrundiColors.Green, null),
    Working(CrundiColors.Yellow, CrundiColors.YellowDim),
    Waiting(CrundiColors.Sky, CrundiColors.SkyDim),
    NeedsInput(CrundiColors.Accent, CrundiColors.AccentDim),
    Error(CrundiColors.Red, CrundiColors.RedDim),
}

@Composable
fun StateDot(state: DotState, modifier: Modifier = Modifier) {
    Box(modifier.size(14.dp), contentAlignment = Alignment.Center) {
        if (state.halo != null) Box(Modifier.size(14.dp).clip(CircleShape).background(state.halo))
        Box(Modifier.size(8.dp).clip(CircleShape).background(state.color))
    }
}

/**
 * A list row, drawn as the desktop's sidebar-item: radius 6, secondary text,
 * --bg-hover when pressed, and when `active` the accent-dim fill with the
 * gradient bar on its left edge.
 *
 * `leading` is usually a CrundiIcon or StateDot; `trailing` a meta text,
 * badge or icon button. Title and subtitle are one line each.
 */
@Composable
fun ListRow(
    title: String,
    modifier: Modifier = Modifier,
    subtitle: String? = null,
    active: Boolean = false,
    icon: String? = null,
    leading: (@Composable () -> Unit)? = null,
    trailing: (@Composable RowScope.() -> Unit)? = null,
    onClick: (() -> Unit)? = null,
) {
    val src = remember { MutableInteractionSource() }
    val pressed by src.collectIsPressedAsState()
    val fg = when {
        active -> CrundiColors.AccentHover
        else -> CrundiColors.TextPrimary
    }
    Row(
        modifier
            .fillMaxWidth()
            .clip(Dimens.ShapeSm)
            .background(
                when {
                    active -> CrundiColors.AccentDim
                    pressed -> CrundiColors.BgHover
                    else -> Color.Transparent
                },
            )
            .then(if (onClick != null) Modifier.clickable(interactionSource = src, indication = null, onClick = onClick) else Modifier)
            .drawBehind {
                if (active) {
                    val inset = 7.dp.toPx()
                    drawRoundRect(
                        brush = CrundiColors.AccentGrad,
                        topLeft = Offset(0f, inset),
                        size = androidx.compose.ui.geometry.Size(3.dp.toPx(), size.height - 2 * inset),
                        cornerRadius = androidx.compose.ui.geometry.CornerRadius(3.dp.toPx()),
                    )
                }
            }
            .padding(horizontal = Dimens.RowPadH, vertical = Dimens.RowPadV),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        if (leading != null) leading()
        else if (icon != null) CrundiIcon(icon, size = Dimens.Icon, tint = if (active) CrundiColors.AccentHover else CrundiColors.TextSecondary)
        Column(Modifier.weight(1f)) {
            Text(title, style = CrundiType.Body.copy(color = fg), maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (!subtitle.isNullOrEmpty()) Text(subtitle, style = CrundiType.Small, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        if (trailing != null) trailing()
    }
}

/** Kinds of status badge, as the top bar's: dim fill, coloured mono text. */
enum class BadgeKind(val bg: Color, val fg: Color) {
    Neutral(CrundiColors.BgTertiary, CrundiColors.TextSecondary),
    Accent(CrundiColors.AccentDim, CrundiColors.AccentHover),
    Good(CrundiColors.GreenDim, CrundiColors.Green),
    Warn(CrundiColors.YellowDim, CrundiColors.Yellow),
    Bad(CrundiColors.RedDim, CrundiColors.Red),
    Info(CrundiColors.SkyDim, CrundiColors.Sky),
}

/** A status badge ("connected", "running", "3 waiting"). */
@Composable
fun StatusBadge(text: String, kind: BadgeKind = BadgeKind.Neutral, modifier: Modifier = Modifier) {
    Text(
        text,
        style = CrundiType.Badge.copy(color = kind.fg),
        maxLines = 1,
        modifier = modifier.clip(Dimens.ShapePill).background(kind.bg).padding(horizontal = 10.dp, vertical = 3.dp),
    )
}

/**
 * A choice chip (filters, segmented choices). Selected: accent-dim with
 * accent-hover text; otherwise tertiary with a border.
 */
@Composable
fun CrundiChip(text: String, selected: Boolean, onClick: () -> Unit, modifier: Modifier = Modifier, icon: String? = null) {
    val shape = RoundedCornerShape(14.dp)
    Row(
        modifier
            .clip(shape)
            .background(if (selected) CrundiColors.AccentDim else CrundiColors.BgTertiary)
            .border(Dimens.Hairline, if (selected) CrundiColors.Accent.copy(alpha = 0.5f) else CrundiColors.Border, shape)
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        val c = if (selected) CrundiColors.AccentHover else CrundiColors.TextSecondary
        if (icon != null) CrundiIcon(icon, size = 14.dp, tint = c)
        Text(text, style = CrundiType.Small.copy(color = c))
    }
}

/** A vertical 1px line, for splitting things in a bar. */
@Composable
fun VerticalRule(modifier: Modifier = Modifier) {
    Box(modifier.width(Dimens.Hairline).fillMaxHeight().background(CrundiColors.Border))
}
