package mv.obitech.crundi.ui.operations

import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
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
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import mv.obitech.crundi.Downloads
import mv.obitech.crundi.Pickers
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.SkillsApi
import mv.obitech.crundi.api.model.Skill
import mv.obitech.crundi.api.model.SkillDetail
import mv.obitech.crundi.api.model.SkillList
import mv.obitech.crundi.api.model.SkillScope
import mv.obitech.crundi.api.str
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.Banner
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiDivider
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.ErrorWords
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.StatusBadge
import mv.obitech.crundi.ui.components.TopBar
import mv.obitech.crundi.ui.components.errorOrUnsupported
import mv.obitech.crundi.ui.components.routeMissing
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens
import mv.obitech.crundi.ui.workspace.PickedFile
import mv.obitech.crundi.ui.workspace.pickedFile
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull

// ─── Skills ───
//
// The web's Settings > Skills (renderSkillsSection and the sk* functions in
// src/webapp-html.js), on /api/skills. What the web shows inline under a row
// is a screen of its own here: the list, one skill (its files and actions),
// and one file in the editor. The words and the rules are the web's: the
// kinds that are not the user's own (shipped with Crundi, synced from the
// Claude account, a link) are read-only, and only the user's own and links
// can be deleted.

/** Which skill: its scope ("global", "project:<alias>", "synced") and name. */
data class SkillRef(val scope: String, val name: String)

/** The inline form that is open (the web's skForm). */
enum class SkillForm { None, New, Rename, Transfer, NewFile }

/** A file in the editor. [draft] is what is in the box; [saved] what the server has. */
data class SkillEdit(
    val path: String,
    val saved: String = "",
    val draft: String = saved,
    val binary: Boolean = false,
    val tooLarge: Boolean = false,
    val readOnly: Boolean = false,
    val size: Long = 0,
) {
    val dirty: Boolean get() = !binary && !readOnly && draft != saved
}

/**
 * Something waiting on an answer (the web's skPending): the upload needs a
 * name, or what it would put somewhere is already there and replacing it
 * removes every file in it first.
 */
data class SkillPending(
    val upload: Boolean,
    val scope: String,
    val existing: List<String> = emptyList(),
    val needsName: Boolean = false,
    val file: PickedFile? = null,
    val name: String = "",
    val from: SkillRef? = null,
    val asName: String = "",
    val move: Boolean = false,
)

data class SkillsUi(
    val data: Load<SkillList> = Load.Loading,
    /** The server has no /api/skills: it is older than this app. */
    val unsupported: Boolean = false,
    /** Where Upload and New put a skill. */
    val target: String = "global",
    val showSynced: Boolean = false,
    /** Said on the list in red (what an upload left out, why it failed). */
    val error: String = "",
    val form: SkillForm = SkillForm.None,
    val pending: SkillPending? = null,
    val open: SkillRef? = null,
    /** The open skill's files; null while they come. */
    val detail: Load<SkillDetail>? = null,
    val file: SkillEdit? = null,
    /** A file picked for Upload file whose name the skill already has: waiting on "Replace?". */
    val replaceAsset: PickedFile? = null,
    val busy: String? = null,
    val note: Actions.Note? = null,
) {
    val list: SkillList? get() = data.valueOrNull
    val openSkill: Skill? get() = open?.let { o -> list?.skills?.firstOrNull { it.scope == o.scope && it.name == o.name } }

    /** The web's skScopeLabel. */
    fun scopeLabel(id: String): String = if (id == "synced") "your Claude account" else list?.scopes?.firstOrNull { it.id == id }?.label ?: if (id == "global") "All projects" else id

    /** Where Copy or move points first: elsewhere than where the skill is (the web's drawSkills). */
    fun transferDefault(): String {
        val o = open ?: return "global"
        val other = list?.scopes?.firstOrNull { it.id != o.scope }
        return if (o.scope == "global" || o.scope == "synced") other?.id ?: "global" else "global"
    }
}

object Skills {
    const val INTRO = "A skill is a folder with a SKILL.md that tells Claude how to do one kind of task. A chat loads its skills when it starts, so changes here reach new chats. Chats that are already running keep the skills they started with."
    fun uploadHint(limit: Long) = "Upload a .zip or .skill archive holding one or more skills, or a single SKILL.md. Up to ${limit / 1048576} MB."

    /** The tags after a skill's name (the web's skTags). */
    fun tags(s: Skill): List<Pair<String, BadgeKind>> = buildList {
        when (s.origin) {
            "bundled" -> add("Included with Crundi" to BadgeKind.Accent)
            "synced" -> add("Synced" to BadgeKind.Info)
            "linked" -> add("Link" to BadgeKind.Neutral)
        }
        if (s.problem.isNotEmpty()) add("Needs attention" to BadgeKind.Warn)
    }

