package mv.obitech.crundi.ui.operations

import mv.obitech.crundi.ui.components.errorText

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.model.Secret
import mv.obitech.crundi.api.model.SecretRequest
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.DialogCard
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.PanelHeader
import mv.obitech.crundi.ui.components.SectionHeader
import mv.obitech.crundi.ui.components.StatusBadge
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/** How long a revealed value stays on screen before it is wiped. */
const val REVEAL_MS = 30_000L

data class SecretsState(
    val secrets: Load<List<Secret>> = Load.Loading,
    val requests: List<SecretRequest> = emptyList(),
    /** Values shown right now, by secret id. Memory only; wiped on a timer and when the screen goes. */
    val revealed: Map<String, String> = emptyMap(),
    val busy: String? = null,
    val note: Actions.Note? = null,
)

/** A PIN to ask for: what it unlocks and the words of the dialog. */
data class PinAsk(val title: String, val subtitle: String, val submit: String, val run: suspend (String) -> Unit)

class SecretsEvents(
    /** Ask for the PIN, then run with it; a wrong PIN keeps the dialog open with the server's words. */
    val onReveal: (Secret) -> Unit = {},
    val onHide: (Secret) -> Unit = {},
    val onCopy: (Secret) -> Unit = {},
    val onDelete: (Secret) -> Unit = {},
    val onApprove: (SecretRequest) -> Unit = {},
    val onDeny: (SecretRequest) -> Unit = {},
    val onAdd: (name: String, description: String, value: String, pin: String) -> Unit = { _, _, _, _ -> },
    val onCloseNote: () -> Unit = {},
    val onRetry: () -> Unit = {},
)

@Composable
fun SecretsRoute() {
    val api = LocalAppServices.current.api
    val actions = rememberActions()
    val clip = LocalClipboardManager.current
    val list = rememberLoad { api.secrets.list() }
    val liveRequests by api.events.secretRequests.collectAsStateWithLifecycle(initialValue = null)
    val revealed = remember { mutableStateMapOf<String, String>() }
    var pin by remember { mutableStateOf<PinAsk?>(null) }
    // Nothing revealed outlives the screen.
    DisposableEffect(Unit) { onDispose { revealed.clear() } }
    LaunchedEffect(revealed.keys.toSet()) { if (revealed.isNotEmpty()) { delay(REVEAL_MS); revealed.clear() } }

    SecretsScreen(
        SecretsState(
            secrets = when (val l = list.value) { is Load.Ready -> Load.Ready(l.value.secrets); is Load.Failed -> l; Load.Loading -> Load.Loading },
            requests = liveRequests ?: list.value.valueOrNull?.requests.orEmpty(),
            revealed = revealed.toMap(),
            busy = actions.busy,
            note = actions.note,
        ),
        SecretsEvents(
            onReveal = { s -> pin = PinAsk("Reveal secret", "Enter the PIN for \"${s.name}\".", "Reveal") { p -> revealed[s.id] = api.secrets.reveal(s.id, p) } },
            onHide = { s -> revealed.remove(s.id) },
            onCopy = { s -> revealed[s.id]?.let { clip.setText(AnnotatedString(it)); actions.note = Actions.Note("Copied", false) } },
            onDelete = { s -> actions.run(s.id, "Deleted", list::reload) { api.secrets.delete(s.id) } },
            onApprove = { r ->
                pin = PinAsk("Approve access", "Enter the PIN for \"${r.secretName}\" to release it to Claude.", "Approve") { p ->
                    api.secrets.approve(r.id, p); actions.note = Actions.Note("Approved", false); list.reload()
                }
            },
            onDeny = { r -> actions.run(r.id, "Denied", list::reload) { api.secrets.deny(r.id) } },
            onAdd = { n, d, v, p -> actions.run("add", "Secret saved", list::reload) { api.secrets.add(n, v, p, d) } },
            onCloseNote = actions::clear,
            onRetry = list::reload,
        ),
    )
    pin?.let { ask -> PinDialog(ask, onDone = { pin = null }) }
}

