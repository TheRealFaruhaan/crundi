package mv.obitech.crundi.voice

import android.Manifest
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
import androidx.activity.result.contract.ActivityResultContracts
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
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import mv.obitech.crundi.AppGraph
import mv.obitech.crundi.ServerCaps
import mv.obitech.crundi.Servers
import mv.obitech.crundi.UrgentAlerts
import mv.obitech.crundi.api.CrundiClient
import mv.obitech.crundi.api.DeepgramVoice
import mv.obitech.crundi.api.MemoryHit
import mv.obitech.crundi.api.VoiceMemory
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
    val dgDayText: String = "",
    val dgLimitText: String = "",
    val busy: Boolean = false,
    val note: String = "",
    /** "Hey Crundi" (Phase 6): this build has the keyword spotter for this phone. */
    val wake: WakeUi = WakeUi(),
    /** The notebook (capability "voice-memory"); null when the server has none. */
    val memory: MemoryUi? = null,
)

/** The "Hey Crundi" section. */
data class WakeUi(
    val supported: Boolean = true,
    val inApp: Boolean = false,
    /** Background listening is on. */
    val background: Boolean = false,
    /** The settings choice, minutes; 0 = until turned off (D7). */
    val minutes: Int = HeyCrundiPlan.DEFAULT_MINUTES,
    /** "24 min left", "Until you turn it off". */
    val left: String = "",
    /** Who has the microphone for the wake word now. */
    val listening: HeyCrundiPlan.Listener = HeyCrundiPlan.Listener.None,
    /** Full-screen notifications are allowed: heard in the background, the voice screen opens over the lock screen. */
    val fullScreen: Boolean = true,
    /** The microphone's time after the last thing said or answered, seconds. */
    val micSeconds: Int = 30,
    /** Then the screen stays on, listening for "Hey Crundi", this many minutes. */
    val screenMinutes: Int = 2,
)

