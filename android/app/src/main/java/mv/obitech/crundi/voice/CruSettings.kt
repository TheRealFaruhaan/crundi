package mv.obitech.crundi.voice

import android.media.MediaPlayer
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalInspectionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import mv.obitech.crundi.api.BrainStats
import mv.obitech.crundi.api.DeepgramVoice
import mv.obitech.crundi.api.MemoryHit
import mv.obitech.crundi.api.VoiceApi
import mv.obitech.crundi.api.VoiceMemory
import mv.obitech.crundi.api.VoiceSettings
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiCard
import mv.obitech.crundi.ui.components.CrundiChip
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.StatusBadge
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.operations.Actions
import mv.obitech.crundi.ui.operations.SettingsPage
import mv.obitech.crundi.ui.operations.SettingsPageFrame
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens
import java.io.File
import java.util.Locale

/**
 * Settings > Voice assistant: Cru's settings on the server, shared by every
 * device (the phone, the web, the PC). What belongs to one phone (on or off,
 * the side key, the microphone and screen times, "Hey Cru", the phone's own
 * voice) stays in that phone's Voice settings (VoiceSettingsActivity).
 *
 * A newer app on an older server: whatever the server does not report is
 * left out (no brain, no screen-close time, no Claude Code idle time, no
 * brain stats), and a server without the assistant says so.
 */
data class CruSettingsUi(
    val host: String = "",
    /** The server's capabilities, for the sections an older server has no part in. */
    val caps: Set<String> = emptySet(),
    /** Null while loading; [error] says why it isn't there. */
    val server: VoiceSettings? = null,
    val error: String = "",
    val deepgram: List<DeepgramVoice> = emptyList(),
    val limitText: String = "",
    val dayText: String = "",
    val dgDayText: String = "",
    val dgLimitText: String = "",
    /** What is typed for each key (VoiceApi.KEYS); never filled from the server. */
    val keyText: Map<String, String> = emptyMap(),
    val busy: Boolean = false,
    /** The notebook (capability "voice-memory"); null when the server has none. */
    val memory: MemoryUi? = null,
    /** The brains compared; null when the server has none to compare. */
    val stats: BrainStats? = null,
    /** Cru's tickers and triggers (capability "voice-wakeups"); null when the server has none. */
    val wakeups: WakeupsUi? = null,
    /** "Your voice" (capability "voice-speaker"); null when the server has none. */
    val speaker: SpeakerUi? = null,
)

/** Tickers and triggers as the server has them, and the one whose Delete was tapped once. */
data class WakeupsUi(
    val loaded: mv.obitech.crundi.api.VoiceWakeups? = null,
    val error: String = "",
    val confirmDelete: String = "",
    /** The ids being changed now. */
    val busy: Set<String> = emptySet(),
)

/** The notebook: files as the server has them, the owner's edits, the search. */
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

class CruSettingsEvents(
    val onBack: () -> Unit = {},
    val onCloseNote: () -> Unit = {},
    val onPickDeepgram: (String) -> Unit = {},
    /** Each device speaks with its own voice (ttsVoice ""). */
    val onDeviceVoice: () -> Unit = {},
    val onPreviewDeepgram: (String) -> Unit = {},
    val onModel: (String) -> Unit = {},
    val onBrain: (String) -> Unit = {},
    val onCliIdle: (Int) -> Unit = {},
    val onScreenClose: (Int) -> Unit = {},
    val onLimitText: (String) -> Unit = {},
    val onSaveLimit: () -> Unit = {},
    val onDayText: (String) -> Unit = {},
    val onSaveDay: () -> Unit = {},
    val onDgDayText: (String) -> Unit = {},
    val onSaveDgDay: () -> Unit = {},
    val onDgLimitText: (String) -> Unit = {},
    val onSaveDgLimit: () -> Unit = {},
    val onPause: (Int) -> Unit = {},
    val onRate: (Double) -> Unit = {},
    val onVolume: (Int) -> Unit = {},
    val onAdminMode: (Boolean) -> Unit = {},
    /** A ticker or trigger switched on (true) or off. */
    val onWakeupSwitch: (String, Boolean) -> Unit = { _, _ -> },
    /** Delete tapped: once asks, twice deletes. */
    val onWakeupDelete: (String) -> Unit = {},
    val speaker: SpeakerEvents = SpeakerEvents(),
    val onKeyText: (String, String) -> Unit = { _, _ -> },
    val onSaveKey: (String) -> Unit = {},
    val onReset: () -> Unit = {},
    val onMemoryText: (String, String) -> Unit = { _, _ -> },
    val onMemorySave: (String) -> Unit = {},
    val onMemoryDelete: (String) -> Unit = {},
    val onMemoryQuery: (String) -> Unit = {},
    val onMemoryReload: () -> Unit = {},
)

/** The sections, each folded until tapped. */
object CruSection {
    const val BRAIN = "brain"
    const val MODEL = "model"
    const val VOICE = "voice"
    const val SPEED = "speed"
    const val VOLUME = "volume"
    const val ADMIN = "admin"
    const val WAKEUPS = "wakeups"
    const val SPEAKER = "speaker"
    const val LISTENING = "listening"
    const val SCREEN = "screen"
    const val SPENDING = "spending"
    const val KEYS = "keys"
    const val MEMORY = "memory"
    const val STATS = "stats"
}

