package mv.obitech.crundi.screens

import mv.obitech.crundi.api.model.FileSearch
import mv.obitech.crundi.api.model.Folder
import mv.obitech.crundi.api.model.GitCommit
import mv.obitech.crundi.api.model.GitCommitDetail
import mv.obitech.crundi.api.model.GitCommitFile
import mv.obitech.crundi.api.model.GitStatus
import mv.obitech.crundi.api.model.MediaItem
import mv.obitech.crundi.api.model.Pane
import mv.obitech.crundi.ui.components.DotState
import mv.obitech.crundi.ui.shell.Area
import mv.obitech.crundi.ui.shell.CrundiApp
import mv.obitech.crundi.ui.shell.Host
import mv.obitech.crundi.ui.workspace.files.FileBody
import mv.obitech.crundi.ui.workspace.files.FileClip
import mv.obitech.crundi.ui.workspace.files.FileItem
import mv.obitech.crundi.ui.workspace.files.FileViewScreen
import mv.obitech.crundi.ui.workspace.files.FileViewState
import mv.obitech.crundi.ui.workspace.files.FilesScreen
import mv.obitech.crundi.ui.workspace.files.FilesState
import mv.obitech.crundi.ui.workspace.files.Transfer
import mv.obitech.crundi.ui.workspace.git.CommitScreen
import mv.obitech.crundi.ui.workspace.git.CommitState
import mv.obitech.crundi.ui.workspace.git.DiffParser
import mv.obitech.crundi.ui.workspace.git.DiffScreen
import mv.obitech.crundi.ui.workspace.git.DiffState
import mv.obitech.crundi.ui.workspace.git.GitBusy
import mv.obitech.crundi.ui.workspace.git.GitScreen
import mv.obitech.crundi.ui.workspace.git.GitState
import mv.obitech.crundi.ui.workspace.git.HistoryScreen
import mv.obitech.crundi.ui.workspace.git.HistoryState
import mv.obitech.crundi.ui.workspace.media.MediaScreen
import mv.obitech.crundi.ui.workspace.media.MediaState
import mv.obitech.crundi.ui.workspace.media.MediaViewScreen
import mv.obitech.crundi.ui.workspace.media.MediaViewState
import mv.obitech.crundi.ui.workspace.terminal.NewTerminalDialog
import mv.obitech.crundi.ui.workspace.terminal.TermEmulator
import mv.obitech.crundi.ui.workspace.terminal.TerminalScreen
import mv.obitech.crundi.ui.workspace.terminal.TerminalUi
import mv.obitech.crundi.ui.workspace.terminal.TerminalsScreen
import mv.obitech.crundi.ui.workspace.terminal.TerminalsState
import org.junit.Test
import java.time.Instant

/** Terminals, Files, Git and Media at phone size, in the states that matter. */
class WorkspaceScreens : ScreenshotTest() {
    private val now = Instant.parse("2026-10-21T15:00:00Z")

    // ─── Terminals ───

    private val panes = listOf(
        Pane("t1", "crundi", "Shell", kind = "terminal", status = "running", agentState = "idle", order = 0),
        Pane("t2", "crundi", "Claude: release notes", kind = "terminal", status = "running", agentState = "working", order = 1),
        Pane("t3", "crundi", "npm run dev", kind = "terminal", status = "running", agentState = "needs-input", order = 2),
    )
    private val parked = listOf(Pane("t4", "crundi", "Claude: tidy imports", kind = "parked", paneKind = "terminal", status = "parked", reason = "restart"))

    @Test fun terminalsList() = shoot("terminals-list") { TerminalsScreen(TerminalsState("crundi", terminals = panes, parked = parked)) }
    @Test fun terminalsEmpty() = shoot("terminals-empty") { TerminalsScreen(TerminalsState("crundi")) }
    @Test fun terminalsNoProject() = shoot("terminals-no-project") { TerminalsScreen(TerminalsState(null)) }
    @Test fun terminalsNew() = shoot("terminals-new") { NewTerminalDialog({}, {}) }

