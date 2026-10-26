package mv.obitech.crundi.ui.operations

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
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
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import mv.obitech.crundi.LocalPinKeeper
import mv.obitech.crundi.PinKeeper
import mv.obitech.crundi.SavedPins
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.CrundiClient
import mv.obitech.crundi.api.model.Secret
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.DialogCard
import mv.obitech.crundi.ui.components.ErrorWords
import mv.obitech.crundi.ui.components.errorOrUnsupported
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType

/**
 * How a secret's lock is typed, re-locked and edited, as the web's
 * (app/vendor/crundi-secret-lock.js). A secret is locked with a passphrase
 * (any text, 8+ characters and not digits only, for every new or re-locked
 * secret) or, made before passphrases, a 6-digit PIN that still opens it.
 *
 * Capabilities (src/client-version.js): [CAP_PASSPHRASE] (the lock kind in
 * lists and requests; new secrets take a passphrase), [CAP_EDIT] (action
 * 'edit') and [CAP_UPGRADE] (needsUpgrade: after an old lock is accepted,
 * "set a passphrase" right there). An older server has 6-digit PINs only.
 * Pure parts here, for tests.
 */
object SecretLocks {
    const val CAP_PASSPHRASE = "secret-passphrase"
    const val CAP_EDIT = "secret-edit"
    const val CAP_UPGRADE = "secret-upgrade"

    const val MIN = 8
    const val MAX = 1024
    /** The server's rule for a new lock (src/secrets-store.js PASSPHRASE_RULE). */
    const val RULE = "At least 8 characters, and not only digits"
    const val UPGRADE_PIN = "This secret uses an old 6-digit PIN. Set a new passphrase (8+ characters)."
    const val UPGRADE_KDF = "This secret uses an older, weaker lock. Set a passphrase (8+ characters) to seal it again; it may be the same one."

    fun passphrase(lock: String): Boolean = lock == "passphrase"

    /** May this be a NEW lock? */
    fun passphraseOk(t: String): Boolean = t.codePointCount(0, t.length) >= MIN && t.length <= MAX && t.any { !it.isDigit() }

    /** What is wrong with a new lock and its confirmation, or null (nothing typed yet is not wrong). */
    fun newLockProblem(a: String, b: String): String? = when {
        a.isEmpty() -> null
        !passphraseOk(a) -> "$RULE."
        b.isNotEmpty() && a != b -> "The two do not match."
        else -> null
    }
    fun newLockReady(a: String, b: String): Boolean = passphraseOk(a) && a == b

    /** Enough to send for this lock. */
    fun lockOk(lock: String, t: String): Boolean = if (passphrase(lock)) t.isNotEmpty() && t.length <= MAX else Regex("^\\d{6}$").matches(t)

    /** What the field keeps of what was typed: six digits for a PIN, the text for a passphrase. */
    fun filter(lock: String, typed: String): String = if (passphrase(lock)) typed.take(MAX) else typed.filter(Char::isDigit).take(6)

    fun placeholder(lock: String): String = if (passphrase(lock)) "Passphrase" else "PIN"
    fun noun(lock: String): String = if (passphrase(lock)) "passphrase" else "PIN"
    fun askText(lock: String): String = if (passphrase(lock)) "Enter the passphrase" else "Enter a 6-digit PIN"
    fun wrongText(lock: String): String = if (passphrase(lock)) "Wrong passphrase" else "Wrong PIN"
    fun upgradeText(lock: String): String = if (passphrase(lock)) UPGRADE_KDF else UPGRADE_PIN

    /** An older server answers an unknown action with these words: it has no edit. */
    fun unsupported(e: Throwable): String =
        if (e is ApiException && e.message.contains("Unknown secrets action")) ErrorWords.NOT_SUPPORTED else errorOrUnsupported(e)

    /** Offer the upgrade after a use: the server can, and this secret is on an old lock. */
    fun offerUpgrade(capabilities: Set<String>, needsUpgrade: Boolean): Boolean = CAP_UPGRADE in capabilities && needsUpgrade
}

/** A field for a secret's current lock: the 6-digit PIN field as always, or a passphrase field. */
@Composable
fun LockField(
    value: String, onValue: (String) -> Unit, lock: String, modifier: Modifier = Modifier,
    placeholder: String = SecretLocks.placeholder(lock), icon: String? = null, onDone: (() -> Unit)? = null,
) {
    val pass = SecretLocks.passphrase(lock)
    CrundiTextField(
        value, { onValue(SecretLocks.filter(lock, it)) }, modifier, placeholder = placeholder, password = true, mono = !pass, icon = icon,
        keyboardOptions = KeyboardOptions(keyboardType = if (pass) KeyboardType.Password else KeyboardType.NumberPassword, imeAction = if (onDone != null) ImeAction.Done else ImeAction.Default),
        keyboardActions = KeyboardActions(onDone = { onDone?.invoke() }),
    )
}

