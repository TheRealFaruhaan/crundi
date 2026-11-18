package mv.obitech.crundi.api

import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import mv.obitech.crundi.api.model.parseWorkerLabels
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.operations.countdown
import mv.obitech.crundi.ui.operations.jobChip
import mv.obitech.crundi.ui.operations.jobDuration
import mv.obitech.crundi.ui.operations.jobMeta
import mv.obitech.crundi.ui.operations.workerEventWords
import mv.obitech.crundi.ui.operations.workerMachine
import mv.obitech.crundi.ui.operations.workerState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Settings > Workers against a server in memory: GET /api/workers as specs/workers-protocol.md has it (src/test/resources/api/workers.json), and the actions. */
class WorkersApiTest {
    private val listJson = javaClass.classLoader!!.getResource("api/workers.json")!!.readText()

    @Test fun theListIsRead() = runBlocking {
        val ov = CrundiClient(FakeTransport().on("GET /api/workers", listJson)).workers.list()
        assertEquals(2, ov.workers.size)
        val pc = ov.workers[0]
        assertEquals("Studio PC", pc.title)
        assertEquals("tray", pc.kind)
        assertTrue(pc.online)
        assertEquals(1795430000000L, pc.connectedMs)
        val i = pc.info!!
        assertEquals(32, i.cpu!!.threads)
        assertEquals(64L, i.ramGb)
        assertEquals("NVIDIA GeForce RTX 4090", i.gpus[0].name)
        assertEquals(812000L, i.disk!!.freeMb)
        assertNull(i.load)
        assertEquals(true, i.tools["claudeLoggedIn"]?.jsonPrimitive?.content?.toBoolean())
        // Versions only: the claudeLoggedIn flag beside them is not a tool.
        assertEquals(listOf("cargo", "claude", "ffmpeg", "git", "node"), i.toolVersions.map { it.first })
        assertEquals("windows 11 23H2 · x86_64 · 32 threads · 64 GB RAM", workerMachine(pc))
        assertEquals(listOf("render"), pc.customLabels)
        assertFalse("render" in pc.derivedLabels)
        assertTrue("gpu:nvidia" in pc.derivedLabels)
        assertEquals(2, pc.logLines.size)
        assertEquals("warn", pc.logLines[1].level)
        assertEquals("online" to BadgeKind.Good, workerState(pc))

        val box = ov.workers[1]
        assertFalse(box.online)
        assertNull(box.info)
        assertEquals(0L, box.connectedMs)
        assertEquals(java.time.Instant.parse("2026-11-17T09:30:00Z").toEpochMilli(), box.lastSeenMs)
        assertEquals("pc", box.pausedBy)
        assertEquals("paused", workerState(box).first)
        assertEquals("", workerMachine(box))

        assertEquals("K7MPQ2XZ", ov.codes[0].code)
        assertEquals("4:59 left", countdown(ov.codes[0].expiresMs, ov.codes[0].expiresMs - 299_500))
        assertEquals("expired", countdown(ov.codes[0].expiresMs, ov.codes[0].expiresMs + 1))
    }

    @Test fun aSparseOrNewerListStillReads() = runBlocking {
        val ov = CrundiClient(FakeTransport().on("GET /api/workers", """{"workers":[{"id":"w1","future":[1,2],"info":{"tools":{}}}]}""")).workers.list()
        assertEquals("w1", ov.workers[0].title)
        assertTrue(ov.codes.isEmpty())
        assertEquals("offline", workerState(ov.workers[0]).first)
    }

    @Test fun actionsSendWhatTheServerReads() = runBlocking {
        val t = FakeTransport().on("POST /api/workers", """{"ok":true,"code":"ABCD2345","expiresAt":1795436600000,"history":[{"at":1795000000000,"what":"joined","name":"Studio PC"},{"at":1795430000000,"what":"terminal-opened","ref":"w1"}]}""")
        val api = CrundiClient(t).workers
        assertEquals("ABCD2345", api.code().code)
        assertEquals("code", t.requests.last().json!!.jsonObject["action"]!!.jsonPrimitive.content)

        api.set("w1", paused = true, labels = listOf("render", "gpu-box"))
        val set = t.requests.last().json!!.jsonObject
        assertEquals("set", set["action"]!!.jsonPrimitive.content)
        assertEquals("true", set["paused"]!!.jsonPrimitive.content)
        assertFalse("allowChats" in set)
        assertEquals(listOf("render", "gpu-box"), set["labels"]!!.jsonArray.map { it.jsonPrimitive.content })

        val h = api.history("w1")
        assertEquals("history", t.requests.last().json!!.jsonObject["action"]!!.jsonPrimitive.content)
        assertEquals(listOf("terminal-opened", "joined"), h.map { it.what })
        assertEquals("Terminal opened (ref w1)", workerEventWords(h[0]))

        api.rename("w1", "Render PC")
        assertEquals("Render PC", t.requests.last().json!!.jsonObject["name"]!!.jsonPrimitive.content)
    }

