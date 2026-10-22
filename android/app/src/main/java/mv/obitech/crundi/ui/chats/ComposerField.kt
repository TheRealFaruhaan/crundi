package mv.obitech.crundi.ui.chats

import android.net.Uri
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.content.MediaType
import androidx.compose.foundation.content.consume
import androidx.compose.foundation.content.contentReceiver
import androidx.compose.foundation.content.hasMediaType
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.input.TextFieldLineLimits
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.foundation.text.input.setTextAndPlaceCursorAtEnd
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.unit.dp
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/**
 * The chat's message box. It looks like CrundiTextField but is built on the
 * state-based text field, which can say it takes images: the keyboard then
 * offers pictures from its clipboard and image search, and a pasted picture
 * comes here ([onImages]) to be uploaded like an attachment. The older field
 * could not, so the keyboard never offered to paste one.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun ComposerField(
    draft: String,
    onDraft: (String) -> Unit,
    placeholder: String,
    enabled: Boolean,
    keyboardOptions: KeyboardOptions,
    onSend: () -> Unit,
    onImages: (List<Uri>) -> Unit,
    modifier: Modifier = Modifier,
) {
    val state = rememberTextFieldState(draft)
    val latestDraft by rememberUpdatedState(draft)
    val latestOnDraft by rememberUpdatedState(onDraft)
    // The draft is the screen's; keep the two in step both ways.
    LaunchedEffect(draft) { if (state.text.toString() != draft) state.setTextAndPlaceCursorAtEnd(draft) }
    LaunchedEffect(state) {
        snapshotFlow { state.text.toString() }.collect { t -> if (t != latestDraft) latestOnDraft(t) }
    }
    var focused by remember { mutableStateOf(false) }
    val shape = androidx.compose.foundation.shape.RoundedCornerShape(8.dp)
    val style = CrundiType.Body.copy(color = CrundiColors.TextPrimary)
    val images by rememberUpdatedState(onImages)
    BasicTextField(
        state = state,
        enabled = enabled,
        textStyle = style,
        cursorBrush = SolidColor(CrundiColors.AccentHover),
        lineLimits = TextFieldLineLimits.MultiLine(1, 6),
        keyboardOptions = keyboardOptions,
        onKeyboardAction = { onSend() },
        modifier = modifier
            .onFocusChanged { focused = it.isFocused }
            .contentReceiver { content ->
                if (!content.hasMediaType(MediaType.Image)) return@contentReceiver content
                val got = ArrayList<Uri>()
                val rest = content.consume { item -> item.uri?.let { got += it; true } ?: false }
                if (got.isNotEmpty()) images(got)
                rest
            },
        decorator = { inner ->
            Box(
                Modifier.fillMaxWidth().background(CrundiColors.BgPrimary, shape)
                    .border(Dimens.Hairline, if (focused) CrundiColors.Accent else CrundiColors.Border, shape)
                    .padding(horizontal = 11.dp, vertical = 10.dp),
            ) {
                if (state.text.isEmpty()) Text(placeholder, style = style.copy(color = CrundiColors.TextMuted), maxLines = 1)
                inner()
            }
        },
    )
}
