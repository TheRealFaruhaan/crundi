package mv.obitech.crundi

import android.app.KeyguardManager
import android.content.Context
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.material3.Text
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.LinearOutSlowInEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import mv.obitech.crundi.ui.components.motionStill
import androidx.compose.animation.core.spring
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.IntOffset
import kotlinx.coroutines.launch
import kotlin.math.roundToInt
import mv.obitech.crundi.ui.theme.Dimens
import mv.obitech.crundi.ui.icons.CrundiIcon
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.core.app.NotificationManagerCompat
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiTheme
import mv.obitech.crundi.ui.theme.CrundiType

/**
 * An urgent alert over the lock screen, like an alarm clock's: opened by the
 * alert's full-screen intent when the person turned on "Full-screen urgent
 * alerts" and the phone is locked or asleep. The ringing is the
 * notification's own; Dismiss and Attend take the notification away, which
 * stops it. Attend (slide right) asks for the phone to be unlocked first: what the alert is
 * about is not for whoever picks the phone up.
 */
class AlertActivity : ComponentActivity() {
    companion object {
        const val EXTRA_TITLE = "alert.title"
        const val EXTRA_BODY = "alert.body"
        const val EXTRA_WHERE = "alert.where"
        /** How long Attend waits for the lock screen to say it was unlocked before opening the chat anyway. */
        const val UNLOCK_WAIT_MS = 8_000L
    }

    private val handler = android.os.Handler(android.os.Looper.getMainLooper())
    private val ringOut = Runnable { PushLog.add(applicationContext, "The full-screen alert closed: the ring time ran out"); finish() }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setShowWhenLocked(true)
        setTurnScreenOn(true)
        show()
    }

    override fun onDestroy() {
        handler.removeCallbacks(ringOut)
        super.onDestroy()
    }

    private val serverId get() = intent.getStringExtra(Notifier.EXTRA_SERVER).orEmpty()
    private val alertId get() = intent.getStringExtra(AlertAnswers.EXTRA_ALERT).orEmpty()

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        show()
    }

    private fun show() {
        val title = intent.getStringExtra(EXTRA_TITLE).orEmpty().ifEmpty { "Urgent alert" }
        val body = intent.getStringExtra(EXTRA_BODY).orEmpty()
        val where = intent.getStringExtra(EXTRA_WHERE).orEmpty()
        // The screen goes when the ring time is up, as the notification does.
        val posted = intent.getLongExtra(AlertAnswers.EXTRA_POSTED, 0)
        val ringMs = intent.getLongExtra(AlertAnswers.EXTRA_RING_MS, 0)
        handler.removeCallbacks(ringOut)
        if (posted > 0 && ringMs > 0) {
            val left = posted + ringMs - System.currentTimeMillis()
            if (left <= 0) { finish(); return }
            handler.postDelayed(ringOut, left)
        }
        setContent { CrundiTheme { AlertScreen(title, body, where, onOpen = ::open, onDismiss = ::dismiss, onSnooze = ::snooze) } }
    }

    private fun snooze(minutes: Int) {
        stopRinging()
        AlertAnswers.snooze(this, serverId, intent.getStringExtra(AlertAnswers.EXTRA_PAYLOAD).orEmpty(), minutes)
        AlertAnswers.report(this, serverId, alertId, "snoozed", minutes = minutes)
        finish()
    }

    private fun stopRinging() {
        val id = intent.getIntExtra(Notifier.EXTRA_NOTIFICATION, 0)
        if (id != 0) NotificationManagerCompat.from(this).cancel(id)
    }

    private fun dismiss() {
        stopRinging()
        PushLog.add(applicationContext, "An urgent alert was dismissed from the full-screen alert")
        AlertAnswers.report(this, serverId, alertId, "dismissed")
        finish()
    }

    private fun open() {
        PushLog.add(applicationContext, "Urgent alert: slid to attend")
        stopRinging()
        AlertAnswers.report(this, serverId, alertId, "attended")
        val serverId = this.serverId
        val path = intent.getStringExtra(Notifier.EXTRA_PATH).orEmpty().ifEmpty { "/" }
        // Once, whatever answers first. The chat opens either way: unlocked,
        // it is in front; not (cancelled, refused, or Android never said),
        // it waits behind the lock screen for the person to unlock as usual.
        // This screen never stays: it once sat with the slider at the end
        // when only "unlocked" was listened for.
        var gone = false
        val go = { why: String ->
            if (!gone) {
                gone = true
                PushLog.add(applicationContext, "Urgent alert attended: $why")
                startActivity(
                    Intent(this, MainActivity::class.java).setAction("open:$serverId:$path")
                        .putExtra(Notifier.EXTRA_SERVER, serverId).putExtra(Notifier.EXTRA_PATH, path)
                        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP),
                )
                finish()
            }
        }
        val km = getSystemService(Context.KEYGUARD_SERVICE) as KeyguardManager
        if (!km.isKeyguardLocked) { go("the phone was unlocked"); return }
        PushLog.add(applicationContext, "Urgent alert: asking the lock screen to unlock (secure: ${km.isKeyguardSecure})")
        km.requestDismissKeyguard(this, object : KeyguardManager.KeyguardDismissCallback() {
            override fun onDismissSucceeded() { go("unlocked") }
            override fun onDismissCancelled() { go("unlock cancelled, the chat waits behind the lock screen") }
            override fun onDismissError() { go("Android could not show the unlock, the chat waits behind the lock screen") }
        })
        // Some phones never answer at all.
        window.decorView.postDelayed({ go("no answer from the lock screen, the chat waits behind it") }, UNLOCK_WAIT_MS)
    }
}

