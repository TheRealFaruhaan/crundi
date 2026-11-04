package mv.obitech.crundi.voicelab

import android.Manifest
import android.app.Activity
import android.app.KeyguardManager
import android.app.role.RoleManager
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Color
import android.graphics.Typeface
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.provider.Settings
import android.speech.RecognitionListener
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import android.speech.tts.TextToSpeech
import android.speech.tts.Voice
import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import android.widget.Toast
import java.util.Locale

/**
 * Voice Lab — the Phase 0 device checks of specs/voice-assistant-plan.md, in
 * one screen of plain views (debug builds only). Everything it finds goes to
 * [LabLog]; "Copy report" puts that on the clipboard to paste back.
 *
 *   0.1  side key overlay over the lock screen + microphone service from it
 *   0.2  "Hey Google, open Crundi Voice" (this screen's launcher name)
 *   0.3  which British English voices the phone has, played as Crundi
 *   0.4  on-device speech recognition: available, how fast, how accurate
 *   0.5  wake word: instructions for the sherpa-onnx demo app
 */
class VoiceLabActivity : Activity() {
    private val main = Handler(Looper.getMainLooper())
    private lateinit var report: TextView
    private lateinit var assistantState: TextView
    private lateinit var voices: LinearLayout
    private lateinit var heard: TextView
    private var tts: TextToSpeech? = null
    private var recognizer: SpeechRecognizer? = null
    private val refresh: () -> Unit = { main.post { if (::report.isInitialized) report.text = LabLog.read(this) } }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(build())
        LabLog.listen(refresh)
        noteOpened(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        noteOpened(intent)
    }

    override fun onResume() {
        super.onResume()
        assistantState.text = assistantLine()
        refresh()
    }

    override fun onDestroy() {
        LabLog.unlisten(refresh)
        tts?.shutdown()
        recognizer?.destroy()
        super.onDestroy()
    }

    /** Check 0.2: how this screen was opened, and whether the phone was locked. */
    private fun noteOpened(i: Intent?) {
        val via = i?.component?.className?.substringAfterLast('.') ?: "?"
        val locked = getSystemService(KeyguardManager::class.java)?.isKeyguardLocked == true
        LabLog.add(this, "[0.2] lab opened via $via, phone ${if (locked) "LOCKED (shown over lock screen)" else "unlocked"}")
    }

    // ─── Layout ───

    private fun build(): View {
        val col = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(40, 56, 40, 80)
        }
        col.addView(title("Crundi Voice Lab"))
        col.addView(text(LabLog.device()))

        // Permissions
        col.addView(heading("Permissions"))
        col.addView(button("Grant microphone + notifications") {
            val want = mutableListOf(Manifest.permission.RECORD_AUDIO)
            if (Build.VERSION.SDK_INT >= 33) want += Manifest.permission.POST_NOTIFICATIONS
            requestPermissions(want.toTypedArray(), 1)
        })

        // 0.1
        col.addView(heading("0.1 Side key over the lock screen"))
        col.addView(text(
            "1. Make this app the digital assistant (button below).\n" +
            "2. Samsung: Settings › Advanced features › Side button › Press and hold › Digital assistant.\n" +
            "3. Lock the phone, hold the side key. The lab overlay should appear and test the microphone by itself.\n" +
            "4. Repeat unlocked, from another app."
        ))
        assistantState = text(assistantLine())
        col.addView(assistantState)
        col.addView(button("Open default assistant settings") { openAssistantSettings() })
        col.addView(button("Control: mic service now (app open)") { LabMicService.start(this, "lab screen, app open") })
        col.addView(button("Control: mic service in 10 s (press Home now)") {
            Toast.makeText(this, "Press Home or lock the phone now", Toast.LENGTH_LONG).show()
            main.postDelayed({ LabMicService.start(applicationContext, "10 s after leaving the lab (background)") }, 10_000)
        })

