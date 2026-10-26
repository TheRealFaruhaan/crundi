package mv.obitech.crundi.ui.planning

import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.ApiResponse
import mv.obitech.crundi.api.CrundiClient
import mv.obitech.crundi.api.CrundiJson
import mv.obitech.crundi.api.FactoryJson
import mv.obitech.crundi.api.FakeTransport
import mv.obitech.crundi.api.model.Factory
import mv.obitech.crundi.api.model.FactoryDetail
import mv.obitech.crundi.api.model.FactoryIssue
import mv.obitech.crundi.api.model.FactoryValidation
import mv.obitech.crundi.api.model.Position
import mv.obitech.crundi.screens.FakeApi
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The phone's factory editor, held to the web's (tools/factory-flow/src) and
 * to the server's model (src/factory-model.js): every provided template, as
 * the server hands it out, survives being opened, edited and saved without
 * losing a field, and each edit does what the web's does.
 */
class FactoryEditTest {
    private val templates = listOf("pair", "squad", "kickoff", "studio", "platform")
    private fun raw(name: String): JsonObject = FakeApi.json("factory-template-$name")["factory"]!!.jsonObject
    private fun decode(o: JsonObject): Factory = CrundiJson.decodeFromJsonElement(o)

    /** JSON equal as the server reads it: 10 and 10.0 are the same number. */
    private fun norm(e: JsonElement): JsonElement = when (e) {
        is JsonObject -> JsonObject(e.mapValues { norm(it.value) }.toSortedMap())
        is JsonArray -> JsonArray(e.map { norm(it) })
        is JsonNull -> e
        is JsonPrimitive -> if (!e.isString && e.booleanOrNull == null && e.doubleOrNull != null) JsonPrimitive(e.doubleOrNull) else e
    }
    private fun noNulls(e: JsonElement): JsonElement = when (e) {
        is JsonObject -> JsonObject(e.filterValues { it !is JsonNull }.mapValues { noNulls(it.value) })
        is JsonArray -> JsonArray(e.map { noNulls(it) })
        else -> e
    }
    private fun assertSameJson(expected: JsonElement, actual: JsonElement) = assertEquals(norm(expected).toString(), norm(actual).toString())

    // ─── Saving loses nothing ───

    @Test fun everyTemplateRoundTripsUnchanged() {
        for (t in templates) {
            val r = raw(t)
            val f = decode(r)
            assertTrue("$t has a team", f.roster.size >= 3)
            assertSameJson(r, FactoryJson.forSave(f, r))
            // And with no server copy at all, the typed model alone carries every field the server keeps
            // (a null it leaves out, such as answeredBy, the server reads as null).
            assertSameJson(noNulls(r), noNulls(FactoryJson.forSave(f, null)))
        }
    }

