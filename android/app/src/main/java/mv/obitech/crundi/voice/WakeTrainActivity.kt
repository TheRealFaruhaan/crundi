package mv.obitech.crundi.voice

import android.Manifest
import android.annotation.SuppressLint
import android.content.pm.PackageManager
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
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
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.TopBar
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiTheme
import mv.obitech.crundi.ui.theme.CrundiType
import kotlin.math.sqrt

/** One of the five recordings. */
enum class Take { Empty, Good, Redo }

/** What the "Hey Cru" training screen shows. */
data class TrainUi(
    val takes: List<Take> = List(CruWake.SAMPLES_NEEDED) { Take.Empty },
    /** The recording being asked for now (0-based), or -1 when none is (all good, or setting up). */
    val current: Int = 0,
    val recording: Boolean = false,
    /** Setting up from the five: the spotter runs over each, several seconds. */
    val working: Boolean = false,
    /** Trained, and when ("6 Nov"). */
    val doneOn: String = "",
    /** The line under the button: what was wrong, what to do next. */
    val message: String = "",
    val messageBad: Boolean = false,
    /** Trying it after training: listening for a few seconds. */
    val trying: Boolean = false,
    val tryResult: String = "",
    val tryGood: Boolean = false,
    /** This phone can't run the spotter. */
    val unsupported: Boolean = false,
)

class TrainEvents(
    val onBack: () -> Unit = {},
    val onRecord: () -> Unit = {},
    val onTry: () -> Unit = {},
    val onAgain: () -> Unit = {},
)

/**
 * "Hey Cru" is trained before it can listen (the owner's own voice, five
 * times; see [CruWake]). Recording takes the microphone from the wake word
 * ([HeyCrundi.training]) and gives it back after.
 */