/** A new passphrase, typed twice, with the rule (or what is wrong) under it. */
@Composable
fun NewLockFields(a: String, b: String, onA: (String) -> Unit, onB: (String) -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        LockField(a, onA, "passphrase", placeholder = "New passphrase")
        LockField(b, onB, "passphrase", placeholder = "Confirm the passphrase")
        val why = SecretLocks.newLockProblem(a, b)
        Text(why ?: "${SecretLocks.RULE}.", style = CrundiType.Small.copy(color = if (why != null) CrundiColors.Yellow else CrundiColors.TextSecondary))
    }
}

/** The upgrade prompt's state. */
data class UpgradeForm(val a: String = "", val b: String = "", val save: Boolean = false, val busy: Boolean = false, val error: String? = null)

/**
 * "Update this secret's lock": after an old lock was accepted. New + confirm,
 * Update (only once they meet the rule and match) and Not now. Stateless.
 */
@Composable
fun UpgradeLockCard(
    name: String, lock: String, f: UpgradeForm, onForm: (UpgradeForm) -> Unit, onUpdate: () -> Unit, onLater: () -> Unit,
    modifier: Modifier = Modifier, canSave: Boolean = false,
) {
    DialogCard("Update this secret’s lock", modifier, message = null, confirm = null, dismiss = null) {
        Text(name.ifEmpty { "This secret" }, style = CrundiType.Code.copy(color = CrundiColors.AccentHover))
        Text(SecretLocks.upgradeText(lock), style = CrundiType.Body)
        NewLockFields(f.a, f.b, { onForm(f.copy(a = it, error = null)) }, { onForm(f.copy(b = it, error = null)) })
        if (canSave) CheckRow("Save the new passphrase for fingerprint", f.save) { onForm(f.copy(save = it)) }
        f.error?.let { Text(it, style = CrundiType.Small.copy(color = CrundiColors.Red)) }
        Row(Modifier.fillMaxWidth().padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(12.dp, Alignment.End)) {
            CrundiButton("Not now", onLater, enabled = !f.busy)
            CrundiButton(if (f.busy) "Updating…" else "Update", onUpdate, kind = ButtonKind.Primary, enabled = !f.busy && SecretLocks.newLockReady(f.a, f.b))
        }
    }
}

/** What the phone does with a lock it saved behind the fingerprint, once the secret is re-locked. */
object SavedLockAfterRelock {
    /**
     * The old saved lock no longer opens the secret: forget it. Then, when
     * asked ([save], the option on), save the new one (the fingerprint
     * confirms, as the request card's "Save PIN for fingerprint").
     */
    fun apply(keeper: PinKeeper, server: String, secretId: String, name: String, newPin: String, save: Boolean, done: (String?) -> Unit = {}) {
        keeper.forget(server, secretId)
        if (save && keeper.on) keeper.save(server, secretId, name, newPin) { _, why -> done(why) } else done(null)
    }
}

/** A secret to offer the upgrade for, with the lock just accepted (memory only, for this step). */
data class PendingUpgrade(val secretId: String, val name: String, val lock: String, val pin: String)

/**
 * The upgrade prompt, live: re-locks with the lock just typed ([PendingUpgrade.pin]),
 * then forgets the old saved lock and saves the new one if ticked.
 * [onDone] with the new passphrase, or null for Not now.
 */
@Composable
fun UpgradeLockDialog(api: CrundiClient, up: PendingUpgrade, onDone: (String?) -> Unit) {
    var f by remember(up.secretId) { mutableStateOf(UpgradeForm()) }
    val scope = rememberCoroutineScope()
    val keeper = LocalPinKeeper.current
    val server = remember(api) { SavedPins.serverKey(api.transport.baseUrl) }
    fun later() { f = UpgradeForm(); onDone(null) }
    androidx.compose.ui.window.Dialog(onDismissRequest = ::later) {
        UpgradeLockCard(up.name, up.lock, f, { f = it }, onUpdate = {
            val np = f.a
            f = f.copy(busy = true, error = null)
            scope.launch {
                try {
                    api.secrets.edit(up.secretId, up.pin, newPin = np)
                    SavedLockAfterRelock.apply(keeper, server, up.secretId, up.name, np, f.save)
                    f = UpgradeForm()
                    onDone(np)
                } catch (e: Exception) { f = f.copy(busy = false, error = SecretLocks.unsupported(e)) }
            }
        }, onLater = ::later, modifier = Modifier.padding(16.dp), canSave = keeper.on)
    }
}

