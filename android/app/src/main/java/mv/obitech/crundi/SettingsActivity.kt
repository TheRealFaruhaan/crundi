package mv.obitech.crundi

import android.Manifest
import android.app.Activity
import android.app.AlertDialog
import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.util.TypedValue
import android.view.Gravity
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import androidx.core.app.NotificationManagerCompat

/**
 * The app's own settings: what the phone lets it do, and which servers it is
 * paired with. Everything about a server itself lives in that server's
 * Settings page; this is only for what belongs to the app on this phone.
 *
 * It is redrawn every time it comes back to the front, because most of what
 * it shows is changed somewhere else: in Android's own settings, which its
 * buttons open.
 */
class SettingsActivity : Activity() {
    private lateinit var list: LinearLayout

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        list = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            val p = Ui.dp(this@SettingsActivity, 18)
            setPadding(p, p, p, p)
        }
        setContentView(ScrollView(this).apply { setBackgroundColor(getColor(R.color.bg)); fitsSystemWindows = true; addView(list) })
    }

    override fun onResume() {
        super.onResume()
        draw()
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        draw()
    }

    private fun heading(text: String) = TextView(this).apply {
        this.text = text.uppercase()
        setTextColor(getColor(R.color.text_dim))
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 12f)
        typeface = Typeface.DEFAULT_BOLD
        letterSpacing = 0.08f
        setPadding(Ui.dp(this@SettingsActivity, 4), Ui.dp(this@SettingsActivity, 22), 0, Ui.dp(this@SettingsActivity, 8))
    }

    /** One setting: what it is, how it stands, and at most one thing to do about it. */
    private fun row(title: String, state: String, good: Boolean?, action: String?, onAction: (() -> Unit)?) {
        val card = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            val p = Ui.dp(this@SettingsActivity, 14)
            setPadding(p, p, p, p)
            background = GradientDrawable().apply { cornerRadius = Ui.dp(this@SettingsActivity, 12).toFloat(); setColor(getColor(R.color.surface)); setStroke(Ui.dp(this@SettingsActivity, 1), getColor(R.color.border)) }
        }
        val text = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        text.addView(TextView(this).apply { this.text = title; setTextColor(getColor(R.color.text)); setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f) })
        text.addView(TextView(this).apply {
            this.text = state
            setTextColor(getColor(when (good) { true -> R.color.green; false -> R.color.amber; null -> R.color.text_dim }))
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
            setLineSpacing(0f, 1.2f)
            setPadding(0, Ui.dp(this@SettingsActivity, 3), 0, 0)
        })
        card.addView(text, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        if (action != null && onAction != null) {
            card.addView(Ui.button(this, action, false, onAction).apply { minimumWidth = 0; minWidth = 0; setPadding(Ui.dp(this@SettingsActivity, 14), 0, Ui.dp(this@SettingsActivity, 14), 0) },
                LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, Ui.dp(this, 40)).apply { marginStart = Ui.dp(this@SettingsActivity, 10) })
        }
        list.addView(card, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { bottomMargin = Ui.dp(this@SettingsActivity, 8) })
    }

    private fun has(permission: String) = checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED

    private fun appDetails() = startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:$packageName")))
    private fun liveSettings() {
        runCatching { startActivity(Intent("android.settings.MANAGE_APP_PROMOTED_NOTIFICATIONS").putExtra(Settings.EXTRA_APP_PACKAGE, packageName)) }.onFailure { notificationSettings() }
    }
    private fun notificationSettings() = startActivity(Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE, packageName))

    /** Ask for a permission if Android will still show the question; otherwise send the person to where it can be changed. */
    private fun ask(permission: String, code: Int) {
        val prefs = getSharedPreferences("crundi", Context.MODE_PRIVATE)
        val askedBefore = prefs.getBoolean("asked.$permission", false)
        // After a refusal Android may stop showing the question at all. It
        // says so only through this call, and only once it has been asked.
        if (askedBefore && !shouldShowRequestPermissionRationale(permission)) { appDetails(); return }
        prefs.edit().putBoolean("asked.$permission", true).apply()
        requestPermissions(arrayOf(permission), code)
    }

    private fun draw() {
        list.removeAllViews()
        list.addView(Ui.title(this, "App settings").apply { gravity = Gravity.START })
        list.addView(TextView(this).apply {
            text = "Crundi ${BuildConfig.VERSION_NAME} (${BuildConfig.STORE})  ·  Android ${Build.VERSION.RELEASE} (${Build.MANUFACTURER} ${Build.MODEL})"
            setTextColor(getColor(R.color.text_dim)); setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
            setPadding(Ui.dp(this@SettingsActivity, 2), Ui.dp(this@SettingsActivity, 4), 0, 0)
        })

        // ─── What the phone lets the app do ───
        list.addView(heading("Permissions"))
        val notify = NotificationManagerCompat.from(this).areNotificationsEnabled()
        if (!Push.available) row("Notifications", "This build was made without notifications.", null, null, null)
        else if (notify) row("Notifications", "Allowed", true, "Manage") { notificationSettings() }
        else row("Notifications", "Blocked. Claude finishing, or needing you, will not reach this phone.", false, "Allow") {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) ask(Manifest.permission.POST_NOTIFICATIONS, 1) else notificationSettings()
        }

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            val pref = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) nm.bubblePreference else NotificationManager.BUBBLE_PREFERENCE_ALL
            val (state, good) = when (pref) {
                NotificationManager.BUBBLE_PREFERENCE_ALL -> "Any chat can float as a bubble" to true
                NotificationManager.BUBBLE_PREFERENCE_SELECTED -> "Only chats you choose can float as a bubble" to true
                else -> "Off. Chats cannot float as bubbles." to false
            }
            row("Bubbles", state + if (Build.MANUFACTURER.equals("samsung", true)) "\nOn Samsung, also set Notifications, Advanced settings, Floating notifications to Bubbles." else "", good, "Manage") {
                runCatching { startActivity(Intent(Settings.ACTION_APP_NOTIFICATION_BUBBLE_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE, packageName)) }.onFailure { notificationSettings() }
            }
            val prefs = getSharedPreferences("crundi", Context.MODE_PRIVATE)
            val onLeave = prefs.getBoolean(MainActivity.PREF_BUBBLE_ON_LEAVE, true)
            row("Bubbles when you leave", if (onLeave) "On. Chats that are working or waiting on you get a bubble when you leave the app." else "Off. Bubbles appear only when you ask for one, or a chat notifies.", null, if (onLeave) "Turn off" else "Turn on") {
                prefs.edit().putBoolean(MainActivity.PREF_BUBBLE_ON_LEAVE, !onLeave).apply()
                draw()
            }
        } else row("Bubbles", "Need Android 11 or newer.", null, null, null)

        if (LiveCards.onSamsung) {
            val prefs = getSharedPreferences("crundi", Context.MODE_PRIVATE)
            val live = LiveCards.samsungLive(this)
            row(
                "Live notification on Samsung",
                if (live) "On. The card is in the Now Bar and the status bar, with One UI's glow round it."
                else "Off. The card is an ordinary notification: no Now Bar, no status bar chip, and no glow.",
                null, if (live) "Turn off" else "Turn on",
            ) {
                prefs.edit().putBoolean(LiveCards.PREF_SAMSUNG_LIVE, !live).apply()
                LiveCards.refresh(applicationContext)
                draw()
            }
        }
        when (LiveCards.chipAllowed(this)) {
            true -> row("Live card", "While Claude works, a chip in the status bar and a card on the lock screen." + if (Build.MANUFACTURER.equals("samsung", true)) "\nOn Samsung this may also need Developer options, Live notifications for all apps, turned on." else "", true, "Manage") { liveSettings() }
            false -> row("Live card", "The status bar chip is turned off. The card still shows among your notifications.", false, "Allow") { liveSettings() }
            null -> row("Live card", "Shown among your notifications while Claude works. The status bar chip needs Android 16 or newer.", null, null, null)
        }

        row("Camera", if (has(Manifest.permission.CAMERA)) "Allowed. Used to scan the pairing picture, and when a page asks for it." else "Not allowed. Needed to scan the pairing picture.", has(Manifest.permission.CAMERA), if (has(Manifest.permission.CAMERA)) "Manage" else "Allow") {
            if (has(Manifest.permission.CAMERA)) appDetails() else ask(Manifest.permission.CAMERA, 2)
        }
        row("Microphone", if (has(Manifest.permission.RECORD_AUDIO)) "Allowed. Used only when a page asks for it." else "Not allowed. Only needed if a page records sound.", if (has(Manifest.permission.RECORD_AUDIO)) true else null, if (has(Manifest.permission.RECORD_AUDIO)) "Manage" else "Allow") {
            if (has(Manifest.permission.RECORD_AUDIO)) appDetails() else ask(Manifest.permission.RECORD_AUDIO, 3)
        }
        row("Files", "Downloads are saved to the phone's Downloads folder. Android asks for no permission for that, and files you attach are picked one at a time.", null, null, null)

        // ─── Servers ───
        list.addView(heading("Servers"))
        val current = Servers.current(this)?.id
        val servers = Servers.all(this)
        if (servers.isEmpty()) row("No server", "This phone is not paired with a server.", false, null, null)
        for (s in servers) {
            val isCurrent = s.id == current
            val line = listOfNotNull(
                if (s.username.isNotEmpty()) "Signed in as ${s.username}" else null,
                if (s.insecure) "Not a private connection (http)" else null,
                if (Push.available) (if (s.pushToken.isNotEmpty()) "Registered for notifications" else "Not registered for notifications yet") else null,
            ).joinToString("\n")
            row(s.host + if (isCurrent) "  (open)" else "", line, if (s.insecure) false else null, if (isCurrent) "Remove" else "Open") {
                if (isCurrent) confirmRemove(s) else {
                    Servers.setCurrent(this, s.id)
                    startActivity(Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP))
                    finish()
                }
            }
            if (!isCurrent) list.addView(Ui.button(this, "Remove ${s.host}", false) { confirmRemove(s) }, Ui.wide(this, 0).apply { bottomMargin = Ui.dp(this@SettingsActivity, 8) })
        }
        list.addView(Ui.button(this, "Connect another server", true) { startActivity(Intent(this, PairActivity::class.java)) }, Ui.wide(this, 6))

        // ─── Finding out why a notification did not arrive ───
        if (Push.available) {
            list.addView(heading("Notification check"))
            row("Show a test notification", "Made on this phone, with no server involved. If this does not appear, the phone is hiding Crundi's notifications.", null, "Show") { Notifier.localTest(this); draw() }
            row("Register again", "Gives each paired server this phone's address and keys afresh.", null, "Register") {
                PushLog.add(this, "Asked to register again")
                Push.registerAll(this, force = true)
                list.postDelayed({ draw() }, 2500)
            }
            val lines = PushLog.lines(this)
            list.addView(TextView(this).apply {
                text = if (lines.isEmpty()) "Nothing has happened yet. When a notification arrives, or fails to, it is noted here." else lines.takeLast(25).reversed().joinToString("\n")
                setTextColor(getColor(R.color.text_dim))
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 12f)
                typeface = Typeface.MONOSPACE
                setLineSpacing(0f, 1.3f)
                setTextIsSelectable(true)
                val p = Ui.dp(this@SettingsActivity, 12)
                setPadding(p, p, p, p)
                background = GradientDrawable().apply { cornerRadius = Ui.dp(this@SettingsActivity, 12).toFloat(); setColor(getColor(R.color.surface)); setStroke(Ui.dp(this@SettingsActivity, 1), getColor(R.color.border)) }
            }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { bottomMargin = Ui.dp(this@SettingsActivity, 8) })
        }

        // ─── The app ───
        list.addView(heading("About"))
        row("Updates", "This copy updates from ${Updater.WHERE}.", null, "Open") { Updater.open(this) }
        row("Android's settings for Crundi", "Storage used, battery, and every permission in one place.", null, "Open") { appDetails() }
    }

    private fun confirmRemove(s: Server) {
        AlertDialog.Builder(this)
            .setTitle("Remove ${s.host}?")
            .setMessage("This phone will be signed out of that server and will stop getting its notifications. To use it again you pair again.")
            .setNegativeButton("Cancel", null)
            .setPositiveButton("Remove") { _, _ ->
                val app = applicationContext
                // Tell the server first, while there is still a login to tell
                // it with. If it cannot be reached the phone forgets it anyway;
                // the owner can still cut this device off from the server's side.
                Bg.run({
                    runCatching { Api.authed(app, s.id, "POST", "/api/push/mobile/unregister") }
                    runCatching { Api.authed(app, s.id, "POST", "/api/auth/logout") }
                }) {
                    LiveCards.forget(app, s.id)
                    Icons.forget(s.id)
                    Servers.remove(app, s.id)
                    if (Servers.all(app).isEmpty()) {
                        startActivity(Intent(this, PairActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_CLEAR_TASK or Intent.FLAG_ACTIVITY_NEW_TASK))
                        finish()
                    } else draw()
                }
            }
            .show()
    }
}
