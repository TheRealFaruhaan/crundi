package mv.obitech.crundi.voice

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.async
import kotlinx.coroutines.launch
import mv.obitech.crundi.api.KidApi
import mv.obitech.crundi.api.KidArea
import mv.obitech.crundi.api.KidGame
import mv.obitech.crundi.api.KidGames
import mv.obitech.crundi.api.KidGamesSettings
import mv.obitech.crundi.api.KidPoint
import mv.obitech.crundi.api.KidProgress
import mv.obitech.crundi.api.KidStatus
import mv.obitech.crundi.api.PcAutoLock
import mv.obitech.crundi.api.PcLock
import mv.obitech.crundi.api.PlayDay
import mv.obitech.crundi.api.VoiceApi
import mv.obitech.crundi.api.VoiceSettings
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiChip
import mv.obitech.crundi.ui.components.CrundiSwitchRow
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.StatusBadge
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/**
 * Settings > Voice assistant > Kid mode (capability "voice-kid"), as the
 * web's #vs-kid: starting it (and, while it is on, pausing, adding
 * time, ending it), how kid Cru talks, the overnight games workshop, what the
 * child is learning over time, the games, what kid Cru remembers, what the
 * child calls the parent, and the parent's instructions. Nothing here is ever
 * shown on the kid screen. The server refuses changes while kid mode is on,
 * so they are greyed out then.
 *
 * The PIN itself (one PIN for kid mode and the PC lock) and the PC lock
 * (capability "pc-lock") are their own section, as the web's #vs-pin:
 * [PinLockPart].
 */
data class KidUi(
    /** Null while loading; [error] says why it isn't there. */
    val status: KidStatus? = null,
    val error: String = "",
    val range: String = "month",
    val progress: KidProgress? = null,
    val games: KidGames? = null,
    /** What kid Cru remembers (friend.md); null not loaded. */
    val friend: String? = null,
    val pinText: String = "",
    /** Null: not typed yet, the server's [KidStatus.parentName] is shown. */
    val parentText: String? = null,
    val fromText: String? = null,
    val toText: String? = null,
    /** What is being changed now (game ids, "pin", "start", …). */
    val busy: Set<String> = emptySet(),
    /** Tapped once, waiting for the second tap: "reset", "forget". */
    val confirm: String = "",
    /** The PC lock (capability "pc-lock"); null: not known, or a server without it. */
    val lock: PcLock? = null,
)

class KidEvents(
    val onPinText: (String) -> Unit = {},
    val onSetPin: () -> Unit = {},
    val onStart: () -> Unit = {},
    /** pause, resume, time, end. */
    val onKid: (String) -> Unit = {},
    val onRange: (String) -> Unit = {},
    /** A game: enable, disable or rollback. */
    val onGame: (String, String) -> Unit = { _, _ -> },
    val onReset: () -> Unit = {},
    val onForget: () -> Unit = {},
    val onParentText: (String) -> Unit = {},
    val onSaveParent: () -> Unit = {},
    val onVoice: (String) -> Unit = {},
    val onChattiness: (String) -> Unit = {},
    val onWait: (Int) -> Unit = {},
    val onReflex: (Boolean) -> Unit = {},
    val onImprove: (Boolean) -> Unit = {},
    val onFromText: (String) -> Unit = {},
    val onToText: (String) -> Unit = {},
    val onSaveTimes: () -> Unit = {},
    /** Lock the PC: visible or full. */
    val onLock: (String) -> Unit = {},
    val onUnlock: () -> Unit = {},
    /** The auto lock changed (switch, minutes or mode): saved at once. */
    val onAutoLock: (PcAutoLock) -> Unit = {},
)

/** An auto lock time as the web labels it: "5 min", 60 "1 hour". */
fun autoMinutesLabel(m: Int): String = if (m >= 60 && m % 60 == 0) "${m / 60} hour" + (if (m == 60) "" else "s") else "$m min"

