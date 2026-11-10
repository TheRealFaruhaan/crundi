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
open class VoiceActivity : ComponentActivity() {
    /** Cru in its own floating bubble ([CruBubbleActivity]): no lock screen, and closing is the bubble's to do. */
    protected open val inBubble: Boolean = false
    private lateinit var controller: VoiceController
    private val askMic = registerForActivityResult(ActivityResultContracts.RequestPermission()) { controller.open() }

    // ─── files for Cru: the attach button's three ways in ───
    /** A picker or the camera is open over this screen: going out of view is not the end of the session. */
    private var picking = false
    /** Where the camera is saving the photo. */
    private var photo: android.net.Uri? = null
    private val takePhoto = registerForActivityResult(ActivityResultContracts.TakePicture()) { ok ->
        picking = false
        val u = photo; photo = null
        if (u == null) return@registerForActivityResult
        if (ok) controller.attach(listOf(u), deleteAfter = true) else CruFiles.delete(this, u)
    }
    /** The app declares the camera (pairing scan), so Android wants it granted before the camera app may be used. */
    private val askCamera = registerForActivityResult(ActivityResultContracts.RequestPermission()) { ok ->
        if (ok) startCamera() else {
            picking = false
            android.widget.Toast.makeText(this, "Cru needs the camera to take a photo", android.widget.Toast.LENGTH_SHORT).show()
        }
    }
    private val pickMedia = registerForActivityResult(ActivityResultContracts.PickMultipleVisualMedia(10)) { uris ->
        picking = false
        controller.attach(uris)
    }
    private val pickFiles = registerForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        picking = false
        controller.attach(uris)
    }

    /** The attach button's choice. Over the lock screen the phone is unlocked first: pickers can't show over it. */
    private fun pick(source: AttachSource) {
        if (locked()) {
            val km = getSystemService(KeyguardManager::class.java) ?: return
            km.requestDismissKeyguard(this, object : KeyguardManager.KeyguardDismissCallback() {
                override fun onDismissSucceeded() { controller.unlocked(); pick(source) }
            })
            return
        }
        picking = true
        try {
            when (source) {
                AttachSource.Camera ->
                    if (checkSelfPermission(Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) startCamera()
                    else askCamera.launch(Manifest.permission.CAMERA)
                AttachSource.Media -> pickMedia.launch(androidx.activity.result.PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageAndVideo))
                AttachSource.Files -> pickFiles.launch(arrayOf("*/*"))
            }
        } catch (e: android.content.ActivityNotFoundException) {
            picking = false
            android.widget.Toast.makeText(this, "No app on this phone can do that", android.widget.Toast.LENGTH_SHORT).show()
        }
    }

    private fun startCamera() {
        val u = runCatching { CruFiles.cameraFile(this) }.getOrNull() ?: run { picking = false; return }
        photo = u
        try { takePhoto.launch(u) } catch (e: android.content.ActivityNotFoundException) {
            picking = false; photo = null; CruFiles.delete(this, u)
            android.widget.Toast.makeText(this, "No camera app on this phone", android.widget.Toast.LENGTH_SHORT).show()
        }
    }

    /** Shared to Cru (the share sheet's "Cru"): files and pictures attached, text and links dropped in. */
    private fun shared(intent: android.content.Intent?) {
        val i = intent ?: return
        if (i.action != android.content.Intent.ACTION_SEND && i.action != android.content.Intent.ACTION_SEND_MULTIPLE) return
        val uris = ArrayList<android.net.Uri>()
        @Suppress("DEPRECATION")
        if (i.action == android.content.Intent.ACTION_SEND_MULTIPLE) i.getParcelableArrayListExtra<android.net.Uri>(android.content.Intent.EXTRA_STREAM)?.let { uris += it }
        else (i.getParcelableExtra<android.os.Parcelable>(android.content.Intent.EXTRA_STREAM) as? android.net.Uri)?.let { uris += it }
        // Some apps put what they share only in the clip.
        if (uris.isEmpty()) i.clipData?.let { c -> for (n in 0 until c.itemCount) c.getItemAt(n).uri?.let { uris += it } }
        val text = if (uris.isEmpty()) i.getCharSequenceExtra(android.content.Intent.EXTRA_TEXT)?.toString().orEmpty() else ""
        val subject = i.getStringExtra(android.content.Intent.EXTRA_SUBJECT).orEmpty()
        if (uris.isNotEmpty()) controller.attach(uris)
        else if (text.isNotBlank()) controller.attachText(if (subject.isNotBlank() && !text.contains(subject)) "$subject: $text" else text)
        // Handled once: turning the phone or coming back does not attach it again.
        i.action = null
    }

    private val handler = android.os.Handler(android.os.Looper.getMainLooper())
    /** The session is asleep and the screen open: listening for "Hey Cru" only. */
    private var waiting by mutableStateOf(false)
    private var inFront = false
    private val letSleep = Runnable { keepOn(false) }
    /** After a short silence (the server's screenCloseSeconds): the voice screen closes, even on a phone that stays awake; "Hey Cru" opens it again. */
    private val closeIdle = Runnable { finish() }

    private fun keepOn(on: Boolean) {
        val f = android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON
        if (on) window.addFlags(f) else window.clearFlags(f)
    }

    /** The microphone's time is up: listen for the wake word, and let the screen sleep later. */
    private fun asleep() {
        // Asleep while a picker is open: the screen stays for the file coming back.
        if (!inFront && picking) return
        if (!inFront) { finish(); return }
        waiting = true
        // Keyboard mode: no microphone at all, "Hey Cru" included; typing wakes it.
        if (!controller.ui.value.keyboard) HeyCrundi.screenWaiting { wake() }
        handler.removeCallbacks(letSleep)
        handler.postDelayed(letSleep, VoicePrefs.screenAwakeMinutes(this) * 60_000L)
        // Silent since the last thing said, which was the microphone's time ago. An older server sets none.
        closeLater(VoicePrefs.silenceSeconds(this) * 1000L)
    }

    /** The screen closes after the server's screenCloseSeconds of silence, [already] ms of which have passed; typing starts it again. */
    private fun closeLater(already: Long = 0L) {
        val close = controller.ui.value.spend?.screenCloseSeconds ?: 0
        handler.removeCallbacks(closeIdle)
        if (close > 0) handler.postDelayed(closeIdle, (close * 1000L - already).coerceAtLeast(0L))
    }

    /** "Hey Cru", or the mic tapped: listening again. */
    private fun wake() {
        handler.removeCallbacks(letSleep)
        handler.removeCallbacks(closeIdle)
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
        if (intent?.getBooleanExtra(EXTRA_CHAT, false) == true) VoicePrefs.setChatView(this, true)
        controller = VoiceController(this, if (inBubble) "Cru bubble" else "Cru", ::locked, onFinish = { asleep() }, onDismiss = { finish() })
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
                    onCancel = { controller.cancelWords() },
                    onAllowMic = { askMic.launch(Manifest.permission.RECORD_AUDIO) },
                    more = VoiceMore(
                        onCompact = { controller.compact() },
                        onChat = { controller.setChat(it) },
                        onMoreHistory = { controller.moreHistory() },
                        onKeyboard = { on -> if (waiting) wake(); controller.setKeyboard(on) },
                        onSend = { t -> if (waiting) wake(); controller.send(t) },
                        onTyping = { if (waiting) closeLater() else controller.typing() },
                        live = controller.live,
                        onAttach = { pick(it) },
                        onRemoveAttachment = { controller.removeAttachment(it) },
                        attachmentThumb = { controller.attachmentThumb(it) },
                        onSay = { controller.sayAgain(it) },
                        onStopSaying = { controller.stopSaying() },
                        // Float Cru as its own bubble (Android 11+, bubbles allowed); not from inside one.
                        // Offered whenever Android can bubble (11+): if bubbles are off, the button says how to turn them on.
                        // The voice screen goes only once Android has really made a bubble of it.
                        onBubble = if (!inBubble && android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.R) ({ CruBubble.request(this) { finish() } }) else null,
                    ),
                )
            }
        }
        if (intent?.getBooleanExtra(EXTRA_UNLOCK, false) == true) unlock()
        if (savedInstanceState == null) shared(intent)
        // Opened by "Hey Cru" from the background: its "tap to talk" is done.
        HeyCrundi.cancelWakeNotification(this)
    }

    override fun onStart() {
        super.onStart()
        inFront = true
        // Keyboard mode chosen on this phone: it needs no microphone, so none is asked for.
        if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED && !asked && VoicePrefs.keyboard(this) != true) {
            asked = true
            askMic.launch(Manifest.permission.RECORD_AUDIO)
        } else controller.open()
    }

    override fun onStop() {
        inFront = false
        // A picker or the camera over it: the session carries on, and the screen
        // stays (it is not closed for being quiet meanwhile).
        if (picking) {
            handler.removeCallbacks(letSleep)
            handler.removeCallbacks(closeIdle)
            if (waiting) { HeyCrundi.screenWaiting(null); waiting = false }
            super.onStop()
            return
        }
        // Off screen: stop listening; coming back wakes it again. Asleep, and the
        // screen went off (or the owner left): it's done.
        val wasWaiting = waiting
        handler.removeCallbacks(letSleep)
        handler.removeCallbacks(closeIdle)
        HeyCrundi.screenWaiting(null)
        waiting = false
        controller.close()
        // A bubble folded away keeps its window: tapping it again carries on.
        if (wasWaiting && !inBubble) finish()
        super.onStop()
    }

    override fun onDestroy() {
        handler.removeCallbacks(letSleep)
        handler.removeCallbacks(closeIdle)
        if (waiting) HeyCrundi.screenWaiting(null)
        controller.release()
        super.onDestroy()
    }

    private var asked = false

    override fun onNewIntent(intent: android.content.Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        if (intent.getBooleanExtra(EXTRA_CHAT, false)) controller.setChat(true)
        shared(intent)
    }

    private fun unlock() {
        val km = getSystemService(KeyguardManager::class.java) ?: return
        if (!km.isKeyguardLocked) { controller.unlocked(); return }
        km.requestDismissKeyguard(this, object : KeyguardManager.KeyguardDismissCallback() {
            override fun onDismissSucceeded() { controller.unlocked() }
        })
    }

    companion object {
        const val EXTRA_UNLOCK = "unlock"
        /** Opened for Cru's unheard reports: straight into the chat view. */
        const val EXTRA_CHAT = "cruChat"
    }
}
