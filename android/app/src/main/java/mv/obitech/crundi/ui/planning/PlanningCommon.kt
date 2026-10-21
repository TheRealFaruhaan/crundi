package mv.obitech.crundi.ui.planning

import mv.obitech.crundi.ui.components.errorText

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.launch
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.ServerEvent
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.CrundiDivider
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.shell.Area
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

/** What a planning screen has to show: still loading, failed (words fit to show), or the data. */
sealed interface Load<out T> {
    data object Loading : Load<Nothing>
    data class Failed(val message: String) : Load<Nothing>
    data class Ready<T>(val value: T) : Load<T>

    val valueOrNull: T? get() = (this as? Ready<T>)?.value
}

/**
 * Data from the server for one screen, fetched once and again on each live
 * "fetch again" signal ([events]) that [accept] lets through, as the desktop
 * does. A refetch keeps showing the old data until the new arrives.
 */
class Loader<T>(private val scope: CoroutineScope, private val fetch: suspend () -> T) {
    var load: Load<T> by mutableStateOf(Load.Loading)
        private set

    /** Fetch again now. Failures only replace the screen when there is nothing yet to show. */
    fun refresh() {
        scope.launch {
            try { load = Load.Ready(fetch()) } catch (e: ApiException) {
                if (load !is Load.Ready) load = Load.Failed(errorText(e))
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                if (load !is Load.Ready) load = Load.Failed(errorText(e))
            }
        }
    }

    /** Change what is shown at once (an optimistic edit); the next refetch puts the server's word back. */
    fun update(f: (T) -> T) { (load as? Load.Ready<T>)?.let { load = Load.Ready(f(it.value)) } }
}

@Composable
fun <T> rememberLoader(
    vararg keys: Any?,
    events: Array<String> = emptyArray(),
    accept: (ServerEvent) -> Boolean = { true },
    fetch: suspend () -> T,
): Loader<T> {
    val scope = rememberCoroutineScope()
    val api = LocalAppServices.current.api
    val currentFetch by rememberUpdatedState(fetch)
    val currentAccept by rememberUpdatedState(accept)
    val loader = remember(*keys) { Loader(scope) { currentFetch() } }
    LaunchedEffect(loader) { loader.refresh() }
    if (events.isNotEmpty()) {
        LaunchedEffect(loader, events.joinToString()) {
            api.events.changes(*events).collect { if (currentAccept(it)) loader.refresh() }
        }
    }
    return loader
}

/**
 * Runs the screen's actions against the server and keeps the last failure
 * to show ([message]), so no action fails silently.
 */
class Actions(private val scope: CoroutineScope, private val after: () -> Unit) {
    var message: String? by mutableStateOf(null)
    var busy: Boolean by mutableStateOf(false)
        private set

    fun run(refresh: Boolean = true, block: suspend () -> Unit) {
        scope.launch {
            busy = true
            try { block(); message = null } catch (e: ApiException) { message = errorText(e) } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                message = errorText(e)
            } finally {
                busy = false
                if (refresh) after()
            }
        }
    }
}

@Composable
fun rememberActions(after: () -> Unit = {}): Actions {
    val scope = rememberCoroutineScope()
    val a by rememberUpdatedState(after)
    return remember { Actions(scope) { a() } }
}

/** The current project's alias, live (null when none is picked). */
@Composable
fun currentProject(): String? {
    val shell = LocalAppServices.current.shell
    val data by shell.state.collectAsStateWithLifecycle()
    return data.currentProject
}

/** An area that shows one project's things: asks for a project when none is picked. */
@Composable
fun WithProject(area: Area, content: @Composable (String) -> Unit) {
    val p = currentProject()
    if (p == null) EmptyState(area.icon, "Pick a project", body = "${area.label} belongs to a project. Choose one from the menu at the top left.")
    else content(p)
}

private val ZONE: ZoneId get() = ZoneId.systemDefault()

fun parseInstant(iso: String?): Instant? = iso?.takeIf { it.isNotBlank() }?.let { runCatching { Instant.parse(it) }.getOrNull() }

/**
 * A time the way the desktop words it in lists: "just now", "5m ago",
 * "3h ago", "yesterday", then the date. [now] is for tests.
 */
fun ago(ms: Long, now: Long = System.currentTimeMillis()): String {
    if (ms <= 0) return ""
    val d = (now - ms) / 1000
    return when {
        d < 45 -> "just now"
        d < 3600 -> "${(d + 30) / 60}m ago"
        d < 86_400 -> "${d / 3600}h ago"
        d < 2 * 86_400 -> "yesterday"
        d < 7 * 86_400 -> "${d / 86_400}d ago"
        else -> DateTimeFormatter.ofPattern("d MMM yyyy", Locale.ENGLISH).withZone(ZONE).format(Instant.ofEpochMilli(ms))
    }
}

fun ago(iso: String?, now: Long = System.currentTimeMillis()): String = parseInstant(iso)?.let { ago(it.toEpochMilli(), now) } ?: ""

/** "21 Oct, 08:30" in the phone's zone. */
fun shortDateTime(ms: Long): String = if (ms <= 0) "" else DateTimeFormatter.ofPattern("d MMM, HH:mm", Locale.ENGLISH).withZone(ZONE).format(Instant.ofEpochMilli(ms))