    fun meta(s: Skill): String = if (s.more) "over ${s.files} files" else "${s.files} ${if (s.files == 1) "file" else "files"}, ${Fmt.bytes(s.size)}"

    /** Only the user's own skills and links can be deleted (a link loses the link, never what it points at). */
    fun deletable(s: Skill) = s.origin == "user" || s.origin == "linked"
}

/**
 * The section's state and what it does, as the web's sk* functions. Every
 * action is a suspend function so a test can run it against a fake server;
 * the screen launches them.
 */
class SkillsController(private val api: SkillsApi) {
    var ui by mutableStateOf(SkillsUi())

    private fun said(text: String, error: Boolean = false) { ui = ui.copy(note = Actions.Note(text, error)) }
    private fun failed(e: Throwable) = said(errorOrUnsupported(e), true)

    private suspend fun busy(key: String, block: suspend () -> Unit) {
        ui = ui.copy(busy = key)
        try { block() } catch (e: ApiException) { failed(e) } finally { if (ui.busy == key) ui = ui.copy(busy = null) }
    }

    fun clearNote() { ui = ui.copy(note = null) }
    fun setTarget(id: String) { ui = ui.copy(target = id) }
    fun toggleSynced() { ui = ui.copy(showSynced = !ui.showSynced) }
    fun form(f: SkillForm) { ui = ui.copy(form = if (ui.form == f) SkillForm.None else f, pending = if (f == SkillForm.New) null else ui.pending) }
    fun closeForm() { ui = ui.copy(form = SkillForm.None) }
    fun cancelPending() { ui = ui.copy(pending = null) }
    fun setDraft(text: String) { ui.file?.let { ui = ui.copy(file = it.copy(draft = text)) } }
    fun closeFile() { ui = ui.copy(file = null) }
    fun closeSkill() { ui = ui.copy(open = null, detail = null, file = null, form = SkillForm.None) }

    /** The web's skLoad: the list, and the open skill's files again. */
    suspend fun load() {
        try {
            ui = ui.copy(data = Load.Ready(api.list()), unsupported = false, error = "")
        } catch (e: ApiException) {
            if (routeMissing(e)) { ui = ui.copy(unsupported = true, data = Load.Failed(ErrorWords.NOT_SUPPORTED)); return }
            ui = if (ui.data is Load.Ready) ui.copy(error = errorOrUnsupported(e)) else ui.copy(data = Load.Failed(errorOrUnsupported(e)))
        }
        val d = ui.list ?: return
        if (d.scopes.none { it.id == ui.target }) ui = ui.copy(target = "global")
        val o = ui.open
        if (o != null && d.skills.none { it.scope == o.scope && it.name == o.name }) closeSkill()
        if (ui.open != null) loadDetail()
    }

    suspend fun loadDetail() {
        val o = ui.open ?: return
        val d = try { Load.Ready(api.detail(o.scope, o.name)) } catch (e: ApiException) { Load.Failed(errorOrUnsupported(e).ifEmpty { "Could not open this skill" }) }
        val f = ui.file
        ui = ui.copy(detail = d, file = if (f != null && d is Load.Ready && d.value.files.none { it.path == f.path }) null else f)
    }

    suspend fun open(ref: SkillRef) {
        ui = ui.copy(open = ref, detail = null, file = null, form = SkillForm.None)
        loadDetail()
    }

    /** The web's skUpload: one upload, possibly asked twice (again with a name, or again to replace). */
    suspend fun upload(file: PickedFile, scope: String = ui.target, name: String = "", overwrite: Boolean = false) {
        val limit = ui.list?.limits?.upload
        if (limit != null && file.size > limit) { ui = ui.copy(error = "That file is too large (the limit is ${limit / 1048576} MB)"); return }
        ui = ui.copy(form = SkillForm.None, error = "", busy = "upload")
        try {
            val r = api.upload(scope, file.name, file.size, name, overwrite, open = file.open)
            val n = r.installed.map { it.name }
            said("Installed " + (if (n.size == 1) n[0] else "${n.size} skills") + ". New chats will have " + (if (n.size == 1) "it." else "them."))
            val keep = if (r.dropped.isNotEmpty()) "Left out of the upload: " + r.dropped.joinToString(", ") else ""
            ui = ui.copy(pending = null)
            if (n.size == 1) ui = ui.copy(open = SkillRef(r.installed[0].scope, n[0]), detail = null, file = null)
            load()
            if (keep.isNotEmpty()) ui = ui.copy(error = keep)
        } catch (e: ApiException) {
            val body = e.body
            ui = when {
                body?.get("conflict").let { (it as? JsonPrimitive)?.booleanOrNull == true } ->
                    ui.copy(pending = SkillPending(upload = true, scope = scope, existing = existing(body), file = file, name = name), error = "")
                body?.get("needsName").let { (it as? JsonPrimitive)?.booleanOrNull == true } ->
                    ui.copy(pending = SkillPending(upload = true, scope = scope, needsName = true, file = file), error = "")
                else -> ui.copy(pending = null, error = errorOrUnsupported(e).ifEmpty { "Could not install that" })
            }
        } finally { if (ui.busy == "upload") ui = ui.copy(busy = null) }
    }

