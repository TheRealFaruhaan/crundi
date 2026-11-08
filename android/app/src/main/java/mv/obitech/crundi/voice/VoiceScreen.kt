package mv.obitech.crundi.voice

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.withFrameMillis
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.layout.positionInParent
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import mv.obitech.crundi.api.VoiceApi
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.MarkdownText
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin


/**
 * The voice assistant's screen, over the lock screen or the app: the Crundi
 * head at the centre of a field of the projects (VoiceField), its voice lines
 * moving with the owner's voice while listening and with Crundi's while it
 * speaks; what was heard as it is heard, the spoken answer, and the detail
 * that is only shown (D28).
 *
 * [shell] is the app's live picture of the server (projects and their state,
 * usage); null shows the head alone. [mic] and [voice] are read every frame,
 * not as state, so the field runs at the display's rate without recomposing.
 */
@Composable
fun VoiceScreen(
    ui: VoiceUi,
    mic: () -> Float,
    voice: () -> Float,
    onMic: () -> Unit,
    onClose: () -> Unit,
    onUnlock: (() -> Unit)? = null,
    onAllowMic: () -> Unit = {},
    shell: mv.obitech.crundi.ui.shell.ShellData? = null,
    /** Yes / No on a question awaiting the owner's answer. */
    onAnswer: (String) -> Unit = {},
    /** Asleep, listening on the phone for the wake word: says so. */
    waitingHint: String = "",
    /** The × by the owner's words: take back what he said, until the answer comes. */
    onCancel: () -> Unit = {},
    /** Compact, the chat view, keyboard mode. */
    more: VoiceMore = VoiceMore(),
    modifier: Modifier = Modifier,
) {
    val s = ui.state
    val cells = remember(shell?.projects) { cellsOf(shell?.projects.orEmpty()) }
    val arcs = remember(shell?.usage, ui.spend) {
        // Cru answering through Claude Code has no API spend: only the API brain shows it (an older server has only that one).
        val sp = ui.spend
        val apiSp = sp?.takeIf { it.brain != "claude-code" }
        UsageArcs(
            week = shell?.usage?.weekUsed,
            fiveHour = shell?.usage?.fiveHourUsed,
            api = apiSp?.let { if (it.monthlyLimitUsd > 0) (it.spend.claudeUsd / it.monthlyLimitUsd).toFloat() else 1f },
            apiLabel = apiSp?.let { "Claude $" + "%.2f".format(java.util.Locale.US, it.spend.claudeUsd) + " / $" + fmtUsd(it.monthlyLimitUsd) }.orEmpty(),
            deepgram = sp?.let { if (it.deepgramLimitUsd > 0) (it.spend.deepgramUsd / it.deepgramLimitUsd).toFloat() else 1f },
            deepgramLabel = sp?.let { "Deepgram $" + "%.2f".format(java.util.Locale.US, it.spend.deepgramUsd) + " / $" + fmtUsd(it.deepgramLimitUsd) }.orEmpty(),
        )
    }
    val phase = if (ui.blocked.isNotEmpty() || ui.needsMic) Phase.Ended else s.phase
    val (a, b) = palette(phase)
    val ca by animateColorAsState(a, tween(450), label = "orbA")
    val cb by animateColorAsState(b, tween(450), label = "orbB")

    Box(
        modifier.fillMaxSize().background(CrundiColors.BgPrimary),
    ) {
        Column(Modifier.fillMaxSize().safeDrawingPadding().padding(horizontal = 20.dp), horizontalAlignment = Alignment.CenterHorizontally) {
            TopRow(ui, phase, more)
            // The usage, right under the top row (a phone has no room beside the mark).
            UsageLegend(arcs)
            val chatView = ui.chat && VoiceApi.HISTORY in ui.caps
            if (chatView) {
                ChatList(ui, more, Modifier.fillMaxWidth().weight(1f).padding(top = 8.dp))
                if (ui.needsMic || ui.blocked.isNotEmpty()) Words(ui, phase, onAllowMic, onCancel, Modifier.padding(bottom = 4.dp))
                else if (s.notice.isNotEmpty()) Text(s.notice, style = CrundiType.Small.copy(color = CrundiColors.Yellow), textAlign = TextAlign.Center, modifier = Modifier.padding(top = 6.dp))
            } else {
                VoiceField(
                    phase, ca, cb, mic, voice, cells, arcs,
                    Modifier.fillMaxWidth().weight(1.5f).padding(top = 8.dp).semantics { contentDescription = statusText(ui, phase) },
                )
                Words(ui, phase, onAllowMic, onCancel, Modifier.weight(1f, fill = true))
            }
            if (waitingHint.isNotEmpty() && phase == Phase.Ended && ui.blocked.isEmpty() && !ui.keyboard) Text(waitingHint, style = CrundiType.Small.copy(color = CrundiColors.AccentHover), modifier = Modifier.padding(top = 4.dp))
            // A question awaiting the owner's yes: answered by voice (Phase 4), no buttons.
            if (s.question.isNotEmpty() && phase != Phase.Thinking) Text(if (ui.keyboard) "Type yes or no" else "Say yes or no", style = CrundiType.Small.copy(color = CrundiColors.AccentHover), modifier = Modifier.padding(top = 4.dp))
            if (s.needsUnlock && onUnlock != null) CrundiButton("Unlock the phone", onUnlock, kind = ButtonKind.Primary, icon = "lock", modifier = Modifier.padding(top = 6.dp))
            BottomRow(ui, phase, ca, mic, onMic, onClose, onUnlock, more)
        }
    }
}

