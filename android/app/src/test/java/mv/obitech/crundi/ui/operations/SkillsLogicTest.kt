package mv.obitech.crundi.ui.operations

import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.ApiResponse
import mv.obitech.crundi.api.CrundiClient
import mv.obitech.crundi.api.FakeTransport
import mv.obitech.crundi.api.model.Skill
import mv.obitech.crundi.screens.FakeApi
import mv.obitech.crundi.ui.components.ErrorWords
import mv.obitech.crundi.ui.components.errorOrUnsupported
import mv.obitech.crundi.ui.components.routeMissing
import mv.obitech.crundi.ui.workspace.PickedFile
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Settings > Skills against a fake server, held to the web's sk* functions
 * (src/webapp-html.js) and the routes in src/webapp.js.
 */
class SkillsLogicTest {
    private fun server(): FakeTransport = FakeTransport()
        .on("GET /api/skills", FakeApi.text("skills.json"))
        .on("GET /api/skills/detail", FakeApi.text("skills-detail.json"))
        .on("GET /api/skills/file", FakeApi.text("skills-file.json"))

    private fun controller(t: FakeTransport) = SkillsController(CrundiClient(t).skills)
    private fun body(t: FakeTransport, route: String): JsonObject = t.requests.last { "${it.method} ${it.path}" == route }.json as JsonObject
    private fun JsonObject.s(k: String) = (this[k] as JsonPrimitive).content
    private fun JsonObject.b(k: String) = (this[k] as JsonPrimitive).booleanOrNull
    private fun file(name: String, bytes: ByteArray = "zip".toByteArray()) = PickedFile(name, bytes.size.toLong()) { bytes.inputStream() }

    @Test fun listsEveryScope() = runTest {
        val c = controller(server())
        c.load()
        val d = c.ui.list!!
        assertEquals(7, d.skills.size)
        assertEquals(listOf("global", "project:crundi", "project:ledger"), d.scopes.map { it.id })
        assertEquals(52428800L, d.limits.upload)
        assertEquals("All projects", c.ui.scopeLabel("global"))
        assertEquals("your Claude account", c.ui.scopeLabel("synced"))
        assertFalse(c.ui.unsupported)
    }

    @Test fun anOlderServerSaysItNeedsAnUpdate() = runTest {
        // No /api/skills at all: the router's own 404.
        val c = controller(FakeTransport())
        c.load()
        assertTrue(c.ui.unsupported)
        assertEquals(ErrorWords.NOT_SUPPORTED, (c.ui.data as Load.Failed).message)
        assertEquals("This server does not support this yet. Update the server to use it.", ErrorWords.NOT_SUPPORTED)
    }

    @Test fun aMissingRouteIsNotAMissingFile() {
        assertTrue(routeMissing(ApiException(404, "Not found")))
        assertTrue(routeMissing(ApiException(404, "Not found.")))
        assertTrue(routeMissing(ApiException(405, "Not Found")))
        assertFalse(routeMissing(ApiException(404, "File not found")))
        assertFalse(routeMissing(ApiException(400, "Skill not found")))
        assertFalse(routeMissing(ApiException(403, "Not available")))
        assertEquals("File not found", errorOrUnsupported(ApiException(404, "File not found")))
    }

    @Test fun anActionOnAnOlderServerSaysSoAndTheRestKeepsWorking() = runTest {
        val c = controller(server())
        c.load()
        c.open(SkillRef("global", "frontend-design"))
        c.rename("web-design")
        assertEquals(Actions.Note(ErrorWords.NOT_SUPPORTED, true), c.ui.note)
        // Still open, still listed.
        assertEquals(SkillRef("global", "frontend-design"), c.ui.open)
        assertEquals(7, c.ui.list!!.skills.size)
    }

    @Test fun theServersOwnErrorIsShown() = runTest {
        val t = server().on("POST /api/skills/rename", """{"ok":false,"error":"A skill called \"run\" already exists there","conflict":true,"existing":["run"]}""", 409)
        val c = controller(t)
        c.load()
        c.open(SkillRef("global", "frontend-design"))
        c.rename("run")
        assertEquals(Actions.Note("A skill called \"run\" already exists there", true), c.ui.note)
    }