    private fun shellSession(): TermEmulator {
        val e = TermEmulator(50, 36)
        val esc = "\u001b"
        e.write(
            "$esc[1;32mcrundi@dev$esc[0m:$esc[1;34m~/projects/crundi$esc[0m$ ls -la\r\n" +
                "total 248\r\n" +
                "drwxr-xr-x 12 crundi crundi  4096 Oct 21 14:02 $esc[1;34m.$esc[0m\r\n" +
                "-rw-r--r--  1 crundi crundi   412 Oct 20 09:11 .gitignore\r\n" +
                "drwxr-xr-x  6 crundi crundi  4096 Oct 21 13:40 $esc[1;34mandroid$esc[0m\r\n" +
                "-rwxr-xr-x  1 crundi crundi  1180 Oct 18 17:22 $esc[1;32mdeploy.sh$esc[0m\r\n" +
                "-rw-r--r--  1 crundi crundi  2210 Oct 21 11:05 package.json\r\n" +
                "drwxr-xr-x  4 crundi crundi  4096 Oct 21 14:00 $esc[1;34msrc$esc[0m\r\n" +
                "$esc[1;32mcrundi@dev$esc[0m:$esc[1;34m~/projects/crundi$esc[0m$ git status -sb\r\n" +
                "## $esc[32mdev$esc[0m...$esc[31morigin/dev$esc[0m [ahead 2]\r\n" +
                " $esc[31mM$esc[0m src/webapp.js\r\n" +
                "$esc[32mA$esc[0m  src/git-log.js\r\n" +
                "$esc[31m??$esc[0m scripts/test-git-log.mjs\r\n" +
                "$esc[1;32mcrundi@dev$esc[0m:$esc[1;34m~/projects/crundi$esc[0m$ node scripts/test-git-log.mjs\r\n" +
                "$esc[32mPASS$esc[0m  a repository with no commits has an empty history\r\n" +
                "$esc[32mPASS$esc[0m  all five commits, nothing more\r\n" +
                "$esc[32mPASS$esc[0m  newest first\r\n" +
                "$esc[33mWARN$esc[0m  $esc[2mgit is 2.43.0$esc[0m\r\n" +
                "$esc[38;5;141m256-colour$esc[0m and $esc[38;2;251;191;36mtruecolour$esc[0m $esc[7m inverse $esc[0m $esc[4munderline$esc[0m\r\n" +
                "\r\nall passed\r\n" +
                "$esc[1;32mcrundi@dev$esc[0m:$esc[1;34m~/projects/crundi$esc[0m$ ",
        )
        return e
    }

    @Test fun terminalShell() {
        val e = shellSession()
        shoot("terminals-terminal") {
            TerminalScreen(TerminalUi("Shell", "crundi · idle", DotState.Running, input = "git log --oneline -5"), e, e.version)
        }
    }

    @Test fun terminalCtrlAndExited() {
        val e = shellSession()
        e.write("exit\r\n[Process exited with code 0]\r\n")
        shoot("terminals-terminal-exited") {
            TerminalScreen(TerminalUi("Shell", "crundi · exited", DotState.Idle, exited = 0, ctrl = true, fontSize = 11), e, e.version)
        }
    }

    // ─── Files ───