@Composable
fun CruSettingsRoute() {
    val app = LocalAppServices.current
    val nav = LocalNavigator.current
    val ctx = LocalContext.current
    val inspecting = LocalInspectionMode.current
    val shell by app.shell.state.collectAsStateWithLifecycle()
    val v = app.api.voice
    val scope = rememberCoroutineScope()
    var ui by remember { mutableStateOf(CruSettingsUi(host = shell.serverHost, caps = shell.capabilities.toSet())) }
    LaunchedEffect(shell.capabilities) { ui = ui.copy(caps = shell.capabilities.toSet()) }
    var note by remember { mutableStateOf<Actions.Note?>(null) }
    var search by remember { mutableStateOf<Job?>(null) }
    val preview = remember { arrayOfNulls<MediaPlayer>(1) }
    DisposableEffect(Unit) { onDispose { preview[0]?.release() } }
    LaunchedEffect(note) { if (note?.error == false) { delay(4000); note = null } }
    val hasMemory = VoiceApi.MEMORY in shell.capabilities
    val hasWakeups = VoiceApi.WAKEUPS in shell.capabilities
    val hasSpeaker = VoiceApi.SPEAKER in shell.capabilities
    // Enrolling: the phone's microphone at 16 kHz, as the voice screen hears it.
    val rec = remember { arrayOfNulls<MicStream>(1) }
    val recBuf = remember { java.io.ByteArrayOutputStream() }
    var recLevel by remember { mutableStateOf(0f) }
    DisposableEffect(Unit) { onDispose { rec[0]?.stop(); rec[0] = null; HeyCrundi.training(false) } }

    fun say(text: String, error: Boolean = false) { note = Note(text, error) }
    fun withServer(s: VoiceSettings) = ui.copy(server = s, limitText = fmtUsd(s.monthlyLimitUsd), dayText = s.billingDay.toString(), dgDayText = s.deepgramBillingDay.toString(), dgLimitText = fmtUsd(s.deepgramLimitUsd))
    fun memory(change: (MemoryUi) -> MemoryUi) { ui.memory?.let { ui = ui.copy(memory = change(it)) } }
    fun loadStats() { scope.launch { ui = ui.copy(stats = runCatching { v.brainStats() }.getOrNull()) } }
    fun loadMemory() {
        if (!hasMemory) { ui = ui.copy(memory = null); return }
        scope.launch {
            val m = ui.memory ?: MemoryUi()
            ui = try {
                val got = v.memory()
                ui.copy(memory = m.copy(loaded = got, error = "", drafts = m.drafts.filter { (p, t) -> got.files.any { it.path == p && it.text != t } }))
            } catch (e: Exception) {
                ui.copy(memory = m.copy(error = if (VoiceApi.unsupported(e)) "This server has no voice memory yet. Update the server." else e.message ?: "Could not load the memory"))
            }
        }
    }
    fun saveMemory(path: String, text: String, done: String) {
        scope.launch {
            try {
                v.saveMemory(path, text)
                memory { it.copy(drafts = it.drafts - path, confirmDelete = "") }
                say(done)
                loadMemory()
            } catch (e: Exception) { say(e.message ?: "Not saved", true) }
        }
    }
    fun wakeups(change: (WakeupsUi) -> WakeupsUi) { ui.wakeups?.let { ui = ui.copy(wakeups = change(it)) } }
    fun loadWakeups() {
        if (!hasWakeups) { ui = ui.copy(wakeups = null); return }
        scope.launch {
            val w = ui.wakeups ?: WakeupsUi()
            ui = try {
                val got = v.wakeups()
                if (got == null) ui.copy(wakeups = null) else ui.copy(wakeups = (ui.wakeups ?: w).copy(loaded = got, error = ""))
            } catch (e: Exception) { ui.copy(wakeups = w.copy(error = e.message ?: "Could not load the tickers and triggers")) }
        }
    }
    fun changeWakeup(id: String, action: String, done: String) {
        wakeups { it.copy(busy = it.busy + id, confirmDelete = "") }
        scope.launch {
            try {
                val got = v.wakeup(id, action)
                wakeups { it.copy(loaded = got, busy = it.busy - id, error = "") }
                say(done)
            } catch (e: Exception) {
                wakeups { it.copy(busy = it.busy - id) }
                say(e.message ?: "Not changed", true)
            }
        }
    }
    fun spk(change: (SpeakerUi) -> SpeakerUi) { ui.speaker?.let { ui = ui.copy(speaker = change(it)) } }
    fun loadSpeaker() {
        if (!hasSpeaker) { ui = ui.copy(speaker = null); return }
        scope.launch {
            val cur = ui.speaker ?: SpeakerUi()
            ui = try {
                val got = v.speaker()
                if (got == null) ui.copy(speaker = null) else ui.copy(speaker = (ui.speaker ?: cur).copy(status = got, error = ""))
            } catch (e: Exception) { ui.copy(speaker = cur.copy(error = e.message ?: "Could not load your voice")) }
        }
    }
    fun endEnrol(msg: String, bad: Boolean) {
        rec[0]?.stop(); rec[0] = null
        HeyCrundi.training(false)
        spk { it.copy(active = false, recording = false, busy = false, msg = msg, bad = bad) }
    }
    fun stopRecording() {
        val m = rec[0] ?: return
        m.stop(); rec[0] = null
        val pcm = synchronized(recBuf) { recBuf.toByteArray() }
        val restart = ui.speaker?.let { Enrolment.restart(it) } ?: true
        spk { it.copy(recording = false, busy = true, msg = "Checking…", bad = false) }
        scope.launch {
            val r = runCatching { v.enrolPhrase(pcm, Enrolment.RATE, restart) }.getOrElse { mv.obitech.crundi.api.EnrolReply(ok = false, error = it.message ?: "Could not reach the server") }
            if (ui.speaker?.active == true) spk { Enrolment.after(it, r) }
        }
    }
    fun startRecording() {
        if (androidx.core.content.ContextCompat.checkSelfPermission(ctx, android.Manifest.permission.RECORD_AUDIO) != android.content.pm.PackageManager.PERMISSION_GRANTED) {
            spk { it.copy(msg = "Allow the microphone for Crundi (Android settings, Apps, Crundi, Permissions), then try again.", bad = true) }
            return
        }
        synchronized(recBuf) { recBuf.reset() }
        val m = MicStream(Enrolment.RATE) { bytes, level ->
            recLevel = level
            val full = synchronized(recBuf) { recBuf.write(bytes); recBuf.size() >= Enrolment.RATE * 2 * Enrolment.MAX_SECONDS }
            if (full) scope.launch { stopRecording() }
        }
        if (!m.start()) { spk { it.copy(msg = "The microphone could not be opened. Is another app using it?", bad = true) }; return }
        rec[0] = m
        spk { it.copy(recording = true, msg = "", bad = false) }
    }
    fun save(done: String, call: suspend () -> VoiceSettings) {
        ui = ui.copy(busy = true)
        scope.launch {
            try { ui = withServer(call()).copy(busy = false); say(done) }
            catch (e: Exception) { ui = ui.copy(busy = false); say(e.message ?: "Could not save", true) }
        }
    }

    LaunchedEffect(Unit) {
        if (inspecting) return@LaunchedEffect
        try {
            ui = withServer(v.settings()).copy(error = "")
            runCatching { v.voices() }.getOrNull()?.let { ui = ui.copy(deepgram = it.voices) }
            if (ui.server?.brain?.isNotEmpty() == true) loadStats()
            loadMemory()
            loadWakeups()
            loadSpeaker()
        } catch (e: Exception) {
            ui = ui.copy(error = if (VoiceApi.unsupported(e)) VoiceApi.UNSUPPORTED else e.message ?: "Could not load the settings")
        }
    }
    // Cru made, changed or fired one ('voice-wakeups' on the event stream): the list again.
    LaunchedEffect(hasWakeups) {
        if (inspecting || !hasWakeups) return@LaunchedEffect
        app.api.events.changes("voice-wakeups").collect { loadWakeups() }
    }

    CruSettingsScreen(ui.speaker?.let { sp -> if (sp.recording) ui.copy(speaker = sp.copy(level = recLevel)) else ui } ?: ui, note, CruSettingsEvents(
        onBack = { nav.back() },
        onCloseNote = { note = null },
        onPickDeepgram = { save("Cru will speak with this voice") { v.save(ttsVoice = it) } },
        onDeviceVoice = { save("Each device speaks with its own voice") { v.save(ttsVoice = "") } },
        onPreviewDeepgram = { model ->
            scope.launch {
                try {
                    val mp3 = v.speak(VoiceSettingsActivity.SAMPLE, model)
                    val f = withContext(Dispatchers.IO) { File(ctx.cacheDir, "voice-preview.mp3").apply { writeBytes(mp3) } }
                    preview[0]?.release()
                    preview[0] = MediaPlayer().apply { setDataSource(f.path); setOnPreparedListener { it.start() }; prepareAsync() }
                } catch (e: Exception) { say(e.message ?: "Could not play it", true) }
            }
        },
        onModel = { save("Model changed. It applies from the next conversation.") { v.save(model = it) } },
        onBrain = { b -> save(if (b == "claude-code") "Cru answers through your Claude Code now" else "Cru answers through the Claude API now") { v.save(brain = b) }; loadStats() },
        onCliIdle = { m -> save("Claude Code stops after $m minutes idle") { v.save(cliIdleMinutes = m) } },
        onScreenClose = { sec -> save("The voice screen closes after ${closeLabel(sec)} of silence") { v.save(screenCloseSeconds = sec) } },
        onLimitText = { ui = ui.copy(limitText = it) },
        onSaveLimit = {
            val n = ui.limitText.trim().removePrefix("$").toDoubleOrNull()
            if (n == null || n < 0) say("The limit is a number of dollars", true) else save("Limit saved") { v.save(monthlyLimitUsd = n) }
        },
        onDayText = { ui = ui.copy(dayText = it) },
        onSaveDay = {
            val d = ui.dayText.trim().toIntOrNull()
            if (d == null || d !in 1..31) say("The billing day is a day of the month, 1 to 31", true) else save("Billing day saved") { v.save(billingDay = d) }
        },
        onDgDayText = { ui = ui.copy(dgDayText = it) },
        onSaveDgDay = {
            val d = ui.dgDayText.trim().toIntOrNull()
            if (d == null || d !in 1..31) say("The billing day is a day of the month, 1 to 31", true) else save("Deepgram billing day saved") { v.save(deepgramBillingDay = d) }
        },
        onDgLimitText = { ui = ui.copy(dgLimitText = it) },
        onSaveDgLimit = {
            val n = ui.dgLimitText.trim().removePrefix("$").toDoubleOrNull()
            if (n == null || n < 0) say("The limit is a number of dollars", true) else save("Deepgram limit saved") { v.save(deepgramLimitUsd = n) }
        },
        onPause = { ms -> save("Cru waits ${ms / 1000.0} s of quiet before sending") { v.save(pauseMs = ms) } },
        onRate = { r -> save("Cru talks at ${rateLabel(r)}") { v.save(speechRate = r) } },
        onVolume = { n -> save("Cru talks at $n%") { v.save(volume = n) } },
        onAdminMode = { on -> save(if (on) "Admin mode is on" else "Admin mode is off") { v.save(adminMode = on) } },
        onWakeupSwitch = { id, on -> changeWakeup(id, if (on) "enable" else "disable", if (on) "Switched on" else "Switched off") },
        speaker = SpeakerEvents(
            onStart = { HeyCrundi.training(true); spk { Enrolment.start(it) } },
            onRecord = { if (rec[0] != null) stopRecording() else if (ui.speaker?.busy != true) startRecording() },
            onFinish = {
                spk { it.copy(busy = true) }
                scope.launch {
                    try {
                        val st = v.finishEnrol()
                        spk { it.copy(status = (it.status ?: st).copy(enrolled = st.enrolled || st.phrases > 0, phrases = st.phrases, seconds = st.seconds, enrolledAt = st.enrolledAt ?: it.status?.enrolledAt)) }
                        endEnrol("Cru knows your voice now.", false)
                        loadSpeaker()
                    } catch (e: Exception) { spk { it.copy(busy = false, msg = e.message ?: "Not saved", bad = true) } }
                }
            },
            onCancel = { endEnrol("Enrolment cancelled: nothing changed.", false) },
            onForget = {
                if (ui.speaker?.confirmForget != true) spk { it.copy(confirmForget = true) }
                else scope.launch {
                    try { v.forgetSpeaker(); spk { it.copy(confirmForget = false, msg = "Your voice is forgotten.", bad = false) }; loadSpeaker() }
                    catch (e: Exception) { spk { it.copy(confirmForget = false, msg = e.message ?: "Not changed", bad = true) } }
                }
            },
            onOwnerOnly = { on ->
                scope.launch {
                    try { v.save(ownerOnly = on); spk { it.copy(status = it.status?.copy(ownerOnly = on)) }; say(if (on) "Cru acts on your voice only" else "Cru acts on any voice") }
                    catch (e: Exception) { say(e.message ?: "Not saved", true) }
                }
            },
        ),
        onWakeupDelete = { id ->
            if (ui.wakeups?.confirmDelete == id) changeWakeup(id, "delete", "Deleted")
            else wakeups { it.copy(confirmDelete = id) }
        },
        onKeyText = { k, t -> ui = ui.copy(keyText = ui.keyText + (k to t)) },
        onSaveKey = { k ->
            val t = ui.keyText[k].orEmpty().trim()
            if (t.isNotEmpty()) save("Key saved") { v.saveKey(k, t).also { ui = ui.copy(keyText = ui.keyText - k) } }
        },
        onReset = { scope.launch { runCatching { v.reset() }.onSuccess { say("Crundi starts a fresh conversation") }.onFailure { say(it.message ?: "Could not reset", true) } } },
        onMemoryText = { path, text ->
            val orig = ui.memory?.loaded?.files?.firstOrNull { it.path == path }?.text
            memory { it.copy(drafts = if (text == orig) it.drafts - path else it.drafts + (path to text), confirmDelete = "") }
        },
        onMemorySave = { path -> ui.memory?.drafts?.get(path)?.let { saveMemory(path, it, "Saved $path") } },
        onMemoryDelete = { path ->
            if (ui.memory?.confirmDelete == path) saveMemory(path, "", "Deleted $path")
            else memory { it.copy(confirmDelete = path) }
        },
        onMemoryQuery = { q ->
            memory { it.copy(query = q) }
            search?.cancel()
            if (q.isBlank()) memory { it.copy(hits = null, searching = false) }
            else search = scope.launch {
                delay(350)
                memory { it.copy(searching = true) }
                val hits = runCatching { v.searchMemory(q.trim()) }.getOrElse { e -> say(e.message ?: "Search failed", true); emptyList() }
                memory { it.copy(hits = hits, searching = false) }
            }
        },
        onMemoryReload = { loadMemory() },
    ))
}

