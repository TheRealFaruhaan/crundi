package mv.obitech.crundi.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/**
 * Nothing to show yet: what this place is for and the one thing to do about it.
 * Say what to do, not how it feels ("No services yet. Add one to run it here.").
 */
@Composable
fun EmptyState(
    icon: String,
    title: String,
    modifier: Modifier = Modifier,
    body: String? = null,
    action: String? = null,
    onAction: (() -> Unit)? = null,
) {
    Box(modifier.fillMaxSize().padding(Dimens.S6), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.widthIn(max = 320.dp)) {
            Box(
                Modifier.size(56.dp).clip(CircleShape).background(CrundiColors.BgTertiary).border(Dimens.Hairline, CrundiColors.Border, CircleShape),
                contentAlignment = Alignment.Center,
            ) { CrundiIcon(icon, size = 24.dp, tint = CrundiColors.TextSecondary) }
            Text(title, style = CrundiType.Heading, textAlign = TextAlign.Center)
            if (body != null) Text(body, style = CrundiType.Small, textAlign = TextAlign.Center)
            if (action != null && onAction != null) CrundiButton(action, onAction, kind = ButtonKind.Primary, modifier = Modifier.padding(top = 6.dp))
        }
    }
}

/** Waiting for the server. Optional words under the spinner. */
@Composable
fun LoadingState(modifier: Modifier = Modifier, text: String? = null) {
    Box(modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
            CircularProgressIndicator(color = CrundiColors.Accent, trackColor = CrundiColors.BgTertiary, strokeWidth = 2.dp, modifier = Modifier.size(28.dp))
            if (text != null) Text(text, style = CrundiType.Meta)
        }
    }
}

/** Something failed: say what, and offer to try again. */
@Composable
fun ErrorState(message: String, modifier: Modifier = Modifier, onRetry: (() -> Unit)? = null) {
    EmptyState(icon = "alert-triangle", title = "Could not load this", body = message, action = if (onRetry != null) "Try again" else null, onAction = onRetry, modifier = modifier)
}

/** A one-line notice inside a screen (info, warning, error), as the desktop's banners. */
@Composable
fun Banner(text: String, modifier: Modifier = Modifier, kind: BadgeKind = BadgeKind.Info, icon: String = "info") {
    androidx.compose.foundation.layout.Row(
        modifier.clip(Dimens.ShapeSm).background(kind.bg).padding(horizontal = 12.dp, vertical = 9.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        CrundiIcon(icon, size = Dimens.IconSm, tint = kind.fg)
        Text(text, style = CrundiType.Small.copy(color = CrundiColors.TextPrimary))
    }
}
