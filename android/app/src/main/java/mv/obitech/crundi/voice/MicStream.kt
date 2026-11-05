package mv.obitech.crundi.voice

import android.annotation.SuppressLint
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import android.media.audiofx.AcousticEchoCanceler
import android.media.audiofx.NoiseSuppressor
import android.os.Build
import android.os.IBinder
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import mv.obitech.crundi.R
import kotlin.math.sqrt

/**
 * The microphone, as 16-bit mono PCM at [rate] in 100 ms chunks, with the
 * phone's echo cancelling on so Crundi's own voice is mostly taken out
 * (that is what lets the phone notice Master talking over it).
 *
 * [onChunk] is called on the recording thread with the bytes (little-endian,
 * what Deepgram's linear16 expects) and the chunk's level, 0..1.
 */
class MicStream(private val rate: Int = RATE, private val onChunk: (bytes: ByteArray, level: Float) -> Unit) {
    @Volatile private var running = false
    private var thread: Thread? = null

    /** False when the microphone could not be opened (no permission, in use). */
    @SuppressLint("MissingPermission")
    fun start(): Boolean {
        if (running) return true
        val min = AudioRecord.getMinBufferSize(rate, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
        if (min <= 0) return false
        val rec = try {
            AudioRecord(MediaRecorder.AudioSource.VOICE_COMMUNICATION, rate, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT, maxOf(min, rate))
        } catch (e: Exception) { Log.w(TAG, "AudioRecord: ${e.message}"); return false }
        if (rec.state != AudioRecord.STATE_INITIALIZED) { rec.release(); return false }
        val aec = runCatching { if (AcousticEchoCanceler.isAvailable()) AcousticEchoCanceler.create(rec.audioSessionId)?.apply { enabled = true } else null }.getOrNull()
        val ns = runCatching { if (NoiseSuppressor.isAvailable()) NoiseSuppressor.create(rec.audioSessionId)?.apply { enabled = true } else null }.getOrNull()
        running = true
        thread = Thread({
            val samples = ShortArray(rate / 10)
            try {
                rec.startRecording()
                while (running) {
                    val got = rec.read(samples, 0, samples.size)
                    if (got <= 0) { if (got < 0) break else continue }
                    val bytes = ByteArray(got * 2)
                    var sum = 0.0
                    for (i in 0 until got) {
                        val v = samples[i].toInt()
                        bytes[2 * i] = (v and 0xff).toByte()
                        bytes[2 * i + 1] = (v shr 8 and 0xff).toByte()
                        sum += v.toDouble() * v
                    }
                    onChunk(bytes, level(sqrt(sum / got)))
                }
            } catch (e: Exception) {
                Log.w(TAG, "recording stopped: ${e.message}")
            } finally {
                runCatching { rec.stop() }
                rec.release(); aec?.release(); ns?.release()
                running = false
            }
        }, "crundi-mic").apply { start() }
        return true
    }

    fun stop() {
        running = false
        thread = null
    }

    companion object {
        const val RATE = 16000
        private const val TAG = "CrundiMic"

        /** RMS of 16-bit samples to a 0..1 level that moves with speech (square-root scale). */
        fun level(rms: Double): Float = sqrt((rms / 32768.0).coerceIn(0.0, 1.0)).toFloat()
    }
}

/**
 * Holds the microphone in the foreground while a voice session is open, so
 * Android lets the app record with the screen locked (Phase 0: started from
 * the side-key overlay it records locked and unlocked). Shows the ongoing
 * "Crundi is listening" notification; does nothing else.
 */
class VoiceMicService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel(CHANNEL, "Voice assistant listening", NotificationManager.IMPORTANCE_LOW).apply { setShowBadge(false) })
        val n: Notification = NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_stat)
            .setContentTitle("Crundi is listening")
            .setContentText("While the voice assistant is open")
            .setOngoing(true)
            .setSilent(true)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .build()
        val type = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE else 0
        try {
            ServiceCompat.startForeground(this, NOTIFICATION_ID, n, type)
        } catch (e: Exception) {
            Log.w("CrundiVoice", "microphone service refused: ${e.message}")
            stopSelf()
        }
        return START_NOT_STICKY
    }

    companion object {
        private const val CHANNEL = "voice-listening"
        private const val NOTIFICATION_ID = 4201

        fun start(ctx: Context) {
            runCatching { ContextCompat.startForegroundService(ctx, Intent(ctx, VoiceMicService::class.java)) }
                .onFailure { Log.w("CrundiVoice", "microphone service not started: ${it.message}") }
        }

        fun stop(ctx: Context) { runCatching { ctx.stopService(Intent(ctx, VoiceMicService::class.java)) } }
    }
}

/** This phone's own voice-assistant settings (the server keeps the rest). */
object VoicePrefs {
    private fun p(ctx: Context) = ctx.getSharedPreferences("crundi", Context.MODE_PRIVATE)

    /** On unless turned off in App settings > Voice assistant. */
    fun enabled(ctx: Context) = p(ctx).getBoolean("voice.enabled", true)
    fun setEnabled(ctx: Context, on: Boolean) = p(ctx).edit().putBoolean("voice.enabled", on).apply()

    /** The phone voice to use when no Deepgram voice is chosen (D34); "" = the default order. */
    fun deviceVoice(ctx: Context) = p(ctx).getString("voice.deviceVoice", "") ?: ""
    fun setDeviceVoice(ctx: Context, name: String) = p(ctx).edit().putString("voice.deviceVoice", name).apply()

    /** Seconds of quiet before the voice screen goes back to sleep. */
    fun silenceSeconds(ctx: Context) = p(ctx).getInt("voice.silence", 30)

    // ─── "Hey Crundi" (Phase 6) ───

    /** D3: listen for "Hey Crundi" while the app is in front. Off by default. */
    fun wakeInApp(ctx: Context) = p(ctx).getBoolean("voice.wake.inApp", false)
    fun setWakeInApp(ctx: Context, on: Boolean) = p(ctx).edit().putBoolean("voice.wake.inApp", on).apply()

    /** D7: the settings toggle's duration, minutes; [HeyCrundiPlan.UNTIL_OFF] = until turned off. Default 30. */
    fun wakeMinutes(ctx: Context) = HeyCrundiPlan.choice(p(ctx).getInt("voice.wake.minutes", HeyCrundiPlan.DEFAULT_MINUTES))
    fun setWakeMinutes(ctx: Context, minutes: Int) = p(ctx).edit().putInt("voice.wake.minutes", HeyCrundiPlan.choice(minutes)).apply()

    /** Background listening is meant to be on (the service may have been stopped by Android). */
    fun wakeOn(ctx: Context) = p(ctx).getBoolean("voice.wake.on", false)

    /** When background listening ends, epoch ms; null = until turned off (or not on). */
    fun wakeDeadline(ctx: Context): Long? = p(ctx).getLong("voice.wake.deadline", -1L).takeIf { it > 0 }

    fun setWakeOn(ctx: Context, on: Boolean, deadline: Long?) {
        p(ctx).edit().putBoolean("voice.wake.on", on).putLong("voice.wake.deadline", if (on) deadline ?: -1L else -1L).commit()
    }
}