@Composable
fun SecretsScreen(state: SecretsState, events: SecretsEvents = SecretsEvents(), adding: Boolean = false) {
    var add by remember { mutableStateOf(adding) }
    Column(Modifier.fillMaxSize()) {
        PanelHeader("Secrets", meta = state.secrets.valueOrNull?.let { "${it.size} stored · values need the PIN" }) {
            CrundiButton("Add secret", { add = true }, kind = ButtonKind.Primary, icon = "plus")
        }
        NoteBanner(state.note, events.onCloseNote)
        when (val l = state.secrets) {
            Load.Loading -> LoadingState(text = "Loading secrets")
            is Load.Failed -> ErrorState(l.message, onRetry = events.onRetry)
            is Load.Ready -> LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = Dimens.S6)) {
                if (state.requests.isNotEmpty()) {
                    item { SectionHeader("Pending access requests") { StatusBadge("${state.requests.size} waiting", BadgeKind.Info) } }
                    items(state.requests, key = { "r" + it.id }) { r -> RequestCard(r, state.busy == r.id, events) }
                }
                item { SectionHeader("Secrets (${l.value.size})") }
                if (l.value.isEmpty()) item {
                    EmptyState("lock", "No secrets yet.", body = "Add one, and Claude can use it with your PIN without ever showing it in a chat.", action = "Add secret", onAction = { add = true },
                        modifier = Modifier.padding(top = Dimens.S6))
                }
                items(l.value, key = { it.id }) { s -> SecretRow(s, state.revealed[s.id], state.busy == s.id, events) }
            }
        }
    }
    if (add) CrundiDialog("Add a secret", onDismiss = { add = false }, dismiss = null) {
        AddSecretForm(onCancel = { add = false }, onSave = { n, d, v, p -> add = false; events.onAdd(n, d, v, p) })
    }
}

@Composable
private fun RequestCard(r: SecretRequest, busy: Boolean, events: SecretsEvents) {
    Column(
        Modifier.padding(horizontal = Dimens.ScreenPad, vertical = 5.dp).fillMaxWidth().clip(Dimens.Shape).background(CrundiColors.SkyDim.copy(alpha = 0.10f))
            .border(Dimens.Hairline, CrundiColors.Sky.copy(alpha = 0.45f), Dimens.Shape).padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiIcon("lock", size = Dimens.IconSm, tint = CrundiColors.Sky)
            Text(if (r.kind == "run") "Run a command with ${r.secretName}" else "Release ${r.secretName}", style = CrundiType.Heading, modifier = Modifier.weight(1f))
        }
        if (r.project.isNotEmpty()) Text("project: ${r.project}", style = CrundiType.Meta)
        Text(r.reason.ifEmpty { "Claude requested access" }, style = CrundiType.Body)
        if (r.kind == "run") {
            if (r.command.isNotEmpty()) Text(r.command, style = CrundiType.Code.copy(fontSize = 12.sp), modifier = Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgPrimary).padding(8.dp))
            Text("The value is bound to \$${r.envName.ifEmpty { "SECRET" }} and never shown to Claude.", style = CrundiType.Small)
        } else Text("Claude will be able to read this value.", style = CrundiType.Small)
        Row(Modifier.padding(top = 4.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiButton("Approve", { events.onApprove(r) }, kind = ButtonKind.Primary, icon = "check", enabled = !busy)
            CrundiButton("Deny", { events.onDeny(r) }, icon = "x", enabled = !busy)
        }
    }
}

@Composable
private fun SecretRow(s: Secret, value: String?, busy: Boolean, events: SecretsEvents) {
    var confirm by remember { mutableStateOf(false) }
    Column(
        Modifier.padding(horizontal = Dimens.ScreenPad, vertical = 5.dp).fillMaxWidth().clip(Dimens.Shape).background(CrundiColors.BgCard)
            .border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape).padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            CrundiIcon("key", size = Dimens.IconSm, tint = CrundiColors.TextSecondary)
            Text(s.name, style = CrundiType.Code.copy(fontWeight = FontWeight.SemiBold), modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
            val at = Fmt.iso(s.updatedAt.ifEmpty { s.createdAt })
            if (at > 0) Text(Fmt.ago(at), style = CrundiType.Meta)
        }
        if (s.description.isNotEmpty()) Text(s.description, style = CrundiType.Small)
        if (value != null) Text(
            value, style = CrundiType.Code.copy(color = CrundiColors.Green, fontSize = 12.sp),
            modifier = Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgPrimary).padding(8.dp),
        )
        @OptIn(ExperimentalLayoutApi::class)
        FlowRow(Modifier.padding(top = 2.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            if (value == null) CrundiButton("Reveal", { events.onReveal(s) }, icon = "eye", enabled = !busy)
            else {
                CrundiButton("Copy", { events.onCopy(s) }, icon = "copy")
                CrundiButton("Hide", { events.onHide(s) }, icon = "eye-off")
            }
            CrundiButton("Delete", { confirm = true }, kind = ButtonKind.Danger, icon = "trash", enabled = !busy)
        }
    }
    if (confirm) CrundiDialog(
        "Delete secret \"${s.name}\"?", onDismiss = { confirm = false }, message = "This cannot be undone.",
        confirm = "Delete secret", danger = true, onConfirm = { confirm = false; events.onDelete(s) },
    )
}

