package mv.obitech.crundi.ui.operations

import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import mv.obitech.crundi.api.ApiResponse
import mv.obitech.crundi.api.CrundiClient
import mv.obitech.crundi.api.CrundiJson
import mv.obitech.crundi.api.FakeTransport
import mv.obitech.crundi.api.model.Skill
import mv.obitech.crundi.api.model.SkillSource
import mv.obitech.crundi.screens.FakeApi
import mv.obitech.crundi.ui.components.ErrorWords
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Settings > Skills > Install from link, held to the web's skLinkInstall and
 * skUpdate (src/webapp-html.js), the routes in src/webapp.js and the parsing in
 * src/skills-github.js.
 */
class SkillsLinkTest {
    private fun server(): FakeTransport = FakeTransport()
        .on("GET /api/skills", FakeApi.text("skills.json"))
        .on("GET /api/skills/detail", FakeApi.text("skills-detail.json"))

    private fun controller(t: FakeTransport, supported: Boolean = true) = SkillsController(CrundiClient(t).skills, supported)
    private fun body(t: FakeTransport, route: String): JsonObject = t.requests.last { "${it.method} ${it.path}" == route }.json as JsonObject
    private fun JsonObject.s(k: String) = (this[k] as? JsonPrimitive)?.content
    private fun JsonObject.b(k: String) = (this[k] as? JsonPrimitive)?.booleanOrNull
    private fun sent(t: FakeTransport, route: String) = t.requests.count { "${it.method} ${it.path}" == route }

    // ─── Reading a link ───

    @Test fun readsThePastedNpxLine() {
        val p = SkillLinks.parse("npx skills add https://github.com/latent-spaces/brag --skill brag-slim")!!
        assertEquals("latent-spaces", p.owner); assertEquals("brag", p.repo); assertEquals("brag-slim", p.skill)
        assertEquals("latent-spaces/brag", p.full)
        assertEquals("x", SkillLinks.parse("npx -y skills add https://github.com/o/r --skill=x")!!.skill)
        assertEquals("x", SkillLinks.parse("npx skills@latest add github.com/o/r -s x")!!.skill)
        assertEquals("x", SkillLinks.parse("npx skills add https://github.com/o/r -s=x")!!.skill)
    }

    @Test fun acceptsWhatTheServerAccepts() {
        // Everything parseSource in src/skills-github.js takes, so the phone never
        // turns away a link the web would install.
        val ssh = SkillLinks.parse("git@github.com:latent-spaces/brag.git")!!
        assertEquals("latent-spaces", ssh.owner); assertEquals("brag", ssh.repo)
        assertEquals("brag-slim", SkillLinks.parse("npx skills add git@github.com:latent-spaces/brag --skill brag-slim")!!.skill)
        assertEquals("o/r", SkillLinks.parse("git@github.com:o/r")!!.full)
        assertEquals("x", SkillLinks.parse("npx -y skills add o/r -s=x")!!.skill)
        assertNull(SkillLinks.parse("git@gitlab.com:o/r"))
    }

    @Test fun readsLinksAndTheShorthand() {
        assertEquals("r", SkillLinks.parse("https://github.com/o/r.git")!!.repo)
        assertEquals("o/r", SkillLinks.parse("o/r")!!.full)
        assertEquals("o/r", SkillLinks.parse("https://github.com/o/r/tree/main/skills/pdf")!!.full)
        assertEquals("o/r", SkillLinks.parse("  https://www.github.com/o/r?tab=readme  ")!!.full)
    }

    @Test fun anythingButGitHubIsRefused() {
        assertNull(SkillLinks.parse("https://gitlab.com/o/r"))
        assertNull(SkillLinks.parse("https://evil.example/github.com/o/r"))
        assertNull(SkillLinks.parse("https://github.com.evil.example/o/r"))
        assertNull(SkillLinks.parse("https://github.com/o"))
        assertNull(SkillLinks.parse("npx skills add"))
        assertNull(SkillLinks.parse(""))
    }

    // ─── Which servers have it ───

    @Test fun gatedOnTheCapability() {
        assertEquals("skills-install-link", SkillLinks.CAPABILITY)
        assertTrue(SkillLinks.supported(setOf("handshake", "skills-install-link")))
        // A handshake that lists what it can do, without this: an older server.
        assertFalse(SkillLinks.supported(setOf("handshake", "pairing", "webhooks")))
        // Nothing known yet: offered, and a 404 turns it off.
        assertTrue(SkillLinks.supported(emptySet()))
    }

