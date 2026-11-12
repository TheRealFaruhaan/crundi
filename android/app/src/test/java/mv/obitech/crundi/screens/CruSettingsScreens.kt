package mv.obitech.crundi.screens

import mv.obitech.crundi.api.BrainStat
import mv.obitech.crundi.api.BrainStats
import mv.obitech.crundi.api.DeepgramVoice
import mv.obitech.crundi.api.VoiceModel
import mv.obitech.crundi.api.VoiceSettings
import mv.obitech.crundi.api.VoiceSpend
import mv.obitech.crundi.voice.CruSection
import mv.obitech.crundi.voice.CruSettingsScreen
import mv.obitech.crundi.voice.CruSettingsUi
import mv.obitech.crundi.voice.DeviceVoice
import mv.obitech.crundi.voice.VoiceSettingsEvents
import mv.obitech.crundi.voice.VoiceSettingsScreen
import mv.obitech.crundi.voice.VoiceSettingsUi
import mv.obitech.crundi.voice.WakeUi
import org.junit.Test
import org.robolectric.annotation.Config

/** Cru's settings in the Settings tab (the server's, every device's), and the phone's own Voice settings beside them. */
class CruSettingsScreens : ScreenshotTest() {
    private val server = VoiceSettings(
        model = "claude-haiku-4-5", models = listOf(VoiceModel("claude-haiku-4-5", "Claude Haiku 4.5"), VoiceModel("claude-sonnet-4-5", "Claude Sonnet 4.5")),
        anthropicKeySet = true, deepgramKeySet = true, anthropicAdminKeySet = false, ttsVoice = "aura-2-draco-en",
        monthlyLimitUsd = 50.0, deepgramLimitUsd = 200.0, billingDay = 20, periodStart = "2026-10-20", nextReset = "2026-11-20",
        deepgramBillingDay = 1, deepgramNextReset = "2026-12-01", pauseMs = 2000, speechRate = 1.15, screenCloseSeconds = 60,
        brain = "claude-code", cliIdleMinutes = 30, spend = VoiceSpend(claudeUsd = 3.21, deepgramUsd = 1.04, turns = 142),
    )
    private val ui = CruSettingsUi(
        host = "dev.faruhaan.com", server = server,
        deepgram = listOf(DeepgramVoice("aura-2-draco-en", "Draco", "British", "masculine"), DeepgramVoice("aura-2-pandora-en", "Pandora", "British", "feminine")),
        limitText = "50", dayText = "20", dgDayText = "1", dgLimitText = "200",
        stats = BrainStats(true, "claude-code", listOf(
            BrainStat("api", "claude-haiku-4-5", turns = 118, failed = 1, medianMs = 2340.0, p90Ms = 4100.0, costPerTurnUsd = 0.0061, cacheReadShare = 0.82),
            BrainStat("claude-code", "claude-haiku-4-5", turns = 24, medianMs = 5120.0, p90Ms = 9800.0, costPerTurnUsd = 0.0143, cacheReadShare = 0.91),
        )),
    )

    @Config(qualifiers = "w412dp-h1400dp-420dpi")
    @Test fun brainOpen() = shoot("settings-voice") { CruSettingsScreen(ui, open = setOf(CruSection.BRAIN)) }

    @Config(qualifiers = "w412dp-h1400dp-420dpi")
    @Test fun brainsCompared() = shoot("settings-voice-compared") { CruSettingsScreen(ui, open = setOf(CruSection.STATS, CruSection.KEYS)) }

    /** An older server: no brain, no screen-close time, no Admin key flag. Those rows are left out. */
    @Test fun olderServer() = shoot("settings-voice-older-server") {
        CruSettingsScreen(ui.copy(server = server.copy(brain = "", cliIdleMinutes = 0, screenCloseSeconds = 0, anthropicAdminKeySet = null), stats = null))
    }

    @Config(qualifiers = "w412dp-h2600dp-420dpi")
    @Test fun phonePage() = shoot("voice-settings-phone") {
        VoiceSettingsScreen(
            VoiceSettingsUi(
                host = "dev.faruhaan.com", isAssistant = true, sharedVoice = "",
                device = listOf(DeviceVoice("en-gb-x-gbd-local", "en-GB", needsNetwork = false, installed = true), DeviceVoice("en-gb-x-rjs-network", "en-GB", needsNetwork = true, installed = true)),
                deviceChosen = "en-gb-x-gbd-local", wake = WakeUi(trainedOn = "6 Nov", inApp = true),
            ),
            VoiceSettingsEvents(),
        )
    }
}
