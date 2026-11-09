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
    /**
     * The server's voice for Cru (ttsVoice): "" when each device speaks with
     * its own, a Deepgram voice otherwise; null until the server has said.
     */
    val sharedVoice: String? = null,
    val device: List<DeviceVoice> = emptyList(),
    val deviceChosen: String = "",
    val note: String = "",
    /** "Hey Cru" (Phase 6): this build has the keyword spotter for this phone. */
    val wake: WakeUi = WakeUi(),
)

/** The "Hey Cru" section. */
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
    /** Then the screen stays on, listening for "Hey Cru", this many minutes. */
    val screenMinutes: Int = 2,
    /** "Hey Cru" has been trained on the owner's voice ("6 Nov"); empty until it has. */
    val trainedOn: String = "",
    /** Forget was tapped once: tap again to forget. */
    val confirmForget: Boolean = false,
)

class VoiceSettingsEvents(
    val onBack: () -> Unit = {},
    val onEnabled: (Boolean) -> Unit = {},
    val onAssistantSettings: () -> Unit = {},
    /** Cru's own settings, on the server: the Settings tab's Voice assistant page. */
    val onCruSettings: () -> Unit = {},
    val onPickDevice: (String) -> Unit = {},
    val onPreviewDevice: (String) -> Unit = {},
    val onTry: () -> Unit = {},
    val onWakeInApp: (Boolean) -> Unit = {},
    val onTrain: () -> Unit = {},
    val onForget: () -> Unit = {},
    val onWakeBackground: (Boolean) -> Unit = {},
    val onWakeMinutes: (Int) -> Unit = {},
    val onMicSeconds: (Int) -> Unit = {},
    val onScreenMinutes: (Int) -> Unit = {},
    val onFullScreen: () -> Unit = {},
)

/**
 * App settings > Voice assistant: what belongs to this phone. On or off, how
 * to open it (D33's two modes), the microphone and screen times, "Hey Cru",
 * and the phone's own voice. Cru's settings on the server (brain, model,
 * Deepgram voice, spending, keys, notebook), shared by every device, are in
 * the Settings tab's Voice assistant page (CruSettings.kt), linked from here.
 */
class VoiceSettingsActivity : ComponentActivity() {
    private var ui by mutableStateOf(VoiceSettingsUi())
    private var client: CrundiClient? = null
    private var tts: TextToSpeech? = null
    /** What to do once the microphone (and notifications) are allowed. */
    private var afterGrant: (() -> Unit)? = null
    private val askMic = registerForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { r ->
        HeyCrundi.refresh()
        if (r[Manifest.permission.RECORD_AUDIO] == true || micGranted()) afterGrant?.invoke()
        else ui = ui.copy(note = "Hey Cru needs the microphone. Allow it in Android's settings for Crundi.")
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
        // "Hey Cru stopped, tap to resume": the tap is in front, so the microphone may start again.
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
            trainedOn = WakeProfiles.get(this)?.let { p -> runCatching { java.time.format.DateTimeFormatter.ofPattern("d MMM").format(java.time.Instant.parse(p.trainedAt).atZone(java.time.ZoneId.systemDefault())) }.getOrDefault("yes").ifEmpty { "yes" } }.orEmpty(),
            confirmForget = ui.wake.confirmForget,
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
        tts?.shutdown()
        super.onDestroy()
    }

    private fun voice(): VoiceApi? = client?.voice

    /** The server's voice for Cru, so the phone's own shows as chosen only when it is the one used. Best effort. */
    private fun load() {
        val v = voice() ?: return
        lifecycleScope.launch { runCatching { v.settings() }.getOrNull()?.let { ui = ui.copy(sharedVoice = it.ttsVoice) } }
    }