/** The notebook: files as the server has them, Master's edits, the search. */
data class MemoryUi(
    val loaded: VoiceMemory? = null,
    val error: String = "",
    /** path to the text being edited (only files that differ from [loaded]). */
    val drafts: Map<String, String> = emptyMap(),
    /** The file whose Delete was tapped once: tap again to delete. */
    val confirmDelete: String = "",
    val query: String = "",
    val hits: List<MemoryHit>? = null,
    val searching: Boolean = false,
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
    val onDgDayText: (String) -> Unit = {},
    val onSaveDgDay: () -> Unit = {},
    val onPause: (Int) -> Unit = {},
    val onDgLimitText: (String) -> Unit = {},
    val onSaveDgLimit: () -> Unit = {},
    val onReset: () -> Unit = {},
    val onTry: () -> Unit = {},
    val onWakeInApp: (Boolean) -> Unit = {},
    val onWakeBackground: (Boolean) -> Unit = {},
    val onWakeMinutes: (Int) -> Unit = {},
    val onMicSeconds: (Int) -> Unit = {},
    val onScreenMinutes: (Int) -> Unit = {},
    val onFullScreen: () -> Unit = {},
    val onMemoryText: (String, String) -> Unit = { _, _ -> },
    val onMemorySave: (String) -> Unit = {},
    val onMemoryDelete: (String) -> Unit = {},
    val onMemoryQuery: (String) -> Unit = {},
    val onMemoryReload: () -> Unit = {},
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
    private var search: Job? = null
    /** What to do once the microphone (and notifications) are allowed. */
    private var afterGrant: (() -> Unit)? = null
    private val askMic = registerForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { r ->
        HeyCrundi.refresh()
        if (r[Manifest.permission.RECORD_AUDIO] == true || micGranted()) afterGrant?.invoke()
        else ui = ui.copy(note = "Hey Crundi needs the microphone. Allow it in Android's settings for Crundi.")
        afterGrant = null
        wakeRefresh()
    }

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
        // "Hey Crundi stopped, tap to resume": the tap is in front, so the microphone may start again.
        if (intent?.getBooleanExtra(HeyCrundiService.EXTRA_RESUME, false) == true && micGranted()) HeyCrundiService.resume(this)
        lifecycleScope.launch { HeyCrundi.listening.collect { wakeRefresh() } }
        // The time left, while the screen is open.
        lifecycleScope.launch { while (isActive) { wakeRefresh(); delay(15_000) } }
    }

    override fun onResume() {
        super.onResume()
        ui = ui.copy(isAssistant = holdsAssistant(this))
        wakeRefresh()
    }

    private fun micGranted() = checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED

    private fun wakeRefresh() {
        val on = VoicePrefs.wakeOn(this)
        ui = ui.copy(wake = WakeUi(
            supported = WakeWordEngine.supported(this),
            inApp = VoicePrefs.wakeInApp(this),
            background = on,
            minutes = VoicePrefs.wakeMinutes(this),
            left = if (on) HeyCrundiPlan.timeLeft(System.currentTimeMillis(), VoicePrefs.wakeDeadline(this)) else "",
            listening = HeyCrundi.listening.value,
            fullScreen = UrgentAlerts.fullScreenGranted(this),
            micSeconds = VoicePrefs.silenceSeconds(this),
            screenMinutes = VoicePrefs.screenAwakeMinutes(this),
        ))
    }

    /** Run [then] with the microphone allowed, asking first if need be (notifications too, for background listening's). */
    private fun withMic(then: () -> Unit) {
        if (micGranted()) { then(); return }
        afterGrant = then
        val ask = mutableListOf(Manifest.permission.RECORD_AUDIO)
        if (Build.VERSION.SDK_INT >= 33) ask += Manifest.permission.POST_NOTIFICATIONS
        askMic.launch(ask.toTypedArray())
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
                ui = ui.copy(server = s, error = "", limitText = fmtUsd(s.monthlyLimitUsd), dayText = s.billingDay.toString(), dgDayText = s.deepgramBillingDay.toString(), dgLimitText = fmtUsd(s.deepgramLimitUsd))
                runCatching { v.voices() }.getOrNull()?.let { ui = ui.copy(deepgram = it.voices) }
                loadMemory()
            } catch (e: Exception) {
                ui = ui.copy(error = if (VoiceApi.unsupported(e)) VoiceApi.UNSUPPORTED else e.message ?: "Could not load the settings")
            }
        }
    }

    // ─── memory (D39: the same notebook as /voice) ───

    private fun hasMemory(): Boolean {
        val server = Servers.current(this) ?: return false
        return VoiceApi.MEMORY in ServerCaps.get(this, server.id)
    }

    private fun loadMemory() {
        val v = voice() ?: return
        if (!hasMemory()) { ui = ui.copy(memory = null); return }
        lifecycleScope.launch {
            val m = ui.memory ?: MemoryUi()
            ui = try {
                val got = v.memory()
                ui.copy(memory = m.copy(loaded = got, error = "", drafts = m.drafts.filter { (p, t) -> got.files.any { it.path == p && it.text != t } }))
            } catch (e: Exception) {
                ui.copy(memory = m.copy(error = if (VoiceApi.unsupported(e)) "This server has no voice memory yet. Update the server." else e.message ?: "Could not load the memory"))
            }
        }
    }

    private fun memory(change: (MemoryUi) -> MemoryUi) { ui.memory?.let { ui = ui.copy(memory = change(it)) } }

    private fun saveMemory(path: String, text: String, note: String) {
        val v = voice() ?: return
        lifecycleScope.launch {
            try {
                v.saveMemory(path, text)
                memory { it.copy(drafts = it.drafts - path, confirmDelete = "") }
                ui = ui.copy(note = note)
                loadMemory()
            } catch (e: Exception) { ui = ui.copy(note = e.message ?: "Not saved") }
        }
    }

    private fun searchMemory(q: String) {
        memory { it.copy(query = q) }
        search?.cancel()
        if (q.isBlank()) { memory { it.copy(hits = null, searching = false) }; return }
        val v = voice() ?: return
        search = lifecycleScope.launch {
            delay(350)
            memory { it.copy(searching = true) }
            val hits = runCatching { v.searchMemory(q.trim()) }.getOrElse { e -> ui = ui.copy(note = e.message ?: "Search failed"); emptyList() }
            memory { it.copy(hits = hits, searching = false) }
        }
    }

    private fun save(model: String? = null, limit: Double? = null, ttsVoice: String? = null, day: Int? = null, dgDay: Int? = null, pause: Int? = null, dgLimit: Double? = null, note: String = "Saved") {
        val v = voice() ?: return
        ui = ui.copy(busy = true)
        lifecycleScope.launch {
            ui = try {
                val s = v.save(model, limit, ttsVoice, day, dgDay, pause, dgLimit)
                ui.copy(server = s, busy = false, note = note, limitText = fmtUsd(s.monthlyLimitUsd), dayText = s.billingDay.toString(), dgDayText = s.deepgramBillingDay.toString(), dgLimitText = fmtUsd(s.deepgramLimitUsd))
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
        onDgDayText = { ui = ui.copy(dgDayText = it) },
        onSaveDgDay = {
            val d = ui.dgDayText.trim().toIntOrNull()
            if (d == null || d !in 1..31) ui = ui.copy(note = "The billing day is a day of the month, 1 to 31") else save(dgDay = d, note = "Deepgram billing day saved")
        },
        onDgLimitText = { ui = ui.copy(dgLimitText = it) },
        onSaveDgLimit = {
            val n = ui.dgLimitText.trim().removePrefix("$").toDoubleOrNull()
            if (n == null || n < 0) ui = ui.copy(note = "The limit is a number of dollars") else save(dgLimit = n, note = "Deepgram limit saved")
        },
        onPause = { ms -> save(pause = ms, note = "Crundi waits ${ms / 1000.0} s of quiet before sending") },
        onReset = {
            lifecycleScope.launch { ui = ui.copy(note = runCatching { voice()?.reset(); "Crundi starts a fresh conversation" }.getOrElse { it.message ?: "Could not reset" }) }
        },
        onTry = { startActivity(Intent(this, VoiceActivity::class.java)) },
        onMicSeconds = { v -> VoicePrefs.setSilenceSeconds(this, v); wakeRefresh() },
        onScreenMinutes = { v -> VoicePrefs.setScreenAwakeMinutes(this, v); wakeRefresh() },
        onWakeInApp = { on ->
            if (on) withMic { HeyCrundi.setInApp(true); ui = ui.copy(note = "Say \"Hey Crundi\" while Crundi is open"); wakeRefresh() }
            else { HeyCrundi.setInApp(false); wakeRefresh() }
        },
        onWakeBackground = { on ->
            if (on) withMic {
                val m = VoicePrefs.wakeMinutes(this)
                HeyCrundiService.start(this, m)
                ui = ui.copy(note = if (m == HeyCrundiPlan.UNTIL_OFF) "Listening for \"Hey Crundi\" until you turn it off" else "Listening for \"Hey Crundi\" for ${HeyCrundiPlan.label(m)}")
                wakeRefresh()
            } else { HeyCrundiService.stop(this); wakeRefresh() }
        },
        onWakeMinutes = { m ->
            VoicePrefs.setWakeMinutes(this, m)
            // On already: the new choice starts now.
            if (VoicePrefs.wakeOn(this) && micGranted()) HeyCrundiService.start(this, m)
            wakeRefresh()
        },
        onFullScreen = {
            if (Build.VERSION.SDK_INT >= 34) runCatching { startActivity(Intent(Settings.ACTION_MANAGE_APP_USE_FULL_SCREEN_INTENT, android.net.Uri.parse("package:$packageName"))) }
        },
        onMemoryText = { path, text ->
            val orig = ui.memory?.loaded?.files?.firstOrNull { it.path == path }?.text
            memory { it.copy(drafts = if (text == orig) it.drafts - path else it.drafts + (path to text), confirmDelete = "") }
        },
        onMemorySave = { path -> ui.memory?.drafts?.get(path)?.let { saveMemory(path, it, "Saved $path") } },
        onMemoryDelete = { path ->
            if (ui.memory?.confirmDelete == path) saveMemory(path, "", "Deleted $path")
            else memory { it.copy(confirmDelete = path) }
        },
        onMemoryQuery = { searchMemory(it) },
        onMemoryReload = { loadMemory() },
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

            // ─── Winding down: the microphone, then "Hey Crundi", then sleep ───
            Section("On the voice screen")
            ScreenTimes(ui.wake, ev)

            // ─── "Hey Crundi" (Phase 6, D3–D7) ───
            Section("Hey Crundi")
            HeyCrundiPart(ui.wake, ui.isAssistant, ev)

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

    // ─── Pause before sending (D44) ───
    Section("Listening")
    CrundiCard(Modifier.fillMaxWidth(), padding = 14.dp) {
        Text("Pause before sending", style = CrundiType.Heading)
        Text("How long you can pause before what you said is sent. Longer lets you speak slowly, with pauses.", style = CrundiType.Small, modifier = Modifier.padding(top = 4.dp, bottom = 10.dp))
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            for (ms in listOf(1500, 2000, 2500, 3000, 4000)) CrundiChip("${ms / 1000.0} s".replace(".0 s", " s"), s.pauseMs == ms, { if (s.pauseMs != ms) ev.onPause(ms) })
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
            Text("Claude API", style = CrundiType.Heading, modifier = Modifier.weight(1f))
            Text("$" + "%.2f".format(Locale.US, sp.claudeUsd) + " of $" + fmtUsd(s.monthlyLimitUsd), style = CrundiType.Code.copy(color = if (s.limitReached) CrundiColors.Red else CrundiColors.TextPrimary))
        }
        val frac = if (s.monthlyLimitUsd > 0) (sp.claudeUsd / s.monthlyLimitUsd).toFloat().coerceIn(0f, 1f) else 1f
        mv.obitech.crundi.ui.operations.PaceBar(frac, 0f, Modifier.padding(vertical = 8.dp))
        Text(sp.turns.toString() + " request" + (if (sp.turns == 1) "" else "s") + " this billing month", style = CrundiType.Meta.copy(color = CrundiColors.TextSecondary))
        if (s.limitReached) Text("The limit is reached: Crundi stops answering until next month or a higher limit.", style = CrundiType.Small.copy(color = CrundiColors.Red), modifier = Modifier.padding(top = 6.dp))
        if (s.nextReset.isNotEmpty()) Text("Claude restarts on ${s.nextReset}" + (if (s.deepgramNextReset.isNotEmpty()) ", Deepgram on ${s.deepgramNextReset}" else "") + ": each on its own billing day.", style = CrundiType.Small, modifier = Modifier.padding(top = 4.dp))
        Row(Modifier.padding(top = 12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiTextField(ui.limitText, ev.onLimitText, Modifier.weight(1f), placeholder = "Monthly limit in dollars", icon = "sliders")
            CrundiButton("Save", ev.onSaveLimit, kind = ButtonKind.Primary, enabled = !ui.busy)
        }
        // Deepgram: its own spend and limit (D46).
        Row(Modifier.padding(top = 14.dp), verticalAlignment = Alignment.Bottom) {
            Text("Deepgram", style = CrundiType.Heading, modifier = Modifier.weight(1f))
            Text("$" + "%.2f".format(Locale.US, sp.deepgramUsd) + " of $" + fmtUsd(s.deepgramLimitUsd), style = CrundiType.Code.copy(color = if (s.deepgramLimitReached) CrundiColors.Red else CrundiColors.TextPrimary))
        }
        mv.obitech.crundi.ui.operations.PaceBar(if (s.deepgramLimitUsd > 0) (sp.deepgramUsd / s.deepgramLimitUsd).toFloat().coerceIn(0f, 1f) else 1f, 0f, Modifier.padding(vertical = 8.dp))
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiTextField(ui.dgLimitText, ev.onDgLimitText, Modifier.weight(1f), placeholder = "Deepgram limit in dollars", icon = "sliders")
            CrundiButton("Save", ev.onSaveDgLimit, enabled = !ui.busy)
        }
        if (s.periodStart.isNotEmpty()) Row(Modifier.padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiTextField(ui.dayText, ev.onDayText, Modifier.weight(1f), placeholder = "Claude API billing day, 1 to 31", icon = "clock")
            CrundiButton("Save", ev.onSaveDay, enabled = !ui.busy)
        }
        if (s.deepgramNextReset.isNotEmpty()) Row(Modifier.padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiTextField(ui.dgDayText, ev.onDgDayText, Modifier.weight(1f), placeholder = "Deepgram billing day, 1 to 31", icon = "clock")
            CrundiButton("Save", ev.onSaveDgDay, enabled = !ui.busy)
        }
    }

    // ─── Memory (D39: the notebook, as on /voice) ───
    ui.memory?.let { MemoryPart(it, ev) }

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

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ScreenTimes(w: WakeUi, ev: VoiceSettingsEvents) {
    CrundiCard(Modifier.fillMaxWidth(), padding = 14.dp) {
        Text("Microphone stays on", style = CrundiType.Heading)
        Text("After the last thing you said or Crundi answered, then it stops recording. Noise in the room doesn't keep it on.", style = CrundiType.Small, modifier = Modifier.padding(top = 4.dp, bottom = 10.dp))
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            for (v in listOf(15, 30, 60, 120, 300)) CrundiChip(if (v < 60) "$v s" else "${v / 60} min", w.micSeconds == v, { if (w.micSeconds != v) ev.onMicSeconds(v) })
        }
        Text("Then the screen stays on", style = CrundiType.Heading, modifier = Modifier.padding(top = 16.dp))
        Text(
            (if (w.supported) "Listening on the phone only for \"Hey Crundi\", which starts again where you left off. Nothing is sent meanwhile, and the Hey Crundi settings below needn't be on. " else "") +
                "After this the phone may sleep as usual, and the voice screen closes.",
            style = CrundiType.Small, modifier = Modifier.padding(top = 4.dp, bottom = 10.dp),
        )
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            for (v in listOf(1, 2, 5, 10)) CrundiChip("$v min", w.screenMinutes == v, { if (w.screenMinutes != v) ev.onScreenMinutes(v) })
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun HeyCrundiPart(w: WakeUi, isAssistant: Boolean, ev: VoiceSettingsEvents) {
    CrundiCard(Modifier.fillMaxWidth(), padding = 4.dp) {
        if (!w.supported) {
            Text("This build of Crundi has no wake word for this phone's processor.", style = CrundiType.Small.copy(color = CrundiColors.Yellow), modifier = Modifier.padding(14.dp))
            return@CrundiCard
        }
        CrundiSwitchRow(
            "In the app", w.inApp, ev.onWakeInApp,
            detail = "While Crundi is open, say \"Hey Crundi\" to open the voice screen. Heard on the phone; nothing is sent until it opens.",
        )
        CrundiSwitchRow(
            "In the background", w.background, ev.onWakeBackground,
            detail = if (w.background) "Listening with the screen off. " + w.left + "." else "Listens with the screen off and the app closed, for as long as you choose.",
        )
        Column(Modifier.padding(start = Dimens.ScreenPad, end = Dimens.ScreenPad, bottom = 12.dp)) {
            Text("How long", style = CrundiType.Small, modifier = Modifier.padding(bottom = 6.dp))
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                for (m in HeyCrundiPlan.CHOICES) CrundiChip(HeyCrundiPlan.label(m), w.minutes == m, { if (w.minutes != m) ev.onWakeMinutes(m) })
            }
            if (w.listening != HeyCrundiPlan.Listener.None) Row(Modifier.padding(top = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                CrundiIcon("mic", size = 14.dp, tint = CrundiColors.Green)
                Text(when (w.listening) { HeyCrundiPlan.Listener.Background -> "Listening now, in the background"; HeyCrundiPlan.Listener.Screen -> "Listening now, on the voice screen"; else -> "Listening now, while Crundi is open" }, style = CrundiType.Small.copy(color = CrundiColors.Green), modifier = Modifier.padding(start = 6.dp))
            }
            Text(
                "Listening uses some battery, and Android shows the green microphone dot while it does. On Samsung, add Crundi to Settings > Battery > Background usage limits > Never sleeping apps, or the phone may stop it; if it does, a notification offers to resume. \"Hey Crundi\" can also turn it on for a while: \"keep listening for an hour\".",
                style = CrundiType.Small, modifier = Modifier.padding(top = 10.dp),
            )
            if (!isAssistant && !w.fullScreen) {
                Text("Heard with the phone locked, Crundi Voice opens over the lock screen only with full-screen notifications allowed; otherwise it shows a notification to tap.", style = CrundiType.Small, modifier = Modifier.padding(top = 8.dp, bottom = 8.dp))
                CrundiButton("Allow full-screen notifications", ev.onFullScreen, icon = "bell")
            }
        }
    }
}

@Composable
private fun MemoryPart(m: MemoryUi, ev: VoiceSettingsEvents) {
    Section("Memory")
    CrundiCard(Modifier.fillMaxWidth(), padding = 14.dp) {
        val got = m.loaded
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                when {
                    m.error.isNotEmpty() -> m.error
                    got == null -> "Loading the memory…"
                    else -> "${got.entries} thing" + (if (got.entries == 1) "" else "s") + " remembered · meaning search: " + got.meaning.ifEmpty { "off" }
                },
                style = CrundiType.Small.copy(color = if (m.error.isNotEmpty()) CrundiColors.Yellow else CrundiColors.TextSecondary),
                modifier = Modifier.weight(1f),
            )
            Box(Modifier.size(36.dp).clip(CircleShape).clickable(onClickLabel = "Reload", onClick = ev.onMemoryReload), contentAlignment = Alignment.Center) {
                CrundiIcon("refresh", size = 16.dp, tint = CrundiColors.TextSecondary)
            }
        }
        CrundiTextField(m.query, ev.onMemoryQuery, Modifier.fillMaxWidth().padding(top = 10.dp), placeholder = "Search what Crundi remembers", icon = "search")
        m.hits?.let { hits ->
            Column(Modifier.padding(top = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                if (hits.isEmpty() && !m.searching) Text("Nothing found.", style = CrundiType.Small)
                for (h in hits) Column(Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgTertiary).padding(10.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        StatusBadge(h.kind.ifEmpty { "memory" }, BadgeKind.Neutral)
                        Text(h.at.take(16).replace('T', ' ') + (h.project?.takeIf { it.isNotEmpty() }?.let { " · $it" } ?: ""), style = CrundiType.Meta)
                    }
                    Text(h.text.take(400), style = CrundiType.Body, modifier = Modifier.padding(top = 4.dp))
                }
            }
        }
    }
    val files = m.loaded?.files.orEmpty()
    if (m.loaded != null && files.isEmpty()) Text("Nothing in the notebook yet. Crundi writes in it as it learns about you.", style = CrundiType.Small, modifier = Modifier.padding(horizontal = 2.dp))
    for (f in files) {
        val draft = m.drafts[f.path]
        CrundiCard(Modifier.fillMaxWidth(), padding = 14.dp) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                CrundiIcon("file-text", size = 14.dp, tint = CrundiColors.TextMuted)
                Text(f.path, style = CrundiType.Code, modifier = Modifier.weight(1f).padding(start = 6.dp))
                Text(if (f.size < 1024) "${f.size} B" else "%.1f KB".format(Locale.US, f.size / 1024.0), style = CrundiType.Meta)
            }
            CrundiTextField(draft ?: f.text, { ev.onMemoryText(f.path, it) }, Modifier.fillMaxWidth().padding(top = 8.dp), mono = true, singleLine = false, minLines = 3, maxLines = 14)
            Row(Modifier.padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                CrundiButton("Save", { ev.onMemorySave(f.path) }, kind = ButtonKind.Primary, icon = "check", enabled = draft != null)
                CrundiButton(if (m.confirmDelete == f.path) "Delete for good?" else "Delete", { ev.onMemoryDelete(f.path) }, kind = ButtonKind.Danger, icon = "trash")
            }
        }
    }
}