        // 0.2
        col.addView(heading("0.2 Gemini"))
        col.addView(text(
            "This screen's launcher name is \"Crundi Voice\".\n" +
            "1. Unlocked: say \"Hey Google, open Crundi Voice\".\n" +
            "2. Locked: say it again. Note whether Gemini opens it or asks to unlock. Each opening is logged."
        ))

        // 0.3
        col.addView(heading("0.3 British voices"))
        col.addView(button("List en-GB voices") { listVoices() })
        col.addView(button("Text-to-speech settings") { open(Intent("com.android.settings.TTS_SETTINGS")) })
        voices = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        col.addView(voices)

        // 0.4
        col.addView(heading("0.4 Speech recognition"))
        col.addView(text(recognitionLine()))
        col.addView(button("Listen (on-device)") { listen(onDevice = true) })
        col.addView(button("Listen (default recogniser)") { listen(onDevice = false) })
        heard = text("Say: \"Crundi, how much usage do I have left this week?\"")
        col.addView(heard)

        // 0.5
        col.addView(heading("0.5 \"Hey Crundi\" wake word"))
        col.addView(text(
            "Not in this lab: install sherpa-onnx's keyword-spotting demo APK (github.com/k2-fsa/sherpa-onnx, Android KWS) " +
            "and set the keyword to \"crundi\". Leave it running for an hour in a normal room and note: " +
            "false triggers, missed \"hey crundi\"s, and battery used (Settings › Battery). Type the results into the report."
        ))

