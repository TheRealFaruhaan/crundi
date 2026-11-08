package mv.obitech.crundi.voice

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import mv.obitech.crundi.api.EnrolReply
import mv.obitech.crundi.api.SpeakerStatus
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiSwitchRow
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType

/** One phrase read while enrolling: kept ([accepted]) or why not. */
data class EnrolResult(val n: Int, val accepted: Boolean, val seconds: Double, val consistency: Double?, val problem: String)

/**
 * "Your voice" (capability "voice-speaker"): whether Cru knows the owner's
 * voice, and the enrolment under way: the phrase being read ([pos] of the
 * server's prompts), each phrase's result, recording now, a note.
 */
data class SpeakerUi(
    val status: SpeakerStatus? = null,
    val error: String = "",
    val active: Boolean = false,
    val pos: Int = 0,
    val results: List<EnrolResult> = emptyList(),
    val recording: Boolean = false,
    /** The microphone's level while recording, 0..1. */
    val level: Float = 0f,
    val busy: Boolean = false,
    val msg: String = "",
    val bad: Boolean = false,
    /** Forget was tapped once: tap again to forget. */
    val confirmForget: Boolean = false,
)

/** The enrolment's rules, with no Android in it (EnrolmentTest). */
object Enrolment {
    /** At most this long per phrase: the recording stops by itself. */
    const val MAX_SECONDS = 15
    const val RATE = MicStream.RATE

    /** Phrases kept so far. */
    fun kept(u: SpeakerUi) = u.results.count { it.accepted }

    /** The first phrase kept starts a fresh enrolment on the server (?restart=1). */
    fun restart(u: SpeakerUi) = u.results.none { it.accepted }

    /** Enough phrases for a profile. */
    fun canFinish(u: SpeakerUi) = kept(u) >= (u.status?.minPhrases ?: 5)

    fun start(u: SpeakerUi) = u.copy(active = true, pos = 0, results = emptyList(), recording = false, busy = false, msg = "", bad = false, confirmForget = false)

    /** What the server made of the phrase just read: on to the next, or try it again. */
    fun after(u: SpeakerUi, r: EnrolReply): SpeakerUi {
        if (!r.ok) return u.copy(busy = false, msg = r.error.ifEmpty { "Could not check that phrase" }, bad = true)
        val res = EnrolResult(u.pos, r.accepted, r.seconds, r.consistency, r.problem)
        return if (r.accepted) u.copy(busy = false, results = u.results + res, pos = u.pos + 1, msg = "", bad = false)
        else u.copy(busy = false, results = u.results + res, msg = r.problem.ifEmpty { "Try that one again." }, bad = true)
    }

    /** 0..1 of a full-scale level as the web's meter shows it. */
    fun meter(level: Float) = level.coerceIn(0f, 1f)
}

class SpeakerEvents(
    val onStart: () -> Unit = {},
    /** Record, or stop and send the phrase. */
    val onRecord: () -> Unit = {},
    val onFinish: () -> Unit = {},
    val onCancel: () -> Unit = {},
    val onForget: () -> Unit = {},
    val onOwnerOnly: (Boolean) -> Unit = {},
)

