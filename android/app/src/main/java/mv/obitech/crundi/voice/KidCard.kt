package mv.obitech.crundi.voice

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.platform.LocalInspectionMode
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.launch
import mv.obitech.crundi.api.KidApi
import mv.obitech.crundi.api.KidStatus
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.errorText
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType

/**
 * The parent's "Kid mode" card in the app, as the web's #kid-card: while kid
 * mode is on (on the PC), what is playing and the time used, with Pause /
 * Resume, Add 15 min and End kid mode. Follows the server's 'voice-kid'
 * event; nothing at all on a server without kid mode. [visible] false keeps
 * it listening without drawing it (a screen it would cover).
 */
@Composable
fun KidCardRoute(visible: Boolean = true) {
    if (LocalInspectionMode.current) return
    val app = LocalAppServices.current
    val shell by app.shell.state.collectAsStateWithLifecycle()
    val has = KidApi.KID in shell.capabilities
    var st by remember { mutableStateOf<KidStatus?>(null) }
    var error by remember { mutableStateOf("") }
    val scope = rememberCoroutineScope()
    LaunchedEffect(app, has) {
        if (!has) { st = null; return@LaunchedEffect }
        st = runCatching { app.api.kid.status() }.getOrNull()
        app.api.events.changes("voice-kid").collect { ev -> st = KidStatus.of(ev.data) ?: runCatching { app.api.kid.status() }.getOrNull() ?: st }
    }
    val k = st
    if (!visible || k == null || !k.on) return
    KidCard(k, error, onKid = { action ->
        error = ""
        scope.launch {
            try {
                when (action) {
                    "pause" -> app.api.kid.pause(true)
                    "resume" -> app.api.kid.pause(false)
                    "time" -> app.api.kid.addTime(15)
                    "end" -> { app.api.kid.end(); KidStatus(on = false, pinSet = true) }
                    else -> null
                }?.let { st = it }
            } catch (e: Exception) { error = errorText(e) }
        }
    })
}

/** The card itself (stateless, for screenshots). Folds to a small pill. */
@Composable
fun KidCard(k: KidStatus, error: String = "", onKid: (String) -> Unit = {}) {
    var folded by rememberSaveable { mutableStateOf(false) }
    var confirmEnd by remember { mutableStateOf(false) }
    val now by produceState(System.currentTimeMillis()) { while (true) { kotlinx.coroutines.delay(30_000); value = System.currentTimeMillis() } }
    val shape = RoundedCornerShape(12.dp)
    Box(Modifier.fillMaxWidth().padding(top = 8.dp, start = 12.dp, end = 12.dp), contentAlignment = Alignment.TopCenter) {
        if (folded) {
            Row(
                Modifier.shadow(8.dp, RoundedCornerShape(99.dp)).clip(RoundedCornerShape(99.dp)).background(CrundiColors.BgSecondary)
                    .border(1.dp, CrundiColors.Yellow.copy(alpha = 0.5f), RoundedCornerShape(99.dp))
                    .clickable(onClickLabel = "Show the kid mode card") { folded = false }.padding(horizontal = 12.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Box(Modifier.size(7.dp).clip(CircleShape).background(if (k.paused) CrundiColors.TextMuted else CrundiColors.Yellow))
                Text("Kid mode · " + kidLine(k, now), style = CrundiType.Small.copy(color = CrundiColors.TextPrimary))
            }
            return@Box
        }
        Column(
            Modifier.widthIn(max = 460.dp).fillMaxWidth().shadow(12.dp, shape).clip(shape).background(CrundiColors.BgSecondary)
                .border(1.dp, CrundiColors.Border, shape).padding(start = 14.dp, end = 6.dp, top = 6.dp, bottom = 12.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                CrundiIcon("users", size = 15.dp, tint = CrundiColors.Yellow)
                Column(Modifier.weight(1f).padding(start = 8.dp)) {
                    Text("Kid mode is on", style = CrundiType.Body.copy(fontWeight = FontWeight.SemiBold, color = CrundiColors.TextPrimary))
                    Text(kidLine(k, now), style = CrundiType.Small)
                }
                CrundiIconButton("chevron-up", { folded = true }, contentDescription = "Fold the kid mode card", size = 15.dp, touch = 36.dp)
            }
            if (k.asks.isNotEmpty()) Text("Asking you: " + k.asks.first().title, style = CrundiType.Small.copy(color = CrundiColors.Ask))
            Box(Modifier.padding(end = 8.dp)) { KidButtons(k, confirmEnd, onConfirm = { confirmEnd = it }, onKid = onKid) }
            if (error.isNotEmpty()) Text(error, style = CrundiType.Small.copy(color = CrundiColors.Red))
        }
    }
}
