package mv.obitech.crundi.api

import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import mv.obitech.crundi.api.model.parseWorkerLabels
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.operations.countdown
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
}
