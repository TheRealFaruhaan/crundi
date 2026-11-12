package mv.obitech.crundi.screens

import mv.obitech.crundi.voice.Take
import mv.obitech.crundi.voice.TrainEvents
import mv.obitech.crundi.voice.TrainUi
import mv.obitech.crundi.voice.VoiceSettingsEvents
import mv.obitech.crundi.voice.VoiceSettingsScreen
import mv.obitech.crundi.voice.VoiceSettingsUi
import mv.obitech.crundi.voice.WakeTrainScreen
import mv.obitech.crundi.voice.WakeUi
import org.junit.Test
import org.robolectric.annotation.Config

/** "Hey Cru" is trained before it listens: the training screen, and Settings before and after. */
class VoiceTrainScreens : ScreenshotTest() {
    @Test fun start() = shoot("voice-train-start") { WakeTrainScreen(TrainUi(), { 0f }, TrainEvents()) }

    @Test fun recording() = shoot("voice-train-recording") {
        WakeTrainScreen(TrainUi(takes = listOf(Take.Good, Take.Good, Take.Empty, Take.Empty, Take.Empty), current = 2, recording = true), { 0.55f }, TrainEvents())
    }

    @Test fun again() = shoot("voice-train-again") {
        WakeTrainScreen(TrainUi(takes = listOf(Take.Good, Take.Good, Take.Redo, Take.Empty, Take.Empty), current = 2, message = "That was too short. Say the whole of \"Hey Cru\".", messageBad = true), { 0f }, TrainEvents())
    }

    @Test fun done() = shoot("voice-train-done") {
        WakeTrainScreen(TrainUi(takes = List(5) { Take.Good }, current = -1, doneOn = "6 Nov", tryResult = "Heard you. That would wake Cru.", tryGood = true), { 0f }, TrainEvents())
    }

    // A tall phone, so the Hey Cru section (below the fold) is in the picture.
    @Config(qualifiers = "w412dp-h2600dp-420dpi")
    @Test fun settingsUntrained() = shoot("voice-settings-heycru-untrained") {
        VoiceSettingsScreen(VoiceSettingsUi(wake = WakeUi()), VoiceSettingsEvents())
    }

    @Config(qualifiers = "w412dp-h2600dp-420dpi")
    @Test fun settingsTrained() = shoot("voice-settings-heycru-trained") {
        VoiceSettingsScreen(VoiceSettingsUi(wake = WakeUi(trainedOn = "6 Nov", inApp = true)), VoiceSettingsEvents())
    }
}