@Composable
internal fun AlertScreen(title: String, body: String, where: String, onOpen: () -> Unit = {}, onDismiss: () -> Unit = {}, onSnooze: (Int) -> Unit = {}) {
    // Slowly drifting colour behind, and rings pulsing out from the bell, as
    // the alarm sounds. Still when the phone's animations are off.
    // No transition at all when still: a running one never lets a test (or
    // a screenshot) settle.
    val (d, p) = if (motionStill()) 0.5f to 0.25f else alertMotion()
    Box(
        Modifier.fillMaxSize().drawBehind {
            drawRect(CrundiColors.BgPrimary)
            // Two soft glows that swap places: red (urgent) and the accent.
            drawCircle(
                Brush.radialGradient(listOf(CrundiColors.Red.copy(alpha = 0.32f), Color.Transparent), center = Offset(size.width * (0.15f + 0.7f * d), size.height * 0.22f), radius = size.width * 0.95f),
                radius = size.width * 0.95f, center = Offset(size.width * (0.15f + 0.7f * d), size.height * 0.22f),
            )
            drawCircle(
                Brush.radialGradient(listOf(CrundiColors.Accent.copy(alpha = 0.30f), Color.Transparent), center = Offset(size.width * (0.85f - 0.7f * d), size.height * 0.85f), radius = size.width * 1.0f),
                radius = size.width * 1.0f, center = Offset(size.width * (0.85f - 0.7f * d), size.height * 0.85f),
            )
        },
    ) {
        Column(
            Modifier.fillMaxSize().safeDrawingPadding().padding(28.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
        ) {
            // The bell, with two rings going out from it.
            Box(Modifier.size(140.dp), contentAlignment = Alignment.Center) {
                Canvas(Modifier.fillMaxSize()) {
                    for (k in 0..1) {
                        val f = (p + k * 0.5f) % 1f
                        drawCircle(CrundiColors.Red.copy(alpha = (1f - f) * 0.55f), radius = size.minDimension / 2f * (0.42f + 0.58f * f), style = Stroke(width = 3.dp.toPx()))
                    }
                }
                Box(
                    Modifier.size(64.dp).clip(CircleShape).background(Brush.linearGradient(listOf(CrundiColors.Red, CrundiColors.Accent))),
                    contentAlignment = Alignment.Center,
                ) { CrundiIcon("bell", size = 30.dp, tint = Color.White) }
            }
            Spacer(Modifier.height(22.dp))
            Text("URGENT ALERT", style = CrundiType.Meta.copy(color = CrundiColors.Red))
            Spacer(Modifier.height(10.dp))
            Text(title, style = CrundiType.Title, textAlign = TextAlign.Center)
            if (body.isNotEmpty() && body != title) {
                Spacer(Modifier.height(10.dp))
                Text(body, style = CrundiType.Body.copy(color = CrundiColors.TextSecondary), textAlign = TextAlign.Center, maxLines = 8)
            }
            if (where.isNotEmpty()) {
                Spacer(Modifier.height(10.dp))
                Text(where, style = CrundiType.Meta, textAlign = TextAlign.Center)
            }
            Spacer(Modifier.height(44.dp))
            AttendSlider(onAttend = onOpen, onDismiss = onDismiss)
            Spacer(Modifier.height(26.dp))
            Text("Snooze", style = CrundiType.Meta)
            Spacer(Modifier.height(10.dp))
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                for (m in AlertAnswers.SNOOZE_MINUTES) {
                    Box(
                        Modifier.weight(1f).height(44.dp).clip(RoundedCornerShape(22.dp))
                            .background(CrundiColors.BgTertiary.copy(alpha = 0.85f))
                            .border(Dimens.Hairline, CrundiColors.Border, RoundedCornerShape(22.dp))
                            .clickable { onSnooze(m) }
                            .semantics { contentDescription = "Snooze for ${AlertAnswers.snoozeLabel(m)}" },
                        contentAlignment = Alignment.Center,
                    ) { Text(AlertAnswers.snoozeLabel(m), style = CrundiType.Body.copy(color = CrundiColors.TextPrimary)) }
                }
            }
        }
    }
}

