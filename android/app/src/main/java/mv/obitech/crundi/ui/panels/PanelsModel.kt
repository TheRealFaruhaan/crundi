package mv.obitech.crundi.ui.panels

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.longOrNull
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.model.Widget
import mv.obitech.crundi.api.str
import mv.obitech.crundi.ui.operations.Load
import mv.obitech.crundi.ui.operations.Loader
import mv.obitech.crundi.ui.operations.rememberLoad
import mv.obitech.crundi.ui.shell.LocalAppServices

/*
 * The panels of the project on screen, and the small rules the web's
 * crundi-widgets.js applies to them (placement, order, chip values).
 */

/** When it last changed, in ms (the server sends a number; older ones a string). */
fun Widget.updatedMs(): Long = (updatedAt as? JsonPrimitive)?.let { it.longOrNull ?: it.contentOrNull?.toDoubleOrNull()?.toLong() } ?: 0L

/**
 * Where a panel shows right now, as the web's placeOf: none (closed, the
 * Panels tab only, inline, or a dock whose chat is gone), cell, dock or chip.
 * [liveChats] null means "do not know" (treat a dock as docked).
 */
fun placeOf(w: Widget, liveChats: Set<String>? = null): String = when {
    !w.open -> "none"
    w.slot == "dock" && w.chat.isEmpty() -> "cell"
    w.slot == "dock" -> if (liveChats == null || w.chat in liveChats) "dock" else "none"
    w.slot == "inline" || w.slot == "tab" -> "none"
    else -> w.slot
}

/** The Panels tab's order: open first, then the most recently changed. */
fun sortForTab(list: List<Widget>): List<Widget> = list.sortedWith(compareBy<Widget> { if (it.open) 0 else 1 }.thenByDescending { it.updatedMs() })

/** The word under a panel in the Panels tab list (the web's first word of its slot label). */
fun statusWord(w: Widget): String = if (!w.open) "closed" else (Widget.SLOT_LABELS[w.slot] ?: w.slot).substringBefore(' ').lowercase()

/** The word in the picker of a Panel pane. */
fun pickerWord(w: Widget, liveChats: Set<String>? = null): String = when {
    !w.open -> "closed"
    else -> when (placeOf(w, liveChats)) {
        "cell" -> "already open"
        "dock" -> "docked to a chat"
        "chip" -> "top bar"
        else -> "Panels tab"
    }
}

/** The slots a person may move [w] to (dock only when it has a chat). */
fun movableSlots(w: Widget): List<Pair<String, String>> =
    Widget.SLOT_LABELS.entries.filter { it.key != "dock" || w.chat.isNotEmpty() }.map { it.key to it.value }

/** The value at a dotted [path], as the chip shows it: text, at most 60 characters. */
fun chipText(value: JsonElement?, path: String): String {
    var cur: JsonElement? = value
    if (path.isNotEmpty()) for (part in path.split('.')) {
        cur = when (cur) {
            is JsonObject -> cur[part]
            is JsonArray -> part.toIntOrNull()?.let { cur.getOrNull(it) }
            else -> null
        } ?: return ""
    }
    return when (cur) {
        null, JsonNull -> ""
        is JsonPrimitive -> cur.content
        else -> cur.toString()
    }.take(60)
}

/** Claude's check renders (GET /api/widgets/renders): one per frame and state. */
data class PanelShot(val frame: String, val state: String) {
    val name get() = "$frame--$state"
    val label get() = frame + if (state != "live") " · $state" else ""
}

fun parseShots(renders: JsonObject?): Pair<List<PanelShot>, Long> {
    val shots = (renders?.get("shots") as? JsonArray).orEmpty().mapNotNull { e ->
        val o = e as? JsonObject ?: return@mapNotNull null
        PanelShot(o.str("frame") ?: return@mapNotNull null, o.str("state") ?: "live")
    }
    return shots to ((renders?.get("at") as? JsonPrimitive)?.longOrNull ?: 0L)
}

/** The project the shell shows ("" for none: global panels only). */
@Composable
fun currentPanelProject(): String {
    val shell by LocalAppServices.current.shell.state.collectAsStateWithLifecycle()
    return shell.currentProject.orEmpty()
}

/** The project's panels, fetched again on every widget list/doc event. */
@Composable
fun rememberPanelList(project: String): Loader<List<Widget>> {
    val api = LocalAppServices.current.api
    val list = rememberLoad(api, project) { api?.widgets?.list(project) ?: emptyList() }
    LaunchedEffect(api, project) {
        api ?: return@LaunchedEffect
        api.events.changes("widget").collect { ev ->
            val p = ev.project
            if (p != null && p != "_global" && p != panelKey(project)) return@collect
            if (ev.data.str("kind") !in setOf("data", "store", "render")) list.reload()
        }
    }
    return list
}

/** The chips' values (chip id to text), loaded for open chips and on their data events. */
@Composable
fun rememberChipValues(project: String, widgets: List<Widget>): Map<String, String> {
    val api = LocalAppServices.current.api
    var values by remember(project) { mutableStateOf(emptyMap<String, String>()) }
    val chips = widgets.filter { it.isChip && it.chip?.source?.isNotEmpty() == true }
    suspend fun load(w: Widget) {
        val c = w.chip ?: return
        val d = try { api?.widgets?.data(project, w.id, c.source) } catch (e: ApiException) { null } ?: return
        values = values + (w.id to chipText(d.data[c.source]?.value, c.path))
    }
    LaunchedEffect(api, project, chips.map { it.id + ":" + it.rev }) { chips.forEach { load(it) } }
    LaunchedEffect(api, project, chips.map { it.id }) {
        api ?: return@LaunchedEffect
        api.events.changes("widget").collect { ev ->
            if (ev.data.str("kind") != "data") return@collect
            val w = chips.firstOrNull { it.id == ev.id } ?: return@collect
            val src = ev.data.str("source")
            if (src == null || src == w.chip?.source) load(w)
        }
    }
    return values
}

/** Plain helper for screens that only want a value. */
fun <T> Load<List<T>>.items(): List<T> = valueOrNull.orEmpty()