    private fun existing(body: kotlinx.serialization.json.JsonObject?): List<String> =
        (body?.get("existing") as? JsonArray)?.mapNotNull { it.str() }.orEmpty()

    /** The web's skTransfer: copy, or move, to another scope. */
    suspend fun transfer(from: SkillRef, to: String, asName: String, move: Boolean, overwrite: Boolean = false) {
        ui = ui.copy(busy = "transfer")
        try {
            val s = api.transfer(from.scope, from.name, to, asName, move, overwrite)
            ui = ui.copy(pending = null, form = SkillForm.None, file = null, open = SkillRef(s.scope, s.name), detail = null)
            said((if (move) "Moved to " else "Copied to ") + ui.scopeLabel(s.scope))
            load()
        } catch (e: ApiException) {
            val body = e.body
            if ((body?.get("conflict") as? JsonPrimitive)?.booleanOrNull == true) {
                ui = ui.copy(pending = SkillPending(upload = false, scope = to, existing = existing(body), from = from, asName = asName, move = move))
            } else {
                ui = ui.copy(pending = null); failed(e)
            }
        } finally { if (ui.busy == "transfer") ui = ui.copy(busy = null) }
    }

    /** Replace: the waiting upload or copy again, with overwrite. */
    suspend fun replacePending() {
        val p = ui.pending ?: return
        if (p.upload) p.file?.let { upload(it, p.scope, p.name, overwrite = true) }
        else p.from?.let { transfer(it, p.scope, p.asName, p.move, overwrite = true) }
    }

    /** The upload again, with the name the person gave it. */
    suspend fun namePending(name: String) {
        val p = ui.pending ?: return
        p.file?.let { upload(it, p.scope, name.trim()) }
    }

    suspend fun create(name: String, description: String) = busy("create") {
        val s = api.create(ui.target, name.trim(), description.trim())
        ui = ui.copy(form = SkillForm.None, open = SkillRef(s.scope, s.name), detail = null, file = null)
        load()
        openFile("SKILL.md")
    }

    suspend fun delete() {
        val o = ui.open ?: return
        busy("delete") {
            api.delete(o.scope, o.name)
            said("Deleted ${o.name}")
            closeSkill()
            load()
        }
    }

    suspend fun rename(to: String) {
        val o = ui.open ?: return
        busy("rename") {
            val s = api.rename(o.scope, o.name, to.trim())
            ui = ui.copy(form = SkillForm.None, file = null, open = SkillRef(s.scope, s.name))
            load()
        }
    }

    /** The web's skOpenFile. */
    suspend fun openFile(path: String) {
        val o = ui.open ?: return
        busy("file") {
            val f = api.file(o.scope, o.name, path)
            ui = ui.copy(file = SkillEdit(f.path, f.text, f.text, f.binary, f.tooLarge, f.readOnly, f.size))
        }
    }

    suspend fun save() {
        val o = ui.open ?: return
        val f = ui.file ?: return
        val text = f.draft
        busy("save") {
            val r = api.write(o.scope, o.name, f.path, text)
            ui.file?.takeIf { it.path == f.path }?.let { ui = ui.copy(file = it.copy(saved = text, size = r.size)) }
            said("Saved ${f.path}")
            load()
        }
    }

    suspend fun newFile(path: String) {
        val o = ui.open ?: return
        val p = path.trim()
        if (p.isEmpty()) return
        if (ui.detail?.valueOrNull?.files?.any { it.path == p } == true) { said("That file already exists", true); return }
        busy("newfile") {
            val r = api.write(o.scope, o.name, p, "")
            ui = ui.copy(form = SkillForm.None)
            load()
            openFile(r.path.ifEmpty { p })
        }
    }

    suspend fun deleteFile(path: String) {
        val o = ui.open ?: return
        busy("file-delete") {
            api.deleteFile(o.scope, o.name, path)
            if (ui.file?.path == path) ui = ui.copy(file = null)
            load()
        }
    }

    /**
     * A file picked for Upload file. One the skill already has asks first, as
     * the web's confirm('Replace <name> in this skill?'); anything else goes up.
     */
    suspend fun pickAsset(file: PickedFile) {
        if (ui.detail?.valueOrNull?.files?.any { it.path == file.name } == true) ui = ui.copy(replaceAsset = file)
        else uploadAsset(file)
    }

    /** Replace was confirmed: upload the waiting file over the one there. */
    suspend fun confirmAsset() {
        val f = ui.replaceAsset ?: return
        ui = ui.copy(replaceAsset = null)
        uploadAsset(f)
    }

    fun cancelAsset() { ui = ui.copy(replaceAsset = null) }