/** The edit dialog's state. */
data class EditForm(
    val name: String = "", val description: String = "", val value: String = "",
    val relock: Boolean = false, val a: String = "", val b: String = "", val current: String = "",
    val save: Boolean = false, val busy: Boolean = false, val error: String? = null,
) {
    fun metaChanged(s: Secret) = name.trim() != s.name || description != s.description
    fun needsLock() = relock || value.isNotEmpty()
    fun ready(s: Secret): Boolean = name.isNotBlank() && (relock || value.isNotEmpty() || metaChanged(s)) &&
        (!relock || SecretLocks.newLockReady(a, b)) && (!needsLock() || SecretLocks.lockOk(s.lock, current))

    companion object { fun of(s: Secret) = EditForm(name = s.name, description = s.description) }
}

/** Edit a secret: name and description, a new value, a new passphrase; the current lock for either of the last two. Stateless. */
@Composable
fun EditSecretCard(s: Secret, f: EditForm, onForm: (EditForm) -> Unit, onSave: () -> Unit, onCancel: () -> Unit, modifier: Modifier = Modifier, canSave: Boolean = false) {
    DialogCard("Edit secret", modifier, confirm = null, dismiss = null) {
        Column(Modifier.weight(1f, fill = false).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text("NAME AND DESCRIPTION", style = CrundiType.Meta)
            CrundiTextField(f.name, { onForm(f.copy(name = it, error = null)) }, placeholder = "Name")
            CrundiTextField(f.description, { onForm(f.copy(description = it, error = null)) }, placeholder = "Description", singleLine = false, minLines = 2)
            Text("NEW VALUE", style = CrundiType.Meta)
            CrundiTextField(f.value, { onForm(f.copy(value = it, error = null)) }, placeholder = "Leave empty to keep the value", mono = true, password = true)
            Text("NEW PASSPHRASE", style = CrundiType.Meta)
            CheckRow(if (SecretLocks.passphrase(s.lock)) "Change the passphrase" else "Replace the 6-digit PIN with a passphrase", f.relock) {
                onForm(if (it) f.copy(relock = true) else f.copy(relock = false, a = "", b = "", save = false))
            }
            if (f.relock) {
                NewLockFields(f.a, f.b, { onForm(f.copy(a = it, error = null)) }, { onForm(f.copy(b = it, error = null)) })
                if (canSave) CheckRow("Save the new passphrase for fingerprint", f.save) { onForm(f.copy(save = it)) }
            }
            Text(if (SecretLocks.passphrase(s.lock)) "CURRENT PASSPHRASE" else "CURRENT PIN", style = CrundiType.Meta)
            LockField(f.current, { onForm(f.copy(current = it, error = null)) }, s.lock)
            Text("Needed for any change to the value or the lock. It is never stored.", style = CrundiType.Small)
            f.error?.let { Text(it, style = CrundiType.Small.copy(color = CrundiColors.Red)) }
        }
        Row(Modifier.fillMaxWidth().padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(12.dp, Alignment.End)) {
            CrundiButton("Cancel", onCancel, enabled = !f.busy)
            CrundiButton(if (f.busy) "Saving…" else "Save", onSave, kind = ButtonKind.Primary, enabled = !f.busy && f.ready(s))
        }
    }
}

/** The edit dialog, live. [onSaved] once saved (the list reloads). */
@Composable
fun EditSecretDialog(api: CrundiClient, s: Secret, onDone: (saved: Boolean) -> Unit) {
    var f by remember(s.id) { mutableStateOf(EditForm.of(s)) }
    val scope = rememberCoroutineScope()
    val keeper = LocalPinKeeper.current
    val server = remember(api) { SavedPins.serverKey(api.transport.baseUrl) }
    fun cancel() { f = EditForm(); onDone(false) }
    androidx.compose.ui.window.Dialog(onDismissRequest = ::cancel) {
        EditSecretCard(s, f, { f = it }, onSave = {
            val form = f
            f = f.copy(busy = true, error = null)
            scope.launch {
                try {
                    val meta = form.metaChanged(s)
                    if (!form.needsLock()) api.secrets.rename(s.id, form.name.trim(), form.description)
                    else api.secrets.edit(
                        s.id, form.current, newPin = form.a.takeIf { form.relock }, value = form.value.ifEmpty { null },
                        name = if (meta) form.name.trim() else null, description = if (meta) form.description else null,
                    )
                    // A new lock: the one saved behind the fingerprint is no use now.
                    if (form.relock) SavedLockAfterRelock.apply(keeper, server, s.id, form.name.trim().ifEmpty { s.name }, form.a, form.save)
                    f = EditForm()
                    onDone(true)
                } catch (e: Exception) { f = f.copy(busy = false, current = "", error = SecretLocks.unsupported(e)) }
            }
        }, onCancel = ::cancel, modifier = Modifier.padding(16.dp), canSave = keeper.on)
    }
}
