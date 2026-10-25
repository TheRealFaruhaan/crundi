package mv.obitech.crundi.ui.planning

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import mv.obitech.crundi.api.CrundiJson
import mv.obitech.crundi.api.model.FactoryRun
import mv.obitech.crundi.api.model.RunAsk
import mv.obitech.crundi.api.model.RunToken
import mv.obitech.crundi.screens.PlanningFakes
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The Factory pane's live view: what it says and does, as the web's Viewer in panel mode. */
class FactoryLiveTest {
    private val live = PlanningFakes.runLive

    @Test fun workingNowSaysWhatEachIsDoing() {
        val w = LiveRun.working(live)
        assertEquals(listOf("Android dev: chats", "Android dev: planning", "Android dev: operations", "Android dev: workspace"), w.map { it.name })
        assertEquals("Editing Composer.kt", w[0].what)
        assertEquals("working", w[0].state)
        assertEquals("c3", w[0].chatId)
        assertTrue(w[2].blocked)
        // A job the Manager handed out says so, as the web's strip.
        assertEquals("Job: Move Downloads.kt onto the API client and drop we…", w[3].what)
        // Nobody is at work while it is crashed or paused.
        assertTrue(LiveRun.working(PlanningFakes.runCrashed).isEmpty())
        // An older server sends no activity: Starting, or Deciding at a decision.
        val old = live.copy(employees = live.employees.mapValues { it.value.copy(activity = null) }, tokens = live.tokens.take(1) + RunToken("t9", "n_review", "UI review", "answer", "e_test", "Tester"))
        assertEquals(listOf("Starting", "Deciding"), LiveRun.working(old).map { it.what })
    }

    @Test fun activeStepsShowWhoAndWhatAndTheJobIsDrawnOnItsPerson() {
        val v = RunView(live, live = true)
        val snap = live.snapshot!!
        val chats = snap.nodes.first { it.id == "n_chats" }
        assertEquals(Doing("Android dev: chats", "Editing Composer.kt", "working", "c3"), v.doing(chats))
        val work = snap.nodes.first { it.id == "n_work" }
        assertEquals("Job: Reading Downloads.kt", v.doing(work)?.text)
        assertEquals(NodeState.Active, v.state(work))
        assertEquals(listOf("n_chats", "n_ops", "n_plan", "n_work"), v.activeIds())
        // Not live (the Factories tab's run page): no activity, as before.
        assertNull(RunView(live).doing(chats))
        assertTrue(RunView(live).activeIds().isEmpty())
        assertEquals(NodeState.Visited, RunView(live).state(work))
    }

    @Test fun needsYouSaysOneThingWithItsButton() {
        assertEquals(LiveRun.Needs(LiveRun.Tone.You, "The Manager is talking to you in its chat.", LiveRun.Act.OpenManager, "Open its chat"), LiveRun.needs(live))
        val crashed = LiveRun.needs(PlanningFakes.runCrashed)!!
        assertEquals(LiveRun.Act.Recover, crashed.act)
        assertEquals("It stopped: the server restarted mid-turn. Recover picks it up where it was.", crashed.text)
        assertEquals(LiveRun.Act.OpenResult, LiveRun.needs(live.copy(status = "finishing"))?.act)
        val cap = live.copy(capHit = JsonObject(mapOf("which" to JsonPrimitive("maxMinutes"), "limit" to JsonPrimitive(240))))
        assertEquals("It reached its limit (240 minutes).", LiveRun.needs(cap)?.text)
        assertEquals(LiveRun.Act.CarryOn, LiveRun.needs(cap)?.act)
        val ask = live.copy(asks = listOf(RunAsk("t", "n_review", "Fit, or **keep 100%**?")))
        assertEquals("A question for you: Fit, or keep 100%?", LiveRun.needs(ask)?.text)
        assertEquals(LiveRun.Act.Answer, LiveRun.needs(ask)?.act)
        val usage = live.copy(waitingOn = "usage", paused = JsonObject(mapOf("by" to JsonPrimitive("usage"))))
        assertEquals(LiveRun.Act.Resume, LiveRun.needs(usage)?.act)
        assertEquals("Paused by the Manager: waiting for CI", LiveRun.needs(live.copy(waitingOn = "paused", paused = JsonObject(mapOf("by" to JsonPrimitive("manager"), "reason" to JsonPrimitive("waiting for CI")))))?.text)
        assertNull(LiveRun.needs(live.copy(waitingOn = "")))
    }

