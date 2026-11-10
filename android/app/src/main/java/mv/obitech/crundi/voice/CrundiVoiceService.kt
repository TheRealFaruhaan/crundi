package mv.obitech.crundi.voice

import android.app.KeyguardManager
import android.content.Context
import android.content.Intent
import android.graphics.Color
import android.os.Bundle
import android.service.voice.VoiceInteractionService
import android.service.voice.VoiceInteractionSession
import android.service.voice.VoiceInteractionSessionService
import android.speech.RecognitionService
import android.speech.SpeechRecognizer
import android.view.View
import androidx.compose.runtime.getValue
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.platform.ViewCompositionStrategy
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.setViewTreeLifecycleOwner
import androidx.savedstate.SavedStateRegistry
import androidx.savedstate.SavedStateRegistryController
import androidx.savedstate.SavedStateRegistryOwner
import androidx.savedstate.setViewTreeSavedStateRegistryOwner
import mv.obitech.crundi.ui.theme.CrundiTheme

/**
 * Trigger mode A (D33): Crundi as the phone's digital assistant app. Holding
 * the side key (Samsung: Side button > Press and hold > Digital assistant),
 * the assist gesture or long-pressing home opens [CrundiSession] over
 * whatever is on screen, the lock screen included.
 *
 * Being the default assistant hands "Hey Google" to Crundi too, which is
 * why mode B exists (VoiceActivity); App settings explains both.
 */
class CrundiVoiceService : VoiceInteractionService() {
    override fun onReady() {
        super.onReady()
        current = this
    }

    override fun onShutdown() {
        if (current === this) current = null
        super.onShutdown()
    }

    companion object {
        @Volatile private var current: CrundiVoiceService? = null

        /**
         * "Hey Cru" heard in the background (Phase 6): as the phone's
         * assistant Crundi may show its session as any assistant does on its
         * hotword. False when Crundi isn't the assistant (mode B).
         */
        fun show(): Boolean {
            val s = current ?: return false
            return runCatching { s.showSession(Bundle(), 0); true }.getOrDefault(false)
        }
    }
}

class CrundiSessionService : VoiceInteractionSessionService() {
    override fun onNewSession(args: Bundle?): VoiceInteractionSession = CrundiSession(this)
}

/**
 * The assistant overlay. One [VoiceController] per showing: it opens when
 * the overlay is shown and closes when it is hidden (or goes quiet).
 */
class CrundiSession(ctx: Context) : VoiceInteractionSession(ctx) {
    private val owner = SessionOwner()
    private var controller: VoiceController? = null
    private var view: ComposeView? = null

    private fun locked() = context.getSystemService(KeyguardManager::class.java)?.isKeyguardLocked == true

    override fun onCreate() {
        super.onCreate()
        owner.create()
    }

    override fun onCreateContentView(): View {
        val v = ComposeView(context).apply {
            setViewTreeLifecycleOwner(owner)
            setViewTreeSavedStateRegistryOwner(owner)
            setViewCompositionStrategy(ViewCompositionStrategy.DisposeOnDetachedFromWindow)
            setBackgroundColor(Color.TRANSPARENT)
            // The screen stays on while Crundi is open: no sleeping mid-conversation.
            keepScreenOn = true
        }
        view = v
        return v
    }

    override fun onShow(args: Bundle?, showFlags: Int) {
        super.onShow(args, showFlags)
        // The screen stays on while the overlay is open: on the window itself,
        // which Android honours for an assistant's window (the view's flag alone is not enough).
        runCatching { window?.window?.addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON) }
        // Keyboard mode: the screen lays itself out round the soft keyboard (safeDrawing
        // includes it), so the window must not also be panned or resized for it.
        runCatching {
            window?.window?.let { w ->
                androidx.core.view.WindowCompat.setDecorFitsSystemWindows(w, false)
                w.setSoftInputMode(android.view.WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE)
            }
        }
        owner.resume()
        val c = controller ?: VoiceController(context, "side key", ::locked, onFinish = { hide() }).also { controller = it }
        view?.setContent {
            CrundiTheme {
                val ui by c.ui.collectAsStateWithLifecycle()
                val shell = c.shell?.collectAsStateWithLifecycle()?.value
                VoiceScreen(
                    ui,
                    mic = { c.micLevel.value },
                    voice = { c.voiceLevel.value },
                    onMic = { c.tapMic() },
                    onClose = { hide() },
                    onUnlock = { openActivity(unlock = true) },
                    shell = shell,
                    onAnswer = { c.answer(it) },
                    onCancel = { c.cancelWords() },
                    onAllowMic = { openActivity(unlock = false) },
                    more = VoiceMore(
                        onCompact = { c.compact() },
                        onChat = { c.setChat(it) },
                        onMoreHistory = { c.moreHistory() },
                        onKeyboard = { c.setKeyboard(it) },
                        onSend = { c.send(it) },
                        onTyping = { c.typing() },
                        onSay = { c.sayAgain(it) },
                        onStopSaying = { c.stopSaying() },
                        onDropLive = { c.dropLive(it) },
                        onVoiceOff = { c.setVoiceOff(it) },
                        live = c.live,
                    ),
                )
            }
        }
        c.open()
    }

    /**
     * The voice screen as an activity: it can ask for the microphone and for
     * an unlock, which an overlay can't. The conversation carries on there
     * (the server keeps it).
     */
    private fun openActivity(unlock: Boolean) {
        controller?.close()
        val i = Intent(context, VoiceActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            .putExtra(VoiceActivity.EXTRA_UNLOCK, unlock)
        if (android.os.Build.VERSION.SDK_INT >= 29) startAssistantActivity(i) else runCatching { context.startActivity(i) }
        hide()
    }

    override fun onHide() {
        controller?.close()
        owner.pause()
        super.onHide()
    }

    override fun onDestroy() {
        controller?.release(); controller = null
        owner.destroy()
        super.onDestroy()
    }
}

/** What Compose needs from a window that is not an activity. */
private class SessionOwner : LifecycleOwner, SavedStateRegistryOwner {
    private val registry = LifecycleRegistry(this)
    private val saved = SavedStateRegistryController.create(this)
    override val lifecycle: Lifecycle get() = registry
    override val savedStateRegistry: SavedStateRegistry get() = saved.savedStateRegistry

    fun create() { saved.performRestore(null); registry.currentState = Lifecycle.State.CREATED }
    fun resume() { registry.currentState = Lifecycle.State.RESUMED }
    fun pause() { if (registry.currentState.isAtLeast(Lifecycle.State.STARTED)) registry.currentState = Lifecycle.State.CREATED }
    fun destroy() { registry.currentState = Lifecycle.State.DESTROYED }
}

/**
 * Android insists that an assistant names a recognition service. Crundi
 * doesn't offer recognition to other apps (its own goes through the server,
 * D37), so this one refuses every request.
 */
class CrundiRecognitionService : RecognitionService() {
    override fun onStartListening(recognizerIntent: Intent?, listener: RecognitionService.Callback?) { listener?.error(SpeechRecognizer.ERROR_CLIENT) }
    override fun onCancel(listener: RecognitionService.Callback?) {}
    override fun onStopListening(listener: RecognitionService.Callback?) {}
}
