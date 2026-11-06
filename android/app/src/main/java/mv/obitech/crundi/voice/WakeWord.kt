package mv.obitech.crundi.voice

import android.Manifest
import android.annotation.SuppressLint
import android.app.Activity
import android.app.Application
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import com.k2fsa.sherpa.onnx.FeatureConfig
import com.k2fsa.sherpa.onnx.KeywordSpotter
import com.k2fsa.sherpa.onnx.KeywordSpotterConfig
import com.k2fsa.sherpa.onnx.OnlineModelConfig
import com.k2fsa.sherpa.onnx.OnlineStream
import com.k2fsa.sherpa.onnx.OnlineTransducerModelConfig
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import mv.obitech.crundi.R
import mv.obitech.crundi.UrgentAlerts

/**
 * The on-phone keyword spotter for "Hey Cru" (sherpa-onnx, Apache-2.0):
 * a 3.3M-parameter streaming zipformer trained on GigaSpeech, open
 * vocabulary, with the keywords in assets/kws/keywords.txt. Nothing leaves
 * the phone: the microphone goes to the server only once a voice session
 * has opened.
 *
 * The model and the native library are not in the repository: the build
 * fetches them from sherpa-onnx's GitHub releases, pinned by SHA-256
 * (app/build.gradle.kts, fetchWakeWord). On a phone whose processor the
 * library wasn't built for, [load] returns null and "Hey Cru" says it
 * isn't available.
 */
class WakeWordEngine private constructor(private val kws: KeywordSpotter) {
    private var stream: OnlineStream = kws.createStream(CruWake.line(0.2))

    /** Listen with the owner's tuned keyword line (their profile's threshold). */
    fun use(line: String) {
        runCatching { stream.release() }
        stream = kws.createStream(line)
    }

    /** 16 kHz mono floats in -1..1. True when the wake word was just heard. */
    fun feed(samples: FloatArray): Boolean = feed(stream, samples)

    private fun feed(s: OnlineStream, samples: FloatArray): Boolean {
        s.acceptWaveform(samples, RATE)
        var heard = false
        while (kws.isReady(s)) {
            kws.decode(s)
            if (kws.getResult(s).keyword.isNotBlank()) {
                kws.reset(s)
                heard = true
            }
        }
        return heard
    }

    /** Training: would [line] hear "Hey Cru" in [clip]? On a stream of its own; the live one is untouched. */
    fun hears(line: String, clip: FloatArray): Boolean {
        val s = kws.createStream(line)
        try {
            var hit = false
            var i = 0
            while (i < clip.size) { if (feed(s, clip.copyOfRange(i, minOf(clip.size, i + 1600)))) hit = true; i += 1600 }
            return hit
        } finally { runCatching { s.release() } }
    }

    fun release() {
        runCatching { stream.release() }
        runCatching { kws.release() }
    }

    companion object {
        const val RATE = 16000
        private const val DIR = "kws"

        /** Null when the native library or the model is missing on this phone. */
        fun load(ctx: Context): WakeWordEngine? = try {
            val cfg = KeywordSpotterConfig(
                featConfig = FeatureConfig(sampleRate = RATE, featureDim = 80),
                modelConfig = OnlineModelConfig(
                    transducer = OnlineTransducerModelConfig(
                        encoder = "$DIR/encoder.int8.onnx",
                        decoder = "$DIR/decoder.int8.onnx",
                        joiner = "$DIR/joiner.int8.onnx",
                    ),
                    tokens = "$DIR/tokens.txt",
                    numThreads = 1,
                    modelType = "zipformer2",
                ),
                maxActivePaths = 4,
                keywordsFile = "$DIR/keywords.txt",
                // Each stream's keyword line carries its own boost and threshold
                // (the owner's profile, CruWake.line); these are only the defaults.
                keywordsScore = 1.5f,
                keywordsThreshold = 0.2f,
                numTrailingBlanks = 1,
            )
            WakeWordEngine(KeywordSpotter(ctx.assets, cfg))
        } catch (e: Throwable) {
            // UnsatisfiedLinkError (no library for this processor), a missing asset.
            Log.w("CrundiWake", "keyword spotter unavailable: $e")
            null
        }

        /** Whether this build carries the library for this phone's processor (cheap; doesn't load it). */
        fun supported(ctx: Context): Boolean {
            val dir = ctx.applicationInfo.nativeLibraryDir ?: return false
            return java.io.File(dir, "libsherpa-onnx-jni.so").exists() ||
                // Not extracted (uncompressed in the APK): trust the ABI list.
                Build.SUPPORTED_ABIS.contains("arm64-v8a")
        }
    }
}

