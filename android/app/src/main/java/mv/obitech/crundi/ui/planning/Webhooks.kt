package mv.obitech.crundi.ui.planning

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import mv.obitech.crundi.Versions
import mv.obitech.crundi.api.model.FactoryTrigger
import mv.obitech.crundi.api.model.HookSettings
import mv.obitech.crundi.api.model.Schedule
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiChip
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.shell.ShellData
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/**
 * Webhooks, as the web has them since server 1.21.1: a schedule that runs
 * "When called" (src/schedule-hooks.js) and a factory trigger signed with a
 * secret (src/factory-triggers.js). An older server has neither, so what it
 * cannot do stays hidden and the old editors work as they did.
 */
object Webhooks {
    /** The handshake's name for them (src/client-version.js CAPABILITIES). */
    const val CAPABILITY = "webhooks"

    /** The first build that has them. It (and 1.21.1) does not list [CAPABILITY], so the version counts too. */
    const val FIRST_SERVER = "1.21.1-dev.12"

    const val DEFAULT_SIGNATURE_HEADER = "x-crundi-signature"

    /** What a "When called" schedule may run (HOOK_ACTIONS in src/schedule-hooks.js). */
    val ACTIONS = setOf("chat", "service")

    /** The server says it has them, by name or by version. */
    fun supported(serverVersion: String, capabilities: Set<String>): Boolean =
        CAPABILITY in capabilities || Versions.compare(serverVersion, FIRST_SERVER) >= 0

    fun supported(shell: ShellData) = supported(shell.serverVersion, shell.capabilities)

    /** The data shows them: a server that sends webhook settings has them, whatever its handshake said. */
    fun inSchedules(list: List<Schedule>) = list.any { it.webhook != null || it.calledNotTimed }

    fun inTriggers(list: List<FactoryTrigger>) = list.any { it.webhook?.auth != null || it.ifBusy == "manager" }

    /** The settings a new webhook starts with (the web's defaultSchedule().webhook). */
    val DEFAULTS = HookSettings(auth = "key", signatureHeader = DEFAULT_SIGNATURE_HEADER, timestampHeader = "", idempotency = "")

    /** [h] with every field the editor shows filled, keeping the key and secret and whatever the server sent. */
    fun filled(h: HookSettings?) = HookSettings(
        key = h?.key, secret = h?.secret,
        auth = h?.auth ?: DEFAULTS.auth,
        signatureHeader = h?.signatureHeader ?: DEFAULTS.signatureHeader,
        timestampHeader = h?.timestampHeader ?: DEFAULTS.timestampHeader,
        idempotency = h?.idempotency ?: DEFAULTS.idempotency,
    )

    /** The URL to POST to. A key schedule carries its key in it; a signed one does not. */
    fun url(host: String, path: String, h: HookSettings?): String =
        "https://$host$path" + if (h == null || h.signed || h.key.isNullOrEmpty()) "" else "?key=${h.key}"

    fun scheduleUrl(host: String, id: String, h: HookSettings?) = url(host, "/api/schedule-hook/$id", h)
    fun triggerUrl(host: String, id: String, h: HookSettings?) = url(host, "/api/factory-hook/trigger/$id", h)

    /** A credential as dots, keeping its length roughly. */
    fun mask(s: String) = "•".repeat(s.length.coerceIn(8, 24))

    /** [url] with its key hidden. */
    fun maskedUrl(url: String): String = url.substringBefore("?key=").let { base -> if (base == url) url else "$base?key=" + mask(url.substringAfter("?key=")) }

    /** The placeholders a webhook's text may hold (the web's hint, and the server's varsFor). */
    const val PLACEHOLDERS = "{{payload.some.field}} (arrays by index: {{payload.alerts.0.labels.alertname}}), {{headers.name}}, {{query.name}}, {{date}}, {{time}}"
}

@Composable
private fun HookNote(t: String) = Text(t, style = CrundiType.Small.copy(fontSize = 12.5.sp, color = CrundiColors.TextMuted))

/**
 * How calls prove who they are from, and what makes a repeat: Key or
 * Signature (with its headers), and the idempotency key template. [label]
 * draws a field's title in the editor's own style.
 */
