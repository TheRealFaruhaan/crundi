package mv.obitech.crundi.ui.chats

import mv.obitech.crundi.ui.components.errorText

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardOptions
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
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import mv.obitech.crundi.api.model.ApprovalItem
import mv.obitech.crundi.api.model.Approvals
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.Banner
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiCard
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.DialogCard
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.components.StatusBadge
import mv.obitech.crundi.ui.components.TopBar
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

data class ApprovalsState(
    val approvals: Approvals = Approvals(),
    val busy: Set<String> = emptySet(),
    /** A secret request waiting for its PIN. */
    val pinFor: ApprovalItem? = null,
    val pin: String = "",
    val error: String? = null,
)

interface ApprovalsActions {
    fun back() {}
    fun openChat(item: ApprovalItem) {}
    fun allow(item: ApprovalItem) {}
    fun deny(item: ApprovalItem) {}
    fun askPin(item: ApprovalItem?) {}
    fun pin(value: String) {}
    fun approveSecret() {}
    fun clearAll() {}
}

private fun ApprovalItem.icon() = when (source) { "secret" -> "key"; "collab" -> "network"; else -> if (isQuestion) "message-square" else "shield-alert" }
private fun ApprovalItem.kindLabel() = when (source) { "secret" -> if (kind == "run") "secret · run a command" else "secret"; "collab" -> "collaborator" + (if (name.isNotEmpty()) " $name" else ""); else -> if (isQuestion) "question" else "permission" }

