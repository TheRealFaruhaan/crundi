package mv.obitech.crundi.ui.operations

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalInspectionMode
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.BackupItem
import mv.obitech.crundi.api.BackupRunning
import mv.obitech.crundi.api.BackupSchedule
import mv.obitech.crundi.api.BackupStatus
import mv.obitech.crundi.api.BackupStorageForm
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiCard
import mv.obitech.crundi.ui.components.CrundiChip
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiSwitchRow
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.StatusBadge
import mv.obitech.crundi.ui.components.errorText
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

/*
 * Settings > Backup and restore, as the web's section of that name
 * (renderBackupSection): what is happening now, the storage, the passphrase,
 * the schedule, and the backups in storage with Restore and Delete. Restore
 * and Delete ask first. Progress comes live ("backup" events). A server
 * without backups (404): "Update the server to use this".
 */

/** The sections, each folded until tapped. */
object BackupSection {
    const val STORAGE = "storage"
    const val PASSPHRASE = "passphrase"
    const val SCHEDULE = "schedule"
    const val LIST = "list"
}

/** The words and rules the page shows, kept apart so they can be tested. */
object BackupRules {
    val DAYS = listOf("Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday")

    fun phase(r: BackupRunning): String {
        val pct = if (r.total > 0) " " + Math.round(100 * r.done / r.total) + "%" else ""
        return when (r.phase) {
            "collecting" -> "Gathering files…"
            "packing" -> "Packing and encrypting…$pct"
            "uploading" -> "Uploading…$pct"
            "tidying" -> "Removing old backups…"
            "downloading" -> "Downloading…$pct"
            "unpacking" -> "Decrypting and unpacking…"
            else -> "Working…"
        }
    }

    /** How far along, 0 to 1, or null while it cannot be told. */
    fun progress(r: BackupRunning): Float? =
        if (r.total > 0 && r.phase !in setOf("unpacking", "collecting", "tidying")) (r.done / r.total).toFloat().coerceIn(0f, 1f) else null

    /** "today at 03:00", "tomorrow at 03:00", "Fri 6 Nov at 14:00". */
    fun whenText(ms: Long, now: Long = System.currentTimeMillis(), zone: ZoneId = ZoneId.systemDefault()): String {
        if (ms <= 0) return ""
        val d = Instant.ofEpochMilli(ms).atZone(zone)
        val diff = d.toLocalDate().toEpochDay() - LocalDate.ofInstant(Instant.ofEpochMilli(now), zone).toEpochDay()
        val t = DateTimeFormatter.ofPattern("HH:mm", Locale.UK).format(d)
        return when (diff) {
            0L -> "today at $t"
            1L -> "tomorrow at $t"
            -1L -> "yesterday at $t"
            else -> DateTimeFormatter.ofPattern("EEE d MMM", Locale.UK).format(d) + " at $t"
        }
    }

    /** The schedule in a few words, for its folded header. */
    fun scheduleSummary(s: BackupSchedule): String = when {
        !s.enabled -> "Off"
        s.frequency == "weekly" -> "${DAYS.getOrElse(s.weekday) { "Sunday" }}s at ${s.time}"
        s.frequency == "hours" -> "Every ${s.everyHours} h"
        else -> "Daily at ${s.time}"
    }

    /** What is wrong with the form before it is sent, or null. The server checks the rest. */
    fun formError(pass1: String, pass2: String, time: String, everyHours: String, keep: String): String? = when {
        pass1 != pass2 -> "The two passphrases differ"
        !Regex("^([01]\\d|2[0-3]):[0-5]\\d$").matches(time.trim()) -> "Time must be HH:MM"
        everyHours.trim().toIntOrNull()?.let { it in 1..168 } != true -> "Every 1 to 168 hours"
        keep.trim().toIntOrNull()?.let { it in 0..1000 } != true -> "Keep 0 to 1000 backups"
        else -> null
    }
}

data class BackupState(
    /** Ready(null): the server has no backups (older). */
    val status: Load<BackupStatus?> = Load.Loading,
    /** The bucket's listing, once asked for; [listError] when it could not be read. */
    val items: List<BackupItem>? = null,
    val listError: String = "",
    val listing: Boolean = false,
    /** "Connected. The bucket can be listed." or the storage's words. */
    val testResult: Pair<String, Boolean>? = null,
    val busy: String? = null,
    val note: Actions.Note? = null,
)

