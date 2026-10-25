package mv.obitech.crundi.ui.operations

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.launch
import mv.obitech.crundi.api.model.SecretRequest
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.errorText
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/** One secret request's card state: nothing, asking for the PIN, or working. */
data class SecretPromptState(val armed: String? = null, val pin: String = "", val error: String? = null, val busy: String? = null)

/**
 * Claude asking to use a secret, wherever the person is (the web's
 * sreq-dock, renderSecretPrompt): a card per request at the bottom of the
 * screen. "Release <name>" (Claude reads the value) or "Run a command with
 * <name>" (the command, the variable it is bound to). Approve opens the PIN
 * in the card itself; Unlock sends it; Deny refuses. Stateless, for tests.
 */
@Composable
fun SecretPromptDock(
    requests: List<SecretRequest>,
    state: SecretPromptState,
    onApprove: (SecretRequest) -> Unit = {},
    onPin: (String) -> Unit = {},
    onUnlock: (SecretRequest) -> Unit = {},
    onCancel: () -> Unit = {},
    onDeny: (SecretRequest) -> Unit = {},
    modifier: Modifier = Modifier,
) {
    if (requests.isEmpty()) return
    Box(modifier.fillMaxSize(), contentAlignment = Alignment.BottomCenter) {
        Column(
            Modifier.navigationBarsPadding().padding(12.dp).widthIn(max = 460.dp).fillMaxWidth().heightIn(max = 520.dp).verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            for (r in requests) SecretRequestCard(r, state, onApprove, onPin, onUnlock, onCancel, onDeny)
        }
    }
}

@Composable
private fun SecretRequestCard(
    r: SecretRequest, s: SecretPromptState,
    onApprove: (SecretRequest) -> Unit, onPin: (String) -> Unit, onUnlock: (SecretRequest) -> Unit, onCancel: () -> Unit, onDeny: (SecretRequest) -> Unit,
) {
    val run = r.kind == "run"
    val busy = s.busy == r.id
    Column(
        Modifier.fillMaxWidth().shadow(16.dp, Dimens.Shape).background(CrundiColors.BgCard, Dimens.Shape).border(Dimens.Hairline, CrundiColors.Accent, Dimens.Shape).padding(horizontal = 16.dp, vertical = 14.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(7.dp)) {
            CrundiIcon("lock", size = 15.dp, tint = CrundiColors.AccentHover)
            Text(if (run) "Run a command with " else "Release ", style = CrundiType.Small.copy(color = CrundiColors.TextPrimary, fontWeight = FontWeight.SemiBold))
            Text(r.secretName, style = CrundiType.Code.copy(fontSize = 13.sp, color = CrundiColors.AccentHover), maxLines = 1)
        }
        if (r.project.isNotEmpty()) Text("project: ${r.project}", style = CrundiType.Small)
        if (r.reason.isNotEmpty()) Text(r.reason, style = CrundiType.Small)
        if (run) {
            // The command is the decision: room to read it, wrapped, never cut.
            Text(
                r.command, style = CrundiType.Code.copy(fontSize = 12.sp),
                modifier = Modifier.fillMaxWidth().heightIn(max = 140.dp).background(CrundiColors.BgPrimary, Dimens.ShapeSm)
                    .border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm).verticalScroll(rememberScrollState()).padding(8.dp),
            )
            Text("the value is bound to \$${r.envName} and never shown to Claude", style = CrundiType.Meta)
        } else Text("Claude will be able to read this value.", style = CrundiType.Small)
        if (s.armed == r.id) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                CrundiTextField(
                    s.pin, { onPin(it.filter(Char::isDigit).take(6)) }, Modifier.weight(1f), placeholder = "PIN", password = true, mono = true,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword),
                )
                CrundiButton(if (busy) "Unlocking…" else "Unlock", { onUnlock(r) }, kind = ButtonKind.Primary, enabled = !busy && s.pin.length == 6)
                CrundiButton("Cancel", onCancel, enabled = !busy)
            }
            s.error?.let { Text(it, style = CrundiType.Small.copy(color = CrundiColors.Red)) }
        } else Row(horizontalArrangement = Arrangement.spacedBy(6.dp), modifier = Modifier.horizontalScroll(rememberScrollState())) {
            CrundiButton("Approve", { onApprove(r) }, kind = ButtonKind.Primary, enabled = !busy)
            CrundiButton("Deny", { onDeny(r) }, enabled = !busy)
        }
    }
}

/** Which requests a screen shows. */
object SecretPrompts {
    /**
     * A chat by itself (a bubble, ChatActivity) shows the requests that chat
     * made, and ones no chat is named for from its project (or from none),
     * as the compact chat page does (crundi-chat-page.js loadAsks).
     */
    fun forChat(requests: List<SecretRequest>, chatId: String, project: String): List<SecretRequest> =
        requests.filter { r -> if (r.sessionId.isNotEmpty()) r.sessionId == chatId else (r.project.isEmpty() || project.isEmpty() || r.project == project) }

    /**
     * The live list, with what was fetched on opening: the live list is empty
     * until the event stream has connected, and a request already waiting
     * must show at once. The fetch is made again each time the live list
     * changes, so an answered request does not linger.
     */
    fun merge(live: List<SecretRequest>, fetched: List<SecretRequest>?): List<SecretRequest> =
        if (fetched == null) live else live + fetched.filter { f -> live.none { it.id == f.id } }
}

/**
 * The dock, live: the server's secret requests, answered with the PIN. The
 * shell shows it over every screen (all requests); a bubble's chat shows it
 * too, for that chat ([chatId]): without it a request from a chat open in a
 * bubble was never answerable there.
 */
@Composable
fun SecretPromptRoute(chatId: String? = null) {
    val api = LocalAppServices.current.api
    val live by api.events.secretRequests.collectAsStateWithLifecycle(initialValue = emptyList())
    var fetched by remember { mutableStateOf<List<SecretRequest>?>(null) }
    val inspecting = androidx.compose.ui.platform.LocalInspectionMode.current
    androidx.compose.runtime.LaunchedEffect(live) {
        if (inspecting) return@LaunchedEffect
        runCatching { api.secrets.list().requests }.getOrNull()?.let { fetched = it }
    }
    val server by api.events.state.collectAsStateWithLifecycle(initialValue = api.events.lastState)
    val all = SecretPrompts.merge(live, fetched)
    val requests = if (chatId == null) all else SecretPrompts.forChat(all, chatId, server?.terminals?.firstOrNull { it.id == chatId }?.project.orEmpty())
    var st by remember { mutableStateOf(SecretPromptState()) }
    val scope = rememberCoroutineScope()
    SecretPromptDock(
        requests, st,
        onApprove = { st = SecretPromptState(armed = it.id) },
        onPin = { st = st.copy(pin = it, error = null) },
        onCancel = { st = SecretPromptState() },
        onUnlock = { r ->
            val pin = st.pin
            st = st.copy(busy = r.id, error = null)
            scope.launch {
                st = try { api.secrets.approve(r.id, pin); fetched = fetched?.filter { it.id != r.id }; SecretPromptState() }
                catch (e: Exception) { st.copy(busy = null, pin = "", error = errorText(e)) }
            }
        },
        onDeny = { r ->
            st = st.copy(busy = r.id)
            scope.launch {
                st = try { api.secrets.deny(r.id); fetched = fetched?.filter { it.id != r.id }; SecretPromptState() } catch (e: Exception) { st.copy(busy = null, error = errorText(e)) }
            }
        },
    )
}