    /** Add a file from the phone to the open skill, under its own name (the web's Upload file). */
    suspend fun uploadAsset(file: PickedFile) {
        val o = ui.open ?: return
        busy("asset") {
            api.uploadFile(o.scope, o.name, file.name, file.size, open = file.open)
            said("Added ${file.name}")
            load()
        }
    }

    /** Says how a download went (the screen does the saving: it needs the phone). */
    fun downloaded(what: String?, error: Throwable?) {
        if (error != null) said(errorOrUnsupported(error).let { if (it == ErrorWords.NOT_SUPPORTED) it else "Could not download: $it" }, true)
        else if (what != null) said(what)
    }
}

// ─── The route ───

@Composable
fun SkillsRoute() {
    val app = LocalAppServices.current
    val nav = LocalNavigator.current
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val c = remember { SkillsController(app.api.skills) }
    LaunchedEffect(Unit) { c.load() }
    val ui = c.ui
    var pickFor by remember { mutableStateOf("") }
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
        Pickers.active = false
        val what = pickFor
        if (uri != null) {
            val f = pickedFile(ctx, uri)
            scope.launch { if (what == "asset") c.pickAsset(f) else c.upload(f) }
        }
    }
    val pick = { what: String -> pickFor = what; Pickers.active = true; picker.launch("*/*") }
    fun save(name: String, fetch: suspend (java.io.OutputStream) -> mv.obitech.crundi.api.DownloadInfo) {
        scope.launch {
            try { val s = Downloads.save(ctx, name) { sink -> fetch(sink) }; c.downloaded("Saved ${s.name} to ${s.where}", null) }
            catch (e: ApiException) { c.downloaded(null, e) }
            catch (e: java.io.IOException) { c.downloaded(null, e) }
        }
    }
    val back: () -> Unit = { when { ui.file != null -> c.closeFile(); ui.open != null -> c.closeSkill(); else -> { nav.back() } } }
    BackHandler(enabled = ui.open != null && (ui.file == null || !ui.file.dirty)) { back() }
    SkillsScreen(
        ui,
        SkillsEvents(
            onBack = back,
            onCloseNote = c::clearNote,
            onRetry = { scope.launch { c.load() } },
            onTarget = c::setTarget,
            onUpload = { pick("skill") },
            onForm = c::form,
            onCloseForm = c::closeForm,
            onCreate = { n, d -> scope.launch { c.create(n, d) } },
            onReplace = { scope.launch { c.replacePending() } },
            onPendingName = { n -> scope.launch { c.namePending(n) } },
            onCancelPending = c::cancelPending,
            onToggleSynced = c::toggleSynced,
            onOpen = { r -> scope.launch { c.open(r) } },
            onRename = { to -> scope.launch { c.rename(to) } },
            onTransfer = { to, asName, move -> ui.open?.let { o -> scope.launch { c.transfer(o, to, asName, move) } } },
            onDownload = { ui.open?.let { o -> save("${o.name}.zip") { sink -> app.api.skills.download(o.scope, o.name, sink) } } },
            onDelete = { scope.launch { c.delete() } },
            onOpenFile = { p -> scope.launch { c.openFile(p) } },
            onDeleteFile = { p -> scope.launch { c.deleteFile(p) } },
            onNewFile = { p -> scope.launch { c.newFile(p) } },
            onUploadFile = { pick("asset") },
            onReplaceAsset = { scope.launch { c.confirmAsset() } },
            onCancelAsset = c::cancelAsset,
            onDraft = c::setDraft,
            onSave = { scope.launch { c.save() } },
            onDownloadFile = { val o = ui.open; val f = ui.file; if (o != null && f != null) save(f.path.substringAfterLast('/')) { sink -> app.api.skills.downloadFile(o.scope, o.name, f.path, sink) } },
            onCloseFile = c::closeFile,
        ),
    )
}

class SkillsEvents(
    val onBack: () -> Unit = {},
    val onCloseNote: () -> Unit = {},
    val onRetry: () -> Unit = {},
    val onTarget: (String) -> Unit = {},
    val onUpload: () -> Unit = {},
    val onForm: (SkillForm) -> Unit = {},
    val onCloseForm: () -> Unit = {},
    val onCreate: (String, String) -> Unit = { _, _ -> },
    val onReplace: () -> Unit = {},
    val onPendingName: (String) -> Unit = {},
    val onCancelPending: () -> Unit = {},
    val onToggleSynced: () -> Unit = {},
    val onOpen: (SkillRef) -> Unit = {},
    val onRename: (String) -> Unit = {},
    val onTransfer: (to: String, asName: String, move: Boolean) -> Unit = { _, _, _ -> },
    val onDownload: () -> Unit = {},
    val onDelete: () -> Unit = {},
    val onOpenFile: (String) -> Unit = {},
    val onDeleteFile: (String) -> Unit = {},
    val onNewFile: (String) -> Unit = {},
    val onUploadFile: () -> Unit = {},
    val onReplaceAsset: () -> Unit = {},
    val onCancelAsset: () -> Unit = {},
    val onDraft: (String) -> Unit = {},
    val onSave: () -> Unit = {},
    val onDownloadFile: () -> Unit = {},
    val onCloseFile: () -> Unit = {},
)

