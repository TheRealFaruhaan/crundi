package mv.obitech.crundi.voice

import android.app.NotificationManager
import android.app.PendingIntent
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
 */
object CruBubble {
    private const val KEY = "cru"
    private const val ID = 0x0C20

    fun available(ctx: Context): Boolean = Build.VERSION.SDK_INT >= Build.VERSION_CODES.R && !BubbleHelp.blocked(ctx)

    /** Open it; a sentence for the person when it can't be done, else null. */
    fun open(ctx: Context): String? {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return "Bubbles need Android 11 or newer."
        if (!NotificationManagerCompat.from(ctx).areNotificationsEnabled()) return "Allow notifications for Crundi first: a bubble is a kind of notification."
        val nm = ctx.getSystemService(NotificationManager::class.java)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && nm.bubblePreference == NotificationManager.BUBBLE_PREFERENCE_NONE) return "Bubbles are turned off for Crundi. Turn them on in Android's notification settings for this app."
        Notifier.channels(ctx)
        val icon = IconCompat.createWithAdaptiveBitmap(mark(ctx, adaptive = true))
        val cru = Person.Builder().setName("Cru").setKey(KEY).setIcon(IconCompat.createWithBitmap(mark(ctx, adaptive = false))).setImportant(true).setBot(true).build()
        val target = Intent(ctx, CruBubbleActivity::class.java).setAction(Intent.ACTION_VIEW)
        runCatching {
            ShortcutManagerCompat.pushDynamicShortcut(ctx, ShortcutInfoCompat.Builder(ctx, KEY)
                .setLongLived(true).setShortLabel("Cru").setLongLabel("Cru, your voice assistant")
                .setIcon(icon).setPerson(cru).setLocusId(LocusIdCompat(KEY)).setIntent(target).build())
        }
        val pi = PendingIntent.getActivity(ctx, ID, target, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE)
        val style = NotificationCompat.MessagingStyle(Person.Builder().setName("You").build()).setConversationTitle("Cru")
            .addMessage("Tap to talk to Cru", System.currentTimeMillis(), cru)
        val n = NotificationCompat.Builder(ctx, Notifier.CHATS)
            .setSmallIcon(R.drawable.ic_stat)
            .setColor(ctx.getColor(R.color.accent))
            .setStyle(style)
            .setShortcutId(KEY)
            .setLocusId(LocusIdCompat(KEY))
            .setCategory(NotificationCompat.CATEGORY_MESSAGE)
            .addPerson(cru)
            .setSilent(true).setOnlyAlertOnce(true)
            .setContentIntent(pi)
            .setBubbleMetadata(
                NotificationCompat.BubbleMetadata.Builder(pi, icon)
                    .setDesiredHeight(640)
                    // Asked for: open now, and no second copy in the shade.
                    .setAutoExpandBubble(true)
                    .setSuppressNotification(true)
                    .build()
            )
            .build()
        return try { NotificationManagerCompat.from(ctx).notify(ID, n); null } catch (e: SecurityException) { "Allow notifications for Crundi first: a bubble is a kind of notification." }
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
