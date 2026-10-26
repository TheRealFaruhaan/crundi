package mv.obitech.crundi.ui.operations

import androidx.compose.foundation.background
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.clickable
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.height
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
import mv.obitech.crundi.api.model.ChatSecretGrant
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
data class SecretPromptState(
    val armed: String? = null, val pin: String = "", val error: String? = null, val busy: String? = null,
    /** "Allow for this chat" ticked on the open card ('chat-secret-preauth'), and for how long (a preset key). */
    val allow: Boolean = false, val allowFor: String = "1h",
    /** "Save PIN for fingerprint" ticked on the open card (BiometricPins): kept once the PIN is accepted. */
    val save: Boolean = false,
)

/**
 * Claude asking to use a secret, wherever the person is (the web's
 * sreq-dock, renderSecretPrompt): a card per request at the bottom of the
 * screen. "Release <name>" (Claude reads the value) or "Run a command with
 * <name>" (the command, the variable it is bound to). The PIN box is on the
 * card from the start: typing the PIN is the approval, Unlock (or the
 * fingerprint) sends it, Deny refuses. Stateless, for tests.
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
    /** Whether a card offers "Allow for this chat" (a run request from a chat, on a server that offers it). */
    canAllow: (SecretRequest) -> Boolean = { false },
    onAllow: (Boolean) -> Unit = {},
    onAllowFor: (String) -> Unit = {},
    /** A PIN for this request's secret is saved behind the fingerprint (BiometricPins). */
    savedPin: (SecretRequest) -> Boolean = { false },
    onFingerprint: (SecretRequest) -> Unit = {},
    /** Whether a card offers "Save PIN for fingerprint" (the option on, this PIN not saved yet). */
    canSave: (SecretRequest) -> Boolean = { false },
    onSave: (Boolean) -> Unit = {},
) {
    if (requests.isEmpty()) return
    Box(modifier.fillMaxSize(), contentAlignment = Alignment.BottomCenter) {
        Column(
            Modifier.navigationBarsPadding().padding(12.dp).widthIn(max = 460.dp).fillMaxWidth().heightIn(max = 520.dp).verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            for (r in requests) SecretRequestCard(r, state, onApprove, onPin, onUnlock, onCancel, onDeny, canAllow(r), onAllow, onAllowFor, savedPin(r), { onFingerprint(r) }, canSave(r), onSave)
        }
    }
}

@Composable
private fun SecretRequestCard(
    r: SecretRequest, s: SecretPromptState,
    onApprove: (SecretRequest) -> Unit, onPin: (String) -> Unit, onUnlock: (SecretRequest) -> Unit, onCancel: () -> Unit, onDeny: (SecretRequest) -> Unit,
    canAllow: Boolean = false, onAllow: (Boolean) -> Unit = {}, onAllowFor: (String) -> Unit = {},
    saved: Boolean = false, onFingerprint: () -> Unit = {},
    canSave: Boolean = false, onSave: (Boolean) -> Unit = {},
) {
    val run = r.kind == "run"
    val busy = s.busy == r.id
    // One card is being answered at a time: the one typed in last (or the
    // only one). Its PIN, ticks and error are the state's; the others start empty.
    val mine = s.armed == r.id
    val pin = if (mine) s.pin else ""
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
        // The PIN is the approval: no separate step. Box and buttons one
        // height, in one line: Approve (the fingerprint while nothing is
        // typed and a PIN is saved) and Reject.
        Row(
            Modifier.fillMaxWidth().height(PIN_ROW_HEIGHT),
            horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically,
        ) {
            CrundiTextField(
                pin, { if (!mine) onApprove(r); onPin(it.filter(Char::isDigit).take(6)) }, Modifier.weight(1f).fillMaxHeight(), placeholder = "PIN", password = true, mono = true,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword),
            )
            // A PIN saved behind the fingerprint, nothing typed: the fingerprint
            // in Approve's place. Typing brings Approve back (PinButtons).
            if (mv.obitech.crundi.PinButtons.fingerprint(saved, pin)) FingerprintButton({ if (!mine) onApprove(r); onFingerprint() }, enabled = !busy, modifier = Modifier.fillMaxHeight().widthIn(min = PIN_ROW_HEIGHT))
            else CrundiButton(if (busy) "Approving…" else "Approve", { onUnlock(r) }, Modifier.fillMaxHeight(), kind = ButtonKind.Primary, enabled = !busy && mine && pin.length == 6)
            CrundiButton("Reject", { onDeny(r) }, Modifier.fillMaxHeight(), enabled = !busy)
        }
        if (canAllow) {
            CheckRow("Allow for this chat", mine && s.allow) { if (!mine) onApprove(r); onAllow(it) }
            if (mine && s.allow) {
                mv.obitech.crundi.ui.planning.ChipRow(ChatSecretGrant.PRESETS, s.allowFor) { onAllowFor(it) }
                Text(mv.obitech.crundi.ui.chats.ChatSecretPreauth.ALLOW_HINT, style = CrundiType.Meta)
            }
        }
        if (canSave) CheckRow("Save PIN for fingerprint", mine && s.save) { if (!mine) onApprove(r); onSave(it) }
        if (mine) s.error?.let { Text(it, style = CrundiType.Small.copy(color = CrundiColors.Red)) }
    }
}

