package mv.obitech.crundi.ui.workbench

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow

/**
 * Something dragged out of one pane to drop into another, as the web's drag
 * from Files onto a chat: the reference ("[File Path: /a/b.js]") goes into the
 * chat's message box. While a drag is on, the workbench draws the held item
 * over every pane and slides to the next column when the finger rests at the
 * screen's edge.
 */
object PaneDrop {
    /** The text a drop inserts, while something is held. */
    var text by mutableStateOf<String?>(null); private set
    /** What the held item shows. */
    var label by mutableStateOf(""); private set
    /** Where the finger is, in window coordinates. */
    var finger by mutableStateOf(Offset.Zero)
    /** Chats that take a drop: chat id -> their area on screen. */
    val targets = mutableStateMapOf<String, Rect>()
    private val _inserts = MutableSharedFlow<Pair<String, String>>(extraBufferCapacity = 8)
    /** (chat id, text) for each drop on a chat. */
    val inserts: SharedFlow<Pair<String, String>> = _inserts

    fun start(text: String, label: String, at: Offset) { this.text = text; this.label = label; finger = at }
    fun target(): String? = if (text == null) null else targets.entries.firstOrNull { it.value.contains(finger) }?.key
    /** Let go: true if it went into a chat. */
    fun drop(): Boolean {
        val t = target(); val x = text
        text = null
        if (t != null && x != null) { _inserts.tryEmit(t to x); return true }
        return false
    }
    fun cancel() { text = null }
}