        // Report
        col.addView(heading("Report"))
        val row = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        row.addView(button("Copy report") {
            val text = "Crundi Voice Lab — ${LabLog.device()}\n\n${LabLog.read(this)}"
            getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("Voice Lab report", text))
            Toast.makeText(this, "Copied", Toast.LENGTH_SHORT).show()
        })
        row.addView(button("Clear") { LabLog.clear(this) })
        col.addView(row)
        report = text(LabLog.read(this)).apply { typeface = Typeface.MONOSPACE; textSize = 12f }
        col.addView(report)

        return ScrollView(this).apply {
            setBackgroundColor(Color.parseColor("#101418"))
            addView(col)
        }
    }

    private fun title(s: String) = TextView(this).apply { text = s; textSize = 24f; setTextColor(Color.WHITE) }
    private fun heading(s: String) = TextView(this).apply {
        text = s; textSize = 18f; setTextColor(Color.parseColor("#8AB4F8")); setPadding(0, 40, 0, 8)
    }
    private fun text(s: String) = TextView(this).apply { text = s; textSize = 14f; setTextColor(Color.parseColor("#DDFFFFFF")) }
    private fun button(s: String, onClick: () -> Unit) = Button(this).apply {
        text = s; isAllCaps = false
        layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
        setOnClickListener { onClick() }
    }

    private fun open(i: Intent) {
        try { startActivity(i) } catch (e: Exception) { Toast.makeText(this, "Can't open: ${e.javaClass.simpleName}", Toast.LENGTH_SHORT).show() }
    }

    // ─── 0.1 ───

    private fun assistantLine(): String {
        val held = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            getSystemService(RoleManager::class.java)?.isRoleHeld(RoleManager.ROLE_ASSISTANT) == true
        } else null
        val mic = checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED
        val role = when (held) { true -> "YES"; false -> "no"; null -> "unknown (Android 9 or older)" }
        return "Default assistant: $role · Microphone permission: ${if (mic) "granted" else "NOT granted"}"
    }

    private fun openAssistantSettings() {
        val tries = listOf(
            Intent(Settings.ACTION_VOICE_INPUT_SETTINGS),
            Intent(Settings.ACTION_MANAGE_DEFAULT_APPS_SETTINGS),
        )
        for (i in tries) {
            try { startActivity(i); return } catch (_: Exception) { }
        }
        Toast.makeText(this, "Open Settings › Apps › Choose default apps › Digital assistant app", Toast.LENGTH_LONG).show()
    }

    // ─── 0.3 ───

    private fun listVoices() {
        voices.removeAllViews()
        tts?.shutdown()
        tts = TextToSpeech(this) { status ->
            main.post {
                val engine = tts ?: return@post
                if (status != TextToSpeech.SUCCESS) {
                    LabLog.add(this, "[0.3] text-to-speech failed to start (status $status)")
                    return@post
                }
                val gb = engine.voices.orEmpty()
                    .filter { it.locale.language == "en" && it.locale.country == "GB" }
                    .sortedWith(compareBy<Voice>({ it.isNetworkConnectionRequired }, { -it.quality }, { it.name }))
                LabLog.add(this, "[0.3] engine ${engine.defaultEngine}: ${gb.size} en-GB voices: " +
                    gb.joinToString { "${it.name}(q${it.quality}${if (it.isNetworkConnectionRequired) ",net" else ""})" })
                if (gb.isEmpty()) voices.addView(text("No en-GB voices. Install UK English in text-to-speech settings."))
                for (v in gb) voices.addView(button("▶ ${v.name}${if (v.isNetworkConnectionRequired) " (network)" else ""}") { speak(v) })
            }
        }
    }

    private fun speak(v: Voice) {
        val engine = tts ?: return
        engine.setVoice(v)
        engine.speak(
            "Yes, Master. Your weekly usage is at forty two percent, and all four services are running. The API chat finished its turn a minute ago.",
            TextToSpeech.QUEUE_FLUSH, null, "lab-${v.name}",
        )
        LabLog.add(this, "[0.3] played ${v.name}")
    }

    // ─── 0.4 ───

    private fun recognitionLine(): String {
        val any = SpeechRecognizer.isRecognitionAvailable(this)
        val onDevice = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) SpeechRecognizer.isOnDeviceRecognitionAvailable(this).toString() else "needs Android 12"
        return "Recogniser available: $any · On-device available: $onDevice"
    }

    private fun listen(onDevice: Boolean) {
        recognizer?.destroy()
        val r = if (onDevice) {
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S || !SpeechRecognizer.isOnDeviceRecognitionAvailable(this)) {
                LabLog.add(this, "[0.4] on-device recognition not available")
                return
            }
            SpeechRecognizer.createOnDeviceSpeechRecognizer(this)
        } else SpeechRecognizer.createSpeechRecognizer(this)
        recognizer = r
        val kind = if (onDevice) "on-device" else "default"
        var startedAt = 0L
        var speechEndedAt = 0L
        r.setRecognitionListener(object : RecognitionListener {
            override fun onReadyForSpeech(params: Bundle?) { heard.text = "Listening ($kind)…"; startedAt = SystemClock.elapsedRealtime() }
            override fun onBeginningOfSpeech() {}
            override fun onRmsChanged(rmsdB: Float) {}
            override fun onBufferReceived(buffer: ByteArray?) {}
            override fun onEndOfSpeech() { speechEndedAt = SystemClock.elapsedRealtime() }
            override fun onError(error: Int) {
                heard.text = "Error $error"
                LabLog.add(this@VoiceLabActivity, "[0.4] $kind recognition error $error")
            }
            override fun onResults(results: Bundle?) {
                val best = results?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull() ?: ""
                val now = SystemClock.elapsedRealtime()
                val lag = if (speechEndedAt > 0) now - speechEndedAt else now - startedAt
                heard.text = "Heard: \"$best\""
                LabLog.add(this@VoiceLabActivity, "[0.4] $kind heard \"$best\" (${lag} ms after speech ended)")
            }
            override fun onPartialResults(partialResults: Bundle?) {
                partialResults?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull()?.let { heard.text = "… $it" }
            }
            override fun onEvent(eventType: Int, params: Bundle?) {}
        })
        val i = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH)
            .putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
            .putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, true)
            .putExtra(RecognizerIntent.EXTRA_LANGUAGE, Locale.getDefault().toLanguageTag())
        r.startListening(i)
    }
}