/** The list, one skill, or one file, by what is open. */
@Composable
fun SkillsScreen(ui: SkillsUi, events: SkillsEvents = SkillsEvents()) {
    when {
        ui.open != null && ui.file != null -> SkillFileView(ui, events)
        ui.open != null -> SkillDetailView(ui, events)
        else -> SkillListView(ui, events)
    }
}

// ─── The list ───

@Composable
private fun SkillListView(ui: SkillsUi, events: SkillsEvents) {
    SettingsPageFrame(SettingsPage.Skills, ui.note, events.onCloseNote, events.onBack) {
        val d = ui.data
        when {
            ui.unsupported -> EmptyState("alert-triangle", "Not available on this server", body = ErrorWords.NOT_SUPPORTED)
            d is Load.Loading -> LoadingState()
            d is Load.Failed -> ErrorState(d.message, onRetry = events.onRetry)
            d is Load.Ready -> ScrollBody { SkillList(ui, d.value, events) }
        }
    }
}

@Composable
private fun SkillList(ui: SkillsUi, d: SkillList, events: SkillsEvents) {
    Text(Skills.INTRO, style = CrundiType.Small, modifier = Modifier.padding(horizontal = Dimens.ScreenPad, vertical = 6.dp))
    Group("Add a skill") {
        Text("Add to", style = CrundiType.Small)
        ScopeField(d.scopes, ui.target, events.onTarget)
        @OptIn(ExperimentalLayoutApi::class)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiButton(if (ui.busy == "upload") "Installing…" else "Upload", events.onUpload, kind = ButtonKind.Primary, icon = "upload", enabled = ui.busy != "upload")
            CrundiButton("New", { events.onForm(SkillForm.New) }, icon = "plus")
        }
        Text(Skills.uploadHint(d.limits.upload), style = CrundiType.Small)
        if (ui.error.isNotEmpty()) Text(ui.error, style = CrundiType.Small.copy(color = CrundiColors.Red))
        ui.pending?.let { PendingBox(ui, it, events) }
        if (ui.form == SkillForm.New) NewSkillForm(ui, events)
    }
    val inScope = { id: String -> d.skills.filter { it.scope == id } }
    SkillGroup("All projects", d.scopes.firstOrNull()?.path.orEmpty(), inScope("global"), events)
    for (sc in d.scopes.drop(1)) { val it = inScope(sc.id); if (it.isNotEmpty()) SkillGroup(sc.label, sc.path, it, events) }
    val synced = inScope("synced")
    if (synced.isNotEmpty()) {
        Column(Modifier.fillMaxWidth().padding(horizontal = Dimens.ScreenPad, vertical = 8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("Synced from your Claude account (${synced.size})".uppercase(), style = CrundiType.Section, modifier = Modifier.weight(1f))
                CrundiButton(if (ui.showSynced) "Hide" else "Show", events.onToggleSynced, kind = ButtonKind.Ghost)
            }
            if (ui.showSynced) SkillRows(synced, events)
        }
    }
}

@Composable
private fun SkillGroup(title: String, path: String, items: List<Skill>, events: SkillsEvents) {
    Column(Modifier.fillMaxWidth().padding(horizontal = Dimens.ScreenPad, vertical = 8.dp)) {
        Text(title.uppercase(), style = CrundiType.Section)
        if (path.isNotEmpty()) Text(path, style = CrundiType.Meta, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(bottom = 4.dp))
        if (items.isEmpty()) NoneYet() else SkillRows(items, events)
    }
}

@Composable
private fun NoneYet() {
    Column(
        Modifier.fillMaxWidth().padding(top = 4.dp).background(CrundiColors.BgSecondary, Dimens.Shape).border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape).padding(Dimens.S3),
    ) { Text("None yet.", style = CrundiType.Small) }
}

@Composable
private fun SkillRows(items: List<Skill>, events: SkillsEvents) {
    Column(
        Modifier.fillMaxWidth().padding(top = 4.dp).background(CrundiColors.BgSecondary, Dimens.Shape).border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape),
    ) {
        for ((i, s) in items.withIndex()) {
            if (i > 0) CrundiDivider()
            SkillRow(s) { events.onOpen(SkillRef(s.scope, s.name)) }
        }
    }
}