class BackupEvents(
    val onRun: () -> Unit = {},
    val onSkip: () -> Unit = {},
    val onRestart: () -> Unit = {},
    val onCancelRestore: () -> Unit = {},
    val onTest: (BackupStorageForm) -> Unit = {},
    val onSave: (BackupStorageForm, String, BackupSchedule, Int) -> Unit = { _, _, _, _ -> },
    val onList: () -> Unit = {},
    val onRestore: (String, String) -> Unit = { _, _ -> },
    val onDelete: (String) -> Unit = {},
    val onCloseNote: () -> Unit = {},
    val onBack: () -> Unit = {},
)

@Composable
fun BackupRoute() {
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val actions = rememberActions()
    val scope = rememberCoroutineScope()
    val status = rememberLoad { api.backup.status() }
    var items by remember { mutableStateOf<List<BackupItem>?>(null) }
    var listError by remember { mutableStateOf("") }
    var listing by remember { mutableStateOf(false) }
    var test by remember { mutableStateOf<Pair<String, Boolean>?>(null) }
    val inspecting = LocalInspectionMode.current
    fun loadItems() {
        listing = true
        scope.launch {
            try { items = api.backup.list(); listError = "" } catch (e: ApiException) { listError = errorText(e) }
            listing = false
        }
    }
    // Progress, a finished backup, a staged restore: as they happen.
    LaunchedEffect(Unit) {
        if (inspecting) return@LaunchedEffect
        var wasRunning = false
        api.events.changes("backup").collect { ev ->
            val running = ev.data["running"].let { it != null && it !is kotlinx.serialization.json.JsonNull }
            status.reload()
            if (wasRunning && !running && items != null) loadItems()
            wasRunning = running
        }
    }
    BackupScreen(
        BackupState(status.value, items, listError, listing, test, actions.busy, actions.note),
        BackupEvents(
            onRun = { actions.run("run", "Backing up. It goes on if you leave this page.", status::reload) { api.backup.run() } },
            onSkip = { actions.run("skip", "Skipped. The next backup runs on schedule.", status::reload) { api.backup.skip() } },
            onRestart = { actions.run("restart", "Restarting to finish the restore") { api.backup.restart() } },
            onCancelRestore = { actions.run("cancel", "Restore cancelled", status::reload) { api.backup.cancelRestore() } },
            onTest = { form ->
                test = "Checking…" to true
                scope.launch { test = try { api.backup.test(form); "Connected. The bucket can be listed." to true } catch (e: ApiException) { errorText(e) to false } }
            },
            onSave = { form, pass, sch, keep -> actions.run("save", "Backup settings saved", status::reload) { api.backup.save(form, pass, sch, keep) } },
            onList = { loadItems() },
            onRestore = { key, pass -> actions.run("restore", "Restoring. Crundi restarts when the backup is unpacked.", status::reload) { api.backup.restore(key, pass) } },
            onDelete = { key -> actions.run("delete", "Backup deleted", ::loadItems) { api.backup.delete(key) } },
            onCloseNote = actions::clear,
            onBack = { nav.back() },
        ),
    )
}