class WakeTrainActivity : ComponentActivity() {
    private var ui by mutableStateOf(TrainUi())
    private var level by mutableStateOf(0f)
    private val clips = arrayOfNulls<FloatArray>(CruWake.SAMPLES_NEEDED)
    private var job: Job? = null
    private val askMic = registerForActivityResult(ActivityResultContracts.RequestPermission()) { ok -> if (ok) record() }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        window.addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        if (!WakeWordEngine.supported(this)) ui = ui.copy(unsupported = true, current = -1)
        setContent {
            CrundiTheme {
                WakeTrainScreen(ui, { level }, TrainEvents(
                    onBack = { finish() },
                    onRecord = { if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) record() else askMic.launch(Manifest.permission.RECORD_AUDIO) },
                    onTry = { tryIt() },
                    onAgain = { clips.fill(null); ui = TrainUi() },
                ))
            }
        }
    }

    override fun onStart() { super.onStart(); HeyCrundi.training(true) }

    override fun onStop() {
        job?.cancel(); job = null
        if (ui.recording || ui.trying) ui = ui.copy(recording = false, trying = false)
        HeyCrundi.training(false)
        super.onStop()
    }

    /** One take: up to three seconds, ended early by a short quiet after the words. */
    private fun record() {
        val i = ui.current
        if (i < 0 || ui.recording || ui.working) return
        ui = ui.copy(recording = true, message = "Say \"Hey Cru\"", messageBad = false)
        job = lifecycleScope.launch {
            val audio = withContext(Dispatchers.IO) { capture(3.0, stopOnQuiet = true) }
            level = 0f
            if (audio == null) { ui = ui.copy(recording = false, message = "The microphone could not be opened. Is another app using it?", messageBad = true); return@launch }
            when (val c = CruWake.checkSample(audio)) {
                is CruWake.Sample.Bad -> ui = ui.copy(recording = false, takes = ui.takes.set(i, Take.Redo), message = c.why, messageBad = true)
                is CruWake.Sample.Ok -> {
                    clips[i] = audio
                    val takes = ui.takes.set(i, Take.Good)
                    val next = takes.indexOfFirst { it != Take.Good }
                    ui = ui.copy(recording = false, takes = takes, current = next, message = if (next >= 0) "Good. Again, as you would normally say it." else "", messageBad = false)
                    if (next < 0) train()
                }
            }
        }
    }

    private fun train() {
        val engine = HeyCrundi.engineForTraining() ?: run { ui = ui.copy(unsupported = true); return }
        ui = ui.copy(working = true, current = -1, message = "Setting up from your voice…")
        lifecycleScope.launch {
            val now = java.time.Instant.now().toString()
            val r = withContext(Dispatchers.Default) { CruWake.train(clips.map { it!! }, now) { line, clip -> engine.hears(line, clip) } }
            when (r) {
                is CruWake.Trained.Ok -> {
                    WakeProfiles.save(this@WakeTrainActivity, r.profile)
                    HeyCrundi.profileChanged()
                    ui = ui.copy(working = false, doneOn = day(now), message = "")
                }
                is CruWake.Trained.Redo -> {
                    var takes = ui.takes
                    for (k in r.which) { takes = takes.set(k, Take.Redo); clips[k] = null }
                    ui = ui.copy(working = false, takes = takes, current = takes.indexOfFirst { it != Take.Good }, message = r.why, messageBad = true)
                }
            }
        }
    }

    /** After training: listen for five seconds with the new profile and say what it made of it. */
    private fun tryIt() {
        val engine = HeyCrundi.engineForTraining() ?: return
        val profile = WakeProfiles.get(this) ?: return
        if (ui.trying) return
        ui = ui.copy(trying = true, tryResult = "Say \"Hey Cru\"…", tryGood = false)
        job = lifecycleScope.launch {
            val audio = withContext(Dispatchers.IO) { capture(5.0, stopOnQuiet = false) }
            level = 0f
            if (audio == null) { ui = ui.copy(trying = false, tryResult = "The microphone could not be opened.", tryGood = false); return@launch }
            // As when listening: the spotter first, then the voice match on what was said.
            val verdict = withContext(Dispatchers.Default) { if (engine.hears(profile.keywordLine(), CruWake.padded(audio))) profile.verify(audio) else null }
            ui = when {
                verdict == null -> ui.copy(trying = false, tryResult = "It didn't hear \"Hey Cru\". Try again, or train again if it keeps missing.", tryGood = false)
                verdict.ok -> ui.copy(trying = false, tryResult = "Heard you. That would wake Cru.", tryGood = true)
                else -> ui.copy(trying = false, tryResult = "It heard \"Hey Cru\", but not quite like your recordings, so it wouldn't wake. Train again in the room you'll use it in.", tryGood = false)
            }
        }
    }

    /** Up to [seconds] of 16 kHz mono; with [stopOnQuiet], ends 0.7 s after the words do. Null without a microphone. */
    @SuppressLint("MissingPermission")
    private suspend fun capture(seconds: Double, stopOnQuiet: Boolean): FloatArray? {
        val rate = CruWake.RATE
        val min = AudioRecord.getMinBufferSize(rate, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
        if (min <= 0) return null
        val rec = runCatching { AudioRecord(MediaRecorder.AudioSource.VOICE_RECOGNITION, rate, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT, maxOf(min, rate)) }.getOrNull() ?: return null
        if (rec.state != AudioRecord.STATE_INITIALIZED) { rec.release(); return null }
        val out = FloatArray((rate * seconds).toInt())
        var n = 0
        try {
            rec.startRecording()
            val buf = ShortArray(rate / 20)
            var spoke = false; var quietMs = 0; var noise = 0.004
            while (n < out.size && kotlinx.coroutines.currentCoroutineContext().isActive) {
                val got = rec.read(buf, 0, buf.size)
                if (got <= 0) break
                var s = 0.0
                for (i in 0 until got) { val x = buf[i] / 32768f; if (n < out.size) out[n++] = x; s += x * x }
                val rms = sqrt(s / got)
                level = MicStream.level(rms * 32768.0).coerceIn(0f, 1f)
                if (n < rate / 4) noise = maxOf(noise, rms)
                if (rms > maxOf(0.02, noise * 4)) { spoke = true; quietMs = 0 } else if (spoke) quietMs += got * 1000 / rate
                if (stopOnQuiet && spoke && quietMs >= 700) break
            }
        } catch (e: Exception) {
            return null
        } finally {
            runCatching { rec.stop() }
            rec.release()
        }
        return out.copyOf(n)
    }

    private fun day(iso: String) = runCatching {
        java.time.format.DateTimeFormatter.ofPattern("d MMM").format(java.time.Instant.parse(iso).atZone(java.time.ZoneId.systemDefault()))
    }.getOrDefault("")
}

private fun <T> List<T>.set(i: Int, v: T): List<T> = toMutableList().also { it[i] = v }