/** The PIN section's line: no PIN yet, PC locked, or PIN set. */
fun pinSummary(k: KidUi): String = when {
    k.status == null -> ""
    k.lock?.on == true -> "PC locked"
    !k.status.pinSet -> "No PIN yet"
    else -> "PIN set"
}

/** The fold's line: On, Off, or no PIN yet. */
fun kidSummary(k: KidStatus?): String = when {
    k == null -> ""
    k.on -> if (k.paused) "On, paused" else "On"
    !k.pinSet -> "No PIN yet"
    else -> "Off"
}

/** Kid mode's state and calls, for the settings route (state is Compose state, so the screen follows it). */
class KidSettingsModel(
    private val scope: CoroutineScope,
    private val kid: KidApi,
    private val voice: VoiceApi,
    /** The server's voice settings changed (a kid setting saved). */
    private val onSettings: (VoiceSettings) -> Unit,
    private val say: (String, Boolean) -> Unit,
) {
    var ui by mutableStateOf(KidUi())
        private set

    /** The server has the PC lock (capability "pc-lock"). */
    var lockSupported = false

    private fun busy(key: String, on: Boolean) { ui = ui.copy(busy = if (on) ui.busy + key else ui.busy - key) }

    fun load() {
        scope.launch {
            val st = try { kid.status() } catch (e: Exception) { ui = ui.copy(error = e.message ?: "Could not load kid mode"); return@launch }
            if (st == null) { ui = ui.copy(error = "Not supported by this server: update it for kid mode."); return@launch }
            ui = ui.copy(status = st, error = "")
            loadLock()
            val p = async { runCatching { kid.progress(ui.range) }.getOrNull() }
            val g = async { runCatching { kid.games() }.getOrNull() }
            val f = async { runCatching { kid.friend() }.getOrNull() }
            ui = ui.copy(progress = p.await() ?: ui.progress, games = g.await() ?: ui.games, friend = f.await() ?: ui.friend)
            // Seen: new games stop being marked new.
            if (!st.on && ui.games?.games?.any { it.isNew } == true) runCatching { kid.game("", "seen") }
        }
    }

    /** Kid mode changed on the server ('voice-kid'): its state again (the rest only when it switched). */
    fun changed(next: KidStatus?) {
        val was = ui.status
        if (next != null) ui = ui.copy(status = next.copy(instructions = next.instructions.ifEmpty { was?.instructions.orEmpty() }, parentName = next.parentName.ifEmpty { was?.parentName.orEmpty() }))
        if (next == null || was?.on != next.on) load()
    }

    fun loadLock() {
        if (!lockSupported) { ui = ui.copy(lock = null); return }
        scope.launch { runCatching { kid.lock() }.getOrNull()?.let { ui = ui.copy(lock = it) } }
    }

    /** The PC lock changed on the server ('pc-lock'). */
    fun lockChanged(next: PcLock?) { if (next == null) loadLock() else ui = ui.copy(lock = next) }

    fun loadGames() { scope.launch { runCatching { kid.games() }.getOrNull()?.let { ui = ui.copy(games = it) } } }

    private fun loadProgress() { scope.launch { runCatching { kid.progress(ui.range) }.onSuccess { ui = ui.copy(progress = it) }.onFailure { say(it.message ?: "Could not load the progress", true) } } }

    private fun saveKid(done: String, call: suspend () -> VoiceSettings) {
        scope.launch {
            try { onSettings(call()); say(done, false) } catch (e: Exception) { say(e.message ?: "Not saved", true) }
        }
    }

    fun events(server: () -> VoiceSettings?) = KidEvents(
        onPinText = { ui = ui.copy(pinText = it.filter { c -> c.isDigit() }.take(8)) },
        onSetPin = {
            val pin = ui.pinText.trim()
            if (!KidApi.PIN.matches(pin)) say("The PIN is 4 to 8 digits", true)
            else scope.launch {
                busy("pin", true)
                try { val st = kid.setPin(pin); ui = ui.copy(pinText = "", status = ui.status?.copy(pinSet = true) ?: st, lock = ui.lock?.copy(pinSet = true)); say("PIN saved", false) }
                catch (e: Exception) { say(e.message ?: "Not saved", true) }
                busy("pin", false)
            }
        },
        onStart = {
            scope.launch {
                busy("start", true)
                try { val st = kid.start(); changed(st); say("Kid mode is on", false) } catch (e: Exception) { say(e.message ?: "Not started", true) }
                busy("start", false)
            }
        },
        onKid = { action ->
            scope.launch {
                try {
                    when (action) {
                        "pause" -> kid.pause(true)?.let { changed(it) }
                        "resume" -> kid.pause(false)?.let { changed(it) }
                        "time" -> { kid.addTime(15)?.let { changed(it) }; say("15 more minutes added", false) }
                        "end" -> { kid.end(); changed(null); say("Kid mode ended", false) }
                    }
                } catch (e: Exception) { say(e.message ?: "That did not work", true) }
            }
        },
        onRange = { r -> if (r != ui.range) { ui = ui.copy(range = r); loadProgress() } },
        onGame = { id, action ->
            scope.launch {
                busy(id, true)
                try { ui = ui.copy(games = kid.game(id, action)); say(when (action) { "enable" -> "Switched on"; "disable" -> "Switched off"; else -> "Rolled back" }, false) }
                catch (e: Exception) { say(e.message ?: "Not changed", true) }
                busy(id, false)
            }
        },
        onReset = {
            if (ui.confirm != "reset") ui = ui.copy(confirm = "reset")
            else scope.launch {
                ui = ui.copy(confirm = "")
                try { kid.resetProgress(); say("Progress forgotten", false); loadProgress() } catch (e: Exception) { say(e.message ?: "Not reset", true) }
            }
        },
        onForget = {
            if (ui.confirm != "forget") ui = ui.copy(confirm = "forget")
            else scope.launch {
                ui = ui.copy(confirm = "")
                try { kid.forgetFriend(); ui = ui.copy(friend = ""); say("Cleared", false) } catch (e: Exception) { say(e.message ?: "Not cleared", true) }
            }
        },
        onParentText = { ui = ui.copy(parentText = it.take(60)) },
        onSaveParent = {
            val name = (ui.parentText ?: ui.status?.parentName).orEmpty().trim()
            scope.launch {
                try { kid.parentName(name); ui = ui.copy(parentText = null, status = ui.status?.copy(parentName = name)); say("Saved", false); load() }
                catch (e: Exception) { say(e.message ?: "Not saved", true) }
            }
        },
        onVoice = { v -> saveKid("Kid Cru's voice saved") { voice.saveKid(kidVoice = v) } },
        onChattiness = { c -> saveKid("Saved") { voice.saveKid(kidChattiness = c) } },
        onWait = { ms -> saveKid("Saved") { voice.saveKid(kidPauseMs = ms) } },
        onReflex = { on -> saveKid(if (on) "Instant cheers on" else "Instant cheers off") { voice.saveKid(kidReflex = on) } },
        onImprove = { on ->
            val g = server()?.kidGames ?: KidGamesSettings()
            saveKid(if (on) "Games improve overnight" else "No overnight changes") { voice.saveKid(kidGames = g.copy(improve = on)) }
        },
        onFromText = { ui = ui.copy(fromText = it.take(5)) },
        onToText = { ui = ui.copy(toText = it.take(5)) },
        onSaveTimes = {
            val g = server()?.kidGames ?: KidGamesSettings()
            val from = (ui.fromText ?: g.from).trim()
            val to = (ui.toText ?: g.to).trim()
            if (!TIME.matches(from) || !TIME.matches(to)) say("A time is HH:MM, e.g. 01:00", true)
            else saveKid("Saved") { voice.saveKid(kidGames = g.copy(from = from, to = to)).also { ui = ui.copy(fromText = null, toText = null) } }
        },
        onLock = { mode ->
            scope.launch {
                busy("lock", true)
                try { ui = ui.copy(lock = kid.lockStart(mode)); say("The PC is locked", false) } catch (e: Exception) { say(e.message ?: "Not locked", true) }
                busy("lock", false)
            }
        },
        onAutoLock = { next ->
            val was = ui.lock
            ui = ui.copy(lock = was?.copy(auto = next))
            scope.launch {
                busy("auto", true)
                try { val l = kid.lockAuto(next); ui = ui.copy(lock = l); say(if (l.auto?.on ?: next.on) "Auto lock on" else "Auto lock off", false) }
                catch (e: Exception) { ui = ui.copy(lock = was); say(e.message ?: "Not saved", true) }
                busy("auto", false)
            }
        },
        onUnlock = {
            scope.launch {
                busy("lock", true)
                try { ui = ui.copy(lock = kid.lockEnd()); say("The PC is unlocked", false) } catch (e: Exception) { say(e.message ?: "Not unlocked", true) }
                busy("lock", false)
            }
        },
    )

    companion object { private val TIME = Regex("^([01]\\d|2[0-3]):[0-5]\\d$") }
}

