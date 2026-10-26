package mv.obitech.crundi.screens

import mv.obitech.crundi.api.model.KanbanBoard
import mv.obitech.crundi.api.model.KanbanEvent
import mv.obitech.crundi.api.model.KanbanTask
import mv.obitech.crundi.api.model.KanbanTodo
import mv.obitech.crundi.api.model.LinkInfo
import mv.obitech.crundi.api.model.LinkedIdea
import mv.obitech.crundi.api.model.LinkedTask
import mv.obitech.crundi.api.model.MindMap
import mv.obitech.crundi.api.model.MindNode
import mv.obitech.crundi.api.model.NoteBlock
import mv.obitech.crundi.api.model.NotePage
import mv.obitech.crundi.api.model.NoteSummary
import mv.obitech.crundi.api.model.*
import kotlinx.serialization.json.JsonPrimitive
import java.time.Instant

/** Planning test data: fuller than the API fixtures, so the images look like a board in use. */
object PlanningFakes {
    val now: Instant = Instant.now()
    fun ago(minutes: Long): String = now.minusSeconds(minutes * 60).toString()

    private fun todo(id: String, text: String, done: Boolean = false, deleted: Boolean = false) = KanbanTodo(id, text, done, deleted)

    val board = KanbanBoard(
        tasks = listOf(
            KanbanTask("t1", "Fix login redirect", "After a password login the page loops back to /login on Safari.", "in_progress",
                listOf(todo("d1", "Reproduce on the phone", true), todo("d2", "Check the cookie path", true), todo("d3", "Add a test for SameSite"), todo("d9", "Old idea", deleted = true)),
                mindmapNodes = listOf(LinkedIdea("mm1", "Login flow"))),
            KanbanTask("t2", "Native Android app", "Replace the web pages with Compose screens.", "in_progress",
                listOf(todo("d4", "Kanban and notes", true), todo("d5", "Mind map canvas"), todo("d6", "Factories live graph"))),
            KanbanTask("t3", "Rotate the Telegram bot token", "", "todo"),
            KanbanTask("t4", "Usage chart per project", "Weekly bars, one colour per project, from usage-history.", "todo", listOf(todo("d7", "Ask about the colours"))),
            KanbanTask("t5", "Docker stack logs", "", "backlog"),
            KanbanTask("t6", "Release 1.21", "", "done", listOf(todo("d8", "Tag main", true))),
            KanbanTask("t7", "Fix bubble icon on Android 15", "", "done"),
        ),
        deletedTasks = listOf(KanbanTask("t8", "Try Paparazzi", "Went with Roborazzi instead.", "backlog", deleted = true, deletedAt = ago(60 * 26))),
        history = listOf(
            KanbanEvent("h1", ago(12), "task.move", "Moved \"Native Android app\" to In Progress", "t2"),
            KanbanEvent("h2", ago(40), "todo.done", "Checked \"Check the cookie path\" in \"Fix login redirect\"", "t1"),
            KanbanEvent("h3", ago(180), "task.add", "Added \"Usage chart per project\" to To Do", "t4"),
            KanbanEvent("h4", ago(60 * 26), "task.delete", "Deleted \"Try Paparazzi\"", "t8"),
        ),
    )

    val mindmap = MindMap(
        listOf(
            MindNode("mm1", "Login flow", listOf("Cookies are SameSite=Lax", "Safari drops them on redirect"), null, "crundi", LinkedTask("crundi", "t1"), effectiveProject = "crundi",
                linkedTaskInfo = LinkInfo("task", "crundi", "t1", title = "Fix login redirect", status = "in_progress", todosDone = 2, todosTotal = 3)),
            MindNode("mm2", "Session refresh", parentId = "mm1", effectiveProject = "crundi"),
            MindNode("mm3", "Remember me", parentId = "mm1", effectiveProject = "crundi"),
            MindNode("mm4", "Sliding expiry", parentId = "mm2", effectiveProject = "crundi"),
            MindNode("mm5", "Passkeys", listOf("WebAuthn on the desktop app too"), parentId = "mm1", effectiveProject = "crundi"),
            MindNode("mm6", "Android app", emptyList(), null, "crundi", effectiveProject = "crundi"),
            MindNode("mm7", "Kanban drag", parentId = "mm6", effectiveProject = "crundi",
                linkedTask = LinkedTask("crundi", "t2", "d4"), linkedTaskInfo = LinkInfo("todo", "crundi", "t2", "d4", taskTitle = "Native Android app", text = "Kanban and notes", done = true)),
            MindNode("mm8", "Live graph", parentId = "mm6", effectiveProject = "crundi"),
            MindNode("mm9", "Ideas", emptyList(), null, null),
            MindNode("mm10", "Voice notes in chats", parentId = "mm9"),
        ),
    )

