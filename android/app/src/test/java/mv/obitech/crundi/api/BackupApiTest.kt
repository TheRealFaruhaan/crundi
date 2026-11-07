package mv.obitech.crundi.api

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/** Settings > Backup and restore against a server in memory (the /api/backup routes), and signing this phone out. */
class BackupApiTest {
    private val statusJson = """{"ok":true,"storage":{"endpoint":"https://acc.r2.cloudflarestorage.com","region":"auto","bucket":"crundi-backups","prefix":"crundi/","accessKeyId":"AKIA1","secretAccessKey":"","hasSecret":true,"pathStyle":true},
        "hasPassphrase":true,"schedule":{"enabled":true,"frequency":"weekly","time":"03:00","weekday":5,"everyHours":12},"keep":14,"configured":true,
        "running":{"phase":"uploading","reason":"manual","done":512,"total":1024,"startedAt":1},"waiting":null,
        "last":{"ok":true,"at":1762400000000,"reason":"scheduled","key":"crundi/a.crundi-backup","size":20480,"files":312,"transcripts":40,"tookMs":9000,"pruned":1},
        "nextAt":1762920000000,"timezone":"Indian/Maldives","restored":null,"staged":false,"restoreAttempt":null,"restartMode":"systemd"}"""

    @Test fun theStatusIsRead() = runBlocking {
        val s = CrundiClient(FakeTransport().on("GET /api/backup", statusJson)).backup.status()!!
        assertTrue(s.configured && s.hasPassphrase && s.storage.hasSecret)
        assertEquals("crundi-backups", s.storage.bucket)
        assertEquals("weekly", s.schedule.frequency)
        assertEquals(5, s.schedule.weekday)
        assertEquals("uploading", s.running!!.phase)
        assertEquals(312, s.last!!.files)
        assertNull(s.waiting)
        assertEquals("systemd", s.restartMode)
    }

    @Test fun anOlderServerHasNoBackups() = runBlocking {
        assertNull(CrundiClient(FakeTransport()).backup.status())
    }

    @Test fun savingSendsWhatTheWebSends() = runBlocking {
        val t = FakeTransport().on("POST /api/backup/settings", """{"ok":true,"config":{}}""")
        CrundiClient(t).backup.save(BackupStorageForm("https://e.example", "", "b", "crundi/", "id", "", false), "", BackupSchedule(true, "hours", "03:00", 0, 6), 10)
        val body = t.requests.last().json.toString()
        assertTrue(body.contains("\"region\":\"auto\"") && body.contains("\"secretAccessKey\":\"\"") && body.contains("\"pathStyle\":false"))
        assertTrue(body.contains("\"frequency\":\"hours\"") && body.contains("\"everyHours\":6") && body.contains("\"keep\":10") && body.contains("\"passphrase\":\"\""))
    }

    @Test fun listRestoreDeleteAndTheirErrors() = runBlocking {
        val t = FakeTransport()
            .on("GET /api/backup/list", """{"ok":true,"items":[{"key":"crundi/crundi-box-20261106-0300-scheduled.crundi-backup","size":2048,"lastModified":"x","host":"box","at":1762398000000,"reason":"scheduled"}]}""")
            .on("POST /api/backup/restore", """{"ok":true,"started":true,"restartMode":"systemd"}""")
            .on("POST /api/backup/delete", """{"ok":false,"error":"Not a backup in this folder"}""", 400)
        val b = CrundiClient(t).backup
        val items = b.list()
        assertEquals(1, items.size)
        assertEquals("scheduled", items[0].reason)
        b.restore(items[0].key, "")
        assertTrue(t.requests.last().json.toString().contains("\"passphrase\":\"\""))
        try { b.delete("x"); fail() } catch (e: ApiException) { assertEquals("Not a backup in this folder", e.message) }
    }

    @Test fun signingOutThisPhone() = runBlocking {
        val t = FakeTransport().on("POST /api/auth/logout", """{"ok":true}""")
        CrundiClient(t).system.signOut()
        assertEquals("/api/auth/logout", t.requests.last().path)
        assertFalse(t.requests.last().method != "POST")
    }
}