    @Test fun aServerWithoutItSaysSoAndUploadKeepsWorking() = runTest {
        val t = server()
        val c = controller(t, supported = false)
        c.load()
        c.setLink("https://github.com/o/r")
        c.installFromLink()
        assertEquals(0, sent(t, "POST /api/skills/install-from-link"))
        assertEquals(ErrorWords.NOT_SUPPORTED, c.ui.error)
        assertEquals(7, c.ui.list!!.skills.size)
    }

    @Test fun aMissingRouteTurnsItOff() = runTest {
        // The capability was not known, and the server is older: its router's own 404.
        val t = server()
        val c = controller(t)
        c.load()
        c.installFromLink("https://github.com/o/r")
        assertFalse(c.ui.linkSupported)
        assertEquals(ErrorWords.NOT_SUPPORTED, c.ui.error)
        assertNull(c.ui.pending)
        assertTrue(c.ui.list != null)
    }

    // ─── Installing ───

    @Test fun installsWhatTheLinkNames() = runTest {
        val t = server().on("POST /api/skills/install-from-link", """{"ok":true,"installed":[{"scope":"project:crundi","name":"android-release","origin":"user","source":{"repo":"latent-spaces/brag","ref":"main","path":"skills/brag-slim","commit":"3f2a9c1d0b8e7f6a5c4d3e2f1a0b9c8d7e6f5a4b"}}],"dropped":[],"source":{"repo":"latent-spaces/brag","ref":"main"},"skills":[]}""")
        val c = controller(t)
        c.load()
        c.setTarget("project:crundi")
        c.setLink("npx skills add https://github.com/latent-spaces/brag --skill brag-slim")
        c.installFromLink()
        val b = body(t, "POST /api/skills/install-from-link")
        assertEquals("project:crundi", b.s("scope"))
        assertEquals("npx skills add https://github.com/latent-spaces/brag --skill brag-slim", b.s("url"))
        assertEquals(false, b.b("overwrite"))
        assertEquals(Actions.Note("Installed android-release. New chats will have it.", false), c.ui.note)
        assertEquals(SkillRef("project:crundi", "android-release"), c.ui.open)
        assertEquals("", c.ui.link)
        assertNull(c.ui.busy)
    }

    @Test fun notGitHubIsSaidWithoutAsking() = runTest {
        val t = server()
        val c = controller(t)
        c.load()
        c.installFromLink("https://gitlab.com/o/r")
        assertEquals(0, sent(t, "POST /api/skills/install-from-link"))
        assertEquals(SkillLinks.NOT_GITHUB, c.ui.error)
    }

    @Test fun severalSkillsAskWhichAndInstallTheOnePicked() = runTest {
        val t = server().on("POST /api/skills/install-from-link") { r ->
            val b = r.json as JsonObject
            when {
                (b["skill"] as? JsonPrimitive)?.content == "team-style" -> ApiResponse(200, """{"ok":true,"installed":[{"scope":"global","name":"team-style"}],"dropped":[]}""")
                (b["all"] as? JsonPrimitive)?.booleanOrNull == true -> ApiResponse(200, """{"ok":true,"installed":[{"scope":"global","name":"brag"},{"scope":"global","name":"brag-slim"}],"dropped":[]}""")
                else -> ApiResponse(400, """{"ok":false,"choose":true,"error":"latent-spaces/brag holds 2 skills. Choose one, or install all of them.","skills":[{"name":"team-style","description":"House style","path":"skills/team-style","files":2,"size":300},{"name":"brag-slim","description":"Smaller","path":"skills/brag-slim","files":1,"size":90}]}""")
            }
        }
        val c = controller(t)
        c.load()
        c.installFromLink("https://github.com/latent-spaces/brag")
        val p = c.ui.pending!!
        assertEquals(listOf("team-style", "brag-slim"), p.choices.map { it.name })
        assertEquals("https://github.com/latent-spaces/brag", p.link)
        assertEquals("latent-spaces/brag holds 2 skills. Choose one, or install all of them.", p.message)
        c.pickFromLink("team-style")
        assertEquals("team-style", body(t, "POST /api/skills/install-from-link").s("skill"))
        assertNull(c.ui.pending)
        assertEquals(SkillRef("global", "team-style"), c.ui.open)

        c.closeSkill()
        c.installFromLink("https://github.com/latent-spaces/brag")
        c.pickFromLink(null)
        assertEquals(true, body(t, "POST /api/skills/install-from-link").b("all"))
        assertEquals(Actions.Note("Installed 2 skills. New chats will have them.", false), c.ui.note)
        assertNull(c.ui.open)
    }