/** Usage under the field, in the arcs' colours: week, 5 hours, Claude and Deepgram spend. */
@OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
@Composable
private fun UsageLegend(u: UsageArcs) {
    val items = listOfNotNull(
        u.week?.let { CrundiColors.AccentHover to "wk ${(it * 100).toInt()}%" },
        u.fiveHour?.let { CrundiColors.Sky to "5h ${(it * 100).toInt()}%" },
        u.api?.let { CrundiColors.Ask to u.apiLabel },
        u.deepgram?.let { CrundiColors.Green to u.deepgramLabel },
    )
    if (items.isEmpty()) return
    androidx.compose.foundation.layout.FlowRow(Modifier.fillMaxWidth().padding(top = 4.dp), horizontalArrangement = Arrangement.spacedBy(14.dp, Alignment.CenterHorizontally), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        for ((col, label) in items) Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
            Box(Modifier.size(7.dp).clip(CircleShape).background(col))
            Text(label, style = CrundiType.Meta.copy(fontSize = 11.sp, color = col.copy(alpha = 0.9f)))
        }
    }
}

/** What the voice screen's newer buttons do; each defaults to nothing (a host that has not wired it). */
class VoiceMore(
    /** The Compact button (capability "voice-compact"). */
    val onCompact: () -> Unit = {},
    /** The chat view on (true) or off (capability "voice-history"). */
    val onChat: (Boolean) -> Unit = {},
    /** "Earlier messages" in the chat view. */
    val onMoreHistory: () -> Unit = {},
    /** Keyboard mode on (true) or back to voice. */
    val onKeyboard: (Boolean) -> Unit = {},
    /** Words typed in keyboard mode, sent. */
    val onSend: (String) -> Unit = {},
    /** The owner is typing: activity, for the timeouts. */
    val onTyping: () -> Unit = {},
)