/** One skill in the list (the web's skRow): name and tags, size, what it does. */
@Composable
fun SkillRow(s: Skill, onClick: () -> Unit) {
    Row(Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = Dimens.S3, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            @OptIn(ExperimentalLayoutApi::class)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(s.name, style = CrundiType.Body.copy(fontFamily = CrundiType.Mono))
                for ((t, k) in Skills.tags(s)) StatusBadge(t, k)
            }
            Text(Skills.meta(s), style = CrundiType.Meta)
            Text(s.description.ifEmpty { s.problem.ifEmpty { "No description" } }, style = CrundiType.Small, maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
        CrundiIcon("chevron-right", size = Dimens.IconSm, tint = CrundiColors.TextMuted)
    }
}

/** A scope, picked from the server's list (the web's select). */
@Composable
private fun ScopeField(scopes: List<SkillScope>, selected: String, onPick: (String) -> Unit) {
    var open by remember { mutableStateOf(false) }
    val label = scopes.firstOrNull { it.id == selected }?.label ?: selected
    Row(
        Modifier.fillMaxWidth().heightIn(min = Dimens.Touch).background(CrundiColors.BgPrimary, Dimens.ShapeSm).border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm)
            .clickable { open = true }.padding(horizontal = 11.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, style = CrundiType.Body, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
        CrundiIcon("chevron-down", size = Dimens.IconSm, tint = CrundiColors.TextMuted)
    }
    if (open) CrundiDialog("Where", { open = false }) { ScopeOptions(scopes, selected) { open = false; onPick(it) } }
}

/** The rows of the scope picker (also drawn in place by the screenshots). */
@Composable
fun ScopeOptions(scopes: List<SkillScope>, selected: String, onPick: (String) -> Unit) {
    Column(Modifier.heightIn(max = 420.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        for (s in scopes) Row(
            Modifier.fillMaxWidth().heightIn(min = Dimens.Touch).background(if (s.id == selected) CrundiColors.AccentDim else CrundiColors.BgSecondary, Dimens.ShapeSm)
                .clickable { onPick(s.id) }.padding(horizontal = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Text(s.label, style = CrundiType.Body.copy(color = if (s.id == selected) CrundiColors.AccentHover else CrundiColors.TextPrimary), modifier = Modifier.weight(1f))
            if (s.id == selected) CrundiIcon("check", size = Dimens.IconSm, tint = CrundiColors.AccentHover)
        }
    }
}

@Composable
private fun NewSkillForm(ui: SkillsUi, events: SkillsEvents) {
    var name by remember { mutableStateOf("") }
    var desc by remember { mutableStateOf("") }
    CrundiDivider()
    Text("A new, empty skill in ${ui.scopeLabel(ui.target)}. The description is how Claude decides when to use it.", style = CrundiType.Small)
    CrundiTextField(name, { name = it.take(64) }, placeholder = "skill-name", mono = true, keyboardOptions = NoCaps)
    CrundiTextField(desc, { desc = it.take(1024) }, placeholder = "What it does and when to use it", singleLine = false, maxLines = 4)
    FormButtons("Create", { events.onCreate(name, desc) }, events.onCloseForm, enabled = ui.busy != "create")
}

/** The upload needs a name, or would replace what is there (the web's skPendingHtml). */
@Composable
private fun PendingBox(ui: SkillsUi, p: SkillPending, events: SkillsEvents) {
    if (p.needsName) {
        var name by remember { mutableStateOf("") }
        CrundiDivider()
        Text("This upload does not say what the skill is called. Give it a name: lowercase letters, digits and hyphens.", style = CrundiType.Small)
        CrundiTextField(name, { name = it.take(64) }, placeholder = "skill-name", mono = true, keyboardOptions = NoCaps)
        FormButtons("Install", { events.onPendingName(name) }, events.onCancelPending, enabled = ui.busy != "upload")
        return
    }
    PendingReplace(ui, p, events)
}

@Composable
private fun PendingReplace(ui: SkillsUi, p: SkillPending, events: SkillsEvents) {
    val many = p.existing.size > 1
    val where = ui.scopeLabel(p.scope)
    val words = (if (many) "These skills already exist in $where: ${p.existing.joinToString(", ")}" else "A skill called ${p.existing.firstOrNull().orEmpty()} already exists in $where") +
        ". Replacing ${if (many) "them" else "it"} removes every file in ${if (many) "them" else "it"} first."
    Column(Modifier.fillMaxWidth().background(BadgeKind.Warn.bg, Dimens.ShapeSm).padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiIcon("info", size = Dimens.IconSm, tint = BadgeKind.Warn.fg)
            Text(words, style = CrundiType.Small.copy(color = CrundiColors.TextPrimary))
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiButton("Replace", events.onReplace, kind = ButtonKind.Danger, enabled = ui.busy == null)
            CrundiButton("Cancel", events.onCancelPending)
        }
    }
}

@Composable
private fun FormButtons(go: String, onGo: () -> Unit, onCancel: () -> Unit, enabled: Boolean = true, extra: (@Composable () -> Unit)? = null) {
    @OptIn(ExperimentalLayoutApi::class)
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        CrundiButton(go, onGo, kind = ButtonKind.Primary, enabled = enabled)
        extra?.invoke()
        CrundiButton("Cancel", onCancel)
    }
}