    val notes = listOf(
        NoteSummary("n1", "Release checklist", ago(60 * 400), ago(35), preview = "Production is main's top commit. Tag main, wait for the build, check the Play listing", version = 14),
        NoteSummary("n2", "Android app: screens to build", ago(60 * 30), ago(60 * 3), preview = "Kanban, notes, mind map, schedules, factories. Each with a screenshot test.", version = 6),
        NoteSummary("n3", "Server ports", ago(60 * 900), ago(60 * 50), preview = "443 and 80 are Crundi, 8888 is the loopback API. Dev servers 4000 to 9999.", version = 3),
        NoteSummary("n4", "", ago(60 * 2), ago(60 * 2), version = 1),
        NoteSummary("n5", "Old pm2 notes", ago(60 * 2000), ago(60 * 1500), deleted = true, deletedAt = ago(60 * 30), preview = "pm2 startup on the Windows PC"),
    )

    val page = NotePage(
        "n1", "Release checklist",
        listOf(
            NoteBlock("b1", "p", "Production is <strong>main</strong>'s top commit; dev ships as a <code>-dev.N</code> prerelease."),
            NoteBlock("b2", "h2", "Before tagging"),
            NoteBlock("b3", "todo", "Fast-forward <em>dev</em> to main", checked = true),
            NoteBlock("b4", "todo", "Write the What's new tour step", checked = false),
            NoteBlock("b5", "bullet", "Never upload the installer by hand"),
            NoteBlock("b6", "bullet", "GitHub Actions builds both flavours"),
            NoteBlock("b7", "code", "git tag v1.21.0\ngit push origin v1.21.0", lang = "bash"),
            NoteBlock("b8", "quote", "Bug fixes get no tour step."),
            NoteBlock("b9", "divider"),
            NoteBlock("b10", "table", rows = listOf(listOf("Build", "Status"), listOf("play", "ok"), listOf("github", "<s>failed</s> ok")), header = true),
            NoteBlock("b11", "number", "Tag"),
            NoteBlock("b12", "number", "Watch the <a href=\"https://github.com\">Actions run</a>"),
        ),
        version = 14, updatedAt = ago(35),
    )

    // ─── Schedules ───
    /** "When called" (server 1.21.1): a Grafana alert looked into by one ongoing chat, by key; and a signed one. */
    val hookKey = Schedule("a1b2c3d4e5f60718", "Grafana alerts", "crundi", true, "Indian/Maldives",
        ScheduleAction("chat", prompt = "Look into this alert: {{payload.commonLabels.alertname}} on {{payload.commonLabels.instance}}", mode = "normal", model = "sonnet", session = "resume", sessionId = "8441d374-fd1a-4a7e-9c55-0d2a7b1e6f3c"),
        ScheduleWhen("webhook", "09:00"), webhook = HookSettings("Zk3pQ9vX2mT7wL5nR8yB1cD4fG6hJ0kA", "s3cr3tS1gn1ngK3yF0rGr4f4n4", "key", "x-crundi-signature", "", "{{payload.groupKey}}"),
        lastRun = ago(42))
    val hookSigned = hookKey.copy(id = "b2c3d4e5f6071829", name = "Deploy hook",
        action = ScheduleAction("service", serviceKey = "crundi:web", op = "start"),
        webhook = HookSettings("Zk3pQ9vX2mT7wL5nR8yB1cD4fG6hJ0kA", "Qm9yZ2hlcnRzLXNpZ25pbmctc2VjcmV0", "signature", "x-grafana-alerting-signature", "x-grafana-alerting-signature-timestamp", "{{headers.x-request-id}}"),
        lastRun = null)

    val schedules = Schedules(
        listOf(
            Schedule("s1", "Morning review", "crundi", true, "Indian/Maldives", ScheduleAction("chat", prompt = "Review yesterday's commits and list anything risky", mode = "normal", model = "opus"),
                ScheduleWhen("recurring", "08:30", days = listOf(0, 1, 2, 3, 4)), listOf(ScheduleCondition("usage", "5h", "below", 50.0)), lastRun = ago(60 * 6)),
            Schedule("s2", "Nightly build", "crundi", true, "Indian/Maldives", ScheduleAction("command", command = "npm run build && npm test"),
                ScheduleWhen("recurring", "02:00", days = (0..6).toList()), lastRun = ago(60 * 12)),
            Schedule("s3", "Restart docs", "crundi", false, "Indian/Maldives", ScheduleAction("service", serviceKey = "crundi:docs", op = "toggle"), ScheduleWhen("once", "23:00", "2026-10-25")),
            Schedule("s4", "Weekly tidy", "dhivo-socials", true, "Indian/Maldives", ScheduleAction("agent", mode = "skip", model = "sonnet", effort = "high", session = "continue", prompt = "/loop tidy the content calendar"),
                ScheduleWhen("recurring", "18:00", days = listOf(5)), listOf(ScheduleCondition("terminals", op = "below", value = 1.0), ScheduleCondition("kanban", taskId = "t4", status = "todo"))),
            hookKey, hookSigned,
        ),
        listOf(ScheduleServiceChoice("crundi:docs", "docs", "crundi", "running"), ScheduleServiceChoice("crundi:web", "web", "crundi", "stopped")),
    )