private fun Note(text: String, error: Boolean) = Actions.Note(text, error)

/** [open]: sections shown unfolded at first (screenshots); otherwise all start folded. */
@Composable
fun CruSettingsScreen(ui: CruSettingsUi, note: Actions.Note? = null, ev: CruSettingsEvents = CruSettingsEvents(), open: Set<String> = emptySet()) {
    SettingsPageFrame(SettingsPage.Voice, note, ev.onCloseNote, ev.onBack) {
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(Dimens.ScreenPad), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text("Cru's settings live on the server and are the same on every device: this phone, the web and the PC. What belongs to one phone is in that phone's App settings > Voice assistant.", style = CrundiType.Small, modifier = Modifier.padding(horizontal = 2.dp))
            val s = ui.server
            when {
                ui.error.isNotEmpty() -> CrundiCard(Modifier.fillMaxWidth()) { Text(ui.error, style = CrundiType.Body.copy(color = CrundiColors.Yellow)) }
                s == null -> Text("Loading Cru's settings…", style = CrundiType.Small)
                else -> CruSections(ui, s, ev, open)
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun CruSections(ui: CruSettingsUi, s: VoiceSettings, ev: CruSettingsEvents, open: Set<String>) {
    val claudeCode = s.brain.isNotEmpty()

    // ─── Brain (an experiment): the Claude API, or the owner's own Claude Code ───
    if (claudeCode) Fold(CruSection.BRAIN, "Brain", if (s.brain == "claude-code") "Claude Code" else "Claude API", open) {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            for ((id, label) in listOf("api" to "Claude API", "claude-code" to "Claude Code")) CrundiChip(label, s.brain == id, { if (s.brain != id) ev.onBrain(id) })
        }
        Text("An experiment. Claude Code runs on your subscription instead of the API key, with the same tools and notebook. Each keeps its own conversation. Brains compared, below, shows their speed and cost.", style = CrundiType.Small, modifier = Modifier.padding(top = 8.dp))
        if (s.cliIdleMinutes > 0) {
            Text("Stop Claude Code after", style = CrundiType.Heading, modifier = Modifier.padding(top = 16.dp))
            Text("This long without a turn. Your next message starts it again and carries on the same conversation.", style = CrundiType.Small, modifier = Modifier.padding(top = 4.dp, bottom = 10.dp))
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                for (m in IDLE_CHOICES) CrundiChip("$m min", s.cliIdleMinutes == m, { if (s.cliIdleMinutes != m) ev.onCliIdle(m) })
            }
            if (s.cliIdleMinutes !in IDLE_CHOICES) Text("Now ${s.cliIdleMinutes} min", style = CrundiType.Small, modifier = Modifier.padding(top = 8.dp))
        }
    }

    // ─── Model (D8) ───
    Fold(CruSection.MODEL, "Model", s.models.firstOrNull { it.id == s.model }?.label ?: s.model, open) {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            for (m in s.models) CrundiChip(m.label, m.id == s.model, { if (m.id != s.model) ev.onModel(m.id) })
        }
        Text("Haiku is the quickest and cheapest. A change starts a fresh conversation's cache.", style = CrundiType.Small, modifier = Modifier.padding(top = 8.dp))
    }

    // ─── The voice (D19, D34) ───
    val chosen = ui.deepgram.firstOrNull { it.model == s.ttsVoice }?.name ?: s.ttsVoice.ifEmpty { "Each device's own" }
    Fold(CruSection.VOICE, "Voice", chosen, open, padding = 6.dp) {
        val british = ui.deepgram.filter { it.accent.contains("Brit", true) }
        val others = ui.deepgram.filter { !it.accent.contains("Brit", true) }
        if (ui.deepgram.isEmpty()) Text(if (s.deepgramKeySet) "Loading Deepgram's voices…" else "No Deepgram key on the server: each device uses its own voice.", style = CrundiType.Small, modifier = Modifier.padding(8.dp))
        for (v in british + others.take(12)) {
            VoiceRow(
                v.name + if (v.model == VoiceSettingsActivity.RECOMMENDED) "  ·  recommended" else "",
                listOf(v.accent, v.gender, v.tags.filterNot { it.equals(v.gender, true) || it == "masculine" || it == "feminine" }.take(3).joinToString(", ")).filter { it.isNotEmpty() }.joinToString(" · "),
                s.ttsVoice == v.model,
                onPick = { ev.onPickDeepgram(v.model) }, onPreview = { ev.onPreviewDeepgram(v.model) },
            )
        }
        VoiceRow("Each device's own voice", "The phone's text-to-speech, or the browser's. Each phone picks its own in its Voice settings.", s.ttsVoice.isEmpty(), onPick = { if (s.ttsVoice.isNotEmpty()) ev.onDeviceVoice() }, onPreview = null)
    }

    // ─── Talking speed (Deepgram's speed; the phone's voice follows) ───
    Fold(CruSection.SPEED, "Talking speed", rateLabel(s.speechRate), open) {
        Text("The pace of its voice, not how much it says. You can also ask Cru: \"talk faster\".", style = CrundiType.Small, modifier = Modifier.padding(bottom = 10.dp))
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            for (r in RATE_CHOICES) CrundiChip(rateLabel(r), kotlin.math.abs(s.speechRate - r) < 0.01, { if (kotlin.math.abs(s.speechRate - r) >= 0.01) ev.onRate(r) })
        }
        if (RATE_CHOICES.none { kotlin.math.abs(s.speechRate - it) < 0.01 }) Text("Now ${rateLabel(s.speechRate)}", style = CrundiType.Small, modifier = Modifier.padding(top = 8.dp))
    }

    // ─── Volume (capability "voice-volume"): every reply and cue, on every device ───
    if (VoiceApi.VOLUME in ui.caps) Fold(CruSection.VOLUME, "Volume", "${s.volume}%", open) {
        Text("How loud Cru talks, on every device. You can also ask Cru: \"speak up\" or \"quieter\".", style = CrundiType.Small, modifier = Modifier.padding(bottom = 10.dp))
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            for (n in VOLUME_CHOICES) CrundiChip("$n%", s.volume == n, { if (s.volume != n) ev.onVolume(n) })
        }
        if (s.volume !in VOLUME_CHOICES) Text("Now ${s.volume}%", style = CrundiType.Small, modifier = Modifier.padding(top = 8.dp))
    }

    // ─── Pause before sending (D44) ───
    Fold(CruSection.LISTENING, "Pause before sending", pauseLabel(s.pauseMs), open) {
        Text("How long you can pause before what you said is sent. Longer lets you speak slowly, with pauses.", style = CrundiType.Small, modifier = Modifier.padding(bottom = 10.dp))
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            for (ms in listOf(1500, 2000, 2500, 3000, 4000)) CrundiChip(pauseLabel(ms), s.pauseMs == ms, { if (s.pauseMs != ms) ev.onPause(ms) })
        }
    }

    // ─── When the voice screen closes (every client) ───
    if (s.screenCloseSeconds > 0) Fold(CruSection.SCREEN, "The voice screen closes", "After ${closeLabel(s.screenCloseSeconds)}", open) {
        Text("After this much silence, even when a phone stays awake (plugged in). \"Hey Cru\" opens it again. The same time closes it on the phone, the web and the PC.", style = CrundiType.Small, modifier = Modifier.padding(bottom = 10.dp))
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            for (v in CLOSE_CHOICES) CrundiChip(closeLabel(v), s.screenCloseSeconds == v, { if (s.screenCloseSeconds != v) ev.onScreenClose(v) })
        }
        if (s.screenCloseSeconds !in CLOSE_CHOICES) Text("Now ${closeLabel(s.screenCloseSeconds)}", style = CrundiType.Small, modifier = Modifier.padding(top = 8.dp))
    }

    // ─── Spend and limit (D25) ───
    val sp = s.spend
    Fold(CruSection.SPENDING, "Spending", "$" + "%.2f".format(Locale.US, sp.claudeUsd + sp.deepgramUsd) + " this month", open, warn = s.limitReached || s.deepgramLimitReached) {
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

    // ─── Keys (D25: write only, never on the phone) ───
    val keys = listOfNotNull(
        Triple("anthropicKey", "Claude API key", s.anthropicKeySet),
        Triple("deepgramKey", "Deepgram key", s.deepgramKeySet),
        s.anthropicAdminKeySet?.let { Triple("anthropicAdminKey", "Admin API key", it) },
    )
    val missing = keys.count { !it.third }
    Fold(CruSection.KEYS, "Keys", if (missing == 0) "All set" else "$missing not set", open, warn = !s.anthropicKeySet) {
        Text("Keys live on the server and never come back to the phone. Paste a new one to replace it.", style = CrundiType.Small, modifier = Modifier.padding(bottom = 6.dp))
        for ((i, k) in keys.withIndex()) {
            KeyLine(k.second, k.third, Modifier.padding(top = if (i == 0) 0.dp else 10.dp))
            if (k.first == "anthropicAdminKey") Text("Only for reading what the whole account has cost. It cannot answer.", style = CrundiType.Small)
            Row(Modifier.padding(top = 4.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                val typed = ui.keyText[k.first].orEmpty()
                CrundiTextField(typed, { ev.onKeyText(k.first, it) }, Modifier.weight(1f), placeholder = if (k.third) "Replace the key" else "Paste the key", icon = "key", mono = true, password = true)
                CrundiButton("Save", { ev.onSaveKey(k.first) }, enabled = !ui.busy && typed.isNotBlank())
            }
        }
    }

    // ─── Admin mode: any command on the server, by voice (an older server has none) ───
    s.adminCapable?.let { capable ->
        Fold(CruSection.ADMIN, "Admin mode", if (s.adminMode) "On" else "Off", open, warn = s.adminMode) {
            Text(
                "Cru can run any command on this server as the Crundi user, with no confirmation. Only your voice is obeyed.",
                style = CrundiType.Small.copy(color = CrundiColors.Yellow), modifier = Modifier.padding(bottom = 4.dp),
            )
            mv.obitech.crundi.ui.components.CrundiSwitchRow(
                "Admin mode", s.adminMode, { ev.onAdminMode(it) },
                Modifier.padding(horizontal = 0.dp), detail = if (capable) "Switched on here only, never by Cru." else "Needs Sonnet or above: choose another model first.",
                enabled = !ui.busy && (capable || s.adminMode),
            )
        }
    }

    // ─── Your voice (capability "voice-speaker") ───
    ui.speaker?.let { sp ->
        val st = sp.status
        Fold(CruSection.SPEAKER, "Your voice", when { st == null -> ""; !st.available -> "Not available"; st.enrolled -> "Enrolled"; else -> "Not enrolled" }, open) { SpeakerPart(sp, ev.speaker) }
    }

    // ─── Tickers and triggers (capability "voice-wakeups") ───
    ui.wakeups?.let { w ->
        val all = w.loaded?.let { it.tickers + it.triggers }.orEmpty()
        Fold(CruSection.WAKEUPS, "Tickers and triggers", if (w.loaded == null) "" else "${all.count { it.enabled }} of ${all.size} on", open) { WakeupsPart(w, ev) }
    }

    // ─── Memory (D39: the notebook) ───
    ui.memory?.let { m ->
        val got = m.loaded
        Fold(CruSection.MEMORY, "Notebook", if (got == null) "" else "${got.entries} thing" + (if (got.entries == 1) "" else "s") + " remembered", open, padding = 0.dp, card = false) { MemoryPart(m, ev) }
    }

    // ─── The brains compared ───
    if (claudeCode) Fold(CruSection.STATS, "Brains compared", ui.stats?.stats?.takeIf { it.isNotEmpty() }?.let { st -> "${st.sumOf { it.turns }} turns" }.orEmpty(), open) {
        BrainTable(ui.stats, s)
    }

    CrundiButton("Start a fresh conversation", ev.onReset, icon = "refresh", modifier = Modifier.fillMaxWidth().padding(top = 6.dp))
}

@Composable
private fun Fold(key: String, title: String, summary: String, open: Set<String>, padding: androidx.compose.ui.unit.Dp = 14.dp, warn: Boolean = false, card: Boolean = true, content: @Composable ColumnScope.() -> Unit) =
    mv.obitech.crundi.ui.operations.FoldSection(key, title, summary, open, padding, if (warn) CrundiColors.Yellow else null, card, content)

/** The brains side by side: turns, the median time (and the slowest tenth), cost per turn (and how much came from the cache). */
@Composable
private fun BrainTable(st: BrainStats?, s: VoiceSettings) {
    val rows = st?.stats.orEmpty()
    if (rows.isEmpty()) {
        Text(if (st == null) "This server does not compare the brains yet." else "The two brains are compared here once Cru has answered with them.", style = CrundiType.Small)
        return
    }
    val name = { id: String -> (s.models.firstOrNull { it.id == id }?.label ?: id).removePrefix("Claude ") }
    @Composable fun Cell(top: String, under: String, w: Float, end: Boolean, head: Boolean = false) {
        Column(Modifier.fillMaxWidth(w), horizontalAlignment = if (end) Alignment.End else Alignment.Start) {
            Text(top, style = if (head) CrundiType.Meta else CrundiType.Body)
            if (under.isNotEmpty()) Text(under, style = CrundiType.Meta)
        }
    }
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        for ((i, g) in (listOf<mv.obitech.crundi.api.BrainStat?>(null) + rows).withIndex()) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.Top) {
                if (g == null) {
                    Box(Modifier.weight(1.4f)) { Cell("BRAIN", "", 1f, false, true) }
                    Box(Modifier.weight(0.8f)) { Cell("TURNS", "", 1f, true, true) }
                    Box(Modifier.weight(1f)) { Cell("TIME", "", 1f, true, true) }
                    Box(Modifier.weight(1.1f)) { Cell("PER TURN", "", 1f, true, true) }
                } else {
                    Box(Modifier.weight(1.4f)) { Cell(if (g.brain == "claude-code") "Claude Code" else "API", name(g.model), 1f, false) }
                    Box(Modifier.weight(0.8f)) { Cell(g.turns.toString(), if (g.failed > 0) "${g.failed} failed" else "", 1f, true) }
                    Box(Modifier.weight(1f)) { Cell(secs(g.medianMs), "up to " + secs(g.p90Ms), 1f, true) }
                    Box(Modifier.weight(1.1f)) { Cell("$" + "%.4f".format(Locale.US, g.costPerTurnUsd), "${Math.round(g.cacheReadShare * 100)}% cached", 1f, true) }
                }
            }
            if (i == 0) Box(Modifier.fillMaxWidth().size(Dimens.Hairline).background(CrundiColors.Border))
        }
        Text("Time: the median turn, and up to the slowest tenth. Claude Code costs are list prices, for comparison: your subscription pays them, within its usage limits.", style = CrundiType.Small)
    }
}

