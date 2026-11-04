package mv.obitech.crundi.voicelab

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
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import mv.obitech.crundi.R
import kotlin.math.abs
import kotlin.math.max

/**
 * Check 0.1 (and the control for it): can a microphone foreground service be
 * started from where we are, and does it actually hear anything?
 *
 * It records three seconds and logs the loudest sample. A peak of 0 means
 * Android let the service start but silenced the microphone, which is what
 * happens when the start didn't count as "while in use".
 */
class LabMicService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val from = intent?.getStringExtra(EXTRA_FROM) ?: "?"
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel(CHANNEL, "Voice Lab microphone", NotificationManager.IMPORTANCE_LOW))
        val n: Notification = NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_stat)
            .setContentTitle("Voice Lab is listening")
            .setContentText("Recording 3 seconds to test the microphone")
            .setOngoing(true)
            .setSilent(true)
            .build()
        val type = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE else 0
        try {
            ServiceCompat.startForeground(this, NOTIFICATION_ID, n, type)
            LabLog.add(this, "[0.1] mic service started (from: $from)")
        } catch (e: Exception) {
            LabLog.add(this, "[0.1] mic service REFUSED startForeground (from: $from): ${e.javaClass.simpleName}: ${e.message}")
            stopSelf()
            return START_NOT_STICKY
        }
        Thread { record(from) }.start()
        return START_NOT_STICKY
    }

    @SuppressLint("MissingPermission")
    private fun record(from: String) {
        val rate = 16000
        try {
            val min = AudioRecord.getMinBufferSize(rate, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
            val rec = AudioRecord(MediaRecorder.AudioSource.VOICE_RECOGNITION, rate, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT, max(min, rate))
            if (rec.state != AudioRecord.STATE_INITIALIZED) {
                LabLog.add(this, "[0.1] AudioRecord did not initialise (from: $from); is the microphone permission granted?")
                rec.release()
                return
            }
            val buf = ShortArray(1024)
            var peak = 0
            val end = System.currentTimeMillis() + 3000
            rec.startRecording()
            while (System.currentTimeMillis() < end) {
                val got = rec.read(buf, 0, buf.size)
                for (i in 0 until max(got, 0)) peak = max(peak, abs(buf[i].toInt()))
            }
            rec.stop()
            rec.release()
            val verdict = if (peak == 0) "SILENCED (all zeros)" else "heard sound"
            LabLog.add(this, "[0.1] mic peak $peak / 32767 — $verdict (from: $from)")
        } catch (e: Exception) {
            LabLog.add(this, "[0.1] recording failed (from: $from): ${e.javaClass.simpleName}: ${e.message}")
        } finally {
            ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
            stopSelf()
        }
    }

    companion object {
        private const val CHANNEL = "voicelab"
        private const val NOTIFICATION_ID = 4101
        private const val EXTRA_FROM = "from"

        /** Starts the check; a refusal at the call site is logged too. */
        fun start(ctx: Context, from: String) {
            try {
                ContextCompat.startForegroundService(ctx, Intent(ctx, LabMicService::class.java).putExtra(EXTRA_FROM, from))
            } catch (e: Exception) {
                LabLog.add(ctx, "[0.1] mic service REFUSED at start (from: $from): ${e.javaClass.simpleName}: ${e.message}")
            }
        }
    }
}
