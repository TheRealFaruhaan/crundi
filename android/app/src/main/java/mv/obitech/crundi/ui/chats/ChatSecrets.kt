package mv.obitech.crundi.ui.chats

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalInspectionMode
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.decodeFromJsonElement
import mv.obitech.crundi.api.CrundiJson
import mv.obitech.crundi.api.CrundiClient
import mv.obitech.crundi.api.model.ChatSecretGrant
import mv.obitech.crundi.api.model.Secret
import mv.obitech.crundi.api.str
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.DialogCard
import mv.obitech.crundi.ui.components.errorOrUnsupported
import mv.obitech.crundi.ui.operations.CheckRow
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/**
 * Secrets pre-authorised for ONE chat, as the web's (app/vendor/crundi-chat-secrets.js):
 * the person picks secrets and a duration and types each PIN once; the server
 * checks them and keeps them in its memory for this chat only, so this chat's
 * secret_run uses them without asking. Only to run: secret_get still asks,
 * and no value is ever shown. Offered only when the server lists
 * [CAPABILITY]; an older server has none of it.
 */
object ChatSecretPreauth {
    /** The handshake's name for it (src/client-version.js CAPABILITIES). */
    const val CAPABILITY = "chat-secret-preauth"
    fun supported(capabilities: Set<String>): Boolean = CAPABILITY in capabilities

    const val HINT = "A pre-authorised secret lets this chat run commands with it without asking for the PIN. Only to run: its value is never shown, and Claude reading a value still asks you every time. The PINs are kept in the server's memory only, for this chat only."
    const val ALLOW_HINT = "This chat may then run commands with this secret without asking, until then. Its value is never shown."

    /** Grants the clock has not passed. */
    fun active(grants: List<ChatSecretGrant>, now: Long): List<ChatSecretGrant> = grants.filter { g -> g.expiresAt == null || (parse(g.expiresAt) ?: Long.MAX_VALUE) > now }

    /** "until 14:05 (42 min left)", or "until this chat ends". */
    fun describe(g: ChatSecretGrant, now: Long, zone: java.time.ZoneId = java.time.ZoneId.systemDefault()): String {
        val at = g.expiresAt?.let(::parse) ?: return "until this chat ends"
        val mins = ((at - now).coerceAtLeast(0) + 30_000) / 60_000
        val left = when {
            mins < 1 -> "less than a minute"
            mins < 60 -> "$mins min"
            else -> "${mins / 60} h" + if (mins % 60 != 0L) " ${mins % 60} min" else ""
        }
        val t = java.time.Instant.ofEpochMilli(at).atZone(zone)
        return "until %02d:%02d (%s left)".format(t.hour, t.minute, left)
    }

    private fun parse(iso: String): Long? = runCatching { java.time.Instant.parse(iso).toEpochMilli() }.getOrNull()

    /** The dialog's form after ticking or unticking a secret (its PIN goes with it). */
    fun toggle(f: ChatSecretsForm, id: String): ChatSecretsForm =
        if (id in f.chosen) f.copy(chosen = f.chosen - id, pins = f.pins - id, error = null) else f.copy(chosen = f.chosen + id, error = null)

    fun pin(f: ChatSecretsForm, id: String, typed: String): ChatSecretsForm = f.copy(pins = f.pins + (id to typed.filter(Char::isDigit).take(6)), error = null, bad = if (f.bad == id) null else f.bad)

    /** What is wrong before sending, or null. */
    fun problem(f: ChatSecretsForm): Pair<String, String?>? {
        if (f.chosen.isEmpty()) return "Choose at least one secret." to null
        val missing = f.chosen.firstOrNull { !Regex("^\\d{6}$").matches(f.pins[it].orEmpty()) }
        return missing?.let { "Enter the 6-digit PIN of each secret chosen." to it }
    }

    /** A chat's list from the server's `chat-secrets` event. */
    fun fromEvent(obj: kotlinx.serialization.json.JsonObject, chatId: String): List<ChatSecretGrant>? {
        if (obj.str("sessionId") != chatId) return null
        return runCatching { CrundiJson.decodeFromJsonElement<List<ChatSecretGrant>>(obj["grants"] ?: JsonArray(emptyList())) }.getOrNull()
    }
}

/** The dialog's state. [secrets] null while loading. */
data class ChatSecretsForm(
    val grants: List<ChatSecretGrant> = emptyList(),
    val secrets: List<Secret>? = null,
    val chosen: List<String> = emptyList(),
    val pins: Map<String, String> = emptyMap(),
    val duration: String = "1h",
    val error: String? = null,
    /** The secret whose PIN was wrong. */
    val bad: String? = null,
    val busy: Boolean = false,
    /** A secret being revoked (or "*" for all). */
    val revoking: String? = null,
)