    // ─── Factories ───
    private fun n(id: String, type: String, label: String, x: Int, y: Int, emp: String? = null, decision: FactoryDecision? = null, wait: FactoryWait? = null, ask: FactoryAsk? = null) =
        FactoryNode(id, type, label, Position(x.toDouble(), y.toDouble()), emp, decision, ask, wait)

    val factory = Factory(
        "fac_android", "Android team", "A lead lays the foundation, an API engineer writes the client, four developers build their areas in parallel, a tester and a UI reviewer hold it to the desktop.", 7,
        roster = listOf(
            FactoryMember("e_lead", "Lead Android engineer", "Owns the design system and navigation.", "opus", memory = "keep"),
            FactoryMember("e_api", "API engineer", "Writes the one client for the server.", "opus", skills = listOf("crundi")),
            FactoryMember("e_chats", "Android dev: chats", model = "sonnet"),
            FactoryMember("e_work", "Android dev: workspace", model = "sonnet"),
            FactoryMember("e_plan", "Android dev: planning", "Builds kanban, mind map, notes natively.", "opus", skills = listOf("frontend-design")),
            FactoryMember("e_ops", "Android dev: operations", model = "sonnet"),
            FactoryMember("e_test", "Tester", model = "sonnet", memory = "fresh"),
        ),
        nodes = listOf(
            n("n_start", "start", "Start", 25, 0),
            n("n_lead", "employee", "Foundation", 0, 110, "e_lead"),
            n("n_api", "employee", "API client", 0, 260, "e_api"),
            n("n_chats", "employee", "Chats", -360, 420, "e_chats"),
            n("n_work", "employee", "Workspace", -120, 420, "e_work"),
            n("n_plan", "employee", "Planning", 120, 420, "e_plan"),
            n("n_ops", "employee", "Operations", 360, 420, "e_ops"),
            n("n_join", "join", "All areas in", 0, 590),
            n("n_test", "employee", "Test every screen", 0, 700, "e_test"),
            n("n_ci", "wait", "Wait for CI", 300, 700, wait = FactoryWait("command", command = "gh run watch", timeout = 60.0)),
            n("n_review", "decision", "UI review", 0, 850, decision = FactoryDecision("yesno", "Does it match the desktop?")),
            n("n_end", "end", "End", 25, 1010),
        ),
        edges = listOf(
            FactoryEdge("e1", source = "n_start", target = "n_lead"), FactoryEdge("e2", source = "n_lead", target = "n_api"),
            FactoryEdge("e3", source = "n_api", target = "n_chats"), FactoryEdge("e4", source = "n_api", target = "n_work"),
            FactoryEdge("e5", source = "n_api", target = "n_plan"), FactoryEdge("e6", source = "n_api", target = "n_ops"),
            FactoryEdge("e7", source = "n_chats", target = "n_join"), FactoryEdge("e8", source = "n_work", target = "n_join"),
            FactoryEdge("e9", source = "n_plan", target = "n_join"), FactoryEdge("e10", source = "n_ops", target = "n_join"),
            FactoryEdge("e11", source = "n_join", target = "n_test"), FactoryEdge("e12", source = "n_test", target = "n_review"),
            FactoryEdge("e13", source = "n_review", sourceHandle = "yes", target = "n_end"),
            FactoryEdge("e14", source = "n_review", sourceHandle = "no", target = "n_join", maxRounds = 3),
            FactoryEdge("e15", kind = "note", source = "n_api", target = "n_ci"),
        ),
    )

    private fun ts(minutes: Long) = JsonPrimitive(now.minusSeconds(minutes * 60).toEpochMilli())