/** The fold's contents. [s] the server's voice settings (kid Cru's voice, chattiness and the rest). */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun KidModePart(k: KidUi, s: VoiceSettings, ev: KidEvents) {
    @Composable fun Hint(t: String) = Text(t, style = CrundiType.Small, modifier = Modifier.padding(top = 4.dp))
    @Composable fun Sub(t: String) = Text(t, style = CrundiType.Heading, modifier = Modifier.padding(top = 16.dp, bottom = 6.dp))
    val st = k.status
    Text(
        "A locked, full-screen screen on the PC where Cru is your child's friend: learning games, chats and stories, and only the YouTube channels you approve. It has its own conversation and never sees your work. Only the PIN leaves it. If your child says something worrying, Cru answers kindly, tells them to find a grown-up they trust, and sends you a short note here and on your phone; nothing is recorded.",
        style = CrundiType.Small,
    )
    if (k.error.isNotEmpty()) { Text(k.error, style = CrundiType.Small.copy(color = CrundiColors.Yellow), modifier = Modifier.padding(top = 8.dp)); return }
    if (st == null) { Text("Loading…", style = CrundiType.Small, modifier = Modifier.padding(top = 8.dp)); return }
    val on = st.on
    if (on) {
        Text(
            "Kid mode is on" + (if (st.since > 0) " since " + java.text.SimpleDateFormat("HH:mm", java.util.Locale.getDefault()).format(java.util.Date(st.since)) else "") + ". On the kid screen it ends only with the PIN; from here you can pause it, add time or end it.",
            style = CrundiType.Small.copy(color = CrundiColors.Yellow), modifier = Modifier.padding(top = 10.dp, bottom = 8.dp),
        )
        var confirmEnd by androidx.compose.runtime.remember { mutableStateOf(false) }
        KidButtons(st, confirmEnd, onConfirm = { confirmEnd = it }, onKid = ev.onKid)
    }

    if (!on) CrundiButton("Start kid mode", ev.onStart, kind = ButtonKind.Primary, icon = "play", enabled = st.pinSet && "start" !in k.busy, modifier = Modifier.padding(top = 10.dp))
    Hint(if (st.pinSet) "Kid mode ends only with your PIN (set under Your PIN and the PC lock). Keep it from your child. Cru can also start it when you ask (typed, or in your recognised voice)." else "Set your PIN first (under Your PIN and the PC lock): it is the only way out of kid mode.")

    // ─── How kid Cru talks ───
    Sub("How kid Cru talks")
    Text("Chattiness", style = CrundiType.Small.copy(color = CrundiColors.TextPrimary), modifier = Modifier.padding(bottom = 6.dp))
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        for ((id, label) in KidApi.CHATTINESS) CrundiChip(label, s.kidChattiness == id, { if (!on && s.kidChattiness != id) ev.onChattiness(id) })
    }
    Text("Voice", style = CrundiType.Small.copy(color = CrundiColors.TextPrimary), modifier = Modifier.padding(top = 12.dp, bottom = 6.dp))
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        for ((id, label) in KidApi.VOICES) CrundiChip(label, (s.kidVoice ?: "") == id, { if (!on && (s.kidVoice ?: "") != id) ev.onVoice(id) })
    }
    Text("Wait before answering", style = CrundiType.Small.copy(color = CrundiColors.TextPrimary), modifier = Modifier.padding(top = 12.dp, bottom = 6.dp))
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        for (ms in KidApi.WAITS) CrundiChip("${ms / 1000.0} s".replace(".0 s", " s"), s.kidPauseMs == ms, { if (!on && s.kidPauseMs != ms) ev.onWait(ms) })
    }
    CrundiSwitchRow("Instant cheers in games", s.kidReflex, { ev.onReflex(it) }, Modifier.padding(horizontal = 0.dp), detail = "Quick lines said the moment your child gets something right or tries again.", enabled = !on)
    Hint("Small children pause longer, so kid Cru waits longer before answering.")

    // ─── Improve games overnight ───
    s.kidGames?.let { g ->
        Sub("Improve games overnight")
        CrundiSwitchRow("On a night after kid mode was used", g.improve, { ev.onImprove(it) }, Modifier.padding(horizontal = 0.dp), enabled = !on)
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiTextField(k.fromText ?: g.from, ev.onFromText, Modifier.weight(1f), placeholder = "From 01:00", icon = "clock", enabled = !on, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number))
            CrundiTextField(k.toText ?: g.to, ev.onToText, Modifier.weight(1f), placeholder = "To 05:00", icon = "clock", enabled = !on, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number))
            CrundiButton("Save", ev.onSaveTimes, enabled = !on && (k.fromText != null || k.toText != null))
        }
        Hint("Your own Claude Code improves a game or two and makes one new learning game, from what your child played. Every change is safety-checked before it goes live; a failed one stays off.")
    }

    // ─── Progress ───
    Sub("Progress")
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        for ((r, label) in listOf("week" to "Week", "month" to "Month", "all" to "All")) CrundiChip(label, k.range == r, { ev.onRange(r) })
    }
    val p = k.progress
    if (p == null) Hint("Loading the progress…")
    else {
        for (n in p.notes) Text(n, style = CrundiType.Small.copy(color = CrundiColors.TextSecondary), modifier = Modifier.padding(top = 6.dp))
        val played = p.areas.filter { it.played }
        if (played.isEmpty()) Hint("Nothing measured yet: levels come from the games your child plays.")
        for (a in played) AreaCard(a)
        if (p.playTime.isNotEmpty()) PlayTime(p.playTime, p.totalMinutes)
        CrundiButton(if (k.confirm == "reset") "Forget every level?" else "Reset progress", ev.onReset, kind = ButtonKind.Danger, icon = "trash", enabled = !on, modifier = Modifier.padding(top = 10.dp))
    }

    // ─── Games ───
    k.games?.let { gs ->
        Sub("Games")
        if (gs.games.isEmpty()) Hint("No games yet.")
        for (g in gs.games) GameRow(g, busy = g.id in k.busy, locked = on, ev = ev)
        gs.workshop?.runs?.firstOrNull()?.let { run ->
            Hint(
                "Last overnight run" + (if (run.atMs > 0) " " + java.text.SimpleDateFormat("d MMM HH:mm", java.util.Locale.getDefault()).format(java.util.Date(run.atMs)) else "") +
                    ": ${run.created.size} new, ${run.updated.size} improved, ${run.failed.size} refused by the checks." + (if (run.summary.isNotEmpty()) " " + run.summary.take(300) else ""),
            )
        }
    }

    // ─── What kid Cru remembers ───
    Sub("What kid Cru remembers")
    Pre(k.friend?.takeIf { it.isNotBlank() } ?: if (k.friend == null) "Loading…" else "Nothing yet.", 160)
    CrundiButton(if (k.confirm == "forget") "Clear it for good?" else "Clear it", ev.onForget, kind = ButtonKind.Danger, icon = "trash", enabled = !on, modifier = Modifier.padding(top = 8.dp))
    Hint("First name and likes only; kid Cru never keeps addresses, schools or other private details.")

    // ─── How your child calls you ───
    Sub("How your child calls you")
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        CrundiTextField(k.parentText ?: st.parentName, ev.onParentText, Modifier.weight(1f), placeholder = "e.g. Daddy, Mummy, Baba", icon = "user", enabled = !on)
        CrundiButton("Save", ev.onSaveParent, enabled = !on && k.parentText != null)
    }
    Hint("Kid Cru uses it when it asks you something (\"Let me ask Daddy!\"). Empty: \"your grown-up\".")

    // ─── The parent's instructions ───
    if (st.instructions.isNotBlank()) {
        Sub("Your instructions for kid Cru")
        Pre(st.instructions, 200)
        Hint("Tell Cru what to change outside kid mode (\"the kid's name is Sara\", \"add Blippi to the approved channels\", \"no videos after 7 pm\"), or edit kid-mode.md in the notebook. Kid mode can only read it.")
    }
}