    @Test fun labelsAreChecked() {
        assertEquals(listOf("render", "gpu:nvidia", "a.b_c-d"), parseWorkerLabels(" Render, gpu:nvidia,,a.b_c-d ").first)
        assertNull(parseWorkerLabels("bad label!").first)
        assertNull(parseWorkerLabels((1..21).joinToString(",") { "l$it" }).first)
        assertEquals(emptyList<String>(), parseWorkerLabels("").first)
    }

    private val jobsJson = """{"ok":true,"jobs":[
      {"job":"j2","worker":"Studio PC","workerId":"w1","project":"crundi","command":"cargo build --release","source":"git","branch":"main","commit":"1a2b3c4d5e6f","cwd":"crundi",
       "state":"running","running":true,"phase":"running","detail":null,"offline":true,"exitCode":null,"durationMs":null,"timedOut":false,"cancelled":false,
       "createdAt":"2026-11-18T10:00:00.000Z","startedAt":"2026-11-18T10:00:02.000Z","endedAt":null,"logBytes":2048,"log":"/x/j2.log","artifacts":null,"copy":null,"notes":[],"error":"","future":{"a":1}},
      {"job":"j1","worker":"Studio PC","workerId":"w1","project":"site","command":"npm run build","source":"files","branch":null,"commit":null,
       "state":"failed","running":false,"phase":"done","offline":false,"exitCode":2,"durationMs":185000,
       "createdAt":"2026-11-18T09:00:00.000Z","endedAt":"2026-11-18T09:03:05.000Z","logBytes":100,
       "artifacts":{"path":"/a/j1","bytes":8493465,"count":2,"expired":false},"copy":{"files":40,"sent":3,"bytes":1200,"removed":1},
       "notes":["dist/ was empty"],"error":"Exit code 2."},
      {"job":"j0","command":"ls","state":"done"}
    ]}"""

    @Test fun jobsAreRead() = runBlocking {
        val t = FakeTransport().on("POST /api/workers", jobsJson)
        val jobs = CrundiClient(t).workers.jobs("w1")!!
        val sent = t.requests.last().json!!.jsonObject
        assertEquals("jobs", sent["action"]!!.jsonPrimitive.content)
        assertEquals("w1", sent["id"]!!.jsonPrimitive.content)
        assertEquals(listOf("j2", "j1", "j0"), jobs.map { it.job })

        val run = jobs[0]
        assertTrue(run.running)
        assertNull(run.exitCode)
        assertNull(run.artifacts)
        assertEquals(java.time.Instant.parse("2026-11-18T10:00:00Z").toEpochMilli(), run.createdMs)
        assertEquals("PC away" to BadgeKind.Warn, jobChip(run))
        assertEquals("crundi · main 1a2b3c4 · just now", jobMeta(run, run.createdMs + 30_000))

        val failed = jobs[1]
        assertEquals("failed (exit 2)" to BadgeKind.Bad, jobChip(failed))
        assertEquals(2, failed.artifacts!!.count)
        assertEquals(3, failed.copy!!.sent)
        assertEquals(listOf("dist/ was empty"), failed.notes)
        assertEquals("3 min 5 s", jobDuration(185_000))
        assertEquals("site · files · 3 min 5 s · 1h ago · 2 files back (8.1 MB)", jobMeta(failed, failed.createdMs + 3_600_000))

        val bare = jobs[2]
        assertEquals("done" to BadgeKind.Good, jobChip(bare))
        assertEquals("", bare.error)
        assertEquals(0L, bare.createdMs)
    }

    @Test fun aPhaseOneServerHasNoJobs() = runBlocking {
        val api = CrundiClient(FakeTransport().on("POST /api/workers", """{"ok":false,"error":"Unknown action."}""")).workers
        assertNull(api.jobs("w1"))
    }

    @Test fun aJobsLogAndCancel() = runBlocking {
        val t = FakeTransport().on("POST /api/workers", """{"ok":true,"job":{"job":"j2","command":"make","state":"cancelled","running":false,"tail":"line 1\nline 2\n"}}""")
        val api = CrundiClient(t).workers
        val v = api.jobLog("j2")!!
        val sent = t.requests.last().json!!.jsonObject
        assertEquals("job-log", sent["action"]!!.jsonPrimitive.content)
        assertEquals("j2", sent["job"]!!.jsonPrimitive.content)
        assertEquals("65536", sent["tailBytes"]!!.jsonPrimitive.content)
        assertEquals("line 1\nline 2\n", v.tail)
        assertEquals("cancelled", api.cancelJob("j2")!!.state)
        assertEquals("job-cancel", t.requests.last().json!!.jsonObject["action"]!!.jsonPrimitive.content)
    }

    @Test fun theListCarriesJobFlags() = runBlocking {
        val ov = CrundiClient(FakeTransport().on("GET /api/workers", """{"workers":[{"id":"w1","online":true,"canRunJobs":true,"busy":true},{"id":"w2"}]}""")).workers.list()
        assertTrue(ov.workers[0].canRunJobs && ov.workers[0].busy)
        assertFalse(ov.workers[1].canRunJobs || ov.workers[1].busy)
    }
}
