package mv.obitech.crundi

import android.app.NotificationChannel
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
import androidx.core.app.RemoteInput
import androidx.core.content.LocusIdCompat
import androidx.core.content.pm.ShortcutInfoCompat
import androidx.core.content.pm.ShortcutManagerCompat
import androidx.core.graphics.drawable.IconCompat
import org.json.JSONObject

/** One chat, as much of it as a notification or a bubble needs. */
class ChatRef(val serverId: String, val id: String, val title: String, val project: String, val initials: String, val color: String, val colorDeep: String) {
    /** Android's name for this conversation: the same wherever it is mentioned, or its bubble and its notifications will not find each other. */
    val key: String get() = "chat:$serverId:$id"
    val notificationId: Int get() = key.hashCode()

    companion object {
        fun from(serverId: String, chat: JSONObject): ChatRef {
            val mark = chat.optJSONObject("mark")
            return ChatRef(
                serverId, chat.optString("id"), chat.optString("title", "Chat").ifEmpty { "Chat" }, chat.optString("project"),
                mark?.optString("initials")?.ifEmpty { null } ?: "C", mark?.optString("color")?.ifEmpty { null } ?: "#6366f1", mark?.optString("colorDeep")?.ifEmpty { null } ?: "#4f46e5",
            )
        }
    }
}

/**
 * Showing notifications.
 *
 * A notification about a chat is shown the way Android shows a conversation:
 * under that chat's name and picture, with a box to answer in, and able to
 * float as a bubble. To Android a conversation is three things that have to
 * agree: a long-lived shortcut, a notification in the messaging style that
 * names that shortcut, and (for the bubble) what to open when it is tapped.
 * All three are made here, from one ChatRef.
 */
object Notifier {
    const val CHATS = "chats"
    const val SERVER = "server"
    const val EXTRA_SERVER = "server"
    const val EXTRA_PATH = "path"
    const val EXTRA_CHAT = "chat"
    const val EXTRA_NOTIFICATION = "notification"
    const val KEY_REPLY = "reply"