/** The background's drift and the rings' pulse, 0 to 1, running for ever. */
@Composable
private fun alertMotion(): Pair<Float, Float> {
    val t = rememberInfiniteTransition(label = "alert")
    val drift by t.animateFloat(0f, 1f, infiniteRepeatable(tween(6000, easing = LinearEasing), RepeatMode.Reverse), label = "drift")
    val pulse by t.animateFloat(0f, 1f, infiniteRepeatable(tween(1600, easing = LinearOutSlowInEasing), RepeatMode.Restart), label = "pulse")
    return drift to pulse
}

/** Which way the thumb has gone, as a fraction of how far it can go: -1 dismiss, 1 attend. Past [AlertSlide.COMMIT] it counts. */
internal object AlertSlide {
    const val COMMIT = 0.7f
    fun outcome(fraction: Float): Int = when {
        fraction >= COMMIT -> 1
        fraction <= -COMMIT -> -1
        else -> 0
    }
}

/**
 * Slide right to attend (open the chat), left to dismiss, as an incoming
 * call is answered: a slip of the hand in a pocket does neither. Let go
 * short of the end and it springs back. For TalkBack both are actions on it.
 */
@Composable
internal fun AttendSlider(onAttend: () -> Unit, onDismiss: () -> Unit) {
    val thumb = 64.dp
    var widthPx by remember { mutableFloatStateOf(0f) }
    val thumbPx = with(LocalDensity.current) { thumb.toPx() }
    val reach = ((widthPx - thumbPx) / 2f).coerceAtLeast(1f)
    val offset = remember { Animatable(0f) }
    val scope = rememberCoroutineScope()
    var done by remember { mutableStateOf(false) }
    val fraction = offset.value / reach
    Box(
        Modifier.fillMaxWidth().height(thumb + 12.dp)
            .onSizeChanged { widthPx = it.width.toFloat() }
            .clip(RoundedCornerShape(50))
            .background(CrundiColors.BgTertiary)
            .border(Dimens.Hairline, CrundiColors.Border, RoundedCornerShape(50))
            .semantics {
                contentDescription = "Slide right to attend, left to dismiss"
                customActions = listOf(
                    CustomAccessibilityAction("Attend") { onAttend(); true },
                    CustomAccessibilityAction("Dismiss") { onDismiss(); true },
                )
            },
        contentAlignment = Alignment.Center,
    ) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 22.dp), horizontalArrangement = Arrangement.SpaceBetween) {
            Text("Dismiss", style = CrundiType.Body.copy(color = CrundiColors.TextSecondary), modifier = Modifier.alpha(1f - (fraction.coerceAtLeast(0f))))
            Text("Attend", style = CrundiType.Body.copy(color = CrundiColors.TextSecondary), modifier = Modifier.alpha(1f + (fraction.coerceAtMost(0f))))
        }
        Box(
            Modifier.testTag("alert-thumb").offset { IntOffset(offset.value.roundToInt(), 0) }
                .size(thumb).clip(CircleShape)
                .background(if (fraction > 0.05f) CrundiColors.Accent else if (fraction < -0.05f) CrundiColors.Red else CrundiColors.TextPrimary)
                .draggable(
                    orientation = Orientation.Horizontal,
                    enabled = !done,
                    state = rememberDraggableState { d -> scope.launch { offset.snapTo((offset.value + d).coerceIn(-reach, reach)) } },
                    onDragStopped = {
                        // The action first, then the slider locks. Locking it
                        // (done) turns dragging off, which cancels this very
                        // block: with the action after a suspending animation,
                        // it never ran and the screen sat at the end.
                        when (AlertSlide.outcome(offset.value / reach)) {
                            1 -> { onAttend(); done = true; scope.launch { offset.animateTo(reach) } }
                            -1 -> { onDismiss(); done = true; scope.launch { offset.animateTo(-reach) } }
                            else -> scope.launch { offset.animateTo(0f, spring()) }
                        }
                    },
                ),
            contentAlignment = Alignment.Center,
        ) {
            CrundiIcon(if (fraction < -0.05f) "x" else if (fraction > 0.05f) "check" else "bell", size = 26.dp, tint = if (fraction > 0.05f || fraction < -0.05f) androidx.compose.ui.graphics.Color.White else CrundiColors.BgPrimary)
        }
    }
}
