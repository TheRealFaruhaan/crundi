package mv.obitech.crundi.voice

import android.app.role.RoleManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.media.MediaPlayer
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.speech.tts.TextToSpeech
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import mv.obitech.crundi.AppGraph
import mv.obitech.crundi.Servers
import mv.obitech.crundi.api.CrundiClient
import mv.obitech.crundi.api.DeepgramVoice
import mv.obitech.crundi.api.VoiceApi
import mv.obitech.crundi.api.VoiceSettings
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiCard
import mv.obitech.crundi.ui.components.CrundiChip
import mv.obitech.crundi.ui.components.CrundiSwitchRow
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.StatusBadge
import mv.obitech.crundi.ui.components.TopBar
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiTheme
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens
import java.io.File
import java.util.Locale

/** Everything the voice settings screen shows. */
data class VoiceSettingsUi(
    val enabled: Boolean = true,
    /** Crundi holds Android's assistant role (mode A is set up). */
    val isAssistant: Boolean = false,
    val host: String = "",
    /** Null while loading; [error] says why it isn't there. */
    val server: VoiceSettings? = null,
    val error: String = "",
    val deepgram: List<DeepgramVoice> = emptyList(),
    val device: List<DeviceVoice> = emptyList(),
    val deviceChosen: String = "",
    val limitText: String = "",
    val dayText: String = "",
    val busy: Boolean = false,
    val note: String = "",
)

class VoiceSettingsEvents(
    val onBack: () -> Unit = {},
    val onEnabled: (Boolean) -> Unit = {},
    val onAssistantSettings: () -> Unit = {},
    val onPickDeepgram: (String) -> Unit = {},
    val onPickDevice: (String) -> Unit = {},
    val onPreviewDeepgram: (String) -> Unit = {},
    val onPreviewDevice: (String) -> Unit = {},
    val onModel: (String) -> Unit = {},
    val onLimitText: (String) -> Unit = {},
    val onSaveLimit: () -> Unit = {},
    val onDayText: (String) -> Unit = {},
    val onSaveDay: () -> Unit = {},
    val onReset: () -> Unit = {},
    val onTry: () -> Unit = {},
)

/**
 * App settings > Voice assistant: on or off, how to open it (D33's two
 * modes), the voice (the phone's own or a Deepgram voice, D34), the model,
 * the monthly limit and what has been spent. Model, limit and Deepgram voice
 * are the server's settings; on/off and the phone voice are this phone's.
 */
