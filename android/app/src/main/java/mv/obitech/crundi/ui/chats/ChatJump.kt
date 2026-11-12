package mv.obitech.crundi.ui.chats

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.keyframes
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.spring
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.gestures.animateScrollBy
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.interaction.DragInteraction
import androidx.compose.foundation.lazy.LazyListLayoutInfo
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.components.motionStill
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.Dimens

/**
 * Back to the newest message, as on the web (.cc-jump): a round arrow at the
 * bottom of the log, shown once the reader is a good way up it, not the
 * moment they nudge it. Something arriving below while they read gives it a
 * dot and a small nudge.
 */
object ChatJump {
    /** Shown past a third of the log's height, and never under 240dp of it. */
    fun threshold(viewportPx: Int, density: Float): Int = maxOf((240 * density).toInt(), viewportPx / 3)

    /**
     * How far, in pixels, the end of the log is below what is on screen: the
     * hidden part of the last visible row, plus the rows after it at that
     * row's height. An estimate past the screen is enough to decide.
     */
    fun distanceBelow(info: LazyListLayoutInfo): Int {
        val last = info.visibleItemsInfo.lastOrNull() ?: return 0
        val hidden = (last.offset + last.size - info.viewportEndOffset).coerceAtLeast(0)
        val after = (info.totalItemsCount - 1 - last.index).coerceAtLeast(0)
        return hidden + after * maxOf(last.size, 1)
    }

    fun far(info: LazyListLayoutInfo, density: Float): Boolean =
        info.totalItemsCount > 0 && distanceBelow(info) > threshold(info.viewportEndOffset - info.viewportStartOffset, density)

    /**
     * Where the log stands, from the same layout pass as [atEnd]: the first
     * row shown and how far it is scrolled. A row growing below leaves it be.
     */
    fun position(info: LazyListLayoutInfo): Pair<Int, Int> =
        info.visibleItemsInfo.firstOrNull()?.let { it.index to it.offset } ?: (-1 to 0)

    /** Close enough to the end that whatever came in has been seen. */
    fun atEnd(info: LazyListLayoutInfo, density: Float): Boolean = distanceBelow(info) < (60 * density).toInt()

    /**
     * How many pixels the log got shorter by, from [before] to [now] (the
     * keyboard opening, the box growing a line), or 0: taller, the same, or
     * not laid out yet.
     */
    fun shrunk(before: Int, now: Int): Int = if (before > 0 && now in 1 until before) before - now else 0
}

/**
 * Whether the log follows what comes in (new rows, a reply still being
 * written). It follows while the reader is at the bottom; moving away from it
 * themselves (a drag, a fling, a scroll by accessibility) stops it, and coming
 * back to the bottom, the arrow, or sending a message starts it again. The
 * log's own scrolls to the end are marked with [auto] so they never count as
 * the reader's, and nothing follows while a finger is on the log.
 */
class ChatFollow(following: Boolean = true) {
    var following by mutableStateOf(following)
        private set
    /** A finger is on the log: never scroll under it. */
    var dragging by mutableStateOf(false)
    private var autoDepth = 0
    /** Where the log's last own scroll left it, so hearing of that late does not count as the reader's. */
    private var autoAt: Any? = null

    /** The scroll position changed to [pos]; [atEnd] is where it now stands. */
    fun moved(pos: Any, atEnd: Boolean) {
        if (!dragging && (autoDepth > 0 || pos == autoAt)) return
        following = atEnd
    }

    /** A finger left the log: following again only if it was let go at the bottom. */
    fun released(atEnd: Boolean) { dragging = false; following = atEnd }

    /** Back to the newest on purpose: the arrow, or a message sent from here. */
    fun jumped() { following = true }

    /** Whether new content should take the log to the end now. */
    val shouldFollow: Boolean get() = following && !dragging

    /** Runs one of the log's own scrolls to the end. */
    suspend fun auto(at: () -> Any, block: suspend () -> Unit) {
        autoDepth++
        try { block() } finally { autoDepth--; autoAt = at() }
    }
}

/**
 * A [ChatFollow] kept up to date from [list]: a finger on it, and where the
 * reader has scrolled it to (used by a chat and by Cru's chat view).
 */