    @Test fun aClashAsksToReplaceAsForAnUpload() = runTest {
        val t = server().on("POST /api/skills/install-from-link") { r ->
            if ((r.json as JsonObject)["overwrite"].let { (it as? JsonPrimitive)?.booleanOrNull } == true) ApiResponse(200, """{"ok":true,"installed":[{"scope":"global","name":"brag-slim"}],"dropped":[]}""")
            else ApiResponse(409, """{"ok":false,"conflict":true,"existing":["brag-slim"],"error":"A skill called \"brag-slim\" already exists there"}""")
        }
        val c = controller(t)
        c.load()
        c.installFromLink("https://github.com/latent-spaces/brag", skill = "brag-slim")
        val p = c.ui.pending!!
        assertEquals(listOf("brag-slim"), p.existing)
        assertTrue(p.choices.isEmpty())
        c.replacePending()
        val b = body(t, "POST /api/skills/install-from-link")
        assertEquals(true, b.b("overwrite")); assertEquals("brag-slim", b.s("skill"))
        assertNull(c.ui.pending)
    }

    @Test fun theServersOwnErrorIsShown() = runTest {
        val t = server().on("POST /api/skills/install-from-link", """{"ok":false,"error":"There is no public GitHub repository o/r"}""", 400)
        val c = controller(t)
        c.load()
        c.installFromLink("o/r")
        assertEquals("There is no public GitHub repository o/r", c.ui.error)
        assertTrue(c.ui.linkSupported)
    }

    // ─── Where it came from, and Update ───

    @Test fun theSourceIsReadFromTheList() {
        val s = CrundiJson.decodeFromString(Skill.serializer(), """{"scope":"global","name":"brag-slim","origin":"user","source":{"host":"github","repo":"latent-spaces/brag","ref":"main","path":"skills/brag-slim","commit":"3f2a9c1d0b8e7f6a5c4d3e2f1a0b9c8d7e6f5a4b","installedAt":"2026-10-24T10:00:00.000Z","url":"https://github.com/latent-spaces/brag/tree/main/skills/brag-slim"}}""")
        assertEquals("latent-spaces/brag", s.source!!.repo)
        assertTrue(Skills.tags(s).any { it.first == "GitHub" })
        assertEquals("From latent-spaces/brag, main, skills/brag-slim at 3f2a9c1. Update installs it again from there.", SkillLinks.sourceLine(s.source!!))
        // An older server sends no source: nothing changes.
        assertNull(CrundiJson.decodeFromString(Skill.serializer(), """{"scope":"global","name":"x"}""").source)
        assertEquals("From o/r. Update installs it again from there.", SkillLinks.sourceLine(SkillSource("o/r")))
    }

    @Test fun updateSaysUpToDateOrInstallsTheNewCommit() = runTest {
        var fresh = false
        val t = server().on("POST /api/skills/update-from-source") {
            if (!fresh) ApiResponse(200, """{"ok":true,"upToDate":true}""")
            else ApiResponse(200, """{"ok":true,"upToDate":false,"installed":[{"scope":"global","name":"frontend-design"}],"dropped":[],"source":{"repo":"o/r","commit":"abcdef0123456789abcdef0123456789abcdef01"}}""")
        }
        val c = controller(t)
        c.load()
        c.open(SkillRef("global", "frontend-design"))
        c.update()
        val b = body(t, "POST /api/skills/update-from-source")
        assertEquals("global", b.s("scope")); assertEquals("frontend-design", b.s("name"))
        assertEquals(Actions.Note("frontend-design is up to date", false), c.ui.note)
        fresh = true
        c.update()
        assertEquals(Actions.Note("Updated frontend-design to abcdef0. New chats will have it.", false), c.ui.note)
        assertNull(c.ui.busy)
    }

    @Test fun updateOnAnOlderServerSaysSo() = runTest {
        val c = controller(server())
        c.load()
        c.open(SkillRef("global", "frontend-design"))
        c.update()
        assertEquals(Actions.Note(ErrorWords.NOT_SUPPORTED, true), c.ui.note)
        assertFalse(c.ui.linkSupported)
    }
}