    @Test fun openEditAndSave() = runTest {
        val t = server().on("POST /api/skills/file", """{"ok":true,"path":"SKILL.md","size":12,"created":false}""")
        val c = controller(t)
        c.load()
        c.open(SkillRef("global", "frontend-design"))
        assertEquals(listOf("SKILL.md", "reference/typography.md", "scripts/contrast.py"), c.ui.detail!!.valueOrNull!!.files.map { it.path })
        c.openFile("SKILL.md")
        val f = c.ui.file!!
        assertTrue(f.saved.startsWith("---\nname: frontend-design"))
        assertFalse(f.dirty)
        c.setDraft("changed text")
        assertTrue(c.ui.file!!.dirty)
        c.save()
        val b = body(t, "POST /api/skills/file")
        assertEquals("global", b.s("scope")); assertEquals("frontend-design", b.s("name")); assertEquals("SKILL.md", b.s("path")); assertEquals("changed text", b.s("content"))
        assertFalse(c.ui.file!!.dirty)
        assertEquals(12L, c.ui.file!!.size)
        assertEquals(Actions.Note("Saved SKILL.md", false), c.ui.note)
    }

    @Test fun aReadOnlyFileIsNeverDirty() {
        assertFalse(SkillEdit("SKILL.md", "a", "b", readOnly = true).dirty)
        assertFalse(SkillEdit("logo.png", "", "x", binary = true).dirty)
        assertTrue(SkillEdit("SKILL.md", "a", "b").dirty)
    }

    @Test fun uploadSendsTheBytesRaw() = runTest {
        val t = server().on("POST /api/skills/upload", """{"ok":true,"installed":[{"scope":"project:crundi","name":"android-release","files":2,"origin":"user"}],"dropped":[]}""")
        val c = controller(t)
        c.load()
        c.setTarget("project:crundi")
        val bytes = byteArrayOf(0x50, 0x4b, 3, 4, 0, 1, 2)
        c.upload(file("deploy.zip", bytes))
        val r = t.requests.last { it.path == "/api/skills/upload" }
        assertEquals("application/octet-stream", r.upload!!.contentType)
        assertEquals("project:crundi", r.query["scope"])
        assertEquals("deploy.zip", r.query["filename"])
        assertNull(r.query["overwrite"])
        assertArrayEquals(bytes, t.lastUpload)
        assertEquals(Actions.Note("Installed android-release. New chats will have it.", false), c.ui.note)
        // One skill installed: it opens, as on the web.
        assertEquals(SkillRef("project:crundi", "android-release"), c.ui.open)
    }

    @Test fun uploadThatClashesAsksToReplace() = runTest {
        var calls = 0
        val t = server().on("POST /api/skills/upload") { r ->
            calls++
            if (r.query["overwrite"] == "1") ApiResponse(200, """{"ok":true,"installed":[{"scope":"global","name":"a"},{"scope":"global","name":"b"}],"dropped":["notes.txt"]}""")
            else ApiResponse(409, """{"ok":false,"conflict":true,"existing":["a","b"],"error":"These skills already exist there: a, b"}""")
        }
        val c = controller(t)
        c.load()
        c.upload(file("pack.zip"))
        val p = c.ui.pending!!
        assertTrue(p.upload)
        assertEquals(listOf("a", "b"), p.existing)
        assertEquals("", c.ui.error)
        c.replacePending()
        assertEquals(2, calls)
        assertNull(c.ui.pending)
        assertEquals(Actions.Note("Installed 2 skills. New chats will have them.", false), c.ui.note)
        assertEquals("Left out of the upload: notes.txt", c.ui.error)
        // Several skills: nothing opens.
        assertNull(c.ui.open)
    }