    @Test fun fieldsTheAppDoesNotKnowSurviveAnEdit() {
        // A newer server's fields at every level.
        val base = raw("squad")
        fun JsonObject.with(k: String, v: JsonElement) = JsonObject(this + (k to v))
        val future = JsonPrimitive("kept")
        val r = base.with("future", future)
            .with("settings", base["settings"]!!.jsonObject.with("future", future).with("limits", base["settings"]!!.jsonObject["limits"]!!.jsonObject.with("future", future)))
            .with("roster", JsonArray(base["roster"]!!.jsonArray.map { it.jsonObject.with("future", future).with("tools", it.jsonObject["tools"]!!.jsonObject.with("future", future)) }))
            .with("nodes", JsonArray(base["nodes"]!!.jsonArray.map { n ->
                val o = n.jsonObject.with("future", future)
                if (o["type"]!!.jsonPrimitive.content == "decision") o.with("decision", o["decision"]!!.jsonObject.with("future", future)
                    .with("options", JsonArray(o["decision"]!!.jsonObject["options"]!!.jsonArray.map { it.jsonObject.with("future", future) }))) else o
            }))
            .with("edges", JsonArray(base["edges"]!!.jsonArray.map { it.jsonObject.with("future", future) }))
        val f = decode(r)
        // Edit: rename the planner, clear who answers the review, delete the database step, add a step.
        var g = FactoryEdit.updateMember(f, "plan") { it.copy(name = "Lead planner") }
        g = FactoryEdit.updateNode(g, "ok") { it.copy(decision = it.decision!!.copy(answeredBy = null)) }
        g = FactoryEdit.deleteNodes(g, listOf("db"))
        g = FactoryEdit.addNode(g, "ask", Position(0.0, 0.0)).first
        val out = FactoryJson.forSave(g, r)

        assertEquals(future, out["future"])
        assertEquals(future, out["settings"]!!.jsonObject["future"])
        assertEquals(future, out["settings"]!!.jsonObject["limits"]!!.jsonObject["future"])
        val roster = out["roster"]!!.jsonArray.map { it.jsonObject }
        assertEquals("Lead planner", roster.first { it["id"]!!.jsonPrimitive.content == "plan" }["name"]!!.jsonPrimitive.content)
        assertTrue(roster.all { it["future"] == future && it["tools"]!!.jsonObject["future"] == future })
        val nodes = out["nodes"]!!.jsonArray.map { it.jsonObject }
        assertNull(nodes.firstOrNull { it["id"]!!.jsonPrimitive.content == "db" })
        val ok = nodes.first { it["id"]!!.jsonPrimitive.content == "ok" }
        assertEquals(JsonNull, ok["decision"]!!.jsonObject["answeredBy"])
        assertEquals(future, ok["decision"]!!.jsonObject["future"])
        val parts = nodes.first { it["id"]!!.jsonPrimitive.content == "parts" }
        assertTrue(parts["decision"]!!.jsonObject["options"]!!.jsonArray.all { it.jsonObject["future"] == future })
        // The new step has only what it is: no nulls for the parts of other kinds of step.
        val added = nodes.last()
        assertEquals("ask", added["type"]!!.jsonPrimitive.content)
        assertNull(added["decision"]); assertNull(added["future"])
        assertTrue(out["edges"]!!.jsonArray.all { it.jsonObject["future"] == future })
        assertFalse(out["edges"]!!.jsonArray.any { e -> e.jsonObject["target"]!!.jsonPrimitive.content == "db" })
        // Everything else exactly as it came.
        val expected = r.with("roster", JsonArray(r["roster"]!!.jsonArray.map { if (it.jsonObject["id"]!!.jsonPrimitive.content == "plan") it.jsonObject.with("name", JsonPrimitive("Lead planner")) else it }))
        assertSameJson(expected["roster"]!!, out["roster"]!!)
        assertSameJson(expected["settings"]!!, out["settings"]!!)
    }

    @Test fun saveSendsTheDefinitionOverTheServersCopyAndTheBaseVersion() = runTest {
        val t = FakeTransport()
        val answer = FakeApi.text("factory-template-pair.json")
        t.on("GET /api/factories/get", answer)
        t.on("POST /api/factories") { req ->
            val body = req.json!!.jsonObject
            if (body["baseVersion"]!!.jsonPrimitive.content == "1") ApiResponse(200, """{"ok":false,"conflict":true,"error":"This factory was changed somewhere else.","version":4}""")
            else ApiResponse(200, answer)
        }
        val client = CrundiClient(t)
        val d = client.factories.get("fac_pair")
        assertNotNull(d.raw)
        val edited = FactoryEdit.updateMember(d.factory, "dev") { it.copy(model = "sonnet") }
        client.factories.save(edited, d.raw, d.factory.version)
        val sent = t.requests.last().json!!.jsonObject
        assertEquals("save", sent["action"]!!.jsonPrimitive.content)
        assertEquals("3", sent["baseVersion"]!!.jsonPrimitive.content)
        val def = sent["definition"]!!.jsonObject
        assertEquals("sonnet", def["roster"]!!.jsonArray.first { it.jsonObject["id"]!!.jsonPrimitive.content == "dev" }.jsonObject["model"]!!.jsonPrimitive.content)
        // A newer version elsewhere: the conflict comes back as the web reads it.
        val e = runCatching { client.factories.save(edited, d.raw, 1) }.exceptionOrNull() as ApiException
        assertEquals(true, e.body?.get("conflict")?.jsonPrimitive?.booleanOrNull)
    }

    // ─── The web editor's moves ───

    @Test fun tidyIsTheServersLayout() {
        // The fixtures were laid out by the server's layoutFactory: tidying them changes nothing.
        for (t in templates) {
            val f = decode(raw(t))
            val shuffled = f.copy(nodes = f.nodes.map { it.copy(position = Position(7.0, 7.0)) })
            assertEquals(t, f.nodes.map { it.id to it.position }, FactoryEdit.layout(shuffled).nodes.map { it.id to it.position })
        }
    }

