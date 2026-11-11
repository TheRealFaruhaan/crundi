package mv.obitech.homepoolnode

import android.Manifest
import android.app.Activity
import android.app.AlertDialog
import android.content.ClipboardManager
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.PowerManager
import android.provider.Settings
import android.view.View
import android.widget.Button
import android.widget.EditText
import android.widget.Switch
import android.widget.TextView
import android.widget.Toast
import com.google.zxing.integration.android.IntentIntegrator
import mv.obitech.crundi.homepool.Homepool
import mv.obitech.crundi.homepool.NodeConfig
import mv.obitech.crundi.homepool.NodePhase
import mv.obitech.crundi.homepool.NodeSnapshot
import mv.obitech.crundi.homepool.formatBytes
import mv.obitech.crundi.homepool.formatDuration

/**
 * Homepool Node: one screen over the shared node core
 * (mv.obitech.crundi.homepool, the same code the Crundi app runs).
 *
 *   Start / Stop            run the node now / stop it now
 *   Register service        start at boot + restart if Android kills it (and start now)
 *   Unregister service      stop it; it no longer starts on its own
 */
class MainActivity : Activity() {

    private val ui = Handler(Looper.getMainLooper())

    private lateinit var status: TextView
    private lateinit var details: TextView
    private lateinit var startBtn: Button
    private lateinit var stopBtn: Button
    private lateinit var serviceRow: TextView
    private lateinit var registerBtn: Button
    private lateinit var unregisterBtn: Button
    private lateinit var configRow: TextView
    private lateinit var codeEdit: EditText
    private lateinit var forgetBtn: Button
    private lateinit var wifiOnly: Switch
    private lateinit var capEdit: EditText
    private lateinit var notifRow: TextView
    private lateinit var notifBtn: Button
    private lateinit var batteryRow: TextView
    private lateinit var batteryBtn: Button
    private lateinit var logView: TextView

    private val refresh = object : Runnable {
        override fun run() { render(); ui.postDelayed(this, 1000) }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)
        Homepool.reconcile(this)

        status = findViewById(R.id.status)
        details = findViewById(R.id.details)
        startBtn = findViewById(R.id.startBtn)
        stopBtn = findViewById(R.id.stopBtn)
        serviceRow = findViewById(R.id.serviceRow)
        registerBtn = findViewById(R.id.registerBtn)
        unregisterBtn = findViewById(R.id.unregisterBtn)
        configRow = findViewById(R.id.configRow)
        codeEdit = findViewById(R.id.codeEdit)
        forgetBtn = findViewById(R.id.forgetBtn)
        wifiOnly = findViewById(R.id.wifiOnly)
        capEdit = findViewById(R.id.capEdit)
        notifRow = findViewById(R.id.notifRow)
        notifBtn = findViewById(R.id.notifBtn)
        batteryRow = findViewById(R.id.batteryRow)
        batteryBtn = findViewById(R.id.batteryBtn)
        logView = findViewById(R.id.log)

        // ── service controls ──
        startBtn.setOnClickListener { if (ready()) { askBackgroundPermissions(); Homepool.start(this) } }
        stopBtn.setOnClickListener { Homepool.stop(this) }
        registerBtn.setOnClickListener {
            if (!ready()) return@setOnClickListener
            askBackgroundPermissions()
            Homepool.setAutoStart(this, true)
            toast("Registered: the node starts at boot and restarts if Android stops it")
        }
        unregisterBtn.setOnClickListener {
            AlertDialog.Builder(this)
                .setTitle("Unregister the service?")
                .setMessage("The node stops now and will not start on its own again, not at boot, not after being killed. You can still Start it by hand.")
                .setPositiveButton("Unregister") { _, _ -> Homepool.setAutoStart(this, false); Homepool.stop(this); render() }
                .setNegativeButton("Cancel", null)
                .show()
        }