@Composable
private fun TopRow(ui: VoiceUi, phase: Phase, more: VoiceMore) {
    Row(Modifier.fillMaxWidth().padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        CruIcon(22.dp, live = phase == Phase.Listening || phase == Phase.Thinking || phase == Phase.Speaking, modifier = Modifier.semantics { contentDescription = "Cru" })
        if (ui.server.isNotEmpty()) Text("  ·  " + ui.server, style = CrundiType.Meta.copy(fontSize = 12.sp), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
        Spacer(Modifier.weight(1f))
        if (ui.spend?.adminMode == true) AdminPill()
        StatusPill(statusText(ui, phase), phase)
        if (VoiceApi.COMPACT in ui.caps) CrundiIconButton("minimize-2", more.onCompact, contentDescription = "Compact the conversation", size = 16.dp, touch = 38.dp)
        if (VoiceApi.HISTORY in ui.caps) Box {
            CrundiIconButton(
                "message-circle", { more.onChat(!ui.chat) },
                contentDescription = if (ui.chat) "Back to the voice view" else "Chat view",
                size = 17.dp, touch = 38.dp, tint = if (ui.chat) CrundiColors.AccentHover else CrundiColors.TextSecondary,
            )
            // Cru said something nobody heard: a red dot until the chat view is opened.
            val unread by CruUnread.count.collectAsState()
            if (unread > 0 && !ui.chat) Box(Modifier.align(Alignment.TopEnd).padding(top = 6.dp, end = 6.dp).size(8.dp).clip(CircleShape).background(CrundiColors.Red))
        }
    }
}

/** Admin mode is on: Cru can run any command on the server. */
@Composable
private fun AdminPill() {
    Text(
        "ADMIN",
        style = CrundiType.Badge.copy(fontSize = 10.sp, color = CrundiColors.Red, fontWeight = FontWeight.Bold, letterSpacing = 0.8.sp),
        modifier = Modifier.padding(end = 6.dp).clip(RoundedCornerShape(10.dp)).background(CrundiColors.RedDim).border(1.dp, CrundiColors.Red.copy(alpha = 0.5f), RoundedCornerShape(10.dp))
            .padding(horizontal = 8.dp, vertical = 3.dp)
            .semantics { contentDescription = "Admin mode is on: Cru can run any command" },
    )
}

/**
 * The chat view: the conversation as messages, the owner's on the right and
 * Cru's on the left (what is only shown under what it said), the words being
 * heard as a faded bubble of his, newest at the bottom.
 */
@Composable
private fun ChatList(ui: VoiceUi, more: VoiceMore, modifier: Modifier) {
    val list = ui.history
    val hearing = CruChat.hearing(ui.state)
    val state = rememberLazyListState()
    val count = list.size + (if (hearing.isNotEmpty()) 1 else 0)
    // New messages: follow them down, unless the owner has scrolled up to read (older ones loaded above don't move it).
    val lastIndex = (if (ui.moreHistory) 1 else 0) + (if (list.isEmpty() && hearing.isEmpty()) 1 else 0) + list.size + (if (hearing.isNotEmpty()) 1 else 0) - 1
    val first = remember { booleanArrayOf(true) }
    LaunchedEffect(list.lastOrNull()?.key, hearing.isNotEmpty(), ui.historyLoaded) {
        val info = state.layoutInfo
        val atEnd = info.visibleItemsInfo.lastOrNull()?.let { it.index >= info.totalItemsCount - 2 } ?: true
        if (lastIndex >= 0 && (first[0] || atEnd)) state.scrollToItem(lastIndex)
        if (list.isNotEmpty()) first[0] = false
    }
    val keys = remember(list) { val seen = HashSet<String>(); list.map { m -> var k = m.key; while (!seen.add(k)) k += "'"; k } }
    LazyColumn(modifier, state = state, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (ui.moreHistory) item(key = "more") {
            Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) { CrundiButton("Earlier messages", more.onMoreHistory, kind = ButtonKind.Ghost, icon = "chevron-up") }
        }
        if (list.isEmpty() && hearing.isEmpty()) item(key = "none") {
            Text(
                when {
                    ui.historyError.isNotEmpty() -> ui.historyError
                    !ui.historyLoaded -> "Loading the conversation…"
                    else -> "No messages yet."
                },
                style = CrundiType.Small.copy(color = if (ui.historyError.isNotEmpty()) CrundiColors.Yellow else CrundiColors.TextMuted),
                textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth().padding(top = 24.dp),
            )
        }
        items(list.size, key = { keys[it] }) { i ->
            val m = list[i]
            Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                if (m.said.isNotEmpty()) Bubble(mine = true, text = m.said, detail = "", time = msgTime(m.at))
                if (m.speak.isNotEmpty() || m.show.isNotEmpty()) Bubble(mine = false, text = m.speak, detail = m.show, time = if (m.said.isEmpty()) msgTime(m.at) else "")
            }
        }
        if (hearing.isNotEmpty()) item(key = "hearing") { Bubble(mine = true, text = hearing, detail = "", time = "", faded = true) }
    }
}