    @Test fun connectingAsTheWeb() {
        val f = decode(raw("pair"))
        // A flow output leads to one place: connecting it again replaces the old connection.
        val (g, loop) = FactoryEdit.connect(f, "rev", "out", "dev")!!
        assertEquals(1, g.edges.count { it.source == "rev" && it.sourceHandle == "out" })
        assertEquals("dev", g.edges.single { it.source == "rev" && it.sourceHandle == "out" }.target)
        // dev leads to rev, so rev back to dev closes a loop: it gets a round limit of 3.
        assertTrue(loop)
        assertEquals(3, g.edges.single { it.source == "rev" && it.sourceHandle == "out" }.maxRounds)
        // Forward, to a step that leads nowhere yet: no limit.
        val (f2, x) = FactoryEdit.addNode(f, "employee:dev", Position())
        val (h, loop2) = FactoryEdit.connect(f2, "mgr", "out", x)!!
        assertFalse(loop2)
        assertEquals(0, h.edges.single { it.source == "mgr" && it.sourceHandle == "out" }.maxRounds)
        // The rules: Finish only to End, nothing else to End, nothing into Start, not to itself, notes between people.
        assertNull(FactoryEdit.connect(f, "mgr", "finish", "dev"))
        assertNull(FactoryEdit.connect(f, "dev", "out", "end"))
        assertNull(FactoryEdit.connect(f, "dev", "out", "start"))
        assertNull(FactoryEdit.connect(f, "dev", "out", "dev"))
        assertNull(FactoryEdit.connect(f, "dev", "nope", "rev"))
        assertNull(FactoryEdit.connect(f, "ok", "note", "dev"))
        val (n, _) = FactoryEdit.connect(f, "dev", "note", "rev")!!
        assertEquals("note", n.edges.last().kind)
        // The same note twice is still one note.
        assertEquals(n.edges.size, FactoryEdit.connect(n, "dev", "note", "rev")!!.first.edges.size)
    }

    @Test fun addingAndDeleting() {
        val f = decode(raw("pair"))
        val (g, id) = FactoryEdit.addNode(f, "decision:many", Position(101.0, 13.0))
        val fork = g.nodes.single { it.id == id }
        assertEquals(Position(96.0, 16.0), fork.position)
        assertEquals(2, fork.decision!!.options.size)
        val join = g.nodes.last()
        assertEquals("join", join.type); assertEquals(id, join.join!!.forkId); assertEquals(Position(96.0, 256.0), join.position)
        assertEquals(listOf("yes", "no"), FactoryEdit.outputsOf(FactoryEdit.addNode(f, "decision:yesno", Position()).let { (x, i) -> x.nodes.single { it.id == i } }))
        assertEquals(listOf("done", "timeout"), FactoryEdit.outputsOf(FactoryEdit.newNode("wait", Position())))
        // Deleting the fork unlinks its Join; Start, the Manager and End stay.
        val h = FactoryEdit.deleteNodes(g, listOf(id, "start", "mgr", "end"))
        assertEquals("", h.nodes.single { it.id == join.id }.join!!.forkId)
        assertTrue(h.nodes.any { it.id == "start" } && h.nodes.any { it.id == "mgr" } && h.nodes.any { it.id == "end" })
        // Finish cannot be deleted; others can, with their step's connections.
        val finish = f.edges.single { it.sourceHandle == "finish" }
        assertEquals(f.edges, FactoryEdit.deleteEdge(f, finish.id).edges)
        val d = FactoryEdit.deleteNodes(f, listOf("rev"))
        assertFalse(d.edges.any { it.source == "rev" || it.target == "rev" })
    }

    @Test fun decisionsAndWaits() {
        val f = decode(raw("squad"))
        // Pick any to yes/no: the option routes go, and None.
        val g = FactoryEdit.setDecisionKind(f, "parts", "yesno")
        assertTrue(g.nodes.single { it.id == "parts" }.decision!!.options.isEmpty())
        assertFalse(g.edges.any { it.source == "parts" })
        // Back to pick one: two fresh options.
        assertEquals(2, FactoryEdit.setDecisionKind(g, "parts", "one").nodes.single { it.id == "parts" }.decision!!.options.size)
        // Pick any to pick one keeps the options' routes but not None.
        val h = FactoryEdit.setDecisionKind(f, "parts", "one")
        assertEquals(setOf("ui", "api", "data"), h.edges.filter { it.source == "parts" }.map { it.sourceHandle }.toSet())
        val o = FactoryEdit.removeOption(f, "parts", "ui")
        assertFalse(o.edges.any { it.source == "parts" && it.sourceHandle == "ui" })
        assertEquals("Option 4", FactoryEdit.addOption(f, "parts").nodes.single { it.id == "parts" }.decision!!.options.last().label)
        // A wait from one route to Done / Timed out loses its routes.
        val (w, wid) = FactoryEdit.addNode(f, "wait", Position())
        val wired = FactoryEdit.connect(w, wid, "done", "mgr")!!.first
        assertTrue(FactoryEdit.setWaitKind(wired, wid, "signal").edges.any { it.source == wid })
        assertFalse(FactoryEdit.setWaitKind(wired, wid, "time").edges.any { it.source == wid })
    }