class ChatSecretsActions(
    val onToggle: (String) -> Unit = {},
    val onPin: (String, String) -> Unit = { _, _ -> },
    val onDuration: (String) -> Unit = {},
    val onGrant: () -> Unit = {},
    val onRevoke: (ChatSecretGrant) -> Unit = {},
    val onRevokeAll: () -> Unit = {},
    val onClose: () -> Unit = {},
)

/** The dialog's body: what is pre-authorised now, then pre-authorise more. Stateless, for tests. */
@Composable
fun ChatSecretsCard(
    f: ChatSecretsForm, now: Long, a: ChatSecretsActions, modifier: Modifier = Modifier,
    /** A PIN for this secret is saved behind the fingerprint (BiometricPins): it can fill the box. */
    savedPin: (String) -> Boolean = { false },
    onFingerprint: (String) -> Unit = {},
) {
    val grants = ChatSecretPreauth.active(f.grants, now)
    DialogCard(
        "Secrets for this chat", modifier, message = ChatSecretPreauth.HINT,
        confirm = if (f.busy) "Checking…" else "Pre-authorise", onConfirm = { if (!f.busy) a.onGrant() },
        dismiss = "Close", onDismiss = a.onClose,
    ) {
        // The list scrolls in whatever room is left, so the buttons below are
        // always on screen, even in a bubble's short window.
        Column(Modifier.weight(1f, fill = false).heightIn(max = 460.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Section("Pre-authorised now")
            if (grants.isEmpty()) Text("Nothing is pre-authorised for this chat.", style = CrundiType.Small.copy(color = CrundiColors.TextMuted))
            for (g in grants) Row(
                Modifier.fillMaxWidth().background(CrundiColors.BgPrimary, Dimens.ShapeSm).border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm).padding(horizontal = 10.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Column(Modifier.weight(1f)) {
                    Text(g.name.ifEmpty { "A secret" }, style = CrundiType.Code.copy(fontSize = 13.sp, color = CrundiColors.TextPrimary))
                    Text(ChatSecretPreauth.describe(g, now), style = CrundiType.Meta)
                }
                CrundiButton(if (f.revoking == g.secretId) "Revoking…" else "Revoke", { a.onRevoke(g) }, enabled = f.revoking == null)
            }
            if (grants.size > 1) Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                CrundiButton(if (f.revoking == "*") "Revoking…" else "Revoke all", a.onRevokeAll, kind = ButtonKind.Danger, enabled = f.revoking == null)
            }
            Section("Pre-authorise")
            val list = f.secrets
            when {
                list == null -> Text("Loading the secrets…", style = CrundiType.Small.copy(color = CrundiColors.TextMuted))
                list.isEmpty() -> Text("There are no secrets yet. Add them under Secrets.", style = CrundiType.Small.copy(color = CrundiColors.TextMuted))
                else -> for (s in list) Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    CheckRow(s.name, s.id in f.chosen, Modifier.weight(1f)) { a.onToggle(s.id) }
                    if (s.id in f.chosen) {
                        if (mv.obitech.crundi.PinButtons.fingerprint(savedPin(s.id), f.pins[s.id].orEmpty())) mv.obitech.crundi.ui.operations.FingerprintButton({ onFingerprint(s.id) }, enabled = !f.busy)
                        PinBox(f.pins[s.id].orEmpty(), { a.onPin(s.id, it) }, bad = f.bad == s.id, label = "PIN for ${s.name}")
                    }
                }
            }
            Text("For how long", style = CrundiType.Meta)
            mv.obitech.crundi.ui.planning.ChipRow(ChatSecretGrant.PRESETS, f.duration) { a.onDuration(it) }
            f.error?.let { Text(it, style = CrundiType.Small.copy(color = CrundiColors.Red)) }
        }
    }
}

@Composable
private fun Section(text: String) {
    Text(text.uppercase(), style = CrundiType.Meta.copy(fontWeight = FontWeight.SemiBold, color = CrundiColors.TextSecondary, letterSpacing = 0.5.sp), modifier = Modifier.padding(top = 4.dp))
}

@Composable
internal fun PinBox(value: String, onValue: (String) -> Unit, bad: Boolean = false, label: String = "PIN") {
    Column(Modifier.width(118.dp)) {
        CrundiTextField(
            value, { onValue(it.filter(Char::isDigit).take(6)) }, placeholder = "PIN", password = true, mono = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword),
        )
        if (bad) Text("Wrong PIN", style = CrundiType.Meta.copy(color = CrundiColors.Red))
    }
}