@Composable
fun AddSecretForm(onCancel: () -> Unit, onSave: (String, String, String, String) -> Unit) {
    var name by remember { mutableStateOf("") }
    var description by remember { mutableStateOf("") }
    var value by remember { mutableStateOf("") }
    var pin by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        CrundiTextField(name, { name = it }, placeholder = "Name (e.g. AWS Access Key)")
        CrundiTextField(description, { description = it }, placeholder = "Description: Claude searches names and descriptions", singleLine = false, minLines = 2)
        CrundiTextField(value, { value = it }, placeholder = "Secret value", mono = true, password = true)
        CrundiTextField(pin, { t -> pin = t.filter(Char::isDigit).take(6) }, placeholder = "6-digit PIN to encrypt this secret", password = true, mono = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword))
        Text("The PIN is never stored. If you lose it, this secret can only be deleted and recreated.", style = CrundiType.Small)
        error?.let { Text(it, style = CrundiType.Small.copy(color = CrundiColors.Red)) }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp, Alignment.End)) {
            CrundiButton("Cancel", onCancel)
            CrundiButton("Save secret", {
                error = when {
                    name.isBlank() -> "Name is required"
                    value.isEmpty() -> "Value is required"
                    !Regex("^\\d{6}$").matches(pin) -> "PIN must be exactly 6 digits"
                    else -> null
                }
                if (error == null) onSave(name.trim(), description.trim(), value, pin)
            }, kind = ButtonKind.Primary, icon = "lock")
        }
    }
}

/** The PIN dialog body: six digits, the action, and the server's words when it says no. */
@Composable
fun PinCard(ask: PinAsk, pin: String, onPin: (String) -> Unit, error: String?, working: Boolean, onSubmit: () -> Unit, onCancel: () -> Unit, modifier: Modifier = Modifier) {
    DialogCard(
        ask.title, modifier, message = ask.subtitle,
        confirm = if (working) "Unlocking…" else ask.submit, onConfirm = { if (!working) onSubmit() }, onDismiss = onCancel,
    ) {
        CrundiTextField(
            pin, { t -> onPin(t.filter(Char::isDigit).take(6)) }, placeholder = "6-digit PIN", password = true, mono = true, icon = "lock",
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword, imeAction = ImeAction.Done),
            keyboardActions = KeyboardActions(onDone = { onSubmit() }),
        )
        // Six dots that fill as digits are typed: the length at a glance.
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp, Alignment.CenterHorizontally)) {
            repeat(6) { i ->
                androidx.compose.foundation.layout.Box(
                    Modifier.padding(top = 2.dp).clip(androidx.compose.foundation.shape.CircleShape)
                        .background(if (i < pin.length) CrundiColors.AccentHover else CrundiColors.BgTertiary)
                        .border(Dimens.Hairline, CrundiColors.Border, androidx.compose.foundation.shape.CircleShape)
                        .padding(5.dp),
                )
            }
        }
        if (error != null) Text(error, style = CrundiType.Small.copy(color = CrundiColors.Red))
    }
}

@Composable
private fun PinDialog(ask: PinAsk, onDone: () -> Unit) {
    var pin by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    var working by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    fun submit() {
        if (!Regex("^\\d{6}$").matches(pin)) { error = "Enter a 6-digit PIN"; return }
        working = true
        scope.launch {
            try { ask.run(pin); pin = ""; onDone() } catch (e: ApiException) { error = errorText(e); pin = "" } finally { working = false }
        }
    }
    androidx.compose.ui.window.Dialog(onDismissRequest = { pin = ""; onDone() }) {
        PinCard(ask, pin, { pin = it; error = null }, error, working, ::submit, { pin = ""; onDone() }, Modifier.padding(16.dp))
    }
}