private fun secs(ms: Double) = "%.1f s".format(Locale.US, ms / 1000.0)

@Composable
private fun KeyLine(name: String, set: Boolean, modifier: Modifier = Modifier) {
    Row(modifier.padding(vertical = 3.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(name, style = CrundiType.Body, modifier = Modifier.weight(1f))
        StatusBadge(if (set) "set" else "not set", if (set) BadgeKind.Good else BadgeKind.Warn)
    }
}

/** A voice to pick, with a play button to hear it first ([onPreview] null: none). */
@Composable
internal fun VoiceRow(title: String, detail: String, selected: Boolean, onPick: () -> Unit, onPreview: (() -> Unit)?) {
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
        if (onPreview != null) Box(Modifier.size(40.dp).clip(CircleShape).clickable(onClickLabel = "Hear it", onClick = onPreview), contentAlignment = Alignment.Center) {
            CrundiIcon("play", size = 16.dp, tint = CrundiColors.TextSecondary)
        }
    }
}

@Composable
private fun MemoryPart(m: MemoryUi, ev: CruSettingsEvents) {
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

@Composable
private fun WakeupsPart(w: WakeupsUi, ev: CruSettingsEvents) {
    Text("Reasons for Cru to wake up on its own: a ticker repeats on a schedule, a trigger fires when something happens. Each is off until switched on; on each wake-up Cru decides whether to stay quiet, tell you, or alert you. Ask Cru to add one.", style = CrundiType.Small)
    val got = w.loaded
    if (w.error.isNotEmpty()) Text(w.error, style = CrundiType.Small.copy(color = CrundiColors.Yellow), modifier = Modifier.padding(top = 8.dp))
    if (got == null) { if (w.error.isEmpty()) Text("Loading…", style = CrundiType.Small, modifier = Modifier.padding(top = 8.dp)); return }
    for ((title, list, none) in listOf(
        Triple("Tickers", got.tickers, "No tickers yet. Try: Cru, wake me with a briefing at half seven on weekdays."),
        Triple("Triggers", got.triggers, "No triggers yet. Try: Cru, tell me if the CPU stays above 90 percent."),
    )) {
        Text(title, style = CrundiType.Heading, modifier = Modifier.padding(top = 14.dp, bottom = 4.dp))
        if (list.isEmpty()) Text(none, style = CrundiType.Small)
        for (x in list) WakeRow(x, w.confirmDelete == x.id, x.id in w.busy, ev)
    }
    val lim = got.limits
    Text("At most ${lim.perHour} wake-ups an hour (${lim.firedLastHour} in the last hour), and none once the monthly limit is reached.", style = CrundiType.Small, modifier = Modifier.padding(top = 10.dp))
}

@Composable
private fun WakeRow(x: mv.obitech.crundi.api.WakeItem, confirming: Boolean, busy: Boolean, ev: CruSettingsEvents) {
    Column(Modifier.fillMaxWidth().padding(vertical = 6.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(x.name.ifEmpty { x.id }, style = CrundiType.Body.copy(color = if (x.enabled) CrundiColors.TextPrimary else CrundiColors.TextSecondary))
                if (x.summary.isNotEmpty()) Text(x.summary, style = CrundiType.Small)
            }
            androidx.compose.material3.Switch(
                checked = x.enabled, onCheckedChange = { ev.onWakeupSwitch(x.id, it) }, enabled = !busy,
                colors = androidx.compose.material3.SwitchDefaults.colors(
                    checkedThumbColor = CrundiColors.TextPrimary, checkedTrackColor = CrundiColors.Accent, checkedBorderColor = CrundiColors.Accent,
                    uncheckedThumbColor = CrundiColors.TextSecondary, uncheckedTrackColor = CrundiColors.BgTertiary, uncheckedBorderColor = CrundiColors.Border,
                ),
                modifier = Modifier.semantics { contentDescription = if (x.enabled) "On: switch off" else "Off: switch on" },
            )
        }
        if (x.purpose.isNotEmpty()) Text(x.purpose, style = CrundiType.Small, modifier = Modifier.padding(top = 2.dp))
        Row(Modifier.padding(top = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(
                "Last woke Cru: " + (if (x.lastFiredAt > 0) wakeTime(x.lastFiredAt) else "never") + (if (x.fires > 0) " (${x.fires} in all)" else "") + " · made by " + (if (x.createdBy == "owner") "you" else "Cru"),
                style = CrundiType.Meta, modifier = Modifier.weight(1f),
            )
            CrundiButton(if (confirming) "Delete for good?" else "Delete", { ev.onWakeupDelete(x.id) }, kind = ButtonKind.Danger, icon = "trash", enabled = !busy)
        }
        Box(Modifier.fillMaxWidth().padding(top = 8.dp).size(Dimens.Hairline).background(CrundiColors.Border))
    }
}

private fun wakeTime(ms: Long): String = java.text.SimpleDateFormat("d MMM HH:mm", Locale.getDefault()).format(java.util.Date(ms))

private val VOLUME_CHOICES = listOf(25, 40, 55, 70, 85, 100)
private val RATE_CHOICES = listOf(0.85, 1.0, 1.15, 1.3, 1.5)
private val CLOSE_CHOICES = listOf(30, 45, 60, 120, 300)
private val IDLE_CHOICES = listOf(5, 10, 15, 30, 60, 120)

private fun pauseLabel(ms: Int) = "${ms / 1000.0} s".replace(".0 s", " s")

/** A time of silence as shown: "45 s", "2 min", "1 min 30 s". */
fun closeLabel(sec: Int): String = if (sec < 60) "$sec s" else if (sec % 60 == 0) "${sec / 60} min" else "${sec / 60} min ${sec % 60} s"

/** A talking speed as shown: "Normal", "1.15×". */
fun rateLabel(r: Double): String = if (kotlin.math.abs(r - 1.0) < 0.01) "Normal" else ("%.2f".format(java.util.Locale.US, r).trimEnd('0').trimEnd('.') + "×")