/** This chat's grants, kept current: loaded once, then each `chat-secrets` event. Empty while unsupported. */
@Composable
fun rememberChatGrants(api: CrundiClient, chatId: String, supported: Boolean): List<ChatSecretGrant> {
    var grants by remember(chatId) { mutableStateOf<List<ChatSecretGrant>>(emptyList()) }
    val inspecting = LocalInspectionMode.current
    LaunchedEffect(chatId, supported) {
        if (!supported || inspecting) return@LaunchedEffect
        runCatching { api.secrets.chatGrants(chatId) }.getOrNull()?.let { grants = it }
        chatSecretEvents(api, chatId).collect { grants = it }
    }
    return grants
}

private fun chatSecretEvents(api: CrundiClient, chatId: String): Flow<List<ChatSecretGrant>> =
    api.events.changes("chat-secrets").mapNotNull { ChatSecretPreauth.fromEvent(it.data, chatId) }

/** The dialog, live, over the chat. */
@Composable
fun ChatSecretsRoute(api: CrundiClient, chatId: String, grants: List<ChatSecretGrant>, onClose: () -> Unit) {
    var f by remember(chatId) { mutableStateOf(ChatSecretsForm(grants = grants)) }
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    val scope = rememberCoroutineScope()
    LaunchedEffect(grants) { f = f.copy(grants = grants) }
    LaunchedEffect(chatId) {
        f = try { f.copy(secrets = api.secrets.list().secrets) } catch (e: Exception) { f.copy(secrets = emptyList(), error = errorOrUnsupported(e)) }
    }
    // The times shown ("12 min left") move on while it is open.
    LaunchedEffect(Unit) { while (true) { delay(30_000); now = System.currentTimeMillis() } }
    fun close() { f = f.copy(pins = emptyMap()); onClose() }
    val keeper = mv.obitech.crundi.LocalPinKeeper.current
    val serverKey = remember(api) { mv.obitech.crundi.SavedPins.serverKey(api.transport.baseUrl) }
    ChatSecretsCard(f, now, savedPin = { keeper.saved(serverKey, it) }, onFingerprint = { id ->
        val name = f.secrets?.firstOrNull { it.id == id }?.name.orEmpty()
        keeper.unlock(serverKey, id, name, onPin = { pin -> f = ChatSecretPreauth.pin(f, id, pin) }, onFail = { why -> if (why != null) f = f.copy(error = why) })
    }, a = ChatSecretsActions(
        onToggle = { f = ChatSecretPreauth.toggle(f, it) },
        onPin = { id, v -> f = ChatSecretPreauth.pin(f, id, v) },
        onDuration = { f = f.copy(duration = it) },
        onGrant = grant@{
            ChatSecretPreauth.problem(f)?.let { (msg, bad) -> f = f.copy(error = msg, bad = bad); return@grant }
            val pins = f.chosen.associateWith { f.pins[it].orEmpty() }
            f = f.copy(busy = true, error = null, bad = null)
            scope.launch {
                f = try {
                    val g = api.secrets.grantChat(chatId, pins, f.duration)
                    f.copy(grants = g, chosen = emptyList(), pins = emptyMap(), busy = false)
                } catch (e: Exception) {
                    val bad = (e as? mv.obitech.crundi.api.ApiException)?.body?.str("secretId")
                    val who = f.secrets?.firstOrNull { it.id == bad }?.name
                    f.copy(busy = false, bad = bad, pins = if (bad != null) f.pins - bad else f.pins, error = (if (who != null) "$who: " else "") + errorOrUnsupported(e))
                }
            }
        },
        onRevoke = { g ->
            f = f.copy(revoking = g.secretId)
            scope.launch { f = try { f.copy(grants = api.secrets.revokeChat(chatId, g.secretId), revoking = null) } catch (e: Exception) { f.copy(revoking = null, error = errorOrUnsupported(e)) } }
        },
        onRevokeAll = {
            f = f.copy(revoking = "*")
            scope.launch { f = try { f.copy(grants = api.secrets.revokeAllChat(chatId), revoking = null) } catch (e: Exception) { f.copy(revoking = null, error = errorOrUnsupported(e)) } }
        },
        onClose = ::close,
    ))
}

/**
 * "Secrets for this chat" asked for from outside the chat: a workbench pane's
 * menu, where the chat's own header (with its lock and menu) is not drawn.
 * The chat with that id opens its secrets sheet.
 */
object ChatSecretsBus {
    private val flow = kotlinx.coroutines.flow.MutableSharedFlow<String>(extraBufferCapacity = 8)
    val opens: kotlinx.coroutines.flow.SharedFlow<String> get() = flow
    fun open(chatId: String) { flow.tryEmit(chatId) }
}
