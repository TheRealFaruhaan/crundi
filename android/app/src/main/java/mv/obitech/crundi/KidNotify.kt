package mv.obitech.crundi

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import org.json.JSONObject

/**
 * Kid mode's pushes (capability "voice-kid"):
 *
 *   kid-ask   kid Cru asks the parent (a video, more time, something new):
 *             the question with its own answers as the notification's
 *             buttons (Allow once / Always allow / No, 15 more minutes / No).
 *             The push carries only the words, so the question's id and
 *             answers are read from /api/approvals (source "kid").
 *   kid-note  something the child said that the parent should know about:
 *             shown at once, on the Requests channel.
 *
 * An answer goes to POST /api/voice/kid/ask by [KidAskReceiver]. A question
 * answered elsewhere, or expired (five minutes), is refused by the server
 * and the notification says so.
 */
object KidNotify {
    const val ASK = "kid-ask"
    const val NOTE = "kid-note"
    /** The server lets a question wait this long, then kid Cru is told "no answer". */
    private const val ASK_TIMEOUT_MS = 5 * 60 * 1000L

    const val EXTRA_ID = "kidAsk"
    const val EXTRA_CHOICE = "kidChoice"
    const val EXTRA_LABEL = "kidLabel"
    const val EXTRA_TITLE = "kidTitle"

    /** One question as the notification needs it. */
    data class Ask(val id: String, val title: String, val choices: List<Pair<String, String>>)

    /**
     * The question a kid-ask push is about, from GET /api/approvals: the
     * kid item whose title is the push's [body], else the newest kid item.
     * Null when none is waiting (answered already, or an older server).
     */
    fun pick(approvals: JSONObject?, body: String): Ask? {
        val items = approvals?.optJSONArray("items") ?: return null
        val kid = (0 until items.length()).mapNotNull { items.optJSONObject(it) }.filter { it.optString("source") == "kid" && it.optString("id").isNotEmpty() }
        val o = kid.firstOrNull { it.optString("title") == body } ?: kid.maxByOrNull { it.optLong("createdAt") } ?: return null
        val cs = o.optJSONArray("choices")
        val choices = if (cs == null) emptyList() else (0 until cs.length()).mapNotNull { cs.optJSONObject(it) }
            .map { it.optString("id") to it.optString("label").ifEmpty { it.optString("id") } }.filter { it.first.isNotEmpty() }
        return Ask(o.optString("id"), o.optString("title").ifEmpty { body }, choices.ifEmpty { listOf("yes" to "OK", "no" to "No") })
    }

    private fun notificationId(serverId: String, askId: String) = "kid:$serverId:$askId".hashCode()

    private fun flags() = PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE

