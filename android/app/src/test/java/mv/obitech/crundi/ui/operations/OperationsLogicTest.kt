package mv.obitech.crundi.ui.operations

import mv.obitech.crundi.api.model.Container
import mv.obitech.crundi.api.model.DockerLogFrame
import mv.obitech.crundi.api.model.DockerPort
import mv.obitech.crundi.api.model.Forward
import mv.obitech.crundi.api.model.LoginSession
import mv.obitech.crundi.api.model.Service
import mv.obitech.crundi.api.model.SessionDevice
import mv.obitech.crundi.api.model.UsageWindow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The operations screens' own logic: formatting, matching, folding, the desktop's rules. */
class OperationsLogicTest {
    @Test fun bytesLikeTheDesktop() {
        assertEquals("512 B", Fmt.bytes(512))
        assertEquals("1.5 KB", Fmt.bytes(1536))
        assertEquals("58.4 MB", Fmt.bytes(61235200))
        assertEquals("384 MB", Fmt.bytes(402653184))
        assertEquals("—", Fmt.bytes(null))
    }

    @Test fun percentOneDecimalBelowAHundred() {
        assertEquals("0.4%", Fmt.pct(0.4))
        assertEquals("42.3%", Fmt.pct(42.31))
        assertEquals("130%", Fmt.pct(130.2))
        assertEquals("—", Fmt.pct(null))
    }

    @Test fun remainingAndElapsed() {
        assertEquals("2h 14m", remaining(2 * 3_600_000L + 14 * 60_000L))
        assertEquals("3d 4h", remaining(3 * 86_400_000L + 4 * 3_600_000L))
        assertEquals("45m", remaining(45 * 60_000L))
        val w = UsageWindow(40, "2026-10-21T14:00:00.000Z")
        val reset = w.resetsAtMs
        assertEquals(0.5f, elapsed(w, FIVE_HOURS, reset - FIVE_HOURS / 2), 0.001f)
        assertEquals(0f, elapsed(null, FIVE_HOURS, reset), 0f)
    }

    @Test fun forwardNamesFromServiceNames() {
        assertEquals("docs", forwardSlug("crundi:docs"))
        assertEquals("my-api", forwardSlug("My API!"))
        assertEquals("app", forwardSlug("::"))
        assertEquals(32, forwardSlug("a".repeat(50)).length)
    }

    @Test fun servicesFollowTheProjectAndTheirForward() {
        val a = Service("crundi:docs", "docs", "crundi", tunnelPort = 4811)
        val b = Service("ledger:stack", "stack", "ledger")
        val f = Forward("docs", 4811)
        val other = Forward("old", 4900)
        val s = ServicesState(Load.Ready(listOf(a, b)), forwards = listOf(f, other), project = "crundi")
        assertEquals(listOf(a), s.shown)
        assertEquals(f, s.forwardOf(a))
        assertEquals(null, s.forwardOf(b))
        assertEquals(listOf(other), s.otherForwards)
        assertEquals(2, s.copy(allProjects = true).shown.size)
        assertFalse(s.copy(collaborator = true).canEdit(a))
        assertTrue(s.copy(collaborator = true).canEdit(a.copy(mine = true)))
    }

    @Test fun dockerFormatting() {
        assertEquals("5432→5432", DockerFmt.port(DockerPort("0.0.0.0", 5432, 5432)))
        assertEquals("127.0.0.1:8080→80", DockerFmt.port(DockerPort("127.0.0.1", 8080, 80)))
        assertEquals("53→53/udp", DockerFmt.port(DockerPort("::", 53, 53, "udp")))
        assertEquals("80/tcp", DockerFmt.port(DockerPort(private = 80)))
        assertEquals("unhealthy", DockerFmt.stateKey(Container("x", state = "running", running = true, health = "unhealthy")))
        assertEquals("exited", DockerFmt.stateKey(Container("x")))
        assertTrue(DockerFmt.matches("ledger db", "ledger-db-1", "postgres:16"))
        assertFalse(DockerFmt.matches("ledger redis", "ledger-db-1", "postgres:16"))
        assertTrue(DockerFmt.matches("  ", "anything"))
    }

    @Test fun logLinesSplitTimesAndStreams() {
        val lines = logLines(listOf(
            DockerLogFrame("stdout", "2026-10-21T11:02:10.120456789Z ready\r\nplain line\n"),
            DockerLogFrame("stderr", "boom\n"),
        ))
        assertEquals(3, lines.size)
        assertEquals("2026-10-21 11:02:10.120", lines[0].time)
        assertEquals("ready", lines[0].text)
        assertEquals("plain line", lines[1].text)
        assertTrue(lines[2].stderr)
    }

    @Test fun ansiIsColouredOrStripped() {
        val s = "\u001b[32mLOG\u001b[0m: ok \u001b]0;title\u0007done"
        assertEquals("LOG: ok done", Ansi.strip(s))
        val r = Ansi.render(s)
        assertEquals("LOG: ok done", r.text)
        assertTrue(r.spanStyles.isNotEmpty())
    }

    @Test fun terminalOutputFoldsIntoLines() {
        var l = Ansi.fold(emptyList(), "root@box:/# ls\r\n")
        l = Ansi.fold(l, "a  b\r\nroot@box:/# ")
        assertEquals(listOf("root@box:/# ls", "a  b", "root@box:/# "), l)
        l = Ansi.fold(l, "x\b")
        assertEquals("root@box:/# ", l.last())
        l = Ansi.fold(listOf("50%"), "\r100%")
        assertEquals(listOf("100%"), l)
    }

    @Test fun devicesInWords() {
        assertEquals("Pixel 7 (Crundi app)", deviceName(LoginSession("a", device = SessionDevice("d", "Pixel 7", "android"))))
        assertEquals("Chrome on Windows", deviceName(LoginSession("b", userAgent = "Mozilla/5.0 (Windows NT 10.0) AppleWebKit Chrome/130 Safari/537")))
        assertEquals("Firefox on Linux", deviceName(LoginSession("c", userAgent = "Mozilla/5.0 (X11; Linux x86_64; rv:131.0) Gecko Firefox/131.0")))
    }

    @Test fun everyNotifyEventIsKnownOnce() {
        assertEquals(NOTIFY_EVENTS.size, NOTIFY_EVENTS.map { it.key }.toSet().size)
        assertTrue(NOTIFY_EVENTS.all { it.default in setOf("always", "away", "never") })
    }
}