    @Test fun uploadWithoutANameAsksForOne() = runTest {
        val t = server().on("POST /api/skills/upload") { r ->
            if (r.query["name"] == "my-skill") ApiResponse(200, """{"ok":true,"installed":[{"scope":"global","name":"my-skill"}],"dropped":[]}""")
            else ApiResponse(400, """{"ok":false,"needsName":true,"error":"Could not tell what to call this skill. Give it a name."}""")
        }
        val c = controller(t)
        c.load()
        c.upload(file("SKILL.md", "---\nname:\n---\n".toByteArray()))
        assertTrue(c.ui.pending!!.needsName)
        c.namePending(" my-skill ")
        assertEquals("my-skill", t.requests.last { it.path == "/api/skills/upload" }.query["name"])
        assertNull(c.ui.pending)
        assertEquals(Actions.Note("Installed my-skill. New chats will have it.", false), c.ui.note)
    }

    @Test fun uploadTooLargeIsStoppedBeforeSending() = runTest {
        val t = server()
        val c = controller(t)
        c.load()
        c.upload(PickedFile("huge.zip", 60L * 1048576) { ByteArray(0).inputStream() })
        assertEquals("That file is too large (the limit is 50 MB)", c.ui.error)
        assertTrue(t.requests.none { it.path == "/api/skills/upload" })
    }

    @Test fun createOpensTheNewSkillsSkillMd() = runTest {
        val t = server().on("POST /api/skills/create", """{"ok":true,"skill":{"scope":"global","name":"frontend-design","origin":"user"}}""")
        val c = controller(t)
        c.load()
        c.form(SkillForm.New)
        c.create(" frontend-design ", "Design things")
        val b = body(t, "POST /api/skills/create")
        assertEquals("global", b.s("scope")); assertEquals("frontend-design", b.s("name")); assertEquals("Design things", b.s("description"))
        assertEquals(SkillForm.None, c.ui.form)
        assertEquals("SKILL.md", c.ui.file!!.path)
    }

    @Test fun copyThatClashesAsksToReplace() = runTest {
        val t = server().on("POST /api/skills/transfer") { r ->
            val o = r.json as JsonObject
            if ((o["overwrite"] as JsonPrimitive).booleanOrNull == true) ApiResponse(200, """{"ok":true,"skill":{"scope":"project:crundi","name":"crundi","origin":"user"}}""")
            else ApiResponse(409, """{"ok":false,"conflict":true,"existing":["crundi"],"error":"A skill called \"crundi\" already exists there"}""")
        }
        val c = controller(t)
        c.load()
        c.open(SkillRef("global", "crundi"))
        assertEquals("project:crundi", c.ui.transferDefault())
        c.transfer(c.ui.open!!, "project:crundi", "crundi", move = false)
        assertEquals(listOf("crundi"), c.ui.pending!!.existing)
        assertFalse(c.ui.pending!!.upload)
        c.replacePending()
        val b = body(t, "POST /api/skills/transfer")
        assertEquals("global", b.s("scope")); assertEquals("crundi", b.s("name")); assertEquals("project:crundi", b.s("to"))
        assertEquals(false, b.b("move")); assertEquals(true, b.b("overwrite"))
        assertNull(c.ui.pending)
        assertEquals(Actions.Note("Copied to crundi", false), c.ui.note)
    }

    @Test fun transferDefaultIsElsewhere() = runTest {
        val c = controller(server())
        c.load()
        c.open(SkillRef("project:crundi", "android-release"))
        assertEquals("global", c.ui.transferDefault())
    }

    @Test fun deleteClosesTheSkill() = runTest {
        val t = server().on("POST /api/skills/delete", """{"ok":true}""")
        val c = controller(t)
        c.load()
        c.open(SkillRef("global", "frontend-design"))
        c.delete()
        val b = body(t, "POST /api/skills/delete")
        assertEquals("global", b.s("scope")); assertEquals("frontend-design", b.s("name"))
        assertNull(c.ui.open)
        assertEquals(Actions.Note("Deleted frontend-design", false), c.ui.note)
    }