    fun channels(ctx: Context) {
        val nm = ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        val chats = NotificationChannel(CHATS, "Chats", NotificationManager.IMPORTANCE_HIGH).apply {
            description = "Claude finished, or needs you"
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) setAllowBubbles(true)
        }
        val server = NotificationChannel(SERVER, "Server", NotificationManager.IMPORTANCE_DEFAULT).apply { description = "Services, updates and everything else" }
        nm.createNotificationChannels(listOf(chats, server))
    }

    /** The chat's mark as a picture: its initials on its project's colour. */
    fun markBitmap(ctx: Context, chat: ChatRef, adaptive: Boolean): Bitmap {
        val size = Ui.dp(ctx, 108)
        val bmp = Bitmap.createBitmap(size, size, Bitmap.Config.ARGB_8888)
        val c = Canvas(bmp)
        val a = runCatching { Color.parseColor(chat.color) }.getOrDefault(0xFF6366F1.toInt())
        val b = runCatching { Color.parseColor(chat.colorDeep) }.getOrDefault(0xFF4F46E5.toInt())
        val fill = Paint(Paint.ANTI_ALIAS_FLAG).apply { shader = LinearGradient(0f, 0f, size.toFloat(), size.toFloat(), a, b, Shader.TileMode.CLAMP) }
        // An adaptive icon is cut to shape by the launcher and wants the whole
        // square filled; a plain one is drawn already round.
        if (adaptive) c.drawRect(0f, 0f, size.toFloat(), size.toFloat(), fill) else c.drawCircle(size / 2f, size / 2f, size / 2f, fill)
        val text = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = Color.WHITE
            typeface = Typeface.DEFAULT_BOLD
            textAlign = Paint.Align.CENTER
            textSize = size * (if (adaptive) 0.26f else 0.36f) * (if (chat.initials.length > 1) 1f else 1.15f)
        }
        c.drawText(chat.initials, size / 2f, size / 2f - (text.descent() + text.ascent()) / 2f, text)
        return bmp
    }

    private fun flags(mutable: Boolean) = PendingIntent.FLAG_UPDATE_CURRENT or
        (if (mutable && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) PendingIntent.FLAG_MUTABLE else PendingIntent.FLAG_IMMUTABLE)

    private fun openMain(ctx: Context, serverId: String, path: String, code: Int): PendingIntent =
        PendingIntent.getActivity(
            ctx, code,
            Intent(ctx, MainActivity::class.java).setAction("open:$serverId:$path").putExtra(EXTRA_SERVER, serverId).putExtra(EXTRA_PATH, path)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP),
            flags(false),
        )

    fun chatIntent(ctx: Context, chat: ChatRef, pip: Boolean = false): Intent =
        Intent(ctx, ChatActivity::class.java).setAction("chat:${chat.key}${if (pip) ":pip" else ""}")
            .putExtra(EXTRA_SERVER, chat.serverId).putExtra(EXTRA_CHAT, chat.id).putExtra(ChatActivity.EXTRA_TITLE, chat.title).putExtra(ChatActivity.EXTRA_PIP, pip)

    /** Tell Android this chat is a conversation. Needed before it can be a bubble, and cheap to repeat. */
    private fun publishShortcut(ctx: Context, chat: ChatRef, person: Person) {
        val shortcut = ShortcutInfoCompat.Builder(ctx, chat.key)
            .setLongLived(true)
            .setShortLabel(chat.title.take(40))
            .setLongLabel(if (chat.project.isNotEmpty()) "${chat.title} (${chat.project})".take(80) else chat.title.take(80))
            .setIcon(IconCompat.createWithAdaptiveBitmap(markBitmap(ctx, chat, adaptive = true)))
            .setPerson(person)
            .setLocusId(LocusIdCompat(chat.key))
            .setIntent(chatIntent(ctx, chat).setAction(Intent.ACTION_VIEW))
            .build()
        runCatching { ShortcutManagerCompat.pushDynamicShortcut(ctx, shortcut) }
    }

    private fun chatBuilder(ctx: Context, chat: ChatRef, style: NotificationCompat.MessagingStyle, person: Person): NotificationCompat.Builder {
        publishShortcut(ctx, chat, person)
        val reply = PendingIntent.getBroadcast(
            ctx, chat.notificationId,
            Intent(ctx, ReplyReceiver::class.java).setAction("reply:${chat.key}").putExtra(EXTRA_SERVER, chat.serverId).putExtra(EXTRA_CHAT, chat.id),
            flags(true),
        )
        val b = NotificationCompat.Builder(ctx, CHATS)
            .setSmallIcon(R.drawable.ic_stat)
            .setColor(ctx.getColor(R.color.accent))
            .setStyle(style)
            .setShortcutId(chat.key)
            .setLocusId(LocusIdCompat(chat.key))
            .setCategory(NotificationCompat.CATEGORY_MESSAGE)
            .addPerson(person)
            .setAutoCancel(true)
            .setContentIntent(openMain(ctx, chat.serverId, "/chat/${chat.id}", chat.notificationId))
            .addAction(
                NotificationCompat.Action.Builder(R.drawable.ic_stat, "Reply", reply)
                    .addRemoteInput(RemoteInput.Builder(KEY_REPLY).setLabel("Message Claude").build())
                    .setSemanticAction(NotificationCompat.Action.SEMANTIC_ACTION_REPLY)
                    .setShowsUserInterface(false)
                    .setAllowGeneratedReplies(true)
                    .build()
            )
        // What the bubble opens: this chat by itself, in the compact page.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val target = PendingIntent.getActivity(ctx, chat.notificationId, chatIntent(ctx, chat), flags(true))
            b.setBubbleMetadata(
                NotificationCompat.BubbleMetadata.Builder(target, IconCompat.createWithAdaptiveBitmap(markBitmap(ctx, chat, adaptive = true)))
                    .setDesiredHeight(640)
                    .build()
            )
        }
        return b
    }

    private fun person(ctx: Context, chat: ChatRef): Person =
        Person.Builder().setName(chat.title).setKey(chat.key).setIcon(IconCompat.createWithBitmap(markBitmap(ctx, chat, adaptive = false))).setImportant(true).build()

    private fun me(): Person = Person.Builder().setName("You").build()

    // Markdown shown raw reads as noise in two lines of a notification.
    private fun plain(text: String): String = text.replace(Regex("""(\*\*|__|`{1,3})"""), "").replace(Regex("""(?m)^#{1,6}\s+"""), "").trim()

    private fun post(ctx: Context, id: Int, b: NotificationCompat.Builder) {
        // Refused permission is the person's answer, not an error.
        runCatching { NotificationManagerCompat.from(ctx).notify(id, b.build()) }
    }

    /** A notification that has just arrived, opened and parsed. */
    fun show(ctx: Context, server: Server, payload: JSONObject) {
        val body = plain(payload.optString("body"))
        if (body.isEmpty()) return
        val at = payload.optLong("at", System.currentTimeMillis())
        val chatJson = payload.optJSONObject("chat")
        if (chatJson == null || chatJson.optString("id").isEmpty()) {
            val url = payload.optString("url", "/").let { if (it.startsWith("/") && !it.startsWith("//")) it else "/" }
            val kind = payload.optString("kind", "crundi")
            post(
                ctx, "srv:${server.id}:$kind".hashCode(),
                NotificationCompat.Builder(ctx, SERVER)
                    .setSmallIcon(R.drawable.ic_stat).setColor(ctx.getColor(R.color.accent))
                    .setContentTitle(payload.optString("title", "Crundi")).setContentText(body)
                    .setStyle(NotificationCompat.BigTextStyle().bigText(body)).setSubText(server.host)
                    .setWhen(at).setAutoCancel(true)
                    .setContentIntent(openMain(ctx, server.id, url, "srv:${server.id}:$kind".hashCode())),
            )
            return
        }
        val chat = ChatRef.from(server.id, chatJson)
        val who = person(ctx, chat)
        // Keep what is already showing for this chat and add to it, so two
        // things said in a row read as a conversation, not as one replacing
        // the other.
        val nm = ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        val showing = nm.activeNotifications.firstOrNull { it.id == chat.notificationId }?.notification
        val style = showing?.let { NotificationCompat.MessagingStyle.extractMessagingStyleFromNotification(it) } ?: NotificationCompat.MessagingStyle(me())
        style.setConversationTitle(if (chat.project.isNotEmpty()) "${chat.title} · ${chat.project}" else chat.title)
        style.addMessage(body, at, who)
        post(ctx, chat.notificationId, chatBuilder(ctx, chat, style, who).setWhen(at))
    }

    /** After a reply typed into the notification: show it as said, or say why it was not. */
    fun afterReply(ctx: Context, chat: ChatRef, text: String, problem: String?) {
        val who = person(ctx, chat)
        val nm = ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        val showing = nm.activeNotifications.firstOrNull { it.id == chat.notificationId }?.notification
        val style = showing?.let { NotificationCompat.MessagingStyle.extractMessagingStyleFromNotification(it) } ?: NotificationCompat.MessagingStyle(me())
        style.addMessage(text, System.currentTimeMillis(), null as Person?)
        if (problem != null) style.addMessage(problem, System.currentTimeMillis(), who)
        // Quietly: this is the person's own action coming back, not news.
        post(ctx, chat.notificationId, chatBuilder(ctx, chat, style, who).setOnlyAlertOnce(true).setSilent(true))
    }

    /**
     * Put a chat up as a bubble because the person asked for it, from the
     * app's menu. Android only allows that while the app is in front, which
     * is exactly when the menu can be pressed.
     * @return null if it was posted, or a sentence saying why it cannot be.
     */
    fun bubble(ctx: Context, chat: ChatRef): String? {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return "Bubbles need Android 11 or newer."
        val nm = ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        if (!NotificationManagerCompat.from(ctx).areNotificationsEnabled()) return "Allow notifications for Crundi first: a bubble is a kind of notification."
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && nm.bubblePreference == NotificationManager.BUBBLE_PREFERENCE_NONE) {
            return "Bubbles are turned off for Crundi. Turn them on in Android's notification settings for this app."
        }
        val who = person(ctx, chat)
        val style = NotificationCompat.MessagingStyle(me()).setConversationTitle(chat.title)
        style.addMessage(if (chat.project.isNotEmpty()) "Open in ${chat.project}" else "Open", System.currentTimeMillis(), who)
        val b = chatBuilder(ctx, chat, style, who).setOnlyAlertOnce(true).setSilent(true)
        val target = PendingIntent.getActivity(ctx, chat.notificationId, chatIntent(ctx, chat), flags(true))
        b.setBubbleMetadata(
            NotificationCompat.BubbleMetadata.Builder(target, IconCompat.createWithAdaptiveBitmap(markBitmap(ctx, chat, adaptive = true)))
                .setDesiredHeight(640)
                // Open straight away, and without a second copy in the shade:
                // the person asked for the bubble, not for a notification.
                .setAutoExpandBubble(true)
                .setSuppressNotification(true)
                .build()
        )
        post(ctx, chat.notificationId, b)
        return null
    }
}