/** One height for the PIN box and its buttons, so they line up. */
val PIN_ROW_HEIGHT = 48.dp

/** The fingerprint button that stands where a PIN window's submit button is (BiometricPins). */
@Composable
fun FingerprintButton(onClick: () -> Unit, enabled: Boolean = true, modifier: Modifier = Modifier.size(PIN_ROW_HEIGHT)) {
    androidx.compose.foundation.layout.Box(
        modifier.clip(Dimens.ShapeSm).background(if (enabled) CrundiColors.Accent else CrundiColors.BgTertiary, Dimens.ShapeSm)
            .clickable(enabled = enabled, onClickLabel = "Use your fingerprint", onClick = onClick)
            .semantics { contentDescription = "Use your fingerprint" }.testTag("pin-fingerprint"),
        contentAlignment = Alignment.Center,
    ) { CrundiIcon("fingerprint", size = 24.dp, tint = androidx.compose.ui.graphics.Color.White) }
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
    val shell by LocalAppServices.current.shell.state.collectAsStateWithLifecycle()
    val preauth = mv.obitech.crundi.ui.chats.ChatSecretPreauth.supported(shell.capabilities) && server?.role != "collaborator"
    // PINs saved behind the fingerprint, for this server (BiometricPins).
    val keeper = mv.obitech.crundi.LocalPinKeeper.current
    val serverKey = remember(api) { mv.obitech.crundi.SavedPins.serverKey(api.transport.baseUrl) }
    fun unlockWith(r: SecretRequest, pin: String, typed: Boolean) {
        val allow = if (preauth && st.allow && r.kind == "run" && r.sessionId.isNotEmpty()) st.allowFor else null
        st = st.copy(pin = pin, busy = r.id, error = null)
        scope.launch {
            st = try {
                api.secrets.approve(r.id, pin, allowForChat = allow)
                fetched = fetched?.filter { it.id != r.id }
                // "Save PIN for fingerprint" was ticked: keep it (the fingerprint confirms).
                if (st.save && mv.obitech.crundi.PinButtons.offerSave(keeper.on, keeper.saved(serverKey, r.secretId), typed)) {
                    keeper.save(serverKey, r.secretId, r.secretName, pin) { _, why -> if (why != null) st = st.copy(error = why) }
                }
                SecretPromptState(allowFor = st.allowFor)
            } catch (e: Exception) {
                // A saved PIN the server refuses (the secret was made again): forget it.
                if (!typed && e is mv.obitech.crundi.api.ApiException && e.status in 200..499) keeper.forget(serverKey, r.secretId)
                st.copy(busy = null, pin = "", error = errorText(e))
            }
        }
    }
    SecretPromptDock(
        requests, st,
        canAllow = { r -> preauth && r.kind == "run" && r.sessionId.isNotEmpty() },
        onAllow = { st = st.copy(allow = it) },
        onAllowFor = { st = st.copy(allowFor = it) },
        savedPin = { r -> keeper.saved(serverKey, r.secretId) },
        onFingerprint = { r ->
            keeper.unlock(serverKey, r.secretId, r.secretName, onPin = { pin -> unlockWith(r, pin, typed = false) }, onFail = { why -> if (why != null) st = st.copy(error = why) })
        },
        canSave = { r -> keeper.on && !keeper.saved(serverKey, r.secretId) },
        onSave = { st = st.copy(save = it) },
        // Starting on another card: that card's answer begins empty.
        onApprove = { if (st.armed != it.id) st = SecretPromptState(armed = it.id, allowFor = st.allowFor) },
        onPin = { st = st.copy(pin = it, error = null) },
        onCancel = { st = SecretPromptState() },
        onUnlock = { r -> unlockWith(r, st.pin, typed = true) },
        onDeny = { r ->
            st = st.copy(busy = r.id)
            scope.launch {
                st = try { api.secrets.deny(r.id); fetched = fetched?.filter { it.id != r.id }; SecretPromptState() } catch (e: Exception) { st.copy(busy = null, error = errorText(e)) }
            }
        },
    )
}