@Composable
private fun Bubble(mine: Boolean, text: String, detail: String, time: String, faded: Boolean = false) {
    val shape = RoundedCornerShape(topStart = 16.dp, topEnd = 16.dp, bottomStart = if (mine) 16.dp else 4.dp, bottomEnd = if (mine) 4.dp else 16.dp)
    Box(Modifier.fillMaxWidth(), contentAlignment = if (mine) Alignment.CenterEnd else Alignment.CenterStart) {
        Column(
            Modifier.fillMaxWidth(0.86f).wrapContentWidth(if (mine) Alignment.End else Alignment.Start)
                .alpha(if (faded) 0.55f else 1f)
                .clip(shape)
                .background(if (mine) CrundiColors.AccentDim else CrundiColors.BgSecondary.copy(alpha = 0.9f))
                .border(1.dp, if (mine) CrundiColors.Accent.copy(alpha = 0.35f) else CrundiColors.Border, shape)
                .padding(horizontal = 12.dp, vertical = 8.dp),
            horizontalAlignment = if (mine) Alignment.End else Alignment.Start,
        ) {
            if (text.isNotEmpty()) Text(text, style = CrundiType.Body.copy(fontSize = 15.sp, lineHeight = 21.sp, color = CrundiColors.TextPrimary))
            if (detail.isNotEmpty()) Box(Modifier.padding(top = if (text.isNotEmpty()) 6.dp else 0.dp)) { MarkdownText(detail, compact = true) }
            if (time.isNotEmpty()) Text(time, style = CrundiType.Meta.copy(fontSize = 10.sp, color = CrundiColors.TextMuted), modifier = Modifier.padding(top = 3.dp))
        }
    }
}

/** "9 Nov 14:05". */
private fun msgTime(at: Long): String = if (at <= 0) "" else java.text.SimpleDateFormat("d MMM HH:mm", java.util.Locale.getDefault()).format(java.util.Date(at))