    @Test fun aPauseThatCarriesOnAfterTheResetSaysSo() {
        fun p(vararg kv: Pair<String, Any>) = JsonObject(kv.associate { (k, v) -> k to (if (v is Boolean) JsonPrimitive(v) else JsonPrimitive(v.toString())) })
        assertEquals("Paused.", LiveRun.pausedText(p("by" to "person")))
        assertEquals("Paused; it carries on by itself when the usage window resets.", LiveRun.pausedText(p("by" to "person", "resumeOnReset" to true, "mode" to "now")))
        assertEquals("Paused by the Manager.", LiveRun.pausedText(p("by" to "manager")))
        assertEquals("Paused by the Manager: headroom; it carries on by itself when the usage window resets.", LiveRun.pausedText(p("by" to "manager", "reason" to "headroom", "resumeOnReset" to true)))
        assertEquals("Paused: usage reached the limit set for this run; it carries on by itself after the window resets.", LiveRun.pausedText(p("by" to "usage")))
        assertEquals(LiveRun.pausedText(p("by" to "person", "resumeOnReset" to true)), LiveRun.needs(live.copy(waitingOn = "paused", paused = p("by" to "person", "resumeOnReset" to true)))?.text)
    }

    @Test fun pauseChoicesOnlyWhenTheServerHasThem() {
        assertTrue(PauseOptions.supported(setOf("handshake", "factory-pause-options")))
        assertFalse(PauseOptions.supported(setOf("handshake", "webhooks")))
        assertFalse(PauseOptions.supported(emptySet()))
    }

    @Test fun needsCardsOnlyWhenTheServerHasIt() {
        assertTrue(NeedsCards.supported(setOf("handshake", "trigger-needs-cards")))
        assertFalse(NeedsCards.supported(setOf("handshake", "webhooks")))
        val t = CrundiJson.decodeFromString(mv.obitech.crundi.api.model.FactoryTrigger.serializer(), """{"id":"trg_1","kind":"kanban","kanban":{"status":"done"},"needsCards":"todo"}""")
        assertEquals("todo", t.needsCards)
        assertEquals("When a card enters Done, if To Do has a card", triggerWhen(t))
        // An older server: no field, and saving sends none back (it is left as it is).
        val old = CrundiJson.decodeFromString(mv.obitech.crundi.api.model.FactoryTrigger.serializer(), """{"id":"trg_2","kind":"kanban","kanban":{"status":"done"}}""")
        assertNull(old.needsCards)
        assertEquals("When a card enters Done", triggerWhen(old))
        assertFalse(CrundiJson.encodeToString(mv.obitech.crundi.api.model.FactoryTrigger.serializer(), old).contains("needsCards"))
    }

    @Test fun tappingSomeoneOpensTheirTranscript() {
        assertEquals(OpenChat("c3", "Android dev: chats"), LiveRun.chatAt(live, "n_chats"))
        // A finished step whose person still has a live session: theirs too.
        assertEquals(OpenChat("c2", "API engineer"), LiveRun.chatAt(live, "n_api"))
        // A step with nobody behind it: its history instead.
        assertNull(LiveRun.chatAt(live, "n_join"))
    }

    @Test fun barAndProgress() {
        val now = PlanningFakes.now.toEpochMilli()
        assertEquals("2 handoffs · 5 h 0 min · 412 turns · $18.42 · 2 open findings", LiveRun.progress(live, now))
        assertEquals(listOf("pause", "stop"), LiveRun.controls(live))
        assertEquals(listOf("resume", "stop"), LiveRun.controls(live.copy(paused = JsonObject(emptyMap()))))
        assertEquals(listOf("stop"), LiveRun.controls(PlanningFakes.runCrashed))
        assertEquals("12 min", LiveRun.dur(12 * 60_000L))
    }

    @Test fun summaryFieldsParseAndOldServersStillDo() {
        val r = CrundiJson.decodeFromString(FactoryRun.serializer(), """
            {"id":"r","status":"running","tokens":[{"id":"t","nodeId":"","status":"work","employeeId":"e","job":"Fix it"}],
             "employees":{"e":{"chatId":"c","held":true,"activity":{"state":"working","text":"Reading a.kt","at":"2026-10-24T00:00:00Z"}}},
             "waitingForSeat":[{"tokenId":"t2","employeeId":"e2","employee":"Tester"}]}
        """)
        assertEquals("Fix it", r.tokens[0].job)
        assertEquals("Reading a.kt", r.employees["e"]?.activity?.text)
        assertTrue(r.employees["e"]!!.held)
        assertEquals("Tester", r.waitingForSeat[0].employee)
        assertEquals("Held by the Manager", LiveRun.working(r)[0].what)
        val old = CrundiJson.decodeFromString(FactoryRun.serializer(), """{"id":"r","status":"running","employees":{"e":{"chatId":"c"}}}""")
        assertNull(old.employees["e"]?.activity)
        assertTrue(old.waitingForSeat.isEmpty())
    }
}
