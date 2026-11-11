package mv.obitech.crundi.voice

import android.app.Activity
import android.app.AlertDialog
import android.app.Notification
import android.app.NotificationManager
import android.app.PendingIntent
import android.provider.Settings
import android.util.Log
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Shader
import android.graphics.Typeface
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.Person
import androidx.core.content.LocusIdCompat
import androidx.core.content.pm.ShortcutInfoCompat
import androidx.core.content.pm.ShortcutManagerCompat
import androidx.core.graphics.drawable.IconCompat
import mv.obitech.crundi.BubbleHelp
import mv.obitech.crundi.Notifier
import mv.obitech.crundi.R

/**
 * Cru as its own floating bubble (Android 11+): the voice screen in a
 * bubble over whatever app is open, as the chats' bubbles are. Android puts
 * only a conversation in a bubble, so Cru is one: a long-lived shortcut, a
 * person, a silent notification that opens straight into the bubble and
 * leaves nothing in the shade. Folded away the session pauses (the mic is
 * let go); tapped again it carries on.
 *
 * 11 Nov 2026 (the owner: the button opened Cru full screen): Android
 * bubbles a conversation only where the app's bubble setting allows it, and
 * with "Selected conversations" (Samsung's default) a new conversation never
 * is. It then shows an ordinary notification instead, and that notification
 * opened the bubble's activity full screen. So [request] posts, checks a
 * moment later whether Android really made a bubble of it (FLAG_BUBBLE), and
 * when it did not, takes the notification down and shows the way to allow
 * it. The notification's own tap, and the shortcut, open the normal voice screen.
 */
object CruBubble {
    const val KEY = "cru"
    private const val ID = 0x0C20
    private const val CHECK_MS = 500L
    private const val CHECK_TRIES = 6
    private const val TAG = "CruBubble"
    private val samsung get() = Build.MANUFACTURER.equals("samsung", true)

    fun available(ctx: Context): Boolean = Build.VERSION.SDK_INT >= Build.VERSION_CODES.R && !BubbleHelp.blocked(ctx)

    /** Open it; a sentence for the person when it can't be done, else null. */
    fun open(ctx: Context): String? {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return "Bubbles need Android 11 or newer."
        if (!NotificationManagerCompat.from(ctx).areNotificationsEnabled()) return "Allow notifications for Crundi first: a bubble is a kind of notification."
        val nm = ctx.getSystemService(NotificationManager::class.java)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && nm.bubblePreference == NotificationManager.BUBBLE_PREFERENCE_NONE) return "Bubbles are turned off for Crundi. Turn them on in Android's notification settings for this app."
        Notifier.channels(ctx)
        val icon = IconCompat.createWithAdaptiveBitmap(mark(ctx, adaptive = true))
        // Not a bot: Android counts a notification whose shortcut has only bots as people as not a
        // conversation, and only conversations bubble (11 Nov: Cru's never did while the chats' did).
        val cru = Person.Builder().setName("Cru").setKey(KEY).setIcon(IconCompat.createWithBitmap(mark(ctx, adaptive = false))).setImportant(true).build()
        // The bubble opens the bubble's activity; the notification and the shortcut open the normal voice screen.
        val target = Intent(ctx, CruBubbleActivity::class.java).setAction(Intent.ACTION_VIEW)
        val screen = Intent(ctx, VoiceActivity::class.java).setAction(Intent.ACTION_VIEW).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        val shortcut = ShortcutInfoCompat.Builder(ctx, KEY)
            .setLongLived(true).setShortLabel("Cru").setLongLabel("Cru, your voice assistant")
            .setIcon(icon).setPerson(cru).setLocusId(LocusIdCompat(KEY)).setIntent(screen).build()
        runCatching { ShortcutManagerCompat.pushDynamicShortcut(ctx, shortcut) }.onFailure { Log.w(TAG, "shortcut not published: ${it.message}") }
        val pi = PendingIntent.getActivity(ctx, ID, target, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE)
        val open = PendingIntent.getActivity(ctx, ID + 1, screen, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        val style = NotificationCompat.MessagingStyle(Person.Builder().setName("You").build()).setConversationTitle("Cru")
            .addMessage("Tap to talk to Cru", System.currentTimeMillis(), cru)
        val n = NotificationCompat.Builder(ctx, Notifier.CHATS)
            .setSmallIcon(R.drawable.ic_stat)
            .setColor(ctx.getColor(R.color.accent))
            .setStyle(style)
            // The shortcut itself, not only its id: the notification is a conversation even if publishing lags.
            .setShortcutInfo(shortcut)
            .setLocusId(LocusIdCompat(KEY))
            .setCategory(NotificationCompat.CATEGORY_MESSAGE)
            .addPerson(cru)
            .setSilent(true).setOnlyAlertOnce(true)
            .setContentIntent(open)
            .setBubbleMetadata(
                NotificationCompat.BubbleMetadata.Builder(pi, icon)
                    .setDesiredHeight(640)
                    // Asked for: open now, and no second copy in the shade.
                    .setAutoExpandBubble(true)
                    .setSuppressNotification(true)
                    .build()
            )
            .build()
        return try { NotificationManagerCompat.from(ctx).notify(ID, n); Log.i(TAG, "posted: " + diagnose(ctx)); null } catch (e: SecurityException) { "Allow notifications for Crundi first: a bubble is a kind of notification." }
    }

