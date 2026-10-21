package mv.obitech.crundi.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/**
 * The bar at the top of a screen that is not an area's front page (a chat,
 * a file, a run): back arrow, title (and a meta line), actions on the right.
 * Same height, colour and bottom border as the desktop's top bar.
 */
@Composable
fun TopBar(
    title: String,
    modifier: Modifier = Modifier,
    subtitle: String? = null,
    onBack: (() -> Unit)? = null,
    actions: @Composable RowScope.() -> Unit = {},
) {
    Column(modifier.fillMaxWidth().background(CrundiColors.BgSecondary)) {
        Row(
            Modifier.fillMaxWidth().height(Dimens.TopBarHeight).padding(horizontal = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (onBack != null) CrundiIconButton("chevron-left", onBack, contentDescription = "Back", tint = CrundiColors.TextPrimary, size = 20.dp)
            else Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f).padding(horizontal = 4.dp)) {
                Text(title, style = CrundiType.Heading, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (!subtitle.isNullOrEmpty()) Text(subtitle, style = CrundiType.Meta, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(0.dp), content = actions)
        }
        CrundiDivider(strong = true)
    }
}

/**
 * The strip under an area's front page header for its own tools: a title on
 * the left, buttons on the right, like the desktop's panel headers.
 */
@Composable
fun PanelHeader(title: String, modifier: Modifier = Modifier, meta: String? = null, actions: @Composable RowScope.() -> Unit = {}) {
    Column(modifier.fillMaxWidth()) {
        Row(
            Modifier.fillMaxWidth().padding(start = Dimens.ScreenPad, end = 8.dp, top = 6.dp, bottom = 6.dp).height(40.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text(title, style = CrundiType.Heading, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (!meta.isNullOrEmpty()) Text(meta, style = CrundiType.Meta, maxLines = 1)
            }
            Row(verticalAlignment = Alignment.CenterVertically, content = actions)
        }
        CrundiDivider()
    }
}