@Composable
fun HookAuthFields(h: HookSettings, onChange: (HookSettings) -> Unit, label: @Composable (String) -> Unit) {
    label("Security")
    FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        CrundiChip("Key in the URL", !h.signed, { onChange(h.copy(auth = "key")) }, icon = "key")
        CrundiChip("Signed with a secret", h.signed, { onChange(h.copy(auth = "signature")) }, icon = "lock")
    }
    HookNote(if (h.signed) "HMAC-SHA256 of the raw body with the secret, sent as hex (or sha256=hex) in the signature header." else "The key goes in the URL, or in an X-Crundi-Key or Bearer header.")
    if (h.signed) {
        label("Signature header")
        CrundiTextField(h.signatureHeader.orEmpty(), { onChange(h.copy(signatureHeader = it)) }, Modifier.fillMaxWidth(), placeholder = Webhooks.DEFAULT_SIGNATURE_HEADER, mono = true)
        label("Timestamp header (optional)")
        CrundiTextField(h.timestampHeader.orEmpty(), { onChange(h.copy(timestampHeader = it)) }, Modifier.fillMaxWidth(), placeholder = "X-Grafana-Alerting-Signature-Timestamp", mono = true)
    }
    label("Idempotency key")
    CrundiTextField(h.idempotency.orEmpty(), { onChange(h.copy(idempotency = it)) }, Modifier.fillMaxWidth(), placeholder = "{{payload.groupKey}}", mono = true)
    HookNote("A call with a key already seen is not processed again (remembered 30 days).")
}

/** A read-only value: the URL, the secret. */
@Composable
private fun HookValue(text: String) {
    Text(
        text, style = CrundiType.Code.copy(fontSize = 12.5.sp, color = CrundiColors.TextPrimary),
        modifier = Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgPrimary)
            .border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm).padding(horizontal = 10.dp, vertical = 8.dp),
    )
}

/**
 * Where to call it and with what: the URL (POST, JSON body), and for a
 * signed one its secret, each hidden until shown, with Copy; then New key or
 * New secret ([onRotate] with "key" or "secret"; the caller confirms).
 * [onCopied] says it was copied.
 */
@Composable
fun HookAddress(url: String, h: HookSettings, label: @Composable (String) -> Unit, onRotate: (String) -> Unit, onCopied: (String) -> Unit = {}) {
    val clip = LocalClipboardManager.current
    var showKey by remember { mutableStateOf(false) }
    var showSecret by remember { mutableStateOf(false) }
    val hasKey = !h.signed && url.contains("?key=")
    label("URL (POST, JSON body)")
    HookValue(if (hasKey && !showKey) Webhooks.maskedUrl(url) else url)
    FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        if (hasKey) CrundiButton(if (showKey) "Hide key" else "Show key", { showKey = !showKey }, kind = ButtonKind.Ghost, icon = if (showKey) "eye-off" else "eye")
        CrundiButton("Copy URL", { clip.setText(AnnotatedString(url)); onCopied("URL copied") }, kind = ButtonKind.Ghost, icon = "copy")
        if (!h.signed) CrundiButton("New key", { onRotate("key") }, kind = ButtonKind.Ghost, icon = "refresh")
    }
    if (h.signed) {
        val secret = h.secret.orEmpty()
        label("Secret")
        HookValue(if (showSecret) secret else Webhooks.mask(secret))
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            CrundiButton(if (showSecret) "Hide" else "Show", { showSecret = !showSecret }, kind = ButtonKind.Ghost, icon = if (showSecret) "eye-off" else "eye")
            CrundiButton("Copy secret", { clip.setText(AnnotatedString(secret)); onCopied("Secret copied") }, kind = ButtonKind.Ghost, icon = "copy", enabled = secret.isNotEmpty())
            CrundiButton("New secret", { onRotate("secret") }, kind = ButtonKind.Ghost, icon = "refresh")
        }
        val sig = h.signatureHeader.orEmpty().ifEmpty { Webhooks.DEFAULT_SIGNATURE_HEADER }
        val ts = h.timestampHeader.orEmpty()
        HookNote("Send the signature in $sig as hex (or sha256=<hex>)" + (if (ts.isNotEmpty()) ", and the time in $ts; sign \"<time>:<body>\"" else "") + ". Grafana: in the webhook contact point set HMAC Signature with this secret" + (if (ts.isNotEmpty()) " and this timestamp header" else "") + ".")
    }
}

/** The confirm text for replacing a key or secret (the web's words). */
fun rotateQuestion(which: String) = if (which == "secret") "Calls signed with the old secret stop working. Make a new one?" else "The old URL stops working. Make a new key?"