@Composable
fun rememberChatFollow(list: LazyListState, density: Float): ChatFollow {
    val follow = remember(list) { ChatFollow() }
    LaunchedEffect(list) {
        list.interactionSource.interactions.collect { i ->
            when (i) {
                is DragInteraction.Start -> follow.dragging = true
                is DragInteraction.Stop, is DragInteraction.Cancel -> follow.released(ChatJump.atEnd(list.layoutInfo, density))
            }
        }
    }
    LaunchedEffect(list) {
        // Only the position: a row growing below is not the reader moving.
        // Both from one layout pass, so a position never meets an older layout's idea of the end.
        var last: Any? = null
        snapshotFlow { list.layoutInfo.let { ChatJump.position(it) to ChatJump.atEnd(it, density) } }
            .collect { (pos, atEnd) -> if (pos != last) { last = pos; follow.moved(pos, atEnd) } }
    }
    return follow
}

/** To the last row, then the rest of it: a long last message would otherwise stop at its top. One of the log's own scrolls. */
suspend fun ChatFollow.toEnd(list: LazyListState, animate: Boolean) = auto({ ChatJump.position(list.layoutInfo) }) {
    val n = list.layoutInfo.totalItemsCount
    if (n == 0) return@auto
    val lastShown = list.layoutInfo.visibleItemsInfo.lastOrNull()?.index == n - 1
    if (!lastShown) { if (animate) list.animateScrollToItem(n - 1) else list.scrollToItem(n - 1) }
    // Twice at most: the first pass may have measured before the newest content was laid out.
    repeat(2) {
        val rest = ChatJump.distanceBelow(list.layoutInfo)
        if (rest <= 0) return@auto
        if (animate) list.animateScrollBy(rest.toFloat()) else list.scrollBy(rest.toFloat())
    }
}

/**
 * The keyboard opening (or anything else making the log shorter, the box
 * growing a line) keeps the bottom of the conversation in view, as chat apps
 * do. Following the newest: still at the very end. Reading up the log: the
 * row at the bottom of the screen stays at the bottom, the log moves up with
 * the keyboard. A finger on the log is left alone.
 */
@Composable
fun KeepBottomInView(list: LazyListState, follow: ChatFollow, toEnd: suspend () -> Unit) {
    val end by rememberUpdatedState(toEnd)
    LaunchedEffect(list, follow) {
        var before = 0
        snapshotFlow { list.layoutInfo.viewportSize.height }.collect { h ->
            val by = ChatJump.shrunk(before, h)
            before = h
            if (by == 0 || follow.dragging) return@collect
            if (follow.following) end()
            else follow.auto({ ChatJump.position(list.layoutInfo) }) { list.scrollBy(by.toFloat()) }
        }
    }
}

@Composable
fun JumpToNewest(visible: Boolean, newBelow: Boolean, onJump: () -> Unit, modifier: Modifier = Modifier) {
    AnimatedVisibility(
        visible, modifier,
        enter = fadeIn() + slideInVertically(spring(dampingRatio = 0.6f, stiffness = 500f)) { it / 2 } + scaleIn(initialScale = 0.85f),
        exit = fadeOut() + slideOutVertically { it / 3 } + scaleOut(targetScale = 0.85f),
    ) {
        val still = motionStill()
        // The nudge only runs while there is something new and motion is allowed.
        val nudge = if (newBelow && !still) {
            val t = rememberInfiniteTransition(label = "jump")
            t.animateFloat(0f, 0f, infiniteRepeatable(keyframes {
                durationMillis = 1600
                0f at 0; 0f at 880; 3f at 1120 using FastOutSlowInEasing; -1f at 1360; 0f at 1600
            }, RepeatMode.Restart), label = "nudge").value
        } else 0f
        Box(
            Modifier.size(40.dp).shadow(6.dp, CircleShape).clip(CircleShape).background(CrundiColors.BgSecondary)
                .border(Dimens.Hairline, if (newBelow) CrundiColors.Accent else CrundiColors.Border, CircleShape)
                .clickable(onClickLabel = "Go to the newest message", onClick = onJump)
                .semantics { contentDescription = "Go to the newest message" },
            contentAlignment = Alignment.Center,
        ) {
            CrundiIcon("arrow-down", size = 18.dp, tint = CrundiColors.TextPrimary, modifier = Modifier.offset(y = nudge.dp))
            if (newBelow) Box(Modifier.align(Alignment.TopEnd).offset(x = (-1).dp, y = 1.dp).size(10.dp).clip(CircleShape).background(CrundiColors.Accent).border(2.dp, CrundiColors.BgPrimary, CircleShape))
        }
    }
}
