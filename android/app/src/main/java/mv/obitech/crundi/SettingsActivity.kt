package mv.obitech.crundi

import android.Manifest
import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.core.app.NotificationManagerCompat
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.pair.AppSettingsScreen
import mv.obitech.crundi.ui.pair.SettingItem
import mv.obitech.crundi.ui.pair.SettingSection
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiTheme

/**
 * The app's own settings: what the phone lets it do, and which servers it is
 * paired with. Everything about a server itself lives in that server's
 * Settings (the native Settings area); this is only for what belongs to the
 * app on this phone.
 *
 * It is rebuilt every time it comes back to the front, because most of what
 * it shows is changed somewhere else: in Android's own settings, which its
 * buttons open.
 */
class SettingsActivity : ComponentActivity() {
    private var sections by mutableStateOf<List<SettingSection>>(emptyList())
    private var removing by mutableStateOf<Server?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val subtitle = "Crundi ${BuildConfig.VERSION_NAME} (${BuildConfig.STORE}), Android ${Build.VERSION.RELEASE}, ${Build.MANUFACTURER} ${Build.MODEL}"
        setContent {
            CrundiTheme {
                Box(Modifier.fillMaxSize().background(CrundiColors.BgPrimary).safeDrawingPadding()) {
                    AppSettingsScreen(subtitle, sections, onBack = { finish() })
                    removing?.let { s ->
                        CrundiDialog(
                            title = "Remove ${s.host}?",
                            message = "This phone will be signed out of that server and will stop getting its notifications. To use it again you pair again.",
                            confirm = "Remove server", danger = true,
                            onConfirm = { removing = null; remove(s) },
                            onDismiss = { removing = null },
                        )
                    }
                }
            }
        }
    }

    override fun onResume() {
        super.onResume()
        draw()
    }

    @Deprecated("Still how a plain permission request answers.")
    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<String>, grantResults: IntArray) {
        @Suppress("DEPRECATION")
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        draw()
    }

    private fun has(permission: String) = checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED

    private fun appDetails() = startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:$packageName")))
    private fun liveSettings() {
        runCatching { startActivity(Intent("android.settings.MANAGE_APP_PROMOTED_NOTIFICATIONS").putExtra(Settings.EXTRA_APP_PACKAGE, packageName)) }.onFailure { notificationSettings() }
    }
    private fun dndAccessSettings() {
        runCatching { startActivity(Intent(Settings.ACTION_NOTIFICATION_POLICY_ACCESS_SETTINGS)) }.onFailure { notificationSettings() }
    }
    /** Android 14 and up: this app's "full-screen notifications" switch. */
    private fun fullScreenSettings() {
        if (Build.VERSION.SDK_INT >= 34) runCatching {
            startActivity(Intent(Settings.ACTION_MANAGE_APP_USE_FULL_SCREEN_INTENT, android.net.Uri.parse("package:$packageName")))
            return
        }
        notificationSettings()
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
        @Suppress("DEPRECATION")
        requestPermissions(arrayOf(permission), code)
    }

    private fun draw() {
        val out = ArrayList<SettingSection>()
        val prefs = getSharedPreferences("crundi", Context.MODE_PRIVATE)

        // ─── What the phone lets the app do ───
        val perms = ArrayList<SettingItem>()
        val notify = NotificationManagerCompat.from(this).areNotificationsEnabled()
        perms += when {
            !Push.available -> SettingItem.Line("Notifications", "This build was made without notifications.")
            notify -> SettingItem.Line("Notifications", "Allowed", true, "Manage") { notificationSettings() }
            else -> SettingItem.Line("Notifications", "Blocked. Claude finishing, or needing you, will not reach this phone.", false, "Allow") {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) ask(Manifest.permission.POST_NOTIFICATIONS, 1) else notificationSettings()
            }
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            val pref = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) nm.bubblePreference else NotificationManager.BUBBLE_PREFERENCE_ALL
            val (state, good) = when (pref) {
                NotificationManager.BUBBLE_PREFERENCE_ALL -> "Any chat can float as a bubble" to true
                NotificationManager.BUBBLE_PREFERENCE_SELECTED -> "Only chats you choose can float as a bubble" to true
                else -> "Off. Chats cannot float as bubbles." to false
            }
            perms += SettingItem.Line("Bubbles", state + if (Build.MANUFACTURER.equals("samsung", true)) "\nOn Samsung, also set Notifications, Advanced settings, Floating notifications to Bubbles." else "", good, "Manage") {
                runCatching { startActivity(Intent(Settings.ACTION_APP_NOTIFICATION_BUBBLE_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE, packageName)) }.onFailure { notificationSettings() }
            }
            val onLeave = prefs.getBoolean(MainActivity.PREF_BUBBLE_ON_LEAVE, true)
            perms += SettingItem.Line(
                "Bubbles when you leave",
                if (onLeave) "On. Chats that are working or waiting on you get a bubble when you leave the app." else "Off. Bubbles appear only when you ask for one, or a chat notifies.",
                null, if (onLeave) "Turn off" else "Turn on",
            ) { prefs.edit().putBoolean(MainActivity.PREF_BUBBLE_ON_LEAVE, !onLeave).apply(); draw() }
        } else perms += SettingItem.Line("Bubbles", "Need Android 11 or newer.")

        if (LiveCards.onSamsung) {
            val live = LiveCards.samsungLive(this)
            perms += SettingItem.Line(
                "Live notification on Samsung",
                if (live) "On. The card is in the Now Bar and the status bar, with One UI's glow round it."
                else "Off. The card is an ordinary notification: no Now Bar, no status bar chip, and no glow.",
                null, if (live) "Turn off" else "Turn on",
            ) { prefs.edit().putBoolean(LiveCards.PREF_SAMSUNG_LIVE, !live).apply(); LiveCards.refresh(applicationContext); draw() }
        }
        perms += when (LiveCards.chipAllowed(this)) {
            true -> SettingItem.Line("Live card", "While Claude works, a chip in the status bar and a card on the lock screen." + if (Build.MANUFACTURER.equals("samsung", true)) "\nOn Samsung this may also need Developer options, Live notifications for all apps, turned on." else "", true, "Manage") { liveSettings() }
            false -> SettingItem.Line("Live card", "The status bar chip is turned off. The card still shows among your notifications.", false, "Allow") { liveSettings() }
            null -> SettingItem.Line("Live card", "Shown among your notifications while Claude works. The status bar chip needs Android 16 or newer.")
        }
        if (Push.available) {
            // Urgent alerts ring through Do Not Disturb only if the person
            // says so here AND grants the access in Android's settings.
            // Whatever the state, the channel follows it (made or removed).
            Notifier.urgentDndChannel(this)
            val dndOn = UrgentAlerts.dndOptIn(this)
            val granted = UrgentAlerts.dndGranted(this)
            perms += when {
                !dndOn -> SettingItem.Line("Ring through Do Not Disturb", "Off. Urgent alerts stay silent while Do Not Disturb is on.", null, "Turn on") {
                    UrgentAlerts.setDndOptIn(this, true)
                    if (!UrgentAlerts.dndGranted(this)) dndAccessSettings() else draw()
                }
                !granted -> SettingItem.Line("Ring through Do Not Disturb", "On, but Crundi has not been given Do Not Disturb access, so urgent alerts stay silent in Do Not Disturb. Allow it in Android's settings.", false, "Allow") { dndAccessSettings() }
                else -> SettingItem.Line("Ring through Do Not Disturb", "On. Urgent alerts ring at alarm volume even in Do Not Disturb.", true, "Turn off") {
                    UrgentAlerts.setDndOptIn(this, false); draw()
                }
            }
            if (dndOn && !granted) perms += SettingItem.Button("Turn off ringing through Do Not Disturb") { UrgentAlerts.setDndOptIn(this, false); draw() }
            // Full screen, like an alarm clock: the person's choice, and on
            // Android 14 and up also Android's switch for this app.
            val fullOn = UrgentAlerts.fullScreenOptIn(this)
            val fullOk = UrgentAlerts.fullScreenGranted(this)
            perms += when {
                !fullOn -> SettingItem.Line("Full-screen urgent alerts", "Off. Urgent alerts ring as a notification at the top of the screen.", null, "Turn on") {
                    UrgentAlerts.setFullScreenOptIn(this, true)
                    if (!UrgentAlerts.fullScreenGranted(this)) fullScreenSettings() else draw()
                }
                !fullOk -> SettingItem.Line("Full-screen urgent alerts", "On, but Android has not allowed Crundi to show full-screen notifications, so urgent alerts stay a notification. Allow it in Android's settings.", false, "Allow") { fullScreenSettings() }
                else -> SettingItem.Line("Full-screen urgent alerts", "On. When the phone is locked or asleep, an urgent alert lights up the screen like an alarm: slide right to attend, left to dismiss.", true, "Turn off") {
                    UrgentAlerts.setFullScreenOptIn(this, false); draw()
                }
            }
            if (fullOn && !fullOk) perms += SettingItem.Button("Turn off full-screen urgent alerts") { UrgentAlerts.setFullScreenOptIn(this, false); draw() }
            // How long an urgent alert rings before it counts as unanswered
            // (and alert_user, waiting on it, hears "no answer").
            val ring = UrgentAlerts.ringMinutes(this)
            perms += SettingItem.Line("Urgent alerts ring for", "$ring minute${if (ring == 1) "" else "s"}, then stop and count as unanswered. Snooze brings one back after 5, 15, 30 or 60 minutes.", null, "Change") {
                val choices = UrgentAlerts.RING_RANGE.toList()
                android.app.AlertDialog.Builder(this, android.R.style.Theme_DeviceDefault_Dialog_Alert)
                    .setTitle("Ring for")
                    .setSingleChoiceItems(choices.map { "$it minute${if (it == 1) "" else "s"}" }.toTypedArray(), choices.indexOf(ring)) { dlg, which ->
                        UrgentAlerts.setRingMinutes(this, choices[which]); dlg.dismiss(); draw()
                    }
                    .setNegativeButton("Cancel", null)
                    .show()
            }
        }
        val cam = has(Manifest.permission.CAMERA)
        perms += SettingItem.Line("Camera", if (cam) "Allowed. Used to scan the pairing picture, and to take a photo for a chat." else "Not allowed. Needed to scan the pairing picture.", cam, if (cam) "Manage" else "Allow") {
            if (cam) appDetails() else ask(Manifest.permission.CAMERA, 2)
        }
        val mic = has(Manifest.permission.RECORD_AUDIO)
        perms += SettingItem.Line("Microphone", if (mic) "Allowed. Used only when you record sound." else "Not allowed. Only needed to record sound.", if (mic) true else null, if (mic) "Manage" else "Allow") {
            if (mic) appDetails() else ask(Manifest.permission.RECORD_AUDIO, 3)
        }
        perms += SettingItem.Line("Files", "Downloads are saved to the phone's Downloads folder. Android asks for no permission for that, and files you attach are picked one at a time.")
        out += SettingSection("Permissions", perms)

        // ─── Secret PINs behind the fingerprint (BiometricPins) ───
        val pins = BiometricPins.store(this)
        // The enrolled biometrics changed: Android destroyed the key, so the PINs are useless.
        val forgottenNow = pins.enabled && pins.list().isNotEmpty() && !PinKey.stillValid()
        if (forgottenNow) pins.forgetAll()
        out += SettingSection("Secret PINs", PinSettings.items(
            BiometricPins.available(this), pins.enabled, pins.list(), forgottenNow,
            toggle = {
                if (pins.enabled) { pins.enabled = false; PinKey.delete() } else pins.enabled = true
                draw()
            },
            forget = { p -> pins.forget(p.server, p.secretId); draw() },
            forgetAll = { pins.forgetAll(); draw() },
        ))

        // ─── Servers ───
        val srv = ArrayList<SettingItem>()
        val current = Servers.current(this)?.id
        val servers = Servers.all(this)
        if (servers.isEmpty()) srv += SettingItem.Line("No server", "This phone is not paired with a server.", false)
        for (s in servers) {
            val isCurrent = s.id == current
            val line = listOfNotNull(
                if (s.username.isNotEmpty()) "Signed in as ${s.username}" else null,
                if (s.insecure) "Not a private connection (http)" else null,
                if (Push.available) (if (s.pushToken.isNotEmpty()) "Registered for notifications" else "Not registered for notifications yet") else null,
            ).joinToString("\n")
            srv += SettingItem.Line(s.host + if (isCurrent) " (open)" else "", line, if (s.insecure) false else null, if (isCurrent) "Remove" else "Open") {
                if (isCurrent) removing = s else {
                    Servers.setCurrent(this, s.id)
                    startActivity(Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP))
                    finish()
                }
            }
            if (!isCurrent) srv += SettingItem.Button("Remove ${s.host}") { removing = s }
            if (Push.available) {
                // Per server: an urgent alert from it rings until seen, or is
                // shown as an ordinary notification.
                val urgentOn = UrgentAlerts.serverOn(this, s.id)
                srv += SettingItem.Line(
                    "Urgent alerts from ${s.host}",
                    if (urgentOn) "On. Claude can ring this phone until you look, at most 3 times an hour." else "Off. Urgent alerts from this server are shown as ordinary notifications, without ringing.",
                    null, if (urgentOn) "Turn off" else "Turn on",
                ) { UrgentAlerts.setServerOn(this, s.id, !urgentOn); draw() }
            }
        }
        srv += SettingItem.Button("Connect another server", primary = true) { startActivity(Intent(this, PairActivity::class.java)) }
        out += SettingSection("Servers", srv)

        // ─── Finding out why a notification did not arrive ───
        if (Push.available) {
            out += SettingSection(
                "Notification check",
                listOf(
                    SettingItem.Line("Show a test notification", "Made on this phone, with no server involved. If this does not appear, the phone is hiding Crundi's notifications.", null, "Show") { Notifier.localTest(this); draw() },
                    SettingItem.Line("Register again", "Gives each paired server this phone's address and keys afresh.", null, "Register") {
                        PushLog.add(this, "Asked to register again")
                        Push.registerAll(this, force = true)
                        window.decorView.postDelayed({ draw() }, 2500)
                    },
                    SettingItem.Log(PushLog.lines(this).takeLast(25).reversed()),
                ),
            )
        }

        // ─── The app ───
        out += SettingSection(
            "About",
            listOf(
                SettingItem.Line("Updates", "This copy updates from ${Updater.WHERE}.", null, "Open") { Updater.open(this) },
                SettingItem.Line("Android's settings for Crundi", "Storage used, battery, and every permission in one place.", null, "Open") { appDetails() },
            ),
        )
        sections = out
    }

    private fun remove(s: Server) {
        val app = applicationContext
        // Tell the server first, while there is still a login to tell it with.
        // If it cannot be reached the phone forgets it anyway; the owner can
        // still cut this device off from the server's side.
        Bg.run({
            runCatching { Api.authed(app, s.id, "POST", "/api/push/mobile/unregister") }
            runCatching { Api.authed(app, s.id, "POST", "/api/auth/logout") }
        }) {
            LiveCards.forget(app, s.id)
            Icons.forget(s.id)
            PageLogin.forget(app, s.id)
            UrgentAlerts.forget(app, s.id)
            BiometricPins.store(app).forgetServer(SavedPins.serverKey(s.baseUrl))
            AppGraph.forget(s.id)
            Servers.remove(app, s.id)
            if (Servers.all(app).isEmpty()) {
                startActivity(Intent(this, PairActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_CLEAR_TASK or Intent.FLAG_ACTIVITY_NEW_TASK))
                finish()
            } else draw()
        }
    }
}
