package mv.obitech.crundi

import android.content.Context
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * What has happened to notifications on this phone, in plain lines, newest
 * last. A notification passes through five hands before it is seen (server,
 * relay, Google, this app's receiver, Android's drawer) and when one does not
 * appear there is otherwise no way to tell which hand dropped it. App
 * settings shows this.
 *
 * Nothing private goes in: no message text, no tokens, no keys. Sizes, server
 * names and outcomes only.
 */
object PushLog {
    private const val PREFS = "crundi.pushlog"
    private const val KEY = "lines"
    private const val MAX = 60

    @Synchronized
    fun add(ctx: Context, line: String) {
        val prefs = ctx.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val stamp = SimpleDateFormat("dd MMM HH:mm:ss", Locale.getDefault()).format(Date())
        val all = (prefs.getString(KEY, "") ?: "").split('\n').filter { it.isNotEmpty() } + "$stamp  ${line.replace('\n', ' ').take(200)}"
        prefs.edit().putString(KEY, all.takeLast(MAX).joinToString("\n")).apply()
    }

    fun lines(ctx: Context): List<String> =
        (ctx.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString(KEY, "") ?: "").split('\n').filter { it.isNotEmpty() }

    @Synchronized
    fun clear(ctx: Context) { ctx.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().remove(KEY).apply() }
}