    private fun events() = VoiceSettingsEvents(
        onBack = { finish() },
        onEnabled = { on ->
            VoicePrefs.setEnabled(this, on)
            // The "Cru" launcher entry comes and goes with it.
            // ...and so does "Cru" in the share sheet.
            for (alias in listOf("CrundiVoice", "ShareToCru")) packageManager.setComponentEnabledSetting(
                ComponentName(this, "$packageName.voice.$alias"),
                if (on) PackageManager.COMPONENT_ENABLED_STATE_DEFAULT else PackageManager.COMPONENT_ENABLED_STATE_DISABLED,
                PackageManager.DONT_KILL_APP,
            )
            ui = ui.copy(enabled = on)
        },
        onAssistantSettings = { openAssistantSettings() },
        onCruSettings = { openCruSettings() },
        onPickDevice = { name ->
            VoicePrefs.setDeviceVoice(this, name)
            ui = ui.copy(deviceChosen = name, note = if (ui.sharedVoice.isNullOrEmpty()) "Cru will speak with this voice on this phone" else "Saved. Cru speaks with this voice once its settings say each device's own voice.")
        },
        onPreviewDevice = { previewDevice(it) },
        onTry = { startActivity(Intent(this, VoiceActivity::class.java)) },
        onMicSeconds = { v -> VoicePrefs.setSilenceSeconds(this, v); wakeRefresh() },
        onScreenMinutes = { v -> VoicePrefs.setScreenAwakeMinutes(this, v); wakeRefresh() },
        onTrain = { startActivity(Intent(this, WakeTrainActivity::class.java)) },
        onForget = {
            if (!ui.wake.confirmForget) { ui = ui.copy(wake = ui.wake.copy(confirmForget = true)); lifecycleScope.launch { delay(4000); ui = ui.copy(wake = ui.wake.copy(confirmForget = false)) } }
            else {
                WakeProfiles.forget(this); HeyCrundiService.stop(this); HeyCrundi.setInApp(false); HeyCrundi.profileChanged()
                ui = ui.copy(wake = ui.wake.copy(confirmForget = false), note = "Hey Cru forgot your voice. Train it again to use it.")
                wakeRefresh()
            }
        },
        onWakeInApp = { on ->
            if (on) withMic { HeyCrundi.setInApp(true); ui = ui.copy(note = "Say \"Hey Cru\" while Crundi is open"); wakeRefresh() }
            else { HeyCrundi.setInApp(false); wakeRefresh() }
        },
        onWakeBackground = { on ->
            if (on) withMic {
                val m = VoicePrefs.wakeMinutes(this)
                HeyCrundiService.start(this, m)
                ui = ui.copy(note = if (m == HeyCrundiPlan.UNTIL_OFF) "Listening for \"Hey Cru\" until you turn it off" else "Listening for \"Hey Cru\" for ${HeyCrundiPlan.label(m)}")
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
    )

    private fun previewDevice(name: String) {
        val say = { t: TextToSpeech ->
            t.voices?.firstOrNull { it.name == name }?.let { t.setVoice(it) } ?: t.setLanguage(Locale.UK)
            t.speak(SAMPLE, TextToSpeech.QUEUE_FLUSH, null, "preview")
        }
        tts?.let { say(it); return }
        var made: TextToSpeech? = null
        made = TextToSpeech(this) { st -> if (st == TextToSpeech.SUCCESS) made?.let { tts = it; say(it) } }
    }

    /** The Settings tab's Voice assistant page, in the main window, on this server. */
    private fun openCruSettings() {
        val id = Servers.current(this)?.id.orEmpty()
        startActivity(
            Intent(this, mv.obitech.crundi.MainActivity::class.java).setAction("open:$id:/settings/voice")
                .putExtra(mv.obitech.crundi.Notifier.EXTRA_SERVER, id).putExtra(mv.obitech.crundi.Notifier.EXTRA_PATH, "/settings/voice")
                .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP),
        )
    }

    private fun openAssistantSettings() {
        val tries = listOf(Intent(Settings.ACTION_MANAGE_DEFAULT_APPS_SETTINGS), Intent(Settings.ACTION_VOICE_INPUT_SETTINGS), Intent(Settings.ACTION_SETTINGS))
        for (i in tries) if (runCatching { startActivity(i) }.isSuccess) return
    }

    companion object {
        const val SAMPLE = "Good evening, Sir. Everything is running smoothly, and the kanban board is in order."
        const val RECOMMENDED = "aura-2-draco-en"

        fun holdsAssistant(ctx: Context): Boolean =
            if (Build.VERSION.SDK_INT >= 29) runCatching { ctx.getSystemService(RoleManager::class.java)?.isRoleHeld(RoleManager.ROLE_ASSISTANT) == true }.getOrDefault(false)
            else false
    }
}

/** Dollars as shown: "50", "12.50". */
fun fmtUsd(v: Double): String = if (v >= 100 || v == Math.floor(v)) "%.0f".format(Locale.US, v) else "%.2f".format(Locale.US, v)

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun VoiceSettingsScreen(ui: VoiceSettingsUi, ev: VoiceSettingsEvents) {
    Column(Modifier.fillMaxSize()) {
        TopBar("Voice assistant", onBack = ev.onBack, subtitle = ui.host.takeIf { it.isNotEmpty() }?.let { "Crundi on $it" })
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(Dimens.ScreenPad), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            CrundiCard(Modifier.fillMaxWidth(), padding = 4.dp) {
                CrundiSwitchRow("Voice assistant", ui.enabled, ev.onEnabled, detail = if (ui.enabled) "On. Ask Cru anything about your projects, by voice." else "Off. The side key and Cru do nothing.")
            }
            if (ui.note.isNotEmpty()) StatusBadge(ui.note, BadgeKind.Info)
            Text("These settings are for this phone only.", style = CrundiType.Small, modifier = Modifier.padding(horizontal = 2.dp))

            // ─── Cru's own settings: on the server, the same on every device ───
            CrundiCard(Modifier.fillMaxWidth(), onClick = ev.onCruSettings, padding = 14.dp) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    CrundiIcon("sliders", size = Dimens.Icon, tint = CrundiColors.AccentHover)
                    Column(Modifier.weight(1f).padding(start = 12.dp)) {
                        Text("Cru's settings (shared by every device)", style = CrundiType.Heading)
                        Text("Brain, model, voice, talking speed, spending, keys and notebook. In Settings > Voice assistant.", style = CrundiType.Small, modifier = Modifier.padding(top = 2.dp))
                    }
                    CrundiIcon("chevron-right", size = Dimens.IconSm, tint = CrundiColors.TextMuted)
                }
            }

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
                "Gemini stays the phone's assistant. A double press of the side key opens Cru directly; Gemini isn't involved. Works after unlocking.",
                if (ui.isAssistant) "Gemini is not the assistant now" else "Available",
                null,
                steps = "On Samsung: Advanced features > Side button > Double press > Open app > Cru.",
            ) { CrundiButton("Try Cru", ev.onTry, icon = "mic") }

            // ─── Winding down: the microphone, then "Hey Cru", then sleep ───
            Section("On the voice screen")
            ScreenTimes(ui.wake, ev)

            // ─── "Hey Cru" (Phase 6, D3–D7) ───
            Section("Hey Cru")
            HeyCrundiPart(ui.wake, ui.isAssistant, ev)

            // ─── The phone's own voice (D34): used when Cru's settings say each device's own ───
            Section("This phone's voice")
            PhoneVoices(ui, ev)
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