    @Test fun theTeam() {
        val f = decode(raw("pair"))
        val data = FactoryEditorData.bundled
        assertEquals(14, data.personas.size)
        val reviewer = data.personas.single { it.key == "reviewer" }
        // A second Reviewer is "Reviewer 2"; read-only roles cannot edit files; skills not on this machine are left off.
        val m = FactoryEdit.newEmployee(f, reviewer, setOf("crundi"))
        assertEquals("Reviewer 2", m.name)
        assertEquals("fresh", m.memory)
        assertFalse(FactoryEdit.canEditFiles(m))
        val fe = FactoryEdit.newEmployee(f, data.personas.single { it.key == "frontend" }, setOf("crundi"))
        assertEquals(listOf("crundi"), fe.skills)
        assertTrue(FactoryEdit.canEditFiles(FactoryEdit.setCanEditFiles(m, true)))
        assertEquals("New employee", FactoryEdit.newEmployee(f, null).name)
        // Added for a step: the step is theirs.
        val (g, step) = FactoryEdit.addNode(f, "employee:", Position())
        val withM = FactoryEdit.addEmployee(g, m, step)
        assertEquals(m.id, withM.nodes.single { it.id == step }.employeeId)
        // Removing the reviewer takes their step and clears the decision they answer.
        val r = FactoryEdit.removeEmployee(f, "rev")
        assertFalse(r.roster.any { it.id == "rev" })
        assertFalse(r.nodes.any { it.id == "rev" })
        assertNull(r.nodes.single { it.id == "ok" }.decision!!.answeredBy)
        assertEquals(listOf("frontend-design", "crundi"), FactoryEdit.splitList("frontend-design, /crundi ,", stripSlash = true))
    }

    @Test fun skillMentionsAsTheServerFindsThem() {
        assertEquals(listOf("frontend-design", "plugin:skill"), FactoryEdit.findMentions("Use /frontend-design and (/plugin:skill), not /home/crundi or src/a.js or https://x.io/a or /api/x."))
    }

    @Test fun undoRedoCoalescesTyping() {
        val f = decode(raw("pair"))
        val h = EditHistory()
        h.record(f, "name", now = 0)
        h.record(f.copy(name = "a"), "name", now = 500)
        h.record(f.copy(name = "ab"), "other", now = 600)
        assertEquals(f.copy(name = "ab"), h.undo(f.copy(name = "abc")))
        assertEquals(f, h.undo(f.copy(name = "ab")))
        assertNull(h.undo(f))
        assertEquals(f.copy(name = "ab"), h.redo(f))
    }

    @Test fun editorStateConnectsAndJumpsToIssues() {
        val r = raw("pair")
        val st = FactoryEditorState(FactoryDetail(decode(r), FactoryValidation(), r))
        st.startConnect("rev")
        assertEquals("rev" to "out", st.connect)
        st.tapNode("dev")
        assertNull(st.connect)
        assertEquals(3, st.factory.edges.single { it.source == "rev" && it.sourceHandle == "out" }.maxRounds)
        assertNotNull(st.toast)
        assertTrue(st.dirty)
        st.undo()
        assertEquals("ok", st.factory.edges.single { it.source == "rev" && it.sourceHandle == "out" }.target)
        st.pick(FactoryIssue(employeeId = "rev"))
        assertEquals(EditorTab.Team, st.tab); assertEquals("rev", st.rosterSel)
        st.pick(FactoryIssue(nodeIds = listOf("ok", "dev")))
        assertEquals(EditSel.Node("ok"), st.selection); assertEquals(EditorTab.Edit, st.tab); assertEquals("ok", st.focusNode)
        st.focusOn("edge:e3")
        assertEquals(EditSel.Edge("e3"), st.selection)
        assertTrue(st.has("limits"))
    }
}
