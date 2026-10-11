package mv.obitech.crundi

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import org.json.JSONObject
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * The live card: what is showing while Claude works.
 *
 * One card per project, for as long as that project has a chat that is
 * working or waiting on the person. In the status bar it is a chip (how long
 * one chat has been at it, how many are, or that one needs you); opened, it
 * is a line per chat with the time it started, the ones that need action
 * first and marked.
 *
 * Android calls this a Live Update, and is strict about it: an ongoing
 * notification with a title, in one of the system's own styles, that asks to
 * be promoted. No custom layout is allowed, so the card says what the
 * owner's sketch says in the shape Android gives it. On a phone older than
 * Android 16, or with promotion switched off, the same card is an ordinary
 * ongoing notification without the chip.
 *
 * The server sends a silent message each time a chat's state changes (see
 * tellPhonesState in webapp.js). What is known is kept on disk, because each
 * message says one chat's state and the card needs all of them.
 */
object LiveCards {
    const val CHANNEL = "live"
    private const val PREFS = "crundi.live"
    private const val KEY = "chats"
    // A chat that stopped reporting (the server restarted, the chat was
    // closed) must not sit in the status bar for ever.
    private const val STALE_MS = 6 * 60 * 60 * 1000L
    private const val DONE_SHOWN_MS = 6000L

    fun channel(ctx: Context) {
        val nm = ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        // Default importance, silenced: a card must not be MIN to be promoted,
        // and must never make a sound, since it changes many times a minute.
        nm.createNotificationChannel(NotificationChannel(CHANNEL, "Live card", NotificationManager.IMPORTANCE_DEFAULT).apply {
            description = "While Claude is working"
            setSound(null, null)
            enableVibration(false)
            setShowBadge(false)
        })
    }

    private class Entry(val serverId: String, val chatId: String, val title: String, val project: String, val color: String, val colorDeep: String, val state: String, val since: Long, val at: Long) {
        val active get() = state == "working" || state == "waiting" || state == "needs-input"
        val needs get() = state == "needs-input"
        fun json() = JSONObject().put("serverId", serverId).put("chatId", chatId).put("title", title).put("project", project).put("color", color).put("colorDeep", colorDeep).put("state", state).put("since", since).put("at", at)
    }

    private fun load(ctx: Context): MutableMap<String, Entry> {
        val out = mutableMapOf<String, Entry>()
        val raw = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString(KEY, null) ?: return out
        runCatching {
            val o = JSONObject(raw)
            for (k in o.keys()) {
                val e = o.getJSONObject(k)
                out[k] = Entry(e.getString("serverId"), e.getString("chatId"), e.optString("title", "Chat"), e.optString("project"), e.optString("color", "#6366f1"), e.optString("colorDeep", "#4f46e5"), e.optString("state"), e.optLong("since"), e.optLong("at"))
            }
        }
        return out
    }

    private fun save(ctx: Context, map: Map<String, Entry>) {
        val o = JSONObject()
        for ((k, e) in map) o.put(k, e.json())
        ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putString(KEY, o.toString()).apply()
    }

    private fun cardId(serverId: String, project: String) = "live:$serverId:$project".hashCode()

    /** "my-cool project" -> "MC": the project's own letters, for the card's picture. */
    private fun projectInitials(name: String): String {
        val words = name.split(Regex("[^\\p{L}\\p{N}]+")).filter { it.isNotEmpty() }
        if (words.isEmpty()) return "C"
        return (words[0].take(1) + (words.getOrNull(1)?.take(1) ?: "")).uppercase()
    }

    /** A silent message from the server: one chat's state has changed. */
    @Synchronized
    fun onState(ctx: Context, server: Server, payload: JSONObject) {
        val chat = payload.optJSONObject("chat") ?: return
        val id = chat.optString("id")
        if (id.isEmpty()) return
        val ref = ChatRef.from(server.id, chat)
        val map = load(ctx)
        val now = System.currentTimeMillis()
        map.entries.removeAll { now - it.value.at > STALE_MS }
        val state = payload.optString("state")
        val entry = Entry(server.id, id, ref.title, ref.project, ref.color, ref.colorDeep, state, payload.optLong("since", now).takeIf { it > 0 } ?: now, now)
        val wasActive = map[ref.key]?.active == true
        if (entry.active) map[ref.key] = entry else map.remove(ref.key)
        save(ctx, map)
        render(ctx, server.id, ref.project, map, justFinished = if (!entry.active && wasActive) entry else null)
    }

    /** Forget everything about a server: it was removed from this phone. */
    @Synchronized
    fun forget(ctx: Context, serverId: String) {
        val map = load(ctx)
        val projects = map.values.filter { it.serverId == serverId }.map { it.project }.toSet()
        map.entries.removeAll { it.value.serverId == serverId }
        save(ctx, map)
        val nm = NotificationManagerCompat.from(ctx)
        for (p in projects) nm.cancel(cardId(serverId, p))
    }

    private fun clock(ms: Long) = SimpleDateFormat("HH:mm", Locale.getDefault()).format(Date(ms))

