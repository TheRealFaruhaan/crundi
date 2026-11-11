package mv.obitech.crundi.screens

import mv.obitech.crundi.api.model.HomepoolOverview
import mv.obitech.crundi.api.model.HpKeyInput
import mv.obitech.crundi.api.model.HpNodeSetup
import mv.obitech.crundi.homepool.NodePhase
import mv.obitech.crundi.homepool.NodeSnapshot
import mv.obitech.crundi.homepool.ServerView
import mv.obitech.crundi.ui.components.DialogCard
import mv.obitech.crundi.ui.operations.HomepoolScreen
import mv.obitech.crundi.ui.operations.HomepoolState
import mv.obitech.crundi.ui.operations.HpShown
import mv.obitech.crundi.ui.operations.HpTab
import mv.obitech.crundi.ui.operations.KeyForm
import mv.obitech.crundi.ui.operations.Load
import mv.obitech.crundi.ui.operations.SettingsHome
import mv.obitech.crundi.ui.operations.SettingsScreen
import mv.obitech.crundi.ui.operations.ShareCard
import mv.obitech.crundi.ui.operations.ShareState
import mv.obitech.crundi.ui.operations.homepoolStatusLine
import org.junit.Test
import org.robolectric.annotation.Config

/** Settings > Homepool: this phone sharing, each tab, the add-access form, a node key shown once, an older server, and the front page's line. */
class HomepoolScreens : ScreenshotTest() {
    private val ov: HomepoolOverview = FakeApi.model("homepool")
    private val now = (ov.networks.firstOrNull { it.pool != null }?.pool?.checkedAt ?: System.currentTimeMillis()) + 60_000
    private val sharing = NodeSnapshot.EMPTY.copy(
        phase = NodePhase.CONNECTED, networkLabel = "Wi-Fi", publicIp = "203.0.113.40", connectedSince = now - 2 * 3_600_000L - 14 * 60_000L,
        configured = true, enabled = true, autoStart = true, mobileCapMb = 500, bytesToday = 126_000_000, mobileBytesToday = 4_200_000,
        nodeName = "Pixel 9", networkName = "Home", server = ServerView(conns = 3, slots = 2, bytesToday = 125_800_000, connsToday = 41, keys = listOf("scraper-sim")),
        log = listOf("09:12:03  connected to dev.faruhaan.com", "09:12:04  hello: Pixel 9 in Home", "09:40:51  network changed: Wi-Fi"), serverUrl = "https://dev.faruhaan.com",
    )
    private fun state() = HomepoolState(Load.Ready(ov), projects = listOf("crundi", "insta-scraper"), now = now)

    @Config(qualifiers = "w412dp-h2200dp-420dpi")
    @Test fun sharingAndExits() = shoot("settings-homepool") {
        HomepoolScreen(state(), tab = HpTab.Exits, share = { ShareCard(ShareState(sharing, notificationsOk = true, batteryOk = false, now = now), networks = ov.own) })
    }

    @Test fun notSharingYet() = shoot("settings-homepool-share") {
        HomepoolScreen(state(), tab = HpTab.Requests, share = { ShareCard(ShareState(NodeSnapshot.EMPTY), networks = ov.own) })
    }

    @Test fun refused() = shoot("settings-homepool-refused") {
        HomepoolScreen(state(), tab = HpTab.Activity, share = {
            ShareCard(ShareState(sharing.copy(phase = NodePhase.REFUSED, pauseReason = "node key refused", connectedSince = 0, server = null, log = emptyList()), now = now), networks = ov.own)
        })
    }

    @Config(qualifiers = "w412dp-h2000dp-420dpi")
    @Test fun access() = shoot("settings-homepool-access") { HomepoolScreen(state(), tab = HpTab.Access) }

    @Test fun activity() = shoot("settings-homepool-activity") { HomepoolScreen(state(), tab = HpTab.Activity) }

    @Config(qualifiers = "w412dp-h1200dp-420dpi")
    @Test fun addAccess() = shoot("settings-homepool-add-access") {
        DialogCard("Add access", dismiss = null) { KeyForm(HpKeyInput(kind = "software", name = "insta-scraper", networks = listOf("home"), portBlock = true, allowFrom = listOf("homepool")), ov, listOf("crundi"), false, "Add access", {}, {}) }
    }

    @Test fun nodeKeyShownOnce() = shoot("settings-homepool-node-key") {
        HomepoolScreen(state().copy(shown = HpShown.Node("Mum's PC", HpNodeSetup("n1", "hpn_n1.abc", "crundi-homepool://node?s=https%3A%2F%2Fdev.faruhaan.com&k=hpn_n1.abc", "https://dev.faruhaan.com", "docker run -d --name crundi-homepool-node node:22-alpine"))), tab = HpTab.Exits)
    }

    @Test fun olderServer() = shoot("settings-homepool-unsupported") { HomepoolScreen(HomepoolState(supported = false)) }

    @Test fun frontPageLine() = shoot("settings-home-homepool") {
        SettingsScreen(SettingsHome(host = "dev.faruhaan.com", appVersion = "1.1.3-dev.39", homepool = true, homepoolStatus = homepoolStatusLine(sharing)))
    }
}
