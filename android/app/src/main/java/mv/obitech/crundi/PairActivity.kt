package mv.obitech.crundi

import android.content.Intent
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.pair.PairScreen
import mv.obitech.crundi.ui.pair.PairUi
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiTheme
import java.util.UUID

/**
 * Pairing this phone with a server.
 *
 * The server shows a picture (Settings, Connect a phone); this scans it, or
 * takes the same link pasted in. The link carries a one-time code, which is
 * swapped for a login. No password is typed on the phone and the phone never
 * holds one.
 */
class PairActivity : ComponentActivity() {
    private var ui by mutableStateOf(PairUi())
    /** A plain-http link waiting for "Connect anyway". */
    private var insecure by mutableStateOf<PairingLink?>(null)
    private val busy get() = ui.busy

    private val scan = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { r ->
        if (r.resultCode == RESULT_OK) r.data?.getStringExtra(ScanActivity.EXTRA_LINK)?.let { ui = ui.copy(link = it); submit(it) }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        // Pairing AGAIN: the server ended this phone's sign-in, and the app
        // has no other way back in (the server refuses a password from it).
        val again = intent.getStringExtra(EXTRA_AGAIN)?.let { Servers.byId(this, it) }
        ui = PairUi(againHost = again?.host)
        setContent {
            CrundiTheme {
                Box(Modifier.fillMaxSize().background(CrundiColors.BgPrimary).safeDrawingPadding()) {
                    PairScreen(
                        ui,
                        onScan = { scan.launch(Intent(this@PairActivity, ScanActivity::class.java)) },
                        onLinkChange = { ui = ui.copy(link = it) },
                        onConnect = { submit(ui.link) },
                    )
                    insecure?.let { link ->
                        // Plain http: everything, the login included, would cross the
                        // network readable. Fine on a home network; never without being asked.
                        CrundiDialog(
                            title = "This connection is not private",
                            message = "${link.baseUrl} does not use https. Anyone on the same network could read what is sent, including your login.\n\nOnly continue if this is your own server on a network you trust.",
                            confirm = "Connect anyway", danger = true,
                            onConfirm = { insecure = null; pair(link) },
                            onDismiss = { insecure = null },
                        )
                    }
                }
            }
        }
    }

    private fun say(text: String) { ui = ui.copy(status = text) }

    private fun submit(text: String) {
        if (busy) return
        val link = PairingLink.parse(text.trim())
        if (link == null) { say("That is not a Crundi pairing link. It looks like https://your-server/pair#… and comes from the server's Connect a phone screen."); return }
        if (!link.insecure) { pair(link); return }
        insecure = link
    }

    private fun pair(link: PairingLink) {
        ui = ui.copy(busy = true, status = "Connecting to ${link.baseUrl.substringAfter("://")}…")
        val device = listOf(Build.MANUFACTURER, Build.MODEL).filter { !it.isNullOrBlank() }.joinToString(" ").replaceFirstChar { it.uppercase() }
        Bg.run({
            // Ask first whether we can work together at all. A code is spent
            // by trying it, and there is no sense spending one on a server
            // this app would refuse to use.
            val hs = Api.handshake(link.baseUrl)
            val verdict = hs.json?.let { Gate.judge(it, BuildConfig.VERSION_NAME, BuildConfig.MIN_SERVER) }
            if (verdict == null || !verdict.ok) Pair(verdict, null) else Pair(verdict, Api.claim(link, device))
        }) { result ->
            ui = ui.copy(busy = false)
            val (verdict, reply) = result.getOrElse { say("Could not reach ${link.baseUrl.substringAfter("://")}. Check the address and that this phone can reach the server."); return@run }
            when {
                verdict == null -> say("That address did not answer as a Crundi server. It may be running a version from before phone pairing existed.")
                verdict.serverTooOld && verdict.appTooOld -> say("This app and that server are each too old for the other. Update the server to ${BuildConfig.MIN_SERVER} or newer, and this app from ${Updater.WHERE}.")
                verdict.serverTooOld -> say("That server is version ${verdict.serverVersion.ifEmpty { "unknown" }}. This app needs ${BuildConfig.MIN_SERVER} or newer. Update the server, then pair again.")
                verdict.appTooOld -> say("That server needs a newer version of this app${if (verdict.minApp.isNotEmpty()) " (${verdict.minApp} or newer)" else ""}. Update it from ${Updater.WHERE}.")
                reply == null || !reply.ok -> say(reply?.error ?: "The server did not accept that code. It may have expired; show a new one and try again.")
                else -> finishPairing(link, device, reply)
            }
        }
    }

    private fun finishPairing(link: PairingLink, device: String, reply: Reply) {
        val j = reply.json!!
        val sessions = j.optJSONObject("sessions")
        val page = sessions?.optJSONObject("page")
        val bg = sessions?.optJSONObject("background")
        if (page == null || bg == null) { say("The server's answer was not what this app expects. Nothing was saved."); return }
        val server = Server(
            id = j.optJSONObject("device")?.optString("id")?.takeIf { it.isNotEmpty() } ?: UUID.randomUUID().toString(),
            baseUrl = link.baseUrl,
            username = j.optString("username"),
            deviceName = j.optJSONObject("device")?.optString("name") ?: device,
            bgToken = bg.optString("token"), bgRefresh = bg.optString("refreshToken"),
            pageToken = page.optString("token"), pageRefresh = page.optString("refreshToken"),
        )
        // A phone paired again is a new device to the server, with a new id.
        // What it replaces is the old pairing with the same server and the
        // same person, whose logins are dead: left in the list it would be a
        // second entry for one server that answers nothing.
        for (old in Servers.all(this)) {
            if (old.id == server.id || old.baseUrl != server.baseUrl || old.username != server.username) continue
            LiveCards.forget(applicationContext, old.id)
            Icons.forget(old.id)
            PageLogin.forget(applicationContext, old.id)
            AppGraph.forget(old.id)
            Servers.remove(this, old.id)
        }
        Servers.save(this, server)
        Servers.setCurrent(this, server.id)
        startActivity(Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_NEW_TASK))
        finish()
    }

    companion object {
        /** The id of a server whose pairing has ended and is being made again. */
        const val EXTRA_AGAIN = "again"
    }
}