@Composable
fun WakeTrainScreen(ui: TrainUi, level: () -> Float, ev: TrainEvents) {
    Column(Modifier.fillMaxSize().background(CrundiColors.BgPrimary).safeDrawingPadding()) {
        TopBar("Train Hey Cru", onBack = ev.onBack)
        Column(
            Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 24.dp, vertical = 16.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            if (ui.unsupported) {
                Text("This phone can't run the wake word: its processor isn't one this build of Crundi has the engine for.", style = CrundiType.Body, textAlign = TextAlign.Center)
                return@Column
            }
            val done = ui.doneOn.isNotEmpty()
            Text(
                if (done) "Cru knows your voice" else "Say \"Hey Cru\" five times",
                style = CrundiType.Title.copy(fontSize = 22.sp), textAlign = TextAlign.Center,
            )
            Text(
                if (done) "Trained ${ui.doneOn}. Turn on \"Hey Cru\" in Voice assistant settings, and it will listen for you."
                else "The way you'll call Cru, in the room you'll use it in. It learns how you say it, so it wakes for you and not for the television. The recordings stay on this phone.",
                style = CrundiType.Body.copy(color = CrundiColors.TextSecondary, lineHeight = 21.sp), textAlign = TextAlign.Center,
                modifier = Modifier.padding(top = 8.dp),
            )
            Spacer(Modifier.height(28.dp))
            Takes(ui)
            Spacer(Modifier.height(32.dp))
            if (!done) {
                RecordButton(ui, level, ev.onRecord)
                Spacer(Modifier.height(18.dp))
                Text(
                    when {
                        ui.working -> "Setting up from your voice…"
                        ui.recording -> "Listening…"
                        ui.message.isNotEmpty() -> ui.message
                        else -> "Tap, then say \"Hey Cru\" (${ui.current + 1} of ${CruWake.SAMPLES_NEEDED})"
                    },
                    style = CrundiType.Body.copy(color = if (ui.messageBad && !ui.recording && !ui.working) CrundiColors.Yellow else CrundiColors.TextSecondary),
                    textAlign = TextAlign.Center,
                )
            } else {
                CruIcon(96.dp)
                Spacer(Modifier.height(22.dp))
                CrundiButton(if (ui.trying) "Listening…" else "Try it", ev.onTry, kind = ButtonKind.Primary, icon = "mic", enabled = !ui.trying)
                if (ui.tryResult.isNotEmpty()) Text(
                    ui.tryResult,
                    style = CrundiType.Body.copy(color = if (ui.tryGood) CrundiColors.Green else if (ui.trying) CrundiColors.TextSecondary else CrundiColors.Yellow),
                    textAlign = TextAlign.Center, modifier = Modifier.padding(top = 14.dp),
                )
                Spacer(Modifier.height(18.dp))
                CrundiButton("Train again", ev.onAgain, kind = ButtonKind.Ghost)
            }
        }
    }
}

@Composable
private fun Takes(ui: TrainUi) {
    Row(horizontalArrangement = Arrangement.spacedBy(14.dp), verticalAlignment = Alignment.CenterVertically) {
        ui.takes.forEachIndexed { i, t ->
            val now = i == ui.current && !ui.working
            val (fill, ring) = when (t) {
                Take.Good -> CrundiColors.Accent to CrundiColors.Accent
                Take.Redo -> CrundiColors.YellowDim to CrundiColors.Yellow
                Take.Empty -> Color.Transparent to if (now) CrundiColors.AccentHover else CrundiColors.Border
            }
            Box(
                Modifier.size(if (now) 18.dp else 14.dp).clip(CircleShape).background(fill).border(1.5.dp, ring, CircleShape)
                    .semantics { contentDescription = "Recording ${i + 1}: " + when (t) { Take.Good -> "done"; Take.Redo -> "to record again"; Take.Empty -> "to do" } },
                contentAlignment = Alignment.Center,
            ) { if (t == Take.Good) CrundiIcon("check", size = 10.dp, tint = Color.White) }
        }
    }
}

/** The big round button; while recording its ring follows the voice. */
@Composable
private fun RecordButton(ui: TrainUi, level: () -> Float, onClick: () -> Unit) {
    val on = ui.recording
    val lvl by animateFloatAsState(if (on) level() else 0f, tween(90), label = "lvl")
    Box(Modifier.size(148.dp), contentAlignment = Alignment.Center) {
        Canvas(Modifier.fillMaxSize()) {
            val c = Offset(size.width / 2, size.height / 2)
            val base = size.minDimension * 0.34f
            for (k in 0 until 5) {
                val r = base + (k + 1) * size.minDimension * 0.028f * (0.6f + 2.4f * lvl)
                drawCircle(CrundiColors.Sky.copy(alpha = (if (on) 0.5f else 0.18f) * (1f - k / 5f)), r, c, style = Stroke(1.2f * density))
            }
        }
        Box(
            Modifier.size(104.dp).clip(CircleShape)
                .background(if (on) CrundiColors.Accent else CrundiColors.AccentDim)
                .border(1.5.dp, CrundiColors.Accent, CircleShape)
                .clickable(enabled = !on && !ui.working && ui.current >= 0, onClickLabel = "Record", onClick = onClick),
            contentAlignment = Alignment.Center,
        ) { CrundiIcon("mic", size = 34.dp, tint = if (on) Color.White else CrundiColors.AccentHover) }
    }
}