@Composable
private fun Pre(text: String, maxHeight: Int) {
    Box(Modifier.fillMaxWidth().heightIn(max = maxHeight.dp).clip(Dimens.ShapeSm).background(CrundiColors.BgTertiary).verticalScroll(rememberScrollState()).padding(8.dp)) {
        Text(text, style = CrundiType.Code.copy(fontSize = 12.sp, color = CrundiColors.TextSecondary))
    }
}

/** One area: its level, trend and line over time, then what is mastered and being learnt. */
@Composable
private fun AreaCard(a: KidArea) {
    val (word, colour) = trendWord(a.trend)
    Column(Modifier.fillMaxWidth().padding(top = 8.dp).clip(Dimens.ShapeSm).border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm).padding(10.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(a.label, style = CrundiType.Body.copy(fontWeight = FontWeight.SemiBold))
                if (a.about.isNotEmpty()) Text(a.about, style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans))
            }
            Spark(a.series, Modifier.padding(start = 8.dp).width(120.dp).height(32.dp))
        }
        Row(Modifier.padding(top = 4.dp), horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("Level " + (if (a.level > 0) fmtLevel(a.level) else "-") + " of 5", style = CrundiType.Small.copy(fontSize = 12.sp, color = CrundiColors.TextPrimary))
            if (word.isNotEmpty()) Text(word, style = CrundiType.Small.copy(fontSize = 12.sp, color = colour))
        }
        if (a.mastered.isNotEmpty()) Text("Mastered: " + a.mastered.joinToString("; "), style = CrundiType.Small.copy(fontSize = 12.sp, color = CrundiColors.Green), modifier = Modifier.padding(top = 4.dp))
        if (a.learning.isNotEmpty()) Text("Learning now: " + a.learning.joinToString("; "), style = CrundiType.Small.copy(fontSize = 12.sp, color = CrundiColors.AccentHover), modifier = Modifier.padding(top = 2.dp))
    }
}

