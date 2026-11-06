package mv.obitech.crundi.voice

import android.content.Context
import android.media.AudioDeviceInfo
import android.media.AudioManager
import android.os.Build
import android.util.Log

/**
 * A voice session's audio as a speakerphone call ([CruAudio.Route.Call]):
 * the phone in communication mode with the built-in speaker as the call
 * device, so Cru's voice, played as call audio, is the far end the echo
 * canceller on the call-mode microphone takes out. Put back as it was when
 * the session ends. When the speaker can't be made the call device the
 * route stays [CruAudio.Route.Media] (never the earpiece).
 */
class CallAudio(private val ctx: Context) {
    private val am = ctx.getSystemService(AudioManager::class.java)
    private var savedMode: Int? = null
    private var savedVolume: Int? = null

    var route: CruAudio.Route = CruAudio.Route.Media
        private set

    fun start(): CruAudio.Route {
        if (route == CruAudio.Route.Call) return route
        val outputs = runCatching { am.getDevices(AudioManager.GET_DEVICES_OUTPUTS).map { it.type } }.getOrDefault(emptyList())
        if (CruAudio.route(Build.VERSION.SDK_INT, outputs) != CruAudio.Route.Call || Build.VERSION.SDK_INT < Build.VERSION_CODES.S) return route
        try {
            val speaker = am.availableCommunicationDevices.firstOrNull { it.type == AudioDeviceInfo.TYPE_BUILTIN_SPEAKER } ?: return route
            savedMode = am.mode
            am.mode = AudioManager.MODE_IN_COMMUNICATION
            if (!am.setCommunicationDevice(speaker)) { restore(); return route }
            // Call audio has its own volume: not quieter than the media volume was.
            val max = am.getStreamMaxVolume(AudioManager.STREAM_VOICE_CALL)
            val cur = am.getStreamVolume(AudioManager.STREAM_VOICE_CALL)
            val media = am.getStreamVolume(AudioManager.STREAM_MUSIC).toFloat() / maxOf(1, am.getStreamMaxVolume(AudioManager.STREAM_MUSIC))
            val want = CruAudio.callVolume(cur, max, media)
            if (want != cur) { savedVolume = cur; runCatching { am.setStreamVolume(AudioManager.STREAM_VOICE_CALL, want, 0) } }
            route = CruAudio.Route.Call
        } catch (e: Exception) {
            Log.w(TAG, "call audio: ${e.message}")
            restore()
        }
        return route
    }

    fun stop() {
        if (route != CruAudio.Route.Call && savedMode == null) return
        restore()
    }

    private fun restore() {
        runCatching { if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) am.clearCommunicationDevice() }
        savedVolume?.let { v -> runCatching { am.setStreamVolume(AudioManager.STREAM_VOICE_CALL, v, 0) } }
        savedMode?.let { m -> runCatching { am.mode = m } }
        savedVolume = null; savedMode = null
        route = CruAudio.Route.Media
    }

    private companion object { const val TAG = "CruCallAudio" }
}