    private val folder: Folder get() = FakeApi.model("files-list")
    private val androidFolder = Folder(
        entries = listOf(
            mv.obitech.crundi.api.model.FileEntry("app", "dir", 0, "/home/crundi/projects/crundi/android/app"),
            mv.obitech.crundi.api.model.FileEntry("gradle", "dir", 0, "/home/crundi/projects/crundi/android/gradle"),
            mv.obitech.crundi.api.model.FileEntry("tools", "dir", 0, "/home/crundi/projects/crundi/android/tools"),
            mv.obitech.crundi.api.model.FileEntry(".gitignore", "file", 118, "/home/crundi/projects/crundi/android/.gitignore"),
            mv.obitech.crundi.api.model.FileEntry("build.gradle.kts", "file", 812, "/home/crundi/projects/crundi/android/build.gradle.kts"),
            mv.obitech.crundi.api.model.FileEntry("gradle.properties", "file", 1290, "/home/crundi/projects/crundi/android/gradle.properties"),
            mv.obitech.crundi.api.model.FileEntry("gradlew", "file", 8762, "/home/crundi/projects/crundi/android/gradlew"),
            mv.obitech.crundi.api.model.FileEntry("README.md", "file", 3420, "/home/crundi/projects/crundi/android/README.md"),
            mv.obitech.crundi.api.model.FileEntry("release-key.png", "file", 233901, "/home/crundi/projects/crundi/android/release-key.png"),
            mv.obitech.crundi.api.model.FileEntry("settings.gradle.kts", "file", 402, "/home/crundi/projects/crundi/android/settings.gradle.kts"),
            mv.obitech.crundi.api.model.FileEntry("crundi-1.0.2.apk", "file", 14_820_113, "/home/crundi/projects/crundi/android/crundi-1.0.2.apk"),
        ),
        crumbs = listOf(mv.obitech.crundi.api.model.Crumb("crundi", "/home/crundi/projects/crundi"), mv.obitech.crundi.api.model.Crumb("android", "/home/crundi/projects/crundi/android")),
        path = "/home/crundi/projects/crundi/android", root = "/home/crundi/projects/crundi", parent = "/home/crundi/projects/crundi",
    )

    @Test fun filesRoot() = shoot("files-list") { FilesScreen(FilesState("crundi", folder)) }
    @Test fun filesFolder() = shoot("files-folder") {
        FilesScreen(FilesState("crundi", androidFolder, clip = FileClip(listOf("/home/crundi/projects/crundi/android/gradlew"), copy = false), transfer = Transfer("Uploading screenshot-1021.png", 412_000, 1_204_000)))
    }
    @Test fun filesSearch() = shoot("files-search") { FilesScreen(FilesState("crundi", folder, query = "webapp", search = FakeApi.model<FileSearch>("files-search"))) }
    @Test fun filesMenu() = shoot("files-menu") {
        FilesScreen(FilesState("crundi", androidFolder, clip = FileClip(listOf("/home/crundi/projects/crundi/README.md"), copy = true)), menuFor = FileItem("app", "/home/crundi/projects/crundi/android/app", true))
    }
    @Test fun filesEmpty() = shoot("files-empty") { FilesScreen(FilesState("crundi", Folder(path = "/home/crundi/projects/crundi/tmp", root = "/home/crundi/projects/crundi", parent = "/home/crundi/projects/crundi", crumbs = listOf(mv.obitech.crundi.api.model.Crumb("crundi", "/home/crundi/projects/crundi"), mv.obitech.crundi.api.model.Crumb("tmp", "/home/crundi/projects/crundi/tmp"))))) }
    @Test fun filesError() = shoot("files-error") { FilesScreen(FilesState("crundi", error = "The server cannot be reached. Check the connection and try again.")) }
    @Test fun filesNoProject() = shoot("files-no-project") { FilesScreen(FilesState(null)) }

    private val kotlinSource = """
        package mv.obitech.crundi.ui.workspace

        import java.util.Locale

        /**
         * "0 B", "8.0 KB", "2.3 MB", as the desktop's Files tab.
         * Sizes of 0 show nothing at all.
         */
        fun formatSize(bytes: Long): String = when {
            bytes <= 0 -> ""
            bytes < 1024 -> "${'$'}bytes B"
            bytes < 1024 * 1024 -> String.format(Locale.US, "%.1f KB", bytes / 1024.0)
            else -> String.format(Locale.US, "%.1f MB", bytes / 1048576.0) // big
        }

        data class Transfer(val label: String, val done: Long, val total: Long)

        object Limits {
            const val MAX_READ = 1_048_576
            val names = listOf("files", "git", "media", "terminals")
        }
    """.trimIndent()