    @Test fun newFileAndDeleteFile() = runTest {
        val t = server()
            .on("POST /api/skills/file", """{"ok":true,"path":"reference/notes.md","size":0,"created":true}""")
            .on("POST /api/skills/file-delete", """{"ok":true}""")
        val c = controller(t)
        c.load()
        c.open(SkillRef("global", "frontend-design"))
        c.newFile("reference/typography.md")
        assertEquals(Actions.Note("That file already exists", true), c.ui.note)
        c.newFile("reference/notes.md")
        assertEquals("", body(t, "POST /api/skills/file").s("content"))
        c.deleteFile("scripts/contrast.py")
        assertEquals("scripts/contrast.py", body(t, "POST /api/skills/file-delete").s("path"))
    }

    @Test fun uploadAFileIntoTheSkill() = runTest {
        val t = server().on("POST /api/skills/file-upload", """{"ok":true,"path":"logo.png","size":3}""")
        val c = controller(t)
        c.load()
        c.open(SkillRef("global", "frontend-design"))
        c.uploadAsset(file("logo.png", byteArrayOf(1, 2, 3)))
        val r = t.requests.last { it.path == "/api/skills/file-upload" }
        assertEquals(mapOf("scope" to "global", "name" to "frontend-design", "path" to "logo.png"), r.query)
        assertEquals("application/octet-stream", r.upload!!.contentType)
        assertEquals(Actions.Note("Added logo.png", false), c.ui.note)
    }

    @Test fun uploadingAFileTheSkillHasAsksFirst() = runTest {
        val t = server().on("POST /api/skills/file-upload", """{"ok":true,"path":"SKILL.md","size":3}""")
        val c = controller(t)
        c.load()
        c.open(SkillRef("global", "frontend-design"))
        // SKILL.md is already there: nothing goes up until Replace.
        val same = file("SKILL.md", byteArrayOf(1, 2, 3))
        c.pickAsset(same)
        assertEquals(same, c.ui.replaceAsset)
        assertTrue(t.requests.none { it.path == "/api/skills/file-upload" })
        // Cancel: still nothing sent.
        c.cancelAsset()
        assertNull(c.ui.replaceAsset)
        assertTrue(t.requests.none { it.path == "/api/skills/file-upload" })
        // Asked again and confirmed: it goes up over the old one.
        c.pickAsset(same)
        c.confirmAsset()
        assertNull(c.ui.replaceAsset)
        assertEquals("SKILL.md", t.requests.single { it.path == "/api/skills/file-upload" }.query["path"])
        assertEquals(Actions.Note("Added SKILL.md", false), c.ui.note)
    }

    @Test fun uploadingANewFileDoesNotAsk() = runTest {
        val t = server().on("POST /api/skills/file-upload", """{"ok":true,"path":"logo.png","size":3}""")
        val c = controller(t)
        c.load()
        c.open(SkillRef("global", "frontend-design"))
        c.pickAsset(file("logo.png", byteArrayOf(1, 2, 3)))
        assertNull(c.ui.replaceAsset)
        assertEquals(1, t.requests.count { it.path == "/api/skills/file-upload" })
    }

    @Test fun tagsAndRulesLikeTheWeb() {
        assertEquals(listOf("Included with Crundi"), Skills.tags(Skill(origin = "bundled", readOnly = true)).map { it.first })
        assertEquals(listOf("Synced"), Skills.tags(Skill(origin = "synced")).map { it.first })
        assertEquals(listOf("Link", "Needs attention"), Skills.tags(Skill(origin = "linked", problem = "No SKILL.md")).map { it.first })
        assertTrue(Skills.deletable(Skill(origin = "user")))
        assertTrue(Skills.deletable(Skill(origin = "linked")))
        assertFalse(Skills.deletable(Skill(origin = "bundled")))
        assertFalse(Skills.deletable(Skill(origin = "synced")))
        assertEquals("1 file, 212 B", Skills.meta(Skill(files = 1, size = 212)))
        assertEquals("over 2000 files", Skills.meta(Skill(files = 2000, more = true)))
        assertEquals("Upload a .zip or .skill archive holding one or more skills, or a single SKILL.md. Up to 50 MB.", Skills.uploadHint(52428800))
    }
}