/** rising (green), dipping (yellow), steady (muted). */
fun trendWord(t: String): Pair<String, androidx.compose.ui.graphics.Color> = when (t) {
    "up" -> "rising" to CrundiColors.Green
    "down" -> "dipping" to CrundiColors.Yellow
    "steady" -> "steady" to CrundiColors.TextMuted
    else -> "" to CrundiColors.TextMuted
}

private fun fmtLevel(v: Double) = if (v == Math.floor(v)) v.toLong().toString() else "%.1f".format(java.util.Locale.US, v)

/** Where each point of a sparkline goes in a [w] by [h] box (2 px in from each edge), mastery 0..1 bottom to top. Tested. */
fun sparkPoints(series: List<KidPoint>, w: Float, h: Float): List<Offset> {
    if (series.size < 2) return emptyList()
    val pad = 2f
    return series.mapIndexed { i, pt ->
        Offset(pad + i / (series.size - 1f) * (w - 2 * pad), pad + (1f - pt.mastery.toFloat().coerceIn(0f, 1f)) * (h - 2 * pad))
    }
}

/** Mastery over time, a small line chart (vsSpark): a baseline, the line, a dot on the latest day. */
@Composable
private fun Spark(series: List<KidPoint>, modifier: Modifier) {
    if (series.size < 2) {
        Box(modifier, contentAlignment = Alignment.CenterEnd) { Text(if (series.isEmpty()) "no play yet" else "one day so far", style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans)) }
        return
    }
    Canvas(modifier.semantics { contentDescription = "Mastery over time" }) {
        val pts = sparkPoints(series, size.width, size.height)
        drawLine(CrundiColors.Border, Offset(2f, size.height - 2f), Offset(size.width - 2f, size.height - 2f), strokeWidth = 1.dp.toPx())
        val path = Path().apply { pts.forEachIndexed { i, o -> if (i == 0) moveTo(o.x, o.y) else lineTo(o.x, o.y) } }
        drawPath(path, CrundiColors.Accent, style = Stroke(width = 2.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round))
        drawCircle(CrundiColors.Accent, radius = 3.dp.toPx(), center = pts.last())
    }
}

