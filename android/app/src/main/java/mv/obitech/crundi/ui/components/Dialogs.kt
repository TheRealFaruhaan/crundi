package mv.obitech.crundi.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/** The body of a dialog, without the window: used by CrundiDialog and by screenshot tests. */
@Composable
fun DialogCard(
    title: String,
    modifier: Modifier = Modifier,
    message: String? = null,
    confirm: String? = null,
    onConfirm: () -> Unit = {},
    dismiss: String? = "Cancel",
    onDismiss: () -> Unit = {},
    danger: Boolean = false,
    content: @Composable ColumnScope.() -> Unit = {},
) {
    Column(
        modifier
            .widthIn(max = 440.dp)
            .fillMaxWidth()
            .background(CrundiColors.BgSecondary, Dimens.Shape)
            .border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape)
            .padding(Dimens.S6),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(title, style = CrundiType.Title)
        if (message != null) Text(message, style = CrundiType.Small.copy(lineHeight = CrundiType.Body.lineHeight))
        content()
        if (confirm != null || dismiss != null) {
            Row(Modifier.fillMaxWidth().padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(12.dp, androidx.compose.ui.Alignment.End)) {
                if (dismiss != null) CrundiButton(dismiss, onDismiss)
                if (confirm != null) CrundiButton(confirm, onConfirm, kind = if (danger) ButtonKind.Danger else ButtonKind.Primary)
            }
        }
    }
}

/**
 * A dialog as the desktop's: dark scrim, --bg-secondary card, radius 10,
 * title, words, Cancel and the action on the right. Name the action by what
 * it does ("Delete note", not "OK").
 */
@Composable
fun CrundiDialog(
    title: String,
    onDismiss: () -> Unit,
    message: String? = null,
    confirm: String? = null,
    onConfirm: () -> Unit = {},
    dismiss: String? = "Cancel",
    danger: Boolean = false,
    content: @Composable ColumnScope.() -> Unit = {},
) {
    Dialog(onDismissRequest = onDismiss) {
        DialogCard(title, Modifier.padding(16.dp), message, confirm, onConfirm, dismiss, onDismiss, danger, content)
    }
}
