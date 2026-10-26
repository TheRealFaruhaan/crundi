package mv.obitech.crundi.ui.planning

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.model.Secret
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.errorText
import mv.obitech.crundi.ui.operations.PinAsk
import mv.obitech.crundi.ui.operations.PinCard
import mv.obitech.crundi.ui.operations.SecretLocks

/**
 * A factory's pre-authorised secrets, as the web's (tools/factory-flow
 * Inspector.jsx SecretsPicker, app/vendor/crundi-factories.js askSecretPins):
 * chosen by name in the editor and kept as ids; their PINs are asked one at a
 * time when a run starts and sent with the start, where the server keeps them
 * in memory for that run. An older server does not list [CAPABILITY]: the
 * picker and the prompts are hidden and every use asks for the PIN, as before.
 */
internal object FactorySecrets {
    /** The handshake's name for it (src/client-version.js CAPABILITIES). */
    const val CAPABILITY = "factory-secrets"
    fun supported(capabilities: Set<String>): Boolean = CAPABILITY in capabilities

    /** The list with [id] chosen or not, in the order chosen, once each. */
    fun toggle(list: List<String>, id: String, on: Boolean): List<String> = if (on) (list + id).distinct() else list - id

    /** The factory's secrets that still exist, by name, in the factory's order: the prompts at the start. */
    fun toUnlock(ids: List<String>, known: List<Secret>): List<Secret> = ids.mapNotNull { id -> known.firstOrNull { it.id == id } }

    /** Listed ids whose secret is gone (deleted on this machine). */
    fun missing(ids: List<String>, known: List<Secret>): List<String> = ids.filter { id -> known.none { it.id == id } }

    const val HINT = "Their PINs are asked when you start a run and kept in memory for that run only, so it does not stop at every use. Values are never shown. The Manager and the Gatekeeper use them directly; anyone else asks them first. Any other secret asks you each time."
    const val PROMPT = "This factory may use this secret. Its PIN is kept in memory for this run only, so the team does not stop to ask; the value is never shown. Skip to be asked when it is first used."
    fun prompt(lock: String): String = if (SecretLocks.passphrase(lock)) PROMPT.replace("Its PIN", "Its passphrase") else PROMPT
}

/**
 * The start of a run: each of the factory's secrets in turn. [check] asks the
 * server whether a PIN is right (never the value); a wrong one keeps the
 * prompt open with the server's words. Skip leaves that secret locked.
 * [onDone] gets { id: PIN } for those unlocked, or null when cancelled.
 */
@Composable
internal fun SecretPinsDialog(
    secrets: List<Secret>, check: suspend (String, String) -> Unit,
    /** The server re-locks old locks ('secret-upgrade'): offer it after one is accepted. */
    api: mv.obitech.crundi.api.CrundiClient? = null, offerUpgrade: Boolean = false,
    onDone: (Map<String, String>?) -> Unit,
) {
    var i by remember { mutableStateOf(0) }
    var pin by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    var working by remember { mutableStateOf(false) }
    val pins = remember { LinkedHashMap<String, String>() }
    // An old lock just accepted: "set a passphrase" before the next one. Memory only.
    var upgrade by remember { mutableStateOf<mv.obitech.crundi.ui.operations.PendingUpgrade?>(null) }
    val scope = rememberCoroutineScope()
    if (i >= secrets.size) return
    val s = secrets[i]
    fun next() { pin = ""; error = null; if (i + 1 >= secrets.size) onDone(pins.toMap()) else i++ }
    upgrade?.let { up ->
        // Updated: the run is started with the new passphrase (the server has it now).
        if (api != null) mv.obitech.crundi.ui.operations.UpgradeLockDialog(api, up) { np -> pins[up.secretId] = np ?: up.pin; upgrade = null; next() }
        return
    }
    fun submit() {
        if (!SecretLocks.lockOk(s.lock, pin)) { error = SecretLocks.askText(s.lock); return }
        working = true
        scope.launch {
            try {
                check(s.id, pin); pins[s.id] = pin
                if (offerUpgrade && api != null && s.needsUpgrade) { upgrade = mv.obitech.crundi.ui.operations.PendingUpgrade(s.id, s.name, s.lock, pin); pin = "" } else next()
            } catch (e: ApiException) { error = errorText(e); pin = "" } finally { working = false }
        }
    }
    val ask = PinAsk(
        title = s.name.ifEmpty { "A secret" },
        subtitle = (if (secrets.size > 1) "Secret ${i + 1} of ${secrets.size}. " else "") + FactorySecrets.prompt(s.lock),
        submit = "Unlock",
        lock = s.lock,
        run = {},
    )
    androidx.compose.ui.window.Dialog(onDismissRequest = { pin = ""; onDone(null) }) {
        PinCard(ask, pin, { pin = it; error = null }, error, working, ::submit, { pin = ""; onDone(null) }, Modifier.padding(16.dp)) {
            CrundiButton("Skip: ask me when it is first used", { next() }, kind = ButtonKind.Ghost, enabled = !working, modifier = Modifier.fillMaxWidth())
        }
    }
}