    /** Is Cru's notification on screen as a bubble now (Android set FLAG_BUBBLE on it)? Null: not posted at all. */
    fun bubbled(ctx: Context): Boolean? {
        val nm = ctx.getSystemService(NotificationManager::class.java)
        val n = runCatching { nm.activeNotifications.firstOrNull { it.id == ID } }.getOrNull() ?: return null
        return n.notification.flags and Notification.FLAG_BUBBLE != 0
    }

    /** Take Cru's notification down (it did not become a bubble: no stray card left to open). */
    fun cancel(ctx: Context) { runCatching { NotificationManagerCompat.from(ctx).cancel(ID) } }

    /**
     * Until when Crundi's main window, coming to the front, steps aside again
     * at once (MainActivity.onResume): the voice screen opened from its top bar
     * has just stepped back for Cru's bubble, which would leave the main window
     * on screen, and a task coming to the front folds an open bubble away.
     */
    @Volatile private var stepAsideUntil = 0L
    private const val STEP_ASIDE_MS = 3_000L

    /** Called by the main window as it comes to the front: true (once) when it should step aside for Cru's bubble. */
    fun mainShouldStepAside(): Boolean {
        val due = System.currentTimeMillis() < stepAsideUntil
        stepAsideUntil = 0L
        return due
    }

    /** On the voice screen's intent: the bubble was not made; show how to allow it (the value: what Android reported). */
    const val EXTRA_BUBBLE_HELP = "cruBubbleHelp"

