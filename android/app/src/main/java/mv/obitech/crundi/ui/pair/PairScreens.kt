package mv.obitech.crundi.ui.pair

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiCard
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.SectionHeader
import mv.obitech.crundi.ui.components.TopBar
import mv.obitech.crundi.ui.shell.CrundiMark
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType

/** The pairing screen's state. `againHost`: the server whose sign-in ended, when pairing again. */
data class PairUi(val againHost: String? = null, val link: String = "", val status: String = "", val busy: Boolean = false)

/**
 * Pairing this phone with a server: scan the picture the server shows, or
 * paste its link. Shown before there is any server, and to pair again.
 */
@Composable
fun PairScreen(ui: PairUi, onScan: () -> Unit, onLinkChange: (String) -> Unit, onConnect: () -> Unit) {
    Box(Modifier.fillMaxSize().background(CrundiColors.BgPrimary).verticalScroll(rememberScrollState()), contentAlignment = Alignment.Center) {
        Column(
            Modifier.widthIn(max = 420.dp).fillMaxWidth().padding(horizontal = 24.dp, vertical = 40.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            CrundiMark(size = 84.dp)
            Text(
                if (ui.againHost == null) "Connect to your Crundi server" else "Pair this phone again",
                style = CrundiType.Title, textAlign = TextAlign.Center, modifier = Modifier.padding(top = 20.dp),
            )
            Text(
                if (ui.againHost == null) "On the server, open Settings, then Connect a phone, and point this phone at the picture it shows."
                else "${ui.againHost} ended this phone's sign-in. Open Crundi on a computer, or in this phone's browser, go to Settings, then Connect a phone, and point this phone at the picture it shows.",
                style = CrundiType.Small.copy(lineHeight = CrundiType.Body.lineHeight), textAlign = TextAlign.Center, modifier = Modifier.padding(top = 10.dp),
            )
            CrundiButton("Scan the picture", onScan, kind = ButtonKind.Primary, icon = "camera", enabled = !ui.busy, modifier = Modifier.fillMaxWidth().padding(top = 26.dp))
            Text("Or paste a pairing link", style = CrundiType.Small, modifier = Modifier.padding(top = 28.dp, bottom = 10.dp))
            CrundiTextField(
                ui.link, onLinkChange, placeholder = "https://your-server/pair#...", icon = "link", mono = true, enabled = !ui.busy,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Go, autoCorrectEnabled = false),
                keyboardActions = KeyboardActions(onGo = { onConnect() }),
                modifier = Modifier.fillMaxWidth(),
            )
            CrundiButton("Connect", onConnect, enabled = !ui.busy && ui.link.isNotBlank(), modifier = Modifier.fillMaxWidth().padding(top = 10.dp))
            if (ui.status.isNotEmpty()) Text(
                ui.status, style = CrundiType.Small.copy(color = if (ui.busy) CrundiColors.TextSecondary else CrundiColors.Amber),
                textAlign = TextAlign.Center, modifier = Modifier.padding(top = 18.dp),
            )
        }
    }
}

/** One line of the app settings: what it is, how it stands (green good, amber needs you), and at most one thing to do. */
sealed interface SettingItem {
    data class Line(val title: String, val state: String, val good: Boolean? = null, val action: String? = null, val onAction: () -> Unit = {}) : SettingItem
    data class Button(val text: String, val primary: Boolean = false, val onClick: () -> Unit = {}) : SettingItem
    data class Log(val lines: List<String>) : SettingItem
}

data class SettingSection(val title: String, val items: List<SettingItem>)

/** The app's own settings on this phone: permissions, paired servers, notification check, about. */
@Composable
fun AppSettingsScreen(subtitle: String, sections: List<SettingSection>, onBack: () -> Unit) {
    Column(Modifier.fillMaxSize().background(CrundiColors.BgPrimary)) {
        TopBar("App settings", subtitle = subtitle, onBack = onBack)
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(bottom = 24.dp)) {
            for (s in sections) {
                SectionHeader(s.title)
                Column(Modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    for (item in s.items) SettingView(item)
                }
            }
        }
    }
}

@Composable
private fun SettingView(item: SettingItem) {
    when (item) {
        is SettingItem.Line -> CrundiCard(Modifier.fillMaxWidth(), padding = 14.dp) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(item.title, style = CrundiType.Body)
                    Text(
                        item.state,
                        style = CrundiType.Small.copy(color = when (item.good) { true -> CrundiColors.Green; false -> CrundiColors.Amber; null -> CrundiColors.TextSecondary }),
                        modifier = Modifier.padding(top = 3.dp),
                    )
                }
                if (item.action != null) CrundiButton(item.action, item.onAction, modifier = Modifier.padding(start = 10.dp))
            }
        }
        is SettingItem.Button -> CrundiButton(item.text, item.onClick, kind = if (item.primary) ButtonKind.Primary else ButtonKind.Secondary, modifier = Modifier.fillMaxWidth())
        is SettingItem.Log -> CrundiCard(Modifier.fillMaxWidth()) {
            SelectionContainer {
                Text(
                    if (item.lines.isEmpty()) "Nothing has happened yet. When a notification arrives, or fails to, it is noted here." else item.lines.joinToString("\n"),
                    style = CrundiType.Meta.copy(color = CrundiColors.TextSecondary, lineHeight = CrundiType.Code.lineHeight),
                )
            }
        }
    }
}