/** [open]: sections unfolded at first (screenshots); [confirmRestore]: the backup whose restore is being asked about. */
@Composable
fun BackupScreen(state: BackupState, events: BackupEvents = BackupEvents(), open: Set<String> = emptySet(), confirmRestore: String = "") {
    SettingsPageFrame(SettingsPage.Backup, state.note, events.onCloseNote, events.onBack) {
        when (val l = state.status) {
            Load.Loading -> LoadingState()
            is Load.Failed -> ErrorState(l.message)
            is Load.Ready -> {
                val d = l.value
                if (d == null) EmptyState(icon = "upload", title = "Not supported by this server", body = "This server has no backups yet. Update the server to use this.")
                else BackupBody(d, state, events, open, confirmRestore)
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun BackupBody(d: BackupStatus, state: BackupState, events: BackupEvents, open: Set<String>, confirmRestoreAt: String) {
    val st = d.storage
    val sch = d.schedule
    // The form, kept while the page is open; filled again when the server's settings change.
    var endpoint by remember(st) { mutableStateOf(st.endpoint) }
    var bucket by remember(st) { mutableStateOf(st.bucket) }
    var region by remember(st) { mutableStateOf(st.region) }
    var prefix by remember(st) { mutableStateOf(st.prefix) }
    var keyId by remember(st) { mutableStateOf(st.accessKeyId) }
    var secret by remember(st) { mutableStateOf("") }
    var pathStyle by remember(st) { mutableStateOf(st.pathStyle) }
    var pass1 by remember(d.hasPassphrase) { mutableStateOf("") }
    var pass2 by remember(d.hasPassphrase) { mutableStateOf("") }
    var on by remember(sch) { mutableStateOf(sch.enabled) }
    var freq by remember(sch) { mutableStateOf(sch.frequency) }
    var weekday by remember(sch) { mutableStateOf(sch.weekday) }
    var hours by remember(sch) { mutableStateOf(sch.everyHours.toString()) }
    var time by remember(sch) { mutableStateOf(sch.time) }
    var keep by remember(d.keep) { mutableStateOf(d.keep.toString()) }
    var formError by remember { mutableStateOf<String?>(null) }
    var restoreKey by remember { mutableStateOf(confirmRestoreAt) }
    var restorePass by remember { mutableStateOf("") }
    var deleteKey by remember { mutableStateOf<String?>(null) }
    val form = { BackupStorageForm(endpoint, region, bucket, prefix, keyId, secret, pathStyle) }

    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(Dimens.ScreenPad), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text("The whole of Crundi, encrypted, to S3-compatible storage: settings, sign-in, projects, layouts, kanban, notes, mindmap, media, schedules, services, and Claude's transcripts and skills.", style = CrundiType.Small, modifier = Modifier.padding(horizontal = 2.dp))
        StatusCard(d, state, events)

        FoldSection(BackupSection.STORAGE, "Storage", if (d.storage.bucket.isNotEmpty() && d.storage.hasSecret) d.storage.bucket else "Not set up", open, summaryColor = if (d.storage.bucket.isEmpty()) CrundiColors.TextMuted else null) {
            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Field("Endpoint")
                CrundiTextField(endpoint, { endpoint = it }, Modifier.fillMaxWidth(), placeholder = "https://ACCOUNT.r2.cloudflarestorage.com", mono = true)
                Text("Any S3-compatible storage: Cloudflare R2, AWS S3, Backblaze B2, MinIO, Wasabi…", style = CrundiType.Small)
                Field("Bucket")
                CrundiTextField(bucket, { bucket = it }, Modifier.fillMaxWidth(), mono = true)
                Field("Region")
                CrundiTextField(region, { region = it }, Modifier.fillMaxWidth(), placeholder = "auto", mono = true)
                Text("R2 uses auto; AWS needs the bucket's region.", style = CrundiType.Small)
                Field("Folder")
                CrundiTextField(prefix, { prefix = it }, Modifier.fillMaxWidth(), placeholder = "crundi/", mono = true)
                Field("Addresses")
                Segments(listOf("path" to "Path", "vhost" to "Virtual-hosted"), if (pathStyle) "path" else "vhost", { pathStyle = it == "path" }, Modifier.fillMaxWidth())
                Field("Access key ID")
                CrundiTextField(keyId, { keyId = it }, Modifier.fillMaxWidth(), mono = true)
                Field("Secret access key")
                CrundiTextField(secret, { secret = it }, Modifier.fillMaxWidth(), placeholder = if (st.hasSecret) "Saved. Type to replace it" else "", mono = true, password = true)
                Row(Modifier.padding(top = 4.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    CrundiButton("Test the connection", { events.onTest(form()) }, icon = "refresh")
                    state.testResult?.let { (t, good) -> Text(t, style = CrundiType.Small.copy(color = if (t == "Checking…") CrundiColors.TextSecondary else if (good) CrundiColors.Green else CrundiColors.Red), modifier = Modifier.weight(1f)) }
                }
            }
        }

        FoldSection(BackupSection.PASSPHRASE, "Passphrase", if (d.hasPassphrase) "Set" else "Not set", open, summaryColor = if (d.hasPassphrase) null else CrundiColors.TextMuted) {
            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Field(if (d.hasPassphrase) "New passphrase" else "Passphrase")
                CrundiTextField(pass1, { pass1 = it }, Modifier.fillMaxWidth(), placeholder = if (d.hasPassphrase) "Set. Type to change it" else "At least 8 characters", icon = "key", password = true)
                Field("Again")
                CrundiTextField(pass2, { pass2 = it }, Modifier.fillMaxWidth(), icon = "key", password = true)
                Text("Backups hold your sign-in and settings, so they are encrypted with this. You need it to restore; without it a backup cannot be opened. Keep it somewhere other than this server.", style = CrundiType.Small)
            }
        }

        FoldSection(BackupSection.SCHEDULE, "Schedule", BackupRules.scheduleSummary(sch), open) {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                CrundiSwitchRow("Back up automatically", on, { on = it }, Modifier.padding(horizontal = 0.dp))
                Segments(listOf("daily" to "Daily", "weekly" to "Weekly", "hours" to "Every few hours"), freq, { freq = it }, Modifier.fillMaxWidth())
                if (freq == "weekly") FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    for ((i, n) in BackupRules.DAYS.withIndex()) CrundiChip(n.take(3), weekday == i, { weekday = i })
                }
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    if (freq == "hours") {
                        Text("Every", style = CrundiType.Small.copy(color = CrundiColors.TextSecondary))
                        CrundiTextField(hours, { hours = it.filter(Char::isDigit).take(3) }, Modifier.width(70.dp), mono = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number))
                        Text("hours from", style = CrundiType.Small.copy(color = CrundiColors.TextSecondary))
                    } else Text("At", style = CrundiType.Small.copy(color = CrundiColors.TextSecondary))
                    CrundiTextField(time, { time = it.take(5) }, Modifier.width(90.dp), placeholder = "03:00", mono = true)
                    if (d.timezone.isNotEmpty()) Text("${d.timezone} time", style = CrundiType.Small, modifier = Modifier.weight(1f))
                }
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("Keep the last", style = CrundiType.Small.copy(color = CrundiColors.TextSecondary))
                    CrundiTextField(keep, { keep = it.filter(Char::isDigit).take(4) }, Modifier.width(80.dp), mono = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number))
                    Text("scheduled backups", style = CrundiType.Small.copy(color = CrundiColors.TextSecondary))
                }
                Text("Older scheduled backups of this machine are deleted after each new one. 0 keeps them all. Backups you make by hand are kept until you delete them.", style = CrundiType.Small)
                Text("If you are using Crundi when a backup is due, it tells you and waits until you have left.", style = CrundiType.Small)
            }
        }

        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiButton("Save", {
                formError = BackupRules.formError(pass1, pass2, time, hours, keep)
                if (formError == null) {
                    events.onSave(form(), pass1, BackupSchedule(on, freq, time.trim(), weekday, hours.trim().toInt()), keep.trim().toInt())
                    secret = ""; pass1 = ""; pass2 = ""
                }
            }, kind = ButtonKind.Primary, icon = "check", enabled = state.busy != "save")
            Text(formError ?: "Saves the storage, passphrase and schedule together.", style = CrundiType.Small.copy(color = if (formError != null) CrundiColors.Red else CrundiColors.TextMuted), modifier = Modifier.weight(1f))
        }

        FoldSection(BackupSection.LIST, "Backups in storage", state.items?.size?.let { "$it" } ?: "", open, card = false) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("What is in the storage folder, newest first.", style = CrundiType.Small, modifier = Modifier.weight(1f))
                CrundiButton(if (state.listing) "Loading…" else if (state.items == null) "Show" else "Refresh", events.onList, icon = "refresh", enabled = !state.listing && d.configured)
            }
            if (!d.configured) Text("Set up the storage and a passphrase first.", style = CrundiType.Small)
            if (state.listError.isNotEmpty()) Text(state.listError, style = CrundiType.Small.copy(color = CrundiColors.Red))
            val list = state.items
            if (list != null && list.isEmpty()) Text("No backups in this folder yet. Back up now to make the first.", style = CrundiType.Small)
            for (it in list.orEmpty()) CrundiCard(Modifier.fillMaxWidth(), padding = 12.dp) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(DateTimeFormatter.ofPattern("d MMM yyyy, HH:mm", Locale.UK).format(Instant.ofEpochMilli(it.at).atZone(ZoneId.systemDefault())), style = CrundiType.Body)
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                            StatusBadge(if (it.reason == "scheduled") "Scheduled" else "By hand", BadgeKind.Neutral)
                            Text(Fmt.bytes(it.size) + if (it.host.isNotEmpty()) " · ${it.host}" else "", style = CrundiType.Meta)
                        }
                    }
                    CrundiButton("Restore", { restoreKey = it.key; restorePass = "" }, icon = "download", enabled = state.busy != "restore" && d.running == null)
                    mv.obitech.crundi.ui.components.CrundiIconButton("trash", { deleteKey = it.key }, contentDescription = "Delete this backup")
                }
                if (restoreKey == it.key) Column(
                    Modifier.padding(top = 10.dp).fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgTertiary).border(Dimens.Hairline, CrundiColors.Yellow, Dimens.ShapeSm).padding(10.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Text("This replaces this Crundi's settings, sign-in, projects, layouts, kanban, notes, mindmap, media, schedules and services with the backup's, and adds back its Claude transcripts and any global skills that are not already here. What it replaces is kept in a pre-restore folder. Crundi restarts to finish.", style = CrundiType.Small.copy(color = CrundiColors.TextPrimary))
                    CrundiTextField(restorePass, { restorePass = it }, Modifier.fillMaxWidth(), placeholder = "Passphrase (blank: the saved one)", icon = "key", password = true)
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        CrundiButton("Restore and restart", { events.onRestore(it.key, restorePass); restoreKey = ""; restorePass = "" }, kind = ButtonKind.Danger, icon = "download", enabled = state.busy != "restore")
                        CrundiButton("Cancel", { restoreKey = "" })
                    }
                }
            }
        }
    }
    deleteKey?.let { k ->
        CrundiDialog("Delete this backup?", { deleteKey = null }, message = "It is removed from the storage. This cannot be undone.", confirm = "Delete", danger = true, onConfirm = { deleteKey = null; events.onDelete(k) })
    }
}