@Composable
private fun StatusPill(text: String, phase: Phase) {
    val (a, _) = palette(phase)
    val pulse = rememberInfiniteTransition(label = "pill").animateFloat(0.35f, 1f, infiniteRepeatable(tween(900), RepeatMode.Reverse), label = "pulse")
    Row(
        Modifier.clip(RoundedCornerShape(14.dp)).background(a.copy(alpha = 0.14f)).border(1.dp, a.copy(alpha = 0.35f), RoundedCornerShape(14.dp)).padding(horizontal = 12.dp, vertical = 5.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(7.dp),
    ) {
        Canvas(Modifier.size(7.dp)) { drawCircle(a.copy(alpha = if (phase == Phase.Ended) 0.6f else pulse.value)) }
        Text(text, style = CrundiType.Small.copy(color = lerp(a, Color.White, 0.35f), fontWeight = FontWeight.Medium))
    }
}

@Composable
private fun Words(ui: VoiceUi, phase: Phase, onAllowMic: () -> Unit, onCancel: () -> Unit, modifier: Modifier) {
    val s = ui.state
    // Scrolls: a long answer is never cut off. While Cru says it, the part being said stays in view.
    val scroll = rememberScrollState()
    val box = remember { FloatArray(3) }   // the reply's top and height in the scrolled column, the visible height
    val own = remember { mutableStateOf(false) }
    LaunchedEffect(s.reply) { own.value = false }
    val rate = (ui.spend?.speechRate ?: 1.0).toFloat()
    LaunchedEffect(phase == Phase.Speaking, s.reply) {
        if (phase != Phase.Speaking || s.reply.isEmpty()) return@LaunchedEffect
        val from = System.currentTimeMillis()
        val lasts = SpeechFollow.durationMs(s.reply, rate)
        while (true) {
            withFrameMillis { }
            if (own.value) break
            val target = SpeechFollow.target(System.currentTimeMillis() - from, lasts, box[0], box[1], box[2], scroll.maxValue.toFloat()) ?: continue
            val cur = scroll.value.toFloat()
            if (kotlin.math.abs(target - cur) > 1f) scroll.scrollTo((cur + (target - cur) * 0.12f).toInt())
        }
    }
    Column(
        modifier.fillMaxWidth().onSizeChanged { box[2] = it.height.toFloat() }
            // The owner touching the text scrolls it himself: stop following until the next reply.
            .pointerInput(Unit) { awaitEachGesture { awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial); own.value = true } }
            .verticalScroll(scroll).padding(top = 10.dp).animateContentSize(),
        horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        when {
            ui.needsMic -> Problem("mic", "Cru needs the microphone to hear you.") {
                CrundiButton("Allow the microphone", onAllowMic, kind = ButtonKind.Primary, icon = "mic")
            }
            ui.blocked.isNotEmpty() -> Problem("alert-triangle", ui.blocked)
            else -> {
                // What the owner said: live while listening, then what was asked.
                val heard = if (phase == Phase.Listening || phase == Phase.Thinking) s.heard else s.asked
                AnimatedVisibility(heard.isNotEmpty(), enter = fadeIn(), exit = fadeOut()) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(
                            "“" + heard + "”",
                            style = CrundiType.Body.copy(fontSize = 17.sp, lineHeight = 24.sp, color = CrundiColors.TextSecondary),
                            textAlign = TextAlign.Center, maxLines = 4,
                            modifier = Modifier.weight(1f, fill = false),
                        )
                        // Until the answer comes, what he said can be taken back.
                        if (ui.blocked.isEmpty() && s.canCancel) {
                            Box(
                                Modifier.padding(start = 6.dp).size(32.dp).clip(CircleShape).background(CrundiColors.BgSecondary)
                                    .clickable(onClickLabel = "Cancel what I said", onClick = onCancel)
                                    .semantics { contentDescription = "Cancel what I said" },
                                contentAlignment = Alignment.Center,
                            ) { CrundiIcon("x", size = 15.dp, tint = CrundiColors.TextSecondary) }
                        }
                    }
                }
                if (heard.isEmpty() && s.reply.isEmpty() && phase == Phase.Listening) {
                    Text(if (s.locked) "Ask anything. Changes wait for an unlock." else if (ui.keyboard) "Type to Cru below, Sir." else "Ask me anything, Sir.", style = CrundiType.Body.copy(fontSize = 16.sp, color = CrundiColors.TextMuted), textAlign = TextAlign.Center)
                }
                AnimatedVisibility(s.reply.isNotEmpty(), enter = fadeIn(tween(300)), exit = fadeOut(), modifier = Modifier.onGloballyPositioned { box[0] = it.positionInParent().y; box[1] = it.size.height.toFloat() }) {
                    Text(s.reply, style = CrundiType.Title.copy(fontSize = 18.sp, lineHeight = 25.sp, fontWeight = FontWeight.Medium), textAlign = TextAlign.Center)
                }
                if (s.detail.isNotEmpty()) {
                    Box(
                        Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp))
                            .background(CrundiColors.BgSecondary.copy(alpha = 0.85f)).border(1.dp, CrundiColors.Border, RoundedCornerShape(14.dp))
                            .padding(14.dp),
                    ) { MarkdownText(s.detail, compact = true) }
                }
                if (s.notice.isNotEmpty()) Text(s.notice, style = CrundiType.Small.copy(color = CrundiColors.Yellow), textAlign = TextAlign.Center)
            }
        }
    }
}

@Composable
private fun Problem(icon: String, text: String, action: @Composable () -> Unit = {}) {
    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        CrundiIcon(icon, size = 26.dp, tint = CrundiColors.Yellow)
        Text(text, style = CrundiType.Body.copy(fontSize = 16.sp, lineHeight = 23.sp), textAlign = TextAlign.Center)
        action()
    }
}

/**
 * The bottom row. Voice: the keyboard button on the left, the liquid button
 * in the middle, close on the right. Keyboard mode: the button back to
 * voice, the text field and send (stop while Cru talks), close.
 */
