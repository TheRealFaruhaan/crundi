package mv.obitech.crundi.ui.chats

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch
import mv.obitech.crundi.api.model.ApprovalItem
import mv.obitech.crundi.api.model.Approvals
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.Banner
import mv.obitech.crundi.ui.components.TopBar
import mv.obitech.crundi.ui.components.errorText
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.shell.Area
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

data class ApprovalsState(
    val approvals: Approvals = Approvals(),
    /** Items whose answer is on its way. */
    val busy: Set<String> = emptySet(),
    val error: String? = null,
)

interface ApprovalsActions {
    fun back() {}
    /** A chat's card: answered in its chat, where the whole request and "always" live. */
    fun openChat(item: ApprovalItem) {}
    fun openSecrets() {}
    /** A collaborator's request. [scope]: once, session, always. */
    fun decide(item: ApprovalItem, approve: Boolean, scope: String = "once") {}
    /** Off the list: a chat card stays answerable in its chat; anything else is declined. */
    fun dismiss(item: ApprovalItem) {}
    fun clearAll() {}
}

/**
 * The top bar's shield (.appr-panel): secrets, chat cards and collaborator
 * requests in one list under the bar, at most 420dp wide and 70% of the
 * screen high. Tapping outside, or back, closes it.
 */
@Composable
fun ApprovalsDropdown(state: ApprovalsState, actions: ApprovalsActions, onDismiss: () -> Unit, modifier: Modifier = Modifier) {
    BackHandler(onBack = onDismiss)
    BoxWithConstraints(modifier.fillMaxSize().clickable(remember { MutableInteractionSource() }, null, onClick = onDismiss)) {
        val shape = RoundedCornerShape(10.dp)
        Column(
            Modifier.align(Alignment.TopEnd).padding(top = 4.dp, end = 8.dp, start = 8.dp)
                .widthIn(max = 420.dp).fillMaxWidth().heightIn(max = maxHeight * 0.7f)
                .shadow(12.dp, shape).clip(shape).background(CrundiColors.BgSecondary).border(Dimens.Hairline, CrundiColors.Border, shape)
                .clickable(remember { MutableInteractionSource() }, null) {}
                .verticalScroll(rememberScrollState()).padding(6.dp),
        ) { ApprovalsList(state, actions) }
    }
}

/** "Waiting on you" full screen: where a notification about a request lands. The same list as the dropdown. */
@Composable
fun ApprovalsScreen(state: ApprovalsState, actions: ApprovalsActions, modifier: Modifier = Modifier) {
    Column(modifier.fillMaxSize().background(CrundiColors.BgPrimary)) {
        TopBar("Waiting on you", subtitle = if (state.approvals.items.isEmpty()) null else "${state.approvals.items.size} to decide", onBack = actions::back)
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(8.dp)) { ApprovalsList(state, actions) }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ApprovalsList(state: ApprovalsState, a: ApprovalsActions) {
    val items = state.approvals.items
    if (state.error != null) Banner(state.error, Modifier.fillMaxWidth().padding(4.dp), kind = BadgeKind.Bad, icon = "alert-triangle")
    if (items.isEmpty()) {
        Text("Nothing is waiting.", style = CrundiType.Small.copy(color = CrundiColors.TextMuted), textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth().padding(14.dp))
        return
    }
    Row(Modifier.fillMaxWidth().padding(start = 8.dp, end = 4.dp, bottom = 6.dp, top = 2.dp), verticalAlignment = Alignment.CenterVertically) {
        Text("${items.size} waiting", style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontSize = 11.5.sp), modifier = Modifier.weight(1f))
        SmallAct("Clear all", CrundiColors.TextSecondary, onClick = a::clearAll)
    }
    mv.obitech.crundi.ui.components.CrundiDivider()
    items.forEachIndexed { i, it ->
        val busy = it.id in state.busy
        Column(Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 9.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp), modifier = Modifier.padding(bottom = 3.dp)) {
                SourcePill(it.source)
                Text(
                    (if (it.name.isNotEmpty()) it.name + " — " else "") + it.title,
                    style = CrundiType.Small.copy(fontSize = 13.sp, color = CrundiColors.TextPrimary), maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f),
                )
            }
            if (it.project.isNotEmpty()) Detail(it.project, faded = true)
            val detail = it.command.ifEmpty { it.detail }
            if (detail.isNotEmpty()) Detail(detail.take(1200))
            FlowRow(Modifier.fillMaxWidth().padding(top = 4.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                when {
                    it.source == "collab" && it.host.isNotEmpty() -> {
                        SmallAct("Allow once", CrundiColors.Green, busy) { a.decide(it, true, "once") }
                        SmallAct("This session", CrundiColors.Green, busy) { a.decide(it, true, "session") }
                        SmallAct("Always", CrundiColors.Green, busy) { a.decide(it, true, "always") }
                        SmallAct("Decline", CrundiColors.Red, busy) { a.decide(it, false) }
                    }
                    it.source == "collab" && it.kind == "command" -> {
                        SmallAct("Run", CrundiColors.Green, busy) { a.decide(it, true) }
                        SmallAct("Decline", CrundiColors.Red, busy) { a.decide(it, false) }
                    }
                    it.source == "collab" && it.kind == "tool" -> {
                        SmallAct("Allow once", CrundiColors.Green, busy) { a.decide(it, true, "once") }
                        SmallAct("This session", CrundiColors.Green, busy) { a.decide(it, true, "session") }
                        SmallAct("Decline", CrundiColors.Red, busy) { a.decide(it, false) }
                    }
                    it.source == "collab" -> {
                        SmallAct("Approve", CrundiColors.Green, busy) { a.decide(it, true) }
                        SmallAct("Decline", CrundiColors.Red, busy) { a.decide(it, false) }
                    }
                    it.source == "chat" -> SmallAct("Open the chat", CrundiColors.AccentHover) { a.openChat(it) }
                    else -> SmallAct("Open secrets", CrundiColors.AccentHover) { a.openSecrets() }
                }
                SmallAct("Dismiss", CrundiColors.TextMuted, busy, grow = false) { a.dismiss(it) }
            }
        }
        if (i < items.lastIndex) mv.obitech.crundi.ui.components.CrundiDivider()
    }
}

