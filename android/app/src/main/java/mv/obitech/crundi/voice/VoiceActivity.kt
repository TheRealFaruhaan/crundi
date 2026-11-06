package mv.obitech.crundi.voice

import android.Manifest
import android.app.KeyguardManager
import android.content.pm.PackageManager
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import mv.obitech.crundi.ui.theme.CrundiTheme

/**
 * Trigger mode B (D33): Gemini stays the phone's assistant, and this screen
 * is opened by double-pressing the side key (set to open "Cru"). The launcher entry "Cru" is an
 * alias of it. Shows over the lock screen.
 *
 * Left alone it winds down in three steps: the microphone stays on for the
 * chosen time after the last thing said or answered; then the screen stays
 * open, listening on the phone only for "Hey Cru" (whatever the wake
 * settings say), which wakes it where it is; after a few more minutes
 * without a word the screen may sleep, and once it does the screen closes.
 *
 * The overlay also opens it when it needs what only an activity can do: ask
 * for the microphone, or ask for the phone to be unlocked ([EXTRA_UNLOCK]).
 */
class VoiceActivity : ComponentActivity() {
    private lateinit var controller: VoiceController
    private val askMic = registerForActivityResult(ActivityResultContracts.RequestPermission()) { controller.open() }

    private val handler = android.os.Handler(android.os.Looper.getMainLooper())
    /** The session is asleep and the screen open: listening for "Hey Cru" only. */
    private var waiting by mutableStateOf(false)
    private var inFront = false
    private val letSleep = Runnable { keepOn(false) }

    private fun keepOn(on: Boolean) {
        val f = android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON
        if (on) window.addFlags(f) else window.clearFlags(f)
    }

    /** The microphone's time is up: listen for the wake word, and let the screen sleep later. */
    private fun asleep() {
        if (!inFront) { finish(); return }
        waiting = true
        HeyCrundi.screenWaiting { wake() }
        handler.removeCallbacks(letSleep)
        handler.postDelayed(letSleep, VoicePrefs.screenAwakeMinutes(this) * 60_000L)
    }

    /** "Hey Cru", or the mic tapped: listening again. */
    private fun wake() {
        handler.removeCallbacks(letSleep)
        keepOn(true)
        waiting = false
        controller.tapMic() // asleep: opens a new session (it takes the microphone first)
        HeyCrundi.screenWaiting(null)
    }

    private fun locked() = getSystemService(KeyguardManager::class.java)?.isKeyguardLocked == true

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        // The screen stays on while the voice screen is open.
        keepOn(true)
        // Asked to close: gone, not asleep listening for Hey Cru.
        controller = VoiceController(this, "Cru", ::locked, onFinish = { asleep() }, onDismiss = { finish() })
        setContent {
            CrundiTheme {
                val ui by controller.ui.collectAsStateWithLifecycle()
                val shell = controller.shell?.collectAsStateWithLifecycle()?.value
                val wake by HeyCrundi.listening.collectAsStateWithLifecycle()
                VoiceScreen(
                    ui,
                    mic = { controller.micLevel.value },
                    voice = { controller.voiceLevel.value },
                    onMic = { if (waiting) wake() else controller.tapMic() },
                    waitingHint = if (waiting && wake == HeyCrundiPlan.Listener.Screen) "Say \"Hey Cru\" to talk again" else "",
                    onClose = { finish() },
                    onUnlock = { unlock() },
                    shell = shell,
                    onAnswer = { controller.answer(it) },
                    onAllowMic = { askMic.launch(Manifest.permission.RECORD_AUDIO) },
                )
            }
        }
        if (intent?.getBooleanExtra(EXTRA_UNLOCK, false) == true) unlock()
        // Opened by "Hey Cru" from the background: its "tap to talk" is done.
        HeyCrundi.cancelWakeNotification(this)
    }

    override fun onStart() {
        super.onStart()
        inFront = true
        if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED && !asked) {
            asked = true
            askMic.launch(Manifest.permission.RECORD_AUDIO)
        } else controller.open()
    }

    override fun onStop() {
        // Off screen: stop listening; coming back wakes it again. Asleep, and the
        // screen went off (or the owner left): it's done.
        inFront = false
        val wasWaiting = waiting
        handler.removeCallbacks(letSleep)
        HeyCrundi.screenWaiting(null)
        waiting = false
        controller.close()
        if (wasWaiting) finish()
        super.onStop()
    }

    override fun onDestroy() {
        handler.removeCallbacks(letSleep)
        if (waiting) HeyCrundi.screenWaiting(null)
        controller.release()
        super.onDestroy()
    }

    private var asked = false

    private fun unlock() {
        val km = getSystemService(KeyguardManager::class.java) ?: return
        if (!km.isKeyguardLocked) { controller.unlocked(); return }
        km.requestDismissKeyguard(this, object : KeyguardManager.KeyguardDismissCallback() {
            override fun onDismissSucceeded() { controller.unlocked() }
        })
    }

    companion object {
        const val EXTRA_UNLOCK = "unlock"
    }
}
