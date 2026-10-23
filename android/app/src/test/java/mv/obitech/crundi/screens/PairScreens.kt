package mv.obitech.crundi.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.ui.unit.dp
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import mv.obitech.crundi.ui.components.DialogCard
import mv.obitech.crundi.ui.pair.AppSettingsScreen
import mv.obitech.crundi.ui.pair.PairScreen
import mv.obitech.crundi.ui.pair.PairUi
import mv.obitech.crundi.ui.pair.ScanScreen
import mv.obitech.crundi.ui.pair.ScanState
import mv.obitech.crundi.ui.pair.SettingItem
import mv.obitech.crundi.ui.pair.SettingSection
import mv.obitech.crundi.ui.theme.CrundiColors
import org.junit.Test

/** Pairing (pair, scan) and the app's own settings on this phone. */
class PairScreens : ScreenshotTest() {
    @Test fun pair() = shoot("pair-start") { PairScreen(PairUi(), {}, {}, {}) }

    @Test fun pairAgain() = shoot("pair-again") {
        PairScreen(PairUi(againHost = "dev.faruhaan.com", link = "https://dev.faruhaan.com/pair#0123456789abcdef01234567"), {}, {}, {})
    }

    @Test fun pairError() = shoot("pair-error") {
        PairScreen(PairUi(link = "https://example.com/login", status = "That is not a Crundi pairing link. It looks like https://your-server/pair#... and comes from the server's Connect a phone screen."), {}, {}, {})
    }

    @Test fun pairBusy() = shoot("pair-connecting") {
        PairScreen(PairUi(link = "https://dev.faruhaan.com/pair#0123456789abcdef01234567", status = "Connecting to dev.faruhaan.com...", busy = true), {}, {}, {})
    }

    @Test fun pairInsecure() = shoot("pair-not-private") {
        Box(Modifier.fillMaxSize()) {
            PairScreen(PairUi(link = "http://192.168.1.20:8888/pair#ffffffffffffffffffffffff"), {}, {}, {})
            Box(Modifier.fillMaxSize().background(CrundiColors.Scrim), contentAlignment = androidx.compose.ui.Alignment.Center) {
                DialogCard(
                    "This connection is not private",
                    Modifier.padding(16.dp),
                    message = "http://192.168.1.20:8888 does not use https. Anyone on the same network could read what is sent, including your login.\n\nOnly continue if this is your own server on a network you trust.",
                    confirm = "Connect anyway", danger = true,
                )
            }
        }
    }

    private val fakeCamera: @androidx.compose.runtime.Composable () -> Unit = {
        Box(Modifier.fillMaxSize().background(Brush.verticalGradient(listOf(Color(0xFF2A2A33), Color(0xFF15151C), Color(0xFF30302A)))))
    }

    @Test fun scan() = shoot("pair-scan") { ScanScreen(ScanState.Looking, {}, fakeCamera) }
    @Test fun scanFound() = shoot("pair-scan-found") { ScanScreen(ScanState.Found, {}, fakeCamera) }
    @Test fun scanDenied() = shoot("pair-scan-denied") { ScanScreen(ScanState.Denied, {}) { Box(Modifier.fillMaxSize()) } }

    private val sections = listOf(
        SettingSection(
            "Permissions",
            listOf(
                SettingItem.Line("Notifications", "Allowed", true, "Manage"),
                SettingItem.Line("Bubbles", "Any chat can float as a bubble", true, "Manage"),
                SettingItem.Line("Bubbles when you leave", "On. Chats that are working or waiting on you get a bubble when you leave the app.", null, "Turn off"),
                SettingItem.Line("Live card", "While Claude works, a chip in the status bar and a card on the lock screen.", true, "Manage"),
                SettingItem.Line("Camera", "Not allowed. Needed to scan the pairing picture.", false, "Allow"),
                SettingItem.Line("Microphone", "Not allowed. Only needed to record sound.", null, "Allow"),
            ),
        ),
        SettingSection(
            "Servers",
            listOf(
                SettingItem.Line("dev.faruhaan.com (open)", "Signed in as faruhaan\nRegistered for notifications", null, "Remove"),
                SettingItem.Line("192.168.1.20:8888", "Signed in as faruhaan\nNot a private connection (http)", false, "Open"),
                SettingItem.Button("Remove 192.168.1.20:8888"),
                SettingItem.Button("Connect another server", primary = true),
            ),
        ),
        SettingSection(
            "Notification check",
            listOf(
                SettingItem.Line("Show a test notification", "Made on this phone, with no server involved.", null, "Show"),
                SettingItem.Log(listOf("14:02:11 Shown: crundi needs you", "13:58:40 Registered with dev.faruhaan.com", "13:58:39 Asked to register again")),
            ),
        ),
    )

    @Test fun settings() = shoot("pair-app-settings") {
        AppSettingsScreen("Crundi 1.1.0-dev.1 (play), Android 15, Google Pixel 7", sections) {}
    }
}