/** .appr-src: chat (accent), secret (yellow), collab (sky). */
@Composable
private fun SourcePill(source: String) {
    val c = when (source) { "secret" -> CrundiColors.Yellow; "collab" -> CrundiColors.Sky; "chat" -> CrundiColors.AccentHover; else -> CrundiColors.TextMuted }
    val b = when (source) { "secret" -> CrundiColors.Yellow; "collab" -> CrundiColors.Sky; "chat" -> CrundiColors.Accent; else -> CrundiColors.Border }
    Text(
        source.uppercase(), style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans, fontSize = 9.sp, fontWeight = FontWeight.Bold, letterSpacing = 0.6.sp, color = c),
        modifier = Modifier.clip(RoundedCornerShape(99.dp)).border(Dimens.Hairline, b, RoundedCornerShape(99.dp)).padding(horizontal = 6.dp, vertical = 1.dp),
    )
}

/** .appr-detail: the request in monospace on the darker ground. */
@Composable
private fun Detail(text: String, faded: Boolean = false) {
    Box(
        Modifier.fillMaxWidth().padding(vertical = 4.dp).heightIn(max = 140.dp).clip(Dimens.ShapeSm).background(CrundiColors.BgPrimary)
            .verticalScroll(rememberScrollState()).padding(horizontal = 7.dp, vertical = 5.dp),
    ) {
        Text(text, style = CrundiType.Code.copy(fontSize = 11.sp, color = CrundiColors.TextSecondary.copy(alpha = if (faded) 0.7f else 1f)))
    }
}

/** .appr-acts button: small, bordered, tinted by what it does. */
@Composable
private fun SmallAct(label: String, tint: Color, busy: Boolean = false, grow: Boolean = true, onClick: () -> Unit) {
    Box(
        Modifier.heightIn(min = 34.dp).widthIn(min = if (grow) 92.dp else 0.dp).clip(Dimens.ShapeSm).background(CrundiColors.BgPrimary)
            .border(Dimens.Hairline, if (tint == CrundiColors.TextMuted || tint == CrundiColors.TextSecondary) CrundiColors.Border else tint.copy(alpha = 0.55f), Dimens.ShapeSm)
            .clickable(enabled = !busy, onClick = onClick).padding(horizontal = 10.dp, vertical = 6.dp),
        contentAlignment = Alignment.Center,
    ) { Text(label, style = CrundiType.Small.copy(fontSize = 12.sp, color = if (busy) CrundiColors.TextMuted else tint), maxLines = 1) }
}

/** The live list and what its buttons do, shared by the dropdown and the full screen. */
@Composable
private fun rememberApprovals(onLeave: () -> Unit): Pair<ApprovalsState, ApprovalsActions> {
    val nav = LocalNavigator.current
    val api = LocalAppServices.current.api
    val approvals by api.events.approvals.collectAsState(Approvals())
    val latest by androidx.compose.runtime.rememberUpdatedState(approvals)
    val leave by androidx.compose.runtime.rememberUpdatedState(onLeave)
    var st by remember { mutableStateOf(ApprovalsState()) }
    val scope = rememberCoroutineScope()
    fun act(id: String?, block: suspend () -> Unit) {
        scope.launch {
            st = st.copy(error = null, busy = if (id != null) st.busy + id else st.busy)
            try { block() } catch (e: Exception) { st = st.copy(error = errorText(e)) }
            if (id != null) st = st.copy(busy = st.busy - id)
        }
    }
    val actions = remember(nav, api) {
        object : ApprovalsActions {
            override fun back() = leave()
            override fun openChat(item: ApprovalItem) { leave(); if (item.sessionId.isNotEmpty()) nav.chat(item.sessionId) }
            override fun openSecrets() { leave(); nav.area(Area.Secrets) }
            override fun decide(item: ApprovalItem, approve: Boolean, scope: String) = act(item.id) { api.approvals.decide(item.id, approve, scope) }
            override fun dismiss(item: ApprovalItem) = act(item.id) { api.approvals.dismiss(listOf(item)) }
            override fun clearAll() = act(null) { api.approvals.dismiss(latest.items) }
        }
    }
    return st.copy(approvals = approvals) to actions
}

/** The dropdown, live: what the shell shows when its shield is tapped. */
@Composable
fun ApprovalsDropdownRoute(onDismiss: () -> Unit) {
    val (st, actions) = rememberApprovals(onDismiss)
    ApprovalsDropdown(st, actions, onDismiss)
}

/** Routes.APPROVALS: the same list full screen, for notifications. */
@Composable
fun ApprovalsRoute() {
    val nav = LocalNavigator.current
    val (st, actions) = rememberApprovals { nav.back() }
    ApprovalsScreen(st, actions)
}
