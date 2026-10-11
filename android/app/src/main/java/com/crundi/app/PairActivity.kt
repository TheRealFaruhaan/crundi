package com.crundi.app

import android.app.Activity
import android.app.AlertDialog
import android.content.Intent
import android.graphics.drawable.GradientDrawable
import android.os.Build
import android.os.Bundle
import android.text.InputType
import android.util.TypedValue
import android.view.Gravity
import android.view.inputmethod.EditorInfo
import android.widget.EditText
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import java.util.UUID

/**
 * Pairing this phone with a server.
 *
 * The server shows a picture (Settings, Connect a phone); this scans it, or
 * takes the same link pasted in. The link carries a one-time code, which is
 * swapped for a login. No password is typed on the phone and the phone never
 * holds one.
 */
class PairActivity : Activity() {
    private lateinit var status: TextView
    private lateinit var input: EditText
    private var busy = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val col = Ui.column(this).apply { gravity = Gravity.CENTER }
        col.addView(ImageView(this).apply { setImageResource(R.drawable.ic_logo) }, LinearLayout.LayoutParams(Ui.dp(this, 84), Ui.dp(this, 84)))
        col.addView(Ui.title(this, "Connect to your Crundi server"), Ui.wide(this, 18))
        col.addView(Ui.body(this, "On the server, open Settings, then Connect a phone, and point this phone at the picture it shows."), Ui.wide(this, 10))
        col.addView(Ui.button(this, "Scan the picture", true) { startActivityForResult(Intent(this, ScanActivity::class.java), REQ_SCAN) }, Ui.wide(this, 26))

        col.addView(Ui.body(this, "Or paste a pairing link"), Ui.wide(this, 28))
        input = EditText(this).apply {
            hint = "https://your-server/pair#…"
            setHintTextColor(getColor(R.color.text_dim))
            setTextColor(getColor(R.color.text))
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI or InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
            imeOptions = EditorInfo.IME_ACTION_GO
            setSingleLine(true)
            val p = Ui.dp(this@PairActivity, 12)
            setPadding(p, p, p, p)
            background = GradientDrawable().apply { cornerRadius = Ui.dp(this@PairActivity, 10).toFloat(); setColor(getColor(R.color.surface)); setStroke(Ui.dp(this@PairActivity, 1), getColor(R.color.border)) }
            setOnEditorActionListener { _, id, _ -> if (id == EditorInfo.IME_ACTION_GO) { submit(text.toString()); true } else false }
        }
        col.addView(input, Ui.wide(this, 10))
        col.addView(Ui.button(this, "Connect", false) { submit(input.text.toString()) }, Ui.wide(this, 10))

        status = Ui.body(this, "").apply { setTextColor(getColor(R.color.amber)) }
        col.addView(status, Ui.wide(this, 18))
        setContentView(ScrollView(this).apply { isFillViewport = true; setBackgroundColor(getColor(R.color.bg)); addView(col) })
    }

    @Deprecated("The platform's own result call; enough for one request.")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == REQ_SCAN && resultCode == RESULT_OK) submit(data?.getStringExtra(ScanActivity.EXTRA_LINK) ?: "")
    }

    private fun say(text: String) { status.text = text }

    private fun submit(text: String) {
        if (busy) return
        val link = PairingLink.parse(text)
        if (link == null) { say("That is not a Crundi pairing link. It looks like https://your-server/pair#… and comes from the server's Connect a phone screen."); return }
        if (!link.insecure) { pair(link); return }
        // Plain http: everything, the login included, would cross the network
        // readable. Fine on a home network; never without being asked.
        AlertDialog.Builder(this)
            .setTitle("This connection is not private")
            .setMessage("${link.baseUrl} does not use https. Anyone on the same network could read what is sent, including your login.\n\nOnly continue if this is your own server on a network you trust.")
            .setNegativeButton("Cancel", null)
            .setPositiveButton("Connect anyway") { _, _ -> pair(link) }
            .show()
    }

    private fun pair(link: PairingLink) {
        busy = true
        say("Connecting to ${link.baseUrl.substringAfter("://")}…")
        val device = listOf(Build.MANUFACTURER, Build.MODEL).filter { !it.isNullOrBlank() }.joinToString(" ").replaceFirstChar { it.uppercase() }
        Bg.run({
            // Ask first whether we can work together at all. A code is spent
            // by trying it, and there is no sense spending one on a server
            // this app would refuse to use.
            val hs = Api.handshake(link.baseUrl)
            val verdict = hs.json?.let { Gate.judge(it, BuildConfig.VERSION_NAME, BuildConfig.MIN_SERVER) }
            if (verdict == null || !verdict.ok) Pair(verdict, null) else Pair(verdict, Api.claim(link, device))
        }) { result ->
            busy = false
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
        Servers.save(this, server)
        Servers.setCurrent(this, server.id)
        startActivity(Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_NEW_TASK))
        finish()
    }

    companion object { private const val REQ_SCAN = 1 }
}