@Composable
private fun PhoneVoices(ui: VoiceSettingsUi, ev: VoiceSettingsEvents) {
    CrundiCard(Modifier.fillMaxWidth(), padding = 6.dp) {
        Text(
            if (ui.sharedVoice.isNullOrEmpty()) "Cru speaks with the phone's own text-to-speech, in the voice you pick here."
            else "Cru speaks with a Deepgram voice now. The voice picked here is used when Cru's settings say each device's own voice, or when Deepgram is not available.",
            style = CrundiType.Small, modifier = Modifier.padding(8.dp),
        )
        val gb = ui.device.filter { it.locale.equals("en-GB", true) }
        val picked = DeviceVoices.pick(ui.device, ui.deviceChosen)?.name
        if (gb.isEmpty()) Text("The phone's text-to-speech has no British English voice.", style = CrundiType.Small, modifier = Modifier.padding(8.dp))
        for (v in gb) {
            VoiceRow(
                v.name + if (v.name == DeviceVoices.PREFERRED) "  ·  default" else "",
                listOf(v.locale, if (v.needsNetwork) "needs the network" else "on the phone", if (!v.installed) "not downloaded" else "").filter { it.isNotEmpty() }.joinToString(" · "),
                v.name == picked,
                onPick = { ev.onPickDevice(v.name) }, onPreview = { ev.onPreviewDevice(v.name) },
            )
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ScreenTimes(w: WakeUi, ev: VoiceSettingsEvents) {
    CrundiCard(Modifier.fillMaxWidth(), padding = 14.dp) {
        Text("Microphone stays on", style = CrundiType.Heading)
        Text("After the last thing you said or Cru answered, then it stops recording. Noise in the room doesn't keep it on.", style = CrundiType.Small, modifier = Modifier.padding(top = 4.dp, bottom = 10.dp))
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            for (v in listOf(15, 30, 60, 120, 300)) CrundiChip(if (v < 60) "$v s" else "${v / 60} min", w.micSeconds == v, { if (w.micSeconds != v) ev.onMicSeconds(v) })
        }
        Text("Then the screen stays on", style = CrundiType.Heading, modifier = Modifier.padding(top = 16.dp))
        Text(
            (if (w.supported && w.trainedOn.isNotEmpty()) "Listening on the phone only for \"Hey Cru\", which starts again where you left off. Nothing is sent meanwhile, and the Hey Cru switches below needn't be on. " else if (w.supported) "Once Hey Cru is trained (below), it listens for it meanwhile. " else "") +
                "After this the phone may sleep as usual. When the voice screen closes is one of Cru's settings, the same on every device.",
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
        // Trained first (the owner's own voice, five times), then the switches.
        if (w.trainedOn.isEmpty()) {
            Column(Modifier.padding(14.dp)) {
                Text("Train it first", style = CrundiType.Heading)
                Text("Say \"Hey Cru\" five times so it learns your voice: it wakes for you, and not for \"crew\" on the television. Takes a minute; the recordings stay on this phone.", style = CrundiType.Small, modifier = Modifier.padding(top = 4.dp, bottom = 12.dp))
                CrundiButton("Train Hey Cru", ev.onTrain, kind = ButtonKind.Primary, icon = "mic")
            }
            return@CrundiCard
        }
        Row(Modifier.fillMaxWidth().padding(start = 14.dp, end = 6.dp, top = 10.dp, bottom = 2.dp), verticalAlignment = Alignment.CenterVertically) {
            CrundiIcon("check", size = 14.dp, tint = CrundiColors.Green)
            Text("Trained on your voice, ${w.trainedOn}", style = CrundiType.Small.copy(color = CrundiColors.Green), modifier = Modifier.weight(1f).padding(start = 6.dp))
            CrundiButton("Train again", ev.onTrain, kind = ButtonKind.Ghost)
            CrundiButton(if (w.confirmForget) "Tap to forget" else "Forget", ev.onForget, kind = if (w.confirmForget) ButtonKind.Danger else ButtonKind.Ghost)
        }
        CrundiSwitchRow(
            "In the app", w.inApp, ev.onWakeInApp,
            detail = "While Crundi is open, say \"Hey Cru\" to open the voice screen. Heard on the phone; nothing is sent until it opens.",
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
                "Listening uses some battery, and Android shows the green microphone dot while it does. On Samsung, add Crundi to Settings > Battery > Background usage limits > Never sleeping apps, or the phone may stop it; if it does, a notification offers to resume. \"Hey Cru\" can also turn it on for a while: \"keep listening for an hour\".",
                style = CrundiType.Small, modifier = Modifier.padding(top = 10.dp),
            )
            if (!isAssistant && !w.fullScreen) {
                Text("Heard with the phone locked, the voice screen opens over the lock screen only with full-screen notifications allowed; otherwise it shows a notification to tap.", style = CrundiType.Small, modifier = Modifier.padding(top = 8.dp, bottom = 8.dp))
                CrundiButton("Allow full-screen notifications", ev.onFullScreen, icon = "bell")
            }
        }
    }
}