/** "Waiting on you": everything that needs the person, from every chat and project. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun ApprovalsScreen(state: ApprovalsState, actions: ApprovalsActions, modifier: Modifier = Modifier) {
    Box(modifier.fillMaxSize().background(CrundiColors.BgPrimary)) {
        Column(Modifier.fillMaxSize()) {
            TopBar("Waiting on you", subtitle = if (state.approvals.items.isEmpty()) null else "${state.approvals.items.size} to decide", onBack = actions::back, actions = {
                if (state.approvals.items.isNotEmpty()) CrundiButton("Dismiss all", actions::clearAll, kind = ButtonKind.Ghost)
            })
            if (state.error != null) Banner(state.error, Modifier.fillMaxWidth().padding(Dimens.ScreenPad), kind = BadgeKind.Bad, icon = "alert-triangle")
            if (state.approvals.items.isEmpty()) {
                EmptyState("check", "Nothing is waiting on you", body = "Permission requests, questions and secret requests from every project show here.")
                return@Column
            }
            LazyColumn(contentPadding = PaddingValues(Dimens.ScreenPad), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                items(state.approvals.items, key = { it.source + it.id }) { item ->
                    val busy = item.id in state.busy
                    CrundiCard(Modifier.fillMaxWidth()) {
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            CrundiIcon(item.icon(), size = Dimens.Icon, tint = if (item.source == "secret") CrundiColors.Amber else CrundiColors.Yellow)
                            Text(item.kindLabel(), style = CrundiType.Meta, modifier = Modifier.weight(1f))
                            if (item.project.isNotEmpty()) StatusBadge(item.project, BadgeKind.Neutral)
                        }
                        Text(item.title, style = CrundiType.Body.copy(fontWeight = androidx.compose.ui.text.font.FontWeight.SemiBold), modifier = Modifier.padding(top = 8.dp))
                        val detail = item.command.ifEmpty { item.detail }
                        if (detail.isNotEmpty()) Text(
                            detail, style = ChatType.ToolBody.copy(color = CrundiColors.TextSecondary), maxLines = 4, overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.padding(top = 6.dp).fillMaxWidth().background(CrundiColors.BgPrimary, Dimens.ShapeSm).padding(horizontal = 9.dp, vertical = 7.dp),
                        )
                        if (item.source == "secret" && item.command.isNotEmpty() && item.detail.isNotEmpty()) Text(item.detail, style = CrundiType.Small, modifier = Modifier.padding(top = 4.dp))
                        Text(ago(item.createdAtMs), style = CrundiType.Meta, modifier = Modifier.padding(top = 6.dp))
                        FlowRow(Modifier.padding(top = 10.dp), horizontalArrangement = Arrangement.spacedBy(7.dp), verticalArrangement = Arrangement.spacedBy(7.dp)) {
                            when (item.source) {
                                "secret" -> {
                                    CrundiButton("Approve with PIN", { actions.askPin(item) }, kind = ButtonKind.Primary, icon = "lock", enabled = !busy)
                                    CrundiButton("Deny", { actions.deny(item) }, kind = ButtonKind.Danger, enabled = !busy)
                                }
                                "chat" -> if (item.isQuestion) CrundiButton("Answer in chat", { actions.openChat(item) }, kind = ButtonKind.Primary, icon = "message-square")
                                else {
                                    CrundiButton("Allow", { actions.allow(item) }, kind = ButtonKind.Primary, enabled = !busy)
                                    CrundiButton("Deny", { actions.deny(item) }, kind = ButtonKind.Danger, enabled = !busy)
                                    CrundiButton("Open chat", { actions.openChat(item) }, kind = ButtonKind.Ghost)
                                }
                                else -> {
                                    CrundiButton("Approve", { actions.allow(item) }, kind = ButtonKind.Primary, enabled = !busy)
                                    CrundiButton("Deny", { actions.deny(item) }, kind = ButtonKind.Danger, enabled = !busy)
                                    if (item.sessionId.isNotEmpty()) CrundiButton("Open chat", { actions.openChat(item) }, kind = ButtonKind.Ghost)
                                }
                            }
                        }
                    }
                }
            }
        }
        state.pinFor?.let { item ->
            SheetOverlay({ actions.askPin(null) }) {
                DialogCard(
                    "Approve with PIN", message = "Claude gets ${item.title.substringAfter('"', "").substringBefore('"').ifEmpty { "the secret" }}. The value goes to Claude, never to this phone.",
                    confirm = "Approve", onConfirm = actions::approveSecret, onDismiss = { actions.askPin(null) },
                ) {
                    CrundiTextField(
                        state.pin, { actions.pin(it.filter(Char::isDigit).take(6)) }, placeholder = "6-digit PIN", mono = true, password = true,
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword), modifier = Modifier.fillMaxWidth(),
                    )
                }
            }
        }
    }
}

@Composable
fun ApprovalsRoute() {
    val nav = LocalNavigator.current
    val api = LocalAppServices.current.api
    val approvals by api.events.approvals.collectAsState(Approvals())
    var st by remember { mutableStateOf(ApprovalsState()) }
    val scope = rememberCoroutineScope()
    fun act(id: String?, block: suspend () -> Unit) = scope.launch {
        st = st.copy(error = null, busy = if (id != null) st.busy + id else st.busy)
        try { block() } catch (e: Exception) { st = st.copy(error = errorText(e)) }
        if (id != null) st = st.copy(busy = st.busy - id)
    }
    ApprovalsScreen(st.copy(approvals = approvals), object : ApprovalsActions {
        override fun back() { nav.back() }
        override fun openChat(item: ApprovalItem) { if (item.sessionId.isNotEmpty()) nav.chat(item.sessionId) }
        override fun allow(item: ApprovalItem) = when (item.source) {
            "chat" -> { act(item.id) { api.chats.allow(item.sessionId, item.id) }; Unit }
            else -> { act(item.id) { api.approvals.decide(item.id, true, "once") }; Unit }
        }
        override fun deny(item: ApprovalItem) = when (item.source) {
            "chat" -> { act(item.id) { api.chats.deny(item.sessionId, item.id) }; Unit }
            "secret" -> { act(item.id) { api.secrets.deny(item.id) }; Unit }
            else -> { act(item.id) { api.approvals.decide(item.id, false) }; Unit }
        }
        override fun askPin(item: ApprovalItem?) { st = st.copy(pinFor = item, pin = "") }
        override fun pin(value: String) { st = st.copy(pin = value) }
        override fun approveSecret() {
            val item = st.pinFor ?: return
            if (st.pin.length != 6) { st = st.copy(error = "The PIN is 6 digits."); return }
            val pin = st.pin
            st = st.copy(pinFor = null, pin = "")
            act(item.id) { api.secrets.approve(item.id, pin) }
        }
        override fun clearAll() { act(null) { api.approvals.dismiss(approvals.items) } }
    })
}