/** A failure from the last action, over the bottom of the screen; tap the cross to close it. */
@Composable
fun BoxScope.ActionError(actions: Actions) {
    val m = actions.message ?: return
    Row(
        Modifier
            .align(Alignment.BottomCenter)
            .navigationBarsPadding()
            .padding(12.dp)
            .fillMaxWidth()
            .background(CrundiColors.BgSecondary, Dimens.Shape)
            .border(Dimens.Hairline, CrundiColors.Red.copy(alpha = 0.5f), Dimens.Shape)
            .padding(start = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        CrundiIcon("alert-triangle", size = Dimens.IconSm, tint = CrundiColors.Red)
        Text(m, style = CrundiType.Small.copy(color = CrundiColors.TextPrimary), modifier = Modifier.weight(1f).padding(vertical = 10.dp))
        CrundiIconButton("x", { actions.message = null }, contentDescription = "Close", size = 14.dp)
    }
}

/**
 * A sheet that rises from the bottom (a phone's stand-in for the desktop's
 * side panels and popovers): --bg-secondary, top corners rounded, a title
 * row with a close cross, and scrolling content.
 */
@Composable
fun PlanningSheet(title: String, onDismiss: () -> Unit, subtitle: String? = null, actions: @Composable () -> Unit = {}, content: @Composable ColumnScope.() -> Unit) {
    Dialog(onDismissRequest = onDismiss, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Box(Modifier.fillMaxWidth().clickable(remember { androidx.compose.foundation.interaction.MutableInteractionSource() }, null) { onDismiss() }) {
            SheetCard(title, onDismiss, Modifier.align(Alignment.BottomCenter), subtitle, actions, content)
        }
    }
}

/** The sheet's body without the window: used by [PlanningSheet] and by screenshot tests. */
@Composable
fun SheetCard(title: String, onDismiss: () -> Unit, modifier: Modifier = Modifier, subtitle: String? = null, actions: @Composable () -> Unit = {}, content: @Composable ColumnScope.() -> Unit) {
    val shape = RoundedCornerShape(topStart = Dimens.Radius, topEnd = Dimens.Radius)
    Column(
        modifier
            .fillMaxWidth()
            .background(CrundiColors.BgSecondary, shape)
            .border(Dimens.Hairline, CrundiColors.Border, shape)
            .clickable(remember { androidx.compose.foundation.interaction.MutableInteractionSource() }, null) {}
            .navigationBarsPadding(),
    ) {
        Box(Modifier.fillMaxWidth().padding(top = 8.dp), contentAlignment = Alignment.Center) {
            Box(Modifier.size(width = 36.dp, height = 4.dp).background(CrundiColors.Border, RoundedCornerShape(2.dp)))
        }
        Row(Modifier.fillMaxWidth().padding(start = Dimens.ScreenPad, end = 4.dp, top = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(title, style = CrundiType.Heading, maxLines = 2, overflow = TextOverflow.Ellipsis)
                if (!subtitle.isNullOrEmpty()) Text(subtitle, style = CrundiType.Meta, maxLines = 1)
            }
            actions()
            CrundiIconButton("x", onDismiss, contentDescription = "Close")
        }
        CrundiDivider()
        Column(
            Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(Dimens.ScreenPad),
            verticalArrangement = Arrangement.spacedBy(12.dp),
            content = content,
        )
    }
}

/** A row in a sheet's menu: icon and words, red for the destructive ones. */
@Composable
fun MenuRow(text: String, icon: String, onClick: () -> Unit, danger: Boolean = false, enabled: Boolean = true) {
    val c = when { !enabled -> CrundiColors.TextMuted; danger -> CrundiColors.Red; else -> CrundiColors.TextPrimary }
    Row(
        Modifier
            .fillMaxWidth()
            .clip(Dimens.ShapeSm)
            .clickable(enabled = enabled, onClick = onClick)
            .padding(horizontal = 4.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        CrundiIcon(icon, size = Dimens.Icon, tint = if (danger && enabled) CrundiColors.Red else CrundiColors.TextSecondary)
        Text(text, style = CrundiType.Body.copy(color = c))
    }
}


/** A small label above a form field, as the desktop's form labels. */
@Composable
fun FieldLabel(text: String, modifier: Modifier = Modifier, hint: String? = null) {
    Column(modifier) {
        Text(text, style = CrundiType.Small.copy(color = CrundiColors.TextSecondary, fontWeight = androidx.compose.ui.text.font.FontWeight.Medium))
        if (hint != null) Text(hint, style = CrundiType.Meta)
    }
}

/** A kanban status, as the desktop colours it. */
fun statusKind(status: String): BadgeKind = when (status) {
    "backlog" -> BadgeKind.Neutral
    "todo" -> BadgeKind.Info
    "in_progress" -> BadgeKind.Warn
    "done" -> BadgeKind.Good
    else -> BadgeKind.Neutral
}

/** For a flow that may not exist in tests. */
internal fun <T> Flow<T>?.orEmpty(): Flow<T> = this ?: emptyFlow()

internal fun <T> MutableState<T>.set(v: T) { value = v }
