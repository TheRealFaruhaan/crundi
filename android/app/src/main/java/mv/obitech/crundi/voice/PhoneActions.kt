package mv.obitech.crundi.voice

import android.content.Context
import android.util.Log
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalInspectionMode
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.compose.runtime.getValue
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.intOrNull
import mv.obitech.crundi.ui.shell.LocalAppServices

/**
 * What Cru asks of the phone itself (a turn's phoneActions; capability
 * "voice-phone-actions"): background listening for "Hey Cru" on for a while,
 * or off. Off stops every listener, not only the background service
 * ([HeyCrundi.stopAllListening]; 6 Nov: the asleep voice screen and in-app
 * listening went on after Cru's off).
 */
object PhoneActions {
    /** Do it; a note for the screen when it could not be done, else null. */
    fun apply(ctx: Context, type: String, minutes: Int): String? = when (val c = HeyCrundiPlan.command(type, minutes)) {
        is HeyCrundiPlan.Command.Start ->
            if (WakeProfiles.trained(ctx)) { HeyCrundiService.start(ctx, c.minutes); null }
            else "Hey Cru isn't trained yet: train it in Voice assistant settings first."
        HeyCrundiPlan.Command.Stop -> { HeyCrundi.stopAllListening(); null }
        null -> { Log.i("CruPhoneActions", "phone action not known here: $type"); null }
    }
}

/**
 * The same order when it was confirmed somewhere else (the PC, the web): the
 * server tells every open app (owner SSE event "voice-phone-action",
 * capability "voice-phone-action-push"), and the phone does it.
 */
@Composable
fun PhoneActionsRoute() {
    if (LocalInspectionMode.current) return
    val app = LocalAppServices.current
    val ctx = LocalContext.current.applicationContext
    val shell by app.shell.state.collectAsStateWithLifecycle()
    val has = PUSH in shell.capabilities
    LaunchedEffect(app, has) {
        if (!has) return@LaunchedEffect
        app.api.events.changes("voice-phone-action").collect { ev ->
            val type = (ev.data["type"] as? JsonPrimitive)?.content.orEmpty()
            val minutes = (ev.data["minutes"] as? JsonPrimitive)?.intOrNull ?: return@collect
            PhoneActions.apply(ctx, type, minutes)
        }
    }
}

private const val PUSH = "voice-phone-action-push"
