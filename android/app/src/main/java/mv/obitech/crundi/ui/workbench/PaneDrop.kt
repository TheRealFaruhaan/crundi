package mv.obitech.crundi.ui.workbench

import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow

/** What can take a drop, which decides the text it gets. */
enum class DropKind { Chat, Terminal, Notes }

/**
 * Something dragged out of one pane to drop into another, as the web's drag
 * from Files, Git, Kanban, the mind map or Media onto a chat, a terminal or a
 * notes page. The held thing is a kind and a value ("file", "/a/b.js"); each
 * target turns that into its own text (see [textFor]). While a drag is on,
 * the workbench draws the held item over every pane and slides to the next
 * column when the finger rests at the screen's edge.
 */
object PaneDrop {
    /** A target's place on screen (window coordinates) and what it is. */
    data class Target(val kind: DropKind, val bounds: Rect)
    /** One drop: the target's key, the text for it, and where the finger let go. */
    data class Insert(val key: String, val text: String, val at: Offset)

    private var kind by mutableStateOf("")
    private var value by mutableStateOf<String?>(null)

    /** The labelled reference ("[File Path: /a/b.js]") while something is held, else null. */
    val text: String? get() = value?.let { formatRef(kind, it) }
    /** What the held item shows. */
    var label by mutableStateOf(""); private set
    /** Where the finger is, in window coordinates. */
    var finger by mutableStateOf(Offset.Zero)
    /**
     * A source that draws its own held item (a Kanban card, an idea) inside
     * its own pane sets that pane's area here, so the workbench's label stays
     * out of the way until the finger leaves it.
     */
    var ownGhost by mutableStateOf<Rect?>(null)
    /** Everything that takes a drop, by key ("chat:<id>", "term:<id>", "notes:<pane>"). */
    val targets = mutableStateMapOf<String, Target>()
    private val _inserts = MutableSharedFlow<Insert>(extraBufferCapacity = 8)
    /** Each drop, for the target with that key to take. */
    val inserts: SharedFlow<Insert> = _inserts

    // The web's DRAG_LABELS: the agent is told what kind of thing it got.
    private val LABELS = mapOf(
        "file" to "File Path", "folder" to "Folder Path",
        "kanban-task" to "Kanban Task ID", "kanban-subtask" to "Kanban Subtask ID",
        "mindmap" to "Mindmap Idea ID", "media" to "Media Path",
    )
    fun formatRef(kind: String, value: String): String = LABELS[kind]?.let { "[$it: $value]" } ?: value

    /** The text a target of [target] kind gets: a note takes a path as a plain path, everything else the labelled reference. */
    fun textFor(target: DropKind): String? {
        val v = value ?: return null
        return if (target == DropKind.Notes && kind in setOf("file", "folder", "media")) v else formatRef(kind, v)
    }

    fun start(kind: String, value: String, label: String, at: Offset) { this.kind = kind; this.value = value; this.label = label; finger = at }
    /** The key of the target under the finger, while something is held. */
    fun target(): String? = if (value == null) null else targets.entries.firstOrNull { it.value.bounds.contains(finger) }?.key
    fun hovered(key: String): Boolean = value != null && target() == key
    /** Let go: true if it went into a target. */
    fun drop(): Boolean {
        val t = target()
        val x = t?.let { targets[it] }?.let { textFor(it.kind) }
        value = null; ownGhost = null
        if (t != null && x != null) { _inserts.tryEmit(Insert(t, x, finger)); return true }
        return false
    }
    fun cancel() { value = null; ownGhost = null }
}

/**
 * This composable takes drops under [key]: its area is kept in [PaneDrop.targets]
 * while it is on screen. Read [PaneDrop.hovered] to light it up and collect
 * [PaneDrop.inserts] for the text.
 */
fun Modifier.paneDropTarget(key: String, kind: DropKind): Modifier = composed {
    DisposableEffect(key) { onDispose { PaneDrop.targets.remove(key) } }
    Modifier.onGloballyPositioned { PaneDrop.targets[key] = PaneDrop.Target(kind, it.boundsInWindow()) }
}

/**
 * Hold this and drag it to a chat, terminal or notes page in another pane to
 * drop a reference to it there ([kind] and [value] as the web's data-drag-kind
 * and data-drag-ref). Only inside a workbench pane, where there is somewhere
 * to drop it; elsewhere the hold stays whatever it was. Held and let go
 * without moving runs [onHold], so a row's hold menu still works.
 */
fun Modifier.paneDragSource(kind: String, value: String, label: String, onHold: (() -> Unit)? = null): Modifier = composed {
    if (LocalPane.current == null || value.isEmpty()) return@composed Modifier
    val now by rememberUpdatedState(Triple(kind, value, label))
    val hold by rememberUpdatedState(onHold)
    var coords by remember { mutableStateOf<LayoutCoordinates?>(null) }
    var moved by remember { mutableStateOf(false) }
    fun win(p: Offset) = coords?.takeIf { it.isAttached }?.localToWindow(p) ?: p
    Modifier.onGloballyPositioned { coords = it }.pointerInput(Unit) {
        detectDragGesturesAfterLongPress(
            onDragStart = { at -> moved = false; PaneDrop.start(now.first, now.second, now.third, win(at)) },
            onDrag = { c, d -> c.consume(); if (d.getDistance() > 0f) moved = true; PaneDrop.finger = win(c.position) },
            onDragEnd = { if (moved) PaneDrop.drop() else { PaneDrop.cancel(); hold?.invoke() } },
            onDragCancel = { PaneDrop.cancel() },
        )
    }
}