        // ── node code ──
        findViewById<Button>(R.id.scanBtn).setOnClickListener { scan() }
        findViewById<Button>(R.id.pasteBtn).setOnClickListener { pasteCode() }
        findViewById<Button>(R.id.useCodeBtn).setOnClickListener { applyCode(codeEdit.text.toString()) }
        forgetBtn.setOnClickListener { forget() }

        // ── network rules ──
        val s = Homepool.snapshot.value
        wifiOnly.isChecked = s.wifiOnly
        wifiOnly.setOnCheckedChangeListener { _, on -> Homepool.setWifiOnly(this, on) }
        capEdit.setText(s.mobileCapMb.toString())
        findViewById<Button>(R.id.capSave).setOnClickListener {
            Homepool.setMobileCapMb(this, capEdit.text.toString().trim().toIntOrNull() ?: 0)
            capEdit.setText(Homepool.snapshot.value.mobileCapMb.toString())
            toast("Saved")
        }

        // ── background permissions ──
        notifBtn.setOnClickListener {
            if (Build.VERSION.SDK_INT >= 33) requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 1)
        }
        batteryBtn.setOnClickListener { requestBatteryExemption() }
        findViewById<Button>(R.id.appSettingsBtn).setOnClickListener {
            startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:$packageName")))
        }
        findViewById<Button>(R.id.oemBtn).setOnClickListener {
            runCatching { startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("https://dontkillmyapp.com/"))) }
        }

        handleLink(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleLink(intent)
    }

    override fun onResume() { super.onResume(); ui.post(refresh) }
    override fun onPause() { ui.removeCallbacks(refresh); super.onPause() }

    private fun ready(): Boolean {
        if (Homepool.config(this) != null) return true
        toast("Set the node code first: scan or paste it")
        return false
    }

    // ── node code ──

    private fun handleLink(i: Intent?) {
        val data = i?.data ?: return
        if (data.scheme == "crundi-homepool") applyCode(data.toString())
    }

    private fun scan() {
        IntentIntegrator(this)
            .setDesiredBarcodeFormats(IntentIntegrator.QR_CODE)
            .setPrompt("Scan the homepool node code")
            .setBeepEnabled(false)
            .setOrientationLocked(false)
            .initiateScan()
    }

    @Deprecated("Plain Activity without the AndroidX result API")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        val r = IntentIntegrator.parseActivityResult(requestCode, resultCode, data)
        if (r != null) { r.contents?.let { applyCode(it) }; return }
        @Suppress("DEPRECATION")
        super.onActivityResult(requestCode, resultCode, data)
    }

    private fun pasteCode() {
        val cm = getSystemService(ClipboardManager::class.java)
        val text = cm.primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(this)?.toString()
        if (text.isNullOrBlank()) { toast("The clipboard is empty: copy the crundi-homepool:// code first"); return }
        applyCode(text)
    }

    private fun applyCode(text: String) {
        val c = NodeConfig.parse(text)
        if (c == null) { toast("That isn't a homepool node code (crundi-homepool://node?…)"); return }
        AlertDialog.Builder(this)
            .setTitle("Use this node code?")
            .setMessage("Node ${c.nodeId.ifEmpty { "?" }}\non ${c.server}\n\nThis phone will carry traffic for that server while the node runs.")
            .setPositiveButton("Use it") { _, _ ->
                Homepool.setConfig(this, c)
                codeEdit.setText("")
                toast("Saved")
                render()
            }
            .setNegativeButton("Cancel", null)
            .show()
    }

    private fun forget() {
        AlertDialog.Builder(this)
            .setTitle("Forget the node code?")
            .setMessage("The node stops, is unregistered, and its key is removed from this phone.")
            .setPositiveButton("Forget") { _, _ -> Homepool.setConfig(this, null); render() }
            .setNegativeButton("Cancel", null)
            .show()
    }

    // ── background permissions ──

    private fun askBackgroundPermissions() {
        if (Build.VERSION.SDK_INT >= 33 &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            // The node runs either way; without this its status notification is hidden.
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 1)
        } else if (!ignoringBatteryOptimizations()) {
            requestBatteryExemption()
        }
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        // Chain on to the battery exemption once the notification prompt is answered.
        if (requestCode == 1 && Homepool.snapshot.value.enabled && !ignoringBatteryOptimizations()) requestBatteryExemption()
    }

    private fun ignoringBatteryOptimizations(): Boolean =
        getSystemService(PowerManager::class.java).isIgnoringBatteryOptimizations(packageName)

    private fun requestBatteryExemption() {
        if (ignoringBatteryOptimizations()) return
        runCatching {
            @Suppress("BatteryLife")
            startActivity(Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:$packageName")))
        }.onFailure {
            runCatching { startActivity(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS)) }
        }
    }

    // ── rendering ──

    private fun render() {
        val s: NodeSnapshot = Homepool.snapshot.value
        val running = s.enabled && s.phase != NodePhase.STOPPED && s.phase != NodePhase.REFUSED

        status.text = s.label()
        status.setTextColor(
            when (s.phase) {
                NodePhase.STOPPED -> 0xFF777777.toInt()
                NodePhase.CONNECTED -> 0xFF1B8A3A.toInt()
                NodePhase.PAUSED -> 0xFFB26A00.toInt()
                NodePhase.REFUSED -> 0xFFB3261E.toInt()
                NodePhase.CONNECTING -> 0xFF1F5FBF.toInt()
            }
        )

        startBtn.isEnabled = s.configured && !running
        stopBtn.isEnabled = running || s.enabled
        registerBtn.isEnabled = s.configured && !s.autoStart
        unregisterBtn.isEnabled = s.autoStart
        serviceRow.text = if (s.autoStart)
            "Service: registered. Starts at boot and restarts if Android stops it."
        else
            "Service: not registered. Runs only while started, never on its own."

        details.text = buildString {
            if (!s.configured) append("Not configured\n")
            else {
                append("Node: ").append(s.nodeName ?: Homepool.config(this@MainActivity)?.nodeId ?: "?")
                s.networkName?.let { append(" · network ").append(it) }
                append('\n')
            }
            append("Network: ").append(s.networkLabel)
            append("\nPublic IP: ").append(s.publicIp ?: "—")
            if (s.connectedSince > 0) append("\nConnected for: ").append(formatDuration(System.currentTimeMillis() - s.connectedSince))
            append("\nData today: ").append(formatBytes(s.bytesToday))
            append(" (mobile ").append(formatBytes(s.mobileBytesToday))
            if (s.mobileCapMb > 0) append(" of ").append(s.mobileCapMb).append(" MB")
            append(")")
            s.server?.let { v ->
                append("\nCarrying: ").append(v.conns).append(" connection").append(if (v.conns == 1) "" else "s")
                append(", ").append(v.slots).append(" slot").append(if (v.slots == 1) "" else "s")
                if (v.keys.isNotEmpty()) append(" for ").append(v.keys.joinToString(", "))
            }
        }

        configRow.text = s.serverUrl?.let { "Server: $it" } ?: "No node code set yet."
        forgetBtn.isEnabled = s.configured

        val notifOk = Build.VERSION.SDK_INT < 33 ||
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED
        notifRow.text = if (notifOk) "Notifications: allowed" else "Notifications: blocked, the node's status won't show"
        notifBtn.visibility = if (notifOk) View.GONE else View.VISIBLE
        val batteryOk = ignoringBatteryOptimizations()
        batteryRow.text = if (batteryOk) "Battery: unrestricted, can run in the background"
            else "Battery: optimised, Android may stop the node"
        batteryBtn.visibility = if (batteryOk) View.GONE else View.VISIBLE

        logView.text = s.log.takeLast(25).joinToString("\n").ifEmpty { "No activity yet." }
    }

    private fun toast(msg: String) = Toast.makeText(this, msg, Toast.LENGTH_LONG).show()
}