    /**
     * The voice screen's bubble button: post it, step out of the way at once,
     * and a moment later look.
     *
     * 11 Nov 2026 (the owner, 1.1.3-dev.37: the bubble opened, Crundi came to
     * the front and the bubble closed): the voice screen used to stay in front
     * while it looked (up to three seconds) and only then finish(). By then
     * Android had opened the bubble over it, and finishing sent the owner back
     * to the task the screen was opened from (Crundi's main window, from its
     * top bar). Android folds an open bubble away whenever another task comes
     * to the front, so the bubble closed and Crundi was left on screen. Now the
     * screen steps back right after posting, as a chat's bubble button does
     * (MainActivity.openBubble), before the bubble opens, and the main window
     * steps aside too if it surfaces ([mainShouldStepAside]): whatever app was
     * under them stays on screen with the bubble over it.
     *
     * A real bubble: [onBubbled] (the voice screen, now behind, is finished:
     * nothing comes to the front). Not one: the notification goes, the voice
     * screen comes back (allowed a few seconds after it stepped back) and
     * shows how to allow it ([EXTRA_BUBBLE_HELP]).
     */
    fun request(activity: Activity, onBubbled: () -> Unit) {
        open(activity)?.let { problem -> android.widget.Toast.makeText(activity, problem, android.widget.Toast.LENGTH_LONG).show(); if (BubbleHelp.blocked(activity) || problem.contains("turned off")) explain(activity); return }
        stepAsideUntil = System.currentTimeMillis() + STEP_ASIDE_MS
        activity.moveTaskToBack(true)
        // Looked at a few times over about three seconds: Android marks the bubble shortly after the post.
        // On the app's context: the screen behind may be closed meanwhile (asleep, it closes when it leaves the front).
        val app = activity.applicationContext
        val screen = activity.javaClass
        val h = android.os.Handler(android.os.Looper.getMainLooper())
        var tries = 0
        val look = object : Runnable {
            override fun run() {
                val gone = activity.isFinishing || activity.isDestroyed
                if (bubbled(app) == true) { Log.i(TAG, "bubbled after ${tries + 1} look(s)"); stepAsideUntil = 0L; if (!gone) onBubbled(); return }
                if (++tries < CHECK_TRIES) { h.postDelayed(this, CHECK_MS); return }
                stepAsideUntil = 0L
                val why = diagnose(app)
                Log.w(TAG, "not a bubble: $why")
                cancel(app)
                // Back in front to say why: the dialog needs the screen showing.
                runCatching { app.startActivity(Intent(app, screen).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK).putExtra(EXTRA_BUBBLE_HELP, why)) }
                    .onFailure { android.widget.Toast.makeText(app, "Android did not make a bubble of Cru: allow bubbles for Crundi in its notification settings.", android.widget.Toast.LENGTH_LONG).show() }
            }
        }
        h.postDelayed(look, CHECK_MS)
    }

    /**
     * Everything Android says about Cru's bubble, in one line (logcat tag
     * CruBubble, and the dialog's details): the phone, the app's bubble
     * preference, the Chats channel, Cru's conversation channel if Android
     * made one, the shortcut, and the posted notification's flags.
     */
    fun diagnose(ctx: Context): String {
        val nm = ctx.getSystemService(NotificationManager::class.java)
        val parts = ArrayList<String>()
        parts += "${Build.MANUFACTURER} ${Build.MODEL}, Android ${Build.VERSION.RELEASE} (API ${Build.VERSION.SDK_INT})"
        parts += "notifications " + (if (NotificationManagerCompat.from(ctx).areNotificationsEnabled()) "on" else "OFF")
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            parts += "app bubbles " + when (nm.bubblePreference) { NotificationManager.BUBBLE_PREFERENCE_ALL -> "all"; NotificationManager.BUBBLE_PREFERENCE_SELECTED -> "selected"; else -> "NONE" }
            parts += "phone bubbles " + (if (runCatching { nm.areBubblesEnabled() }.getOrDefault(true)) "on" else "OFF")
        } else if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            @Suppress("DEPRECATION") parts += "app bubbles " + (if (nm.areBubblesAllowed()) "allowed" else "not allowed")
        }
        runCatching { nm.getNotificationChannel(Notifier.CHATS) }.getOrNull()?.let { c ->
            parts += "chats channel importance ${c.importance}" + (if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) ", can bubble ${c.canBubble()}" else "")
        } ?: run { parts += "chats channel MISSING" }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val conv = runCatching { nm.getNotificationChannel(Notifier.CHATS, KEY) }.getOrNull()
            parts += if (conv == null || conv.id == Notifier.CHATS) "no Cru conversation channel" else "Cru conversation importance ${conv.importance}, can bubble ${conv.canBubble()}, demoted ${conv.isDemoted}"
        }
        val sc = runCatching { ShortcutManagerCompat.getDynamicShortcuts(ctx).firstOrNull { it.id == KEY } }.getOrNull()
        parts += if (sc == null) "shortcut MISSING" else "shortcut published"
        val sbn = runCatching { nm.activeNotifications.firstOrNull { it.id == ID } }.getOrNull()
        parts += if (sbn == null) "notification not showing" else "notification flags 0x${Integer.toHexString(sbn.notification.flags)}, bubble flag ${sbn.notification.flags and Notification.FLAG_BUBBLE != 0}, metadata ${sbn.notification.bubbleMetadata != null}, shortcut ${sbn.notification.shortcutId}"
        return parts.joinToString("; ")
    }

    /** Cru's own page in the notification settings (its conversation: "Bubble this conversation"); the app's bubble page otherwise. */
    fun openConversationSettings(ctx: Context) {
        val pkg = ctx.packageName
        runCatching {
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) error("no conversations")
            ctx.startActivity(Intent(Settings.ACTION_CHANNEL_NOTIFICATION_SETTINGS)
                .putExtra(Settings.EXTRA_APP_PACKAGE, pkg).putExtra(Settings.EXTRA_CHANNEL_ID, Notifier.CHATS)
                .putExtra(Settings.EXTRA_CONVERSATION_ID, KEY).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        }.onFailure { BubbleHelp.openAppSettings(ctx) }
    }

    /** What to switch on, with buttons to the pages. */
    fun explain(activity: Activity, details: String = "") {
        val off = BubbleHelp.blocked(activity)
        val all = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && activity.getSystemService(NotificationManager::class.java).bubblePreference == NotificationManager.BUBBLE_PREFERENCE_ALL
        val msg = if (off) BubbleHelp.steps(activity) else if (all) (
            "Crundi's bubbles are on for all conversations, but Android still did not make a bubble of Cru." +
                (if (samsung) "\n\nCheck the phone's Floating notifications is set to Bubbles." else "") +
                "\n\nCopy details and send them to Cru or the developer: they say what Android reported.") else (
            "Android showed Cru as a notification instead of a bubble: Crundi's bubbles are set to selected conversations, and Cru isn't one of them yet.\n\n" +
                "Tap Cru's settings and turn on \"Bubble this conversation\" (or set Crundi's bubbles to All conversations), then tap the bubble button again." +
                (if (samsung) "\n\nOn Samsung, the phone's Floating notifications must also be set to Bubbles." else ""))
        val b = AlertDialog.Builder(activity, android.R.style.Theme_DeviceDefault_Dialog_Alert)
            .setTitle("Let Cru float as a bubble")
            .setMessage(msg)
            .setNegativeButton("Not now", null)
        if (all && details.isNotEmpty()) {
            b.setPositiveButton("Copy details") { _, _ -> copy(activity, details) }
            b.setNeutralButton(if (samsung) "Phone settings" else "App settings") { _, _ -> if (samsung) BubbleHelp.openPhoneSettings(activity) else BubbleHelp.openAppSettings(activity) }
        } else if (off && samsung) {
            b.setPositiveButton("Phone settings") { _, _ -> BubbleHelp.openPhoneSettings(activity) }
            b.setNeutralButton("App settings") { _, _ -> BubbleHelp.openAppSettings(activity) }
        } else if (off) {
            b.setPositiveButton("Open settings") { _, _ -> BubbleHelp.openAppSettings(activity) }
        } else {
            b.setPositiveButton("Cru's settings") { _, _ -> openConversationSettings(activity) }
            b.setNeutralButton(if (samsung) "Phone settings" else "App settings") { _, _ -> if (samsung) BubbleHelp.openPhoneSettings(activity) else BubbleHelp.openAppSettings(activity) }
        }
        runCatching { b.show() }
    }

    private fun copy(ctx: Context, text: String) {
        runCatching {
            ctx.getSystemService(android.content.ClipboardManager::class.java).setPrimaryClip(android.content.ClipData.newPlainText("Cru bubble details", text))
            android.widget.Toast.makeText(ctx, "Copied", android.widget.Toast.LENGTH_SHORT).show()
        }
    }

    /** Cru's round head: sky to violet, "Cru" on it (an adaptive icon fills the square). */
    private fun mark(ctx: Context, adaptive: Boolean): Bitmap {
        val size = (108 * ctx.resources.displayMetrics.density).toInt()
        val bmp = Bitmap.createBitmap(size, size, Bitmap.Config.ARGB_8888)
        val c = Canvas(bmp)
        val fill = Paint(Paint.ANTI_ALIAS_FLAG).apply { shader = LinearGradient(0f, 0f, size.toFloat(), size.toFloat(), 0xFF38BDF8.toInt(), 0xFF6366F1.toInt(), Shader.TileMode.CLAMP) }
        if (adaptive) c.drawRect(0f, 0f, size.toFloat(), size.toFloat(), fill) else c.drawCircle(size / 2f, size / 2f, size / 2f, fill)
        val text = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.WHITE; typeface = Typeface.DEFAULT_BOLD; textAlign = Paint.Align.CENTER; textSize = size * (if (adaptive) 0.24f else 0.32f) }
        c.drawText("Cru", size / 2f, size / 2f - (text.descent() + text.ascent()) / 2f, text)
        return bmp
    }
}

/** The voice screen in Cru's bubble: bubble-ready in the manifest (resizable, embeddable), never over the lock screen. */
class CruBubbleActivity : VoiceActivity() {
    override val inBubble: Boolean = true
}