    @Test fun fileView() = shoot("files-view") {
        FileViewScreen(FileViewState("/home/crundi/projects/crundi/android/app/src/main/java/mv/obitech/crundi/ui/workspace/Common.kt", "crundi", FileBody.Text(kotlinSource)))
    }
    @Test fun fileEdit() = shoot("files-edit") {
        FileViewScreen(FileViewState("/home/crundi/projects/crundi/package.json", "crundi", FileBody.Text("{}"), editing = true, draft = "{\n  \"name\": \"crundi\",\n  \"version\": \"1.21.0-dev.5\",\n  \"type\": \"module\"\n}\n"))
    }
    @Test fun fileTooBig() = shoot("files-too-big") {
        FileViewScreen(FileViewState("/home/crundi/projects/crundi/android/crundi-1.0.2.apk", "crundi", FileBody.TooBig("Files over 1 MB open as a download. Save it to the phone to read it.")))
    }

    // ─── Git ───

    private val status: GitStatus get() = FakeApi.model("git-info")

    @Test fun gitStatus() = shoot("git-status") { GitScreen(GitState("crundi", status, message = "Native workspace screens")) }
    @Test fun gitBusy() = shoot("git-pushing") { GitScreen(GitState("crundi", status.copy(files = status.files.filter { !it.staged }), busy = GitBusy.Pushing)) }
    @Test fun gitClean() = shoot("git-clean") { GitScreen(GitState("crundi", GitStatus("main", ahead = 0, behind = 3))) }
    @Test fun gitNotRepo() = shoot("git-not-repo") { GitScreen(GitState("ledger", error = "fatal: not a git repository (or any of the parent directories): .git")) }

    private val sampleDiff = """
        diff --git a/android/app/src/main/java/mv/obitech/crundi/AppGraph.kt b/android/app/src/main/java/mv/obitech/crundi/AppGraph.kt
        index 3a1f2c0..9b8e7d1 100644
        --- a/android/app/src/main/java/mv/obitech/crundi/AppGraph.kt
        +++ b/android/app/src/main/java/mv/obitech/crundi/AppGraph.kt
        @@ -1,9 +1,10 @@
         package mv.obitech.crundi

        +import mv.obitech.crundi.api.CrundiClient
         import android.content.Context
         import mv.obitech.crundi.ui.shell.AppServices
        -import mv.obitech.crundi.ui.shell.StaticShellSource
        +import mv.obitech.crundi.api.LiveShellSource

         /** Builds the services for one paired server. */
         object AppGraph {
        @@ -14,7 +15,8 @@ object AppGraph {
             fun create(ctx: Context, server: Server): AppServices {
        -        val shell = StaticShellSource(ShellData(serverHost = server.host))
        -        return object : AppServices { override val shell = shell }
        +        val api = CrundiClient.forServer(ctx, server)
        +        val shell = LiveShellSource(api, ctx, server.id)
        +        return object : AppServices { override val shell = shell; override val api = api }
             }
         }
    """.trimIndent()

    @Test fun gitDiff() = shoot("git-diff") {
        DiffScreen(DiffState("android/app/src/main/java/mv/obitech/crundi/AppGraph.kt", false, loading = false, parsed = DiffParser.parse(sampleDiff).first(), stagedChunks = setOf(0)), onStageChunk = {})
    }

