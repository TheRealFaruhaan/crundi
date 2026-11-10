package mv.obitech.crundi.voice

import android.content.Context
import android.media.AudioDeviceCallback
import android.media.AudioDeviceInfo
import android.media.AudioManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.Log

/**
 * A voice session's audio as a speakerphone call ([CruAudio.Route.Call]):
 * the phone in communication mode with the built-in speaker as the call
 * device, so Cru's voice, played as call audio, is the far end the echo
 * canceller on the call-mode microphone takes out. Put back as it was when
 * the last session lets go. When the speaker can't be made the call device
 * the route stays [CruAudio.Route.Media] (never the earpiece).
 *
 * 10 Nov 2026 (the owner: replies muffled and quiet while talking, clear
 * from the chat view): call audio that ends up on the earpiece path sounds
 * just like that, so the speaker is held, not just asked for once:
 *  - the playback is pinned to the speaker too ([speaker], set as each
 *    track's and player's preferred device);
 *  - Android moving the call device off the speaker is put right;
 *  - headphones or Bluetooth coming or going mid-session change the route
 *    ([onRoute]): in the ears it is plain media (no echo to cancel, full
 *    quality), on the loudspeaker call mode again;
 *  - Cru's screen and a chat's voice mode share one call mode ([CallAudioHub]),
 *    so one ending never takes it from the other;
 *  - the call volume starts a little above the media volume's share (call
 *    volume curves are quieter).
 */
class CallAudio(private val ctx: Context) {
    private var held = false

    /** The route changed while held (a headset in or out): play the next sound this way. */
    var onRoute: ((CruAudio.Route) -> Unit)? = null

    val route: CruAudio.Route get() = if (held) CallAudioHub.route else CruAudio.Route.Media

    /** The built-in speaker while the route is Call: the preferred device for Cru's playback. */
    val speaker: AudioDeviceInfo? get() = if (held && CallAudioHub.route == CruAudio.Route.Call) CallAudioHub.speaker else null

    fun start(): CruAudio.Route {
        if (!held) { held = true; CallAudioHub.acquire(ctx, this) }
        return route
    }

    fun stop() {
        if (!held) return
        held = false
        CallAudioHub.release(this)
    }

    internal fun routeChanged(r: CruAudio.Route) { if (held) runCatching { onRoute?.invoke(r) } }
}

/** The one call mode on the phone, held while any [CallAudio] holds it. Main thread. */
internal object CallAudioHub {
    private const val TAG = "CruCallAudio"
    private val main = Handler(Looper.getMainLooper())
    private val holders = LinkedHashSet<CallAudio>()
    private var am: AudioManager? = null
    private var savedMode: Int? = null
    private var savedVolume: Int? = null
    private var fixes = 0

    var route: CruAudio.Route = CruAudio.Route.Media
        private set
    var speaker: AudioDeviceInfo? = null
        private set

    private val devices = object : AudioDeviceCallback() {
        override fun onAudioDevicesAdded(added: Array<out AudioDeviceInfo>?) { main.post { apply() } }
        override fun onAudioDevicesRemoved(removed: Array<out AudioDeviceInfo>?) { main.post { apply() } }
    }
    private var commListener: Any? = null

    fun acquire(ctx: Context, h: CallAudio) {
        val first = holders.isEmpty()
        holders += h
        if (first) {
            am = ctx.applicationContext.getSystemService(AudioManager::class.java)
            runCatching { am?.registerAudioDeviceCallback(devices, main) }
            watchCallDevice()
        }
        apply()
    }

    fun release(h: CallAudio) {
        if (!holders.remove(h) || holders.isNotEmpty()) return
        runCatching { am?.unregisterAudioDeviceCallback(devices) }
        unwatchCallDevice()
        leave()
    }

    /** What the phone can play on now decides: call mode on the speaker, or plain media. */
    private fun apply() {
        val a = am ?: return
        if (holders.isEmpty()) return
        val outputs = runCatching { a.getDevices(AudioManager.GET_DEVICES_OUTPUTS).map { it.type } }.getOrDefault(emptyList())
        val want = CruAudio.route(Build.VERSION.SDK_INT, outputs)
        val was = route
        if (want == CruAudio.Route.Call) enter(a) else leave()
        if (route != was) for (h in holders.toList()) h.routeChanged(route)
    }

    private fun enter(a: AudioManager) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) return
        try {
            val spk = a.availableCommunicationDevices.firstOrNull { it.type == AudioDeviceInfo.TYPE_BUILTIN_SPEAKER } ?: run { leave(); return }
            if (route != CruAudio.Route.Call) {
                savedMode = a.mode
                a.mode = AudioManager.MODE_IN_COMMUNICATION
                fixes = 0
            }
            if (a.communicationDevice?.type != AudioDeviceInfo.TYPE_BUILTIN_SPEAKER && !a.setCommunicationDevice(spk)) { leave(); return }
            speaker = spk
            if (route != CruAudio.Route.Call) {
                val max = a.getStreamMaxVolume(AudioManager.STREAM_VOICE_CALL)
                val cur = a.getStreamVolume(AudioManager.STREAM_VOICE_CALL)
                val media = a.getStreamVolume(AudioManager.STREAM_MUSIC).toFloat() / maxOf(1, a.getStreamMaxVolume(AudioManager.STREAM_MUSIC))
                val want = CruAudio.callVolume(cur, max, media)
                if (want != cur) { savedVolume = cur; runCatching { a.setStreamVolume(AudioManager.STREAM_VOICE_CALL, want, 0) } }
            }
            route = CruAudio.Route.Call
        } catch (e: Exception) {
            Log.w(TAG, "call audio: ${e.message}")
            leave()
        }
    }

    private fun leave() {
        val a = am
        if (a != null && (route == CruAudio.Route.Call || savedMode != null)) {
            runCatching { if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) a.clearCommunicationDevice() }
            savedVolume?.let { v -> runCatching { a.setStreamVolume(AudioManager.STREAM_VOICE_CALL, v, 0) } }
            savedMode?.let { m -> runCatching { a.mode = m } }
        }
        savedVolume = null; savedMode = null; speaker = null
        route = CruAudio.Route.Media
    }

    /** Android moving the call device (to the earpiece, say) while Cru is on the loudspeaker: put it back. */
    private fun watchCallDevice() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S || commListener != null) return
        val a = am ?: return
        val l = AudioManager.OnCommunicationDeviceChangedListener { d ->
            if (route != CruAudio.Route.Call || d?.type == AudioDeviceInfo.TYPE_BUILTIN_SPEAKER) return@OnCommunicationDeviceChangedListener
            // Off the speaker: a headset is handled by apply(); anything else (the earpiece) is put right, a few times at most.
            if (d != null && d.type in CruAudio.HEADSETS) { apply(); return@OnCommunicationDeviceChangedListener }
            if (fixes++ < 5) speaker?.let { s -> runCatching { a.setCommunicationDevice(s) } }
        }
        runCatching { a.addOnCommunicationDeviceChangedListener({ main.post(it) }, l); commListener = l }
    }

    private fun unwatchCallDevice() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) return
        val l = commListener as? AudioManager.OnCommunicationDeviceChangedListener ?: return
        runCatching { am?.removeOnCommunicationDeviceChangedListener(l) }
        commListener = null
    }
}