class VoiceSettingsActivity : ComponentActivity() {
    private var ui by mutableStateOf(VoiceSettingsUi())
    private var client: CrundiClient? = null
    private var preview: MediaPlayer? = null
    private var tts: TextToSpeech? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val server = Servers.current(this)
        client = server?.let { AppGraph.forServer(this, it).api }
        ui = ui.copy(host = server?.host.orEmpty(), enabled = VoicePrefs.enabled(this), deviceChosen = VoicePrefs.deviceVoice(this))
        setContent {
            CrundiTheme {
                Box(Modifier.fillMaxSize().background(CrundiColors.BgPrimary).safeDrawingPadding()) {
                    VoiceSettingsScreen(ui, events())
                }
            }
        }
        listDeviceVoices(this) { ui = ui.copy(device = it) }
        load()
    }

    override fun onResume() {
        super.onResume()
        ui = ui.copy(isAssistant = holdsAssistant(this))
    }

    override fun onDestroy() {
        preview?.release(); tts?.shutdown()
        super.onDestroy()
    }

    private fun voice(): VoiceApi? = client?.voice

    private fun load() {
        val v = voice() ?: run { ui = ui.copy(error = "Pair this phone with a Crundi server first."); return }
        lifecycleScope.launch {
            try {
                val s = v.settings()
                ui = ui.copy(server = s, error = "", limitText = fmtUsd(s.monthlyLimitUsd), dayText = s.billingDay.toString())
                runCatching { v.voices() }.getOrNull()?.let { ui = ui.copy(deepgram = it.voices) }
            } catch (e: Exception) {
                ui = ui.copy(error = if (VoiceApi.unsupported(e)) VoiceApi.UNSUPPORTED else e.message ?: "Could not load the settings")
            }
        }
    }

    private fun save(model: String? = null, limit: Double? = null, ttsVoice: String? = null, day: Int? = null, note: String = "Saved") {
        val v = voice() ?: return
        ui = ui.copy(busy = true)
        lifecycleScope.launch {
            ui = try {
                val s = v.save(model, limit, ttsVoice, day)
                ui.copy(server = s, busy = false, note = note, limitText = fmtUsd(s.monthlyLimitUsd), dayText = s.billingDay.toString())
            } catch (e: Exception) { ui.copy(busy = false, note = e.message ?: "Could not save") }
        }
    }

    private fun events() = VoiceSettingsEvents(
        onBack = { finish() },
        onEnabled = { on ->
            VoicePrefs.setEnabled(this, on)
            // The "Crundi Voice" launcher entry comes and goes with it.
            packageManager.setComponentEnabledSetting(
                ComponentName(this, "$packageName.voice.CrundiVoice"),
                if (on) PackageManager.COMPONENT_ENABLED_STATE_DEFAULT else PackageManager.COMPONENT_ENABLED_STATE_DISABLED,
                PackageManager.DONT_KILL_APP,
            )
            ui = ui.copy(enabled = on)
        },
        onAssistantSettings = { openAssistantSettings() },
        onPickDeepgram = { save(ttsVoice = it, note = "Crundi will speak with this voice") },
        onPickDevice = { name ->
            VoicePrefs.setDeviceVoice(this, name)
            ui = ui.copy(deviceChosen = name)
            if (ui.server?.ttsVoice?.isNotEmpty() == true) save(ttsVoice = "", note = "Crundi will speak with the phone's voice") else ui = ui.copy(note = "Crundi will speak with the phone's voice")
        },
        onPreviewDeepgram = { previewDeepgram(it) },
        onPreviewDevice = { previewDevice(it) },
        onModel = { save(model = it, note = "Model changed. It applies from the next conversation.") },
        onLimitText = { ui = ui.copy(limitText = it) },
        onSaveLimit = {
            val n = ui.limitText.trim().removePrefix("$").toDoubleOrNull()
            if (n == null || n < 0) ui = ui.copy(note = "The limit is a number of dollars") else save(limit = n, note = "Limit saved")
        },
        onDayText = { ui = ui.copy(dayText = it) },
        onSaveDay = {
            val d = ui.dayText.trim().toIntOrNull()
            if (d == null || d !in 1..31) ui = ui.copy(note = "The billing day is a day of the month, 1 to 31") else save(day = d, note = "Billing day saved")
        },
        onReset = {
            lifecycleScope.launch { ui = ui.copy(note = runCatching { voice()?.reset(); "Crundi starts a fresh conversation" }.getOrElse { it.message ?: "Could not reset" }) }
        },
        onTry = { startActivity(Intent(this, VoiceActivity::class.java)) },
    )

    private fun previewDeepgram(model: String) {
        val v = voice() ?: return
        lifecycleScope.launch {
            try {
                val mp3 = v.speak(SAMPLE, model)
                val f = withContext(Dispatchers.IO) { File(cacheDir, "voice-preview.mp3").apply { writeBytes(mp3) } }
                preview?.release()
                preview = MediaPlayer().apply { setDataSource(f.path); setOnPreparedListener { it.start() }; prepareAsync() }
            } catch (e: Exception) { ui = ui.copy(note = e.message ?: "Could not play it") }
        }
    }

    private fun previewDevice(name: String) {
        val say = { t: TextToSpeech ->
            t.voices?.firstOrNull { it.name == name }?.let { t.setVoice(it) } ?: t.setLanguage(Locale.UK)
            t.speak(SAMPLE, TextToSpeech.QUEUE_FLUSH, null, "preview")
        }
        tts?.let { say(it); return }
        var made: TextToSpeech? = null
        made = TextToSpeech(this) { st -> if (st == TextToSpeech.SUCCESS) made?.let { tts = it; say(it) } }
    }

    private fun openAssistantSettings() {
        val tries = listOf(Intent(Settings.ACTION_MANAGE_DEFAULT_APPS_SETTINGS), Intent(Settings.ACTION_VOICE_INPUT_SETTINGS), Intent(Settings.ACTION_SETTINGS))
        for (i in tries) if (runCatching { startActivity(i) }.isSuccess) return
    }

    companion object {
        const val SAMPLE = "Good evening, Master. Everything is running smoothly, and the kanban board is in order."
        const val RECOMMENDED = "aura-2-draco-en"

        fun holdsAssistant(ctx: Context): Boolean =
            if (Build.VERSION.SDK_INT >= 29) runCatching { ctx.getSystemService(RoleManager::class.java)?.isRoleHeld(RoleManager.ROLE_ASSISTANT) == true }.getOrDefault(false)
            else false
    }
}

