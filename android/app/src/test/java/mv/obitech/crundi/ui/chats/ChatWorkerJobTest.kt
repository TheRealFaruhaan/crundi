package mv.obitech.crundi.ui.chats

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import mv.obitech.crundi.api.ChatReducer
import mv.obitech.crundi.api.CrundiJson
import mv.obitech.crundi.api.model.ChatEntry
import mv.obitech.crundi.api.model.ChatState
import mv.obitech.crundi.api.model.workerJobOf
import mv.obitech.crundi.ui.operations.jobChip
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** A worker-job card (a command a chat ran on a PC), as the server sends it and patches it live. */
class ChatWorkerJobTest {
    private val id = "a1b2c3d4e5f60718"
    private fun obj(s: String) = CrundiJson.parseToJsonElement(s).jsonObject

    private val entry = """{"type":"ui-event","id":"$id","event":{"type":"entry","entry":{
        "id":"e7","kind":"worker-job","seq":7,"job":"j1","worker":"Studio PC","workerId":"w1","project":"crundi",
        "command":"npm run build","source":null,"branch":"main","commit":"1a2b3c4d5e6f","cwd":"",
        "state":"running","running":true,"phase":"copying","detail":null,"offline":false,
        "exitCode":null,"durationMs":null,"timedOut":null,"cancelled":null,"artifacts":null,
        "copy":{"files":12,"sent":5,"bytes":20480,"removed":0},"notes":[],"error":"","tail":"step 1\nstep 2"}}}"""

    private val patch = """{"type":"ui-event","id":"$id","event":{"type":"patch","id":"e7","patch":{
        "state":"failed","running":false,"phase":null,"exitCode":2,"durationMs":125000,
        "artifacts":{"path":"/data/workers/j1","bytes":3145728,"count":2,"expired":false},
        "copy":{"files":12,"sent":12,"bytes":40960,"removed":1},"error":"Build failed.","tail":"step 1\nstep 2\nerror: boom"}}}"""

    @Test fun anEntryAndItsPatch() {
        var s = ChatState(id, loaded = true)
        s = ChatReducer.message(s, obj(entry))
        val e = s.messages.single()
        assertEquals("worker-job", e.kind)
        val j = e.workerJob
        assertEquals("j1", j.job)
        assertEquals("Studio PC", j.worker)
        assertEquals("npm run build", j.command)
        assertTrue(j.running)
        assertNull(j.source)
        assertNull(j.exitCode)
        assertEquals("copying" to mv.obitech.crundi.ui.components.BadgeKind.Warn, jobChip(j))
        assertEquals("main 1a2b3c4 · 5 of 12 files sent", workerJobMeta(j))

        s = ChatReducer.message(s, obj(patch))
        val p = s.messages.single().workerJob
        assertFalse(p.running)
        assertEquals("failed", p.state)
        assertEquals(2, p.exitCode)
        assertEquals("failed (exit 2)" to mv.obitech.crundi.ui.components.BadgeKind.Bad, jobChip(p))
        assertEquals("/data/workers/j1", p.artifacts?.path)
        assertEquals("Build failed.", p.error)
        assertTrue(p.tail!!.endsWith("boom"))
        assertEquals("main 1a2b3c4 · 12 of 12 files sent · 2 min 5 s · 2 files back (3.0 MB)", workerJobMeta(p))
    }

    @Test fun missingOrOddFieldsDoNotCrash() {
        val bare = ChatEntry.from(obj("""{"id":"e1","kind":"worker-job"}"""))
        val j = bare.workerJob
        assertEquals("", j.job)
        assertFalse(j.running)
        assertEquals("", workerJobMeta(j))
        val odd = workerJobOf(obj("""{"state":"starting","exitCode":"x","durationMs":12.6,"artifacts":"none","copy":[],"notes":[1,"a"]}"""))
        assertTrue(odd.running)
        assertNull(odd.exitCode)
        assertEquals(12L, odd.durationMs)
        assertNull(odd.artifacts)
        assertEquals(listOf("1", "a"), odd.notes)
        assertEquals(JsonObject(emptyMap()), ChatEntry.from(obj("{}")).input)
    }
}
