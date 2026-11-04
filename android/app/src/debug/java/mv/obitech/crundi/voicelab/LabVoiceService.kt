package mv.obitech.crundi.voicelab

import android.app.KeyguardManager
import android.content.Context
import android.content.Intent
import android.graphics.Color
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.service.voice.VoiceInteractionService
import android.service.voice.VoiceInteractionSession
import android.service.voice.VoiceInteractionSessionService
import android.speech.RecognitionService
import android.speech.SpeechRecognizer
import android.view.Gravity
import android.view.View
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView

/** Present while the lab is the phone's digital assistant app. */
class LabVoiceService : VoiceInteractionService() {
    override fun onReady() {
        super.onReady()
        LabLog.add(this, "Assistant service ready: Crundi Voice Lab is the default digital assistant")
    }
}

class LabSessionService : VoiceInteractionSessionService() {
    override fun onNewSession(args: Bundle?): VoiceInteractionSession = LabSession(this)
}

/**
 * What the side key / assist gesture opens. Check 0.1: is it shown over the
 * lock screen, and may a microphone service be started from here?
 */
class LabSession(ctx: Context) : VoiceInteractionSession(ctx) {
    private lateinit var status: TextView
    private val main = Handler(Looper.getMainLooper())

    override fun onCreateContentView(): View {
        val c = context
        val col = LinearLayout(c).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.parseColor("#EE101418"))
            setPadding(48, 48, 48, 96)
            gravity = Gravity.BOTTOM
        }
        col.addView(TextView(c).apply {
            text = "Crundi Voice Lab"
            textSize = 22f
            setTextColor(Color.WHITE)
        })
        status = TextView(c).apply {
            textSize = 15f
            setTextColor(Color.parseColor("#CCFFFFFF"))
            setPadding(0, 16, 0, 24)
        }
        col.addView(status)
        col.addView(Button(c).apply {
            text = "Start mic service again"
            setOnClickListener { LabMicService.start(context, "assistant overlay (button)") ; status.append("\nStarted again — see the report.") }
        })
        col.addView(Button(c).apply {
            text = "Open the lab"
            setOnClickListener {
                startAssistantActivity(Intent(context, VoiceLabActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            }
        })
        col.addView(Button(c).apply {
            text = "Close"
            setOnClickListener { hide() }
        })
        return col
    }

    override fun onShow(args: Bundle?, showFlags: Int) {
        super.onShow(args, showFlags)
        val locked = context.getSystemService(KeyguardManager::class.java)?.isKeyguardLocked == true
        val how = buildList {
            if (showFlags and SHOW_WITH_ASSIST != 0) add("assist")
            if (showFlags and SHOW_SOURCE_ASSIST_GESTURE != 0) add("assist gesture")
            if (showFlags and SHOW_SOURCE_ACTIVITY != 0) add("activity")
            if (showFlags and SHOW_SOURCE_APPLICATION != 0) add("application")
        }.ifEmpty { listOf("flags=$showFlags") }.joinToString()
        LabLog.add(context, "[0.1] assistant overlay shown, phone ${if (locked) "LOCKED" else "unlocked"}, opened by: $how")
        status.text = "Phone is ${if (locked) "locked" else "unlocked"}. Starting the microphone service from this overlay…"
        // A moment after showing, so the overlay is fully up when it's tried.
        main.postDelayed({ LabMicService.start(context, "assistant overlay, ${if (locked) "locked" else "unlocked"}") }, 500)
    }
}

/** Android insists an assistant names one; the lab doesn't recognise speech itself. */
class LabRecognitionService : RecognitionService() {
    override fun onStartListening(recognizerIntent: Intent?, listener: RecognitionService.Callback?) {
        listener?.error(SpeechRecognizer.ERROR_CLIENT)
    }
    override fun onCancel(listener: RecognitionService.Callback?) {}
    override fun onStopListening(listener: RecognitionService.Callback?) {}
}