private val NoCaps = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false)

// ─── One skill ───

@Composable
private fun SkillDetailView(ui: SkillsUi, events: SkillsEvents) {
    val o = ui.open ?: return
    // The list's entry, or the detail's own when the list has not caught up (or failed to load).
    val sk = ui.openSkill ?: ui.detail?.valueOrNull?.skill
    var confirmDelete by remember { mutableStateOf(false) }
    var confirmFile by remember { mutableStateOf<String?>(null) }
    Column(Modifier.fillMaxSize()) {
        TopBar(o.name, subtitle = ui.scopeLabel(o.scope), onBack = events.onBack)
        NoteBanner(ui.note, events.onCloseNote)
        val d = ui.detail
        when {
            d == null -> LoadingState()
            d is Load.Failed -> ErrorState(d.message, onRetry = events.onRetry)
            sk == null -> ErrorState("Could not open this skill", onRetry = events.onRetry)
            d is Load.Ready -> ScrollBody { SkillDetailBody(ui, sk, d.value, events, onDelete = { confirmDelete = true }, onDeleteFile = { confirmFile = it }) }
        }
    }
    if (confirmDelete) CrundiDialog("Delete this skill?", { confirmDelete = false },
        message = "Delete the skill \"${o.name}\" and all its files? This cannot be undone.",
        confirm = "Delete skill", danger = true, onConfirm = { confirmDelete = false; events.onDelete() })
    confirmFile?.let { p ->
        CrundiDialog("Delete this file?", { confirmFile = null },
            message = "Delete $p from this skill?",
            confirm = "Delete file", danger = true, onConfirm = { confirmFile = null; events.onDeleteFile(p) })
    }
    ui.replaceAsset?.let { f ->
        CrundiDialog("Replace this file?", events.onCancelAsset,
            message = "Replace ${f.name} in this skill?",
            confirm = "Replace file", danger = true, onConfirm = events.onReplaceAsset)
    }
}

@Composable
fun SkillDetailBody(ui: SkillsUi, sk: Skill, d: SkillDetail, events: SkillsEvents, onDelete: () -> Unit = events.onDelete, onDeleteFile: (String) -> Unit = events.onDeleteFile) {
    val ro = sk.readOnly
    Column(Modifier.fillMaxWidth().padding(horizontal = Dimens.ScreenPad, vertical = 6.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        @OptIn(ExperimentalLayoutApi::class)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            for ((t, k) in Skills.tags(sk)) StatusBadge(t, k)
            Text(Skills.meta(sk), style = CrundiType.Meta)
        }
        Text(sk.description.ifEmpty { "No description" }, style = CrundiType.Body.copy(color = if (sk.description.isEmpty()) CrundiColors.TextMuted else CrundiColors.TextPrimary))
        if (d.readOnlyReason.isNotEmpty()) Banner(d.readOnlyReason, Modifier.fillMaxWidth(), BadgeKind.Neutral, "lock")
        if (sk.problem.isNotEmpty()) Banner(sk.problem + ".", Modifier.fillMaxWidth(), BadgeKind.Warn, "info")
        @OptIn(ExperimentalLayoutApi::class)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            if (!ro) CrundiButton("Rename", { events.onForm(SkillForm.Rename) }, icon = "pencil")
            CrundiButton(if (ro) "Copy to…" else "Copy or move…", { events.onForm(SkillForm.Transfer) }, icon = "copy")
            CrundiButton("Download", events.onDownload, icon = "download")
            if (Skills.deletable(sk)) CrundiButton("Delete", onDelete, kind = ButtonKind.Danger, icon = "trash", enabled = ui.busy != "delete")
        }
    }
    if (ui.form == SkillForm.Rename) Group("Rename") {
        var to by remember(sk.name) { mutableStateOf(sk.name) }
        Text("New name. Lowercase letters, digits and hyphens.", style = CrundiType.Small)
        CrundiTextField(to, { to = it.take(64) }, mono = true, keyboardOptions = NoCaps)
        FormButtons("Rename", { events.onRename(to) }, events.onCloseForm, enabled = ui.busy != "rename")
    }
    if (ui.form == SkillForm.Transfer) Group(if (ro) "Copy" else "Copy or move") {
        var to by remember(ui.open) { mutableStateOf(ui.transferDefault()) }
        var asName by remember(sk.name) { mutableStateOf(sk.name) }
        Text("Where to, and under what name. A copy leaves this one where it is.", style = CrundiType.Small)
        ScopeField(ui.list?.scopes.orEmpty(), to) { to = it }
        CrundiTextField(asName, { asName = it.take(64) }, mono = true, keyboardOptions = NoCaps)
        FormButtons("Copy", { events.onTransfer(to, asName, false) }, events.onCloseForm, enabled = ui.busy != "transfer") {
            if (!ro) CrundiButton("Move", { events.onTransfer(to, asName, true) }, enabled = ui.busy != "transfer")
        }
    }
    ui.pending?.takeIf { !it.upload }?.let { p -> Column(Modifier.padding(horizontal = Dimens.ScreenPad, vertical = 6.dp)) { PendingReplace(ui, p, events) } }
    Group("Files") {
        Column {
            for ((i, f) in d.files.withIndex()) {
                if (i > 0) CrundiDivider()
                Row(Modifier.fillMaxWidth().heightIn(min = Dimens.Touch).clickable { events.onOpenFile(f.path) }, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    CrundiIcon(if (f.path == "SKILL.md") "file-text" else "file", size = Dimens.IconSm, tint = CrundiColors.TextSecondary)
                    Text(f.path, style = CrundiType.Code, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text(Fmt.bytes(f.size), style = CrundiType.Meta)
                    if (!ro && f.path != "SKILL.md") CrundiIconButton("trash", { onDeleteFile(f.path) }, contentDescription = "Delete ${f.path}", size = Dimens.IconSm, tint = CrundiColors.Red, touch = 36.dp)
                }
            }
        }
        if (d.more) Text("Only the first ${d.files.size} files are listed. This folder holds more.", style = CrundiType.Small)
        if (d.skippedLinks > 0) Text("${d.skippedLinks}" + (if (d.skippedLinks == 1) " link in this folder is" else " links in this folder are") + " not shown.", style = CrundiType.Small)
        if (!ro) {
            @OptIn(ExperimentalLayoutApi::class)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                CrundiButton("New file", { events.onForm(SkillForm.NewFile) }, icon = "plus")
                CrundiButton(if (ui.busy == "asset") "Uploading…" else "Upload file", events.onUploadFile, icon = "upload", enabled = ui.busy != "asset")
            }
            if (ui.form == SkillForm.NewFile) {
                var path by remember { mutableStateOf("") }
                Text("Path inside the skill, for example reference/notes.md", style = CrundiType.Small)
                CrundiTextField(path, { path = it }, placeholder = "reference/notes.md", mono = true, keyboardOptions = NoCaps)
                FormButtons("Create", { events.onNewFile(path) }, events.onCloseForm, enabled = ui.busy != "newfile")
            }
        }
    }
}