/** Play time per day: bars, the last 31 days of the range, and the total. */
@Composable
private fun PlayTime(days: List<PlayDay>, total: Long) {
    Text("Play time: $total minutes in this period", style = CrundiType.Small, modifier = Modifier.padding(top = 10.dp))
    val shown = days.takeLast(31)
    val max = (shown.maxOfOrNull { it.minutes } ?: 1.0).coerceAtLeast(1.0)
    Row(Modifier.fillMaxWidth().height(48.dp).padding(top = 4.dp), horizontalArrangement = Arrangement.spacedBy(3.dp), verticalAlignment = Alignment.Bottom) {
        for (d in shown) Box(
            Modifier.weight(1f).height((2 + (d.minutes / max) * 42).dp).clip(androidx.compose.foundation.shape.RoundedCornerShape(topStart = 2.dp, topEnd = 2.dp))
                .background(CrundiColors.Accent.copy(alpha = 0.8f)).semantics { contentDescription = "${d.date}: ${d.minutes} min" },
        )
        // A few days still look like days, not a wall.
        if (shown.size < 8) Box(Modifier.weight((8 - shown.size).toFloat()))
    }
}

@Composable
private fun GameRow(g: KidGame, busy: Boolean, locked: Boolean, ev: KidEvents) {
    Column(Modifier.fillMaxWidth().padding(vertical = 6.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text(g.title.ifEmpty { g.id }, style = CrundiType.Body.copy(color = if (g.enabled) CrundiColors.TextPrimary else CrundiColors.TextSecondary))
                    if (g.isNew) StatusBadge("new", BadgeKind.Accent)
                    if (g.changedByWorkshop) Text("updated v${g.version}", style = CrundiType.Meta)
                }
                val line = listOfNotNull(
                    g.skills.joinToString(", ").takeIf { it.isNotEmpty() },
                    g.ageRange.takeIf { it.isNotEmpty() }?.let { "ages $it" },
                    if (g.createdBy == "workshop") "made overnight" else null,
                ).joinToString(" · ")
                if (line.isNotEmpty()) Text(line, style = CrundiType.Small.copy(fontSize = 12.sp))
            }
            androidx.compose.material3.Switch(
                checked = g.enabled, onCheckedChange = { ev.onGame(g.id, if (it) "enable" else "disable") }, enabled = !busy && !locked,
                colors = androidx.compose.material3.SwitchDefaults.colors(
                    checkedThumbColor = CrundiColors.TextPrimary, checkedTrackColor = CrundiColors.Accent, checkedBorderColor = CrundiColors.Accent,
                    uncheckedThumbColor = CrundiColors.TextSecondary, uncheckedTrackColor = CrundiColors.BgTertiary, uncheckedBorderColor = CrundiColors.Border,
                ),
                modifier = Modifier.semantics { contentDescription = if (g.enabled) "On: switch off" else "Off: switch on" },
            )
        }
        if (g.hasPrev) CrundiButton("Roll back", { ev.onGame(g.id, "rollback") }, kind = ButtonKind.Ghost, icon = "rotate-ccw", enabled = !busy && !locked)
        Box(Modifier.fillMaxWidth().padding(top = 6.dp).size(Dimens.Hairline).background(CrundiColors.Border))
    }
}

