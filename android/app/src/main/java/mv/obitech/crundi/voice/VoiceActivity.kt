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
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import mv.obitech.crundi.ui.theme.CrundiTheme

/**
 * Trigger mode B (D33): Gemini stays the phone's assistant, and this screen
 * is opened by double-pressing the side key (set to open "Crundi Voice") or
 * "Hey Google, open Crundi Voice". The launcher entry "Crundi Voice" is an
 * alias of it. Shows over the lock screen; goes back to sleep (closes) after
 * a quiet spell, like the overlay.
 *
 * The overlay also opens it when it needs what only an activity can do: ask
 * for the microphone, or ask for the phone to be unlocked ([EXTRA_UNLOCK]).
 */
class VoiceActivity : ComponentActivity() {
    private lateinit var controller: VoiceController
    private val askMic = registerForActivityResult(ActivityResultContracts.RequestPermission()) { controller.open() }

    private fun locked() = getSystemService(KeyguardManager::class.java)?.isKeyguardLocked == true

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        // The screen stays on while the voice screen is open.
        window.addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        controller = VoiceController(this, "Crundi Voice", ::locked) { finish() }
        setContent {
            CrundiTheme {
                val ui by controller.ui.collectAsStateWithLifecycle()
                val shell = controller.shell?.collectAsStateWithLifecycle()?.value
                VoiceScreen(
                    ui,
                    mic = { controller.micLevel.value },
                    voice = { controller.voiceLevel.value },
                    onMic = { controller.tapMic() },
                    onClose = { finish() },
                    onUnlock = { unlock() },
                    shell = shell,
                    onAnswer = { controller.answer(it) },
                    onAllowMic = { askMic.launch(Manifest.permission.RECORD_AUDIO) },
                )
            }
        }
        if (intent?.getBooleanExtra(EXTRA_UNLOCK, false) == true) unlock()
    }

    override fun onStart() {
        super.onStart()
        if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED && !asked) {
            asked = true
            askMic.launch(Manifest.permission.RECORD_AUDIO)
        } else controller.open()
    }

    override fun onStop() {
        // Off screen: stop listening; coming back wakes it again.
        controller.close()
        super.onStop()
    }

    override fun onDestroy() {
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