/** The "Your voice" fold's body, as the web's group (src/webapp-html.js, vsSpkRender). */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun SpeakerPart(u: SpeakerUi, ev: SpeakerEvents) {
    val st = u.status
    if (st == null) {
        Text(u.error.ifEmpty { "Loading…" }, style = CrundiType.Small.copy(color = if (u.error.isNotEmpty()) CrundiColors.Yellow else CrundiColors.TextSecondary))
        return
    }
    @Composable fun Say(t: String, bad: Boolean) { if (t.isNotEmpty()) Text(t, style = CrundiType.Small.copy(color = if (bad) CrundiColors.Red else CrundiColors.TextSecondary), modifier = Modifier.padding(top = 8.dp)) }
    if (!u.active) {
        when {
            !st.available -> Say("The voice check is not available on this server: onnxruntime-node is not installed.", false)
            st.enrolled -> Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                CrundiIcon("check", size = 14.dp, tint = CrundiColors.Green)
                Text("Enrolled" + (st.enrolledAt?.take(10)?.let { " on $it" } ?: "") + ": ${st.phrases} phrases, ${fmtSec(st.seconds)} s of speech", style = CrundiType.Small)
            }
            else -> Say("Not enrolled yet: Cru acts on any voice. Read a few short phrases so it knows yours (about a minute).", false)
        }
        if (st.available) {
            CrundiSwitchRow(
                "Only act on my voice", st.ownerOnly, ev.onOwnerOnly, Modifier.padding(top = 4.dp),
                detail = "Anyone may still ask Cru questions; only your voice, or typing, changes anything. Admin mode always needs your voice.",
                enabled = st.enrolled && !u.busy,
            )
            FlowRow(Modifier.padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                CrundiButton(if (st.enrolled) "Enrol again" else "Enrol my voice", ev.onStart, kind = if (st.enrolled) ButtonKind.Secondary else ButtonKind.Primary, icon = "mic", enabled = !u.busy)
                if (st.enrolled) CrundiButton(if (u.confirmForget) "Forget it for good?" else "Forget my voice", ev.onForget, kind = ButtonKind.Danger, icon = "trash", enabled = !u.busy)
            }
            if (u.confirmForget) Say("Cru will act on any voice again, and admin mode will not check who is speaking.", false)
        }
        Say(u.msg, u.bad)
        return
    }
    val prompts = st.prompts
    if (u.pos < prompts.size) {
        Text("Phrase ${u.pos + 1} of ${prompts.size}: tap Record, read it as you would say it to Cru, then Stop", style = CrundiType.Small, modifier = Modifier.padding(top = 4.dp))
        Text(prompts[u.pos], style = CrundiType.Body.copy(fontSize = 17.sp, lineHeight = 24.sp, color = CrundiColors.TextPrimary), modifier = Modifier.padding(vertical = 10.dp))
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            CrundiButton(if (u.recording) "Stop" else "Record", ev.onRecord, kind = ButtonKind.Primary, icon = if (u.recording) "stop" else "mic", enabled = !u.busy)
            Box(Modifier.width(120.dp).height(6.dp).clip(RoundedCornerShape(3.dp)).background(CrundiColors.BgTertiary)) {
                Box(Modifier.fillMaxWidth(if (u.recording) Enrolment.meter(u.level) else 0f).height(6.dp).background(CrundiColors.Accent))
            }
        }
        FlowRow(Modifier.padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            if (Enrolment.canFinish(u) && !u.recording) CrundiButton("Finish now", ev.onFinish, enabled = !u.busy)
            CrundiButton("Cancel", ev.onCancel, kind = ButtonKind.Ghost)
        }
    } else {
        Text("All phrases read.", style = CrundiType.Small, modifier = Modifier.padding(top = 4.dp))
        FlowRow(Modifier.padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiButton("Finish", ev.onFinish, kind = ButtonKind.Primary, enabled = !u.busy)
            CrundiButton("Cancel", ev.onCancel, kind = ButtonKind.Ghost)
        }
    }
    Column(Modifier.padding(top = 6.dp)) {
        for (r in u.results) Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp), modifier = Modifier.padding(top = 3.dp)) {
            val col = if (r.accepted) CrundiColors.TextSecondary else CrundiColors.Red
            CrundiIcon(if (r.accepted) "check" else "x", size = 12.dp, tint = col)
            Text(
                "Phrase ${r.n + 1}: ${fmtSec(r.seconds)} s of speech" + (r.consistency?.let { ", ${Math.round(it * 100)}% like the others" } ?: "") + (if (r.problem.isNotEmpty()) ". " + r.problem else ""),
                style = CrundiType.Meta.copy(color = col),
            )
        }
    }
    Say(if (u.recording) "Listening…" else u.msg, !u.recording && u.bad)
}

private fun fmtSec(s: Double) = if (s == Math.floor(s)) s.toLong().toString() else "%.1f".format(java.util.Locale.US, s)