fun fmtUsd(v: Double): String = if (v >= 100 || v == Math.floor(v)) "%.0f".format(Locale.US, v) else "%.2f".format(Locale.US, v)

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun VoiceSettingsScreen(ui: VoiceSettingsUi, ev: VoiceSettingsEvents) {
    Column(Modifier.fillMaxSize()) {
        TopBar("Voice assistant", onBack = ev.onBack, subtitle = ui.host.takeIf { it.isNotEmpty() }?.let { "Crundi on $it" })
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(Dimens.ScreenPad), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            CrundiCard(Modifier.fillMaxWidth(), padding = 4.dp) {
                CrundiSwitchRow("Voice assistant", ui.enabled, ev.onEnabled, detail = if (ui.enabled) "On. Ask Crundi anything about your projects, by voice." else "Off. The side key and Crundi Voice do nothing.")
            }
            if (ui.note.isNotEmpty()) StatusBadge(ui.note, BadgeKind.Info)

            // ─── How to open it (D33) ───
            Section("How to open it")
            Mode(
                "A. Hold the side key",
                "Make Crundi the phone's digital assistant app, then hold the side key. Works on the lock screen. While Crundi is the assistant, \"Hey Google\" does not work.",
                if (ui.isAssistant) "Crundi is the digital assistant" else "Not set up",
                ui.isAssistant,
                steps = "Settings > Apps > Choose default apps > Digital assistant app > Crundi. On Samsung, also Advanced features > Side button > Press and hold > Digital assistant.",
            ) { CrundiButton("Choose the digital assistant", ev.onAssistantSettings, icon = "settings") }
            Mode(
                "B. Keep Gemini, double-press the side key",
                "Gemini stays the assistant and \"Hey Google\" keeps working. Crundi Voice opens on a double press, or when you say \"Hey Google, open Crundi Voice\". Works after unlocking.",
                if (ui.isAssistant) "Gemini is not the assistant now" else "Available",
                null,
                steps = "On Samsung: Advanced features > Side button > Double press > Open app > Crundi Voice.",
            ) { CrundiButton("Try Crundi Voice", ev.onTry, icon = "mic") }

            when {
                ui.error.isNotEmpty() -> CrundiCard(Modifier.fillMaxWidth()) { Text(ui.error, style = CrundiType.Body.copy(color = CrundiColors.Yellow)) }
                ui.server == null -> Text("Loading the server's voice settings…", style = CrundiType.Small)
                else -> ui.server?.let { ServerPart(ui, it, ev) }
            }
        }
    }
}

@Composable
private fun Section(text: String) {
    Text(text.uppercase(), style = CrundiType.Section, modifier = Modifier.padding(top = 8.dp, start = 2.dp))
}

