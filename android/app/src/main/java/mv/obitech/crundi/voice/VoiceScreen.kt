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
import mv.obitech.crundi.ui.components.ButtonKind
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
 * moving with Master's voice while listening and with Crundi's while it
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
    /** Yes / No on a question awaiting Master's answer. */
    onAnswer: (String) -> Unit = {},
    modifier: Modifier = Modifier,
) {
    val s = ui.state
    val cells = remember(shell?.projects) { cellsOf(shell?.projects.orEmpty()) }
    val arcs = remember(shell?.usage, ui.spend) {
        val sp = ui.spend
        UsageArcs(
            week = shell?.usage?.weekUsed,
            fiveHour = shell?.usage?.fiveHourUsed,
            api = sp?.let { if (it.monthlyLimitUsd > 0) (it.spend.claudeUsd / it.monthlyLimitUsd).toFloat() else 1f },
            apiLabel = sp?.let { "Claude $" + "%.2f".format(java.util.Locale.US, it.spend.claudeUsd) + " / $" + fmtUsd(it.monthlyLimitUsd) }.orEmpty(),
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
            TopRow(ui, phase, onClose)
            VoiceField(
                phase, ca, cb, mic, voice, cells, arcs,
                Modifier.fillMaxWidth().weight(1.5f).padding(top = 8.dp).semantics { contentDescription = statusText(ui, phase) },
            )
            UsageLegend(arcs)
            Words(ui, phase, onAllowMic, Modifier.weight(1f, fill = true))
            // A question awaiting Master's yes: answered by voice (Phase 4), no buttons.
            if (s.question.isNotEmpty() && phase != Phase.Thinking) Text("Say yes or no", style = CrundiType.Small.copy(color = CrundiColors.AccentHover), modifier = Modifier.padding(top = 4.dp))
            if (s.needsUnlock && onUnlock != null) CrundiButton("Unlock the phone", onUnlock, kind = ButtonKind.Primary, icon = "lock", modifier = Modifier.padding(top = 6.dp))
            BottomRow(ui, phase, ca, onMic, onClose, onUnlock)
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

@Composable
private fun TopRow(ui: VoiceUi, phase: Phase, onClose: () -> Unit) {
    Row(Modifier.fillMaxWidth().padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        Text("CRUNDI", style = CrundiType.Wordmark.copy(fontSize = 14.sp, color = CrundiColors.TextSecondary))
        if (ui.server.isNotEmpty()) Text("  ·  " + ui.server, style = CrundiType.Meta.copy(fontSize = 12.sp), maxLines = 1, modifier = Modifier.weight(1f, fill = false))
        Spacer(Modifier.weight(1f))
        StatusPill(statusText(ui, phase), phase)
    }
}

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
private fun Words(ui: VoiceUi, phase: Phase, onAllowMic: () -> Unit, modifier: Modifier) {
    val s = ui.state
    // Scrolls: a long answer is never cut off.
    Column(modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(top = 10.dp).animateContentSize(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        when {
            ui.needsMic -> Problem("mic", "Crundi needs the microphone to hear you.") {
                CrundiButton("Allow the microphone", onAllowMic, kind = ButtonKind.Primary, icon = "mic")
            }
            ui.blocked.isNotEmpty() -> Problem("alert-triangle", ui.blocked)
            else -> {
                // What Master said: live while listening, then what was asked.
                val heard = if (phase == Phase.Listening || phase == Phase.Thinking) s.heard else s.asked
                AnimatedVisibility(heard.isNotEmpty(), enter = fadeIn(), exit = fadeOut()) {
                    Text(
                        "“" + heard + "”",
                        style = CrundiType.Body.copy(fontSize = 17.sp, lineHeight = 24.sp, color = CrundiColors.TextSecondary),
                        textAlign = TextAlign.Center, maxLines = 4,
                    )
                }
                if (heard.isEmpty() && s.reply.isEmpty() && phase == Phase.Listening) {
                    Text(if (s.locked) "Ask anything. Changes wait for an unlock." else "Ask me anything, Master.", style = CrundiType.Body.copy(fontSize = 16.sp, color = CrundiColors.TextMuted), textAlign = TextAlign.Center)
                }
                AnimatedVisibility(s.reply.isNotEmpty(), enter = fadeIn(tween(300)), exit = fadeOut()) {
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

@Composable
private fun BottomRow(ui: VoiceUi, phase: Phase, accent: Color, onMic: () -> Unit, onClose: () -> Unit, onUnlock: (() -> Unit)?) {
    Row(Modifier.fillMaxWidth().padding(top = 12.dp, bottom = 18.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.weight(1f)) {
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
        val icon = if (phase == Phase.Speaking) "stop" else "mic"
        val label = when (phase) { Phase.Speaking -> "Stop talking"; Phase.Ended -> "Talk again"; else -> "Listening" }
        val ring = rememberInfiniteTransition(label = "ring").animateFloat(0f, 1f, infiniteRepeatable(tween(1600, easing = LinearEasing)), label = "r")
        Box(Modifier.size(84.dp), contentAlignment = Alignment.Center) {
            if (phase == Phase.Listening) Canvas(Modifier.fillMaxSize()) {
                val p = ring.value
                drawCircle(accent.copy(alpha = 0.35f * (1f - p)), size.minDimension / 2f * (0.78f + 0.22f * p), style = Stroke(2.dp.toPx()))
            }
            Box(
                Modifier.size(64.dp).clip(CircleShape)
                    .background(Brush.linearGradient(listOf(accent, lerp(accent, CrundiColors.AccentDeep, 0.6f))))
                    .border(1.dp, Color.White.copy(alpha = 0.18f), CircleShape)
                    .clickable(onClickLabel = label, onClick = onMic)
                    .semantics { contentDescription = label },
                contentAlignment = Alignment.Center,
            ) { CrundiIcon(icon, size = 26.dp, tint = Color.White) }
        }
        Box(Modifier.weight(1f), contentAlignment = Alignment.CenterEnd) {
            Box(
                Modifier.size(44.dp).clip(CircleShape).background(CrundiColors.BgTertiary.copy(alpha = 0.8f)).clickable(onClickLabel = "Close", onClick = onClose).semantics { contentDescription = "Close" },
                contentAlignment = Alignment.Center,
            ) { CrundiIcon("x", size = 18.dp, tint = CrundiColors.TextSecondary) }
        }
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
        Phase.Listening -> if (ui.state.offline) "Listening (offline)" else "Listening"
        Phase.Thinking -> "Thinking"
        Phase.Speaking -> "Speaking"
        Phase.Ended -> "Asleep"
    }
}