    private val commits = listOf(
        GitCommit("06bcec0aa1", "06bcec0", listOf("3ee0922"), "Faruhaan", "faruhan@outlook.com", "2026-10-21T14:52:00Z", listOf("HEAD -> dev", "origin/dev"), "factory: API client done and wired"),
        GitCommit("3ee0922bb2", "3ee0922", listOf("7c74f58"), "Faruhaan", "faruhan@outlook.com", "2026-10-21T13:40:00Z", emptyList(), "factory: Foundation is in place and builds"),
        GitCommit("7c74f587e9", "7c74f58", listOf("4aa7770"), "Faruhaan", "faruhan@outlook.com", "2026-10-20T18:05:00Z", listOf("tag: android-1.0.2-dev.3"), "Android 1.0.2-dev.3 (version code 15) for Google Play"),
        GitCommit("4aa7770cc3", "4aa7770", listOf("7308e8c", "11aa22b"), "Faruhaan", "faruhan@outlook.com", "2026-10-19T09:30:00Z", listOf("main", "origin/main"), "1.21.1-dev.7: Linux desktop updates install without freezing"),
        GitCommit("7308e8cdd4", "7308e8c", listOf("1a2b3c4"), "Faruhaan", "faruhan@outlook.com", "2026-10-12T16:00:00Z", emptyList(), "Linux desktop: install updates without freezing the app"),
    )

    @Test fun gitHistory() = shoot("git-history") { HistoryScreen(HistoryState(commits, loading = false, more = true, branch = "dev", now = now)) }
    @Test fun gitCommit() = shoot("git-commit") {
        CommitScreen(
            CommitState(
                "06bcec0aa1",
                GitCommitDetail(
                    "06bcec0aa1", "06bcec0", author = "Faruhaan", email = "faruhan@outlook.com", date = "2026-10-21T14:52:00Z",
                    subject = "factory: API client done and wired",
                    body = "CrundiClient with typed suspend APIs for every area.\nLive flows reconnect with backoff.",
                    files = listOf(GitCommitFile("android/app/src/main/java/mv/obitech/crundi/AppGraph.kt", 3, 2), GitCommitFile("android/app/build.gradle.kts", 6, 0), GitCommitFile("android/app/src/main/res/drawable/mark.png", binary = true)),
                    diff = sampleDiff,
                ),
                DiffParser.parse(sampleDiff),
                now = now,
            ),
        )
    }

    // ─── Media ───

    private val media: List<MediaItem>
        get() = FakeApi.model<List<MediaItem>>("media-list", "items") + listOf(
            MediaItem("6b5c4d3e2f1a0b9c", "standup-2026-10-20.m4a", "audio/mp4", ".m4a", 1_920_331, "audio", "crundi", "crundi", null, "none", null, "2026-10-20T08:00:00.000Z", "/m/6b.m4a"),
            MediaItem("7c6d5e4f3a2b1c0d", "pairing-flow.mp4", "video/mp4", ".mp4", 8_302_114, "video", "crundi", "crundi", mv.obitech.crundi.api.model.MediaLink("node", nodeId = "n1"), "alive", "Pairing by QR", "2026-10-19T11:00:00.000Z", "/m/7c.mp4"),
            MediaItem("8d7e6f5a4b3c2d1e", "backup-2026-09.tar.gz", "application/gzip", ".gz", 48_022_114, "other", null, null, mv.obitech.crundi.api.model.MediaLink("task", taskId = "t9"), "deleted", "Old backup task", "2026-09-30T23:00:00.000Z", "/m/8d.gz"),
        )

    @Test fun mediaGrid() = shoot("media-grid") { MediaScreen(MediaState("crundi", media)) }
    @Test fun mediaSearchEmpty() = shoot("media-no-match") { MediaScreen(MediaState("crundi", media, query = "invoice-2025")) }
    @Test fun mediaEmpty() = shoot("media-empty") { MediaScreen(MediaState("crundi", emptyList())) }
    @Test fun mediaOther() = shoot("media-view-other") { MediaViewScreen(MediaViewState(media.last())) }

    // The real routes through the NavHost, over the fake server.
    @Test fun throughTheApp() {
        for (a in listOf(Area.Git, Area.Files, Area.Media)) shoot("${a.key}-app") {
            CrundiApp(services = FakeServices.default(), host = Host.None, openRoute = a.key)
        }
    }
}