@Composable
private fun Mode(title: String, body: String, state: String, good: Boolean?, steps: String, action: @Composable () -> Unit) {
    CrundiCard(Modifier.fillMaxWidth(), padding = 14.dp) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(title, style = CrundiType.Heading, modifier = Modifier.weight(1f))
            StatusBadge(state, when (good) { true -> BadgeKind.Good; false -> BadgeKind.Warn; null -> BadgeKind.Neutral })
        }
        Text(body, style = CrundiType.Body, modifier = Modifier.padding(top = 6.dp))
        Text(steps, style = CrundiType.Small, modifier = Modifier.padding(top = 6.dp, bottom = 10.dp))
        action()
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ServerPart(ui: VoiceSettingsUi, s: VoiceSettings, ev: VoiceSettingsEvents) {
    // ─── The voice (D19, D34) ───
    Section("Voice")
    CrundiCard(Modifier.fillMaxWidth(), padding = 6.dp) {
        val british = ui.deepgram.filter { it.accent.contains("Brit", true) }
        val others = ui.deepgram.filter { !it.accent.contains("Brit", true) }
        if (ui.deepgram.isEmpty()) Text(if (s.deepgramKeySet) "Loading Deepgram's voices…" else "No Deepgram key on the server: only the phone's voices.", style = CrundiType.Small, modifier = Modifier.padding(8.dp))
        for (v in british + others.take(12)) {
            VoiceRow(
                v.name + if (v.model == VoiceSettingsActivity.RECOMMENDED) "  ·  recommended" else "",
                listOf(v.accent, v.gender, v.tags.filterNot { it.equals(v.gender, true) || it == "masculine" || it == "feminine" }.take(3).joinToString(", ")).filter { it.isNotEmpty() }.joinToString(" · "),
                s.ttsVoice == v.model,
                onPick = { ev.onPickDeepgram(v.model) }, onPreview = { ev.onPreviewDeepgram(v.model) },
            )
        }
        Text("ON THIS PHONE", style = CrundiType.Section, modifier = Modifier.padding(start = 8.dp, top = 12.dp, bottom = 4.dp))
        val gb = ui.device.filter { it.locale.equals("en-GB", true) }
        val picked = DeviceVoices.pick(ui.device, ui.deviceChosen)?.name
        if (gb.isEmpty()) Text("The phone's text-to-speech has no British English voice.", style = CrundiType.Small, modifier = Modifier.padding(8.dp))
        for (v in gb) {
            VoiceRow(
                v.name + if (v.name == DeviceVoices.PREFERRED) "  ·  default" else "",
                listOf(v.locale, if (v.needsNetwork) "needs the network" else "on the phone", if (!v.installed) "not downloaded" else "").filter { it.isNotEmpty() }.joinToString(" · "),
                s.ttsVoice.isEmpty() && v.name == picked,
                onPick = { ev.onPickDevice(v.name) }, onPreview = { ev.onPreviewDevice(v.name) },
            )
        }
    }

    // ─── Model (D8) ───
    Section("Model")
    CrundiCard(Modifier.fillMaxWidth(), padding = 14.dp) {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            for (m in s.models) CrundiChip(m.label, m.id == s.model, { if (m.id != s.model) ev.onModel(m.id) })
        }
        Text("Haiku is the quickest and cheapest. A change starts a fresh conversation's cache.", style = CrundiType.Small, modifier = Modifier.padding(top = 8.dp))
    }

    // ─── Spend and limit (D25) ───
    Section("Spending")
    CrundiCard(Modifier.fillMaxWidth(), padding = 14.dp) {
        val sp = s.spend
        Row(verticalAlignment = Alignment.Bottom) {
            Text("This billing month", style = CrundiType.Heading, modifier = Modifier.weight(1f))
            Text("$" + "%.2f".format(Locale.US, sp.totalUsd) + " of $" + fmtUsd(s.monthlyLimitUsd), style = CrundiType.Code.copy(color = if (s.limitReached) CrundiColors.Red else CrundiColors.TextPrimary))
        }
        val frac = if (s.monthlyLimitUsd > 0) (sp.totalUsd / s.monthlyLimitUsd).toFloat().coerceIn(0f, 1f) else 1f
        mv.obitech.crundi.ui.operations.PaceBar(frac, 0f, Modifier.padding(vertical = 8.dp))
        Text("Claude $" + "%.2f".format(Locale.US, sp.claudeUsd) + " · Deepgram $" + "%.2f".format(Locale.US, sp.deepgramUsd) + " · " + sp.turns + " request" + if (sp.turns == 1) "" else "s", style = CrundiType.Meta.copy(color = CrundiColors.TextSecondary))
        if (s.limitReached) Text("The limit is reached: Crundi stops answering until next month or a higher limit.", style = CrundiType.Small.copy(color = CrundiColors.Red), modifier = Modifier.padding(top = 6.dp))
        if (s.nextReset.isNotEmpty()) Text("Restarts on ${s.nextReset}, the API account's billing day.", style = CrundiType.Small, modifier = Modifier.padding(top = 4.dp))
        Row(Modifier.padding(top = 12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiTextField(ui.limitText, ev.onLimitText, Modifier.weight(1f), placeholder = "Monthly limit in dollars", icon = "sliders")
            CrundiButton("Save", ev.onSaveLimit, kind = ButtonKind.Primary, enabled = !ui.busy)
        }
        if (s.periodStart.isNotEmpty()) Row(Modifier.padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiTextField(ui.dayText, ev.onDayText, Modifier.weight(1f), placeholder = "Billing day of the month, 1 to 31", icon = "clock")
            CrundiButton("Save", ev.onSaveDay, enabled = !ui.busy)
        }
    }

    // ─── Keys (D25: never on the phone) ───
    Section("Keys")
    CrundiCard(Modifier.fillMaxWidth(), padding = 14.dp) {
        KeyLine("Claude API key", s.anthropicKeySet)
        KeyLine("Deepgram key", s.deepgramKeySet)
        Text("Keys live on the server and never come to the phone. Set them at ${ui.host}/voice.", style = CrundiType.Small, modifier = Modifier.padding(top = 6.dp))
    }
    CrundiButton("Start a fresh conversation", ev.onReset, icon = "refresh", modifier = Modifier.fillMaxWidth())
}