// ─── One file ───

@Composable
private fun SkillFileView(ui: SkillsUi, events: SkillsEvents) {
    val o = ui.open ?: return
    val f = ui.file ?: return
    var leaving by remember { mutableStateOf(false) }
    BackHandler(enabled = f.dirty) { leaving = true }
    val close = { if (f.dirty) leaving = true else events.onCloseFile() }
    Column(Modifier.fillMaxSize()) {
        TopBar(f.path, subtitle = "${o.name} · ${Fmt.bytes(f.size)}", onBack = close)
        NoteBanner(ui.note, events.onCloseNote)
        Column(Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState()).padding(Dimens.ScreenPad), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            when {
                f.binary -> Banner((if (f.tooLarge) "This file is too large to open here." else "This is not a text file, so it cannot be edited here.") + " Download it to look at it.", Modifier.fillMaxWidth(), BadgeKind.Neutral, "file")
                f.readOnly -> {
                    ui.detail?.valueOrNull?.readOnlyReason?.takeIf { it.isNotEmpty() }?.let { Banner(it, Modifier.fillMaxWidth(), BadgeKind.Neutral, "lock") }
                    EditorBox { SelectionContainer { Text(f.draft, style = CrundiType.Code) } }
                }
                else -> EditorBox {
                    BasicTextField(
                        f.draft, events.onDraft,
                        textStyle = CrundiType.Code.copy(color = CrundiColors.TextPrimary),
                        cursorBrush = SolidColor(CrundiColors.AccentHover),
                        keyboardOptions = NoCaps,
                        modifier = Modifier.fillMaxWidth().heightIn(min = 240.dp),
                    )
                }
            }
        }
        CrundiDivider()
        @OptIn(ExperimentalLayoutApi::class)
        FlowRow(Modifier.fillMaxWidth().padding(horizontal = Dimens.ScreenPad, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiButton("Close", close)
            CrundiButton("Download", events.onDownloadFile, icon = "download")
            if (!f.binary && !f.readOnly) CrundiButton(if (ui.busy == "save") "Saving…" else "Save", events.onSave, kind = ButtonKind.Primary, icon = "check", enabled = ui.busy != "save")
        }
    }
    if (leaving) CrundiDialog("Discard your changes?", { leaving = false },
        message = "Discard the changes you have not saved?",
        confirm = "Discard changes", danger = true, onConfirm = { leaving = false; events.onCloseFile() })
}

@Composable
private fun EditorBox(content: @Composable () -> Unit) {
    Column(Modifier.fillMaxWidth().background(CrundiColors.BgSecondary, Dimens.ShapeSm).border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm).padding(10.dp)) { content() }
}