    private fun openApprovals(ctx: Context, serverId: String, code: Int): PendingIntent = PendingIntent.getActivity(
        ctx, code,
        Intent(ctx, MainActivity::class.java).setAction("kid:$serverId:$code").putExtra(Notifier.EXTRA_SERVER, serverId).putExtra(Notifier.EXTRA_PATH, "/approvals")
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP),
        flags(),
    )

    private fun base(ctx: Context, server: Server, title: String, body: String) = NotificationCompat.Builder(ctx, Notifier.REQUESTS)
        .setSmallIcon(R.drawable.ic_stat).setColor(ctx.getColor(R.color.accent))
        .setContentTitle(title).setContentText(body)
        .setStyle(NotificationCompat.BigTextStyle().bigText(body)).setSubText(server.host)
        .setAutoCancel(true)
        .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)

    /** A kid-ask push (blocking: called on the push's own thread). */
    fun ask(ctx: Context, server: Server, payload: JSONObject) {
        val body = payload.optString("body").trim()
        val at = payload.optLong("at", System.currentTimeMillis())
        val ask = runCatching { pick(Api.authed(ctx, server.id, "GET", "/api/approvals").json, body) }.getOrNull()
        PushLog.add(ctx, "From ${server.host}: kid mode asks the parent${if (ask == null) " (no question waiting)" else ""}")
        if (ask == null) {
            // Answered already, or not readable: the words, opening the list.
            if (body.isEmpty()) return
            val code = "kid:${server.id}:$at".hashCode()
            post(ctx, code, base(ctx, server, payload.optString("title", "Kid mode"), body).setContentIntent(openApprovals(ctx, server.id, code)))
            return
        }
        val id = notificationId(server.id, ask.id)
        val b = base(ctx, server, "Kid mode", ask.title).setWhen(at).setShowWhen(true)
            .setContentIntent(openApprovals(ctx, server.id, id))
            .setTimeoutAfter((ASK_TIMEOUT_MS - (System.currentTimeMillis() - at)).coerceIn(30_000L, ASK_TIMEOUT_MS))
        // Android shows three buttons at most; the question's own answers, in order.
        ask.choices.take(3).forEachIndexed { i, (choice, label) ->
            val pi = PendingIntent.getBroadcast(
                ctx, id + i + 1,
                Intent(ctx, KidAskReceiver::class.java).setAction("kid-ask:${server.id}:${ask.id}:$choice")
                    .putExtra(Notifier.EXTRA_SERVER, server.id).putExtra(EXTRA_ID, ask.id).putExtra(EXTRA_CHOICE, choice)
                    .putExtra(EXTRA_LABEL, label).putExtra(EXTRA_TITLE, ask.title),
                flags(),
            )
            b.addAction(
                NotificationCompat.Action.Builder(R.drawable.ic_stat, label, pi)
                    // Saying yes for the child is the parent's to do: the phone is unlocked first.
                    .setAuthenticationRequired(choice != "no")
                    .setShowsUserInterface(false).build()
            )
        }
        post(ctx, id, b)
    }

    /** A kid-note push: the note, as it came. */
    fun note(ctx: Context, server: Server, payload: JSONObject) {
        val body = payload.optString("body").trim()
        if (body.isEmpty()) return
        PushLog.add(ctx, "From ${server.host}: a note from kid mode")
        val at = payload.optLong("at", System.currentTimeMillis())
        val code = "kid-note:${server.id}:$at".hashCode()
        post(ctx, code, base(ctx, server, payload.optString("title", "Kid mode").ifEmpty { "Kid mode" }, body).setWhen(at).setShowWhen(true)
            .setContentIntent(
                PendingIntent.getActivity(
                    ctx, code,
                    Intent(ctx, MainActivity::class.java).setAction("kid-note:$code").putExtra(Notifier.EXTRA_SERVER, server.id).putExtra(Notifier.EXTRA_PATH, "/inbox")
                        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP),
                    flags(),
                )
            ))
    }

    /** After an answer: what was done, or why not, with the buttons gone. */
    fun answered(ctx: Context, server: Server, askId: String, title: String, outcome: String) {
        post(ctx, notificationId(server.id, askId), base(ctx, server, "Kid mode", "$title\n$outcome").setContentText(outcome).setOnlyAlertOnce(true).setSilent(true).setTimeoutAfter(60_000L))
    }

    private fun post(ctx: Context, id: Int, b: NotificationCompat.Builder) {
        if (!NotificationManagerCompat.from(ctx).areNotificationsEnabled()) { PushLog.add(ctx, "Not shown: notifications are turned off for Crundi"); return }
        runCatching { NotificationManagerCompat.from(ctx).notify(id, b.build()) }
            .onFailure { PushLog.add(ctx, "Android refused to show a notification: ${it.javaClass.simpleName}: ${it.message}") }
    }
}

/** One of kid Cru's answers pressed on its notification: POST /api/voice/kid/ask. */
class KidAskReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        val serverId = intent.getStringExtra(Notifier.EXTRA_SERVER).orEmpty()
        val id = intent.getStringExtra(KidNotify.EXTRA_ID).orEmpty()
        val choice = intent.getStringExtra(KidNotify.EXTRA_CHOICE).orEmpty()
        val label = intent.getStringExtra(KidNotify.EXTRA_LABEL).orEmpty().ifEmpty { choice }
        val title = intent.getStringExtra(KidNotify.EXTRA_TITLE).orEmpty()
        if (serverId.isEmpty() || id.isEmpty() || choice.isEmpty()) return
        val app = ctx.applicationContext
        val server = Servers.byId(app, serverId) ?: return
        val pending = goAsync()
        Bg.run({ Api.authed(app, serverId, "POST", "/api/voice/kid/ask", JSONObject().put("id", id).put("choice", choice)) }) { result ->
            val r = result.getOrNull() ?: Reply(0, null)
            val outcome = when {
                r.ok -> "Answered: $label"
                r.status == 404 -> r.error ?: "That question was already answered, or has expired."
                r.status == 401 || r.status == 403 -> "Not sent. This phone is no longer signed in to that server."
                r.status == 0 -> "Not sent. The server could not be reached."
                else -> r.error ?: "That could not be applied."
            }
            KidNotify.answered(app, server, id, title, outcome)
            pending.finish()
        }
    }
}