    val run = FactoryRun(
        "run_mvhwe7gr9b668d", "crundi", "fac_android", "Android team",
        cr = RunCr("Native Android app replacing the web-page app", listOf(
            RunCrSection("goal", "Goal", "Replace the WebView pages with a native Compose app that has everything the desktop has."),
            RunCrSection("done", "Done when", "- No screen uses a WebView\n- Every area has a native screen\n- versionName 1.1.0-dev.1"),
        )),
        status = "running", createdAt = ts(300), updatedAt = ts(4), handoffs = 2,
        branch = "factory/android-team-9b668d", parentBranch = "dev", managerChatId = "b2c3d4e5f6071829",
        tokens = listOf(
            RunToken("tok1", "n_chats", "Chats", "work", "e_chats", "Android dev: chats"),
            RunToken("tok2", "n_plan", "Planning", "work", "e_plan", "Android dev: planning"),
            RunToken("tok3", "n_ops", "Operations", "work", "e_ops", "Android dev: operations", blocked = true),
        ),
        employees = mapOf(
            "e_lead" to RunEmployee("c1", null, name = "Lead Android engineer"), "e_api" to RunEmployee("c2", null, name = "API engineer"),
            "e_chats" to RunEmployee("c3", "tok1", name = "Android dev: chats"), "e_plan" to RunEmployee("c4", "tok2", name = "Android dev: planning"),
            "e_ops" to RunEmployee("c5", "tok3", name = "Android dev: operations"), "e_work" to RunEmployee("c6", null, name = "Android dev: workspace"),
        ),
        usage = RunUsage(18.42, 412, 6, 4 * 3600_000L),
        findings = listOf(
            RunFinding("F1", "Downloads.kt still imports webkit's CookieManager", "Downloads.kt", true, "open", "Tester"),
            RunFinding("F2", "Mind map nodes clip their second line at 0.9x zoom", "MindMap.kt", false, "claimed", "UI reviewer"),
        ),
        managerMode = "normal", teamMode = "ask",
        history = listOf(
            RunHistoryItem(1, "start"),
            RunHistoryItem(2, "handoff", "n_lead", "Lead Android engineer", "Foundation is in place and builds: theme, icons, nav shell and the screenshot harness."),
            RunHistoryItem(3, "handoff", "n_api", "API engineer", "API client done and wired; all four required Gradle tasks pass."),
            RunHistoryItem(4, "handoff", "n_work", "Android dev: workspace", "Git, files and media are native; Downloads moved to the API client."),
        ),
        snapshot = factory,
    )

    val runAsk = run.copy(
        status = "running", waitingOn = "ask",
        asks = listOf(RunAsk("tok9", "n_review", "The UI reviewer wants the mind map to fit the screen on open. **Fit, or keep 100%?**", listOf(Option("fit", "Fit the screen"), Option("keep", "Keep 100%")))),
        tokens = listOf(RunToken("tok9", "n_review", "UI review", "ask"), RunToken("tok8", "n_ci", "Wait for CI", "wait", wait = RunWait("command", "CI", ts(10), ts(-50), ts(-50), 3, "in_progress"))),
    )

    val runFinished = run.copy(
        status = "finished", tokens = emptyList(), waitingOn = "",
        outcome = RunOutcome("All five areas are native. **52 screens** have screenshot tests; the four Gradle tasks pass.", "", ""),
        history = run.history + RunHistoryItem(5, "finish", "n_end", "Manager", "Ready for you."),
    )

    /**
     * The live graph's run (the workbench pane): four people at work with what
     * each is doing from their chat, one on a job the Manager handed out, one
     * waiting on a permission, one waiting for a seat; and the Manager talking
     * to the person in its chat.
     */
    val runLive = run.copy(
        waitingOn = "manager",
        tokens = listOf(
            RunToken("tok1", "n_chats", "Chats", "work", "e_chats", "Android dev: chats"),
            RunToken("tok2", "n_plan", "Planning", "work", "e_plan", "Android dev: planning"),
            RunToken("tok3", "n_ops", "Operations", "work", "e_ops", "Android dev: operations", blocked = true),
            RunToken("tok5", "", "Job from the Manager", "work", "e_work", "Android dev: workspace", job = "Move Downloads.kt onto the API client and drop webkit's CookieManager"),
        ),
        employees = run.employees + mapOf(
            "e_chats" to RunEmployee("c3", "tok1", name = "Android dev: chats", activity = RunActivity("working", "Editing Composer.kt")),
            "e_plan" to RunEmployee("c4", "tok2", name = "Android dev: planning", activity = RunActivity("working", "Running ./gradlew testPlayDebugUnitTest")),
            "e_ops" to RunEmployee("c5", "tok3", name = "Android dev: operations", activity = RunActivity("waiting", "Running adb install -r app-play-debug.apk")),
            "e_work" to RunEmployee("c6", "tok5", name = "Android dev: workspace", activity = RunActivity("working", "Reading Downloads.kt")),
        ),
        waitingForSeat = listOf(RunSeat("tok6", "e_test", "Tester")),
    )