/**
 * Who listens for "Hey Cru", for the whole app. One microphone owner at
 * a time ([HeyCrundiPlan.listener]): a voice session pauses the wake word
 * ([sessionOpened] / [sessionClosed]); background listening
 * ([HeyCrundiService]) covers the screen being off; in-app listening covers
 * the app being in front (D3). Main thread, except the recording thread.
 */
@SuppressLint("StaticFieldLeak") // the application context only
object HeyCrundi {
    private const val TAG = "CrundiWake"
    private lateinit var app: Context
    private val main = Handler(Looper.getMainLooper())

    private var started = 0
    private var situation = HeyCrundiPlan.Situation()
    private var engine: WakeWordEngine? = null
    private var engineTried = false
    private var thread: Thread? = null
    @Volatile private var running = false
    private var lastHit = 0L

    private val _listening = MutableStateFlow(HeyCrundiPlan.Listener.None)
    /** Who is listening now, for the settings screen. */
    val listening: StateFlow<HeyCrundiPlan.Listener> = _listening

    private val endHandOver = Runnable { update { it.copy(handingOver = false) } }

    /** From CrundiApp: tracks whether Crundi is in front. */
    fun init(application: Application) {
        app = application
        situation = situation.copy(inAppOn = VoicePrefs.wakeInApp(app))
        application.registerActivityLifecycleCallbacks(object : Application.ActivityLifecycleCallbacks {
            override fun onActivityStarted(a: Activity) { if (started++ == 0) inFront(true) }
            override fun onActivityStopped(a: Activity) { if (--started <= 0) { started = 0; inFront(false) } }
            override fun onActivityCreated(a: Activity, b: Bundle?) {}
            override fun onActivityResumed(a: Activity) {}
            override fun onActivityPaused(a: Activity) {}
            override fun onActivitySaveInstanceState(a: Activity, b: Bundle) {}
            override fun onActivityDestroyed(a: Activity) {}
        })
    }

    private fun ready() = ::app.isInitialized

    private fun inFront(front: Boolean) {
        update { it.copy(appInFront = front) }
        // Back in front with background listening still meant to be on (Android
        // stopped it): Crundi may take the microphone again now, so it does.
        if (front && HeyCrundiService.shouldResume(app)) HeyCrundiService.resume(app)
    }

    /** In-app listening turned on or off in settings. */
    fun setInApp(on: Boolean) {
        if (!ready()) return
        VoicePrefs.setWakeInApp(app, on)
        update { it.copy(inAppOn = on) }
    }

    fun setBackground(on: Boolean) = update { it.copy(backgroundOn = on) }

    /** A voice session took the microphone: the wake word lets go of it first (synchronously). */
    fun sessionOpened() {
        main.removeCallbacks(endHandOver)
        update { it.copy(sessionOpen = true, handingOver = false) }
    }

    fun sessionClosed() = update { it.copy(sessionOpen = false) }

    private var screenWake: (() -> Unit)? = null

    /**
     * The voice screen is open and asleep: listen for "Hey Cru" while it
     * is, even with both settings off, and call [onWake] on hearing it. Null
     * when it wakes or goes away.
     */
    fun screenWaiting(onWake: (() -> Unit)?) {
        screenWake = onWake
        update { it.copy(screenWaiting = onWake != null, handingOver = false) }
    }

    /** The microphone permission may have changed. */
    fun refresh() = update { it }

    /** Training is recording: the wake word lets go of the microphone (synchronously) until it's done. */
    fun training(on: Boolean) = update { it.copy(training = on) }

    /** A profile was made or forgotten: listen again with it (or not at all). */
    fun profileChanged() { stopMic(); update { it } }

    /** The keyword spotter, for training; null when this phone can't run it. Main thread. */
    fun engineForTraining(): WakeWordEngine? {
        if (!ready()) return null
        if (!engineTried) { engineTried = true; engine = WakeWordEngine.load(app) }
        return engine
    }