@Composable
private fun BottomRow(ui: VoiceUi, phase: Phase, accent: Color, mic: () -> Float, onMic: () -> Unit, onClose: () -> Unit, onUnlock: (() -> Unit)?, more: VoiceMore) {
    val canSwitch = ui.blocked.isEmpty()
    if (ui.keyboard) {
        var typed by rememberSaveable { mutableStateOf("") }
        val send = { val t = typed.trim(); if (t.isNotEmpty()) { more.onSend(t); typed = "" } }
        Row(Modifier.fillMaxWidth().padding(top = 12.dp, bottom = 18.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            if (canSwitch) RoundButton("mic", "Back to voice") { more.onKeyboard(false) }
            CrundiTextField(
                typed, { typed = it; more.onTyping() }, Modifier.weight(1f),
                placeholder = "Type to Cru", singleLine = false, maxLines = 4,
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send, capitalization = KeyboardCapitalization.Sentences),
                keyboardActions = KeyboardActions(onSend = { send() }),
            )
            if (phase == Phase.Speaking && typed.isBlank()) RoundButton("stop", "Stop talking", onClick = onMic)
            else RoundButton("send", "Send", enabled = typed.isNotBlank(), accent = true, onClick = send)
            RoundButton("x", "Close", onClick = onClose)
        }
        return
    }
    Row(Modifier.fillMaxWidth().padding(top = 12.dp, bottom = 18.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            if (canSwitch) RoundButton("keyboard", "Keyboard mode") { more.onKeyboard(true) }
            if (ui.state.locked && onUnlock != null) {
                Row(
                    Modifier.clip(RoundedCornerShape(16.dp)).background(CrundiColors.BgTertiary).border(1.dp, CrundiColors.Border, RoundedCornerShape(16.dp)).clickable(onClick = onUnlock).padding(horizontal = 12.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    CrundiIcon("lock", size = 14.dp, tint = CrundiColors.TextSecondary)
                    Text("Unlock", style = CrundiType.Small)
                }
            }
            }
        }
        val label = when (phase) { Phase.Speaking -> "Stop talking"; Phase.Ended -> "Talk again"; else -> "Listening" }
        // The animated blob alone: no microphone drawn on it.
        FluidMic(phase, accent, mic, null, label, onMic)
        Box(Modifier.weight(1f), contentAlignment = Alignment.CenterEnd) { RoundButton("x", "Close", onClick = onClose) }
    }
}

/** A round 44dp button of the bottom row. */
@Composable
private fun RoundButton(icon: String, label: String, enabled: Boolean = true, accent: Boolean = false, onClick: () -> Unit) {
    Box(
        Modifier.size(44.dp).alpha(if (enabled) 1f else 0.45f).clip(CircleShape)
            .background(if (accent) CrundiColors.Accent else CrundiColors.BgTertiary.copy(alpha = 0.8f))
            .clickable(enabled = enabled, onClickLabel = label, onClick = onClick).semantics { contentDescription = label },
        contentAlignment = Alignment.Center,
    ) { CrundiIcon(icon, size = 18.dp, tint = if (accent) Color.White else CrundiColors.TextSecondary) }
}

/** Following Cru's reply down as it is said (crundi-voice.js followSpeech), with no Compose in it (SpeechFollowTest). */
object SpeechFollow {
    /** How long [text] takes to say: 14 characters a second at normal speed, at least 1.5 s. */
    fun durationMs(text: String, rate: Float): Long = maxOf(1500L, (text.length / (14f * rate.coerceAtLeast(0.1f)) * 1000f).toLong())

    /**
     * Where to scroll to, [elapsed] ms into saying a reply lasting [lasts]:
     * the point that far through it ([top], [height] in the scrolled column)
     * a third of the way down the visible [view]. Null when it all fits.
     */
    fun target(elapsed: Long, lasts: Long, top: Float, height: Float, view: Float, max: Float): Float? {
        if (max < 4f || height <= 0f) return null
        val frac = (elapsed.toFloat() / lasts.coerceAtLeast(1L)).coerceIn(0f, 1f)
        return (top + frac * height - view / 3f).coerceIn(0f, max)
    }
}

/** The two colours of the orb for each phase. */
fun palette(phase: Phase): Pair<Color, Color> = when (phase) {
    Phase.Starting -> CrundiColors.Accent to CrundiColors.AccentDeep
    Phase.Listening -> CrundiColors.Sky to CrundiColors.Accent
    Phase.Thinking -> CrundiColors.Ask to CrundiColors.AccentDeep
    Phase.Speaking -> CrundiColors.AccentHover to CrundiColors.Ask
    Phase.Ended -> CrundiColors.TextMuted to CrundiColors.BgHover
}

fun statusText(ui: VoiceUi, phase: Phase): String = when {
    ui.needsMic -> "Microphone off"
    ui.blocked.isNotEmpty() -> "Unavailable"
    else -> when (phase) {
        Phase.Starting -> "Waking"
        Phase.Listening -> if (ui.keyboard) "Ready" else if (ui.state.offline) "Listening (offline)" else "Listening"
        Phase.Thinking -> "Thinking"
        Phase.Speaking -> "Speaking"
        Phase.Ended -> "Asleep"
    }
}