    val runCrashed = runLive.copy(status = "crashed", waitingOn = "", crash = kotlinx.serialization.json.JsonObject(mapOf("reason" to JsonPrimitive("the server restarted mid-turn"))))

    val summaries = listOf(
        FactorySummary("fac_android", "Android team", factory.description, 7, updatedAt = ago(60 * 20), nodeCount = 12, employeeCount = 7, runnable = true, warningCount = 1,
            crTemplate = listOf(CrField("goal", "Goal", "What should be true when this is done?", true), CrField("done", "Done when", "Checks a reviewer can tick", true, true))),
        FactorySummary("fac_fix", "Bug fixer", "One engineer reproduces, fixes and tests; a reviewer checks the diff.", 3, updatedAt = ago(60 * 24 * 5), nodeCount = 5, employeeCount = 2, runnable = true),
        FactorySummary("fac_draft", "Release train", "Tags, waits for CI, writes the notes.", 1, updatedAt = ago(90), nodeCount = 4, employeeCount = 1, runnable = false, errorCount = 2),
    )

    val templates = listOf(FactoryTemplate("tpl_review", "Build and review", "A builder and a reviewer, in rounds until it passes.", "Small features with a clear finish line", listOf("Builder", "Reviewer"), 5))

    val runs = listOf(
        run, runAsk.copy(id = "run_b", project = "dhivo-socials", factoryName = "Bug fixer", cr = RunCr("Carousel export drops the last slide")),
        runFinished.copy(id = "run_c", factoryName = "Bug fixer", cr = RunCr("Bubble icon is blank on Android 15"), updatedAt = ts(60 * 26)),
        run.copy(id = "run_d", status = "crashed", factoryName = "Release train", cr = RunCr("Release 1.21"), crash = kotlinx.serialization.json.JsonObject(mapOf("reason" to JsonPrimitive("The worktree's disk is full"))), updatedAt = ts(60 * 50)),
    )

    val triggers = listOf(
        FactoryTrigger("trg_0a1b2c3d4e5f", "Nightly polish", true, "crundi", "fac_android", "schedule", TriggerSchedule("02:00", listOf(1, 2, 3, 4, 5)),
            cr = TriggerCr("Polish pass {{date}}", mapOf("goal" to "Fix the open nits")), mergeMode = "ask", ifBusy = "skip",
            history = listOf(TriggerHistory(ts(60 * 20), "started", "schedule", "run_prev"))),
        FactoryTrigger("trg_1b2c3d4e5f60", "Cards to the team", false, "crundi", "fac_fix", "kanban", kanban = TriggerKanban("todo"), cr = TriggerCr("{{card.title}}")),
        // As a server before 1.21.1 sends it: the key and nothing else.
        FactoryTrigger("trg_2c3d4e5f6071", "From GitHub issues", true, "crundi", "fac_fix", "webhook", webhook = HookSettings("k7Hq2pX9"), cr = TriggerCr("{{body.title}}")),
        // And as 1.21.1 does: signed, with an idempotency key, telling the Manager.
        FactoryTrigger("trg_3d4e5f607182", "Grafana to the team", true, "crundi", "fac_fix", "webhook",
            webhook = HookSettings("k7Hq2pX9rT4u", "Z3JhZmFuYS1zaWduaW5nLXNlY3JldA", "signature", "x-grafana-alerting-signature", "x-grafana-alerting-signature-timestamp", "{{payload.groupKey}}"),
            cr = TriggerCr("Alert: {{payload.commonLabels.alertname}}", mapOf("goal" to "{{payload.message}}")), ifBusy = "manager", seenCount = 3),
    )

    val cleanup = Cleanup(listOf(
        CleanupItem("c1", "worktree", "/home/crundi/crundi-factory-worktrees/crundi/run_old1", "crundi", "run_old1", "Bug fixer", "stopped", bytes = 812_000_000, ageMs = 9 * 86_400_000L, uncommitted = JsonPrimitive(3)),
        CleanupItem("c2", "branch", "", "crundi", "run_old2", "Android team", "kept", "factory/android-team-41aa", ageMs = 14 * 86_400_000L, why = "Kept on its branch: nothing was merged."),
        CleanupItem("c3", "orphan", "/home/crundi/crundi-factory-worktrees/ledger/tmp-7f2", "ledger", bytes = 41_000_000, ageMs = 31 * 86_400_000L),
    ), 14)
}
