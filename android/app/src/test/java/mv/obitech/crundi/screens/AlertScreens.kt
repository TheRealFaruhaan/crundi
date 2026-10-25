package mv.obitech.crundi.screens

import mv.obitech.crundi.AlertScreen
import org.junit.Test

/** The full-screen urgent alert, as it lights up a locked phone: slide right to attend, left to dismiss. */
class AlertScreens : ScreenshotTest() {
    @Test fun alert() = shoot("alert-full-screen") {
        AlertScreen("web-1 is down", "The health check has failed for 5 minutes. The deploy chat is waiting for you.", "Deploy · dev.faruhaan.com")
    }
}
