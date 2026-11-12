package mv.obitech.crundi.voicelab

import android.content.Context
import android.os.Build
import java.io.File
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * The Voice Lab's report: one line per finding, kept in a file so results from
 * the assistant overlay, the service and the lab screen end up in one place.
 * The lab screen shows it and copies it for pasting back into the spec.
 */
object LabLog {
    private const val FILE = "voicelab.txt"
    private val time = SimpleDateFormat("HH:mm:ss", Locale.US)
    private val listeners = mutableListOf<() -> Unit>()

    private fun file(ctx: Context) = File(ctx.applicationContext.filesDir, FILE)

    @Synchronized
    fun add(ctx: Context, line: String) {
        file(ctx).appendText("${time.format(Date())}  $line\n")
        listeners.toList().forEach { runCatching { it() } }
    }

    @Synchronized
    fun read(ctx: Context): String = file(ctx).let { if (it.exists()) it.readText() else "" }

    @Synchronized
    fun clear(ctx: Context) {
        file(ctx).delete()
        listeners.toList().forEach { runCatching { it() } }
    }

    fun listen(l: () -> Unit) { listeners += l }
    fun unlisten(l: () -> Unit) { listeners -= l }

    fun device(): String =
        "${Build.MANUFACTURER} ${Build.MODEL}, Android ${Build.VERSION.RELEASE} (API ${Build.VERSION.SDK_INT}), build ${Build.DISPLAY}"
}