    private fun update(change: (HeyCrundiPlan.Situation) -> HeyCrundiPlan.Situation) {
        if (!ready()) return
        if (Looper.myLooper() != Looper.getMainLooper()) { main.post { update(change) }; return }
        val mic = ContextCompat.checkSelfPermission(app, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED
        situation = change(situation).copy(micAllowed = mic, engineReady = !engineTried || engine != null, trained = WakeProfiles.trained(app))
        val who = HeyCrundiPlan.listener(situation)
        if (who == HeyCrundiPlan.Listener.None) stopMic() else startMic()
        _listening.value = if (running) who else HeyCrundiPlan.Listener.None
    }

    // ─── the microphone ───

    @SuppressLint("MissingPermission")
    private fun startMic() {
        if (running) return
        if (!engineTried) { engineTried = true; engine = WakeWordEngine.load(app) }
        val kws = engine ?: run { situation = situation.copy(engineReady = false); return }
        // Never without the owner's voice: "Hey Cru" is trained first.
        val profile = WakeProfiles.get(app) ?: return
        kws.use(profile.keywordLine())
        val min = AudioRecord.getMinBufferSize(WakeWordEngine.RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
        if (min <= 0) return
        // VOICE_RECOGNITION: the raw microphone tuned for speech, no call processing.
        val rec = try {
            AudioRecord(MediaRecorder.AudioSource.VOICE_RECOGNITION, WakeWordEngine.RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT, maxOf(min, WakeWordEngine.RATE))
        } catch (e: Exception) { Log.w(TAG, "AudioRecord: ${e.message}"); return }
        if (rec.state != AudioRecord.STATE_INITIALIZED) { rec.release(); return }
        running = true
        thread = Thread({
            val shorts = ShortArray(WakeWordEngine.RATE / 10)
            val floats = FloatArray(shorts.size)
            // The last two seconds, for the voice match when the spotter fires.
            val keep = (WakeWordEngine.RATE * CruWake.VERIFY_SECONDS).toInt()
            val ring = FloatArray(keep)
            var at = 0
            var filled = 0
            // The owner's own phrase: each burst of speech on its own, matched against the recordings.
            val bursts = CruWake.Segmenter()
            try {
                rec.startRecording()
                while (running) {
                    val got = rec.read(shorts, 0, shorts.size)
                    if (got < 0) break
                    if (got == 0) continue
                    val chunk = if (got == floats.size) floats else FloatArray(got)
                    for (i in 0 until got) chunk[i] = shorts[i] / 32768f
                    for (i in 0 until got) { ring[at] = chunk[i]; at = (at + 1) % keep }
                    filled = minOf(keep, filled + got)
                    val spotted = kws.feed(chunk)
                    val burst = bursts.accept(chunk)
                    val phrase = burst?.let { profile.matchPhrase(it) }
                    if (phrase?.ok == true && running) { Log.i(TAG, "Hey Cru: the owner's phrase (${phrase.score})"); main.post { heard() } }
                    else if (spotted && running) {
                        val recent = FloatArray(filled) { ring[(at - filled + it + keep) % keep] }
                        val v = profile.verify(recent)
                        if (v.ok) main.post { heard() } else Log.i(TAG, "Hey Cru heard, but not like the owner's (${v.score} > ${profile.limit}): ignored")
                    }
                    HeyCrundiService.tick()
                }
            } catch (e: Exception) {
                Log.w(TAG, "wake word recording stopped: ${e.message}")
            } finally {
                runCatching { rec.stop() }
                rec.release()
            }
        }, "crundi-wake").apply { start() }
        Log.i(TAG, "listening for Hey Cru")
    }

    /** Stops and waits for the recording to let go, so the next owner gets a free microphone. */
    private fun stopMic() {
        if (!running) return
        running = false
        thread?.let { runCatching { it.join(400) } }
        thread = null
    }

    private fun heard() {
        val now = System.currentTimeMillis()
        if (now - lastHit < HeyCrundiPlan.COOLDOWN_MS || HeyCrundiPlan.listener(situation) == HeyCrundiPlan.Listener.None) return
        lastHit = now
        Log.i(TAG, "heard Hey Cru")
        // Free the microphone for the voice session; listen again if none opens.
        update { it.copy(handingOver = true) }
        main.postDelayed(endHandOver, HeyCrundiPlan.HANDOVER_MS)
        when (HeyCrundiPlan.onHeard(situation)) {
            HeyCrundiPlan.Wake.OpenScreen -> openScreen()
            HeyCrundiPlan.Wake.FromBackground -> fromBackground()
            HeyCrundiPlan.Wake.WakeScreen -> screenWake?.invoke()
        }
    }

    private fun openScreen() {
        runCatching { app.startActivity(Intent(app, VoiceActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
            .onFailure { Log.w(TAG, "voice screen not opened: ${it.message}") }
    }

    /**
     * Android doesn't let an app open a screen from the background. Crundi as
     * the phone's assistant (mode A) may show its session, as any assistant
     * does on its hotword. Otherwise a full-screen notification opens the
     * voice screen over the lock screen (when the owner allowed full-screen
     * notifications), or shows "Hey Cru heard, tap to talk".
     */
    private fun fromBackground() {
        if (CrundiVoiceService.show()) return
        val nm = app.getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel(WAKE_CHANNEL, "Hey Cru heard", NotificationManager.IMPORTANCE_HIGH).apply { setShowBadge(false) })
        val open = PendingIntent.getActivity(app, 1, Intent(app, VoiceActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val n = NotificationCompat.Builder(app, WAKE_CHANNEL)
            .setSmallIcon(R.drawable.ic_stat)
            .setContentTitle("Yes, Sir?")
            .setContentText("Tap to talk to Cru")
            .setCategory(NotificationCompat.CATEGORY_CALL)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setAutoCancel(true)
            .setTimeoutAfter(HeyCrundiPlan.HANDOVER_MS)
            .setContentIntent(open)
            .apply { if (UrgentAlerts.fullScreenGranted(app)) setFullScreenIntent(open, true) }
            .build()
        runCatching { nm.notify(WAKE_NOTIFICATION, n) }
    }

    /** The voice screen opened: the "tap to talk" notification has done its job. */
    fun cancelWakeNotification(ctx: Context) {
        runCatching { ctx.getSystemService(NotificationManager::class.java).cancel(WAKE_NOTIFICATION) }
    }

    private const val WAKE_CHANNEL = "hey-crundi-heard"
    private const val WAKE_NOTIFICATION = 4211
}

/**
 * Background listening (D4–D7): holds the microphone in the foreground so
 * "Hey Cru" is heard with the screen off. Started only while Crundi is in
 * front (the settings toggle, a voice session's command, a tap on "tap to
 * resume"), as Android 14 requires for the microphone. Keeps its own
 * deadline and stops itself at it; the ongoing notification counts down and
 * has Stop. If Android kills it, it can't take the microphone back from the
 * background: it says "Hey Cru stopped, tap to resume" instead.
 */
class HeyCrundiService : Service() {
    private val handler = Handler(Looper.getMainLooper())
    private val refresh = object : Runnable {
        override fun run() {
            val d = VoicePrefs.wakeDeadline(this@HeyCrundiService)
            val now = System.currentTimeMillis()
            if (HeyCrundiPlan.expired(now, d)) { finish(); return }
            notifyOngoing()
            HeyCrundiPlan.nextRefresh(now, d)?.let { handler.postDelayed(this, it + 50) }
        }
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_STOP -> { finish(); return START_NOT_STICKY }
            ACTION_START -> {}
            else -> {
                // Restarted by Android after it killed the process: no microphone from the background.
                val verdict = HeyCrundiPlan.afterRestart(VoicePrefs.wakeOn(this), System.currentTimeMillis(), VoicePrefs.wakeDeadline(this))
                if (verdict == HeyCrundiPlan.Restart.NotifyStopped) notifyStopped(this) else VoicePrefs.setWakeOn(this, false, null)
                stopSelf()
                return START_NOT_STICKY
            }
        }
        channels(this)
        val type = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE else 0
        try {
            ServiceCompat.startForeground(this, NOTIFICATION_ID, ongoing(), type)
        } catch (e: Exception) {
            Log.w("CrundiWake", "background listening refused: ${e.message}")
            notifyStopped(this)
            stopSelf()
            return START_NOT_STICKY
        }
        cancelStopped(this)
        alive = this
        handler.removeCallbacks(refresh)
        handler.post(refresh)
        HeyCrundi.setBackground(true)
        // Sticky: if Android kills it, it comes back just long enough to say so.
        return START_STICKY
    }

    private fun ongoing(): Notification {
        val d = VoicePrefs.wakeDeadline(this)
        val now = System.currentTimeMillis()
        val stop = PendingIntent.getService(this, 2, Intent(this, HeyCrundiService::class.java).setAction(ACTION_STOP), PendingIntent.FLAG_IMMUTABLE)
        val settings = PendingIntent.getActivity(this, 3, Intent(this, VoiceSettingsActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK), PendingIntent.FLAG_IMMUTABLE)
        return NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_stat)
            .setContentTitle("Listening for \"Hey Cru\"")
            .setContentText(HeyCrundiPlan.timeLeft(now, d) + (d?.let { ", " + HeyCrundiPlan.until(it) } ?: ""))
            .setOngoing(true)
            .setSilent(true)
            .setOnlyAlertOnce(true)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .setContentIntent(settings)
            .addAction(0, "Stop", stop)
            .apply { if (d != null) setWhen(d).setShowWhen(true).setUsesChronometer(true).setChronometerCountDown(true) }
            .build()
    }

    private fun notifyOngoing() {
        runCatching { getSystemService(NotificationManager::class.java).notify(NOTIFICATION_ID, ongoing()) }
    }

    /** Stop for good: the deadline, Stop, the settings toggle, a voice command. */
    private fun finish() {
        VoicePrefs.setWakeOn(this, false, null)
        stopSelf()
    }

    override fun onDestroy() {
        handler.removeCallbacks(refresh)
        if (alive === this) alive = null
        HeyCrundi.setBackground(false)
        // Stopped by the system rather than by Crundi (the prefs still say on).
        if (VoicePrefs.wakeOn(this) && !HeyCrundiPlan.expired(System.currentTimeMillis(), VoicePrefs.wakeDeadline(this))) notifyStopped(this)
        super.onDestroy()
    }

    companion object {
        private const val CHANNEL = "hey-crundi"
        private const val STOPPED_CHANNEL = "hey-crundi-stopped"
        private const val NOTIFICATION_ID = 4210
        private const val STOPPED_ID = 4212
        private const val ACTION_START = "mv.obitech.crundi.voice.HEY_CRUNDI_START"
        private const val ACTION_STOP = "mv.obitech.crundi.voice.HEY_CRUNDI_STOP"
        const val EXTRA_RESUME = "heyCrundiResume"

        @Volatile private var alive: HeyCrundiService? = null

        fun running() = alive != null

        /** Called on the recording thread: the deadline is checked there too, in case the handler sleeps. */
        fun tick() {
            val s = alive ?: return
            if (HeyCrundiPlan.expired(System.currentTimeMillis(), VoicePrefs.wakeDeadline(s))) s.handler.post { s.finish() }
        }

        private fun channels(ctx: Context) {
            val nm = ctx.getSystemService(NotificationManager::class.java)
            nm.createNotificationChannel(NotificationChannel(CHANNEL, "Hey Cru listening", NotificationManager.IMPORTANCE_LOW).apply { setShowBadge(false) })
            nm.createNotificationChannel(NotificationChannel(STOPPED_CHANNEL, "Hey Cru stopped", NotificationManager.IMPORTANCE_DEFAULT))
        }

        /**
         * Listen in the background for [minutes] ([HeyCrundiPlan.UNTIL_OFF]:
         * until turned off). Only while Crundi is in front: Android refuses the
         * microphone to a service started from the background.
         */
        fun start(ctx: Context, minutes: Int) = startUntil(ctx, HeyCrundiPlan.deadline(System.currentTimeMillis(), minutes))

        private fun startUntil(ctx: Context, deadline: Long?) {
            VoicePrefs.setWakeOn(ctx, true, deadline)
            val i = Intent(ctx, HeyCrundiService::class.java).setAction(ACTION_START)
            alive?.let { it.handler.post(it.refresh); return } // running: just the new deadline
            runCatching { ContextCompat.startForegroundService(ctx, i) }
                .onFailure { Log.w("CrundiWake", "background listening not started: ${it.message}") }
        }

        fun stop(ctx: Context) {
            VoicePrefs.setWakeOn(ctx, false, null)
            cancelStopped(ctx)
            runCatching { ctx.stopService(Intent(ctx, HeyCrundiService::class.java)) }
        }

        /** It was on and Android stopped it: on again, with what is left of its time. */
        fun shouldResume(ctx: Context) = !running() && VoicePrefs.wakeOn(ctx) && !HeyCrundiPlan.expired(System.currentTimeMillis(), VoicePrefs.wakeDeadline(ctx))

        fun resume(ctx: Context) {
            val now = System.currentTimeMillis()
            val d = VoicePrefs.wakeDeadline(ctx)
            if (VoicePrefs.wakeOn(ctx) && !HeyCrundiPlan.expired(now, d)) startUntil(ctx, d)
            else start(ctx, HeyCrundiPlan.resumeMinutes(now, d, VoicePrefs.wakeMinutes(ctx)))
        }

        fun notifyStopped(ctx: Context) {
            channels(ctx)
            // A tap opens the settings screen, in front, which may start the microphone again.
            val open = PendingIntent.getActivity(ctx, 4, Intent(ctx, VoiceSettingsActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK).putExtra(EXTRA_RESUME, true), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
            val n = NotificationCompat.Builder(ctx, STOPPED_CHANNEL)
                .setSmallIcon(R.drawable.ic_stat)
                .setContentTitle("Hey Cru stopped")
                .setContentText("The phone stopped background listening. Tap to resume.")
                .setAutoCancel(true)
                .setContentIntent(open)
                .build()
            runCatching { ctx.getSystemService(NotificationManager::class.java).notify(STOPPED_ID, n) }
        }

        fun cancelStopped(ctx: Context) {
            runCatching { ctx.getSystemService(NotificationManager::class.java).cancel(STOPPED_ID) }
        }
    }
}