@Composable
private fun Field(label: String) = Text(label, style = CrundiType.Small.copy(color = CrundiColors.TextSecondary))

/** What is happening now: a backup or restore under way, one waiting, a staged restore, the last and the next. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun StatusCard(d: BackupStatus, state: BackupState, events: BackupEvents) {
    CrundiCard(Modifier.fillMaxWidth(), padding = 14.dp) {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            d.running?.let { r ->
                Line(if (r.reason == "restore") "Restoring" else "Backing up", BackupRules.phase(r), null)
                val p = BackupRules.progress(r)
                Meter(p ?: 0.15f)
            }
            if (d.running == null && d.waiting != null) {
                Line("A scheduled backup is due.", "It starts once you have left Crundi.", null)
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    CrundiButton("Back up now", events.onRun, kind = ButtonKind.Primary, icon = "upload", enabled = state.busy != "run")
                    CrundiButton("Skip this one", events.onSkip, enabled = state.busy != "skip")
                }
            }
            if (d.staged) {
                Line("A restore is ready.", "It finishes when Crundi restarts.", null)
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    if (d.restartMode.isNotEmpty()) CrundiButton("Restart now", events.onRestart, kind = ButtonKind.Primary, icon = "rotate-ccw", enabled = state.busy != "restart")
                    else Text("Restart Crundi the way you started it.", style = CrundiType.Small)
                    CrundiButton("Cancel the restore", events.onCancelRestore, enabled = state.busy != "cancel")
                }
            }
            d.restoreAttempt?.takeIf { !it.ok && d.running == null }?.let { Line("Restore", it.error, false) }
            val l = d.last
            when {
                l != null && l.ok -> Line("Last backup", BackupRules.whenText(l.at) + " · " + Fmt.bytes(l.size) + ", ${l.files} files, " + (if (l.reason == "scheduled") "scheduled" else "made by hand"), true)
                l != null -> Line("Last backup", "Failed " + BackupRules.whenText(l.at) + (if (l.error.isNotEmpty()) ": ${l.error}" else ""), false)
                d.configured -> Line("Last backup", "None yet", null)
            }
            Line("Next backup", d.nextAt?.takeIf { it > 0 }?.let { BackupRules.whenText(it) + if (d.timezone.isNotEmpty()) " (${d.timezone} time)" else "" }
                ?: if (d.configured) "Scheduled backups are off" else "Set up the storage and a passphrase below", null)
            d.restored?.takeIf { System.currentTimeMillis() - it.at < 7 * 86_400_000L }?.let { r ->
                Line("Restored", BackupRules.whenText(r.at) + " from the backup made " + BackupRules.whenText(Fmt.iso(r.backupCreatedAt)) + (if (r.fromHost.isNotEmpty()) " on ${r.fromHost}" else "") + ". What it replaced is in ${r.safetyCopy}.", null)
            }
            if (d.running == null && d.waiting == null) CrundiButton("Back up now", events.onRun, kind = ButtonKind.Primary, icon = "upload", enabled = d.configured && state.busy != "run")
        }
    }
}

@Composable
private fun Line(key: String, text: String, good: Boolean?) {
    Column {
        Text(key, style = CrundiType.Meta)
        Text(text, style = CrundiType.Body.copy(color = when (good) { true -> CrundiColors.Green; false -> CrundiColors.Red; null -> CrundiColors.TextPrimary }))
    }
}
