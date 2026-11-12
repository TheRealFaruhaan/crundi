package mv.obitech.crundi.screens

import mv.obitech.crundi.api.BackupItem
import mv.obitech.crundi.api.BackupLast
import mv.obitech.crundi.api.BackupSchedule
import mv.obitech.crundi.api.BackupStatus
import mv.obitech.crundi.api.BackupStorage
import mv.obitech.crundi.api.model.LoginSession
import mv.obitech.crundi.ui.operations.BackupScreen
import mv.obitech.crundi.ui.operations.BackupSection
import mv.obitech.crundi.ui.operations.BackupState
import mv.obitech.crundi.ui.operations.Load
import mv.obitech.crundi.ui.operations.SessionsScreen
import mv.obitech.crundi.ui.operations.SessionsState
import org.junit.Test
import org.robolectric.annotation.Config

/** Settings > Backup and restore (folded; storage open; a restore being confirmed), an older server, and signing this phone out. */
class BackupScreens : ScreenshotTest() {
    private val now = System.currentTimeMillis()
    private val status = BackupStatus(
        storage = BackupStorage("https://3f2a.r2.cloudflarestorage.com", "auto", "crundi-backups", "crundi/", "a1b2c3d4", hasSecret = true),
        hasPassphrase = true, schedule = BackupSchedule(true, "daily", "03:00"), keep = 14, configured = true,
        last = BackupLast(true, now - 9 * 3_600_000L, "scheduled", "crundi/x.crundi-backup", 48_300_000, 2312),
        nextAt = now + 15 * 3_600_000L, timezone = "Indian/Maldives", restartMode = "systemd",
    )
    private val items = listOf(
        BackupItem("crundi/crundi-dev-20261106-0300-scheduled.crundi-backup", 48_300_000, now - 9 * 3_600_000L, "scheduled", "dev"),
        BackupItem("crundi/crundi-dev-20261105-1412-manual.crundi-backup", 47_900_000, now - 30 * 3_600_000L, "manual", "dev"),
    )

    @Test fun folded() = shoot("settings-backup") { BackupScreen(BackupState(Load.Ready(status))) }

    @Config(qualifiers = "w412dp-h1700dp-420dpi")
    @Test fun storageOpen() = shoot("settings-backup-storage") {
        BackupScreen(BackupState(Load.Ready(status), testResult = "Connected. The bucket can be listed." to true), open = setOf(BackupSection.STORAGE))
    }

    @Config(qualifiers = "w412dp-h1500dp-420dpi")
    @Test fun restoreAsked() = shoot("settings-backup-restore") {
        BackupScreen(BackupState(Load.Ready(status), items = items), open = setOf(BackupSection.LIST), confirmRestore = items[0].key)
    }

    @Test fun olderServer() = shoot("settings-backup-older-server") { BackupScreen(BackupState(Load.Ready(null))) }

    @Test fun signOutAsked() = shoot("settings-sessions-signout") {
        SessionsScreen(
            SessionsState(Load.Ready(FakeApi.model<List<LoginSession>>("sessions", "sessions"))),
            askSignOut = true,
        )
    }
}