    private fun render(ctx: Context, serverId: String, project: String, map: Map<String, Entry>, justFinished: Entry?) {
        val id = cardId(serverId, project)
        val nm = NotificationManagerCompat.from(ctx)
        val mine = map.values.filter { it.serverId == serverId && it.project == project }
        val needs = mine.filter { it.needs }.sortedBy { it.since }
        val working = mine.filter { !it.needs }.sortedBy { it.since }
        val ordered = needs + working

        if (ordered.isEmpty()) {
            if (justFinished == null) { nm.cancel(id); return }
            // The last chat of this project has finished. Say so for a moment,
            // then go: a card that vanished without a word would leave the
            // person wondering whether it finished or failed.
            val done = base(ctx, justFinished, project)
                .setContentTitle("Claude finished")
                .setContentText(justFinished.title)
                .setOngoing(false).setAutoCancel(true)
                .setTimeoutAfter(DONE_SHOWN_MS)
                .setShortCriticalText("Done")
                .setContentIntent(open(ctx, justFinished, id))
            runCatching { nm.notify(id, done.build()) }.onFailure { PushLog.add(ctx, "Android refused to show the live card: ${it.javaClass.simpleName}: ${it.message}") }
            return
        }

        val first = ordered.first()
        val title = when {
            needs.size == 1 -> "Claude needs you"
            needs.size > 1 -> "${needs.size} chats need you"
            working.size == 1 -> "Claude is working"
            else -> "${working.size} chats working"
        }
        val lines = ordered.joinToString("\n") { e ->
            (if (e.needs) "!  " else "") + clock(e.since) + "   " + e.title + (if (e.needs) "  · needs you" else if (e.state == "waiting") "  · waiting" else "")
        }
        val b = base(ctx, first, project)
            .setContentTitle(title)
            .setContentText(if (ordered.size == 1) first.title else lines.lineSequence().first())
            .setStyle(NotificationCompat.BigTextStyle().bigText(lines))
            .setOngoing(true)
            .setRequestPromotedOngoing(true)
            .setTimeoutAfter(STALE_MS)
            .setContentIntent(open(ctx, first, id))
            .addAction(0, if (needs.isNotEmpty()) "Open" else "Open chat", open(ctx, first, id))
        // The chip. Seven characters or fewer is all Android promises to show.
        when {
            needs.isNotEmpty() -> b.setShortCriticalText("Action")
            working.size == 1 -> b.setUsesChronometer(true).setWhen(first.since).setShowWhen(true)
            else -> b.setShortCriticalText("${working.size} chats")
        }
        // Stopping only means something for a chat that is actually running.
        working.firstOrNull { it.state == "working" }?.let { w ->
            b.addAction(0, if (working.size == 1) "Stop" else "Stop ${w.title.take(18)}", PendingIntent.getBroadcast(
                ctx, ("stop:" + w.serverId + ":" + w.chatId).hashCode(),
                Intent(ctx, StopReceiver::class.java).setAction("stop:${w.serverId}:${w.chatId}").putExtra(Notifier.EXTRA_SERVER, w.serverId).putExtra(Notifier.EXTRA_CHAT, w.chatId),
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
            ))
        }
        runCatching { nm.notify(id, b.build()) }.onFailure { PushLog.add(ctx, "Android refused to show the live card: ${it.javaClass.simpleName}: ${it.message}") }
    }

    private fun base(ctx: Context, e: Entry, project: String): NotificationCompat.Builder {
        val mark = ChatRef(e.serverId, e.chatId, e.title, project, projectInitials(project), e.color, e.colorDeep)
        return NotificationCompat.Builder(ctx, CHANNEL)
            .setSmallIcon(R.drawable.ic_stat)
            .setColor(ctx.getColor(R.color.accent))
            .setLargeIcon(Notifier.markBitmap(ctx, mark, adaptive = false))
            .setSubText(project.ifEmpty { "Crundi" })
            .setCategory(NotificationCompat.CATEGORY_PROGRESS)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
    }

    private fun open(ctx: Context, e: Entry, code: Int): PendingIntent = PendingIntent.getActivity(
        ctx, code,
        Intent(ctx, MainActivity::class.java).setAction("open:${e.serverId}:/chat/${e.chatId}").putExtra(Notifier.EXTRA_SERVER, e.serverId).putExtra(Notifier.EXTRA_PATH, "/chat/${e.chatId}")
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP),
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )

    /** Whether the phone will show the chip at all. Null when the phone is too old to have one. */
    fun chipAllowed(ctx: Context): Boolean? {
        if (Build.VERSION.SDK_INT < 36) return null
        return runCatching { (ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager).canPostPromotedNotifications() }.getOrNull()
    }
}

/** "Stop" on the live card: interrupt that chat, with the app's own login. */
class StopReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        val serverId = intent.getStringExtra(Notifier.EXTRA_SERVER).orEmpty()
        val chatId = intent.getStringExtra(Notifier.EXTRA_CHAT).orEmpty()
        if (serverId.isEmpty() || chatId.isEmpty()) return
        val pending = goAsync()
        // The card is redrawn by the state change the server sends once the
        // chat has actually stopped, so nothing is drawn here.
        Bg.run({ Api.authed(ctx.applicationContext, serverId, "POST", "/api/ui-sessions/$chatId/interrupt") }) { pending.finish() }
    }
}
