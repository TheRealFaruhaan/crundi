package mv.obitech.crundi.screens

import mv.obitech.crundi.api.model.WhatsNew
import mv.obitech.crundi.api.model.WhatsNewKey
import mv.obitech.crundi.api.model.WhatsNewStep
import mv.obitech.crundi.ui.operations.SettingsHome
import mv.obitech.crundi.ui.operations.SettingsScreen
import mv.obitech.crundi.ui.operations.WhatsNewScreen
import org.junit.Test

/** What's new on a phone: a step, a step with keys, the recap, and Settings' "Show it again". */
class WhatsNewScreens : ScreenshotTest() {
    private val data = WhatsNew(
        version = "1.21.2",
        steps = listOf(
            WhatsNewStep(
                "factory-secrets", "1.21.2", "Factories", "Secrets a factory may use",
                "A factory can list the secrets its team may use, by name only. When you start a run you type each one's PIN once; it is kept in memory for that run and forgotten when it ends.",
                art = "<div class=\"wna a-fs\"></div>", artTitle = "Start run",
            ),
            WhatsNewStep(
                "skills-from-link", "1.21.2", "Skills", "Install skills from a GitHub link",
                "Paste a GitHub link or an npx skills add line in Settings, Skills, and pick which skills to install.",
                keys = listOf(WhatsNewKey(listOf("Install from link"), "paste a link, pick the skills"), WhatsNewKey(listOf("Ctrl", "V"), "paste")),
                artTitle = "Skills",
            ),
            WhatsNewStep("trigger-needs-cards", "1.21.2", "Factories", "Triggers that wait for work", "A trigger can start a run only when a column has a card.", artTitle = "Trigger"),
        ),
    )

    @Test fun step() = shoot("whatsnew-step") { WhatsNewScreen(data, 0) }
    @Test fun stepWithKeys() = shoot("whatsnew-step-keys") { WhatsNewScreen(data, 1) }
    @Test fun recap() = shoot("whatsnew-recap") { WhatsNewScreen(data, 3) }
    @Test fun settingsRow() = shoot("whatsnew-settings") { SettingsScreen(SettingsHome(host = "dev.faruhaan.com", appVersion = "1.21.2"), onWhatsNew = {}) }
}