@Composable
private fun KeyLine(name: String, set: Boolean) {
    Row(Modifier.padding(vertical = 3.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(name, style = CrundiType.Body, modifier = Modifier.weight(1f))
        StatusBadge(if (set) "set" else "not set", if (set) BadgeKind.Good else BadgeKind.Warn)
    }
}

@Composable
private fun VoiceRow(title: String, detail: String, selected: Boolean, onPick: () -> Unit, onPreview: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(if (selected) CrundiColors.AccentDim else androidx.compose.ui.graphics.Color.Transparent)
            .clickable(onClick = onPick).padding(horizontal = 10.dp, vertical = 9.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(18.dp).clip(CircleShape).border(2.dp, if (selected) CrundiColors.AccentHover else CrundiColors.Border, CircleShape), contentAlignment = Alignment.Center) {
            if (selected) Box(Modifier.size(8.dp).clip(CircleShape).background(CrundiColors.AccentHover))
        }
        Column(Modifier.weight(1f).padding(start = 12.dp)) {
            Text(title, style = CrundiType.Body.copy(color = if (selected) CrundiColors.AccentHover else CrundiColors.TextPrimary))
            if (detail.isNotEmpty()) Text(detail, style = CrundiType.Small)
        }
        Box(Modifier.size(40.dp).clip(CircleShape).clickable(onClickLabel = "Hear it", onClick = onPreview), contentAlignment = Alignment.Center) {
            CrundiIcon("play", size = 16.dp, tint = CrundiColors.TextSecondary)
        }
    }
}