/**
 * Settings > Voice assistant > Your PIN and the PC lock, as the web's #vs-pin:
 * one PIN for kid mode and the PC lock, and (capability "pc-lock", [KidUi.lock]
 * known) locking the PC app or unlocking it from here.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun PinLockPart(k: KidUi, ev: KidEvents) {
    @Composable fun Hint(t: String) = Text(t, style = CrundiType.Small, modifier = Modifier.padding(top = 4.dp))
    @Composable fun Sub(t: String) = Text(t, style = CrundiType.Heading, modifier = Modifier.padding(top = 16.dp, bottom = 6.dp))
    if (k.error.isNotEmpty()) { Text(k.error, style = CrundiType.Small.copy(color = CrundiColors.Yellow)); return }
    val st = k.status ?: run { Text("Loading…", style = CrundiType.Small); return }
    val kidOn = st.on
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        CrundiTextField(
            k.pinText, ev.onPinText, Modifier.weight(1f), placeholder = if (st.pinSet) "New PIN (4-8 digits)" else "4-8 digits", icon = "lock",
            password = true, enabled = !kidOn, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword),
        )
        CrundiButton(if (st.pinSet) "Change PIN" else "Set PIN", ev.onSetPin, enabled = !kidOn && "pin" !in k.busy && k.pinText.isNotEmpty())
    }
    Hint("One PIN for kid mode and the PC lock. Five wrong tries lock it for a minute, then longer.")

    val lock = k.lock ?: return
    Sub("Lock the PC")
    val busy = "lock" in k.busy
    if (lock.on) {
        val at = if (lock.since > 0) " since " + java.text.SimpleDateFormat("HH:mm", java.util.Locale.getDefault()).format(java.util.Date(lock.since)) else ""
        Text(
            "The PC is locked (" + (if (lock.mode == "visible") "visible" else "full") + ")" + at + ".",
            style = CrundiType.Small.copy(color = CrundiColors.Yellow), modifier = Modifier.padding(bottom = 8.dp),
        )
        CrundiButton("Unlock the PC", ev.onUnlock, icon = "key", enabled = !busy)
    } else {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiButton("Lock, visible", { ev.onLock("visible") }, icon = "lock", enabled = st.pinSet && !busy)
            CrundiButton("Lock, full", { ev.onLock("full") }, icon = "lock", enabled = st.pinSet && !busy)
        }
    }
    lock.auto?.let { au ->
        val can = st.pinSet && "auto" !in k.busy
        mv.obitech.crundi.ui.components.CrundiSwitchRow(
            "Lock by itself after", au.on, { ev.onAutoLock(au.copy(on = it)) },
            Modifier.padding(top = 10.dp), enabled = can,
        )
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            for (m in lock.autoMinutes) CrundiChip(autoMinutesLabel(m), au.minutes == m, { if (can && au.minutes != m) ev.onAutoLock(au.copy(minutes = m)) })
        }
        Text("idle, as", style = CrundiType.Small.copy(color = CrundiColors.TextPrimary), modifier = Modifier.padding(top = 8.dp, bottom = 6.dp))
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            for ((id, label) in listOf("full" to "full lock", "visible" to "visible lock")) CrundiChip(label, au.mode == id, { if (can && au.mode != id) ev.onAutoLock(au.copy(mode = id)) })
        }
        Hint("Idle: no keyboard or mouse on the PC. It also locks when the computer itself locks.")
    }
    Hint("On the PC app: Ctrl+Shift+L locks it with the screen still visible behind a see-through overlay; Super+Shift+L (the Windows key) locks it with an opaque lock screen. Only your PIN unlocks it there; from here you can unlock it straight away. While it is locked, Hey Cru still works as usual: your voice recognition decides what Cru may change.")
}
